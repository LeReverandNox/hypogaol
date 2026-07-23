use tomb_fido2::domain::errors::DomainError;
use tomb_fido2::domain::types::{CreateTarget, Filesystem};
use tomb_fido2::domain::workflows::{close, create, resize, unlock};

use crate::fakes::{FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend};

// Each workflow's run() calls preflight::check as its first statement. A
// failing fake must return the preflight error immediately, proving the gate
// runs before the workflow's own todo!() (which would otherwise panic).

#[test]
fn create_run_stops_at_preflight_before_reaching_its_own_todo() {
    let luks = FakeLuksBackend::failing(&["cryptsetup"]);
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let target = CreateTarget::File {
        path: std::path::PathBuf::from("/tmp/does-not-matter"),
        size: 1024,
    };

    let result = create::run(target, Filesystem::Ext4, &luks, &fido2, &fs);

    assert!(matches!(result, Err(DomainError::PreflightFailed(_))));
}

#[test]
fn unlock_run_stops_at_preflight_before_reaching_its_own_todo() {
    let luks = FakeLuksBackend::failing(&["cryptsetup"]);
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let result = unlock::run(
        std::path::Path::new("/tmp/does-not-matter"),
        &luks,
        &fido2,
        &fs,
    );

    assert!(matches!(result, Err(DomainError::PreflightFailed(_))));
}

#[test]
fn close_run_stops_at_preflight_before_reaching_its_own_todo() {
    let luks = FakeLuksBackend::failing(&["cryptsetup"]);
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let result = close::run(&luks, &fido2, &fs);

    assert!(matches!(result, Err(DomainError::PreflightFailed(_))));
}

#[test]
fn resize_run_stops_at_preflight_before_reaching_its_own_todo() {
    let luks = FakeLuksBackend::failing(&["cryptsetup"]);
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let result = resize::run(&luks, &fido2, &fs);

    assert!(matches!(result, Err(DomainError::PreflightFailed(_))));
}
