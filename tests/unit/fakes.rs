use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use tomb_fido2::domain::errors::DomainError;
use tomb_fido2::domain::types::{Filesystem, KeyMetadata, KeyslotInfo, KeyslotRef, MapperHandle};
use tomb_fido2::ports::fido2_backend::Fido2Backend;
use tomb_fido2::ports::filesystem_backend::FilesystemBackend;
use tomb_fido2::ports::luks_backend::LuksBackend;

fn missing(deps: &[&str]) -> Vec<String> {
    deps.iter().map(|dep| dep.to_string()).collect()
}

/// Shared call log so a test can assert cross-port call ordering (Task 8).
pub type CallLog = Rc<RefCell<Vec<String>>>;

pub fn new_call_log() -> CallLog {
    Rc::new(RefCell::new(Vec::new()))
}

pub struct FakeLuksBackend {
    prerequisites: Result<(), Vec<String>>,
    keyslots: RefCell<Vec<KeyslotInfo>>,
    log: CallLog,
}

impl FakeLuksBackend {
    pub fn passing() -> Self {
        Self {
            prerequisites: Ok(()),
            // Mirrors the real bootstrap flow's state by the time the guard
            // runs: the transient passphrase's slot plus the newly-enrolled
            // FIDO2 slot — trivially > 1, so remove_keyslot_guarded proceeds.
            keyslots: RefCell::new(vec![
                KeyslotInfo {
                    keyslot: KeyslotRef(0),
                },
                KeyslotInfo {
                    keyslot: KeyslotRef(1),
                },
            ]),
            log: new_call_log(),
        }
    }

    pub fn failing(missing_deps: &[&str]) -> Self {
        Self {
            prerequisites: Err(missing(missing_deps)),
            keyslots: RefCell::new(Vec::new()),
            log: new_call_log(),
        }
    }

    pub fn with_log(mut self, log: CallLog) -> Self {
        self.log = log;
        self
    }

    pub fn with_keyslots(self, keyslots: Vec<KeyslotInfo>) -> Self {
        *self.keyslots.borrow_mut() = keyslots;
        self
    }
}

impl LuksBackend for FakeLuksBackend {
    fn check_prerequisites(&self) -> Result<(), Vec<String>> {
        self.prerequisites.clone()
    }

    fn bootstrap_format_and_open(
        &self,
        path: &Path,
        name: &str,
        _filesystem: Filesystem,
    ) -> Result<MapperHandle, DomainError> {
        self.log
            .borrow_mut()
            .push("bootstrap_format_and_open".to_string());
        Ok(MapperHandle {
            name: name.to_string(),
            source_path: path.to_path_buf(),
        })
    }

    fn enroll_fido2_key(
        &self,
        _mapper: &MapperHandle,
        _metadata: KeyMetadata,
    ) -> Result<(), DomainError> {
        self.log.borrow_mut().push("enroll_fido2_key".to_string());
        Ok(())
    }

    fn list_fido2_keyslots(&self, _path: &Path) -> Result<Vec<KeyslotInfo>, DomainError> {
        self.log
            .borrow_mut()
            .push("list_fido2_keyslots".to_string());
        Ok(self.keyslots.borrow().clone())
    }

    fn remove_key(&self, _path: &Path, _keyslot: KeyslotRef) -> Result<(), DomainError> {
        self.log.borrow_mut().push("remove_key".to_string());
        Ok(())
    }
}

pub struct FakeFido2Backend(Result<(), Vec<String>>);

impl FakeFido2Backend {
    pub fn passing() -> Self {
        Self(Ok(()))
    }

    pub fn failing(missing_deps: &[&str]) -> Self {
        Self(Err(missing(missing_deps)))
    }
}

impl Fido2Backend for FakeFido2Backend {
    fn check_prerequisites(&self) -> Result<(), Vec<String>> {
        self.0.clone()
    }
}

pub struct FakeFilesystemBackend {
    prerequisites: Result<(), Vec<String>>,
    path_exists: bool,
    log: CallLog,
}

impl FakeFilesystemBackend {
    pub fn passing() -> Self {
        Self {
            prerequisites: Ok(()),
            path_exists: false,
            log: new_call_log(),
        }
    }

    pub fn failing(missing_deps: &[&str]) -> Self {
        Self {
            prerequisites: Err(missing(missing_deps)),
            path_exists: false,
            log: new_call_log(),
        }
    }

    pub fn with_log(mut self, log: CallLog) -> Self {
        self.log = log;
        self
    }

    pub fn with_path_exists(mut self, value: bool) -> Self {
        self.path_exists = value;
        self
    }
}

impl FilesystemBackend for FakeFilesystemBackend {
    fn check_prerequisites(&self) -> Result<(), Vec<String>> {
        self.prerequisites.clone()
    }

    fn path_exists(&self, _path: &Path) -> bool {
        self.log.borrow_mut().push("path_exists".to_string());
        self.path_exists
    }

    fn set_backing_file_size(&self, _path: &Path, _size: u64) -> Result<(), DomainError> {
        self.log
            .borrow_mut()
            .push("set_backing_file_size".to_string());
        Ok(())
    }

    fn mkfs(&self, _mapper: &MapperHandle, _fs: Filesystem) -> Result<(), DomainError> {
        self.log.borrow_mut().push("mkfs".to_string());
        Ok(())
    }
}
