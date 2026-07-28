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

/// Emergency force-close of every open tomb (AD-18): discovers open tombs
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
    preflight::check(luks, fido2, fs)?;

    let mappings = luks.list_open_mappings()?;

    Ok(mappings
        .into_iter()
        .map(|mapper| {
            let result = slam_mapping(&mapper, warn, luks, fs);
            (mapper, result)
        })
        .collect())
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
        Err(DomainError::AdapterFailure(msg)) if msg.contains("not currently mounted") => {}
        Err(err) => return Err(err),
    }

    match fs.umount(mapper) {
        Ok(()) => luks.close(mapper),
        Err(DomainError::AdapterFailure(msg)) if msg.contains("not currently mounted") => {
            luks.close(mapper)
        }
        Err(err) => {
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
                    let _ = fs.signal_process(pid, signal);
                }

                std::thread::sleep(ESCALATION_PAUSE);

                match fs.umount(mapper) {
                    Ok(()) => return luks.close(mapper),
                    Err(DomainError::AdapterFailure(msg))
                        if msg.contains("not currently mounted") =>
                    {
                        return luks.close(mapper);
                    }
                    Err(err) => last_err = err,
                }
            }

            Err(last_err)
        }
    }
}
