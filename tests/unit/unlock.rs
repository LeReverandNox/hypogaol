use std::cell::RefCell;
use std::path::PathBuf;

use hypogaol::domain::hooks::{BindHookSkipReason, HookWarning};
use hypogaol::domain::mapping_name;
use hypogaol::domain::types::HookFileMeta;
use hypogaol::domain::workflows::unlock;

use crate::fakes::{new_call_log, FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend};

/// `mapping_name` (AD-12) canonicalizes its input directly via `std::fs`, for
/// real, even in these fake-port-backed tests (it is a pure `domain` helper,
/// not something behind a port). Give it a real, uniquely-named file to
/// canonicalize rather than a path that only exists in the fakes' internal
/// bookkeeping. Cleans itself up on drop, including on test panic.
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

/// A real, temporary directory — used to stand in for a mounted volume's
/// mountpoint (matching `FakeFilesystemBackend::mount`'s own hardcoded
/// `/tmp/fake-mount-{name}` return shape) or the invoking user's `$HOME`,
/// since `unlock::run`'s hooks step reads `bind-hooks`/`exec-hooks` via a
/// direct `std::fs::read_to_string` call and `resolve_bind_hook_entry`
/// canonicalizes for real (Task 4's Dev Notes). Cleans itself up on drop,
/// including on test panic.
struct RealFixtureDir(PathBuf);

impl RealFixtureDir {
    fn create(path: PathBuf) -> Self {
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

/// `FakeFilesystemBackend::mount`'s exact, hardcoded return shape — the real
/// directory a hooks-step test must create so `unlock::run`'s direct
/// `std::fs::read_to_string` on `bind-hooks`/`exec-hooks` can find it.
fn fake_mountpoint_for(name: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/fake-mount-{name}"))
}

#[test]
fn happy_path_opens_and_mounts_using_the_shared_mapping_name() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("unlock-happy-path");

    let result = unlock::run(&fixture.0, false, false, &|_| {}, &luks, &fido2, &fs);

    let expected_name = mapping_name::mapping_name(&fixture.0).unwrap();
    assert_eq!(
        result.unwrap(),
        PathBuf::from(format!("/tmp/fake-mount-{expected_name}"))
    );

    // The hooks step (Story 4.4) always checks for `bind-hooks`/`exec-hooks`
    // after a successful mount, even when `skip_hooks` is false and neither
    // file exists — `invoking_home_dir` up front, then one `path_exists`
    // check per hooks file, both reporting absent here.
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "open".to_string(),
            "mount".to_string(),
            "invoking_home_dir".to_string(),
            "path_exists".to_string(),
            "path_exists".to_string(),
        ]
    );
    assert_eq!(
        luks.last_open(),
        Some((fixture.0.clone(), expected_name, false))
    );
}

#[test]
fn read_only_true_is_passed_to_both_open_and_mount() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("unlock-read-only-true");

    let result = unlock::run(&fixture.0, true, false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert!(luks.last_open().unwrap().2);
    assert_eq!(fs.last_mount_read_only(), Some(true));
}

#[test]
fn read_only_false_is_passed_to_both_open_and_mount() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("unlock-read-only-false");

    let result = unlock::run(&fixture.0, false, false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert!(!luks.last_open().unwrap().2);
    assert_eq!(fs.last_mount_read_only(), Some(false));
}

#[test]
fn mount_failure_closes_the_just_opened_mapping() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_failure_at("mount");

    let fixture = RealFixtureFile::create("unlock-mount-failure");

    let result = unlock::run(&fixture.0, false, false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "open".to_string(),
            "mount".to_string(),
            "close".to_string()
        ]
    );
}

// Regression guard proving AC #3's rollback-on-mount-failure discipline
// holds identically in the read-only path — the rollback code is
// unconditional, so no production-code change is needed for this to pass.
#[test]
fn read_only_mount_failure_still_closes_the_just_opened_mapping() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_failure_at("mount");

    let fixture = RealFixtureFile::create("unlock-read-only-mount-failure");

    let result = unlock::run(&fixture.0, true, false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "open".to_string(),
            "mount".to_string(),
            "close".to_string()
        ]
    );
}

