use std::path::PathBuf;

use crate::domain::errors::DomainError;
use crate::domain::types::{KeyMetadata, MapperHandle};

/// How `enroll_fido2_key` should identify which physical FIDO2 device fills
/// which role. Resolved by picking a device's identity spatially (from a
/// single point-in-time enumeration) rather than temporally (diffing two
/// enumerations taken before/after a device is plugged in) — the latter is
/// what produced the enroll hang/"More than one FIDO device found" hardware
/// failure this design replaces.
#[derive(Debug, Clone)]
pub enum Fido2DeviceSelection {
    /// Enumerate currently-plugged-in devices and, if more than one is
    /// present (or an existing-key role must be filled), prompt the user to
    /// assign roles by index. A single device present when only a "new key"
    /// role is needed is auto-picked with no prompt.
    Interactive,

    /// Skip enumeration/prompting; `new` is the device to enroll,
    /// `existing` (when required) is the already-enrolled device to
    /// authenticate against. For unattended/scripted use.
    Explicit {
        new: PathBuf,
        existing: Option<PathBuf>,
    },
}

pub trait Fido2Backend {
    /// `Err` carries one human-readable string per missing/unsupported dependency
    /// this port's real adapter needs; `Ok(())` means all are satisfied.
    fn check_prerequisites(&self) -> Result<(), Vec<String>>;

    /// Enrolls the real FIDO2 key as a `systemd-fido2` token+keyslot, writing
    /// `metadata`'s fields onto that same token object (AD-2). `selection`
    /// identifies which physical device(s) fill the "new key" (and, when an
    /// existing enrolled key must authenticate the operation, "existing
    /// key") roles. `user_verification` maps to
    /// `--fido2-with-user-verification=yes|no` (AD-16): when `true`,
    /// unlocking with this key later requires the device's own
    /// fingerprint/PIN check, not touch alone. Implementations must also
    /// disable clientPin-based verification when `true` (e.g. real
    /// hardware's `--fido2-with-client-pin=false`) — otherwise a token that
    /// supports clientPin satisfies "uv" via a host-typed PIN prompt instead
    /// of its own on-device check, defeating the point of requesting it.
    fn enroll_fido2_key(
        &self,
        mapper: &MapperHandle,
        metadata: KeyMetadata,
        selection: Fido2DeviceSelection,
        user_verification: bool,
    ) -> Result<(), DomainError>;
}
