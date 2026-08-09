use std::path::Path;

use hypogaol::domain::errors::DomainError;
use hypogaol::domain::types::{KeyslotInfo, KeyslotRef};
use hypogaol::domain::workflows::info;

use crate::fakes::{new_call_log, FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend};

#[test]
fn preflight_failure_short_circuits_before_any_port_call() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 =
        FakeFido2Backend::failing(&["fido2-token binary not found on PATH"]).with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let result = info::run(Path::new("/tmp/volume"), &luks, &fido2, &fs);

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
fn happy_path_returns_every_enrolled_keyslots_label() {
    let log = new_call_log();
    let keyslots = vec![
        KeyslotInfo {
            keyslot: KeyslotRef(0),
            key_label: "primary".to_string(),
        },
        KeyslotInfo {
            keyslot: KeyslotRef(1),
            key_label: "backup".to_string(),
        },
    ];
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_keyslots(keyslots.clone());
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let result = info::run(Path::new("/tmp/volume"), &luks, &fido2, &fs);

    match result {
        Ok(returned) => assert_eq!(returned, keyslots),
        other => panic!("expected Ok(keyslots), got {other:?}"),
    }
    assert_eq!(*log.borrow(), vec!["list_fido2_keyslots".to_string()]);
}

#[test]
fn empty_volume_returns_an_empty_list() {
    let luks = FakeLuksBackend::passing().with_keyslots(vec![]);
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let result = info::run(Path::new("/tmp/volume"), &luks, &fido2, &fs);

    match result {
        Ok(returned) => assert_eq!(returned, Vec::<KeyslotInfo>::new()),
        other => panic!("expected Ok(vec![]), got {other:?}"),
    }
}
