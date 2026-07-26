use std::path::PathBuf;

use tomb_fido2::domain::errors::DomainError;
use tomb_fido2::domain::mapping_name;
use tomb_fido2::domain::types::Filesystem;
use tomb_fido2::domain::workflows::resize;

use crate::fakes::{new_call_log, FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend};

/// A real, uniquely-named file to canonicalize (AD-12's `mapping_name` is a
/// pure `domain` helper, not behind a port — it needs something real on
/// disk even in these fake-port-backed tests). Standing in for a "device"
/// path in the device-backed tests below is fine: whether `resize` treats a
/// path as file- or device-backed is entirely decided by the fake's
/// `is_block_device` setting, not by what's actually at the path. Cleans
/// itself up on drop, including on test panic.
struct RealFixtureFile(PathBuf);

impl RealFixtureFile {
    fn create(unique_name: &str, contents: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!("tomb-fido2-unit-test-{unique_name}"));
        std::fs::write(&path, contents).expect("failed to create test fixture file");
        Self(path)
    }
}

impl Drop for RealFixtureFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn file_backed_happy_path_runs_every_port_call_once_in_order() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_is_block_device(false)
        .with_live_device_capacity(4096);

    let fixture = RealFixtureFile::create("resize-file-happy-path", &[0u8; 4096]);

    let result = resize::run(&fixture.0, 8192, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "is_block_device".to_string(),
            "open".to_string(),
            "device_capacity".to_string(),
            "set_backing_file_size".to_string(),
            "resize".to_string(),
            "read_filesystem".to_string(),
            "growfs".to_string(),
            "close".to_string(),
        ]
    );

    let expected_name = mapping_name::mapping_name(&fixture.0).unwrap();
    assert_eq!(luks.last_open().unwrap().1, expected_name);
    assert_eq!(luks.last_resize().unwrap().name, expected_name);
    assert_eq!(luks.last_close().unwrap().name, expected_name);
}

#[test]
fn growfs_receives_whatever_read_filesystem_reports() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_read_filesystem(Filesystem::Ext4);
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_is_block_device(false)
        .with_live_device_capacity(4096);

    let fixture = RealFixtureFile::create("resize-growfs-filesystem", &[0u8; 4096]);

    let result = resize::run(&fixture.0, 8192, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(fs.last_growfs_filesystem(), Some(Filesystem::Ext4));
}

#[test]
fn device_backed_happy_path_never_calls_set_backing_file_size() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_is_block_device(true)
        .with_device_capacity(1024 * 1024 * 1024)
        .with_live_device_capacity(4096);

    let fixture = RealFixtureFile::create("resize-device-happy-path", &[0u8; 4096]);

    let result = resize::run(&fixture.0, 8192, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "is_block_device".to_string(),
            "device_capacity".to_string(),
            "open".to_string(),
            "device_capacity".to_string(),
            "resize".to_string(),
            "read_filesystem".to_string(),
            "growfs".to_string(),
            "close".to_string(),
        ]
    );
}

#[test]
fn file_backed_grow_only_rejection_never_calls_open() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    // Fixture is exactly 4096 bytes; requesting the same size must be
    // rejected as a no-op-or-shrink (grow-only, AC #3) before `luks.open`
    // is ever called — tier 1's whole point is a zero-adapter-call reject
    // for the common case.
    let fixture = RealFixtureFile::create("resize-grow-only-rejection", &[0u8; 4096]);

    let result = resize::run(&fixture.0, 4096, &luks, &fido2, &fs);

    let Err(DomainError::ResizeMustGrow {
        requested,
        current_size,
        ..
    }) = result
    else {
        panic!("expected ResizeMustGrow, got {result:?}");
    };
    assert_eq!(requested, 4096);
    assert_eq!(current_size, 4096);
    assert_eq!(
        *log.borrow(),
        vec!["is_block_device".to_string()],
        "grow-only rejection must not reach luks.open"
    );
}

#[test]
fn device_backed_too_small_partition_rejection_never_calls_open() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_is_block_device(true)
        .with_device_capacity(1024);

    let fixture = RealFixtureFile::create("resize-too-small-partition", &[0u8; 4096]);

    let result = resize::run(&fixture.0, 2048, &luks, &fido2, &fs);

    let Err(DomainError::DeviceSizeExceedsCapacity {
        requested,
        capacity,
        ..
    }) = result
    else {
        panic!("expected DeviceSizeExceedsCapacity, got {result:?}");
    };
    assert_eq!(requested, 2048);
    assert_eq!(capacity, 1024);
    assert_eq!(
        *log.borrow(),
        vec!["is_block_device".to_string(), "device_capacity".to_string()],
        "too-small-partition rejection must not reach luks.open"
    );
}

// Story 1.6's headroom feature means a device-backed tomb's raw capacity can
// be larger than its actual current provisioned size — tier 1 alone (which
// only compares against raw capacity) cannot catch a request that's smaller
// than the *live* current size, so tier 2 (checked after `open`, against
// the active mapping) must still reject it, and must close the mapping it
// just opened rather than leaking it (rollback discipline).
#[test]
fn device_backed_headroom_shrink_is_caught_by_tier_two_and_closes_the_mapping() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_is_block_device(true)
        .with_device_capacity(1024 * 1024 * 1024) // raw device is huge...
        .with_live_device_capacity(4096); // ...but this tomb only uses 4096 bytes of it.

    let fixture = RealFixtureFile::create("resize-headroom-shrink", &[0u8; 4096]);

    // Passes tier 1 (well within raw capacity) but does not actually grow
    // the tomb's live current size of 4096.
    let result = resize::run(&fixture.0, 4096, &luks, &fido2, &fs);

    let Err(DomainError::ResizeMustGrow {
        requested,
        current_size,
        ..
    }) = result
    else {
        panic!("expected ResizeMustGrow, got {result:?}");
    };
    assert_eq!(requested, 4096);
    assert_eq!(current_size, 4096);
    assert_eq!(
        *log.borrow(),
        vec![
            "is_block_device".to_string(),
            "device_capacity".to_string(),
            "open".to_string(),
            "device_capacity".to_string(),
            "close".to_string(),
        ],
        "tier 2's rejection must still close the mapping tier 2 itself just opened"
    );
}

#[test]
fn mid_flow_failure_after_a_successful_resize_still_closes_the_mapping() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_is_block_device(false)
        .with_live_device_capacity(4096)
        .with_failure_at("growfs");

    let fixture = RealFixtureFile::create("resize-mid-flow-failure", &[0u8; 4096]);

    let result = resize::run(&fixture.0, 8192, &luks, &fido2, &fs);

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "is_block_device".to_string(),
            "open".to_string(),
            "device_capacity".to_string(),
            "set_backing_file_size".to_string(),
            "resize".to_string(),
            "read_filesystem".to_string(),
            "growfs".to_string(),
            "close".to_string(),
        ],
        "a failure after a successful resize must still close the mapping"
    );
}
