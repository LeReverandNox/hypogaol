use std::path::PathBuf;

use hypogaol::domain::errors::DomainError;
use hypogaol::domain::mapping_name;
use hypogaol::domain::types::{CreateTarget, Filesystem};
use hypogaol::domain::workflows::create::{self, MIN_VOLUME_SIZE_BYTES};
use hypogaol::ports::fido2_backend::Fido2DeviceSelection;

use crate::fakes::{
    new_call_log, no_progress, FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend,
};

/// `mapping_name` (AD-12) canonicalizes its input directly via `std::fs`, for
/// real, even in these fake-port-backed tests (it is a pure `domain` helper,
/// not something behind a port). Give it a real, uniquely-named file to
/// canonicalize rather than a path that only exists in the fakes' internal
/// bookkeeping — `FakeFilesystemBackend::set_backing_file_size` never
/// actually touches disk. Cleans itself up on drop, including on test panic.
struct RealFixtureFile(PathBuf);

impl RealFixtureFile {
    fn create(unique_name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("hypogaol-unit-test-{unique_name}"));
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
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
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

    // The existence check ran, then has_marker_token (CAP-23's
    // resume check, defaulting to false here) confirmed this is a genuine
    // pre-existing destination — no file allocated, no LUKS formatting
    // attempted (AC #2/#3).
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "path_exists".to_string(),
            "has_marker_token".to_string()
        ]
    );
}

#[test]
fn file_backed_resume_proceeds_through_the_full_happy_path_with_no_confirmation_involved() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_has_marker_token(true);
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_path_exists(true)
        .with_log(log.clone());

    let fixture = RealFixtureFile::create("file-resume-happy-path");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");

    // A marker-verified destination falls through exactly as if it hadn't
    // existed (AC #1): the same happy-path sequence runs, just with
    // has_marker_token inserted where the refusal would otherwise have
    // returned.
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "path_exists".to_string(),
            "has_marker_token".to_string(),
            "set_backing_file_size".to_string(),
            "close_stale_mapping".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "mkfs".to_string(),
            "remove_marker_token".to_string(),
            "list_fido2_keyslots".to_string(),
            "remove_key".to_string(),
            "close".to_string(),
        ]
    );
}

#[test]
fn refuses_a_file_backed_size_below_the_minimum_before_touching_any_port() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let target = CreateTarget::File {
        path: PathBuf::from("/tmp/way-too-small"),
        size: MIN_VOLUME_SIZE_BYTES - 1,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    match result {
        Err(DomainError::DeviceTooSmall { path, size }) => {
            assert_eq!(path, PathBuf::from("/tmp/way-too-small"));
            assert_eq!(size, MIN_VOLUME_SIZE_BYTES - 1);
        }
        other => panic!("expected DomainError::DeviceTooSmall, got {other:?}"),
    }

    // The CLI's own `parse_size` already floor-checks this, but domain must
    // not rely on it as the only gate (mirroring the Device branch) — no
    // backing file allocated, no adapter touched.
    assert_eq!(
        *log.borrow(),
        vec!["check_prerequisites".to_string(), "path_exists".to_string()]
    );
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
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
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
            "check_prerequisites".to_string(),
            "path_exists".to_string(),
            "set_backing_file_size".to_string(),
            "close_stale_mapping".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "mkfs".to_string(),
            "remove_marker_token".to_string(),
            "list_fido2_keyslots".to_string(),
            "remove_key".to_string(),
            "close".to_string(),
        ]
    );
}

#[test]
fn create_with_user_verification_true_threads_it_to_bootstrap_enrollment() {
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let fixture = RealFixtureFile::create("user-verification-true");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        true,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(fido2.user_verification_received(), Some(true));
}

#[test]
fn create_device_with_user_verification_true_threads_it_to_bootstrap_enrollment() {
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing().with_device_capacity(MIN_VOLUME_SIZE_BYTES * 2);

    let fixture = RealFixtureFile::create("device-user-verification-true");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: None,
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        true,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(fido2.user_verification_received(), Some(true));
}

#[test]
fn create_with_label_threads_it_into_the_enrolled_key_metadata() {
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let fixture = RealFixtureFile::create("with-label");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Some("backup".to_string()),
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(fido2.key_label_received(), Some("backup".to_string()));
}

