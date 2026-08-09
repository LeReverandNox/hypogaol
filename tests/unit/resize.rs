use std::path::PathBuf;

use hypogaol::domain::errors::DomainError;
use hypogaol::domain::mapping_name;
use hypogaol::domain::types::Filesystem;
use hypogaol::domain::workflows::resize;

use crate::fakes::{
    new_call_log, no_progress, FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend,
};

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
        let path = std::env::temp_dir().join(format!("hypogaol-unit-test-{unique_name}"));
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
        .with_device_capacity(4096)
        .with_filesystem_size(4096);

    let fixture = RealFixtureFile::create("resize-file-happy-path", &[0u8; 4096]);

    let result = resize::run(&fixture.0, 8192, &no_progress, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "is_block_device".to_string(),
            "read_filesystem".to_string(),
            "check_prerequisites".to_string(),
            "open".to_string(),
            "device_capacity".to_string(),
            "filesystem_size".to_string(),
            "set_backing_file_size".to_string(),
            "resize".to_string(),
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
        .with_device_capacity(4096)
        .with_filesystem_size(4096);

    let fixture = RealFixtureFile::create("resize-growfs-filesystem", &[0u8; 4096]);

    let result = resize::run(&fixture.0, 8192, &no_progress, &luks, &fido2, &fs);

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
        .with_filesystem_size(4096);

    let fixture = RealFixtureFile::create("resize-device-happy-path", &[0u8; 4096]);

    let result = resize::run(&fixture.0, 8192, &no_progress, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "is_block_device".to_string(),
            "device_capacity".to_string(),
            "read_filesystem".to_string(),
            "check_prerequisites".to_string(),
            "open".to_string(),
            "device_capacity".to_string(),
            "filesystem_size".to_string(),
            "resize".to_string(),
            "growfs".to_string(),
            "close".to_string(),
        ]
    );
}

#[test]
fn file_backed_true_shrink_is_rejected_by_tier_one_before_any_adapter_call() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    // Fixture is 4096 bytes; requesting a strictly smaller size is an
    // unambiguous shrink — tier 1 must reject it with zero adapter calls
    // (AC #3), never reaching `read_filesystem` or `luks.open`.
    let fixture = RealFixtureFile::create("resize-true-shrink", &[0u8; 4096]);

    let result = resize::run(&fixture.0, 2048, &no_progress, &luks, &fido2, &fs);

    let Err(DomainError::ResizeMustGrow {
        requested,
        current_size,
        ..
    }) = result
    else {
        panic!("expected ResizeMustGrow, got {result:?}");
    };
    assert_eq!(requested, 2048);
    assert_eq!(current_size, 4096);
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "is_block_device".to_string()
        ],
        "a true shrink must be rejected before any adapter call"
    );
}

#[test]
fn file_backed_no_op_same_size_request_is_rejected_by_tier_two() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_device_capacity(4096)
        .with_filesystem_size(4096);

    // Requesting exactly the current size no longer trips tier 1 (which now
    // only rejects a strict shrink, so a same-size retry after a prior
    // partial grow can still reach tier 2) — but tier 2's live filesystem
    // check still correctly rejects it as a no-op once the mapping is open.
    let fixture = RealFixtureFile::create("resize-no-op-same-size", &[0u8; 4096]);

    let result = resize::run(&fixture.0, 4096, &no_progress, &luks, &fido2, &fs);

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
            "check_prerequisites".to_string(),
            "is_block_device".to_string(),
            "read_filesystem".to_string(),
            "check_prerequisites".to_string(),
            "open".to_string(),
            "device_capacity".to_string(),
            "filesystem_size".to_string(),
            "close".to_string(),
        ],
        "tier 2's rejection must still close the mapping it opened"
    );
}

