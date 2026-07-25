use std::path::PathBuf;

use tomb_fido2::domain::errors::DomainError;
use tomb_fido2::domain::workflows::enroll;
use tomb_fido2::ports::fido2_backend::Fido2DeviceSelection;

use crate::fakes::{new_call_log, FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend};

/// `mapping_name` (AD-12) canonicalizes its input directly via `std::fs`, for
/// real, even in these fake-port-backed tests (it is a pure `domain` helper,
/// not something behind a port) — mirrors `tests/unit/create.rs`'s own
/// `RealFixtureFile` fixture. Cleans itself up on drop, including on test
/// panic.
struct RealFixtureFile(PathBuf);

impl RealFixtureFile {
    fn create(unique_name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("tomb-fido2-unit-test-enroll-{unique_name}"));
        std::fs::write(&path, []).expect("failed to create test fixture file");
        Self(path)
    }
}

impl Drop for RealFixtureFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn preflight_failure_short_circuits_before_any_mutating_call() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 =
        FakeFido2Backend::failing(&["fido2-token binary not found on PATH"]).with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let result = enroll::run(
        &PathBuf::from("/tmp/tomb-fido2-unit-test-enroll-does-not-exist"),
        "backup".to_string(),
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

    match result {
        Err(DomainError::PreflightFailed(missing)) => {
            assert_eq!(
                missing,
                vec!["fido2-token binary not found on PATH".to_string()]
            );
        }
        other => panic!("expected DomainError::PreflightFailed, got {other:?}"),
    }

    assert!(
        log.borrow().is_empty(),
        "no port call should run before preflight fails"
    );
}

#[test]
fn happy_path_calls_enroll_fido2_key_exactly_once_with_the_given_key_label() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("happy-path");

    let result = enroll::run(
        &fixture.0,
        "backup".to_string(),
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(*log.borrow(), vec!["enroll_fido2_key".to_string()]);
}

#[test]
fn enroll_fido2_key_failure_propagates_as_adapter_failure_untouched() {
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing().with_failure_at("enroll_fido2_key");
    let fs = FakeFilesystemBackend::passing();

    let fixture = RealFixtureFile::create("enroll-failure");

    let result = enroll::run(
        &fixture.0,
        "backup".to_string(),
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

    match result {
        Err(DomainError::AdapterFailure(msg)) => {
            assert_eq!(msg, "enroll_fido2_key failed (test)");
        }
        other => panic!("expected DomainError::AdapterFailure, got {other:?}"),
    }
}
