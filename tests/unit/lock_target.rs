use std::path::PathBuf;

use hypogaol::adapters::exec::ExecAdapter;
use hypogaol::domain::errors::DomainError;
use hypogaol::ports::filesystem_backend::FilesystemBackend;

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
