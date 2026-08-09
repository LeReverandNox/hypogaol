---
baseline_commit: 3f630dbf17189f62150bcb11f8aea83e5553f208
---

# Story 6.5: Concurrent-Invocation Guard

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a user,
I want the tool to prevent two simultaneous invocations from racing against the same volume,
so that I can never accidentally corrupt state or bypass a safety guard like the last-keyslot protection.

## Acceptance Criteria

1. **Given** a volume with exactly two valid keyslots, **when** I run two `revoke` invocations concurrently, each targeting a different key, **then** only one succeeds — the second either fails fast with a clear "another operation is already in progress" error, or serializes cleanly behind the first, never both succeeding. [Source: epics.md#Story 6.5, lines 876-880]
2. **Given** any mutating workflow (`create`, `enroll`, `revoke`, `close`, `resize`), **when** it runs, **then** it acquires a non-blocking flock-based lock on the target as its second step, immediately after preflight passes. [Source: epics.md#Story 6.5, lines 882-884]
3. **Given** `close-all` or `slam` processing several open volumes, **when** each mapping is processed, **then** a separate lock is acquired and released per mapping, never one lock held for the whole batch — one mapping's contention is only that mapping's own failure. [Source: epics.md#Story 6.5, lines 886-888]
4. **Given** `info` or `unlock` (including read-only unlock), **when** they run, **then** they acquire no lock at all — these are excluded from the guard by design. [Source: epics.md#Story 6.5, lines 890-892]
5. **Given** the tool crashes or exits mid-operation while holding the lock, **when** a later invocation runs against the same target, **then** it proceeds normally — the kernel releases the lock automatically on process exit, with no stale-lock state to detect or clean up. [Source: epics.md#Story 6.5, lines 894-896]

## Tasks / Subtasks

- [ ] **Task 0: Read every file this story touches before changing anything** (AC: all)
  - Read in full: `src/domain/types.rs` (`Filesystem`, `MapperHandle` — note `MapperHandle.source_path` is already `pub`, the per-mapping lock target for `close_all`/`slam`), `src/domain/errors.rs` (full `DomainError` enum — exhaustively matched by `ux::translate`, so a new variant fails compilation everywhere it isn't handled), `src/domain/mapping_name.rs` (full file — `mapping_name`'s existing canonicalize-or-fail pattern, the direct sibling this story's new helper sits next to), `src/domain/preflight.rs` (confirm `check`'s signature — unchanged by this story, already takes `Option<Filesystem>` from Story 6.4), `src/ports/filesystem_backend.rs` (full trait, 14 methods today — note the doc-comment style to mirror for the 15th), `src/adapters/exec/mod.rs` — specifically `privileged` (~line 54), the `FilesystemBackend impl` block starting ~line 1690 and its `check_prerequisites` (~line 1705), and the file's existing `use std::os::unix::fs::{MetadataExt, PermissionsExt}` import (no `std::os::fd`/`OpenOptionsExt` imports exist yet — this story adds them), `src/domain/workflows/create.rs` (full file — `run`'s single `preflight::check` call and the `CreateTarget` match consuming `target` by value), `src/domain/workflows/enroll.rs`, `revoke.rs`, `close.rs` (full file — note `close_mapping` is `pub(crate)`, shared by `close::run` and `close_all::run`), `close_all.rs`, `resize.rs` (full file — the **two** `preflight::check` calls and exactly where the first one sits relative to `mapping_name::mapping_name`), `slam.rs` (full file — `slam_mapping` is private, used only by `slam::run`, structurally parallel to `close_mapping` but never shared with `close_all`), `unlock.rs`/`info.rs` (confirm neither needs any change — AC #4), `src/cli/main.rs` (confirm its own direct `preflight::check(&adapter, &adapter, &adapter, None)` calls in every `run_*` function are a *separate*, CLI-level pre-check that never calls into `domain::workflows::*` — the lock belongs only inside the `domain` layer, never added at the CLI level), `src/cli/ux.rs` (`translate`'s top-level `match` on lines ~19-100 — an exhaustive match over `DomainError`, the precedent for adding a new arm; distinct from `translate_adapter_failure`'s substring-bucket chain, which only applies to the `AdapterFailure` variant and is *not* where this story's new error goes), `tests/unit/fakes.rs` (`FakeFilesystemBackend`'s full field list, `passing()`/`failing()` constructors, `with_failure_at`/`fail_if` convention, and the `check_prerequisites_filesystem_calls`/`with_failure_for_filesystem` pair added by Story 6.4 — the direct precedent for this story's own per-argument call-capture and selective-failure fields), `tests/unit/workflows.rs` (all 6 `*_stops_at_preflight_before_touching_any_port`/`*_before_reaching_its_own_todo` tests — understand why they still pass unmodified after this story, see Dev Notes), `Cargo.toml`/`Cargo.lock` (confirm `libc 0.2.189` is present as a transitive dependency today, pulled in via `getrandom` — promoting it to a direct dependency adds no new crate to the dependency tree).
  - No spike needed — AD-20 fully specifies the mechanism (`flock(2)`, `LOCK_EX | LOCK_NB`, `O_CLOEXEC`, canonicalize-with-parent-fallback). The concrete Rust-level design gap AD-20 leaves open — **how a single, object-safe `FilesystemBackend` trait method returns something that is a real held OS lock in production but a harmless no-op in the fake** — is resolved below in Dev Notes ("Load-Bearing Design Decision: `LockGuard`'s shape"). Read that section before starting Task 2.

- [ ] **Task 1: Add `DomainError::LockContention`** (AC: #1, #2)
  - `src/domain/errors.rs`: add a new variant, following this enum's existing `#[error(...)]` style:
    ```rust
    #[error("another operation is already in progress on {}", .0.display())]
    LockContention(PathBuf),
    ```
    Placement: anywhere in the enum (order is not semantically significant — `PreflightFailed`/`AdapterFailure` are the nearest thematic neighbors). No new fields needed on any existing variant.

- [ ] **Task 2: Add `LockGuard` to `domain::types`** (AC: #2, #5)
  - `src/domain/types.rs`: add a new struct, publicly holding an `Option<std::os::fd::OwnedFd>` — **the field must be `pub`, not private or `pub(crate)`**: `tests/unit/*.rs` compiles as a separate integration-test crate against the `hypogaol` lib (confirmed via `tests/unit/main.rs`'s `mod` declarations and `tests/unit/fakes.rs`'s `use hypogaol::domain::types::{...}`), so `FakeFilesystemBackend::lock_target` — which lives in that separate crate — must be able to construct `LockGuard(None)` directly, the same way it already constructs other `domain::types` values by field access (`MapperHandle { name, source_path }`'s fields are `pub` for the same reason).
    ```rust
    /// Held for the remainder of a mutating workflow's execution (AD-20).
    /// Dropping it closes the underlying fd, which releases the `flock` the
    /// kernel would also release automatically on process exit — so a crash
    /// mid-operation leaves no stale-lock state to detect or clean up (AC
    /// #5). `None` only for `FakeFilesystemBackend`, which holds no real fd.
    pub struct LockGuard(pub Option<std::os::fd::OwnedFd>);
    ```

- [ ] **Task 3: Add `lock_target_path` to `domain::mapping_name`** (AC: #2)
  - `src/domain/mapping_name.rs`: add a new `pub` function, alongside (not replacing) `mapping_name` — AD-20 explicitly places this "in `domain`, not reimplemented separately" in `adapters::exec`, as a sibling to the existing canonicalize helper:
    ```rust
    /// Resolves `path` to an absolute, canonical form for `lock_target`'s
    /// locking target — `path` itself if it exists, otherwise its parent
    /// directory (AD-20). Unlike `mapping_name`, which requires `path` to
    /// already exist (every one of its callers acts on an already-created
    /// volume), `lock_target` is also called by `create` *before* a
    /// fresh file-backed target exists on disk, so a bare
    /// `std::fs::canonicalize(path)` would always fail there. The
    /// parent-directory fallback over-serializes a genuinely fresh
    /// file-backed create (it blocks unrelated concurrent creates in the
    /// same directory) rather than under-serializing — a deliberate,
    /// acceptable trade-off (AD-20), not a gap: every other workflow's
    /// target already exists by construction, so only that one case ever
    /// takes this fallback branch.
    pub fn lock_target_path(path: &Path) -> Result<PathBuf, DomainError> {
        if let Ok(canonical) = std::fs::canonicalize(path) {
            return Ok(canonical);
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::canonicalize(parent).map_err(|e| {
            DomainError::AdapterFailure(format!(
                "failed to canonicalize {} or its parent directory: {e}",
                path.display()
            ))
        })
    }
    ```
    Needs `use std::path::PathBuf;` added to this file's existing `use std::path::Path;` import.

- [ ] **Task 4: Add `lock_target` to the `FilesystemBackend` trait** (AC: #2)
  - `src/ports/filesystem_backend.rs`: add a 15th trait method, mirroring the existing doc-comment style (e.g. `scaffold_hook_templates`'s, the most recently added):
    ```rust
    /// Acquires a non-blocking, exclusive `flock(2)` lock on `path` (AD-20,
    /// CAP-24) — the second statement of every mutating workflow
    /// (`create`/`enroll`/`revoke`/`close`/`resize`), immediately after
    /// `preflight` passes, and per-mapping inside `close_all`/`slam`'s
    /// existing loops. Returns `DomainError::LockContention` immediately
    /// (never blocks) if another invocation already holds it. The returned
    /// `LockGuard` releases the lock when dropped — hold it for the
    /// remainder of the caller's work; dropping it early re-opens the
    /// race window this method exists to close. `info` and `unlock`
    /// (including read-only unlock) never call this — excluded from the
    /// guard by design (AC #4).
    fn lock_target(&self, path: &Path) -> Result<LockGuard, DomainError>;
    ```
    Add `LockGuard` to this file's existing `use crate::domain::types::{Filesystem, HookFileMeta, MapperHandle, Pid, Signal};` import line.

- [ ] **Task 5: Implement `lock_target` in the real `ExecAdapter`** (AC: #2, #5)
  - `src/adapters/exec/mod.rs`, inside the `FilesystemBackend for ExecAdapter` impl block (near `check_prerequisites`, ~line 1705):
    ```rust
    fn lock_target(&self, path: &Path) -> Result<LockGuard, DomainError> {
        let target = mapping_name::lock_target_path(path)?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_CLOEXEC)
            .open(&target)
            .map_err(|e| {
                DomainError::AdapterFailure(format!(
                    "failed to open {} for locking: {e}",
                    target.display()
                ))
            })?;
        let fd: std::os::fd::OwnedFd = file.into();
        let ret = unsafe { libc::flock(fd.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if ret == 0 {
            Ok(LockGuard(Some(fd)))
        } else {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
                Err(DomainError::LockContention(path.to_path_buf()))
            } else {
                Err(DomainError::AdapterFailure(format!(
                    "failed to lock {}: {err}",
                    target.display()
                )))
            }
        }
    }
    ```
    New imports needed at the top of `src/adapters/exec/mod.rs`: `std::os::fd::{AsRawFd, OwnedFd}` and `std::os::unix::fs::OpenOptionsExt` (for `.custom_flags`) — add alongside the existing `use std::os::unix::fs::{MetadataExt, PermissionsExt};` line. `O_CLOEXEC` on the open call is required, not optional (AD-20 states this explicitly) — a locking fd must never leak into a spawned `cryptsetup`/`mkfs`/etc. subprocess, since an inherited fd in a child process would keep the lock held even after the parent's `LockGuard` drops.
  - `Cargo.toml`: add `libc = "0.2"` to `[dependencies]`, alongside the existing entries (matches the version already pinned transitively in `Cargo.lock` via `getrandom` — `cargo build` should not need to change `Cargo.lock`'s resolved `libc` version, only add it as a direct dependency edge; verify this with `cargo tree -i libc` before and after).

- [ ] **Task 6: Wire the lock into `create`, `enroll`, `revoke`, `close`, `resize`** (AC: #1, #2)
  - `src/domain/workflows/create.rs`: `target: CreateTarget` is consumed by value in the `match target { ... }` block, so the lock target path must be read from it *before* that match. Add a small private helper (both `CreateTarget` variants have a `path` field):
    ```rust
    fn target_path(target: &CreateTarget) -> &Path {
        match target {
            CreateTarget::File { path, .. } => path,
            CreateTarget::Device { path, .. } => path,
        }
    }
    ```
    Then, as `run`'s literal second statement (right after `preflight::check(luks, fido2, fs, Some(filesystem))?;`, before `match target { ... }`):
    ```rust
    let _lock = fs.lock_target(target_path(&target))?;
    ```
    **Binding name matters**: `let _lock = ...`, never `let _ = ...` — the leading underscore silences the unused-variable lint while still binding the value, so the guard stays alive (and the lock held) for the rest of `run`'s scope; `let _ = ...` binds to the wildcard pattern and drops the value immediately, releasing the lock before `match target` even runs and defeating the whole story.
  - `src/domain/workflows/enroll.rs`: as `run`'s second statement, right after `preflight::check(luks, fido2, fs, None)?;`, before `let name = mapping_name::mapping_name(path)?;`: `let _lock = fs.lock_target(path)?;`
  - `src/domain/workflows/revoke.rs`: same position, right after `preflight::check(luks, fido2, fs, None)?;`, before `let target = luks.list_fido2_keyslots(path)?...`: `let _lock = fs.lock_target(path)?;`
  - `src/domain/workflows/close.rs`: same position in `close::run`, right after `preflight::check(luks, fido2, fs, None)?;`, before `let name = mapping_name::mapping_name(path)?;`: `let _lock = fs.lock_target(path)?;`. **Do not** add a lock call inside `close_mapping` itself — `close_mapping` is also called per-mapping by `close_all::run` (Task 7), which needs its own, independently-scoped lock acquisition per iteration; locking inside `close_mapping` would either double-lock (if `close_all` also locks) or make `close::run`'s own single-target lock indistinguishable from `close_all`'s per-mapping one.
  - `src/domain/workflows/resize.rs`: right after the **first** `preflight::check(luks, fido2, fs, None)?;` (line 58 in the baseline — the unconditional one, still the literal first statement, unchanged), before `let name = mapping_name::mapping_name(path)?;` (line 60): `let _lock = fs.lock_target(path)?;`. The **second** `preflight::check(luks, fido2, fs, Some(filesystem))?;` (line 116, after `read_filesystem`) is unaffected — it stays exactly where it is; AC #2 only requires the lock to be acquired "immediately after preflight passes," and resize's Dev Notes precedent (Story 6.4) already established that the *first* preflight call is the one satisfying `resize_run_stops_at_preflight_before_touching_any_port`'s "first statement" invariant, so the lock's natural position is immediately after that first call, not the second.

- [ ] **Task 7: Wire per-mapping locking into `close_all` and `slam`** (AC: #3)
  - `src/domain/workflows/close_all.rs`: wrap each mapping's `close_mapping` call in its own lock acquisition, scoped to that single `.map()` iteration — the guard is a local binding inside the closure, so it drops (releasing the lock) when the closure body finishes, before the next mapping is processed:
    ```rust
    Ok(mappings
        .into_iter()
        .map(|mapper| {
            let result = match fs.lock_target(&mapper.source_path) {
                Ok(_lock) => close_mapping(&mapper, skip_hooks, warn, luks, fs),
                Err(err) => Err(err),
            };
            (mapper, result)
        })
        .collect())
    ```
    This directly satisfies AC #3's "one mapping's contention is only that mapping's own failure" — a lock-contention `Err` for one mapping becomes that mapping's own entry in the returned `CloseAllResults` `Vec`, exactly like any other per-mapping `close_mapping` failure already does; the loop itself never stops (unchanged from today).
  - `src/domain/workflows/slam.rs`: identical shape, wrapping `slam_mapping` instead of `close_mapping`:
    ```rust
    Ok(mappings
        .into_iter()
        .map(|mapper| {
            let result = match fs.lock_target(&mapper.source_path) {
                Ok(_lock) => slam_mapping(&mapper, warn, luks, fs),
                Err(err) => Err(err),
            };
            (mapper, result)
        })
        .collect())
    ```
  - `unlock.rs`/`info.rs`: **no change** — confirms AC #4 structurally, not just by omission.

- [ ] **Task 8: `ux::translate` — add the `LockContention` arm** (AC: #1)
  - `src/cli/ux.rs`: add a new arm to `translate`'s top-level `match` (not `translate_adapter_failure` — `LockContention` is its own `DomainError` variant, not a substring-matched `AdapterFailure`, so it carries no marker-bleed risk and needs no substring guard). Placement anywhere in the match (order doesn't matter here since it's a distinct variant, not a string-substring check):
    ```rust
    DomainError::LockContention(path) => format!(
        "Another Hypogaol operation is already in progress on {}. Wait for it to finish, then try again.",
        path.display()
    ),
    ```
    This satisfies AC #1's literal "another operation is already in progress" wording. The match is exhaustive by construction (this module's own doc comment says so) — omitting this arm fails compilation everywhere `translate` is called, which is how Task 1's new variant gets caught if forgotten.

- [ ] **Task 9: Extend `FakeFilesystemBackend` for `lock_target`** (AC: all — needed by every new unit test)
  - `tests/unit/fakes.rs`: add `lock_target` to the `FilesystemBackend for FakeFilesystemBackend` impl, following the existing `check_prerequisites`/`fail_if` conventions:
    ```rust
    fn lock_target(&self, path: &Path) -> Result<LockGuard, DomainError> {
        self.log.borrow_mut().push("lock_target".to_string());
        self.lock_target_calls.borrow_mut().push(path.to_path_buf());
        if self.lock_contention {
            return Err(DomainError::LockContention(path.to_path_buf()));
        }
        self.fail_if("lock_target")?;
        Ok(LockGuard(None))
    }
    ```
    New fields on `FakeFilesystemBackend`: `lock_target_calls: RefCell<Vec<PathBuf>>` (init `RefCell::new(Vec::new())` in both `passing()`/`failing()`), `lock_contention: bool` (init `false` in both). New builder/accessor methods on `impl FakeFilesystemBackend`:
    ```rust
    /// Every `path` passed to `lock_target`, in call order — lets a test
    /// prove which target each workflow locked, and (for close_all/slam)
    /// that a lock was acquired once per mapping, interleaved with each
    /// mapping's own close/slam calls in the shared `CallLog`, not all
    /// acquired up front.
    pub fn lock_target_calls(&self) -> Vec<PathBuf> {
        self.lock_target_calls.borrow().clone()
    }

    /// Makes every `lock_target` call return `DomainError::LockContention`
    /// — distinct from `with_failure_at("lock_target")`'s generic
    /// `AdapterFailure`, since a test needs to assert the *specific*
    /// variant `ux::translate` and callers pattern-match on.
    pub fn with_lock_contention(mut self) -> Self {
        self.lock_contention = true;
        self
    }
    ```
    `LockGuard` needs importing into `tests/unit/fakes.rs`'s existing `use hypogaol::domain::types::{...}` block.

- [ ] **Task 10: Unit tests proving the lock is acquired at the right position for each single-target workflow** (AC: #1, #2)
  - `tests/unit/workflows.rs` (or each workflow's own test file if it has one — check Task 0's read for where `create`/`enroll`/`revoke`/`close`/`resize`-specific tests currently live; `create.rs`/`resize.rs` exist under `tests/unit/`, so prefer adding there over the shared `workflows.rs` file where a dedicated file exists): for each of `create`, `enroll`, `revoke`, `close`, `resize`, add a test asserting `fs.lock_target_calls() == vec![<expected path>]` after a successful run, proving the real target path (not a stand-in) was passed.
  - For each of the same five, add a lock-contention test: `FakeFilesystemBackend::passing().with_lock_contention()`, assert the workflow returns `Err(DomainError::LockContention(_))`, and assert no *mutating* `luks`/`fido2` call happened first (e.g. `luks.open`/`luks.bootstrap_format_and_open`/`fido2.enroll_fido2_key` never appear in the shared `CallLog`) — proving the lock genuinely gates the workflow's real work, not just something logged alongside it. Mirrors the existing `*_stops_at_preflight_before_touching_any_port` tests' shape (`tests/unit/workflows.rs` lines ~59-110) but for the lock instead of preflight.
  - For `resize` specifically, add one test proving the lock sits between the two `preflight::check` calls in the actual execution order: configure `FakeLuksBackend::passing().with_read_filesystem(Filesystem::Xfs)`, run to completion, assert `fs.check_prerequisites_filesystem_calls() == vec![None, Some(Filesystem::Xfs)]` (Story 6.4's existing assertion shape) **and** `fs.lock_target_calls() == vec![<path>]`, with the `CallLog` showing `lock_target` appearing after the first `check_prerequisites` and before `read_filesystem`/the second `check_prerequisites` — proving the lock is genuinely the second statement, not just eventually called somewhere.

- [ ] **Task 11: Unit tests proving per-mapping locking for `close_all`/`slam`** (AC: #3)
  - `tests/unit/close_all.rs`: configure `FakeLuksBackend` with 2+ open mappings (check this file's existing multi-mapping test, if any, for the setup pattern — likely already exists for AC #2's per-mapping fault tolerance from Story 4.5); make `FakeFilesystemBackend::passing().with_lock_contention()` **not** apply to all mappings uniformly — since `with_lock_contention()` as specced above is all-or-nothing, this test needs `with_lock_contention()` unconditionally (contention on *every* mapping) to prove each mapping's own result carries its own `LockContention` error (not one shared failure that stops the batch): assert the returned `CloseAllResults` has one `Err(DomainError::LockContention(_))` entry per mapping, **and** that `luks.list_open_mappings` was still called exactly once (discovery itself is unaffected) — proving the loop still visits every mapping even though every one fails to lock.
  - Add a second test with `FakeFilesystemBackend::passing()` (no contention) and 2+ mappings: assert `fs.lock_target_calls().len()` equals the number of mappings, each called with that mapping's own `source_path` — proving one lock per mapping, not one shared lock for the batch (the literal AC #3 assertion).
  - `tests/unit/slam.rs`: mirror both tests above for `slam::run`/`slam_mapping`.

- [ ] **Task 12: A real, hardware-independent regression test for the actual `flock` behavior** (AC: #2, #5 — the only task in this story that exercises real OS-level locking, not a fake)
  - This does **not** need FIDO2 hardware, `sudo`, or a real LUKS volume — `flock(2)` semantics apply to any regular file, and two independent `open()` calls against the same path within a single test process behave exactly like two separate processes for locking purposes (a lock is associated with the *open file description*, not the process). Add a test in `tests/unit/` (a new small module, e.g. `tests/unit/lock_target.rs`, registered in `tests/unit/main.rs`'s `mod` list) that:
    1. Constructs a real `hypogaol::adapters::exec::ExecAdapter::default()` (same constructor `tests/hardware/main.rs` already uses).
    2. Creates a real temp file (e.g. via `std::env::temp_dir()` plus a unique-enough name, or check if this project already has a temp-dir test helper elsewhere in `tests/unit/` to reuse).
    3. Calls `adapter.lock_target(&path)` — asserts `Ok(_)`.
    4. While the first `LockGuard` is still alive (not dropped), calls `adapter.lock_target(&path)` a second time — asserts `Err(DomainError::LockContention(_))`, proving real contention (AC #1/#2, the actual mechanism, not just that the fake records a call).
    5. Drops the first `LockGuard` explicitly (`drop(first_guard)`), then calls `adapter.lock_target(&path)` a third time — asserts `Ok(_)`, proving the lock is genuinely released on drop (AC #5's "no stale-lock state" behavior, exercised directly rather than only asserted by architecture text).
  - This test needs no `#[ignore]` — unlike the hardware suite, it has no external dependency (no `cryptsetup`, no FIDO2, no `sudo`), so it should run in the default `make test`/`cargo test` suite alongside every other unit test.

- [ ] **Task 13: Full regression pass**
  - `cargo build` succeeds. `make test` passes with all prior tests green (baseline **235 total: 17 lib + 218 `tests/unit`**, per Story 6.4's Completion Notes, verified fresh against this story's own `baseline_commit` — not from memory) plus this story's new tests. **Every existing test that shares a `CallLog` across `create`/`enroll`/`revoke`/`close`/`close_all`/`resize`/`slam`'s fakes and asserts an exact call sequence (or `log.borrow().is_empty()`) will need `"lock_target"` inserted at the correct position** — this is the same class of ripple Story 6.4's `check_prerequisites` logging change caused (54 assertions across 9 files that time). `unlock.rs`/`info.rs` tests are unaffected (no `lock_target` call added there). Fix every compiler/assertion failure the real `cargo test` run surfaces; do not trust this task list's own file enumeration as exhaustive — same discipline Story 6.4's Dev Notes documents for its own signature-ripple.
  - `cargo fmt --check` and `cargo clippy --all-targets` both clean. This story adds **no new parameters to any public workflow function** (`create::run`/`enroll::run`/`revoke::run`/`close::run`/`resize::run`/`close_all::run`/`slam::run` all keep their existing signatures — only their internal bodies gain a `fs.lock_target(...)` call), so the 5 pre-existing `too_many_arguments` warnings Story 6.4 confirmed (`run_create`/`create::run`/`bootstrap_and_provision`/`finish_provisioning`/`grow_open_mapping`) should be unchanged; if a new clippy warning appears anywhere, note it explicitly in Completion Notes rather than silently suppressing it.
  - `cargo tree -i libc` before/after Task 5's `Cargo.toml` change: confirm `libc`'s resolved version in `Cargo.lock` doesn't change (it's already present transitively at `0.2.189` via `getrandom`) — only its dependency-graph edges change (a new direct edge from the `hypogaol` package itself). If it does change version, note why in Completion Notes.
  - Real concurrent-process verification (AC #1's literal "two revoke invocations concurrently" scenario) needs a genuine two-process race, which neither `tests/unit/` (single test process, though Task 12 already proves the underlying `flock` mechanism works across independent file descriptors) nor a `#[ignore]`d-but-single-process hardware test can fully exercise. If real hardware is available in this session: add one `#[ignore]`d scenario to `tests/hardware/main.rs` that creates a volume with 2 enrolled FIDO2 keys, then spawns two real child processes (`Command::new(env!("CARGO_BIN_EXE_hypogaol"))` or equivalent, check how/whether this project already builds a CLI binary reachable from `tests/hardware/main.rs` — if not, spawning the built binary directly may need `assert_cmd`-style plumbing this project doesn't currently have; if no clean binary-spawn path exists in the current test harness, state that explicitly and fall back to two threads each calling `revoke::run` directly against a real `ExecAdapter` and real LUKS2 header, which still exercises the real `flock` contention across two real, independently-opened file descriptors even without separate OS processes) each revoking a different key, and asserts exactly one succeeds (AC #1). If no hardware is available in this session, state that explicitly (this project's standing convention — Stories 4.3, 5.1, 5.2, 6.1, 6.2, 6.3, 6.4) and flag it as a retrospective action item for `LeReverandNox`.

### Review Findings

_To be filled in during code review._

## Dev Notes

- **No new port, no new architectural layer.** Same framing Story 6.4 confirmed for itself: this story is a pure extension of the existing `FilesystemBackend` port (now AD-20's `lock_target`, alongside AD-8's filesystem methods) — ARCHITECTURE-SPINE.md's Epic 6 framing text says explicitly the invocation-locking guard (CAP-24, AD-20) "extend[s] `FilesystemBackend`/`Fido2Backend` rather than introducing a fourth port." [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md:38]

- **AD-20 is the complete, load-bearing spec for this story — read it in full before starting.** Quoted verbatim: `FilesystemBackend` gains `lock_target(path) -> Result<LockGuard, DomainError>`; `flock(2)` (`LOCK_EX | LOCK_NB`) on an fd opened with `O_CLOEXEC` (required, explicit, not implicit); lives on `FilesystemBackend` (not `LuksBackend`) since it's a generic OS/path primitive, same placement rationale as `path_exists`/`device_capacity`; canonicalizes the path if it exists, else its parent directory (resolves a real gap in `mapping_name`'s own canonicalize-or-fail behavior, since `create` needs to lock a target *before* a fresh file-backed path exists on disk); every mutating workflow acquires it as its second statement, immediately after `preflight` (AD-4) passes; `close_all`/`slam` acquire and release a separate `LockGuard` per mapping inside their existing per-mapping loop, never one lock for the whole batch; `info`/`unlock` (including read-only) acquire none; on contention, returns immediately (never blocks) with a plain-language "another operation is already in progress" translation; the kernel releases the lock automatically on process exit given `O_CLOEXEC`, so there's no stale-lock cleanup needed; the guard is held for the remainder of the workflow's execution and dropped on every exit path (success, refusal, or error). [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-20, lines 173-180]

- **AD-4's amendment**: "AD-20's per-invocation lock is acquired immediately after this gate passes, never before it and never folded into it — `preflight` itself stays read-only and side-effect-free." Confirms `preflight::check`'s own signature/body is untouched by this story — the lock is a caller-side addition in each workflow, not inside `preflight::check` itself. [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-4, line 65]

- **Load-Bearing Design Decision: `LockGuard`'s shape.** AD-20 specifies the OS mechanism (an fd, `flock`, `O_CLOEXEC`) but not how a single, object-safe `dyn FilesystemBackend` trait method returns something that is a real held lock in production and a harmless no-op in tests — this is not optional, and getting it wrong either breaks object-safety (an associated-type return can't be used behind `&dyn FilesystemBackend`, which every workflow function's signature requires) or forces the fake to hold a real, meaningless OS resource just to satisfy the type checker. Resolution (Tasks 2/5/9): `LockGuard` is one concrete struct, `pub struct LockGuard(pub Option<std::os::fd::OwnedFd>)`, living in `domain::types` alongside `MapperHandle`/`HookFileMeta`. The real `ExecAdapter` constructs `LockGuard(Some(fd))` — `OwnedFd`'s own `Drop` impl closes the fd automatically (no manual `Drop` needed anywhere in this story), which is exactly the "kernel releases the lock automatically" mechanism AD-20/AC #5 describe, just triggered by the guard's drop rather than only by process exit. `FakeFilesystemBackend` constructs `LockGuard(None)` — no real fd, no real lock, `Drop` on `None` is a no-op — sufficient because every ACs' actual locking *behavior* (contention, per-mapping scoping, position-in-sequence) is proven through the fake's own call-log/contention-flag machinery (Tasks 9-11), while the real OS mechanism itself is proven separately and directly by Task 12's non-fake test. This mirrors how this codebase already separates "does the workflow call the port correctly" (fakes, `tests/unit/`) from "does the port's real implementation actually work" (`ExecAdapter`, `tests/hardware/main.rs`) for every other port method — Task 12 is unusual only in that it doesn't need real hardware to make that second kind of test meaningful, since `flock` needs nothing external.

- **Why `let _lock = ...`, never `let _ = ...`, is the single highest-risk line in this entire story.** Rust's `_` wildcard pattern (`let _ = expr;`) evaluates `expr` and immediately drops the result — it does *not* bind it to anything with a lifetime. `let _lock = expr;` (a real identifier merely prefixed with `_` to silence the "unused variable" lint) binds the value normally, so it lives until the end of its enclosing scope like any other `let`. Using the wildcard form anywhere in Task 6/7 would compile cleanly, pass every `CallLog`-based unit test (the fake's `lock_target` call is still logged and still returns a value), and silently defeat the entire story: the lock would release itself microseconds after being acquired, before the workflow's real mutating work even begins, reopening exactly the race AC #1 exists to close — a bug no `CallLog` assertion alone would ever catch, since the log only proves the *call* happened, not that its *return value* was held. This is called out explicitly because it is the one mistake in this story that is both easy to make and invisible to this story's own primary test strategy (Task 12's direct-contention test is what actually guards against it — a wildcard-bound guard released immediately would make Task 12's second `lock_target` call spuriously succeed instead of contending).

- **Why `close`/`close_all` each lock independently rather than sharing a lock call inside `close_mapping`.** `close_mapping` (`src/domain/workflows/close.rs`, `pub(crate)`) is called by both `close::run` (a single target, known up front) and `close_all::run` (N targets, discovered via `luks.list_open_mappings()`, each needing its *own* separately-scoped lock per AC #3). Putting the lock inside `close_mapping` itself would force one of two wrong shapes: either `close::run` locks *and* `close_mapping` locks again (double-acquisition on the same target, which would make `close::run` deadlock against its own second `flock` call — `flock` is not reentrant within the same process across two separate fds on the same path), or `close_all::run` would have to skip `close_mapping`'s internal lock somehow, which defeats having it there at all. Keeping the lock call at each *caller's* level (`close::run` locks once for its one target; `close_all::run`/`slam::run` lock once per loop iteration around their shared per-mapping helper) is the only shape that satisfies both AC #2 (single-target workflows lock exactly once) and AC #3 (batch workflows lock/release per mapping) without duplication or reentrancy risk.

- **Recurring review-pattern watchlist from Epic 4/5/6 retros — apply proactively:**
  - Self-reported completion-note claims not matching actual grep/build/test output — verify every test-count claim in Task 13's Completion Notes against real `make test` output, not memory.
  - New pure parsing/logic functions shipping without a direct unit test — `lock_target_path`'s canonicalize-or-parent-fallback logic is exactly this shape; unlike Story 6.4's adapter-internal `xfs_info`/`btrfs` output parsers (which had no meaningful way to unit-test without faking subprocess output), `lock_target_path` is pure, dependency-free, filesystem-boundary logic reachable directly from `tests/unit/` — it should get a direct unit test (e.g. `lock_target_path` of an existing file returns that file's canonical path; of a nonexistent path returns its parent's canonical path; of a path whose parent also doesn't exist returns an `AdapterFailure`), not deferred to the hardware suite.
  - Marker-bleed: not applicable to this story's own new error text — `LockContention` is a dedicated `DomainError` variant matched by its own top-level `translate` arm, never a substring-matched `AdapterFailure` string, so it cannot collide with `translate_adapter_failure`'s bucket chain the way Story 6.4's `with_transient_mount` errors could have. Still worth a quick check that `LockContention`'s message text ("another operation is already in progress on...") doesn't itself get *matched by* some other, later substring check if a future story ever routes it through `AdapterFailure` instead — it should not be, per Task 1's design, but flag if a review finds otherwise.
  - Hardware verification convention: state explicitly in Completion Notes whether real hardware was available and what was/wasn't verified on it. This story is unusual in that its *primary* correctness mechanism (Task 12's real `flock` test) needs no hardware at all — only the literal two-concurrent-*process* framing of AC #1 needs hardware/manual verification, and even that only for the "two real OS processes, not two fds in one process" nuance, since Task 12 already proves the underlying kernel mechanism.

### Project Structure Notes

- Files touched (production): `src/domain/errors.rs` (`DomainError` gains `LockContention`), `src/domain/types.rs` (`LockGuard` struct), `src/domain/mapping_name.rs` (`lock_target_path` function), `src/ports/filesystem_backend.rs` (`lock_target` method), `src/adapters/exec/mod.rs` (`lock_target` impl + new `std::os::fd`/`OpenOptionsExt` imports), `src/domain/workflows/create.rs` (`target_path` helper + 1 lock call), `enroll.rs`/`revoke.rs`/`close.rs`/`resize.rs` (1 lock call each), `close_all.rs`/`slam.rs` (per-mapping lock wrapping their existing `.map()` closures), `src/cli/ux.rs` (1 new `translate` match arm), `Cargo.toml` (`libc` promoted to a direct dependency).
- `unlock.rs`/`info.rs`: **no production changes** — confirms AC #4.
- Files touched (tests): `tests/unit/fakes.rs` (`FakeFilesystemBackend::lock_target` + 2 new fields + 2 new methods), `tests/unit/main.rs` (registers a new `lock_target` test module if Task 12 uses its own file), a new `tests/unit/lock_target.rs` (or equivalent) for Task 12's real-`ExecAdapter` test, plus updates across every existing test in `tests/unit/create.rs`/`enroll.rs`/`revoke.rs`/`close.rs`/`close_all.rs`/`resize.rs`/`slam.rs`/`workflows.rs` that asserts an exact `CallLog` sequence (Task 13's ripple). `tests/hardware/main.rs` gains at most one new `#[ignore]`d scenario if hardware is available (Task 13).
- No new files under `src/domain/workflows/` — this story extends 7 existing workflow files, adds no new one. No new port trait, no `CreateTarget`/`KeyMetadata`/`MapperHandle` shape changes beyond confirming `MapperHandle.source_path` (already `pub`, unchanged) is the per-mapping lock target for `close_all`/`slam`.
- Alignment with the documented source tree: `ARCHITECTURE-SPINE.md`'s Structural Seed already lists `filesystem_backend.rs`'s trait doc as anticipating `lock_target(AD-20, CAP-24)`, and its adapter module doc already says `"lock_target (AD-20) uses flock(2) on an open fd"` — both planning-doc-only language this story turns into real code for the first time. The Capability map already lists `CAP-24 (concurrent-invocation guard) | domain::workflows::{create,enroll,revoke,close,resize}, FilesystemBackend | AD-4, AD-20` — this story implements exactly that mapping, including `close_all`/`slam` as the per-mapping extension of `close`'s own entry (both call through `close_mapping`/their own per-mapping helper, not a separate top-level capability).

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 6.5: Concurrent-Invocation Guard, lines 870-896] — acceptance criteria origin, verbatim.
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 6: Volume Resilience, Filesystem Choice & Everyday Polish, lines 766-768] — epic-level framing; explicitly names "two invocations against the same volume can no longer race past a safety guard" as this story's contribution and confirms no new port/architectural layer for any Epic 6 story.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-20 — Concurrent-invocation guard, lines 173-180] — the complete mechanism spec this story implements verbatim: `flock(2)`, `LOCK_EX | LOCK_NB`, `O_CLOEXEC`, canonicalize-with-parent-fallback, per-workflow acquisition position, per-mapping scoping for close_all/slam, no-block/no-stale-state contention behavior.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-4 — Mandatory shared pre-flight gate, line 65] — the Epic-6 amendment establishing the lock is acquired *after* (never inside/before) `preflight`.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#Deferred, line 306] — confirms this story narrows (not eliminates) a previously-deferred slam busy-mount reacquisition race, and Story 6.4's own deferred finding about `with_transient_mount`'s unguarded two-mount-cycle race, both scoped as "the class of issue AD-20 addresses" without requiring this story to re-solve them individually — this story's guard is a workflow-level (per-invocation) lock, not a per-adapter-call one; a single workflow's own internal sequence of port calls (like `with_transient_mount`'s two independent mounts inside one `resize` call) is already serialized by definition once that whole `resize::run` invocation holds the lock for its own duration — the deferred item is fully closed by this story's implementation, not left open.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-7 — Testing strategy, line 81] — "the shared fake `LuksBackend`/`Fido2Backend`/`FilesystemBackend` gains fake implementations of every Epic 6 addition (`has_marker_token`, `lock_target`, `scaffold_hook_templates`, `client_pin`) the same day the real port method lands" — `lock_target` is explicitly named as an anticipated fake addition; this story is that addition.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#Capability-to-component map, line 300] — `CAP-24 (concurrent-invocation guard) | domain::workflows::{create,enroll,revoke,close,resize}, FilesystemBackend | AD-4, AD-20`.
- [Source: src/domain/mapping_name.rs] — full current implementation, read in full during story creation; `mapping_name`'s existing canonicalize-or-fail behavior is the direct precedent `lock_target_path` sits alongside, and the exact gap (no nonexistent-path fallback) this story's new function must not inherit.
- [Source: src/domain/workflows/create.rs, enroll.rs, revoke.rs, close.rs, close_all.rs, resize.rs, slam.rs, unlock.rs, info.rs] — full current implementations, read in full during story creation; every `preflight::check` call site and exact insertion point for this story's `lock_target` call is derived directly from these files' current bodies, not inferred.
- [Source: src/ports/filesystem_backend.rs] — full current 14-method trait, read in full during story creation; the doc-comment style and method-ordering convention `lock_target` (the 15th method) follows.
- [Source: tests/unit/fakes.rs] — `FakeFilesystemBackend`'s full field list and `check_prerequisites_filesystem_calls`/`with_failure_for_filesystem` pair (Story 6.4), the direct precedent for this story's `lock_target_calls`/`with_lock_contention` pair.
- [Source: tests/unit/workflows.rs, lines ~59-110] — the six existing `*_stops_at_preflight_before_touching_any_port`/`*_before_reaching_its_own_todo` tests, confirmed unaffected by this story (preflight still fails, when it fails, before `fs.lock_target` is ever reached) and the direct shape this story's own lock-contention tests (Task 10) mirror.
- [Source: tests/hardware/main.rs] — `ExecAdapter::default()`'s existing construction pattern, reused unmodified by Task 12's non-hardware `flock` test.
- [Source: _bmad-output/implementation-artifacts/6-4-xfs-and-btrfs-filesystem-support.md] — previous story in this epic; its own "Review Findings" section explicitly defers `with_transient_mount`'s unguarded two-mount-cycle race to this story by name; its Completion Notes establish the baseline test count (235: 17 lib + 218 tests/unit) and the "a shared-signature/logging change ripples through every CallLog-asserting test" pattern this story's Task 13 anticipates for the same reason.
- [Source: _bmad-output/implementation-artifacts/sprint-status.yaml] — confirms this is the fifth story of Epic 6 (epic already `in-progress` since Story 6.1) and no epic-6 action item currently references locking/concurrency (this story is not fixing a previously-reported bug, only implementing the epic's own planned capability).
- `flock(2)`/`open(2)` man pages, `std::os::fd::OwnedFd`/`std::os::unix::fs::OpenOptionsExt` stdlib docs, `libc` crate docs — web/doc-verified 2026-08-10: `flock(LOCK_EX | LOCK_NB)` on an already-locked fd (by another open file description, including a different fd within the same process) returns `EWOULDBLOCK` immediately rather than blocking; `OwnedFd`'s `Drop` impl calls `close(2)`, which — as the last close of a file description holding a `flock` lock — releases it; `O_CLOEXEC` prevents the fd from surviving an `exec()` in a forked child, meaning a spawned `cryptsetup`/`mkfs`/etc. subprocess (which this codebase always launches via `std::process::Command`, itself doing fork+exec) never inherits the locking fd.

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

### File List
