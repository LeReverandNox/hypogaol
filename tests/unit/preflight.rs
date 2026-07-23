use tomb_fido2::domain::errors::DomainError;
use tomb_fido2::domain::preflight;

use crate::fakes::{FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend};

#[test]
fn all_ports_passing_returns_ok() {
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    assert!(preflight::check(&luks, &fido2, &fs).is_ok());
}

#[test]
fn one_missing_dependency_is_named_in_the_error() {
    let luks = FakeLuksBackend::failing(&["cryptsetup"]);
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let err = preflight::check(&luks, &fido2, &fs).unwrap_err();
    let DomainError::PreflightFailed(missing) = err else {
        panic!("expected DomainError::PreflightFailed, got {err:?}");
    };

    assert_eq!(missing, vec!["cryptsetup".to_string()]);
}

#[test]
fn failures_from_every_port_are_aggregated_not_short_circuited() {
    let luks = FakeLuksBackend::failing(&["cryptsetup"]);
    let fido2 = FakeFido2Backend::failing(&["fido2-token"]);
    let fs = FakeFilesystemBackend::failing(&["mkfs.ext4"]);

    let err = preflight::check(&luks, &fido2, &fs).unwrap_err();
    let DomainError::PreflightFailed(missing) = err else {
        panic!("expected DomainError::PreflightFailed, got {err:?}");
    };

    assert_eq!(
        missing,
        vec![
            "cryptsetup".to_string(),
            "fido2-token".to_string(),
            "mkfs.ext4".to_string(),
        ]
    );
}
