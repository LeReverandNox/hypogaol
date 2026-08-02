use std::path::Path;

use hypogaol::domain::errors::DomainError;
use hypogaol::domain::keyslot_guard::remove_keyslot_guarded;
use hypogaol::domain::types::{KeyslotInfo, KeyslotRef};

use crate::fakes::{new_call_log, FakeLuksBackend};

#[test]
fn aborts_when_only_one_valid_keyslot_remains() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_keyslots(vec![KeyslotInfo {
            keyslot: KeyslotRef(0),
            key_label: "primary".to_string(),
        }]);

    let result = remove_keyslot_guarded(&luks, Path::new("/tmp/volume"), KeyslotRef(0));

    assert!(matches!(result, Err(DomainError::LastKeyslotGuard)));
    // The count check ran, but remove_key must never have been called.
    assert_eq!(*log.borrow(), vec!["list_fido2_keyslots".to_string()]);
}

#[test]
fn proceeds_when_more_than_one_valid_keyslot_remains() {
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

    let result = remove_keyslot_guarded(&luks, Path::new("/tmp/volume"), KeyslotRef(0));

    assert!(result.is_ok());
    assert_eq!(
        *log.borrow(),
        vec!["list_fido2_keyslots".to_string(), "remove_key".to_string()]
    );
}

// Regression test for a real-hardware finding: the transient bootstrap
// passphrase keyslot never gets a systemd-fido2 token, so it's never among
// list_fido2_keyslots' results. Removing it must proceed even when it's the
// *only* other keyslot besides the sole valid (FIDO2) one, since it was
// never counted as valid to begin with — a blind `count <= 1` check
// (ignoring whether target is itself valid) would wrongly block this.
#[test]
fn proceeds_when_target_is_not_itself_a_valid_keyslot_even_if_only_one_valid_keyslot_exists() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_keyslots(vec![KeyslotInfo {
            keyslot: KeyslotRef(1),
            key_label: "primary".to_string(),
        }]);

    // Target is keyslot 0 (the transient one) — absent from the valid list.
    let result = remove_keyslot_guarded(&luks, Path::new("/tmp/volume"), KeyslotRef(0));

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec!["list_fido2_keyslots".to_string(), "remove_key".to_string()]
    );
}