#[test]
fn create_without_label_falls_back_to_the_default_label() {
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let fixture = RealFixtureFile::create("without-label");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(fido2.key_label_received(), Some("primary".to_string()));
}

#[test]
fn create_device_with_label_threads_it_into_the_enrolled_key_metadata() {
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing().with_device_capacity(MIN_VOLUME_SIZE_BYTES * 2);

    let fixture = RealFixtureFile::create("device-with-label");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: None,
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Some("backup".to_string()),
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(fido2.key_label_received(), Some("backup".to_string()));
}

#[test]
fn create_device_without_label_falls_back_to_the_default_label() {
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing().with_device_capacity(MIN_VOLUME_SIZE_BYTES * 2);

    let fixture = RealFixtureFile::create("device-without-label");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: None,
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(fido2.key_label_received(), Some("primary".to_string()));
}

#[test]
fn create_with_an_unusual_label_threads_it_through_unmodified() {
    // Every other label test here uses a trivial single-word ASCII string
    // ("backup"); this one uses spaces, punctuation, and non-ASCII
    // characters to prove `create::run`'s plumbing does a true passthrough
    // with no trimming/truncation/escaping anywhere between the CLI-facing
    // `Option<String>` and `enroll_fido2_key`'s `metadata` argument (Review
    // Finding, 6-2 review). The full create -> LUKS2 token -> info/revoke
    // round-trip (AC #3) stays hardware-verified only: the fakes used here
    // don't model token persistence, so a fake-backed test can only prove
    // domain-level threading, not the adapter's on-disk encode/decode.
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let fixture = RealFixtureFile::create("with-unusual-label");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_VOLUME_SIZE_BYTES,
    };
    let label = "Renée's café key #2 🔑".to_string();

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Some(label.clone()),
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(fido2.key_label_received(), Some(label));
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
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
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
            "check_prerequisites".to_string(),
            "path_exists".to_string(),
            "set_backing_file_size".to_string(),
            "close_stale_mapping".to_string(),
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
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "path_exists".to_string(),
            "set_backing_file_size".to_string(),
            "close_stale_mapping".to_string(),
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
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
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
            "check_prerequisites".to_string(),
            "path_exists".to_string(),
            "set_backing_file_size".to_string(),
            "close_stale_mapping".to_string(),
            "bootstrap_format_and_open".to_string(),
            "remove_backing_file".to_string(),
        ]
    );
}

#[test]
fn close_stale_mapping_failure_aborts_before_bootstrap_format_and_open_ever_runs() {
    // A stale mapping that's still busy/mounted (e.g. for an unrelated
    // reason under the same deterministic name) must never be forced
    // through — create aborts instead of proceeding to reformat, and never
    // even reaches bootstrap_format_and_open (review finding, 2026-08-08).
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_failure_at("close_stale_mapping");
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("close-stale-mapping-failure");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_err(), "expected Err, got {result:?}");
    // bootstrap_format_and_open never runs — matches the existing
    // File-branch convention that any bootstrap_and_provision error after
    // set_backing_file_size triggers remove_backing_file, same as the
    // bootstrap_format_and_open-itself-fails case above.
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "path_exists".to_string(),
            "set_backing_file_size".to_string(),
            "close_stale_mapping".to_string(),
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
        .with_device_capacity(MIN_VOLUME_SIZE_BYTES * 2);

    let fixture = RealFixtureFile::create("device-happy-path-no-size");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: None,
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(luks.last_bootstrap_size(), Some(MIN_VOLUME_SIZE_BYTES * 2));
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "has_luks2_header".to_string(),
            "device_capacity".to_string(),
            "close_stale_mapping".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "mkfs".to_string(),
            "remove_marker_token".to_string(),
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
        .with_device_capacity(MIN_VOLUME_SIZE_BYTES * 2);

    let fixture = RealFixtureFile::create("device-happy-path-smaller-size");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: Some(MIN_VOLUME_SIZE_BYTES),
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    // The requested (smaller) size must reach bootstrap_format_and_open
    // unchanged, not the full capacity (AC #2).
    assert_eq!(luks.last_bootstrap_size(), Some(MIN_VOLUME_SIZE_BYTES));
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "has_luks2_header".to_string(),
            "device_capacity".to_string(),
            "close_stale_mapping".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "mkfs".to_string(),
            "remove_marker_token".to_string(),
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
        .with_device_capacity(MIN_VOLUME_SIZE_BYTES - 1);

    let fixture = RealFixtureFile::create("device-capacity-below-minimum");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: None,
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    match result {
        Err(DomainError::DeviceTooSmall { path, size }) => {
            assert_eq!(path, fixture.0);
            assert_eq!(size, MIN_VOLUME_SIZE_BYTES - 1);
        }
        other => panic!("expected DomainError::DeviceTooSmall, got {other:?}"),
    }

    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
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
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
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

    // Header check runs first, then has_marker_token (defaulting to false
    // here) confirms this is a genuine pre-existing header, not a
    // marker-verified resume — nothing else is ever called, even though
    // `confirmed` was true (AC #4: not bypassable by confirming).
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "has_luks2_header".to_string(),
            "has_marker_token".to_string()
        ]
    );
}

