use std::path::{Path, PathBuf};

use crate::domain::errors::DomainError;
use crate::domain::types::{Filesystem, MapperHandle};

pub trait FilesystemBackend {
    /// `Err` carries one human-readable string per missing/unsupported dependency
    /// this port's real adapter needs; `Ok(())` means all are satisfied.
    fn check_prerequisites(&self) -> Result<(), Vec<String>>;

    /// True if `path` already exists (AD-9's create-mode refusal check).
    fn path_exists(&self, path: &Path) -> bool;

    /// True if `path` is a raw block device/partition; false if it's a
    /// regular file (Story 3.2, AC #1/#2's file-vs-device distinction) — a
    /// pure query, no mutation.
    fn is_block_device(&self, path: &Path) -> Result<bool, DomainError>;

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

    /// Grows `fs` on `mapper`'s already-resized mapping to fill it (v1:
    /// `Filesystem::Ext4` only, AD-8) — Story 3.2, AC #1/#4. Called after
    /// `LuksBackend::resize`, so the mapping already reflects the new,
    /// larger size; no explicit target size is passed.
    fn growfs(&self, mapper: &MapperHandle, fs: Filesystem) -> Result<(), DomainError>;

    /// The `fs` filesystem's own current size on `mapper`'s active mapping
    /// (v1: `Filesystem::Ext4` only) — a pure query, no mutation. This is
    /// deliberately distinct from `device_capacity(&mapper.device_node())`:
    /// confirmed empirically on real hardware that a LUKS2 mapping's dynamic
    /// segment always reflects the *full* backing storage on every reopen,
    /// even for a device-backed tomb created with less than the raw
    /// device's full capacity (Story 1.6 headroom) — so the mapping's own
    /// size can never distinguish "this tomb's filesystem currently uses
    /// less than the raw device" from "it uses all of it." Only the
    /// filesystem's own superblock (block count × block size) reports the
    /// tomb's true current provisioned size.
    fn filesystem_size(&self, mapper: &MapperHandle, fs: Filesystem) -> Result<u64, DomainError>;

    /// Best-effort removal of a backing file this adapter created — used to
    /// clean up after a file-backed `create` fails partway through, so a
    /// retry at the same destination isn't permanently blocked.
    fn remove_backing_file(&self, path: &Path) -> Result<(), DomainError>;

    /// Mounts `mapper`'s decrypted device node at a fresh, uniquely-named
    /// mount point, letting the kernel auto-detect the filesystem type from
    /// the superblock. Returns the mount point. The mount point is never
    /// stored or derived from `mapper`'s path (AD-12) — rediscovering it
    /// later is `close`'s job (Story 3.1), via the kernel's own mount table.
    /// `read_only` maps to `mount -o ro` (AD-11).
    fn mount(&self, mapper: &MapperHandle, read_only: bool) -> Result<PathBuf, DomainError>;

    /// Unmounts `mapper`'s decrypted device node, resolving the live mount
    /// point itself via the kernel's mount table (AD-12) — takes the mapper,
    /// never a mountpoint, since none is ever stored. Also removes the
    /// now-empty mount-point directory `mount` created, so a later re-unlock
    /// of the same tomb gets the plain basename back rather than permanently
    /// falling back to a collision-suffixed name.
    fn umount(&self, mapper: &MapperHandle) -> Result<(), DomainError>;
}
