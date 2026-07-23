use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::types::KeyslotRef;
use crate::ports::luks_backend::LuksBackend;

/// Removes `target` only if doing so would not leave the tomb with zero valid
/// keyslots (AD-5). Originated here by Story 1.5 (CAP-8's bootstrap cleanup);
/// Story 2.2 (CAP-3/revoke) reuses this same primitive rather than
/// reimplementing the guard.
pub fn remove_keyslot_guarded(
    luks: &dyn LuksBackend,
    path: &Path,
    target: KeyslotRef,
) -> Result<(), DomainError> {
    let live_keyslots = luks.list_fido2_keyslots(path)?;

    if live_keyslots.len() <= 1 {
        return Err(DomainError::LastKeyslotGuard);
    }

    luks.remove_key(path, target)
}
