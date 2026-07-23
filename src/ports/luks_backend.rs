use std::path::Path;

use crate::domain::errors::DomainError;
use crate::domain::types::{Filesystem, KeyslotInfo, KeyslotRef, MapperHandle};

pub trait LuksBackend {
    /// `Err` carries one human-readable string per missing/unsupported dependency
    /// this port's real adapter needs; `Ok(())` means all are satisfied.
    fn check_prerequisites(&self) -> Result<(), Vec<String>>;

    /// True if `path` already carries a LUKS2 header (AD-9's device-mode
    /// refusal check) — a pure query, no mutation.
    fn has_luks2_header(&self, path: &Path) -> Result<bool, DomainError>;

    /// Formats a brand-new LUKS2 header at `path` seeded with a transient random
    /// passphrase, then opens it as `name`, returning the resulting mapping
    /// (AD-9). `size` constrains the LUKS2 payload to exactly that many bytes
    /// of the underlying storage, leaving any remainder untouched (AC #2 of
    /// Story 1.6). The transient passphrase never crosses into `domain`.
    fn bootstrap_format_and_open(
        &self,
        path: &Path,
        name: &str,
        size: u64,
        filesystem: Filesystem,
    ) -> Result<MapperHandle, DomainError>;

    /// Live keyslots with an associated `systemd-fido2` token, read fresh from
    /// the header every call — the only legitimate way to count valid keyslots (AD-5).
    fn list_fido2_keyslots(&self, path: &Path) -> Result<Vec<KeyslotInfo>, DomainError>;

    /// Removes `keyslot`'s token metadata first, then the keyslot itself (AD-5's
    /// crash-safe ordering).
    fn remove_key(&self, path: &Path, keyslot: KeyslotRef) -> Result<(), DomainError>;

    /// Releases `mapper`'s dm-crypt mapping, leaving the tomb closed/at rest.
    fn close(&self, mapper: &MapperHandle) -> Result<(), DomainError>;
}
