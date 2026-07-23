use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::types::{Filesystem, MapperHandle};

pub trait FilesystemBackend {
    /// `Err` carries one human-readable string per missing/unsupported dependency
    /// this port's real adapter needs; `Ok(())` means all are satisfied.
    fn check_prerequisites(&self) -> Result<(), Vec<String>>;

    /// True if `path` already exists (AD-9's create-mode refusal check).
    fn path_exists(&self, path: &Path) -> bool;

    /// Creates (or truncates/extends) the backing file at `path` to exactly `size` bytes.
    fn set_backing_file_size(&self, path: &Path, size: u64) -> Result<(), DomainError>;

    /// Formats the opened mapping with `fs` (v1: `Filesystem::Ext4` only, AD-8).
    fn mkfs(&self, mapper: &MapperHandle, fs: Filesystem) -> Result<(), DomainError>;
}
