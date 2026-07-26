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

    /// Opens an existing LUKS2 volume at `path` as `name` via its enrolled
    /// FIDO2 token, prompting for touch/PIN on the real terminal. Performs no
    /// formatting — unlike `bootstrap_format_and_open`, `path` must already
    /// carry a LUKS2 header. `name` is derived by the caller via the shared
    /// `mapping_name` helper, never computed here (AD-12). `read_only` maps
    /// to `cryptsetup open --readonly` (AD-11).
    fn open(&self, path: &Path, name: &str, read_only: bool) -> Result<MapperHandle, DomainError>;

    /// Grows `mapper`'s already-open LUKS2 mapping to fill its now-larger
    /// backing storage (Story 3.2, AC #1/#4). Takes no explicit size: the
    /// header's segment sizing stays `"dynamic"` (confirmed at create time,
    /// see `ExecAdapter::bootstrap_format_and_open`'s doc comment) and
    /// recomputes from the backing file/device's actual current size, as
    /// long as that backing storage was already grown before this call runs.
    /// Re-authenticates via the enrolled FIDO2 token, prompting for
    /// touch/PIN on the real terminal — confirmed on real hardware that a
    /// bare `resize` does NOT reuse the kernel keyring entry a preceding
    /// `open` populated, but `resize --token-only` re-touches the token and
    /// succeeds non-interactively w.r.t. any passphrase (Task 0 spike, see
    /// this story's Dev Notes).
    fn resize(&self, mapper: &MapperHandle) -> Result<(), DomainError>;

    /// Reads the `filesystem` field off `path`'s `systemd-fido2` token,
    /// written once by `create`'s `write_fido2_token_metadata` (Story 3.2,
    /// AC #5) — never re-asked of the user or sniffed via `blkid`. A pure
    /// header/token read; does not require the mapping to be open.
    fn read_filesystem(&self, path: &Path) -> Result<Filesystem, DomainError>;
}
