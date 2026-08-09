use crate::domain::errors::DomainError;
use crate::domain::types::Filesystem;
use crate::ports::fido2_backend::Fido2Backend;
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// Runs as the first statement of every `domain::workflows::*` function (AD-4).
/// Collects missing dependencies from all three ports before returning, so a
/// user missing more than one dependency sees all of them in a single run.
/// `filesystem` is forwarded to `fs.check_prerequisites` unchanged: `None`
/// for every workflow but `create`/`resize`, which pass the requested/
/// existing filesystem type so only that type's toolchain is checked.
pub fn check(
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
    filesystem: Option<Filesystem>,
) -> Result<(), DomainError> {
    let mut missing = Vec::new();

    if let Err(errors) = luks.check_prerequisites() {
        missing.extend(errors);
    }
    if let Err(errors) = fido2.check_prerequisites() {
        missing.extend(errors);
    }
    if let Err(errors) = fs.check_prerequisites(filesystem) {
        missing.extend(errors);
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(DomainError::PreflightFailed(missing))
    }
}
