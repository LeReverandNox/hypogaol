use std::path::PathBuf;

/// v1 shipped ext4-only (AD-8); Story 6.4 is that additive later, adding
/// `Xfs`/`Btrfs` alongside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filesystem {
    Ext4,
    Xfs,
    Btrfs,
}

/// AD-9: the two `create` target modes, kept as one enum with no shared/
/// overlapping fields so the file/device confirmation gating lives at the
/// type level, not in prose.
#[derive(Debug, Clone)]
pub enum CreateTarget {
    File {
        path: PathBuf,
        size: u64,
    },
    Device {
        path: PathBuf,
        size: Option<u64>,
        confirmed: bool,
    },
}

/// An opened LUKS2 mapping. Carries both the underlying container path
/// (needed by operations that act on the LUKS2 header itself, e.g.
/// `systemd-cryptenroll`) and the dm-crypt mapping name (needed by
/// operations that act on the decrypted block device, e.g. `mkfs`).
#[derive(Debug, Clone)]
pub struct MapperHandle {
    pub name: String,
    pub source_path: PathBuf,
}

impl MapperHandle {
    pub fn device_node(&self) -> PathBuf {
        PathBuf::from(format!("/dev/mapper/{}", self.name))
    }
}

/// Fields supplied by `domain` when enrolling a FIDO2 key (AD-2). The
/// adapter fills in `credential_id`/`created_at` itself at enroll time.
#[derive(Debug, Clone)]
pub struct KeyMetadata {
    pub key_label: String,
    pub filesystem: Filesystem,
}

/// Identifies a specific LUKS2 keyslot by number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyslotRef(pub u32);

/// One live keyslot with an associated `systemd-fido2` token (AD-5's
/// definition of a "valid keyslot").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyslotInfo {
    pub keyslot: KeyslotRef,
    pub key_label: String,
}

/// `stat`/`lstat` facts about a candidate `exec-hooks` file, gathered by the
/// adapter (AC #3). The adapter resolves "owned by invoking user or root"
/// itself, reusing its existing `invoking_identity()` helper — `domain` never
/// needs to know a raw uid, matching how `mount()` already keeps
/// `invoking_identity()` adapter-internal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookFileMeta {
    pub is_regular_file: bool,
    pub is_symlink: bool,
    pub is_executable: bool,
    pub owned_by_invoking_user_or_root: bool,
    pub is_world_writable: bool,
}

/// A holding process's PID, as reported by `fuser -m` (AD-18) — used only
/// by slam's busy-mount escalation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pid(pub u32);

/// The three signals slam's escalation loop sends, in order (AD-18) — a
/// typed enum so only `adapters::exec` maps each variant to its `kill -s`
/// argument; `domain` never handles a raw signal name/number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Sigterm,
    Sighup,
    Sigkill,
}

/// Held for the remainder of a mutating workflow's execution (AD-20).
/// Dropping it closes the underlying fd, which releases the `flock` the
/// kernel would also release automatically on process exit — so a crash
/// mid-operation leaves no stale-lock state to detect or clean up (AC
/// #5). `None` only for `FakeFilesystemBackend`, which holds no real fd.
#[derive(Debug)]
pub struct LockGuard(pub Option<std::os::fd::OwnedFd>);