// Regression test for the real bug this fix addresses (reconfirmed on real
// hardware across Story 4.3's and 5.2's `make test-hardware` runs): a
// same-size resize request against a volume where the LUKS2 header consumes
// a meaningful share of the raw backing storage (exactly what happens on a
// small volume — confirmed empirically: `cryptsetup luksFormat`'s default
// 16 MiB header is half of a 32 MiB test volume) must still be rejected.
// Before this fix, tier 2 compared `filesystem_size` (post-header payload
// bytes) directly against `new_size` (whole-file bytes), so the header
// itself always looked like "still needs to grow" — this is the fake-port
// equivalent of that exact scenario, reproducible without real hardware.
#[test]
fn file_backed_same_size_request_is_rejected_even_with_a_large_header_overhead() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    // Mapper payload capacity (post-header) is half the raw file size —
    // modeling a LUKS2 header that consumes the other half, and the
    // filesystem is already fully grown to fill that payload.
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_mapper_capacity(16 * 1024 * 1024)
        .with_filesystem_size(16 * 1024 * 1024);

    let fixture = RealFixtureFile::create(
        "resize-same-size-with-header-overhead",
        &[0u8; 32 * 1024 * 1024],
    );

    let result = resize::run(
        &fixture.0,
        32 * 1024 * 1024,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    let Err(DomainError::ResizeMustGrow {
        requested,
        current_size,
        ..
    }) = result
    else {
        panic!("expected ResizeMustGrow, got {result:?}");
    };
    assert_eq!(requested, 32 * 1024 * 1024);
    assert_eq!(current_size, 32 * 1024 * 1024);
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "is_block_device".to_string(),
            "read_filesystem".to_string(),
            "check_prerequisites".to_string(),
            "open".to_string(),
            "device_capacity".to_string(),
            "filesystem_size".to_string(),
            "close".to_string(),
        ],
        "tier 2's rejection must still close the mapping it opened"
    );
}

// Device-backed analog of the test above — the fix's core header-derivation
// logic (two separate `device_capacity` calls: tier 1 against the raw
// device path, tier 2 against the mapper's own device node) is only
// distinguishable from the pre-fix bug when the fake can actually return
// different values for the two call sites, which `with_mapper_capacity`
// (distinct from `with_device_capacity`) now lets it do.
#[test]
fn device_backed_same_size_request_is_rejected_even_with_a_large_header_overhead() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    // Raw device capacity is 32 MiB; the mapper's own payload capacity
    // (post-header) is half of that — modeling a LUKS2 header that
    // consumes the other half, and the filesystem is already fully grown
    // to fill that payload.
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_is_block_device(true)
        .with_device_capacity(32 * 1024 * 1024)
        .with_mapper_capacity(16 * 1024 * 1024)
        .with_filesystem_size(16 * 1024 * 1024);

    let fixture =
        RealFixtureFile::create("resize-device-same-size-with-header-overhead", &[0u8; 4096]);

    let result = resize::run(
        &fixture.0,
        32 * 1024 * 1024,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    let Err(DomainError::ResizeMustGrow {
        requested,
        current_size,
        ..
    }) = result
    else {
        panic!("expected ResizeMustGrow, got {result:?}");
    };
    assert_eq!(requested, 32 * 1024 * 1024);
    assert_eq!(current_size, 32 * 1024 * 1024);
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "is_block_device".to_string(),
            "device_capacity".to_string(),
            "read_filesystem".to_string(),
            "check_prerequisites".to_string(),
            "open".to_string(),
            "device_capacity".to_string(),
            "filesystem_size".to_string(),
            "close".to_string(),
        ],
        "tier 2's rejection must still close the mapping it opened"
    );
}

// Regression test for a review finding (2026-07-26): a resize call that grew
// the backing file to `new_size` but then failed before `luks.resize`/
// `growfs` completed must be retriable with the same `new_size` — tier 1's
// old `<=` comparison wrongly rejected this identical retry as a false
// no-op, permanently blocking recovery.
#[test]
fn file_backed_retry_after_a_partial_failure_completes_instead_of_being_rejected() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_device_capacity(8192)
        .with_filesystem_size(4096);

    // The backing file is already 8192 bytes (as if a prior resize call's
    // `set_backing_file_size` already succeeded), but the live filesystem
    // is still only 4096 bytes (as if that same prior call then failed
    // before `growfs` completed).
    let fixture = RealFixtureFile::create("resize-retry-after-partial-failure", &[0u8; 8192]);

    let result = resize::run(&fixture.0, 8192, &no_progress, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "is_block_device".to_string(),
            "read_filesystem".to_string(),
            "check_prerequisites".to_string(),
            "open".to_string(),
            "device_capacity".to_string(),
            "filesystem_size".to_string(),
            "set_backing_file_size".to_string(),
            "resize".to_string(),
            "growfs".to_string(),
            "close".to_string(),
        ],
        "a retry with an already-grown backing file must still complete the resize"
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

    let result = resize::run(&fixture.0, 2048, &no_progress, &luks, &fido2, &fs);

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
        vec![
            "check_prerequisites".to_string(),
            "is_block_device".to_string(),
            "device_capacity".to_string()
        ],
        "too-small-partition rejection must not reach read_filesystem or luks.open"
    );
}

