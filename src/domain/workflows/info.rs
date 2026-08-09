use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::preflight;
use crate::domain::types::KeyslotInfo;
use crate::ports::fido2_backend::Fido2Backend;
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// `fido2`/`fs` are unused beyond `preflight::check` — kept in the signature
/// only for AD-4's uniform three-port preflight gate, same documented
/// pattern as `revoke.rs`'s unused `fido2`/`fs` parameters. Info never
/// touches a physical key or mounts anything: it only reads the LUKS2
/// header via `luks.list_fido2_keyslots`.
pub fn run(
    path: &Path,
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<Vec<KeyslotInfo>, DomainError> {
    preflight::check(luks, fido2, fs, None)?;

    luks.list_fido2_keyslots(path)
}
