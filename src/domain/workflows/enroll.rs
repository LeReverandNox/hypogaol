use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::mapping_name;
use crate::domain::preflight;
use crate::domain::types::{Filesystem, KeyMetadata, MapperHandle};
use crate::ports::fido2_backend::{Fido2Backend, Fido2DeviceSelection};
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// `luks`/`fs` are unused beyond `preflight::check` — kept in the signature
/// only for AD-4's uniform three-port preflight gate, same documented
/// pattern as `unlock.rs`'s unused `fido2` parameter. `systemd-cryptenroll`/
/// `cryptsetup token *` operate directly on the LUKS2 header at
/// `mapper.source_path`, so enroll never needs `luks.open` or `fs.mount` —
/// the volume is never unlocked/mounted to add a key.
pub fn run(
    path: &Path,
    key_label: String,
    selection: Fido2DeviceSelection,
    user_verification: bool,
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    preflight::check(luks, fido2, fs, None)?;

    let name = mapping_name::mapping_name(path)?;
    let mapper = MapperHandle {
        name,
        source_path: path.to_path_buf(),
    };

    // v1 has only one `Filesystem` variant (AD-8) — nothing to read back
    // from the existing volume or ask the user for.
    let metadata = KeyMetadata {
        key_label,
        filesystem: Filesystem::Ext4,
    };

    fido2.enroll_fido2_key(&mapper, metadata, selection, user_verification)
}
