use std::path::Path;

use tomb_fido2::domain::errors::DomainError;
use tomb_fido2::domain::types::{KeyslotInfo, KeyslotRef};
use tomb_fido2::domain::workflows::revoke;

use crate::fakes::{new_call_log, FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend};

#[test]
fn preflight_failure_short_circuits_before_any_port_call() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 =
        FakeFido2Backend::failing(&["fido2-token binary not found on PATH"]).with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let result = revoke::run(Path::new("/tmp/tomb"), "primary", &luks, &fido2, &fs);

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
fn happy_path_removes_the_keyslot_matching_the_given_label() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_keyslots(vec![
            KeyslotInfo {
                keyslot: KeyslotRef(0),
                key_label: "primary".to_string(),
            },
            KeyslotInfo {
                keyslot: KeyslotRef(1),
                key_label: "backup".to_string(),
            },
        ]);
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let result = revoke::run(Path::new("/tmp/tomb"), "backup", &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(luks.last_removed_keyslot(), Some(KeyslotRef(1)));
    // list_fido2_keyslots runs twice: once here to resolve the label, once
    // more inside remove_keyslot_guarded to re-count live state immediately
    // before deciding (AC #3 — never a cached or prior view).
    assert_eq!(
        *log.borrow(),
        vec![
            "list_fido2_keyslots".to_string(),
            "list_fido2_keyslots".to_string(),
            "remove_key".to_string()
        ]
    );
}

#[test]
fn targeting_an_unenrolled_label_returns_key_not_found_and_never_removes_anything() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_keyslots(vec![KeyslotInfo {
            keyslot: KeyslotRef(0),
            key_label: "primary".to_string(),
        }]);
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let result = revoke::run(Path::new("/tmp/tomb"), "nonexistent", &luks, &fido2, &fs);

    match result {
        Err(DomainError::KeyNotFound(label)) => assert_eq!(label, "nonexistent"),
        other => panic!("expected DomainError::KeyNotFound, got {other:?}"),
    }

    assert_eq!(*log.borrow(), vec!["list_fido2_keyslots".to_string()]);
}

#[test]
fn revoking_the_sole_remaining_keyslot_returns_last_keyslot_guard() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_keyslots(vec![KeyslotInfo {
            keyslot: KeyslotRef(0),
            key_label: "primary".to_string(),
        }]);
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let result = revoke::run(Path::new("/tmp/tomb"), "primary", &luks, &fido2, &fs);

    assert!(matches!(result, Err(DomainError::LastKeyslotGuard)));
    // Same double list_fido2_keyslots call as the happy path (AC #3) —
    // remove_key must never be called either way.
    assert_eq!(
        *log.borrow(),
        vec![
            "list_fido2_keyslots".to_string(),
            "list_fido2_keyslots".to_string()
        ]
    );
}

#[test]
fn remove_key_failure_propagates_as_adapter_failure_untouched() {
    let luks = FakeLuksBackend::passing()
        .with_keyslots(vec![
            KeyslotInfo {
                keyslot: KeyslotRef(0),
                key_label: "primary".to_string(),
            },
            KeyslotInfo {
                keyslot: KeyslotRef(1),
                key_label: "backup".to_string(),
            },
        ])
        .with_failure_at("remove_key");
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let result = revoke::run(Path::new("/tmp/tomb"), "primary", &luks, &fido2, &fs);

    match result {
        Err(DomainError::AdapterFailure(msg)) => {
            assert_eq!(msg, "remove_key failed (test)");
        }
        other => panic!("expected DomainError::AdapterFailure, got {other:?}"),
    }
}
