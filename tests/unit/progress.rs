use std::cell::RefCell;
use std::rc::Rc;

use hypogaol::domain::progress::{CreateStage, ResizeStage};
use hypogaol::domain::types::{CreateTarget, Filesystem};
use hypogaol::domain::workflows::create::{self, MIN_VOLUME_SIZE_BYTES};
use hypogaol::domain::workflows::resize;
use hypogaol::ports::fido2_backend::Fido2DeviceSelection;

use crate::fakes::{FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend};

/// A real, uniquely-named file to canonicalize (AD-12's `mapping_name` is a
/// pure `domain` helper, not something behind a port). Mirrors
/// `create.rs`/`resize.rs`'s own `RealFixtureFile` fixture.
struct RealFixtureFile(std::path::PathBuf);

impl RealFixtureFile {
    fn create(unique_name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("hypogaol-unit-test-{unique_name}"));
        std::fs::write(&path, []).expect("failed to create test fixture file");
        Self(path)
    }

    fn create_with_contents(unique_name: &str, contents: &[u8]) -> Self {
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
fn create_file_backed_fires_all_four_stages_in_real_order() {
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let fixture = RealFixtureFile::create("progress-create-file-backed");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let stages: Rc<RefCell<Vec<CreateStage>>> = Rc::new(RefCell::new(Vec::new()));
    let recorder = stages.clone();

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &|stage| recorder.borrow_mut().push(stage),
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(
        *stages.borrow(),
        vec![
            CreateStage::AllocatingBackingFile,
            CreateStage::FormattingLuks2,
            CreateStage::EnrollingFido2Key,
            CreateStage::CreatingFilesystem,
        ]
    );
}

#[test]
fn create_device_backed_never_fires_allocating_backing_file() {
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing().with_device_capacity(MIN_VOLUME_SIZE_BYTES * 2);

    let fixture = RealFixtureFile::create("progress-create-device-backed");
    let target = CreateTarget::Device {
        path: fixture.0.clone(),
        size: None,
        confirmed: true,
    };

    let stages: Rc<RefCell<Vec<CreateStage>>> = Rc::new(RefCell::new(Vec::new()));
    let recorder = stages.clone();

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &|stage| recorder.borrow_mut().push(stage),
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(
        *stages.borrow(),
        vec![
            CreateStage::FormattingLuks2,
            CreateStage::EnrollingFido2Key,
            CreateStage::CreatingFilesystem,
        ]
    );
}

#[test]
fn resize_file_backed_fires_all_three_stages_in_order() {
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing()
        .with_is_block_device(false)
        .with_device_capacity(4096)
        .with_filesystem_size(4096);

    let fixture =
        RealFixtureFile::create_with_contents("progress-resize-file-backed", &[0u8; 4096]);

    let stages: Rc<RefCell<Vec<ResizeStage>>> = Rc::new(RefCell::new(Vec::new()));
    let recorder = stages.clone();

    let result = resize::run(
        &fixture.0,
        8192,
        &|stage| recorder.borrow_mut().push(stage),
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(
        *stages.borrow(),
        vec![
            ResizeStage::GrowingBackingFile,
            ResizeStage::ResizingLuks2Mapping,
            ResizeStage::GrowingFilesystem,
        ]
    );
}

#[test]
fn resize_device_backed_skips_growing_backing_file() {
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing()
        .with_is_block_device(true)
        .with_device_capacity(1024 * 1024 * 1024)
        .with_filesystem_size(4096);

    let fixture =
        RealFixtureFile::create_with_contents("progress-resize-device-backed", &[0u8; 4096]);

    let stages: Rc<RefCell<Vec<ResizeStage>>> = Rc::new(RefCell::new(Vec::new()));
    let recorder = stages.clone();

    let result = resize::run(
        &fixture.0,
        8192,
        &|stage| recorder.borrow_mut().push(stage),
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
    assert_eq!(
        *stages.borrow(),
        vec![
            ResizeStage::ResizingLuks2Mapping,
            ResizeStage::GrowingFilesystem
        ]
    );
}

#[test]
fn create_does_not_fire_creating_filesystem_when_enroll_fails() {
    // A stage's message must never fire for work that never actually
    // completed — a failure between two stage boundaries should record only
    // the stages up to and including the one whose port call is about to
    // fail, never the ones after it.
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing().with_failure_at("enroll_fido2_key");
    let fs = FakeFilesystemBackend::passing();

    let fixture = RealFixtureFile::create("progress-create-enroll-failure");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_VOLUME_SIZE_BYTES,
    };

    let stages: Rc<RefCell<Vec<CreateStage>>> = Rc::new(RefCell::new(Vec::new()));
    let recorder = stages.clone();

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        None,
        false,
        Fido2DeviceSelection::Interactive,
        &|stage| recorder.borrow_mut().push(stage),
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert_eq!(
        *stages.borrow(),
        vec![
            CreateStage::AllocatingBackingFile,
            CreateStage::FormattingLuks2,
            CreateStage::EnrollingFido2Key,
        ],
        "CreatingFilesystem must not fire once enroll_fido2_key has failed"
    );
}

#[test]
fn resize_does_not_fire_growing_filesystem_when_luks_resize_fails() {
    let luks = FakeLuksBackend::passing().with_failure_at("resize");
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing()
        .with_is_block_device(false)
        .with_device_capacity(4096)
        .with_filesystem_size(4096);

    let fixture =
        RealFixtureFile::create_with_contents("progress-resize-luks-resize-failure", &[0u8; 4096]);

    let stages: Rc<RefCell<Vec<ResizeStage>>> = Rc::new(RefCell::new(Vec::new()));
    let recorder = stages.clone();

    let result = resize::run(
        &fixture.0,
        8192,
        &|stage| recorder.borrow_mut().push(stage),
        &luks,
        &fido2,
        &fs,
    );

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert_eq!(
        *stages.borrow(),
        vec![
            ResizeStage::GrowingBackingFile,
            ResizeStage::ResizingLuks2Mapping,
        ],
        "GrowingFilesystem must not fire once luks.resize has failed"
    );
}