// Story 1.6's headroom feature means a device-backed volume's raw capacity can
// be larger than its actual current provisioned size — tier 1 alone (which
// only compares against raw capacity) cannot catch a request that's smaller
// than the volume's *actual* current filesystem size, so tier 2 (checked after
// `open`, against the filesystem's own superblock — never the raw LUKS
// mapping, which always reports the full backing storage on reopen) must
// still reject it, and must close the mapping it just opened rather than
// leaking it (rollback discipline).
#[test]
fn device_backed_headroom_shrink_is_caught_by_tier_two_and_closes_the_mapping() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_is_block_device(true)
        .with_device_capacity(1024 * 1024 * 1024) // raw device is huge...
        .with_filesystem_size(4096); // ...but this volume's filesystem only uses 4096 bytes of it.

    let fixture = RealFixtureFile::create("resize-headroom-shrink", &[0u8; 4096]);

    // Passes tier 1 (well within raw capacity) but does not actually grow
    // the volume's live current filesystem size of 4096.
    let result = resize::run(&fixture.0, 4096, &no_progress, &luks, &fido2, &fs);

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
            "check_prerequisites".to_string(),
            "is_block_device".to_string(),
            "device_capacity".to_string(),
            "read_filesystem".to_string(),
            "check_prerequisites".to_string(),
            "open".to_string(),
            "device_capacity".to_string(),
            "filesystem_size".to_string(),
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
        .with_device_capacity(4096)
        .with_filesystem_size(4096)
        .with_failure_at("growfs");

    let fixture = RealFixtureFile::create("resize-mid-flow-failure", &[0u8; 4096]);

    let result = resize::run(&fixture.0, 8192, &no_progress, &luks, &fido2, &fs);

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "is_block_device".to_string(),
            "read_filesystem".to_string(),
            "check_prerequisites".to_string(),
            "open".to_string(),
            "device_capacity".to_string(),
            "filesystem_size".to_string(),
            "set_backing_file_size".to_string(),
            "resize".to_string(),
            "growfs".to_string(),
            "close".to_string(),
        ],
        "a failure after a successful resize must still close the mapping"
    );
}

// Regression test for a review finding (2026-07-26): a `luks.close` failure
// *after* a fully successful grow must be distinguishable from a plain
// resize failure, so the user isn't told resize failed when their volume's
// capacity was actually already safely increased.
#[test]
fn close_failure_after_a_successful_grow_reports_the_grow_succeeded() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_failure_at("close");
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_device_capacity(4096)
        .with_filesystem_size(4096);

    let fixture = RealFixtureFile::create("resize-close-failure-after-grow", &[0u8; 4096]);

    let result = resize::run(&fixture.0, 8192, &no_progress, &luks, &fido2, &fs);

    let Err(DomainError::AdapterFailure(message)) = result else {
        panic!("expected AdapterFailure, got {result:?}");
    };
    assert!(
        message.contains("volume grown to 8192 bytes"),
        "message should acknowledge the grow succeeded: {message:?}"
    );
    assert!(
        message.contains("failed to re-lock afterward"),
        "message should distinguish this from a plain resize failure: {message:?}"
    );
}

