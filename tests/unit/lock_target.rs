use std::path::PathBuf;

use hypogaol::adapters::exec::ExecAdapter;
use hypogaol::domain::errors::DomainError;
use hypogaol::ports::filesystem_backend::FilesystemBackend;
use hypogaol::ports::luks_backend::LuksBackend;

/// A real, uniquely-named file whose canonical path feeds `lock_target`'s
/// abstract-socket lock name — needs no hardware, `sudo`, or a real LUKS
/// volume; the lock itself is a kernel-only resource (a Linux
/// abstract-namespace `AF_UNIX` socket), never anything opened on this file.
/// Cleans itself up on drop, including on test panic.
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
fn real_lock_contends_while_held_and_releases_on_drop() {
    let adapter = ExecAdapter::default();
    let fixture = RealFixtureFile::create("real-lock-contends-and-releases");

    let first_guard = adapter
        .lock_target(&fixture.0)
        .expect("first lock_target call should succeed");

    let second_attempt = adapter.lock_target(&fixture.0);
    assert!(
        matches!(second_attempt, Err(DomainError::LockContention(_))),
        "a second lock_target call while the first guard is still held must contend, got {second_attempt:?}"
    );

    drop(first_guard);

    // A short retry, not present in production code: `cargo test` runs every
    // test as a thread inside one shared OS process, so `fork()` (via
    // another concurrently-running test's `Command::spawn()`, e.g. the
    // cryptsetup regression test below) can transiently duplicate this
    // guard's fd into a not-yet-`exec`'d child, which briefly keeps the
    // abstract name bound even after this thread's own `drop` above — a
    // microsecond-scale window closed by that child's own `CLOEXEC` cleanup
    // at `exec()` time. Real hypogaol invocations never hit this: each is a
    // single-threaded process that always waits for one subprocess to fully
    // exit before spawning the next, so there is never another thread whose
    // `fork()` could duplicate this process's fd table.
    let mut third_attempt = adapter.lock_target(&fixture.0);
    for _ in 0..20 {
        if third_attempt.is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        third_attempt = adapter.lock_target(&fixture.0);
    }
    assert!(
        third_attempt.is_ok(),
        "dropping the first guard must release the abstract-socket lock, got {third_attempt:?}"
    );
}

/// Regression test for a real deadlock found post-review (2026-08-10):
/// `cryptsetup`/`systemd-cryptenroll` both take their own internal lock on
/// the LUKS2 container for nearly every operation, including read-only
/// ones — an earlier version of `lock_target` used a real `flock(2)` on the
/// target file itself, so once held for a whole workflow, any subsequent
/// `cryptsetup` subprocess call against that same target blocked forever
/// waiting for a lock this process would never release until that
/// subprocess finished. Reported as `hypogaol create file --size 64M
/// volume.img` hanging indefinitely on a second run against the
/// already-existing file, and again as `hypogaol enroll` hanging silently
/// before ever reaching `systemd-cryptenroll`'s own touch-prompt output
/// (which has no `--disable-locks`-equivalent escape hatch, unlike
/// `cryptsetup`). `lock_target`'s current abstract-socket mechanism makes
/// this structurally impossible — it never opens or locks anything on the
/// target's own filesystem — but this test stays as a permanent guard
/// against ever reintroducing a shared resource with `cryptsetup`.
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
    // does — held for the same duration a real workflow would hold it
    // across a `cryptsetup`/`systemd-cryptenroll` subprocess call.
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
