use crate::domain::errors::DomainError;
use crate::domain::hooks::HookWarning;
use crate::domain::preflight;
use crate::domain::types::{MapperHandle, Signal};
use crate::domain::workflows::close::run_hooks_step;
use crate::domain::workflows::close_all::CloseAllResults;
use crate::ports::fido2_backend::Fido2Backend;
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// Fixed pause between escalation rounds (AD-18) — not user-configurable in
/// v1 (ARCHITECTURE-SPINE.md#Deferred).
const ESCALATION_PAUSE: std::time::Duration = std::time::Duration::from_secs(1);

/// Emergency force-close of every open volume (AD-18): discovers open volumes
/// the same way `close_all` does, and for each busy mount escalates through
/// SIGTERM, SIGHUP, SIGKILL until it clears or no holders remain — with zero
/// confirmation (AC #2, enforced entirely by the CLI layer never prompting;
/// this function itself has no confirmation concept, same as every other
/// workflow). `fido2` is unused beyond `preflight::check`, same AD-4 uniform
/// three-port gate convention `close_all::run` documents for its own unused
/// `fido2` parameter.
///
/// Reuses `close_all::CloseAllResults` — the per-mapping outcome shape is
/// identical to `close_all`'s, and reuses its discovery/batch-collection
/// shape verbatim (one mapping's `Err` never stops the others, AC #4).
///
/// Deliberately takes no `skip_hooks` parameter, unlike every other
/// close/unlock variant — see Story 4.6 Dev Notes: slam's whole framing is a
/// zero-configuration panic button, no flags, not even the confirmation
/// every other mutating command gets.
pub fn run(
    warn: &dyn Fn(HookWarning),
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<CloseAllResults, DomainError> {
    preflight::check(luks, fido2, fs, None)?;

    let mappings = luks.list_open_mappings()?;

    Ok(mappings
        .into_iter()
        .map(|mapper| {
            let result = match fs.lock_mapping(&mapper.name, &mapper.source_path) {
                Ok(_lock) => slam_mapping(&mapper, warn, luks, fs),
                Err(err) => Err(err),
            };
            (mapper, result)
        })
        .collect())
}

/// True when an `AdapterFailure`'s message is the idempotent "already
/// unmounted" marker `close_mapping` also tolerates for a prior partial
/// close.
fn is_not_currently_mounted(err: &DomainError) -> bool {
    matches!(err, DomainError::AdapterFailure(msg) if msg.contains("not currently mounted"))
}

/// True when an `AdapterFailure`'s message indicates the mount target is
/// actually busy (confirmed real `umount` wording: "target is busy.",
/// util-linux 2.42; matched case-insensitively to also cover older
/// "device is busy" phrasing). Any other `umount` failure — permission,
/// I/O, a missing mapping — can't be fixed by signaling holders, so it must
/// not drive the escalation loop below (review finding, 2026-07-28: the
/// original code escalated on *any* non-"not currently mounted" error).
fn is_busy(err: &DomainError) -> bool {
    matches!(err, DomainError::AdapterFailure(msg) if msg.to_lowercase().contains("busy"))
}

/// The per-mapping slam sequence: hooks step once, then `umount`, escalating
/// through signals if busy (AC #1, #3).
fn slam_mapping(
    mapper: &MapperHandle,
    warn: &dyn Fn(HookWarning),
    luks: &dyn LuksBackend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    // Hooks step runs exactly once, before any escalation round (AC #3) —
    // same "not currently mounted" tolerance `close_mapping` already applies
    // for a prior partial close.
    match run_hooks_step(mapper, warn, fs) {
        Ok(()) => {}
        Err(err) if is_not_currently_mounted(&err) => {}
        Err(err) => return Err(err),
    }

    match fs.umount(mapper) {
        Ok(()) => luks.close(mapper),
        Err(err) if is_not_currently_mounted(&err) => luks.close(mapper),
        Err(err) if is_busy(&err) => {
            // Busy — fall through to the escalation loop below, keeping this
            // as the last-seen error in case every signal fails to clear it.
            let mountpoint = fs.mount_point_of(mapper)?;
            let mut last_err = err;

            for signal in [Signal::Sigterm, Signal::Sighup, Signal::Sigkill] {
                let holders = fs.processes_using(&mountpoint)?;
                if holders.is_empty() {
                    // No holders remain but umount still fails — nothing left
                    // to signal; this mapping's failure (AC #1, Dev Notes).
                    break;
                }

                for pid in holders {
                    // Never signal PID 0/1 (init) — a privileged SIGKILL to
                    // PID 1 can crash or reboot the host (review finding,
                    // 2026-07-28). `fuser -m` should never report either for
                    // a volume's mountpoint; skip defensively if it ever does.
                    if pid.0 > 1 {
                        let _ = fs.signal_process(pid, signal);
                    }
                }

                std::thread::sleep(ESCALATION_PAUSE);

                match fs.umount(mapper) {
                    Ok(()) => return luks.close(mapper),
                    Err(err) if is_not_currently_mounted(&err) => return luks.close(mapper),
                    Err(err) => last_err = err,
                }
            }

            Err(last_err)
        }
        // Some other `umount` failure that escalation can't fix — report it
        // immediately instead of signaling unrelated holders.
        Err(err) => Err(err),
    }
}
