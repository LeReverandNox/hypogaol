use std::path::{Path, PathBuf};

use crate::domain::errors::DomainError;
use crate::domain::hooks::{self, HookWarning};
use crate::domain::mapping_name;
use crate::domain::preflight;
use crate::domain::types::MapperHandle;
use crate::ports::fido2_backend::Fido2Backend;
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// `fido2` is unused beyond `preflight::check` — kept in the signature only
/// for AD-4's uniform three-port preflight gate, same as every sibling
/// workflow.
#[allow(clippy::too_many_arguments)]
pub fn run(
    path: &Path,
    read_only: bool,
    skip_hooks: bool,
    warn: &dyn Fn(HookWarning),
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<PathBuf, DomainError> {
    preflight::check(luks, fido2, fs)?;

    let name = mapping_name::mapping_name(path)?;
    let mapper = luks.open(path, &name, read_only)?;

    // A successfully opened mapping must not be left dangling if the mount
    // fails (the same close-on-failure discipline Story 1.6's post-review
    // applied to create's own mid-flow failures).
    let mountpoint = match fs.mount(&mapper, read_only) {
        Ok(mountpoint) => mountpoint,
        Err(err) => {
            let err = match luks.close(&mapper) {
                Ok(()) => err,
                Err(close_err) => {
                    err.with_rollback_cleanup_failure("re-lock the LUKS2 mapping", close_err)
                }
            };
            return Err(err);
        }
    };

    // AC #6: `read_only` forces skip unconditionally, checked first so a
    // `skip_hooks: false, read_only: true` caller never runs hooks.
    let run_hooks = !read_only && !skip_hooks;
    if run_hooks {
        run_hooks_step(&mountpoint, &mapper, warn, luks, fs)?;
    }

    Ok(mountpoint)
}

/// Applies bind-hooks (best-effort) then guardrail-checks and runs
/// exec-hooks (hard-gated) against the just-mounted volume (AC #1-#4). On an
/// exec-hooks guardrail rejection, rolls back every bind-hooks destination
/// that was actually applied, then the primary mount and LUKS2 mapping
/// (AC #4) — see Story 4.4's Dev Notes, "Resolved: rollback on a rejected
/// exec-hooks guardrail also un-does already-applied bind-hooks".
fn run_hooks_step(
    mountpoint: &Path,
    mapper: &MapperHandle,
    warn: &dyn Fn(HookWarning),
    luks: &dyn LuksBackend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    // Best-effort — a stray bind mount under `$HOME` is worse than a failed
    // cleanup attempt being ignored. Shared by every failure path below (not
    // just a guardrail rejection): any hook-step error after the primary
    // mount has succeeded must leave nothing dangling (AC #4's framing).
    // Bind-mount and primary-umount failures stay silently best-effort as
    // before; only the final `luks.close` is surfaced, since that's the one
    // whose silent failure previously left the LUKS2 mapping itself
    // indefinitely open with no trace in the error the user actually sees.
    let rollback = |applied: &[PathBuf]| -> Option<DomainError> {
        for dest in applied {
            let _ = fs.unmount_bind_hook_destination(dest);
        }
        let _ = fs.umount(mapper);
        luks.close(mapper).err()
    };
    let with_rollback = |err: DomainError, applied: &[PathBuf]| match rollback(applied) {
        Some(close_err) => {
            err.with_rollback_cleanup_failure("re-lock the LUKS2 mapping", close_err)
        }
        None => err,
    };

    let home = match fs.invoking_home_dir() {
        Ok(home) => home,
        Err(err) => {
            return Err(with_rollback(err, &[]));
        }
    };

    let applied_bind_mounts = apply_bind_hooks(mountpoint, &home, warn, fs);

    let exec_hooks_path = mountpoint.join("exec-hooks");
    if fs.path_exists(&exec_hooks_path) {
        let meta = match fs.hook_file_metadata(&exec_hooks_path) {
            Ok(meta) => meta,
            Err(err) => {
                return Err(with_rollback(err, &applied_bind_mounts));
            }
        };
        if let Some(reason) = hooks::exec_hook_rejection(&meta) {
            let err = DomainError::HookRejected {
                path: exec_hooks_path,
                reason,
            };
            return Err(with_rollback(err, &applied_bind_mounts));
        }

        let status = match fs.run_hook(&exec_hooks_path, &["open", &mountpoint.to_string_lossy()]) {
            Ok(status) => status,
            Err(err) => {
                return Err(with_rollback(err, &applied_bind_mounts));
            }
        };
        if !status.success() {
            warn(HookWarning::ExecHookNonZeroExit {
                path: exec_hooks_path,
                exit_code: status.code(),
            });
        }
    }

    Ok(())
}

/// Bind-mounts every valid `bind-hooks` entry (AC #1), warning and
/// continuing past any entry that fails to resolve or mount (AC #2 — never a
/// hard failure). Returns every destination `bind_mount` actually succeeded
/// on, so a subsequent exec-hooks rejection can roll them back.
fn apply_bind_hooks(
    mountpoint: &Path,
    home: &Path,
    warn: &dyn Fn(HookWarning),
    fs: &dyn FilesystemBackend,
) -> Vec<PathBuf> {
    let mut applied = Vec::new();

    let bind_hooks_path = mountpoint.join("bind-hooks");
    if !fs.path_exists(&bind_hooks_path) {
        return applied;
    }

    let content = match std::fs::read_to_string(&bind_hooks_path) {
        Ok(content) => content,
        Err(_) => {
            warn(HookWarning::BindHooksFileUnreadable {
                path: bind_hooks_path,
            });
            return applied;
        }
    };

    for entry in hooks::parse_bind_hooks(&content) {
        match hooks::resolve_bind_hook_entry(&entry, mountpoint, home, fs) {
            Ok((source, dest)) => match fs.bind_mount(&source, &dest) {
                Ok(()) => applied.push(dest),
                Err(_) => warn(HookWarning::BindHookSkipped {
                    source: entry.source_relative,
                    dest: entry.dest_relative,
                    reason: hooks::BindHookSkipReason::BindMountFailed,
                }),
            },
            Err(reason) => warn(HookWarning::BindHookSkipped {
                source: entry.source_relative,
                dest: entry.dest_relative,
                reason,
            }),
        }
    }

    applied
}
