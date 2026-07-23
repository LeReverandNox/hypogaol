use std::path::PathBuf;

use tomb_fido2::domain::mapping_name;
use tomb_fido2::domain::workflows::unlock;

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

#[test]
fn happy_path_opens_and_mounts_using_the_shared_mapping_name() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let fixture = RealFixtureFile::create("unlock-happy-path");

    let result = unlock::run(&fixture.0, &luks, &fido2, &fs);

    let expected_name = mapping_name::mapping_name(&fixture.0).unwrap();
    assert_eq!(
        result.unwrap(),
        PathBuf::from(format!("/tmp/fake-mount-{expected_name}"))
    );

    assert_eq!(*log.borrow(), vec!["open".to_string(), "mount".to_string()]);
    assert_eq!(luks.last_open(), Some((fixture.0.clone(), expected_name)));
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

    let result = unlock::run(&fixture.0, &luks, &fido2, &fs);

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec!["open".to_string(), "mount".to_string(), "close".to_string()]
    );
}