#[test]
fn device_backed_resume_proceeds_even_when_not_confirmed() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_has_luks2_header(true)
        .with_has_marker_token(true);
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_device_capacity(MIN_VOLUME_SIZE_BYTES * 2);

    let fixture = RealFixtureFile::create("device-resume-not-confirmed");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: None,
        confirmed: false,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");

    // confirmed: false never triggers DeviceConfirmationRequired — a
    // marker-verified resume skips that check entirely (AC #2), proving
    // confirmation is genuinely skipped, not just defaulted.
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "has_luks2_header".to_string(),
            "has_marker_token".to_string(),
            "device_capacity".to_string(),
            "close_stale_mapping".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "mkfs".to_string(),
            "remove_marker_token".to_string(),
            "list_fido2_keyslots".to_string(),
            "remove_key".to_string(),
            "close".to_string(),
        ]
    );
}

#[test]
fn device_backed_resume_still_enforces_size_against_capacity() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_has_luks2_header(true)
        .with_has_marker_token(true);
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_device_capacity(1024);

    let fixture = RealFixtureFile::create("device-resume-size-exceeds-capacity");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: Some(2048),
        confirmed: false,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
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

    // Confirmation was skipped entirely (marker-verified resume, AC #2),
    // but size resolution against device_capacity still ran unconditionally
    // — a device shrunk since the crashed attempt is still caught.
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "has_luks2_header".to_string(),
            "has_marker_token".to_string(),
            "device_capacity".to_string()
        ]
    );
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
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
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
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "has_luks2_header".to_string()
        ]
    );
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
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
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
            "check_prerequisites".to_string(),
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
        .with_device_capacity(MIN_VOLUME_SIZE_BYTES);

    let fixture = RealFixtureFile::create("device-failure-path");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: None,
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
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
            "check_prerequisites".to_string(),
            "has_luks2_header".to_string(),
            "device_capacity".to_string(),
            "close_stale_mapping".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "close".to_string(),
        ]
    );
}

#[test]
fn create_with_scaffold_hooks_true_mounts_writes_templates_and_unmounts_after_mkfs() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("scaffold-hooks-true-file");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        true,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");

    // AD-9's real execution order: scaffolding lands strictly after mkfs and
    // strictly before final marker/keyslot cleanup (AC #3).
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "path_exists".to_string(),
            "set_backing_file_size".to_string(),
            "close_stale_mapping".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "mkfs".to_string(),
            "mount".to_string(),
            "scaffold_hook_templates".to_string(),
            "umount".to_string(),
            "remove_marker_token".to_string(),
            "list_fido2_keyslots".to_string(),
            "remove_key".to_string(),
            "close".to_string(),
        ]
    );

    // The exact mountpoint `mount` returned is what gets threaded into
    // `scaffold_hook_templates` — not a stand-in.
    let expected_name = mapping_name::mapping_name(&fixture.0).unwrap();
    assert_eq!(
        fs.last_scaffold_hook_templates_mountpoint(),
        Some(PathBuf::from(format!("/tmp/fake-mount-{expected_name}")))
    );
}

#[test]
fn create_with_scaffold_hooks_false_never_mounts_for_scaffolding() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("scaffold-hooks-false-file");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "path_exists".to_string(),
            "set_backing_file_size".to_string(),
            "close_stale_mapping".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "mkfs".to_string(),
            "remove_marker_token".to_string(),
            "list_fido2_keyslots".to_string(),
            "remove_key".to_string(),
            "close".to_string(),
        ],
        "with scaffold_hooks false, the write path must never even be reached"
    );
    assert_eq!(fs.last_scaffold_hook_templates_mountpoint(), None);
}