#[test]
fn open_bind_mounts_every_valid_bind_hooks_entry_after_mount_succeeds() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("unlock-bind-hooks-happy-path");
    let expected_name = mapping_name::mapping_name(&fixture.0).unwrap();
    let mountpoint = RealFixtureDir::create(fake_mountpoint_for(&expected_name));
    mountpoint.subdir("src-a");
    mountpoint.subdir("src-b");
    let home = RealFixtureDir::create(
        std::env::temp_dir().join("hypogaol-unit-test-unlock-bind-hooks-happy-path-home"),
    );
    home.subdir("dest-a");
    home.subdir("dest-b");
    std::fs::write(
        mountpoint.0.join("bind-hooks"),
        "src-a dest-a\nsrc-b dest-b\n",
    )
    .expect("failed to write bind-hooks fixture");

    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_path_exists(true)
        .with_invoking_home_dir(home.0.clone());

    let result = unlock::run(&fixture.0, false, false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    let bind_mount_calls = log.borrow().iter().filter(|c| *c == "bind_mount").count();
    assert_eq!(
        bind_mount_calls,
        2,
        "expected both bind-hooks entries to be bind-mounted, log: {:?}",
        log.borrow()
    );
}

#[test]
fn open_skips_an_escaping_bind_hooks_entry_with_a_warning_and_continues() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("unlock-bind-hooks-escaping-entry");
    let expected_name = mapping_name::mapping_name(&fixture.0).unwrap();
    let mountpoint = RealFixtureDir::create(fake_mountpoint_for(&expected_name));
    mountpoint.subdir("src");
    let home = RealFixtureDir::create(
        std::env::temp_dir().join("hypogaol-unit-test-unlock-bind-hooks-escaping-entry-home"),
    );
    // An absolute `dest` makes `Path::join` discard `home_dir` entirely and
    // resolve straight to `mountpoint` itself — a real, existing directory,
    // but outside `home`, so this entry must be skipped (AC #2), not
    // applied.
    let escaping_dest = mountpoint.0.to_string_lossy().into_owned();
    std::fs::write(
        mountpoint.0.join("bind-hooks"),
        format!("src {escaping_dest}\n"),
    )
    .expect("failed to write bind-hooks fixture");

    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_path_exists(true)
        .with_invoking_home_dir(home.0.clone());

    let warnings: RefCell<Vec<HookWarning>> = RefCell::new(Vec::new());
    let warn = |w: HookWarning| warnings.borrow_mut().push(w);

    let result = unlock::run(&fixture.0, false, false, &warn, &luks, &fido2, &fs);

    assert!(
        result.is_ok(),
        "an escaping bind-hooks entry must not fail open (AC #2), got {result:?}"
    );
    assert!(
        !log.borrow().contains(&"bind_mount".to_string()),
        "an escaping entry must never be bind-mounted, log: {:?}",
        log.borrow()
    );
    assert!(
        luks.last_close().is_none(),
        "AC #2 is not a hard failure — no rollback"
    );
    assert_eq!(
        warnings.borrow().as_slice(),
        [HookWarning::BindHookSkipped {
            source: "src".to_string(),
            dest: escaping_dest,
            reason: BindHookSkipReason::DestEscapesHome,
        }]
    );
}

// AC #2's "on any Err — from `resolve_bind_hook_entry` *or* from
// `bind_mount` itself" clause: a well-formed, contained entry can still fail
// at the actual `mount --bind` call, which must warn and continue exactly
// like a resolution failure, never abort the open.
#[test]
fn open_warns_but_continues_when_bind_mount_itself_fails() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("unlock-bind-mount-call-fails");
    let expected_name = mapping_name::mapping_name(&fixture.0).unwrap();
    let mountpoint = RealFixtureDir::create(fake_mountpoint_for(&expected_name));
    mountpoint.subdir("src");
    let home = RealFixtureDir::create(
        std::env::temp_dir().join("hypogaol-unit-test-unlock-bind-mount-call-fails-home"),
    );
    home.subdir("dest");
    std::fs::write(mountpoint.0.join("bind-hooks"), "src dest\n")
        .expect("failed to write bind-hooks fixture");

    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_path_exists(true)
        .with_invoking_home_dir(home.0.clone())
        .with_bind_mount_failure();

    let warnings: RefCell<Vec<HookWarning>> = RefCell::new(Vec::new());
    let warn = |w: HookWarning| warnings.borrow_mut().push(w);

    let result = unlock::run(&fixture.0, false, false, &warn, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert!(
        luks.last_close().is_none(),
        "not a hard failure — no rollback"
    );
    assert_eq!(
        warnings.borrow().as_slice(),
        [HookWarning::BindHookSkipped {
            source: "src".to_string(),
            dest: "dest".to_string(),
            reason: BindHookSkipReason::BindMountFailed,
        }]
    );
}

