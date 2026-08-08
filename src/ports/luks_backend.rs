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

    /// `true` only for a path with a valid LUKS2 header carrying the marker;
    /// `false` for no header, an unreadable/invalid header, or a valid
    /// header without the marker (AD-9, CAP-23) — a single self-contained
    /// check, safe to call on any path regardless of what's already been
    /// verified about it.
    fn has_marker_token(&self, path: &Path) -> Result<bool, DomainError>;

    /// Removes the marker token written by `bootstrap_format_and_open`.
    /// Called only after a fully successful create, before the bootstrap
    /// keyslot is removed (AD-9's safe-ordering requirement, CAP-23 AC #4).
    fn remove_marker_token(&self, path: &Path) -> Result<(), DomainError>;

    /// Closes any dm-crypt mapping already active under `name`, tolerating
    /// "no such mapping" as `Ok(())` — the expected common case for a fresh
    /// create (never touched before) or a prior attempt that already closed
    /// cleanly. Exists for CAP-23: a real process-death crash between a
    /// successful `luksOpen` and `bootstrap_and_provision`'s own cleanup
    /// leaves exactly this kind of orphaned mapping under the deterministic
    /// name a resume attempt recomputes (`mapping_name` is a pure hash of
    /// the canonicalized path), blocking `bootstrap_format_and_open`'s
    /// `luksFormat` call before the marker-verified resume logic ever gets a
    /// chance to run. Any failure other than "doesn't exist" (e.g. the
    /// mapping exists but is still busy/mounted) propagates — this must
    /// never force through a mapping that's still legitimately in active
    /// use. Called unconditionally at the start of `bootstrap_and_provision`
    /// for both fresh and marker-verified-resume creates — safe either way,
    /// since a stale mapping under this exact name can only exist if this
    /// tool itself already got as far as `luksOpen` on this same path.
    fn close_stale_mapping(&self, name: &str) -> Result<(), DomainError>;

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

    /// Releases `mapper`'s dm-crypt mapping, leaving the volume closed/at rest.
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

    /// AD-17's live-discovery method for `close-all`/`slam` — enumerates
    /// every currently open dm-crypt mapping carrying this tool's fixed
    /// mapping-name prefix, never a stored registry. Each returned
    /// `MapperHandle`'s `source_path` is recovered from `cryptsetup
    /// status`'s `loop:` line when present — the real backing file for a
    /// file-backed mapping, since `device:` there is only the opaque
    /// `/dev/loopN` node cryptsetup opened internally (confirmed against
    /// real hardware, 2026-07-28) — falling back to `device:` for a
    /// device-backed mapping, which has no loop device and so no `loop:`
    /// line at all, leaving `device:` holding the correct raw
    /// device/partition path directly.
    fn list_open_mappings(&self) -> Result<Vec<MapperHandle>, DomainError>;
}
