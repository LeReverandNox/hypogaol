use std::path::PathBuf;

use tomb_fido2::domain::errors::DomainError;
use tomb_fido2::domain::types::{CreateTarget, Filesystem};
use tomb_fido2::domain::workflows::create::{self, MIN_TOMB_SIZE_BYTES};
use tomb_fido2::ports::fido2_backend::Fido2DeviceSelection;

use crate::fakes::{new_call_log, FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend};

/// `mapping_name` (AD-12) canonicalizes its input directly via `std::fs`, for
/// real, even in these fake-port-backed tests (it is a pure `domain` helper,
/// not something behind a port). Give it a real, uniquely-named file to
/// canonicalize rather than a path that only exists in the fakes' internal
/// bookkeeping — `FakeFilesystemBackend::set_backing_file_size` never
/// actually touches disk. Cleans itself up on drop, including on test panic.
struct RealFixtureFile(PathBuf);

impl RealFixtureFile {
    fn create(unique_name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("tomb-fido2-unit-test-{unique_name}"));
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
fn refuses_before_touching_anything_if_destination_already_exists() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing()
        .with_path_exists(true)
        .with_log(log.clone());

    let target = CreateTarget::File {
        path: PathBuf::from("/tmp/already-there"),
        size: MIN_TOMB_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

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
fn refuses_a_file_backed_size_below_the_minimum_before_touching_any_port() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let target = CreateTarget::File {
        path: PathBuf::from("/tmp/way-too-small"),
        size: MIN_TOMB_SIZE_BYTES - 1,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

    match result {
        Err(DomainError::DeviceTooSmall { path, size }) => {
            assert_eq!(path, PathBuf::from("/tmp/way-too-small"));
            assert_eq!(size, MIN_TOMB_SIZE_BYTES - 1);
        }
        other => panic!("expected DomainError::DeviceTooSmall, got {other:?}"),
    }

    // The CLI's own `parse_size` already floor-checks this, but domain must
    // not rely on it as the only gate (mirroring the Device branch) — no
    // backing file allocated, no adapter touched.
    assert_eq!(*log.borrow(), vec!["path_exists".to_string()]);
}

#[test]
fn happy_path_runs_every_port_call_once_in_order() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("happy-path");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_TOMB_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

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
            "close".to_string(),
        ]
    );
}

#[test]
fn enroll_failure_closes_the_mapping_and_removes_the_backing_file() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing()
        .with_log(log.clone())
        .with_failure_at("enroll_fido2_key");
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("enroll-failure");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_TOMB_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_err(), "expected Err, got {result:?}");

    // The mapping opened by bootstrap_format_and_open must still be closed,
    // and the backing file allocated by set_backing_file_size must still be
    // removed, even though enrollment is what actually failed — otherwise a
    // failed create leaks an open mapping and permanently blocks retrying at
    // the same destination (DestinationExists forever).
    assert_eq!(
        *log.borrow(),
        vec![
            "path_exists".to_string(),
            "set_backing_file_size".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "close".to_string(),
            "remove_backing_file".to_string(),
        ]
    );
}

#[test]
fn mkfs_failure_closes_the_mapping_and_removes_the_backing_file() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_failure_at("mkfs");

    let fixture = RealFixtureFile::create("mkfs-failure");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_TOMB_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "path_exists".to_string(),
            "set_backing_file_size".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "mkfs".to_string(),
            "close".to_string(),
            "remove_backing_file".to_string(),
        ]
    );
}

#[test]
fn bootstrap_format_and_open_failure_removes_the_backing_file_without_closing_a_mapping() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_failure_at("bootstrap_format_and_open");
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("bootstrap-failure");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_TOMB_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_err(), "expected Err, got {result:?}");

    // No mapping was ever opened, so there is nothing to close — but the
    // backing file `set_backing_file_size` already allocated must still be
    // removed.
    assert_eq!(
        *log.borrow(),
        vec![
            "path_exists".to_string(),
            "set_backing_file_size".to_string(),
            "bootstrap_format_and_open".to_string(),
            "remove_backing_file".to_string(),
        ]
    );
}

#[test]
fn device_happy_path_with_no_size_given_uses_the_full_capacity() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_device_capacity(MIN_TOMB_SIZE_BYTES * 2);

    let fixture = RealFixtureFile::create("device-happy-path-no-size");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: None,
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(luks.last_bootstrap_size(), Some(MIN_TOMB_SIZE_BYTES * 2));
    assert_eq!(
        *log.borrow(),
        vec![
            "has_luks2_header".to_string(),
            "device_capacity".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "mkfs".to_string(),
            "list_fido2_keyslots".to_string(),
            "remove_key".to_string(),
            "close".to_string(),
        ]
    );
}

