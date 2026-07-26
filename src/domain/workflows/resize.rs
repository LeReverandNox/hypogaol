use std::os::unix::fs::FileTypeExt;
use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::mapping_name;
use crate::domain::preflight;
use crate::domain::types::MapperHandle;
use crate::ports::fido2_backend::Fido2Backend;
use crate::ports::filesystem_backend::FilesystemBackend;
use crate::ports::luks_backend::LuksBackend;

/// `fido2` is unused beyond `preflight::check` — kept in the signature only
/// for AD-4's uniform three-port preflight gate, same as every sibling
/// workflow.
///
/// `resize` grows an existing tomb's volume and filesystem to `new_size`
/// (AC #1/#4). Ordering is AD-10-mandated and non-negotiable: file-backed
/// storage grows first, then the LUKS2 mapping, then the filesystem —
/// reversing any of these risks growing a filesystem onto space the LUKS
/// mapping doesn't have yet.
pub fn run(
    path: &Path,
    new_size: u64,
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    preflight::check(luks, fido2, fs)?;

    let name = mapping_name::mapping_name(path)?;
    let device_backed = is_block_device(path)?;

    // Tier 1 of the grow-only check (AD-10: rejected "before calling any
    // adapter" for the common/obvious cases) — see this story's Dev Notes
    // "Open Design Question" for why a single pre-open check can't fully
    // enforce AC #3 for a device-backed tomb using Story 1.6's headroom
    // feature; tier 2 below closes that gap once the mapping is open.
    if device_backed {
        let capacity = fs.device_capacity(path)?;
        if new_size > capacity {
            return Err(DomainError::DeviceSizeExceedsCapacity {
                path: path.to_path_buf(),
                requested: new_size,
                capacity,
            });
        }
    } else {
        let current_len = current_file_len(path)?;
        if new_size <= current_len {
            return Err(DomainError::ResizeMustGrow {
                path: path.to_path_buf(),
                requested: new_size,
                current_size: current_len,
            });
        }
    }

    let mapper = luks.open(path, &name)?;

    // Rollback discipline (mirrors `unlock::run`/`create::run`): once `open`
    // succeeds, every subsequent exit path must close the mapping — no
    // partial "undo" of a successful set_backing_file_size/resize/growfs
    // step, just close and propagate whichever error occurred.
    let result = grow_open_mapping(path, new_size, device_backed, &mapper, luks, fs);
    match result {
        Ok(()) => luks.close(&mapper),
        Err(err) => {
            let _ = luks.close(&mapper);
            Err(err)
        }
    }
}

/// The rest of `resize`'s work once `mapper` is open: tier 2 of the
/// grow-only check, then the AD-10 ordering itself (backing file, LUKS2
/// mapping, filesystem). The caller (`run`) is responsible for closing
/// `mapper` on every exit path.
fn grow_open_mapping(
    path: &Path,
    new_size: u64,
    device_backed: bool,
    mapper: &MapperHandle,
    luks: &dyn LuksBackend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    // Tier 2 (unavoidable — resize/growfs need the mapping active
    // regardless): re-derives the true current provisioned size from the
    // active mapping itself — the only way to learn a device-backed tomb's
    // real current size when it's smaller than the raw device's (Story 1.6
    // headroom), since nothing about it is ever persisted (AD-2). Runs
    // after `open` (an authentication/read operation, not a mutation) but
    // strictly before any mutating call below.
    let live_current_size = fs.device_capacity(&mapper.device_node())?;
    if new_size <= live_current_size {
        return Err(DomainError::ResizeMustGrow {
            path: path.to_path_buf(),
            requested: new_size,
            current_size: live_current_size,
        });
    }

    if !device_backed {
        fs.set_backing_file_size(path, new_size)?;
    }

    luks.resize(mapper)?;

    let filesystem = luks.read_filesystem(path)?;

    fs.growfs(mapper, filesystem)
}

/// True if `path` is a raw block device/partition; false if it's a regular
/// file. `resize`'s only per-target-type branch (AC #1/#2's file-vs-device
/// distinction) — everything else in this workflow is identical for both,
/// consistent with every other workflow's "identical command works
/// unmodified" convention.
fn is_block_device(path: &Path) -> Result<bool, DomainError> {
    let metadata = std::fs::metadata(path).map_err(|e| {
        DomainError::AdapterFailure(format!("failed to stat {}: {e}", path.display()))
    })?;
    Ok(metadata.file_type().is_block_device())
}

/// A file-backed target's current length via a plain stdlib stat — not even
/// "an adapter" in AD-10's sense, so this satisfies tier 1's true
/// zero-adapter-call reading for the file-backed case.
fn current_file_len(path: &Path) -> Result<u64, DomainError> {
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|e| {
            DomainError::AdapterFailure(format!(
                "failed to read current size of {}: {e}",
                path.display()
            ))
        })
}
