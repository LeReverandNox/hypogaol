use std::path::{Path, PathBuf};

use crate::domain::errors::DomainError;
use crate::domain::mapping_name;
use crate::domain::preflight;
use crate::ports::fido2_backend::Fido2Backend;
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// `fido2` is unused beyond `preflight::check` — kept in the signature only
/// for AD-4's uniform three-port preflight gate, same as every sibling
/// workflow.
pub fn run(
    path: &Path,
    read_only: bool,
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
    match fs.mount(&mapper, read_only) {
        Ok(mountpoint) => Ok(mountpoint),
        Err(err) => {
            let _ = luks.close(&mapper);
            Err(err)
        }
    }
}
