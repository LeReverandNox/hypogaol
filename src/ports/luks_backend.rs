use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::types::{Filesystem, KeyMetadata, KeyslotInfo, KeyslotRef, MapperHandle};

pub trait LuksBackend {
    /// `Err` carries one human-readable string per missing/unsupported dependency
    /// this port's real adapter needs; `Ok(())` means all are satisfied.
    fn check_prerequisites(&self) -> Result<(), Vec<String>>;

    /// Formats a brand-new LUKS2 header at `path` seeded with a transient random
    /// passphrase, then opens it as `name`, returning the resulting mapping
    /// (AD-9). The transient passphrase never crosses into `domain`.
    fn bootstrap_format_and_open(
        &self,
        path: &Path,
        name: &str,
        filesystem: Filesystem,
    ) -> Result<MapperHandle, DomainError>;

    /// Enrolls the real FIDO2 key as a `systemd-fido2` token+keyslot, writing
    /// `metadata`'s fields onto that same token object (AD-2).
    fn enroll_fido2_key(&self, mapper: &MapperHandle, metadata: KeyMetadata) -> Result<(), DomainError>;

    /// Live keyslots with an associated `systemd-fido2` token, read fresh from
    /// the header every call — the only legitimate way to count valid keyslots (AD-5).
    fn list_fido2_keyslots(&self, path: &Path) -> Result<Vec<KeyslotInfo>, DomainError>;

    /// Removes `keyslot`'s token metadata first, then the keyslot itself (AD-5's
    /// crash-safe ordering).
    fn remove_key(&self, path: &Path, keyslot: KeyslotRef) -> Result<(), DomainError>;
}
