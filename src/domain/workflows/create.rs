use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::keyslot_guard;
use crate::domain::mapping_name;
use crate::domain::preflight;
use crate::domain::progress::CreateStage;
use crate::domain::types::{CreateTarget, Filesystem, KeyMetadata, KeyslotRef, MapperHandle};
use crate::ports::fido2_backend::{Fido2Backend, Fido2DeviceSelection};
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// The sole keyslot `bootstrap_format_and_open`'s `luksFormat` creates: a
/// brand-new LUKS2 header always assigns its first (and, until FIDO2 enrolls,
/// only) keyslot to index 0.
const BOOTSTRAP_KEYSLOT: KeyslotRef = KeyslotRef(0);

/// Minimum viable volume size: large enough to hold a LUKS2 header/keyslot area
/// plus a minimal ext4 filesystem. Enforced here (not just by the CLI's
/// `parse_size`) because a device-backed create's size can also come from an
/// unvalidated `device_capacity` reading with no `--size` given.
///
/// Confirmed empirically (`cryptsetup luksDump`, 2026-07-27): a default LUKS2
/// header's payload offset is exactly 16 MiB, so a 16 MiB volume leaves zero
/// bytes for the filesystem — `luksFormat`/`luksOpen` don't reject this size
/// outright, they fail later with "too small for activation, there is no
/// remaining space for data", surfacing to the user as a nonsensical
/// FIDO2-touch/PIN error (see `cli::ux`'s `cryptsetup` bucket). 32 MiB leaves
/// a real 16 MiB payload, comfortably above `mkfs.ext4`'s own minimum (2 MiB
/// avoids even its degraded "too small for a journal" case, also confirmed
/// empirically).
pub const MIN_VOLUME_SIZE_BYTES: u64 = 32 * 1024 * 1024;

/// `mkfs.xfs`'s own real minimum, confirmed empirically against real
/// hardware (2026-08-09): it refuses outright with "Filesystem must be
/// larger than 300MB" below that floor — no exact byte boundary was
/// reverse-engineered beyond that message, so 350 MiB total (leaving a
/// ~334 MiB post-LUKS2-header payload) gives comfortable margin above 300MB
/// under either a decimal-MB or binary-MiB reading of `mkfs.xfs`'s own
/// wording. Only `Xfs` needs a filesystem-specific floor above
/// `MIN_VOLUME_SIZE_BYTES`: Btrfs's `--mixed` mode viable minimum (~16 MiB
/// payload) is already below the generic floor, and ext4's is lower still.
pub const MIN_XFS_VOLUME_SIZE_BYTES: u64 = 350 * 1024 * 1024;

/// The smallest total (pre-LUKS2-header) size `filesystem` can actually be
/// formatted at — `MIN_VOLUME_SIZE_BYTES` for every filesystem except `Xfs`,
/// which needs its own, much larger floor.
fn size_floor_for(filesystem: Filesystem) -> u64 {
    match filesystem {
        Filesystem::Xfs => MIN_XFS_VOLUME_SIZE_BYTES,
        Filesystem::Ext4 | Filesystem::Btrfs => MIN_VOLUME_SIZE_BYTES,
    }
}

