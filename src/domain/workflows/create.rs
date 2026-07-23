use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::keyslot_guard;
use crate::domain::mapping_name;
use crate::domain::preflight;
use crate::domain::types::{CreateTarget, Filesystem, KeyMetadata, KeyslotRef, MapperHandle};
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

            // From here on, the backing file exists: any failure below must
            // remove it again before returning, or every future `create` at
            // this same destination would permanently hit `DestinationExists`
            // with no way to recover.
            let result = bootstrap_file_backed(&path, size, filesystem, luks, fido2, fs);
            if result.is_err() {
                let _ = fs.remove_backing_file(&path);
            }
            result
        }
        CreateTarget::Device { .. } => todo!(),
    }
}

fn bootstrap_file_backed(
    path: &Path,
    size: u64,
    filesystem: Filesystem,
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    let name = mapping_name::mapping_name(path)?;
    let mapper = luks.bootstrap_format_and_open(path, &name, size, filesystem)?;

    // Whatever happens next, a successfully opened mapping must be closed —
    // otherwise a mid-flow failure leaks an open `/dev/mapper/vault-*`
    // mapping indefinitely, same as this story's post-review hardware-run fix
    // for the happy path, just extended to the failure paths too.
    let result = finish_file_backed(&mapper, filesystem, luks, fido2, fs);
    match result {
        Ok(()) => luks.close(&mapper),
        Err(err) => {
            let _ = luks.close(&mapper);
            Err(err)
        }
    }
}

fn finish_file_backed(
    mapper: &MapperHandle,
    filesystem: Filesystem,
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    // Enroll runs before mkfs, not after: systemd-cryptenroll can only add a
    // new keyslot by authenticating with a still-valid existing credential,
    // and the transient bootstrap passphrase is the only one that exists at
    // this point. It lives inside adapters::exec (never crossing into domain,
    // AD-3) between bootstrap_format_and_open and enroll_fido2_key, and is
    // wiped as soon as enroll_fido2_key consumes it — still strictly before
    // mkfs runs, satisfying AC #3/AD-3's wipe-before-mkfs requirement.
    let metadata = KeyMetadata {
        key_label: "primary".to_string(),
        filesystem,
    };
    fido2.enroll_fido2_key(mapper, metadata)?;

    fs.mkfs(mapper, filesystem)?;

    keyslot_guard::remove_keyslot_guarded(luks, &mapper.source_path, BOOTSTRAP_KEYSLOT)
}
