use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::types::KeyslotRef;
use crate::ports::luks_backend::LuksBackend;

/// Removes `target` only if doing so would not leave the volume with zero valid
/// keyslots (AD-5). Originated here by Story 1.5 (CAP-8's bootstrap cleanup);
/// `domain::workflows::revoke::run` (CAP-3) reuses this same primitive rather
/// than reimplementing the guard.
pub fn remove_keyslot_guarded(
    luks: &dyn LuksBackend,
    path: &Path,
    target: KeyslotRef,
) -> Result<(), DomainError> {
    let live_keyslots = luks.list_fido2_keyslots(path)?;

    // Removing a keyslot that isn't itself among the counted valid ones
    // (e.g. create's transient bootstrap passphrase slot, which never gets a
    // systemd-fido2 token) can never reduce the valid count — confirmed by a
    // real hardware run, where the bootstrap keyslot is invisible to
    // list_fido2_keyslots from the start. The last-key guard only applies
    // when target is itself one of the valid keyslots being removed (the
    // revoke case): then, removing the sole remaining one would leave zero.
    let target_is_valid = live_keyslots.iter().any(|info| info.keyslot == target);

    if target_is_valid && live_keyslots.len() <= 1 {
        return Err(DomainError::LastKeyslotGuard);
    }

    luks.remove_key(path, target)
}
