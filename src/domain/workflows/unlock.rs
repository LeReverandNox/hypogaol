use std::path::{Path, PathBuf};

use crate::domain::errors::DomainError;
use crate::domain::mapping_name;
use crate::domain::preflight;
use crate::ports::fido2_backend::Fido2Backend;
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// `fido2` is unused beyond `preflight::check` — kept in the signature only
/// for AD-4's uniform three-port preflight gate, same as `close`/`resize`'s
/// existing stub signatures.
pub fn run(
    path: &Path,
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<PathBuf, DomainError> {
    preflight::check(luks, fido2, fs)?;

    let name = mapping_name::mapping_name(path)?;
    let mapper = luks.open(path, &name)?;

    // A successfully opened mapping must not be left dangling if the mount
    // fails (the same close-on-failure discipline Story 1.6's post-review
    // applied to create's own mid-flow failures).
    match fs.mount(&mapper) {
        Ok(mountpoint) => Ok(mountpoint),
        Err(err) => {
            let _ = luks.close(&mapper);
            Err(err)
        }
    }
}
