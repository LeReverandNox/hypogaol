use std::cell::RefCell;
use std::rc::Rc;

use tomb_fido2::domain::progress::{CreateStage, ResizeStage};
use tomb_fido2::domain::types::{CreateTarget, Filesystem};
use tomb_fido2::domain::workflows::create::{self, MIN_TOMB_SIZE_BYTES};
use tomb_fido2::domain::workflows::resize;
use tomb_fido2::ports::fido2_backend::Fido2DeviceSelection;

use crate::fakes::{FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend};

/// A real, uniquely-named file to canonicalize (AD-12's `mapping_name` is a
/// pure `domain` helper, not something behind a port). Mirrors
/// `create.rs`/`resize.rs`'s own `RealFixtureFile` fixture.
struct RealFixtureFile(std::path::PathBuf);

impl RealFixtureFile {
    fn create(unique_name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("tomb-fido2-unit-test-{unique_name}"));
        std::fs::write(&path, []).expect("failed to create test fixture file");
        Self(path)
    }

    fn create_with_contents(unique_name: &str, contents: &[u8]) -> Self {
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
fn create_file_backed_fires_all_four_stages_in_real_order() {
    let luks = FakeLuksBackend::passing();
    let fido2 = FakeFido2Backend::passing();
    let fs = FakeFilesystemBackend::passing();

    let fixture = RealFixtureFile::create("progress-create-file-backed");
    let target = CreateTarget::File {
        path: fixture.0.clone(),
        size: MIN_TOMB_SIZE_BYTES,
    };

    let stages: Rc<RefCell<Vec<CreateStage>>> = Rc::new(RefCell::new(Vec::new()));
    let recorder = stages.clone();

    let result = create::run(
        target,
        Filesystem::Ext4,
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
    let fs = FakeFilesystemBackend::passing().with_device_capacity(MIN_TOMB_SIZE_BYTES * 2);

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