#[test]
fn create_device_with_scaffold_hooks_true_mounts_writes_templates_and_unmounts_after_mkfs() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_device_capacity(MIN_VOLUME_SIZE_BYTES * 2);

    let fixture = RealFixtureFile::create("scaffold-hooks-true-device");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: None,
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        true,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "has_luks2_header".to_string(),
            "device_capacity".to_string(),
            "close_stale_mapping".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "mkfs".to_string(),
            "mount".to_string(),
            "scaffold_hook_templates".to_string(),
            "umount".to_string(),
            "remove_marker_token".to_string(),
            "list_fido2_keyslots".to_string(),
            "remove_key".to_string(),
            "close".to_string(),
        ]
    );

    let expected_name = mapping_name::mapping_name(&fixture.0).unwrap();
    assert_eq!(
        fs.last_scaffold_hook_templates_mountpoint(),
        Some(PathBuf::from(format!("/tmp/fake-mount-{expected_name}")))
    );
}

#[test]
fn create_device_with_scaffold_hooks_false_never_mounts_for_scaffolding() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_device_capacity(MIN_VOLUME_SIZE_BYTES * 2);

    let fixture = RealFixtureFile::create("scaffold-hooks-false-device");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: None,
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "has_luks2_header".to_string(),
            "device_capacity".to_string(),
            "close_stale_mapping".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "mkfs".to_string(),
            "remove_marker_token".to_string(),
            "list_fido2_keyslots".to_string(),
            "remove_key".to_string(),
            "close".to_string(),
        ],
        "with scaffold_hooks false, the write path must never even be reached"
    );
    assert_eq!(fs.last_scaffold_hook_templates_mountpoint(), None);
}

#[test]
fn create_scaffold_hook_templates_failure_still_unmounts_before_returning_the_error() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_failure_at("scaffold_hook_templates");

    let fixture = RealFixtureFile::create("scaffold-hooks-scaffold-failure");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        true,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "path_exists".to_string(),
            "set_backing_file_size".to_string(),
            "close_stale_mapping".to_string(),
            "bootstrap_format_and_open".to_string(),
            "enroll_fido2_key".to_string(),
            "mkfs".to_string(),
            "mount".to_string(),
            "scaffold_hook_templates".to_string(),
            "umount".to_string(),
            "close".to_string(),
            "remove_backing_file".to_string(),
        ],
        "a scaffold_hook_templates failure must still unmount before returning the error, or \
         the outer luks.close misreports device-busy instead of the real failure"
    );
}

#[test]
fn create_scaffold_hooks_umount_failure_after_a_scaffold_failure_wraps_as_rollback_cleanup_also_failed(
) {
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();

    let fixture = RealFixtureFile::create("scaffold-hooks-scaffold-and-umount-failure");
    let expected_name = mapping_name::mapping_name(&fixture.0).unwrap();
    let fs = FakeFilesystemBackend::passing()
        .with_failure_at("scaffold_hook_templates")
        .with_umount_failure_for(&expected_name);

    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        true,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(
        matches!(result, Err(DomainError::RollbackCleanupAlsoFailed { .. })),
        "expected Err(DomainError::RollbackCleanupAlsoFailed), got {result:?}"
    );
}

#[test]
fn create_scaffold_hooks_umount_failure_alone_surfaces_the_bare_umount_error() {
    // The third arm of finish_provisioning's `match fs.umount(mapper)`:
    // scaffold_hook_templates succeeds but umount fails alone. Distinct from
    // both "scaffold fails, umount succeeds" and "scaffold fails, umount
    // also fails" above — this is the one arm that returns umount_err
    // untouched, and had no direct test coverage (review finding,
    // 2026-08-09).
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();

    let fixture = RealFixtureFile::create("scaffold-hooks-umount-failure-alone");
    let expected_name = mapping_name::mapping_name(&fixture.0).unwrap();
    let fs = FakeFilesystemBackend::passing().with_umount_failure_for(&expected_name);

    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        true,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(
        matches!(result, Err(DomainError::AdapterFailure(_))),
        "expected the bare umount error to surface untouched, got {result:?}"
    );
    assert!(
        !matches!(result, Err(DomainError::RollbackCleanupAlsoFailed { .. })),
        "scaffold_hook_templates succeeded, so this must not be reported as a rollback-also-failed, \
         got {result:?}"
    );
}
