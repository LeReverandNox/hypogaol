use crate::domain::errors::DomainError;
use crate::domain::keyslot_guard;
use crate::domain::mapping_name;
use crate::domain::preflight;
use crate::domain::types::{CreateTarget, Filesystem, KeyMetadata, KeyslotRef};
use crate::ports::fido2_backend::Fido2Backend;
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// The sole keyslot `bootstrap_format_and_open`'s `luksFormat` creates: a
/// brand-new LUKS2 header always assigns its first (and, until FIDO2 enrolls,
/// only) keyslot to index 0.
const BOOTSTRAP_KEYSLOT: KeyslotRef = KeyslotRef(0);

pub fn run(
    target: CreateTarget,
    filesystem: Filesystem,
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    preflight::check(luks, fido2, fs)?;

    match target {
        CreateTarget::File { path, size } => {
            if fs.path_exists(&path) {
                return Err(DomainError::DestinationExists(path));
            }

            fs.set_backing_file_size(&path, size)?;
            let name = mapping_name::mapping_name(&path);

            let mapper = luks.bootstrap_format_and_open(&path, &name, filesystem)?;
            fs.mkfs(&mapper, filesystem)?;

            let metadata = KeyMetadata {
                key_label: "primary".to_string(),
                filesystem,
            };
            luks.enroll_fido2_key(&mapper, metadata)?;

            keyslot_guard::remove_keyslot_guarded(luks, &path, BOOTSTRAP_KEYSLOT)
        }
        CreateTarget::Device { .. } => todo!(),
    }
}