/// `progress` fires at each real stage boundary, in the real execution order
/// (AD-19): `AllocatingBackingFile` (File targets only — a Device target
/// never allocates a backing file, so this stage never fires for it) →
/// `FormattingLuks2` → `EnrollingFido2Key` → `CreatingFilesystem`. A stage
/// only fires once its preceding port call has actually succeeded; if any
/// port call returns `Err`, no later stage in this list fires.
pub fn run(
    target: CreateTarget,
    filesystem: Filesystem,
    user_verification: bool,
    key_label: Option<String>,
    scaffold_hooks: bool,
    fido2_selection: Fido2DeviceSelection,
    progress: &dyn Fn(CreateStage),
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    preflight::check(luks, fido2, fs, Some(filesystem))?;

    match target {
        CreateTarget::File { path, size } => {
            // A destination that already exists is only refused if it
            // doesn't carry CAP-23's marker token — a marker-verified
            // resume falls through exactly as if the path hadn't existed,
            // with no confirmation prompt (AC #1). A genuine pre-existing
            // file/volume (no marker) refuses unchanged (AC #3).
            if fs.path_exists(&path) && !luks.has_marker_token(&path)? {
                return Err(DomainError::DestinationExists(path));
            }

            // The CLI's `parse_size` already floor-checks `--size` against
            // the generic minimum (it has no way to know which filesystem
            // was requested at the time it parses `--size`), but this is
            // domain's own independent guarantee (mirroring the Device
            // branch below) rather than a trust that the CLI is the only
            // caller that will ever construct a `CreateTarget::File` —
            // and the only place that can apply a filesystem-specific
            // floor, since it's the first point both `size` and
            // `filesystem` are known together.
            if size < size_floor_for(filesystem) {
                return Err(DomainError::DeviceTooSmall { path, size });
            }

            progress(CreateStage::AllocatingBackingFile);
            fs.set_backing_file_size(&path, size)?;

            // From here on, the backing file exists: any failure below must
            // remove it again before returning, or every future `create` at
            // this same destination would permanently hit `DestinationExists`
            // with no way to recover.
            let result = bootstrap_and_provision(
                &path,
                size,
                filesystem,
                user_verification,
                key_label,
                scaffold_hooks,
                fido2_selection,
                progress,
                luks,
                fido2,
                fs,
            );
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
            // first. A header with CAP-23's marker token is a
            // marker-verified resume: it skips the confirmation check
            // entirely (AC #2), same as the File branch skips its
            // confirmation-free refusal. A header without the marker
            // refuses unchanged. Size resolution/validation runs last on
            // every path that reaches it — including resume — since it
            // needs an extra adapter call and has no bearing on whether the
            // destination should be refused outright.
            let marker_verified_resume = if luks.has_luks2_header(&path)? {
                if luks.has_marker_token(&path)? {
                    true
                } else {
                    return Err(DomainError::DeviceAlreadyFormatted(path));
                }
            } else {
                false
            };
            if !marker_verified_resume && !confirmed {
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

            // Below this, `cryptsetup luksFormat`/`mkfs.*` fail deep inside
            // the adapter with a cryptic error instead of a clear refusal.
            // An explicit `--size` is already floor-checked by the CLI's
            // `parse_size` against the generic minimum, but a
            // defaulted-from-capacity size (no `--size` given) never passes
            // through that check — this is domain's own independent
            // guarantee, not a trust in the CLI having done it — and, same
            // as the File branch above, the only place that can apply a
            // filesystem-specific floor.
            if resolved_size < size_floor_for(filesystem) {
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
                user_verification,
                key_label,
                scaffold_hooks,
                fido2_selection,
                progress,
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
    user_verification: bool,
    key_label: Option<String>,
    scaffold_hooks: bool,
    fido2_selection: Fido2DeviceSelection,
    progress: &dyn Fn(CreateStage),
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    let name = mapping_name::mapping_name(path)?;

    // A real process-death crash between a prior attempt's successful
    // luksOpen and this function's own close-on-completion below skips that
    // cleanup entirely — dm-crypt mappings are kernel objects, independent
    // of the process that opened them. Left unhandled, a marker-verified
    // resume attempt would compute this exact deterministic name and its
    // luksFormat call below would fail (device/name busy) before ever
    // reaching that logic. Safe to run unconditionally on a fresh create
    // too: a mapping can only exist under this exact name if this tool
    // already reached luksOpen on this same path, and any failure other
    // than "no such mapping" (e.g. still busy/mounted for an unrelated
    // reason) correctly aborts here rather than forcing through a mapping
    // still in legitimate use.
    luks.close_stale_mapping(&name)?;

    progress(CreateStage::FormattingLuks2);
    let mapper = luks.bootstrap_format_and_open(path, &name, size, filesystem)?;

    // Whatever happens next, a successfully opened mapping must be closed —
    // otherwise a mid-flow failure leaks an open `/dev/mapper/vault-*`
    // mapping indefinitely, same as this story's post-review hardware-run fix
    // for the happy path, just extended to the failure paths too.
    let result = finish_provisioning(
        &mapper,
        filesystem,
        user_verification,
        key_label,
        scaffold_hooks,
        fido2_selection,
        progress,
        luks,
        fido2,
        fs,
    );
    match result {
        Ok(()) => luks.close(&mapper),
        Err(err) => Err(match luks.close(&mapper) {
            Ok(()) => err,
            Err(close_err) => {
                err.with_rollback_cleanup_failure("re-lock the LUKS2 mapping", close_err)
            }
        }),
    }
}

fn finish_provisioning(
    mapper: &MapperHandle,
    filesystem: Filesystem,
    user_verification: bool,
    key_label: Option<String>,
    scaffold_hooks: bool,
    fido2_selection: Fido2DeviceSelection,
    progress: &dyn Fn(CreateStage),
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
        key_label: key_label.unwrap_or_else(|| "primary".to_string()),
        filesystem,
    };
    progress(CreateStage::EnrollingFido2Key);
    fido2.enroll_fido2_key(mapper, metadata, fido2_selection, user_verification)?;

    progress(CreateStage::CreatingFilesystem);
    fs.mkfs(mapper, filesystem)?;

    // Scaffolding must land strictly after mkfs (a filesystem must exist to
    // hold the files) and strictly before final marker/keyslot cleanup
    // (AD-9's Rule, AC #3) — no separate FIDO2 selection needed, since
    // `mapper` is already live at this point.
    if scaffold_hooks {
        progress(CreateStage::ScaffoldingHookTemplates);
        let mountpoint = fs.mount(mapper, false)?;
        let scaffold_result = fs.scaffold_hook_templates(&mountpoint);
        match fs.umount(mapper) {
            Ok(()) => scaffold_result?,
            Err(umount_err) => {
                return Err(match scaffold_result {
                    Ok(()) => umount_err,
                    Err(scaffold_err) => scaffold_err
                        .with_rollback_cleanup_failure("unmount the filesystem", umount_err),
                });
            }
        }
    }

    // Marker removed first, bootstrap keyslot second — this order is
    // load-bearing (AD-9/CAP-23 AC #4). A crash between the two leaves a
    // harmless stray keyslot on an already-functional volume, correctly
    // read as "genuine pre-existing volume" (no marker) by a future
    // create's has_marker_token check. The reverse order would let a
    // future create misread a completed volume's surviving marker as
    // resumable and silently wipe it.
    luks.remove_marker_token(&mapper.source_path)?;
    keyslot_guard::remove_keyslot_guarded(luks, &mapper.source_path, BOOTSTRAP_KEYSLOT)
}
