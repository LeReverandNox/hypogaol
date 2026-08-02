use std::path::PathBuf;

use hypogaol::domain::mapping_name;
use hypogaol::domain::types::HookFileMeta;
use hypogaol::domain::workflows::close;

use crate::fakes::{new_call_log, FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend};

/// `mapping_name` (AD-12) canonicalizes its input directly via `std::fs`, for
/// real, even in these fake-port-backed tests (it is a pure `domain` helper,
/// not something behind a port). Give it a real, uniquely-named file to
/// canonicalize rather than a path that only exists in the fakes' internal
/// bookkeeping. Cleans itself up on drop, including on test panic.
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

/// A real, temporary directory standing in for the volume's live mountpoint
/// (returned by `FakeFilesystemBackend::mount_point_of` via
/// `with_mount_point_of`) — `close::run`'s hooks step reads
/// `bind-hooks`/`exec-hooks` via a direct `std::fs::read_to_string` call and
/// `resolve_bind_hook_entry` canonicalizes for real (Task 5's Dev Notes).
/// Cleans itself up on drop, including on test panic.
struct RealFixtureDir(PathBuf);

impl RealFixtureDir {
    fn create(unique_name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("tomb-fido2-unit-test-close-{unique_name}"));
        std::fs::create_dir_all(&path).expect("failed to create test fixture dir");
        Self(path)
    }

    fn subdir(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(&path).expect("failed to create test fixture subdir");
        path
    }
}

impl Drop for RealFixtureDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn happy_path_unmounts_then_closes_using_the_shared_mapping_name() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("close-happy-path");

    let result = close::run(&fixture.0, false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    // The hooks step (Story 4.4) always runs first when `skip_hooks` is
    // false: `mount_point_of` to learn the volume root, then one `path_exists`
    // check per hooks file (`exec-hooks`, then `bind-hooks`), both absent
    // here.
    assert_eq!(
        *log.borrow(),
        vec![
            "mount_point_of".to_string(),
            "path_exists".to_string(),
            "path_exists".to_string(),
            "umount".to_string(),
            "close".to_string(),
        ]
    );

    // Both port calls must have been built from the same derived mapping
    // name — close never re-derives it twice into two different values.
    let expected_name = mapping_name::mapping_name(&fixture.0).unwrap();
    assert_eq!(fs.last_umount().unwrap().name, expected_name);
    assert_eq!(luks.last_close().unwrap().name, expected_name);
    assert_eq!(luks.last_open(), None, "close must never call luks.open");
}

#[test]
fn umount_failure_stops_before_calling_luks_close() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_failure_at("umount");

    let fixture = RealFixtureFile::create("close-umount-failure");

    let result = close::run(&fixture.0, false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "mount_point_of".to_string(),
            "path_exists".to_string(),
            "path_exists".to_string(),
            "umount".to_string(),
        ]
    );
}

// Recovery for a prior partial failure (umount succeeded, luks.close then
// failed): a retry's `umount` now reports "not currently mounted" since the
// filesystem is already unmounted — that must not be treated as a hard
// stop, or the dangling open mapping could never be closed again (review
// finding, 2026-07-26).
#[test]
fn umount_reporting_not_currently_mounted_still_proceeds_to_luks_close() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_umount_not_currently_mounted();

    let fixture = RealFixtureFile::create("close-idempotent-retry");

    let result = close::run(&fixture.0, false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "mount_point_of".to_string(),
            "path_exists".to_string(),
            "path_exists".to_string(),
            "umount".to_string(),
            "close".to_string(),
        ]
    );
}

#[test]
fn luks_close_failure_after_a_successful_umount_still_propagates_as_an_error() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_failure_at("close");
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("close-luks-close-failure");

    let result = close::run(&fixture.0, false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "mount_point_of".to_string(),
            "path_exists".to_string(),
            "path_exists".to_string(),
            "umount".to_string(),
            "close".to_string(),
        ]
    );
}

#[test]
fn close_runs_exec_hooks_with_close_volume_name_loopback_and_mapper_device_args() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("close-exec-hooks-happy-path");
    let mountpoint = RealFixtureDir::create("exec-hooks-happy-path-mnt");
    std::fs::write(mountpoint.0.join("exec-hooks"), "#!/bin/sh\n")
        .expect("failed to write exec-hooks fixture");

    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_path_exists(true)
        .with_mount_point_of(mountpoint.0.clone());

    let result = close::run(&fixture.0, false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");

    let expected_name = mapping_name::mapping_name(&fixture.0).unwrap();
    let expected_device_node = format!("/dev/mapper/{expected_name}");
    let expected_volume_name = fixture
        .0
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let (path, args) = fs.last_run_hook().expect("expected run_hook to be called");
    assert_eq!(path, mountpoint.0.join("exec-hooks"));
    assert_eq!(
        args,
        vec![
            "close",
            &mountpoint.0.to_string_lossy(),
            &expected_volume_name,
            &fixture.0.to_string_lossy(),
            &expected_device_node,
        ]
    );
}

#[test]
fn close_hard_errors_before_touching_umount_when_exec_hooks_guardrail_fails() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("close-exec-hooks-guardrail-fails");
    let mountpoint = RealFixtureDir::create("exec-hooks-guardrail-fails-mnt");
    std::fs::write(mountpoint.0.join("exec-hooks"), "#!/bin/sh\n")
        .expect("failed to write exec-hooks fixture");

    let rejecting_meta = HookFileMeta {
        is_regular_file: true,
        is_symlink: false,
        is_executable: false,
        owned_by_invoking_user_or_root: true,
        is_world_writable: false,
    };
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_path_exists(true)
        .with_mount_point_of(mountpoint.0.clone())
        .with_hook_file_metadata(rejecting_meta);

    let result = close::run(&fixture.0, false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert!(
        !log.borrow().contains(&"umount".to_string()),
        "must not touch umount when the guardrail rejects, log: {:?}",
        log.borrow()
    );
    assert!(
        !log.borrow().contains(&"close".to_string()),
        "must not touch luks.close when the guardrail rejects, log: {:?}",
        log.borrow()
    );
    assert!(luks.last_close().is_none());
}

#[test]
fn close_unmounts_bind_hooks_destinations_before_the_primary_mountpoint() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("close-bind-hooks-teardown-order");
    let mountpoint = RealFixtureDir::create("bind-hooks-teardown-order-mnt");
    mountpoint.subdir("src");
    let home = RealFixtureDir::create("bind-hooks-teardown-order-home-dir");
    home.subdir("dest");
    std::fs::write(mountpoint.0.join("bind-hooks"), "src dest\n")
        .expect("failed to write bind-hooks fixture");

    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_path_exists(true)
        .with_mount_point_of(mountpoint.0.clone())
        .with_invoking_home_dir(home.0.clone());

    let result = close::run(&fixture.0, false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    let bind_teardown = log
        .borrow()
        .iter()
        .position(|c| c == "unmount_bind_hook_destination");
    let umount = log.borrow().iter().position(|c| c == "umount");
    assert!(
        bind_teardown.is_some() && umount.is_some(),
        "expected both calls, log: {:?}",
        log.borrow()
    );
    assert!(
        bind_teardown.unwrap() < umount.unwrap(),
        "bind-hooks destinations must be unmounted before the primary mountpoint, log: {:?}",
        log.borrow()
    );
}

#[test]
fn close_skips_all_hooks_when_skip_hooks_true() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_path_exists(true);

    let fixture = RealFixtureFile::create("close-skip-hooks-true");

    let result = close::run(&fixture.0, true, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec!["umount".to_string(), "close".to_string()]
    );
}
