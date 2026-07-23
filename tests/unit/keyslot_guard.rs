use std::path::Path;

use tomb_fido2::domain::errors::DomainError;
use tomb_fido2::domain::keyslot_guard::remove_keyslot_guarded;
use tomb_fido2::domain::types::{KeyslotInfo, KeyslotRef};

use crate::fakes::{new_call_log, FakeLuksBackend};

#[test]
fn aborts_when_only_one_valid_keyslot_remains() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_keyslots(vec![KeyslotInfo {
            keyslot: KeyslotRef(0),
        }]);

    let result = remove_keyslot_guarded(&luks, Path::new("/tmp/tomb"), KeyslotRef(0));

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
            },
            KeyslotInfo {
                keyslot: KeyslotRef(1),
            },
        ]);

    let result = remove_keyslot_guarded(&luks, Path::new("/tmp/tomb"), KeyslotRef(0));

    assert!(result.is_ok());
    assert_eq!(
        *log.borrow(),
        vec!["list_fido2_keyslots".to_string(), "remove_key".to_string()]
    );
}
