use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::mapping_name;
use crate::domain::preflight;
use crate::domain::types::{Filesystem, MapperHandle};
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
    let device_backed = fs.is_block_device(path)?;

    // Read early (Task 5's own note: a read failure here should abort
    // before anything is touched) — also needed by tier 2 below to know
    // which tool to query the live filesystem size with.
    let filesystem = luks.read_filesystem(path)?;

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
    let result = grow_open_mapping(path, new_size, device_backed, filesystem, &mapper, luks, fs);
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
    filesystem: Filesystem,
    mapper: &MapperHandle,
    luks: &dyn LuksBackend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    // Tier 2 (unavoidable — resize/growfs need the mapping active
    // regardless): re-derives the true current provisioned size from the
    // filesystem's own superblock, not the LUKS mapping — confirmed
    // empirically on real hardware that a LUKS2 mapping's dynamic segment
    // always reflects the *full* backing storage on reopen, even for a
    // device-backed tomb using Story 1.6 headroom (a smaller-than-capacity
    // create-time constraint is never persisted, AD-2), so
    // `device_capacity(&mapper.device_node())` cannot tell "this tomb
    // currently uses less than the raw device" from "it uses all of it."
    // Only the filesystem's own block-count metadata can. Runs after `open`
    // (an authentication/read operation, not a mutation) but strictly
    // before any mutating call below.
    let live_current_size = fs.filesystem_size(mapper, filesystem)?;
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

    fs.growfs(mapper, filesystem)
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
