use std::path::PathBuf;

use hypogaol::adapters::exec::ExecAdapter;
use hypogaol::domain::errors::DomainError;
use hypogaol::ports::filesystem_backend::FilesystemBackend;
use hypogaol::ports::luks_backend::LuksBackend;

/// A real, uniquely-named file to lock — `flock(2)` semantics apply to any
/// regular file, and two independent `open()` calls against the same path
/// within a single test process behave exactly like two separate processes
/// for locking purposes (a lock is associated with the *open file
/// description*, not the process), so this test needs no hardware, `sudo`,
/// or a real LUKS volume. Cleans itself up on drop, including on test panic.
struct RealFixtureFile(PathBuf);

impl RealFixtureFile {
    fn create(unique_name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("hypogaol-unit-test-lock-target-{unique_name}"));
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
fn real_flock_contends_while_held_and_releases_on_drop() {
    let adapter = ExecAdapter::default();
    let fixture = RealFixtureFile::create("real-flock-contends-and-releases");

    let first_guard = adapter
        .lock_target(&fixture.0)
        .expect("first lock_target call should succeed");

    let second_attempt = adapter.lock_target(&fixture.0);
    assert!(
        matches!(second_attempt, Err(DomainError::LockContention(_))),
        "a second lock_target call while the first guard is still held must contend, got {second_attempt:?}"
    );

    drop(first_guard);

    let third_attempt = adapter.lock_target(&fixture.0);
    assert!(
        third_attempt.is_ok(),
        "dropping the first guard must release the real flock, got {third_attempt:?}"
    );
}

/// Regression test for a real deadlock found post-review (2026-08-10):
/// `cryptsetup` takes its own internal lock on the LUKS2 container for
/// essentially every operation, including read-only ones — while our own
/// `LockGuard` is held for the whole workflow, any subsequent `cryptsetup`
/// subprocess call against the same target would block forever waiting for
/// a lock we'd never release until that same subprocess finished. Reported
/// as `hypogaol create file --size 64M volume.img` hanging indefinitely on
/// a second run against the already-existing file — the file already
/// exists, so `fs.lock_target` locks the file itself (not its parent), and
/// the very next call, `luks.has_marker_token`, needed cryptsetup's own
/// lock on that same file.
///
/// No hardware or `sudo` needed: `cryptsetup luksFormat`/`luksDump` on a
/// plain file need no privilege, and this exercises the real `ExecAdapter`
/// port methods (not a fake) against a real, freshly-formatted LUKS2
/// container. Bounded by a timeout on a background thread so a regression
/// fails loudly instead of hanging the whole test suite.
#[test]
fn real_lock_does_not_deadlock_a_subsequent_cryptsetup_metadata_read() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let adapter = ExecAdapter::default();
    let fixture = RealFixtureFile::create("real-lock-no-cryptsetup-deadlock");
    // 32 MiB: enough for a LUKS2 header plus minimal payload (matches
    // MIN_VOLUME_SIZE_BYTES elsewhere in this codebase).
    std::fs::File::create(&fixture.0)
        .and_then(|f| f.set_len(32 * 1024 * 1024))
        .expect("failed to size the fixture file for luksFormat");

    // Format a real LUKS2 container directly on the fixture file — mirrors
    // exactly what `create::run` does before `has_marker_token` is ever
    // called. A fast PBKDF keeps this setup step quick; it has nothing to
    // do with what this test actually proves.
    let mut format_child = Command::new("cryptsetup")
        .args([
            "luksFormat",
            "--type",
            "luks2",
            "--batch-mode",
            "--pbkdf",
            "pbkdf2",
            "--pbkdf-force-iterations",
            "1000",
            "--key-file",
            "-",
        ])
        .arg(&fixture.0)
        .stdin(Stdio::piped())
        .spawn()
        .expect("failed to spawn cryptsetup luksFormat for test setup");
    format_child
        .stdin
        .take()
        .expect("child stdin should be piped")
        .write_all(b"test-passphrase")
        .expect("failed to write passphrase to cryptsetup luksFormat");
    let format_status = format_child
        .wait()
        .expect("failed to wait on cryptsetup luksFormat");
    assert!(format_status.success(), "test setup: luksFormat failed");

    // Hold our own real lock, exactly like create::run's `let _lock = ...`
    // does — the same exclusive flock the reported bug was still held under
    // when has_marker_token's own cryptsetup call hung indefinitely.
    let _lock = adapter
        .lock_target(&fixture.0)
        .expect("lock_target should succeed on the now-existing file");

    // Run has_marker_token (the real port method, real subprocess) on a
    // background thread, bounded by a timeout.
    let (tx, rx) = std::sync::mpsc::channel();
    let path = fixture.0.clone();
    std::thread::spawn(move || {
        let adapter = ExecAdapter::default();
        let _ = tx.send(adapter.has_marker_token(&path));
    });

    let result = rx.recv_timeout(std::time::Duration::from_secs(10)).expect(
        "has_marker_token did not return within 10s while our own lock was held — \
             regression of the cryptsetup-internal-locking deadlock (2026-08-10)",
    );
    assert!(
        !result.expect("has_marker_token should succeed"),
        "a freshly-formatted volume with no marker token should report false"
    );
}
