use std::path::PathBuf;

use thiserror::Error;

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
        "resolved size {size} bytes for {} is too small for a viable tomb",
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

    #[error("no FIDO2 key labeled {0:?} is enrolled on this tomb")]
    KeyNotFound(String),
}