// AC #3's "a nonzero exit from the hook script itself is not an Err ...
// reports a non-fatal HookWarning::ExecHookNonZeroExit" clause.
#[test]
fn open_warns_but_continues_when_exec_hooks_exits_nonzero() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("unlock-exec-hooks-nonzero-exit");
    let expected_name = mapping_name::mapping_name(&fixture.0).unwrap();
    let mountpoint = RealFixtureDir::create(fake_mountpoint_for(&expected_name));
    let exec_hooks_path = mountpoint.0.join("exec-hooks");
    std::fs::write(&exec_hooks_path, "#!/bin/sh\nexit 3\n")
        .expect("failed to write exec-hooks fixture");
    // `with_path_exists(true)` below makes `bind-hooks` appear to exist too —
    // give it a real (empty) file so the read actually succeeds, keeping this
    // test's warnings assertion scoped to exec-hooks only.
    std::fs::write(mountpoint.0.join("bind-hooks"), "")
        .expect("failed to write empty bind-hooks fixture");

    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_path_exists(true)
        .with_run_hook_exit_status(Some(3));

    let warnings: RefCell<Vec<HookWarning>> = RefCell::new(Vec::new());
    let warn = |w: HookWarning| warnings.borrow_mut().push(w);

    let result = unlock::run(&fixture.0, false, false, &warn, &luks, &fido2, &fs);

    assert!(
        result.is_ok(),
        "a nonzero exec-hooks exit must not fail open, got {result:?}"
    );
    assert_eq!(
        warnings.borrow().as_slice(),
        [HookWarning::ExecHookNonZeroExit {
            path: exec_hooks_path,
            exit_code: Some(3),
        }]
    );
}

#[test]
fn open_runs_exec_hooks_with_open_and_the_mountpoint_when_guardrail_passes() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("unlock-exec-hooks-happy-path");
    let expected_name = mapping_name::mapping_name(&fixture.0).unwrap();
    let mountpoint = RealFixtureDir::create(fake_mountpoint_for(&expected_name));
    std::fs::write(mountpoint.0.join("exec-hooks"), "#!/bin/sh\n")
        .expect("failed to write exec-hooks fixture");

    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_path_exists(true);

    let result = unlock::run(&fixture.0, false, false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    let (path, args) = fs.last_run_hook().expect("expected run_hook to be called");
    assert_eq!(path, mountpoint.0.join("exec-hooks"));
    assert_eq!(args, vec!["open", &mountpoint.0.to_string_lossy()]);
}

#[test]
fn open_hard_errors_and_rolls_back_when_exec_hooks_guardrail_fails() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("unlock-exec-hooks-guardrail-fails");
    let expected_name = mapping_name::mapping_name(&fixture.0).unwrap();
    let mountpoint = RealFixtureDir::create(fake_mountpoint_for(&expected_name));
    mountpoint.subdir("src");
    let home = RealFixtureDir::create(
        std::env::temp_dir().join("hypogaol-unit-test-unlock-exec-hooks-guardrail-fails-home"),
    );
    home.subdir("dest");
    std::fs::write(mountpoint.0.join("bind-hooks"), "src dest\n")
        .expect("failed to write bind-hooks fixture");
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
        .with_invoking_home_dir(home.0.clone())
        .with_hook_file_metadata(rejecting_meta);

    let result = unlock::run(&fixture.0, false, false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert!(
        log.borrow().contains(&"bind_mount".to_string()),
        "the valid bind-hooks entry must have been applied before the exec-hooks guardrail ran"
    );
    // AC #4's rollback: the applied bind-hooks destination is unmounted,
    // then the primary mount, then the LUKS2 mapping is closed.
    let bind_teardown = log
        .borrow()
        .iter()
        .position(|c| c == "unmount_bind_hook_destination");
    let umount = log.borrow().iter().position(|c| c == "umount");
    let close = log.borrow().iter().position(|c| c == "close");
    assert!(
        bind_teardown.is_some() && umount.is_some() && close.is_some(),
        "expected full rollback, log: {:?}",
        log.borrow()
    );
    assert!(bind_teardown.unwrap() < umount.unwrap());
    assert!(umount.unwrap() < close.unwrap());
    assert!(luks.last_close().is_some());
}

#[test]
fn open_skips_all_hooks_when_skip_hooks_true() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_path_exists(true);

    let fixture = RealFixtureFile::create("unlock-skip-hooks-true");

    let result = unlock::run(&fixture.0, false, true, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "open".to_string(),
            "mount".to_string()
        ]
    );
}

#[test]
fn open_skips_all_hooks_when_read_only_true_even_if_skip_hooks_false() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_path_exists(true);

    let fixture = RealFixtureFile::create("unlock-skip-hooks-read-only");

    let result = unlock::run(&fixture.0, true, false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "open".to_string(),
            "mount".to_string()
        ]
    );
}
