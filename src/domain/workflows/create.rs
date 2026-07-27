use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::keyslot_guard;
use crate::domain::mapping_name;
use crate::domain::preflight;
use crate::domain::types::{CreateTarget, Filesystem, KeyMetadata, KeyslotRef, MapperHandle};
use crate::ports::fido2_backend::{Fido2Backend, Fido2DeviceSelection};
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// The sole keyslot `bootstrap_format_and_open`'s `luksFormat` creates: a
/// brand-new LUKS2 header always assigns its first (and, until FIDO2 enrolls,
/// only) keyslot to index 0.
const BOOTSTRAP_KEYSLOT: KeyslotRef = KeyslotRef(0);

/// Minimum viable tomb size: large enough to hold a LUKS2 header/keyslot area
/// plus a minimal ext4 filesystem. Enforced here (not just by the CLI's
/// `parse_size`) because a device-backed create's size can also come from an
/// unvalidated `device_capacity` reading with no `--size` given.
///
/// Confirmed empirically (`cryptsetup luksDump`, 2026-07-27): a default LUKS2
/// header's payload offset is exactly 16 MiB, so a 16 MiB tomb leaves zero
/// bytes for the filesystem — `luksFormat`/`luksOpen` don't reject this size
/// outright, they fail later with "too small for activation, there is no
/// remaining space for data", surfacing to the user as a nonsensical
/// FIDO2-touch/PIN error (see `cli::ux`'s `cryptsetup` bucket). 32 MiB leaves
/// a real 16 MiB payload, comfortably above `mkfs.ext4`'s own minimum (2 MiB
/// avoids even its degraded "too small for a journal" case, also confirmed
/// empirically).
pub const MIN_TOMB_SIZE_BYTES: u64 = 32 * 1024 * 1024;

pub fn run(
    target: CreateTarget,
    filesystem: Filesystem,
    fido2_selection: Fido2DeviceSelection,
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

            // The CLI's `parse_size` already floor-checks `--size`, but this
            // is domain's own independent guarantee (mirroring the Device
            // branch below) rather than a trust that the CLI is the only
            // caller that will ever construct a `CreateTarget::File`.
            if size < MIN_TOMB_SIZE_BYTES {
                return Err(DomainError::DeviceTooSmall { path, size });
            }

            fs.set_backing_file_size(&path, size)?;

            // From here on, the backing file exists: any failure below must
            // remove it again before returning, or every future `create` at
            // this same destination would permanently hit `DestinationExists`
            // with no way to recover.
            let result =
                bootstrap_and_provision(&path, size, filesystem, fido2_selection, luks, fido2, fs);
            if result.is_err() {
                let _ = fs.remove_backing_file(&path);
            }
            result
        }
        CreateTarget::Device {
            path,
            size,
            confirmed,
        } => {
            // Order is load-bearing (AD-9): the header check must win even
            // when `confirmed` is true (AC #4), so it runs unconditionally
            // first. Confirmation is checked second, independent of header
            // state (AC #5). Size resolution/validation runs last, since it
            // needs an extra adapter call and has no bearing on whether the
            // destination should be refused outright.
            if luks.has_luks2_header(&path)? {
                return Err(DomainError::DeviceAlreadyFormatted(path));
            }
            if !confirmed {
                return Err(DomainError::DeviceConfirmationRequired);
            }

            let capacity = fs.device_capacity(&path)?;
            let resolved_size = match size {
                Some(requested) if requested > capacity => {
                    return Err(DomainError::DeviceSizeExceedsCapacity {
                        path,
                        requested,
                        capacity,
                    });
                }
                Some(requested) => requested,
                None => capacity,
            };

            // Below this, `cryptsetup luksFormat`/`mkfs.ext4` fail deep inside
            // the adapter with a cryptic error instead of a clear refusal.
            // An explicit `--size` is already floor-checked by the CLI's
            // `parse_size`, but a defaulted-from-capacity size (no `--size`
            // given) never passes through that check — this is domain's own
            // independent guarantee, not a trust in the CLI having done it.
            if resolved_size < MIN_TOMB_SIZE_BYTES {
                return Err(DomainError::DeviceTooSmall {
                    path,
                    size: resolved_size,
                });
            }

            // No backing file was ever created for a device/partition target,
            // so — unlike the File branch — a failure here must not attempt
            // any file removal.
            bootstrap_and_provision(
                &path,
                resolved_size,
                filesystem,
                fido2_selection,
                luks,
                fido2,
                fs,
            )
        }
    }
}

fn bootstrap_and_provision(
    path: &Path,
    size: u64,
    filesystem: Filesystem,
    fido2_selection: Fido2DeviceSelection,
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
    let result = finish_provisioning(&mapper, filesystem, fido2_selection, luks, fido2, fs);
    match result {
        Ok(()) => luks.close(&mapper),
        Err(err) => {
            let _ = luks.close(&mapper);
            Err(err)
        }
    }
}

fn finish_provisioning(
    mapper: &MapperHandle,
    filesystem: Filesystem,
    fido2_selection: Fido2DeviceSelection,
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
    fido2.enroll_fido2_key(mapper, metadata, fido2_selection)?;

    fs.mkfs(mapper, filesystem)?;

    keyslot_guard::remove_keyslot_guarded(luks, &mapper.source_path, BOOTSTRAP_KEYSLOT)
}