#[test]
fn device_happy_path_with_a_size_smaller_than_capacity_uses_the_requested_size() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_device_capacity(MIN_TOMB_SIZE_BYTES * 2);

    let fixture = RealFixtureFile::create("device-happy-path-smaller-size");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: Some(MIN_TOMB_SIZE_BYTES),
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    // The requested (smaller) size must reach bootstrap_format_and_open
    // unchanged, not the full capacity (AC #2).
    assert_eq!(luks.last_bootstrap_size(), Some(MIN_TOMB_SIZE_BYTES));
    assert_eq!(
        *log.borrow(),
        vec![
            "has_luks2_header".to_string(),
            "device_capacity".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "mkfs".to_string(),
            "list_fido2_keyslots".to_string(),
            "remove_key".to_string(),
            "close".to_string(),
        ]
    );
}

#[test]
fn device_with_no_size_given_and_capacity_below_the_minimum_refuses_before_any_mutating_call() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_device_capacity(MIN_TOMB_SIZE_BYTES - 1);

    let fixture = RealFixtureFile::create("device-capacity-below-minimum");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: None,
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

    match result {
        Err(DomainError::DeviceTooSmall { path, size }) => {
            assert_eq!(path, fixture.0);
            assert_eq!(size, MIN_TOMB_SIZE_BYTES - 1);
        }
        other => panic!("expected DomainError::DeviceTooSmall, got {other:?}"),
    }

    assert_eq!(
        *log.borrow(),
        vec![
            "has_luks2_header".to_string(),
            "device_capacity".to_string()
        ]
    );
}

#[test]
fn device_with_existing_luks2_header_refuses_even_when_confirmed() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_has_luks2_header(true);
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("device-already-formatted");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: None,
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

    match result {
        Err(DomainError::DeviceAlreadyFormatted(path)) => {
            assert_eq!(path, fixture.0);
        }
        other => panic!("expected DomainError::DeviceAlreadyFormatted, got {other:?}"),
    }

    // Header check runs first and wins — nothing else is ever called, even
    // though `confirmed` was true (AC #4: not bypassable by confirming).
    assert_eq!(*log.borrow(), vec!["has_luks2_header".to_string()]);
}

#[test]
fn device_without_confirmation_refuses_even_with_no_header() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_has_luks2_header(false);
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("device-not-confirmed");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: None,
        confirmed: false,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

    match result {
        Err(DomainError::DeviceConfirmationRequired) => {}
        other => panic!("expected DomainError::DeviceConfirmationRequired, got {other:?}"),
    }

    // has_luks2_header still ran (AC #4's check always runs first), but
    // confirmation is checked before any sizing/mutating call (AC #5).
    assert_eq!(*log.borrow(), vec!["has_luks2_header".to_string()]);
}

#[test]
fn device_with_requested_size_greater_than_capacity_refuses_before_any_mutating_call() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_device_capacity(1024);

    let fixture = RealFixtureFile::create("device-size-exceeds-capacity");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: Some(2048),
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

    match result {
        Err(DomainError::DeviceSizeExceedsCapacity {
            path,
            requested,
            capacity,
        }) => {
            assert_eq!(path, fixture.0);
            assert_eq!(requested, 2048);
            assert_eq!(capacity, 1024);
        }
        other => panic!("expected DomainError::DeviceSizeExceedsCapacity, got {other:?}"),
    }

    assert_eq!(
        *log.borrow(),
        vec![
            "has_luks2_header".to_string(),
            "device_capacity".to_string()
        ]
    );
}

#[test]
fn device_branch_failure_closes_the_mapping_without_removing_any_backing_file() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing()
        .with_log(log.clone())
        .with_failure_at("enroll_fido2_key");
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_device_capacity(MIN_TOMB_SIZE_BYTES);

    let fixture = RealFixtureFile::create("device-failure-path");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: None,
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_err(), "expected Err, got {result:?}");

    // The mapping must still be closed on failure, but — unlike the File
    // branch — remove_backing_file must never be called: there is no
    // backing file to remove for a device/partition target.
    assert_eq!(
        *log.borrow(),
        vec![
            "has_luks2_header".to_string(),
            "device_capacity".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "close".to_string(),
        ]
    );
}
