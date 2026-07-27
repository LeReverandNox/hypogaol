use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::mapping_name;
use crate::domain::preflight;
use crate::domain::progress::ResizeStage;
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
///
/// `progress` fires at each real stage boundary in that same order:
/// `GrowingBackingFile` (file-backed targets only — a device-backed target
/// never fires this stage) → `ResizingLuks2Mapping` → `GrowingFilesystem`. A
/// stage only fires once its preceding port call has actually succeeded; if
/// any port call returns `Err`, no later stage in this list fires.
pub fn run(
    path: &Path,
    new_size: u64,
    progress: &dyn Fn(ResizeStage),
    luks: &dyn LuksBackend,
    fido2: &dyn Fido2Backend,
    fs: &dyn FilesystemBackend,
) -> Result<(), DomainError> {
    preflight::check(luks, fido2, fs)?;

    let name = mapping_name::mapping_name(path)?;
    let device_backed = fs.is_block_device(path)?;

    // Tier 1 of the grow-only check (AD-10: rejected "before calling any
    // adapter" for the common/obvious cases) — see this story's Dev Notes
    // "Open Design Question" for why a single pre-open check can't fully
    // enforce AC #3 for a device-backed tomb using Story 1.6's headroom
    // feature; tier 2 below closes that gap once the mapping is open. Runs
    // before `read_filesystem` below so an obviously-invalid request never
    // reaches a real adapter call at all (AC #3's literal "before calling
    // any adapter" — review finding, 2026-07-26).
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
        reject_symlink(path)?;
        let current_len = current_file_len(path)?;
        // Strict shrink only, not `<=`: a retry after an earlier resize
        // already grew the backing file to `new_size` but failed before
        // `luks.resize`/`growfs` completed must not be rejected here as a
        // false no-op — tier 2 below re-checks against the filesystem's
        // own (still-unfinished) size and is the authoritative check for
        // the equal-size case (review finding, 2026-07-26).
        if new_size < current_len {
            return Err(DomainError::ResizeMustGrow {
                path: path.to_path_buf(),
                requested: new_size,
                current_size: current_len,
            });
        }
    }

    // Read only now that tier 1 has had its chance to reject — still needed
    // by tier 2 below, so it must run before `luks.open`. Can run either
    // before or after `open` (it reads header/token state, not the live
    // mapping); placed here so a read failure aborts before the mapping is
    // opened at all.
    let filesystem = luks.read_filesystem(path)?;

    let mapper = luks.open(path, &name, false)?;

    // Rollback discipline (mirrors `unlock::run`/`create::run`): once `open`
    // succeeds, every subsequent exit path must close the mapping — no
    // partial "undo" of a successful set_backing_file_size/resize/growfs
    // step, just close and propagate whichever error occurred.
    let result = grow_open_mapping(
        path,
        new_size,
        device_backed,
        filesystem,
        progress,
        &mapper,
        luks,
        fs,
    );
    match result {
        Ok(()) => luks
            .close(&mapper)
            .map_err(|err| grow_succeeded_close_failed(new_size, err)),
        Err(err) => {
            let _ = luks.close(&mapper);
            Err(err)
        }
    }
}

/// Distinguishes "the grow itself failed" from "the grow succeeded but the
/// subsequent re-lock didn't" — both would otherwise surface via the same
/// generic close-failure message, leaving the user thinking growth never
/// happened when the tomb's capacity was in fact already safely increased
/// (review finding, 2026-07-26).
fn grow_succeeded_close_failed(new_size: u64, err: DomainError) -> DomainError {
    let detail = match err {
        DomainError::AdapterFailure(inner) => inner,
        other => format!("{other:?}"),
    };
    DomainError::AdapterFailure(format!(
        "tomb grown to {new_size} bytes, but failed to re-lock afterward: {detail}"
    ))
}

/// Refuses a symlinked file-backed target before any hardware interaction —
/// `set_backing_file_size`'s own symlink guard runs too late to avoid an
/// otherwise-wasted FIDO2 touch, since it only runs after `luks.open`
/// (review finding, 2026-07-26).
fn reject_symlink(path: &Path) -> Result<(), DomainError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|e| {
        DomainError::AdapterFailure(format!("failed to stat {}: {e}", path.display()))
    })?;
    if !metadata.file_type().is_file() {
        return Err(DomainError::AdapterFailure(format!(
            "{} is not a regular file — refusing to grow it",
            path.display()
        )));
    }
    Ok(())
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
    progress: &dyn Fn(ResizeStage),
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
        progress(ResizeStage::GrowingBackingFile);
        fs.set_backing_file_size(path, new_size)?;
    }

    progress(ResizeStage::ResizingLuks2Mapping);
    luks.resize(mapper)?;

    progress(ResizeStage::GrowingFilesystem);
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
