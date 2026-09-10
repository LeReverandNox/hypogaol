use std::path::PathBuf;

use hypogaol::domain::errors::DomainError;
use hypogaol::domain::workflows::enroll;
use hypogaol::ports::fido2_backend::Fido2DeviceSelection;

use crate::fakes::{new_call_log, FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend};

/// `mapping_name` (AD-12) canonicalizes its input directly via `std::fs`, for
/// real, even in these fake-port-backed tests (it is a pure `domain` helper,
/// not something behind a port) — mirrors `tests/unit/create.rs`'s own
/// `RealFixtureFile` fixture. Cleans itself up on drop, including on test
/// panic.
struct RealFixtureFile(PathBuf);

impl RealFixtureFile {
    fn create(unique_name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("hypogaol-unit-test-enroll-{unique_name}"));
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
        &PathBuf::from("/tmp/hypogaol-unit-test-enroll-does-not-exist"),
        "backup".to_string(),
        Fido2DeviceSelection::Interactive,
        false,
        None,
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

    assert_eq!(
        *log.borrow(),
        vec!["check_prerequisites".to_string()],
        "no port call beyond preflight's own check_prerequisites should run before preflight fails"
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
        false,
        None,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "lock_target".to_string(),
            "enroll_fido2_key".to_string()
        ]
    );
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
        false,
        None,
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

#[test]
fn enroll_with_user_verification_true_passes_it_to_enroll_fido2_key() {
    let fido2 = FakeFido2Backend::passing();
    let luks = FakeLuksBackend::passing();
    let fs = FakeFilesystemBackend::passing();

    let fixture = RealFixtureFile::create("user-verification-true");

    let result = enroll::run(
        &fixture.0,
        "backup".to_string(),
        Fido2DeviceSelection::Interactive,
        true,
        None,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(fido2.user_verification_received(), Some(true));
}

#[test]
fn enroll_without_the_flag_passes_false_unchanged_from_epic_2() {
    let fido2 = FakeFido2Backend::passing();
    let luks = FakeLuksBackend::passing();
    let fs = FakeFilesystemBackend::passing();

    let fixture = RealFixtureFile::create("user-verification-false");

    let result = enroll::run(
        &fixture.0,
        "backup".to_string(),
        Fido2DeviceSelection::Interactive,
        false,
        None,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(fido2.user_verification_received(), Some(false));
}

#[test]
fn enroll_with_client_pin_false_passes_it_to_enroll_fido2_key() {
    let fido2 = FakeFido2Backend::passing();
    let luks = FakeLuksBackend::passing();
    let fs = FakeFilesystemBackend::passing();

    let fixture = RealFixtureFile::create("client-pin-false");

    let result = enroll::run(
        &fixture.0,
        "backup".to_string(),
        Fido2DeviceSelection::Interactive,
        false,
        Some(false),
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(fido2.client_pin_received(), Some(Some(false)));
}

#[test]
fn enroll_with_client_pin_true_passes_it_to_enroll_fido2_key() {
    let fido2 = FakeFido2Backend::passing();
    let luks = FakeLuksBackend::passing();
    let fs = FakeFilesystemBackend::passing();

    let fixture = RealFixtureFile::create("client-pin-true");

    let result = enroll::run(
        &fixture.0,
        "backup".to_string(),
        Fido2DeviceSelection::Interactive,
        false,
        Some(true),
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(fido2.client_pin_received(), Some(Some(true)));
}

#[test]
fn enroll_without_the_client_pin_flag_passes_none_unchanged() {
    let fido2 = FakeFido2Backend::passing();
    let luks = FakeLuksBackend::passing();
    let fs = FakeFilesystemBackend::passing();

    let fixture = RealFixtureFile::create("client-pin-not-passed");

    let result = enroll::run(
        &fixture.0,
        "backup".to_string(),
        Fido2DeviceSelection::Interactive,
        false,
        None,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(fido2.client_pin_received(), Some(None));
}

#[test]
fn locks_the_target_path_as_the_second_statement_after_preflight() {
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let fixture = RealFixtureFile::create("lock-target-happy-path");

    let result = enroll::run(
        &fixture.0,
        "backup".to_string(),
        Fido2DeviceSelection::Interactive,
        false,
        None,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(fs.lock_target_calls(), vec![fixture.0.clone()]);
}

#[test]
fn lock_contention_aborts_before_enroll_fido2_key_is_called() {
    let fido2 = FakeFido2Backend::passing();
    let luks = FakeLuksBackend::passing();
    let fs = FakeFilesystemBackend::passing().with_lock_contention();

    let fixture = RealFixtureFile::create("lock-contention");

    let result = enroll::run(
        &fixture.0,
        "backup".to_string(),
        Fido2DeviceSelection::Interactive,
        false,
        None,
        &luks,
        &fido2,
        &fs,
    );

    assert!(matches!(result, Err(DomainError::LockContention(_))));
    assert_eq!(fido2.user_verification_received(), None);
}
