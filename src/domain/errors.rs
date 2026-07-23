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

    #[error("{0}")]
    AdapterFailure(String),
}
