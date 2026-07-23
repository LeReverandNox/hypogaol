use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("missing required dependencies: {}", .0.join(", "))]
    PreflightFailed(Vec<String>),
}
