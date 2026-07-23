use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::types::{Filesystem, MapperHandle};

pub trait FilesystemBackend {
    /// `Err` carries one human-readable string per missing/unsupported dependency
    /// this port's real adapter needs; `Ok(())` means all are satisfied.
    fn check_prerequisites(&self) -> Result<(), Vec<String>>;

    /// True if `path` already exists (AD-9's create-mode refusal check).
    fn path_exists(&self, path: &Path) -> bool;

    /// Byte capacity of the block device/partition at `path` (AD-9's
    /// device-mode default-to-full-capacity sizing) — a pure query, no
    /// mutation.
    fn device_capacity(&self, path: &Path) -> Result<u64, DomainError>;

    /// Creates the backing file at `path` sized to exactly `size` bytes; fails
    /// if a file (or symlink) already exists at `path` — `path_exists` narrows
    /// the check-then-create race but does not eliminate it, so this call
    /// itself must refuse to clobber anything already there (AC #2).
    fn set_backing_file_size(&self, path: &Path, size: u64) -> Result<(), DomainError>;

    /// Formats the opened mapping with `fs` (v1: `Filesystem::Ext4` only, AD-8).
    fn mkfs(&self, mapper: &MapperHandle, fs: Filesystem) -> Result<(), DomainError>;

    /// Best-effort removal of a backing file this adapter created — used to
    /// clean up after a file-backed `create` fails partway through, so a
    /// retry at the same destination isn't permanently blocked.
    fn remove_backing_file(&self, path: &Path) -> Result<(), DomainError>;
}
