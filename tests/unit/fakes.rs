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
    has_luks2_header: bool,
    log: CallLog,
    fail_at: Option<&'static str>,
    last_bootstrap_size: RefCell<Option<u64>>,
    last_open: RefCell<Option<(std::path::PathBuf, String)>>,
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
            has_luks2_header: false,
            log: new_call_log(),
            fail_at: None,
            last_bootstrap_size: RefCell::new(None),
            last_open: RefCell::new(None),
        }
    }

    pub fn failing(missing_deps: &[&str]) -> Self {
        Self {
            prerequisites: Err(missing(missing_deps)),
            keyslots: RefCell::new(Vec::new()),
            has_luks2_header: false,
            log: new_call_log(),
            fail_at: None,
            last_bootstrap_size: RefCell::new(None),
            last_open: RefCell::new(None),
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

    pub fn with_has_luks2_header(mut self, value: bool) -> Self {
        self.has_luks2_header = value;
        self
    }

    /// The `size` argument most recently passed to `bootstrap_format_and_open`
    /// — lets a test assert AC #1/#2's resolved-size behavior directly,
    /// instead of only the call-log's method-name sequence.
    pub fn last_bootstrap_size(&self) -> Option<u64> {
        *self.last_bootstrap_size.borrow()
    }

    /// The `(path, name)` most recently passed to `open` — lets a test assert
    /// AC #3's exact derived mapping name without re-deriving it by hand.
    pub fn last_open(&self) -> Option<(std::path::PathBuf, String)> {
        self.last_open.borrow().clone()
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

    fn has_luks2_header(&self, _path: &Path) -> Result<bool, DomainError> {
        self.log.borrow_mut().push("has_luks2_header".to_string());
        self.fail_if("has_luks2_header")?;
        Ok(self.has_luks2_header)
    }

    fn bootstrap_format_and_open(
        &self,
        path: &Path,
        name: &str,
        size: u64,
        _filesystem: Filesystem,
    ) -> Result<MapperHandle, DomainError> {
        self.log
            .borrow_mut()
            .push("bootstrap_format_and_open".to_string());
        *self.last_bootstrap_size.borrow_mut() = Some(size);
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

    fn open(&self, path: &Path, name: &str) -> Result<MapperHandle, DomainError> {
        self.log.borrow_mut().push("open".to_string());
        *self.last_open.borrow_mut() = Some((path.to_path_buf(), name.to_string()));
        self.fail_if("open")?;
        Ok(MapperHandle {
            name: name.to_string(),
            source_path: path.to_path_buf(),
        })
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

/// Default fake device capacity: large enough that existing tests which
/// don't care about sizing never accidentally trip `DeviceSizeExceedsCapacity`.
const DEFAULT_DEVICE_CAPACITY: u64 = 1024u64.pow(4);

pub struct FakeFilesystemBackend {
    prerequisites: Result<(), Vec<String>>,
    path_exists: bool,
    device_capacity: u64,
    log: CallLog,
    fail_at: Option<&'static str>,
}

impl FakeFilesystemBackend {
    pub fn passing() -> Self {
        Self {
            prerequisites: Ok(()),
            path_exists: false,
            device_capacity: DEFAULT_DEVICE_CAPACITY,
            log: new_call_log(),
            fail_at: None,
        }
    }

    pub fn failing(missing_deps: &[&str]) -> Self {
        Self {
            prerequisites: Err(missing(missing_deps)),
            path_exists: false,
            device_capacity: DEFAULT_DEVICE_CAPACITY,
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

    pub fn with_device_capacity(mut self, value: u64) -> Self {
        self.device_capacity = value;
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

    fn device_capacity(&self, _path: &Path) -> Result<u64, DomainError> {
        self.log.borrow_mut().push("device_capacity".to_string());
        self.fail_if("device_capacity")?;
        Ok(self.device_capacity)
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
