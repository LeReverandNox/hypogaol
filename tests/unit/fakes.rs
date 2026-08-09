use std::cell::{Cell, RefCell};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use hypogaol::domain::errors::DomainError;
use hypogaol::domain::types::{
    Filesystem, HookFileMeta, KeyMetadata, KeyslotInfo, KeyslotRef, MapperHandle, Pid, Signal,
};
use hypogaol::ports::fido2_backend::{Fido2Backend, Fido2DeviceSelection};
use hypogaol::ports::filesystem_backend::FilesystemBackend;
use hypogaol::ports::luks_backend::LuksBackend;

fn missing(deps: &[&str]) -> Vec<String> {
    deps.iter().map(|dep| dep.to_string()).collect()
}

/// Shared call log so a test can assert cross-port call ordering (Task 8).
pub type CallLog = Rc<RefCell<Vec<String>>>;

pub fn new_call_log() -> CallLog {
    Rc::new(RefCell::new(Vec::new()))
}

/// No-op progress callback for tests that don't assert on stage ordering.
/// Generic over both `CreateStage` and `ResizeStage` via inference at each
/// call site.
pub fn no_progress<S>(_stage: S) {}

pub struct FakeLuksBackend {
    prerequisites: Result<(), Vec<String>>,
    keyslots: RefCell<Vec<KeyslotInfo>>,
    has_luks2_header: bool,
    has_marker_token: bool,
    log: CallLog,
    fail_at: Option<&'static str>,
    last_bootstrap_size: RefCell<Option<u64>>,
    last_open: RefCell<Option<(std::path::PathBuf, String, bool)>>,
    last_removed_keyslot: RefCell<Option<KeyslotRef>>,
    last_close: RefCell<Option<MapperHandle>>,
    last_resize: RefCell<Option<MapperHandle>>,
    read_filesystem: Filesystem,
    // Distinct return values for successive `list_fido2_keyslots` calls, so a
    // test can prove a caller re-reads live state on each call rather than
    // reusing an earlier result (AC #3). `None` means "always return
    // `keyslots`" (the common case).
    keyslots_sequence: RefCell<Option<VecDeque<Vec<KeyslotInfo>>>>,
    // `list_open_mappings`'s return value (Story 4.5) — empty by default, so
    // existing tests that don't care about close-all discovery are unaffected.
    open_mappings: RefCell<Vec<MapperHandle>>,
    // Per-mapping selective `close` failure (Story 4.5, AC #2), keyed by
    // `MapperHandle.name`, checked alongside (not instead of) `fail_at` —
    // `fail_at` alone can't express "mapping A's close fails, mapping B's in
    // the same batch succeeds".
    close_failure_for: RefCell<HashSet<String>>,
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
                key_label: "primary".to_string(),
            }]),
            has_luks2_header: false,
            has_marker_token: false,
            log: new_call_log(),
            fail_at: None,
            last_bootstrap_size: RefCell::new(None),
            last_open: RefCell::new(None),
            last_removed_keyslot: RefCell::new(None),
            last_close: RefCell::new(None),
            last_resize: RefCell::new(None),
            read_filesystem: Filesystem::Ext4,
            keyslots_sequence: RefCell::new(None),
            open_mappings: RefCell::new(Vec::new()),
            close_failure_for: RefCell::new(HashSet::new()),
        }
    }

    pub fn failing(missing_deps: &[&str]) -> Self {
        Self {
            prerequisites: Err(missing(missing_deps)),
            keyslots: RefCell::new(Vec::new()),
            has_luks2_header: false,
            has_marker_token: false,
            log: new_call_log(),
            fail_at: None,
            last_bootstrap_size: RefCell::new(None),
            last_open: RefCell::new(None),
            last_removed_keyslot: RefCell::new(None),
            last_close: RefCell::new(None),
            last_resize: RefCell::new(None),
            read_filesystem: Filesystem::Ext4,
            keyslots_sequence: RefCell::new(None),
            open_mappings: RefCell::new(Vec::new()),
            close_failure_for: RefCell::new(HashSet::new()),
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

    /// Returns each snapshot in `sequence` on successive `list_fido2_keyslots`
    /// calls (one call consumes one snapshot), falling back to `keyslots`
    /// once the sequence is exhausted — lets a test prove a caller re-reads
    /// live state on each call instead of reusing an earlier result (AC #3),
    /// by handing back a *different* answer on the second call.
    pub fn with_keyslots_sequence(self, sequence: Vec<Vec<KeyslotInfo>>) -> Self {
        *self.keyslots_sequence.borrow_mut() = Some(sequence.into());
        self
    }

    pub fn with_has_luks2_header(mut self, value: bool) -> Self {
        self.has_luks2_header = value;
        self
    }

    pub fn with_has_marker_token(mut self, value: bool) -> Self {
        self.has_marker_token = value;
        self
    }

    /// The `size` argument most recently passed to `bootstrap_format_and_open`
    /// — lets a test assert AC #1/#2's resolved-size behavior directly,
    /// instead of only the call-log's method-name sequence.
    pub fn last_bootstrap_size(&self) -> Option<u64> {
        *self.last_bootstrap_size.borrow()
    }

    /// The `(path, name, read_only)` most recently passed to `open` — lets a
    /// test assert AC #3's exact derived mapping name, and Story 3.3's
    /// propagated `read_only` bool, without re-deriving them by hand.
    pub fn last_open(&self) -> Option<(std::path::PathBuf, String, bool)> {
        self.last_open.borrow().clone()
    }

    /// The `KeyslotRef` most recently passed to `remove_key` — lets a test
    /// assert *which* keyslot label-to-keyslot resolution actually picked.
    pub fn last_removed_keyslot(&self) -> Option<KeyslotRef> {
        *self.last_removed_keyslot.borrow()
    }

    /// The `MapperHandle` most recently passed to `close` — lets a test
    /// assert `close::run` built the mapper from the same derived mapping
    /// name it also passed to `umount`.
    pub fn last_close(&self) -> Option<MapperHandle> {
        self.last_close.borrow().clone()
    }

    /// The `MapperHandle` most recently passed to `resize` — lets a test
    /// assert `resize::run` called it against the same mapper `open`
    /// returned.
    pub fn last_resize(&self) -> Option<MapperHandle> {
        self.last_resize.borrow().clone()
    }

    /// The `Filesystem` `read_filesystem` returns — settable so a test can
    /// prove `growfs` is called with whatever `read_filesystem` reports.
    pub fn with_read_filesystem(mut self, filesystem: Filesystem) -> Self {
        self.read_filesystem = filesystem;
        self
    }

    /// Makes the named port call log itself as usual, then return an
    /// `AdapterFailure` instead of succeeding — for exercising `create::run`'s
    /// failure-cleanup paths.
    pub fn with_failure_at(mut self, call: &'static str) -> Self {
        self.fail_at = Some(call);
        self
    }

    /// The `Vec<MapperHandle>` `list_open_mappings` returns (Story 4.5,
    /// default empty).
    pub fn with_open_mappings(self, mappings: Vec<MapperHandle>) -> Self {
        *self.open_mappings.borrow_mut() = mappings;
        self
    }

    /// Makes `close` fail only for the mapping named `name`, leaving every
    /// other mapping's `close` call in the same batch unaffected — see the
    /// `close_failure_for` field doc for why `with_failure_at("close")` alone
    /// can't express this (Story 4.5, AC #2).
    pub fn with_close_failure_for(self, name: &str) -> Self {
        self.close_failure_for.borrow_mut().insert(name.to_string());
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

    fn has_marker_token(&self, _path: &Path) -> Result<bool, DomainError> {
        self.log.borrow_mut().push("has_marker_token".to_string());
        self.fail_if("has_marker_token")?;
        Ok(self.has_marker_token)
    }

    fn remove_marker_token(&self, _path: &Path) -> Result<(), DomainError> {
        self.log
            .borrow_mut()
            .push("remove_marker_token".to_string());
        self.fail_if("remove_marker_token")
    }

    fn close_stale_mapping(&self, _name: &str) -> Result<(), DomainError> {
        self.log
            .borrow_mut()
            .push("close_stale_mapping".to_string());
        self.fail_if("close_stale_mapping")
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
        if let Some(next) = self
            .keyslots_sequence
            .borrow_mut()
            .as_mut()
            .and_then(VecDeque::pop_front)
        {
            return Ok(next);
        }
        Ok(self.keyslots.borrow().clone())
    }

    fn remove_key(&self, _path: &Path, keyslot: KeyslotRef) -> Result<(), DomainError> {
        self.log.borrow_mut().push("remove_key".to_string());
        *self.last_removed_keyslot.borrow_mut() = Some(keyslot);
        self.fail_if("remove_key")
    }

    fn close(&self, mapper: &MapperHandle) -> Result<(), DomainError> {
        self.log.borrow_mut().push("close".to_string());
        *self.last_close.borrow_mut() = Some(mapper.clone());
        if self.close_failure_for.borrow().contains(&mapper.name) {
            return Err(DomainError::AdapterFailure(format!(
                "close failed (test) for {}",
                mapper.name
            )));
        }
        self.fail_if("close")
    }

    fn open(&self, path: &Path, name: &str, read_only: bool) -> Result<MapperHandle, DomainError> {
        self.log.borrow_mut().push("open".to_string());
        *self.last_open.borrow_mut() = Some((path.to_path_buf(), name.to_string(), read_only));
        self.fail_if("open")?;
        Ok(MapperHandle {
            name: name.to_string(),
            source_path: path.to_path_buf(),
        })
    }

    fn resize(&self, mapper: &MapperHandle) -> Result<(), DomainError> {
        self.log.borrow_mut().push("resize".to_string());
        *self.last_resize.borrow_mut() = Some(mapper.clone());
        self.fail_if("resize")
    }

    fn read_filesystem(&self, _path: &Path) -> Result<Filesystem, DomainError> {
        self.log.borrow_mut().push("read_filesystem".to_string());
        self.fail_if("read_filesystem")?;
        Ok(self.read_filesystem)
    }

    fn list_open_mappings(&self) -> Result<Vec<MapperHandle>, DomainError> {
        self.log.borrow_mut().push("list_open_mappings".to_string());
        self.fail_if("list_open_mappings")?;
        Ok(self.open_mappings.borrow().clone())
    }
}

pub struct FakeFido2Backend {
    prerequisites: Result<(), Vec<String>>,
    log: CallLog,
    fail_at: Option<&'static str>,
    user_verification_received: Cell<Option<bool>>,
    key_label_received: RefCell<Option<String>>,
}

impl FakeFido2Backend {
    pub fn passing() -> Self {
        Self {
            prerequisites: Ok(()),
            log: new_call_log(),
            fail_at: None,
            user_verification_received: Cell::new(None),
            key_label_received: RefCell::new(None),
        }
    }

    pub fn failing(missing_deps: &[&str]) -> Self {
        Self {
            prerequisites: Err(missing(missing_deps)),
            log: new_call_log(),
            fail_at: None,
            user_verification_received: Cell::new(None),
            key_label_received: RefCell::new(None),
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

    /// The `user_verification` value most recently passed to
    /// `enroll_fido2_key` — lets a test assert the CLI flag/workflow
    /// parameter actually reached the port (Task 8).
    pub fn user_verification_received(&self) -> Option<bool> {
        self.user_verification_received.get()
    }

    /// The `metadata.key_label` most recently passed to `enroll_fido2_key` —
    /// lets a test assert a caller-supplied (or defaulted) label actually
    /// reached the port (Story 6.2, Task 3).
    pub fn key_label_received(&self) -> Option<String> {
        self.key_label_received.borrow().clone()
    }
}

impl Fido2Backend for FakeFido2Backend {
    fn check_prerequisites(&self) -> Result<(), Vec<String>> {
        self.prerequisites.clone()
    }

    fn enroll_fido2_key(
        &self,
        _mapper: &MapperHandle,
        metadata: KeyMetadata,
        _selection: Fido2DeviceSelection,
        user_verification: bool,
    ) -> Result<(), DomainError> {
        self.log.borrow_mut().push("enroll_fido2_key".to_string());
        self.user_verification_received.set(Some(user_verification));
        *self.key_label_received.borrow_mut() = Some(metadata.key_label.clone());
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
    is_block_device: bool,
    device_capacity: u64,
    // Distinct return value for a `device_capacity` call against a mapper's
    // own device node (`/dev/mapper/...`) as opposed to the raw backing
    // path — `resize::run`'s tier 2 calls `device_capacity` on the mapper
    // node specifically to derive the LUKS2 header size (raw capacity minus
    // mapper/payload capacity), which is only a meaningful distinction if
    // the fake can actually return two different values for the two call
    // sites. Defaults to `device_capacity`'s own value (header size 0) so
    // existing tests that don't model a header are unaffected.
    mapper_capacity: Option<u64>,
    // `filesystem_size`'s return value — deliberately a separate field from
    // `device_capacity`, not derived from it: `resize::run`'s tier 2 reads
    // this instead of the mapping's raw size specifically because the two
    // can differ for a device-backed volume using Story 1.6 headroom
    // (confirmed empirically on real hardware — see `filesystem_size`'s own
    // port doc). Defaults to `device_capacity`'s own value so existing
    // tests that don't care about the distinction are unaffected.
    filesystem_size: Option<u64>,
    log: CallLog,
    fail_at: Option<&'static str>,
    umount_not_currently_mounted: bool,
    last_umount: RefCell<Option<MapperHandle>>,
    last_growfs_filesystem: RefCell<Option<Filesystem>>,
    last_mount_read_only: RefCell<Option<bool>>,
    // Story 4.4 (per-volume bind-hooks/exec-hooks automation): six new
    // `FilesystemBackend` methods' fake state.
    hook_file_metadata: Cell<HookFileMeta>,
    invoking_home_dir: PathBuf,
    mount_point_of: RefCell<Option<PathBuf>>,
    bind_mount_failure: bool,
    run_hook_exit_code: Cell<Option<i32>>,
    last_run_hook: RefCell<Option<(PathBuf, Vec<String>)>>,
    // Distinct return values for successive `path_exists` calls (one call
    // consumes one value), falling back to `path_exists` once exhausted —
    // same "prove a caller checks two different paths independently"
    // convention as `FakeLuksBackend::keyslots_sequence`. Needed because
    // `resolve_bind_hook_entry` checks a source and a dest path with two
    // separate `path_exists` calls that must be able to disagree (e.g. AC
    // #2's "source exists but dest doesn't" case).
    path_exists_sequence: RefCell<Option<VecDeque<bool>>>,
    // Per-mapping selective `umount` failure (Story 4.5, AC #2), keyed by
    // `MapperHandle.name`, checked alongside (not instead of) `fail_at`/
    // `umount_not_currently_mounted` — same rationale as
    // `FakeLuksBackend::close_failure_for`.
    umount_failure_for: RefCell<HashSet<String>>,
    // Story 4.6 (slam): `processes_using`'s return value — empty by default,
    // so existing tests that don't care about slam's escalation are
    // unaffected.
    processes_using_result: RefCell<Vec<Pid>>,
    // Story 4.6: each `umount` call decrements this (while > 0) and returns a
    // generic busy `AdapterFailure` — distinct from `umount_not_currently_mounted`,
    // which must look like a real busy-mount failure, not the idempotent-retry
    // marker. Once it reaches 0, `umount` falls through to the existing checks
    // unaffected. Default 0 means zero existing tests change behavior.
    umount_fail_times: Cell<u32>,
    // Story 4.6: every `(Pid, Signal)` passed to `signal_process`, in order.
    last_signal_calls: RefCell<Vec<(Pid, Signal)>>,
    // Story 6.3: the `mountpoint` most recently passed to
    // `scaffold_hook_templates` — lets a test prove `create::run` threaded
    // `mount`'s own return value through, not a stand-in.
    last_scaffold_hook_templates_mountpoint: RefCell<Option<PathBuf>>,
    // Story 6.4: every `filesystem` argument passed to `check_prerequisites`,
    // in call order — lets a test prove `preflight::check` forwards it
    // unchanged, and that `resize::run` calls it twice with the right values.
    check_prerequisites_filesystem_calls: RefCell<Vec<Option<Filesystem>>>,
    // Story 6.4 (review finding, 2026-08-09): lets `check_prerequisites` fail
    // only for one specific `filesystem` argument, leaving every other call
    // (including the unconditional `None` call every workflow makes first)
    // returning `prerequisites`'s own value unchanged. Without this, no unit
    // test could prove resize's second, type-specific preflight call is
    // independently load-bearing — `.failing(&[...])` fails *every* call,
    // so a regression that silently no-ops the second call went undetected.
    fail_for_filesystem: RefCell<Option<(Option<Filesystem>, Vec<String>)>>,
}

/// A valid, unrejectable `exec-hooks` file's metadata (AC #3's guardrail
/// passes on every check) — the default `hook_file_metadata` returns, so
/// tests that don't care about the guardrail never trip it by accident.
fn passing_hook_file_meta() -> HookFileMeta {
    HookFileMeta {
        is_regular_file: true,
        is_symlink: false,
        is_executable: true,
        owned_by_invoking_user_or_root: true,
        is_world_writable: false,
    }
}

impl FakeFilesystemBackend {
    pub fn passing() -> Self {
        Self {
            prerequisites: Ok(()),
            path_exists: false,
            is_block_device: false,
            device_capacity: DEFAULT_DEVICE_CAPACITY,
            mapper_capacity: None,
            filesystem_size: None,
            log: new_call_log(),
            fail_at: None,
            umount_not_currently_mounted: false,
            last_umount: RefCell::new(None),
            last_growfs_filesystem: RefCell::new(None),
            last_mount_read_only: RefCell::new(None),
            hook_file_metadata: Cell::new(passing_hook_file_meta()),
            invoking_home_dir: PathBuf::from("/home/fake-user"),
            mount_point_of: RefCell::new(None),
            bind_mount_failure: false,
            run_hook_exit_code: Cell::new(Some(0)),
            last_run_hook: RefCell::new(None),
            path_exists_sequence: RefCell::new(None),
            umount_failure_for: RefCell::new(HashSet::new()),
            processes_using_result: RefCell::new(Vec::new()),
            umount_fail_times: Cell::new(0),
            last_signal_calls: RefCell::new(Vec::new()),
            last_scaffold_hook_templates_mountpoint: RefCell::new(None),
            check_prerequisites_filesystem_calls: RefCell::new(Vec::new()),
            fail_for_filesystem: RefCell::new(None),
        }
    }

    pub fn failing(missing_deps: &[&str]) -> Self {
        Self {
            prerequisites: Err(missing(missing_deps)),
            path_exists: false,
            is_block_device: false,
            device_capacity: DEFAULT_DEVICE_CAPACITY,
            mapper_capacity: None,
            filesystem_size: None,
            log: new_call_log(),
            fail_at: None,
            umount_not_currently_mounted: false,
            last_umount: RefCell::new(None),
            last_growfs_filesystem: RefCell::new(None),
            last_mount_read_only: RefCell::new(None),
            hook_file_metadata: Cell::new(passing_hook_file_meta()),
            invoking_home_dir: PathBuf::from("/home/fake-user"),
            mount_point_of: RefCell::new(None),
            bind_mount_failure: false,
            run_hook_exit_code: Cell::new(Some(0)),
            last_run_hook: RefCell::new(None),
            path_exists_sequence: RefCell::new(None),
            umount_failure_for: RefCell::new(HashSet::new()),
            processes_using_result: RefCell::new(Vec::new()),
            umount_fail_times: Cell::new(0),
            last_signal_calls: RefCell::new(Vec::new()),
            last_scaffold_hook_templates_mountpoint: RefCell::new(None),
            check_prerequisites_filesystem_calls: RefCell::new(Vec::new()),
            fail_for_filesystem: RefCell::new(None),
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

    /// Returns each value in `sequence` on successive `path_exists` calls
    /// (one call consumes one value), falling back to `path_exists`'s own
    /// value once exhausted — lets a test make a source path "exist" while a
    /// dest path (or vice versa) doesn't, since `resolve_bind_hook_entry`
    /// checks both independently.
    pub fn with_path_exists_sequence(self, sequence: Vec<bool>) -> Self {
        *self.path_exists_sequence.borrow_mut() = Some(sequence.into());
        self
    }

    pub fn with_is_block_device(mut self, value: bool) -> Self {
        self.is_block_device = value;
        self
    }

    pub fn with_device_capacity(mut self, value: u64) -> Self {
        self.device_capacity = value;
        self
    }

    /// See `mapper_capacity` field doc — sets the value `device_capacity`
    /// returns specifically when called against a mapper's own device node,
    /// distinct from `device_capacity`'s own (raw-path) value.
    pub fn with_mapper_capacity(mut self, value: u64) -> Self {
        self.mapper_capacity = Some(value);
        self
    }

    /// See `filesystem_size` field doc — sets the value `filesystem_size`
    /// returns, distinct from `device_capacity`'s own.
    pub fn with_filesystem_size(mut self, value: u64) -> Self {
        self.filesystem_size = Some(value);
        self
    }

    pub fn with_failure_at(mut self, call: &'static str) -> Self {
        self.fail_at = Some(call);
        self
    }

    /// Makes `umount` fail with the same "not currently mounted" marker the
    /// real `ExecAdapter` produces — distinct from `with_failure_at("umount")`'s
    /// generic failure, so a test can exercise `close::run`'s idempotent-retry
    /// path (review finding, 2026-07-26).
    pub fn with_umount_not_currently_mounted(mut self) -> Self {
        self.umount_not_currently_mounted = true;
        self
    }

    /// Makes `umount` fail only for the mapping named `name`, leaving every
    /// other mapping's `umount` call in the same batch unaffected — same
    /// rationale as `FakeLuksBackend::with_close_failure_for` (Story 4.5, AC #2).
    pub fn with_umount_failure_for(self, name: &str) -> Self {
        self.umount_failure_for
            .borrow_mut()
            .insert(name.to_string());
        self
    }

    /// The `MapperHandle` most recently passed to `umount` — lets a test
    /// assert `close::run` built the mapper from the same derived mapping
    /// name it also passed to `close`.
    pub fn last_umount(&self) -> Option<MapperHandle> {
        self.last_umount.borrow().clone()
    }

    /// The `Filesystem` most recently passed to `growfs` — lets a test prove
    /// `resize::run` threads `read_filesystem`'s result into `growfs` rather
    /// than hardcoding a variant.
    pub fn last_growfs_filesystem(&self) -> Option<Filesystem> {
        *self.last_growfs_filesystem.borrow()
    }

    /// The `read_only` bool most recently passed to `mount` — lets a test
    /// assert Story 3.3's propagated flag independent of the happy-path
    /// return value/log.
    pub fn last_mount_read_only(&self) -> Option<bool> {
        *self.last_mount_read_only.borrow()
    }

    /// The `HookFileMeta` `hook_file_metadata` returns — settable so a test
    /// can drive every `HookRejectionReason` branch (Task 10).
    pub fn with_hook_file_metadata(self, meta: HookFileMeta) -> Self {
        self.hook_file_metadata.set(meta);
        self
    }

    /// The `PathBuf` `invoking_home_dir` returns.
    pub fn with_invoking_home_dir(mut self, home: PathBuf) -> Self {
        self.invoking_home_dir = home;
        self
    }

    /// The `PathBuf` `mount_point_of` returns — independent of ever calling
    /// `mount`, since `close`'s tests need it without an `unlock` in the
    /// picture.
    pub fn with_mount_point_of(self, mountpoint: PathBuf) -> Self {
        *self.mount_point_of.borrow_mut() = Some(mountpoint);
        self
    }

    /// Makes `bind_mount` log its call then return an `AdapterFailure`
    /// instead of succeeding.
    pub fn with_bind_mount_failure(mut self) -> Self {
        self.bind_mount_failure = true;
        self
    }

    /// The exit code `run_hook`'s returned `ExitStatus` reports — `Some(0)`
    /// by default (success). `None` simulates a hook killed by a signal
    /// (`ExitStatus::code()` returns `None` in that case too).
    pub fn with_run_hook_exit_status(self, code: Option<i32>) -> Self {
        self.run_hook_exit_code.set(code);
        self
    }

    /// The `(path, args)` most recently passed to `run_hook` — lets a test
    /// assert the exact `open`/`close` argv a hook was invoked with.
    pub fn last_run_hook(&self) -> Option<(PathBuf, Vec<String>)> {
        self.last_run_hook.borrow().clone()
    }

    fn fail_if(&self, call: &'static str) -> Result<(), DomainError> {
        if self.fail_at == Some(call) {
            Err(DomainError::AdapterFailure(format!("{call} failed (test)")))
        } else {
            Ok(())
        }
    }

    /// The `Vec<Pid>` `processes_using` returns (Story 4.6, default empty).
    pub fn with_processes_using(self, pids: Vec<Pid>) -> Self {
        *self.processes_using_result.borrow_mut() = pids;
        self
    }

    /// Makes the next `times` `umount` calls fail with a generic busy
    /// `AdapterFailure` (not the `"not currently mounted"` marker), then fall
    /// through to the existing checks once exhausted — lets a test drive
    /// slam's escalation loop through a fixed number of busy rounds before
    /// clearing (Story 4.6).
    pub fn with_umount_fail_times(self, times: u32) -> Self {
        self.umount_fail_times.set(times);
        self
    }

    /// Every `(Pid, Signal)` passed to `signal_process`, in call order.
    pub fn signal_calls(&self) -> Vec<(Pid, Signal)> {
        self.last_signal_calls.borrow().clone()
    }

    /// The `mountpoint` most recently passed to `scaffold_hook_templates`.
    pub fn last_scaffold_hook_templates_mountpoint(&self) -> Option<PathBuf> {
        self.last_scaffold_hook_templates_mountpoint
            .borrow()
            .clone()
    }

    /// Every `filesystem` argument passed to `check_prerequisites`, in call
    /// order — lets a test inspect exactly what `preflight::check` forwarded.
    pub fn check_prerequisites_filesystem_calls(&self) -> Vec<Option<Filesystem>> {
        self.check_prerequisites_filesystem_calls.borrow().clone()
    }

    /// Makes `check_prerequisites` fail only when called with exactly this
    /// `filesystem` argument — every other call (including the unconditional
    /// `None` call every workflow makes first) still returns `prerequisites`'s
    /// own value. Lets a test build the case `.failing(&[...])` alone can't:
    /// the first (generic) preflight call passes, the second (type-specific)
    /// one fails.
    pub fn with_failure_for_filesystem(
        self,
        filesystem: Option<Filesystem>,
        missing_deps: &[&str],
    ) -> Self {
        *self.fail_for_filesystem.borrow_mut() = Some((filesystem, missing(missing_deps)));
        self
    }
}

impl FilesystemBackend for FakeFilesystemBackend {
    fn check_prerequisites(&self, filesystem: Option<Filesystem>) -> Result<(), Vec<String>> {
        self.log
            .borrow_mut()
            .push("check_prerequisites".to_string());
        self.check_prerequisites_filesystem_calls
            .borrow_mut()
            .push(filesystem);
        if let Some((target, missing_deps)) = self.fail_for_filesystem.borrow().as_ref() {
            if *target == filesystem {
                return Err(missing_deps.clone());
            }
        }
        self.prerequisites.clone()
    }

    fn path_exists(&self, _path: &Path) -> bool {
        self.log.borrow_mut().push("path_exists".to_string());
        if let Some(next) = self
            .path_exists_sequence
            .borrow_mut()
            .as_mut()
            .and_then(VecDeque::pop_front)
        {
            return next;
        }
        self.path_exists
    }

    fn is_block_device(&self, _path: &Path) -> Result<bool, DomainError> {
        self.log.borrow_mut().push("is_block_device".to_string());
        self.fail_if("is_block_device")?;
        Ok(self.is_block_device)
    }

    fn device_capacity(&self, path: &Path) -> Result<u64, DomainError> {
        self.log.borrow_mut().push("device_capacity".to_string());
        self.fail_if("device_capacity")?;
        if path.starts_with("/dev/mapper") {
            Ok(self.mapper_capacity.unwrap_or(self.device_capacity))
        } else {
            Ok(self.device_capacity)
        }
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

    fn growfs(&self, _mapper: &MapperHandle, fs: Filesystem) -> Result<(), DomainError> {
        self.log.borrow_mut().push("growfs".to_string());
        *self.last_growfs_filesystem.borrow_mut() = Some(fs);
        self.fail_if("growfs")
    }

    fn filesystem_size(&self, _mapper: &MapperHandle, _fs: Filesystem) -> Result<u64, DomainError> {
        self.log.borrow_mut().push("filesystem_size".to_string());
        self.fail_if("filesystem_size")?;
        Ok(self.filesystem_size.unwrap_or(self.device_capacity))
    }

    fn remove_backing_file(&self, _path: &Path) -> Result<(), DomainError> {
        self.log
            .borrow_mut()
            .push("remove_backing_file".to_string());
        Ok(())
    }

    fn mount(
        &self,
        mapper: &MapperHandle,
        read_only: bool,
    ) -> Result<std::path::PathBuf, DomainError> {
        self.log.borrow_mut().push("mount".to_string());
        *self.last_mount_read_only.borrow_mut() = Some(read_only);
        self.fail_if("mount")?;
        Ok(std::path::PathBuf::from(format!(
            "/tmp/fake-mount-{}",
            mapper.name
        )))
    }

    fn umount(&self, mapper: &MapperHandle) -> Result<(), DomainError> {
        self.log.borrow_mut().push("umount".to_string());
        *self.last_umount.borrow_mut() = Some(mapper.clone());
        if self.umount_fail_times.get() > 0 {
            self.umount_fail_times.set(self.umount_fail_times.get() - 1);
            return Err(DomainError::AdapterFailure(format!(
                "{} is busy (test)",
                mapper.name
            )));
        }
        if self.umount_not_currently_mounted {
            return Err(DomainError::AdapterFailure(format!(
                "{} is not currently mounted",
                mapper.name
            )));
        }
        if self.umount_failure_for.borrow().contains(&mapper.name) {
            return Err(DomainError::AdapterFailure(format!(
                "umount failed (test) for {}",
                mapper.name
            )));
        }
        self.fail_if("umount")
    }

    fn bind_mount(&self, _source: &Path, _dest: &Path) -> Result<(), DomainError> {
        self.log.borrow_mut().push("bind_mount".to_string());
        if self.bind_mount_failure {
            return Err(DomainError::AdapterFailure(
                "bind_mount failed (test)".to_string(),
            ));
        }
        self.fail_if("bind_mount")
    }

    fn hook_file_metadata(&self, _path: &Path) -> Result<HookFileMeta, DomainError> {
        self.log.borrow_mut().push("hook_file_metadata".to_string());
        self.fail_if("hook_file_metadata")?;
        Ok(self.hook_file_metadata.get())
    }

    fn run_hook(
        &self,
        path: &Path,
        args: &[&str],
    ) -> Result<std::process::ExitStatus, DomainError> {
        self.log.borrow_mut().push("run_hook".to_string());
        *self.last_run_hook.borrow_mut() = Some((
            path.to_path_buf(),
            args.iter().map(|s| s.to_string()).collect(),
        ));
        self.fail_if("run_hook")?;
        Ok(exit_status_with_code(self.run_hook_exit_code.get()))
    }

    fn invoking_home_dir(&self) -> Result<PathBuf, DomainError> {
        self.log.borrow_mut().push("invoking_home_dir".to_string());
        self.fail_if("invoking_home_dir")?;
        Ok(self.invoking_home_dir.clone())
    }

    fn mount_point_of(&self, mapper: &MapperHandle) -> Result<PathBuf, DomainError> {
        self.log.borrow_mut().push("mount_point_of".to_string());
        self.fail_if("mount_point_of")?;
        Ok(self
            .mount_point_of
            .borrow()
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!("/tmp/fake-mount-{}", mapper.name))))
    }

    fn unmount_bind_hook_destination(&self, _dest: &Path) -> Result<(), DomainError> {
        self.log
            .borrow_mut()
            .push("unmount_bind_hook_destination".to_string());
        self.fail_if("unmount_bind_hook_destination")
    }

    fn processes_using(&self, _mountpoint: &Path) -> Result<Vec<Pid>, DomainError> {
        self.log.borrow_mut().push("processes_using".to_string());
        self.fail_if("processes_using")?;
        Ok(self.processes_using_result.borrow().clone())
    }

    fn signal_process(&self, pid: Pid, signal: Signal) -> Result<(), DomainError> {
        self.log.borrow_mut().push("signal_process".to_string());
        self.last_signal_calls.borrow_mut().push((pid, signal));
        self.fail_if("signal_process")
    }

    fn scaffold_hook_templates(&self, mountpoint: &Path) -> Result<(), DomainError> {
        self.log
            .borrow_mut()
            .push("scaffold_hook_templates".to_string());
        *self.last_scaffold_hook_templates_mountpoint.borrow_mut() = Some(mountpoint.to_path_buf());
        self.fail_if("scaffold_hook_templates")
    }
}

/// Builds a real `ExitStatus` reporting `code` (`None` simulates termination
/// by a signal, matching `ExitStatus::code()`'s own `None` case) — this
/// project only targets Linux (Cargo.toml), so `ExitStatusExt::from_raw`'s
/// wait-status encoding is always available.
fn exit_status_with_code(code: Option<i32>) -> std::process::ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    match code {
        Some(code) => std::process::ExitStatus::from_raw((code & 0xff) << 8),
        None => std::process::ExitStatus::from_raw(9),
    }
}