// Proves resize's two preflight calls fire in the right order with the right
// arguments: the unconditional first call (AD-4's base rule, `None`), then a
// second, narrower call using whatever `read_filesystem` actually reported —
// not a stand-in — same "prove the real value flows through" discipline
// Stories 6.2/6.3 established for their own capture fields.
#[test]
fn resize_calls_check_prerequisites_twice_with_none_then_the_read_filesystem_value() {
    let luks = FakeLuksBackend::passing().with_read_filesystem(Filesystem::Xfs);
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing()
        .with_device_capacity(4096)
        .with_filesystem_size(4096);

    let fixture = RealFixtureFile::create("resize-check-prerequisites-order", &[0u8; 4096]);

    let result = resize::run(&fixture.0, 8192, &no_progress, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(
        fs.check_prerequisites_filesystem_calls(),
        vec![None, Some(Filesystem::Xfs)]
    );
}

// Proves a missing xfs toolchain actually aborts resize with PreflightFailed
// before luks.open ever spends a real FIDO2 touch — the preflight gate is
// load-bearing, not just logged.
#[test]
fn resize_aborts_before_opening_when_preflight_finds_a_missing_toolchain() {
    let luks = FakeLuksBackend::passing().with_read_filesystem(Filesystem::Xfs);
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::failing(&["mkfs.xfs"]);

    let fixture = RealFixtureFile::create("resize-second-preflight-blocks-open", &[0u8; 4096]);

    let result = resize::run(&fixture.0, 8192, &no_progress, &luks, &fido2, &fs);

    assert!(
        matches!(result, Err(DomainError::PreflightFailed(_))),
        "expected PreflightFailed, got {result:?}"
    );
    assert_eq!(
        luks.last_open(),
        None,
        "the second preflight call must abort before luks.open is ever reached"
    );
}

// Btrfs's own resize ioctl refuses any resize whose resulting size is under
// 256 MiB (confirmed empirically on real hardware, 2026-08-09) — a genuine
// grow request (well above the live current size) that's still too small
// for Btrfs's own kernel floor must be refused before any mutating call,
// not left to fail deep inside `fs.growfs`.
#[test]
fn resize_to_btrfs_below_its_own_kernel_floor_is_refused_before_any_mutating_call() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_read_filesystem(Filesystem::Btrfs);
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_device_capacity(4096)
        .with_filesystem_size(4096);

    let fixture = RealFixtureFile::create("resize-btrfs-below-kernel-floor", &[0u8; 4096]);

    // A real grow (200 MiB is comfortably above the live 4096-byte current
    // size), but well under Btrfs's own 256 MiB resize floor.
    let target_size = 200 * 1024 * 1024;
    let result = resize::run(&fixture.0, target_size, &no_progress, &luks, &fido2, &fs);

    match result {
        Err(DomainError::DeviceTooSmall { path, size }) => {
            assert_eq!(path, fixture.0);
            assert_eq!(size, target_size);
        }
        other => panic!("expected DomainError::DeviceTooSmall, got {other:?}"),
    }
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "is_block_device".to_string(),
            "read_filesystem".to_string(),
            "check_prerequisites".to_string(),
            "open".to_string(),
            "device_capacity".to_string(),
            "filesystem_size".to_string(),
            "close".to_string(),
        ],
        "must be refused before set_backing_file_size/resize/growfs, but still close the mapping it opened"
    );
}

// Proves the Btrfs-specific floor doesn't leak onto other filesystems: the
// exact same target size that Btrfs's own kernel floor refuses must still
// succeed for ext4, which has no equivalent resize-time minimum.
#[test]
fn resize_to_the_same_small_target_still_succeeds_for_ext4() {
    let luks = FakeLuksBackend::passing().with_read_filesystem(Filesystem::Ext4);
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing()
        .with_device_capacity(4096)
        .with_filesystem_size(4096);

    let fixture = RealFixtureFile::create("resize-ext4-below-btrfs-floor", &[0u8; 4096]);

    let result = resize::run(
        &fixture.0,
        200 * 1024 * 1024,
        &no_progress,
        &luks,
        &fido2,
        &fs,
    );

    assert!(
        result.is_ok(),
        "ext4 must not be rejected by Btrfs's own, much higher resize floor: {result:?}"
    );
}
