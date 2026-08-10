use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::hooks::{self, HookWarning};
use crate::domain::mapping_name;
use crate::domain::preflight;
use crate::domain::types::MapperHandle;
use crate::ports::fido2_backend::Fido2Backend;
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// `fido2` is unused beyond `preflight::check` — kept in the signature only
/// for AD-4's uniform three-port preflight gate, same as `unlock`/`revoke`.
///
/// Unlike `unlock`, `close` never opens a mapping itself — it acts on one
/// that's already open, so the `MapperHandle` is built directly from the
/// derived name rather than obtained via `luks.open`. There is also no
/// mount-failure rollback to mirror: `unlock`'s rollback closes a mapping
/// *it* just opened; `close` opens nothing, so an `umount` failure simply
/// propagates without calling `luks.close` (AC #1's ordering — never lock a
/// mapping that may still be busy) — except "not currently mounted", which
/// means a prior `close` already unmounted but failed before reaching
/// `luks.close`; treating that as done and proceeding makes a retry
/// self-healing instead of permanently stuck (review finding, 2026-07-26).
///
/// `close` gains only `skip_hooks: bool`, no read-only-awareness (Dev Notes:
/// "Resolved: `close` does not need read-only-awareness").
pub fn run(
    path: &Path,
    skip_hooks: bool,
    warn: &dyn Fn(HookWarning),
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    preflight::check(luks, fido2, fs, None)?;
    let _lock = fs.lock_target(path)?;

    let name = mapping_name::mapping_name(path)?;
    let mapper = MapperHandle {
        name,
        source_path: path.to_path_buf(),
    };

    close_mapping(&mapper, skip_hooks, warn, luks, fs)
}

/// The shared per-mapping close sequence (hooks, then `umount`, then
/// `luks.close`) — extracted so `close_all::run` (Story 4.5) can apply the
/// exact same ordering to each mapping `LuksBackend::list_open_mappings`
/// discovers, without re-deriving a mapping name from a path it doesn't
/// have. `close::run` above is now just path-resolution followed by this.
pub(crate) fn close_mapping(
    mapper: &MapperHandle,
    skip_hooks: bool,
    warn: &dyn Fn(HookWarning),
    luks: &dyn LuksBackend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    if !skip_hooks {
        match run_hooks_step(mapper, warn, fs) {
            Ok(()) => {}
            // Same tolerance as the `umount` match below: a prior `close`
            // that already unmounted (but failed before `luks.close`) must
            // stay retry-self-healing even now that the hooks step also
            // resolves the mountpoint up front (review finding, 2026-07-28).
            Err(DomainError::AdapterFailure(msg)) if msg.contains("not currently mounted") => {}
            Err(err) => return Err(err),
        }
    }

    match fs.umount(mapper) {
        Ok(()) => {}
        Err(DomainError::AdapterFailure(msg)) if msg.contains("not currently mounted") => {}
        Err(err) => return Err(err),
    }

    luks.close(mapper)
}

/// Runs `exec-hooks` (hard-gated) then unmounts every still-mounted
/// bind-hooks destination (AC #5) — before the primary `umount`/`luks.close`
/// tail. `close` never opens anything itself, so an exec-hooks rejection has
/// nothing to roll back: it returns immediately, before touching `umount` or
/// `luks.close` at all (AC #4's rollback is `unlock`'s concern, not this
/// one's).
pub(crate) fn run_hooks_step(
    mapper: &MapperHandle,
    warn: &dyn Fn(HookWarning),
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    let mountpoint = fs.mount_point_of(mapper)?;

    let volume_name = mapper
        .source_path
        .file_stem()
        .unwrap_or(mapper.source_path.as_os_str())
        .to_string_lossy()
        .into_owned();

    let exec_hooks_path = mountpoint.join("exec-hooks");
    if fs.path_exists(&exec_hooks_path) {
        let meta = fs.hook_file_metadata(&exec_hooks_path)?;
        if let Some(reason) = hooks::exec_hook_rejection(&meta) {
            return Err(DomainError::HookRejected {
                path: exec_hooks_path,
                reason,
            });
        }

        let status = fs.run_hook(
            &exec_hooks_path,
            &[
                "close",
                &mountpoint.to_string_lossy(),
                &volume_name,
                &mapper.source_path.to_string_lossy(),
                &mapper.device_node().to_string_lossy(),
            ],
        )?;
        if !status.success() {
            warn(HookWarning::ExecHookNonZeroExit {
                path: exec_hooks_path,
                exit_code: status.code(),
            });
        }
    }

    teardown_bind_hooks(&mountpoint, warn, fs);

    Ok(())
}

/// Re-reads and re-parses `bind-hooks` from the still-live `mountpoint`,
/// unmounting each resolvable entry's destination. Per-entry failures are
/// ignored (an entry already unmounted, or one that fails containment on
/// re-check, is simply skipped) and never warned about — teardown failures
/// here are expected/benign, unlike `unlock`'s own bind-hooks warnings. A
/// whole-file read failure is different: it silently disables every entry at
/// once, so unlike per-entry teardown failures, it does warn.
fn teardown_bind_hooks(mountpoint: &Path, warn: &dyn Fn(HookWarning), fs: &dyn FilesystemBackend) {
    let bind_hooks_path = mountpoint.join("bind-hooks");
    if !fs.path_exists(&bind_hooks_path) {
        return;
    }

    let content = match std::fs::read_to_string(&bind_hooks_path) {
        Ok(content) => content,
        Err(_) => {
            warn(HookWarning::BindHooksFileUnreadable {
                path: bind_hooks_path,
            });
            return;
        }
    };

    // `resolve_bind_hook_entry` needs a home directory to resolve `dest`
    // against — `close` has no other reason to know it, so it's fetched
    // here, scoped to teardown only.
    let Ok(home) = fs.invoking_home_dir() else {
        return;
    };

    for entry in hooks::parse_bind_hooks(&content) {
        if let Ok((_source, dest)) = hooks::resolve_bind_hook_entry(&entry, mountpoint, &home, fs) {
            let _ = fs.unmount_bind_hook_destination(&dest);
        }
    }
}
