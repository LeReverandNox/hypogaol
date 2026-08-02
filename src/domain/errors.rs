use std::path::PathBuf;

use thiserror::Error;

use crate::domain::hooks::HookRejectionReason;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("missing required dependencies: {}", .0.join(", "))]
    PreflightFailed(Vec<String>),

    #[error("destination already exists: {}", .0.display())]
    DestinationExists(PathBuf),

    #[error("refusing to remove the last remaining FIDO2 key: at least one key must always stay enrolled")]
    LastKeyslotGuard,

    #[error("{} already carries a LUKS2 header", .0.display())]
    DeviceAlreadyFormatted(PathBuf),

    #[error("device-backed create requires explicit confirmation of the wipe/data-loss warning")]
    DeviceConfirmationRequired,

    #[error(
        "requested size {requested} bytes exceeds {} capacity of {capacity} bytes",
        .path.display()
    )]
    DeviceSizeExceedsCapacity {
        path: PathBuf,
        requested: u64,
        capacity: u64,
    },

    #[error(
        "resolved size {size} bytes for {} is too small for a viable volume",
        .path.display()
    )]
    DeviceTooSmall { path: PathBuf, size: u64 },

    #[error(
        "requested size {requested} bytes is not larger than the current size {current_size} bytes for {} — resize is grow-only",
        .path.display()
    )]
    ResizeMustGrow {
        path: PathBuf,
        requested: u64,
        current_size: u64,
    },

    #[error("{0}")]
    AdapterFailure(String),

    #[error("no FIDO2 key labeled {0:?} is enrolled on this volume")]
    KeyNotFound(String),

    #[error("exec-hooks at {} was rejected: {reason:?}", .path.display())]
    HookRejected {
        path: PathBuf,
        reason: HookRejectionReason,
    },

    #[error("{original}")]
    RollbackCleanupAlsoFailed {
        original: Box<DomainError>,
        close_detail: String,
    },
}

impl DomainError {
    /// Wraps `self` to note that a best-effort rollback `luks.close` also
    /// failed while unwinding from `self`, instead of the previous discipline
    /// of `let _ = luks.close(...)` silently dropping that detail — the
    /// mapping may now be left open. `self` stays the primary, correctly
    /// translated cause; `ux::translate` unwraps `original` first and appends
    /// a note, so no existing translation (including this one) loses
    /// fidelity by being flattened into a generic string.
    pub fn with_rollback_cleanup_failure(self, close_err: DomainError) -> DomainError {
        DomainError::RollbackCleanupAlsoFailed {
            original: Box::new(self),
            close_detail: close_err.to_string(),
        }
    }
}
