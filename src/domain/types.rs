use std::path::PathBuf;

/// v1 supports only ext4 (AD-8); additional variants are additive later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filesystem {
    Ext4,
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
