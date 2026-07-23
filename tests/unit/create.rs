use std::path::PathBuf;

use tomb_fido2::domain::errors::DomainError;
use tomb_fido2::domain::types::{CreateTarget, Filesystem};
use tomb_fido2::domain::workflows::create;

use crate::fakes::{new_call_log, FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend};

#[test]
fn refuses_before_touching_anything_if_destination_already_exists() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing()
        .with_path_exists(true)
        .with_log(log.clone());

    let target = CreateTarget::File {
        path: PathBuf::from("/tmp/already-there"),
        size: 1024,
    };

    let result = create::run(target, Filesystem::Ext4, &luks, &fido2, &fs);

    match result {
        Err(DomainError::DestinationExists(path)) => {
            assert_eq!(path, PathBuf::from("/tmp/already-there"));
        }
        other => panic!("expected DomainError::DestinationExists, got {other:?}"),
    }

    // Only the existence check itself ran — no file allocated, no LUKS
    // formatting attempted (AC #2).
    assert_eq!(*log.borrow(), vec!["path_exists".to_string()]);
}

#[test]
fn happy_path_runs_every_port_call_once_in_order() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let target = CreateTarget::File {
        path: PathBuf::from("/tmp/new-tomb"),
        size: 1024,
    };

    let result = create::run(target, Filesystem::Ext4, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");

    // Enroll runs before mkfs, not after: the transient bootstrap passphrase
    // is the only credential available to authenticate the FIDO2 enrollment,
    // and it must be wiped before mkfs runs (AC #3/AD-3) — see create.rs's
    // own comment and the story's Completion Notes for why this differs
    // from Task 5's originally literal ordering.
    assert_eq!(
        *log.borrow(),
        vec![
            "path_exists".to_string(),
            "set_backing_file_size".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "mkfs".to_string(),
            "list_fido2_keyslots".to_string(),
            "remove_key".to_string(),
        ]
    );
}
