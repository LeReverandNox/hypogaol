use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::mapping_name;
use crate::domain::preflight;
use crate::domain::types::MapperHandle;
use crate::ports::fido2_backend::Fido2Backend;
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// `fido2` is unused beyond `preflight::check` — kept in the signature only
/// for AD-4's uniform three-port preflight gate, same as `unlock`/`revoke`.
///
/// Unlike `unlock`, `close` never opens a mapping itself — it acts on one
/// that's already open, so the `MapperHandle` is built directly from the
/// derived name rather than obtained via `luks.open`. There is also no
/// mount-failure rollback to mirror: `unlock`'s rollback closes a mapping
/// *it* just opened; `close` opens nothing, so an `umount` failure simply
/// propagates without calling `luks.close` (AC #1's ordering — never lock a
/// mapping that may still be busy) — except "not currently mounted", which
/// means a prior `close` already unmounted but failed before reaching
/// `luks.close`; treating that as done and proceeding makes a retry
/// self-healing instead of permanently stuck (review finding, 2026-07-26).
pub fn run(
    path: &Path,
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    preflight::check(luks, fido2, fs)?;

    let name = mapping_name::mapping_name(path)?;
    let mapper = MapperHandle {
        name,
        source_path: path.to_path_buf(),
    };

    match fs.umount(&mapper) {
        Ok(()) => {}
        Err(DomainError::AdapterFailure(msg)) if msg.contains("not currently mounted") => {}
        Err(err) => return Err(err),
    }

    luks.close(&mapper)
}
