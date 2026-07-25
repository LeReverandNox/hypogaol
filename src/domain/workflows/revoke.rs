use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::keyslot_guard;
use crate::domain::preflight;
use crate::ports::fido2_backend::Fido2Backend;
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// `fido2`/`fs` are unused beyond `preflight::check` — kept in the signature
/// only for AD-4's uniform three-port preflight gate, same documented
/// pattern as `unlock.rs`'s unused `fido2` parameter. Revoke never touches a
/// physical key or mounts anything: it only edits the LUKS2 header/token
/// metadata via `luks.list_fido2_keyslots`/`remove_key`.
pub fn run(
    path: &Path,
    key_label: &str,
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    preflight::check(luks, fido2, fs)?;

    let target = luks
        .list_fido2_keyslots(path)?
        .into_iter()
        .find(|info| info.key_label == key_label)
        .ok_or_else(|| DomainError::KeyNotFound(key_label.to_string()))?
        .keyslot;

    keyslot_guard::remove_keyslot_guarded(luks, path, target)
}
