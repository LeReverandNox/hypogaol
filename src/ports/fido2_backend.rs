use crate::domain::errors::DomainError;
use crate::domain::types::{KeyMetadata, MapperHandle};

pub trait Fido2Backend {
    /// `Err` carries one human-readable string per missing/unsupported dependency
    /// this port's real adapter needs; `Ok(())` means all are satisfied.
    fn check_prerequisites(&self) -> Result<(), Vec<String>>;

    /// Enrolls the real FIDO2 key as a `systemd-fido2` token+keyslot, writing
    /// `metadata`'s fields onto that same token object (AD-2).
    fn enroll_fido2_key(
        &self,
        mapper: &MapperHandle,
        metadata: KeyMetadata,
    ) -> Result<(), DomainError>;
}
