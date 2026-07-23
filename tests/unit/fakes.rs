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
    fail_at: Option<&'static str>,
}

impl FakeLuksBackend {
    pub fn passing() -> Self {
        Self {
            prerequisites: Ok(()),
            // Mirrors the real bootstrap flow's state by the time the guard
            // runs: only the newly-enrolled FIDO2 slot has a systemd-fido2
            // token — the transient bootstrap passphrase slot (0) never
            // does, so it's absent here too (confirmed against real
            // hardware; see keyslot_guard.rs's target_is_valid check).
            keyslots: RefCell::new(vec![KeyslotInfo {
                keyslot: KeyslotRef(1),
            }]),
            log: new_call_log(),
            fail_at: None,
        }
    }

    pub fn failing(missing_deps: &[&str]) -> Self {
        Self {
            prerequisites: Err(missing(missing_deps)),
            keyslots: RefCell::new(Vec::new()),
            log: new_call_log(),
            fail_at: None,
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

    /// Makes the named port call log itself as usual, then return an
    /// `AdapterFailure` instead of succeeding — for exercising `create::run`'s
    /// failure-cleanup paths.
    pub fn with_failure_at(mut self, call: &'static str) -> Self {
        self.fail_at = Some(call);
        self
    }

    fn fail_if(&self, call: &'static str) -> Result<(), DomainError> {
        if self.fail_at == Some(call) {
            Err(DomainError::AdapterFailure(format!("{call} failed (test)")))
        } else {
            Ok(())
        }
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
        self.fail_if("bootstrap_format_and_open")?;
        Ok(MapperHandle {
            name: name.to_string(),
            source_path: path.to_path_buf(),
        })
    }

    fn list_fido2_keyslots(&self, _path: &Path) -> Result<Vec<KeyslotInfo>, DomainError> {
        self.log
            .borrow_mut()
            .push("list_fido2_keyslots".to_string());
        self.fail_if("list_fido2_keyslots")?;
        Ok(self.keyslots.borrow().clone())
    }

    fn remove_key(&self, _path: &Path, _keyslot: KeyslotRef) -> Result<(), DomainError> {
        self.log.borrow_mut().push("remove_key".to_string());
        self.fail_if("remove_key")
    }

    fn close(&self, _mapper: &MapperHandle) -> Result<(), DomainError> {
        self.log.borrow_mut().push("close".to_string());
        self.fail_if("close")
    }
}

pub struct FakeFido2Backend {
    prerequisites: Result<(), Vec<String>>,
    log: CallLog,
    fail_at: Option<&'static str>,
}

impl FakeFido2Backend {
    pub fn passing() -> Self {
        Self {
            prerequisites: Ok(()),
            log: new_call_log(),
            fail_at: None,
        }
    }

    pub fn failing(missing_deps: &[&str]) -> Self {
        Self {
            prerequisites: Err(missing(missing_deps)),
            log: new_call_log(),
            fail_at: None,
        }
    }

    pub fn with_log(mut self, log: CallLog) -> Self {
        self.log = log;
        self
    }

    pub fn with_failure_at(mut self, call: &'static str) -> Self {
        self.fail_at = Some(call);
        self
    }
}

impl Fido2Backend for FakeFido2Backend {
    fn check_prerequisites(&self) -> Result<(), Vec<String>> {
        self.prerequisites.clone()
    }

    fn enroll_fido2_key(
        &self,
        _mapper: &MapperHandle,
        _metadata: KeyMetadata,
    ) -> Result<(), DomainError> {
        self.log.borrow_mut().push("enroll_fido2_key".to_string());
        if self.fail_at == Some("enroll_fido2_key") {
            return Err(DomainError::AdapterFailure(
                "enroll_fido2_key failed (test)".to_string(),
            ));
        }
        Ok(())
    }
}

pub struct FakeFilesystemBackend {
    prerequisites: Result<(), Vec<String>>,
    path_exists: bool,
    log: CallLog,
    fail_at: Option<&'static str>,
}

impl FakeFilesystemBackend {
    pub fn passing() -> Self {
        Self {
            prerequisites: Ok(()),
            path_exists: false,
            log: new_call_log(),
            fail_at: None,
        }
    }

    pub fn failing(missing_deps: &[&str]) -> Self {
        Self {
            prerequisites: Err(missing(missing_deps)),
            path_exists: false,
            log: new_call_log(),
            fail_at: None,
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

    pub fn with_failure_at(mut self, call: &'static str) -> Self {
        self.fail_at = Some(call);
        self
    }

    fn fail_if(&self, call: &'static str) -> Result<(), DomainError> {
        if self.fail_at == Some(call) {
            Err(DomainError::AdapterFailure(format!("{call} failed (test)")))
        } else {
            Ok(())
        }
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
        self.fail_if("set_backing_file_size")
    }

    fn mkfs(&self, _mapper: &MapperHandle, _fs: Filesystem) -> Result<(), DomainError> {
        self.log.borrow_mut().push("mkfs".to_string());
        self.fail_if("mkfs")
    }

    fn remove_backing_file(&self, _path: &Path) -> Result<(), DomainError> {
        self.log
            .borrow_mut()
            .push("remove_backing_file".to_string());
        Ok(())
    }
}
