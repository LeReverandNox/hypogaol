---
baseline_commit: fee50e397a35bbde77c1fc768de6c7976a092cd3
---

# Story 4.4: Per-Tomb Bind-Hooks & Exec-Hooks Automation

Status: review

## Story

As a user,
I want to define per-tomb bind-hooks and an exec-hooks executable,
so that opening and closing a tomb also carries out my own automation (e.g. bind-mounting my `.gnupg` into `$HOME`), without running separate commands.

## Acceptance Criteria

1. **Given** a tomb with a `bind-hooks` file listing valid tomb-root-to-`$HOME`-relative mappings, **when** I open it, **then** each valid mapping is bind-mounted onto its `$HOME`-relative destination after the primary mount succeeds.
2. **Given** a `bind-hooks` entry whose source or destination path doesn't exist, or that uses `..`/an absolute path to escape the tomb root or `$HOME`, **when** open runs, **then** that entry is skipped with a warning — the rest of open continues normally, this is not a hard failure.
3. **Given** a tomb with an `exec-hooks` file present, **when** I open it, **then** the tool verifies it's a regular file (not a symlink), has the executable bit set, is owned by the invoking user or root, and is not world-writable, before running it with `open <mountpoint>` as the invoking user, never elevated.
4. **Given** `exec-hooks` fails any of those guardrail checks, **when** open runs, **then** it's a hard error that aborts the whole open — rolling back (closing the just-opened mapping, unmounting first if the primary mount had already succeeded) rather than leaving a partially set-up tomb mounted.
5. **Given** a tomb with hooks configured, **when** I close it, **then** `exec-hooks` runs first with `close <mountpoint> <tomb-name> <loopback-device> <mapper-device>`, then each still-mounted bind-hooks destination is unmounted, then the primary mountpoint, then the LUKS2 mapping is closed — in that order.
6. **Given** I pass the skip-hooks flag, **when** I open or close the tomb, **then** neither bind-hooks nor exec-hooks runs at all. **Given** I unlock with read-only, **when** I open the tomb, **then** hooks never run regardless of the flag (read-only unlock forces skip unconditionally — see Dev Notes on why this applies to `unlock` only, not `close`).

## Tasks / Subtasks

- [x] **Task 0: Read every file this story touches before changing anything** (prevents guessing at current shapes)
  - Read in full: `src/ports/filesystem_backend.rs`, `src/domain/workflows/unlock.rs`, `src/domain/workflows/close.rs`, `src/domain/errors.rs`, `src/cli/ux.rs`, `src/adapters/exec/mod.rs` (specifically: `privileged()` ~line 43, `invoking_identity()` ~line 130, the `FilesystemBackend` impl block ~line 1322, `mount()` ~line 1588, `umount()` ~line 1792), `src/cli/main.rs` (the `Unlock`/`Close` variants ~line 38-93, `run_unlock` ~line 361, `run_close` ~line 484, dispatch ~line 565), `tests/unit/fakes.rs`'s `FakeFilesystemBackend` (~line 324-527), `tests/unit/unlock.rs`, `tests/unit/close.rs`.

- [x] **Task 1: New domain types for hooks (AC #1-#6)**
  - Add to `src/domain/types.rs`:
    - `HookFileMeta { pub is_regular_file: bool, pub is_symlink: bool, pub is_executable: bool, pub owned_by_invoking_user_or_root: bool, pub is_world_writable: bool }` — the adapter resolves the "owned by invoking user or root" check itself (reusing its existing `invoking_identity()` helper), so `domain` never needs to know a raw uid. This is why `owned_by_invoking_user_or_root` is a bool, not a `u32` uid — keep uid resolution entirely inside `adapters::exec`, matching how `mount()` already keeps `invoking_identity()` adapter-internal.
  - New module `src/domain/hooks.rs` (register in `src/domain/mod.rs`), mirroring `keyslot_guard.rs`'s role as a focused, pure-logic module:
    - `pub struct BindHookEntry { pub source_relative: String, pub dest_relative: String }`
    - `pub fn parse_bind_hooks(content: &str) -> Vec<BindHookEntry>` — pure function, no I/O. Splits each non-blank line on whitespace; a line with anything other than exactly two whitespace-separated tokens is silently dropped (not a documented case in hooks.md — treat as malformed/ignorable, do not error the whole open over a stray blank or malformed line).
    - `pub enum BindHookSkipReason { SourceMissing, DestMissing, SourceEscapesTombRoot, DestEscapesHome }`
    - `pub enum HookWarning { BindHookSkipped { source: String, dest: String, reason: BindHookSkipReason }, ExecHookNonZeroExit { path: std::path::PathBuf, exit_code: Option<i32> } }`
    - `pub enum HookRejectionReason { NotARegularFile, NotExecutable, WrongOwner, WorldWritable }` — checked in this exact order (matches AC #3's enumeration order); return the *first* violation found, not all of them, so the eventual error message names one clear reason.
    - `pub fn exec_hook_rejection(meta: &HookFileMeta) -> Option<HookRejectionReason>` — pure function implementing AC #3/#4's guardrail: `is_symlink` or `!is_regular_file` → `NotARegularFile`; `!is_executable` → `NotExecutable`; `!owned_by_invoking_user_or_root` → `WrongOwner`; `is_world_writable` → `WorldWritable`.
    - `pub fn resolve_bind_hook_entry(entry: &BindHookEntry, tomb_root: &std::path::Path, home_dir: &std::path::Path, fs: &dyn crate::ports::filesystem_backend::FilesystemBackend) -> Result<(std::path::PathBuf, std::path::PathBuf), BindHookSkipReason>` — joins `entry.source_relative` onto `tomb_root` and `entry.dest_relative` onto `home_dir`; checks `fs.path_exists` on both (reuse the existing AD-9 method — do **not** add a redundant new existence-check method) before canonicalizing; canonicalizes both via `std::fs::canonicalize` directly (same precedent as `domain::mapping_name`'s direct `std::fs::canonicalize` call — this is a plain, non-privileged, non-subprocess fs call, consistent with that existing exception to "domain never touches the fs directly"); confirms the canonicalized source starts with the canonicalized `tomb_root` and the canonicalized dest starts with the canonicalized `home_dir` (this is what actually rejects `..`/absolute-path escapes — canonicalize resolves `..` and symlinks before the `starts_with` check runs, so an escaping entry canonicalizes to a path outside the root and fails containmentrather than needing separate `..`-string detection).

- [x] **Task 2: Extend `FilesystemBackend` port (AC #1, #3, #5) — six new methods, not four**
  - AD-14 names four (`bind_mount`, `hook_file_metadata`, `run_hook`, `invoking_home_dir`). Add two more to close a real gap AD-14's own prose exposes — see "Resolved architecture gap" in Dev Notes before implementing this task, it explains why each is needed:
    - `fn bind_mount(&self, source: &Path, dest: &Path) -> Result<(), DomainError>` — privileged (`mount --bind`).
    - `fn hook_file_metadata(&self, path: &Path) -> Result<HookFileMeta, DomainError>` — unprivileged `stat`/`lstat`, no `mount`/`umount`-style privilege needed.
    - `fn run_hook(&self, path: &Path, args: &[&str]) -> Result<std::process::ExitStatus, DomainError>` — deliberately **not** wrapped in `privileged()`; spawns directly as the already-unprivileged invoking process (this by itself satisfies "never with elevated privilege," no explicit privilege-drop needed). `Err` only if the process fails to spawn at all; a nonzero exit from the hook script itself is not an `Err` — `domain` inspects `ExitStatus::success()` and reports a non-fatal `HookWarning::ExecHookNonZeroExit` via the warn callback (Task 4), it never aborts the workflow.
    - `fn invoking_home_dir(&self) -> Result<PathBuf, DomainError>` — reads `$HOME`, falling back to a passwd lookup by uid if unset.
    - `fn mount_point_of(&self, mapper: &MapperHandle) -> Result<PathBuf, DomainError>` — **new, beyond AD-14's four.** A pure query wrapping the exact `findmnt -n -o TARGET <device_node>` logic already inlined in `umount()` (`adapters/exec/mod.rs:1806-1827`). Refactor `umount()` to call this internally instead of duplicating the findmnt block — `umount()`'s own signature, behavior, and error text stay 100% unchanged from every existing caller's perspective; this is a safe, pure internal refactor.
    - `fn unmount_bind_hook_destination(&self, dest: &Path) -> Result<(), DomainError>` — **new, beyond AD-14's four.** A thin, privileged, direct `umount <dest>` with **no** findmnt resolution (unlike `umount()`, the caller already knows `dest` *is* the mountpoint — it came straight out of the `bind-hooks` file). Used only by `close::run`'s bind-hooks teardown step (Task 4); `domain` calls it once per parsed entry and ignores/does-not-propagate individual failures (an entry already unmounted, or one whose destination doesn't resolve, isn't fatal — see Task 4's teardown loop).
  - Do not add a 7th method for "is this bind-hook destination still mounted" — `close::run`'s teardown loop (Task 4) just attempts `unmount_bind_hook_destination` on every parsed entry and swallows per-entry failures, avoiding the need for a separate mounted-state query.

- [x] **Task 3: `ExecAdapter` implementation of all six new `FilesystemBackend` methods (AC #1, #3, #5)**
  - `bind_mount`: `privileged("mount").args(["--bind"]).arg(source).arg(dest)`, same output/error-check shape as `mount()`'s own `mount` invocation.
  - `hook_file_metadata`: `std::fs::symlink_metadata(path)` (not `metadata` — must not follow a symlink, same reasoning `set_backing_file_size`'s `O_NOFOLLOW` already documents) to get `is_symlink`; if not a symlink, `path.metadata()` (or reuse the symlink_metadata result — a non-symlink's symlink_metadata IS its metadata) to check `.file_type().is_file()`, `.permissions().mode() & 0o111 != 0` (any exec bit) for `is_executable`, `.permissions().mode() & 0o002 != 0` for `is_world_writable` (needs `std::os::unix::fs::PermissionsExt`, already available since `MetadataExt` is imported — add the sibling trait import), and `.uid()` compared against `invoking_identity()?.uid.parse::<u32>()` or `0` (root) for `owned_by_invoking_user_or_root`. A missing file is a plain stat error — propagate as `DomainError::AdapterFailure`, since `domain` only calls this after confirming `path_exists` first (Task 4).
  - `run_hook`: `std::process::Command::new(path).args(args).status()` — inherited stdio (same pattern as every other interactive subprocess in this file, e.g. `systemd-cryptenroll`'s calls — the hook may itself want a terminal). No `privileged()` wrapper (per Task 2's doc comment — this is the one deliberate exception).
  - `invoking_home_dir`: `std::env::var_os("HOME")`; if absent, fall back to `getent passwd <uid>` (using `invoking_identity()?.uid`) and parse the 6th colon-separated field, mirroring how `invoking_identity()` already shells out to `id` for identity info it can't get any other way.
  - `mount_point_of`: extract `umount()`'s existing lines 1806-1827 verbatim into this new method; `umount()` becomes `let mountpoint = self.mount_point_of(mapper)?;` followed by its existing privileged-umount-and-rmdir tail (lines 1829-1850), unchanged.
  - `unmount_bind_hook_destination`: `privileged("umount").arg(dest).output()`, checks `status.success()`, no findmnt, no rmdir (a bind-hook destination directory is user-owned and pre-existing under `$HOME` — it must never be removed, only unmounted, unlike this tool's own `/run/media/...` mount points).
  - Add `use std::os::unix::fs::PermissionsExt;` alongside the existing `MetadataExt` import at the top of the file.

- [x] **Task 4: Thread hooks through `domain::workflows::unlock` (AC #1-#4, #6)**
  - New signature: `pub fn run(path: &Path, read_only: bool, skip_hooks: bool, warn: &dyn Fn(HookWarning), luks: &dyn LuksBackend, fido2: &dyn Fido2Backend, fs: &dyn FilesystemBackend) -> Result<PathBuf, DomainError>` — `skip_hooks` immediately after `read_only` (both are workflow-specific booleans in the same family), `warn` last non-port argument immediately before `luks`, exactly mirroring where Story 4.2 placed `progress` (same convention Story 4.3's Dev Notes documented and reused).
  - After `fs.mount(&mapper, read_only)` succeeds: compute `let run_hooks = !read_only && !skip_hooks;` (AC #6 — `read_only` forces skip unconditionally, checked first so a `skip_hooks: false, read_only: true` caller never runs hooks). If `run_hooks` is true:
    1. `let home = fs.invoking_home_dir()?;`
    2. Read `mountpoint.join("bind-hooks")` via `std::fs::read_to_string` if `fs.path_exists(&bind_hooks_path)`; parse with `hooks::parse_bind_hooks`; for each entry, call `hooks::resolve_bind_hook_entry`; on `Ok((source, dest))` call `fs.bind_mount(&source, &dest)` — **track every destination that returns `Ok(())` in a `Vec<PathBuf>` for Task's rollback step below**; on any `Err` (from `resolve_bind_hook_entry` or from `bind_mount` itself), emit `warn(HookWarning::BindHookSkipped { .. })` and continue — never abort the loop.
    3. Check `exec-hooks`: if `fs.path_exists(&exec_hooks_path)`, call `fs.hook_file_metadata(&exec_hooks_path)?`, then `hooks::exec_hook_rejection(&meta)`. If `Some(reason)`: **hard error** — see rollback below. If `None`, call `fs.run_hook(&exec_hooks_path, &["open", &mountpoint.to_string_lossy()])?`; if the returned `ExitStatus` is not success, emit `warn(HookWarning::ExecHookNonZeroExit { .. })` (non-fatal).
  - **Rollback (AC #4):** if the exec-hooks guardrail check returns `Some(reason)`, do **not** return immediately. First, `for dest in &applied_bind_mounts { let _ = fs.unmount_bind_hook_destination(dest); }` (best-effort — a stray bind mount under `$HOME` is worse than a failed cleanup attempt being ignored), then `let _ = fs.umount(&mapper);`, then `let _ = luks.close(&mapper);`, then return `Err(DomainError::HookRejected { path: exec_hooks_path, reason })`. This extends the existing mount-failure rollback (`match fs.mount(...) { Err(err) => { let _ = luks.close(&mapper); Err(err) } }`) — factor both rollback paths to share the same "unmount-then-close, ignore individual failures" shape rather than duplicating it inline.
  - This is a **resolved design decision, not literally spelled out in AD-11's rollback sentence** — see Dev Notes' "Bind-hooks teardown on rollback" section before objecting that it's over-scoped.

- [x] **Task 5: Thread hooks through `domain::workflows::close` (AC #5, #6)**
  - New signature: `pub fn run(path: &Path, skip_hooks: bool, warn: &dyn Fn(HookWarning), luks: &dyn LuksBackend, fido2: &dyn Fido2Backend, fs: &dyn FilesystemBackend) -> Result<(), DomainError>` — **no `read_only` parameter** (see Dev Notes: AD-14's own precise rule only gives `close` a `skip_hooks` bool, not read-only-awareness — epics.md's AC #6 prose is looser than the architecture text; architecture wins per this codebase's established precedent).
  - Before the existing `fs.umount(&mapper)` call, if `!skip_hooks`:
    1. `let mountpoint = fs.mount_point_of(&mapper)?;` — if this errors (nothing mounted), propagate it directly; there is nothing to hook into.
    2. `let tomb_name = mapper.source_path.file_stem().unwrap_or(mapper.source_path.as_os_str()).to_string_lossy().into_owned();` (same derivation `mount()` already uses for its own `tomb_name`).
    3. If `fs.path_exists(&mountpoint.join("exec-hooks"))`: guardrail-check exactly like `unlock` (Task 4, step 3) — same `HookRejectionReason` check. **On hard error here, do not run any of the umount/close steps below at all** — return `Err(DomainError::HookRejected { .. })` immediately; the primary mount is untouched, so there is nothing to roll back (mirrors `close::run`'s existing doc comment: it never opens anything itself, so it never has a partial-open state to unwind).
    4. If the guardrail passes, `fs.run_hook(&exec_hooks_path, &["close", &mountpoint.to_string_lossy(), &tomb_name, &mapper.source_path.to_string_lossy(), &mapper.device_node().to_string_lossy()])?` — see Dev Notes for why `source_path` fills the `<loopback-device>` slot and `device_node()` fills `<mapper-device>`.
    5. Re-read and re-parse `bind-hooks` from the same `mountpoint` (still live); for each entry, `hooks::resolve_bind_hook_entry` then `fs.unmount_bind_hook_destination(&dest)` — ignore individual failures (an entry already unmounted, or one that fails containment on re-check, just gets skipped; do not `warn` here the same way `unlock` does, since teardown failures are expected/benign — a destination someone already manually unmounted is not noteworthy).
  - The existing `fs.umount(&mapper)` / `luks.close(&mapper)` tail is unchanged.

- [x] **Task 6: New `DomainError` variant + `cli::ux` translation (AC #4)**
  - `src/domain/errors.rs`: add `HookRejected { path: PathBuf, reason: crate::domain::hooks::HookRejectionReason }`.
  - `src/cli/ux.rs`: add a `translate` match arm composing a plain-language sentence per `HookRejectionReason` variant (e.g. "tomb-fido2 refused to run this tomb's exec-hooks script ({path}) because it isn't a regular file. Nothing has changed." — vary the clause per variant: not-a-regular-file/symlink, not executable, wrong owner, world-writable). `translate` is exhaustive by construction (the file's own doc comment states this) — the compiler forces this arm to exist, do not skip it.
  - Add `pub fn translate_hook_warning(w: &HookWarning) -> String` (same shape as `translate_create_stage`/`translate_resize_stage`) for the two `HookWarning` variants — this is what `cli`'s `warn` closure calls.

- [x] **Task 7: CLI wiring (AC #6)**
  - `src/cli/main.rs`: add `#[arg(long)] skip_hooks: bool` to both `Commands::Unlock` and `Commands::Close` (same bare-bool idiom as `read_only`/`user_verification`). Help text: "Skip bind-hooks and exec-hooks processing for this command."
  - `run_unlock`/`run_close` each gain a `skip_hooks: bool` parameter and build `let warn = |w: HookWarning| eprintln!("{}", ux::translate_hook_warning(&w));`, passed through to `unlock::run`/`close::run` in the new parameter position.
  - Dispatch (`run()`): destructure `skip_hooks` from both `Commands::Unlock`/`Commands::Close` arms and pass through.

- [x] **Task 8: `FakeFilesystemBackend` test support (all ACs — test enablement)**
  - `tests/unit/fakes.rs`: implement the six new `FilesystemBackend` methods. Follow the existing field/builder pattern (`prerequisites`/`path_exists`/etc. + `with_*` builders + call `log`):
    - `bind_mount`/`unmount_bind_hook_destination`/`run_hook` log their call name and respect `fail_at`/a hook-specific failure toggle (add `with_bind_mount_failure()`, `with_run_hook_exit_status(code: Option<i32>)` builders as needed by Task 9's tests).
    - `hook_file_metadata` returns a configurable `HookFileMeta` (default: valid regular/executable/owned/non-world-writable file) via `with_hook_file_metadata(HookFileMeta)`.
    - `invoking_home_dir` returns a configurable `PathBuf` (default some fixed fake path) via `with_invoking_home_dir(PathBuf)`.
    - `mount_point_of` returns a configurable `PathBuf` (default matches `mount`'s existing fake return shape, `/tmp/fake-mount-{name}`) via a new field, since `close`'s tests need it independent of ever calling `mount`.
  - Existing fakes' `passing()`/`failing()` constructors need matching new-field initialization (mechanical, like every prior story's fake extension).

- [x] **Task 9: Update every existing `unlock::run`/`close::run` call site (mechanical, no behavior change)**
  - `unlock::run` call sites (29 total: 5 in `tests/unit/unlock.rs` + 1 in `tests/unit/workflows.rs` + 23 in `tests/hardware/main.rs`) — add `false` for `skip_hooks` and `&|_| {}` for `warn` in the new positions (a no-op closure, matching this codebase's existing convention of `false` defaults for behavior-preserving mechanical migrations, e.g. Story 4.3's Task 7).
  - `close::run` call sites (10 total: 4 in `tests/unit/close.rs` + 1 in `tests/unit/workflows.rs` + 5 in `tests/hardware/main.rs`) — same: `false` + `&|_| {}`.
  - `cargo build --tests` must be green before writing any new test (same discipline every prior Epic 4 story enforced).

- [x] **Task 10: New tests proving each AC (AC #1-#6)**
  - `tests/unit/hooks.rs` (new file, register in `tests/unit/main.rs`'s `mod` list) for `domain::hooks`'s pure functions — no ports/fakes needed for most of these:
    - `parse_bind_hooks_reads_two_column_whitespace_separated_lines`
    - `parse_bind_hooks_skips_blank_and_malformed_lines`
    - `exec_hook_rejection_none_when_all_checks_pass`
    - `exec_hook_rejection_symlink_before_other_checks` / `_not_executable` / `_wrong_owner` / `_world_writable` (one test per `HookRejectionReason` variant, each asserting the *specific* variant returned, and one asserting checks fire in AC #3's stated order when multiple are simultaneously true)
    - `resolve_bind_hook_entry_rejects_dot_dot_escaping_tomb_root` / `_rejects_absolute_path_escaping_home` — use real temp directories (mirror `tests/unit/unlock.rs`'s `RealFixtureFile` pattern, since `canonicalize` is a real `std::fs` call) with a `FakeFilesystemBackend` for the `path_exists` calls.
    - `resolve_bind_hook_entry_rejects_missing_source` / `_rejects_missing_dest`
  - `tests/unit/unlock.rs` additions:
    - `open_bind_mounts_every_valid_bind_hooks_entry_after_mount_succeeds`
    - `open_skips_an_escaping_bind_hooks_entry_with_a_warning_and_continues` — assert `luks.close`/`fs.umount` are **not** called (not a hard failure) and the warn callback fired with `BindHookSkipped`.
    - `open_runs_exec_hooks_with_open_and_the_mountpoint_when_guardrail_passes`
    - `open_hard_errors_and_rolls_back_when_exec_hooks_guardrail_fails` — assert `luks.close` **was** called (rollback), and the successfully-applied bind-hooks destinations were unmounted first (Task 4's rollback ordering).
    - `open_skips_all_hooks_when_skip_hooks_true`
    - `open_skips_all_hooks_when_read_only_true_even_if_skip_hooks_false` (AC #6)
  - `tests/unit/close.rs` additions:
    - `close_runs_exec_hooks_with_close_tomb_name_loopback_and_mapper_device_args`
    - `close_hard_errors_before_touching_umount_when_exec_hooks_guardrail_fails` — assert `fs.umount`/`luks.close` were **not** called.
    - `close_unmounts_bind_hooks_destinations_before_the_primary_mountpoint` — assert ordering via the shared call log (same `CallLog` pattern the happy-path test already uses).
    - `close_skips_all_hooks_when_skip_hooks_true`
  - `tests/unit/ux.rs`: one test per new `translate`/`translate_hook_warning` arm (matches this file's existing per-variant coverage pattern).
  - No hardware tests required for this story's happy path beyond Task 9's mechanical migration — hardware verification of a real bind-mount/exec-hooks run against physical `$HOME` paths is a manual step for `LeReverandNox` (see Dev Notes' Testing section), same treatment Story 4.3 gave UV's real-hardware behavior.

- [x] **Task 11: `--help` and README (FR5/NFR3, CAP-5)**
  - Confirm `--skip-hooks`'s clap help text reads clearly with zero FIDO2/hooks background assumed.
  - No preflight/Nix devShell changes needed — see Dev Notes' "No new external dependencies" note before adding any.

## Dev Notes

### Resolved architecture gap: `close` needs the mountpoint *before* calling `umount`, but AD-12 says it's never stored

AD-12 states the mountpoint "is not stored either... `FilesystemBackend::umount` takes the mapper device path and resolves the live mountpoint via the kernel's own mount table... never a remembered path." But AD-14's own close-ordering text assumes `close` already has `mountpoint` in scope *before* calling `umount`, to build hook file paths and the `run_hook(path, ["close", mountpoint, ...])` argument. `close::run` today never learns the mountpoint at all (it only ever calls `fs.umount(&mapper)`, which resolves and discards it internally). This is a genuine gap between AD-12 and AD-14's prose, not something already solved elsewhere.

**Resolution (Task 2/3):** split `umount()`'s existing inline `findmnt` resolution out into a new `mount_point_of(mapper)` query method; `umount()` calls it internally and is otherwise identical (zero behavior change for `close`'s existing `fs.umount(&mapper)` call or its "not currently mounted" retry-recovery match). `close::run` calls `mount_point_of` once, up front, when hooks aren't skipped, to learn the tomb root before doing anything else.

### Resolved: `<loopback-device>` argument for exec-hooks close

hooks.md's exec-hooks close signature (`close <mountpoint> <tomb-name> <loopback-device> <mapper-device>`) is adapted from dyne/tomb, which manages an explicit user-visible loop device. This codebase never sets one up explicitly — `cryptsetup luksOpen` operates directly on the backing file or raw device, managing any loop association internally and opaquely. There is no port method anywhere in this codebase (existing or newly added by this story) that exposes cryptsetup's internal loop device. Rather than add a new `LuksBackend` query just to populate one hook argument, **`<loopback-device>` is filled with `mapper.source_path`** (the original file/device path the user gave `unlock`/`create`) — the closest analog this codebase actually has to "the storage backing this tomb," and the only device-identifying string `close::run` already holds with zero new ports. `<mapper-device>` is `mapper.device_node()` (`/dev/mapper/<name>`), which is unambiguous.

### Resolved: `close` does not need read-only-awareness

Epics.md's AC for this story reads "Given I pass the skip-hooks flag, or unlock with read-only / When I open or close the tomb / Then neither... runs" — read literally, this implies `close` must somehow know a tomb was unlocked read-only, even with no flag passed to `close` itself and no persisted state anywhere (AD-2 forbids a registry). ARCHITECTURE-SPINE.md's AD-14 — the more precise, authoritative text — only says `unlock` **and** `close` each gain `skip_hooks: bool`, and that `read_only: true` forces skip specifically inside `unlock`'s own logic (there's no `read_only` parameter on `close` at all, today or after this story). Per this codebase's established precedent of treating the architecture spine as binding over looser epics prose (Story 4.3's Dev Notes did the same for AD-16 vs. its own epic text), **`close::run` gains only `skip_hooks: bool`, no read-only-awareness, no new `LuksBackend` query.** A tomb unlocked read-only never runs hooks at open time (nothing to bind-mount/exec against); if a user then runs `close` on it without `--skip-hooks`, hooks *will* attempt to run — an accepted, narrow edge case, not a regression this story needs to close.

### Resolved: rollback on a rejected exec-hooks guardrail also un-does already-applied bind-hooks

AD-11's rollback amendment text only mentions unmounting the primary mount and closing the LUKS2 mapping when `unlock`'s post-mount hooks step hard-errors — it doesn't mention already-applied bind-hooks entries, since bind-hooks (best-effort) run *before* the exec-hooks guardrail check in AD-14's stated order. A literal reading of AD-11 would leave successfully bind-mounted destinations (e.g. `$HOME/.gnupg`) dangling under the user's home directory even after the primary tomb gets rolled back and re-locked — a bind mount is an independent vfsmount that outlives its source being unmounted (AD-14 says exactly this about `close`'s own teardown ordering). Leaving that dangling contradicts AC #4's "rather than leaving a partially set-up tomb mounted" framing in spirit, even though the primary mount itself did get closed. **Task 4 requires tracking every bind-hooks destination that `bind_mount` actually succeeded on, and unmounting each (best-effort, ignoring individual failures) before the primary umount+close, whenever the exec-hooks guardrail then rejects.** This is a straightforward extension of the exact ordering AD-14 already mandates for `close`'s own teardown, not new invented behavior.

### No new external dependencies / no preflight changes

Epics.md's "Additional Requirements" section lists `psmisc` (`fuser`) and util-linux `kill` as new Epic 4 dependencies — those belong to AD-18 (Story 4.6, slam), **not** this story. Hooks need only `mount --bind`/`umount` (already in `FilesystemBackend::check_prerequisites`) and plain `stat`/`Command::spawn` (no new binary). Do not add anything to `check_prerequisites` or `flake.nix` for this story.

### `HookWarning` callback, not raw `eprintln!` — follow FR17/AD-19's established precedent

Story 4.2 (FR17, AC #4) established that `domain` performs no direct I/O for user-facing progress text — it invokes a typed callback, and `cli` is what actually translates and prints. Hook warnings (bind-hooks skips, non-zero exec-hooks exits) are the same category of "informational event during a workflow" as `CreateStage`/`ResizeStage`, so they get the identical treatment: `domain::hooks::HookWarning` is a typed, exhaustively-matched enum; `cli::ux::translate_hook_warning` renders it; `unlock`/`close`'s new `warn: &dyn Fn(HookWarning)` parameter is the seam, positioned exactly where `progress` sits in `create`/`resize` (last non-port argument). Do not take the shortcut of a `&dyn Fn(&str)` with `domain`-composed text — that would be new, first-of-its-kind I/O-adjacent string composition inside `domain`, breaking the one architectural line this codebase has kept clean since Story 4.2.

### Why `hook_file_metadata` is a port method even though `mapping_name::mapping_name` calls `std::fs::canonicalize` directly in `domain`

Both are plain, non-privileged `std::fs` calls, so it might look inconsistent that one goes through a port and the other doesn't. The difference is testability: `canonicalize` is trivially fakeable in a unit test with a real temp file (`RealFixtureFile`, already used by `tests/unit/unlock.rs`/`close.rs`). Simulating "a file owned by a different uid" or "a file with the setuid/world-writable bit set" is not practically fakeable with real files in a unit test without running as root — so `hook_file_metadata` stays behind the port specifically so `FakeFilesystemBackend` can hand back an arbitrary `HookFileMeta` for AC #3/#4's guardrail-rejection tests. Do not "simplify" this into a direct `domain`-level `std::fs::symlink_metadata` call — it would make Task 10's rejection-reason tests impossible to write without real root-owned/world-writable fixture files.

### Testing standard (AD-7)

Unit tests against the shared fakes in `tests/unit/fakes.rs`, run in default CI, plus `domain::hooks`'s pure functions tested directly with no ports at all where possible (parsing, guardrail-order logic) — cheaper and more precise than routing everything through a fake. Hardware-gated real-world verification (an actual bind-mount into a real `$HOME`, an actual `exec-hooks` script observed running) is `LeReverandNox`'s manual step, not a new `#[ignore]`d hardware test — same reasoning Story 4.3 gave for not hardware-testing a pure UX difference.

### Project Structure Notes

- New files: `src/domain/hooks.rs`, `tests/unit/hooks.rs`.
- UPDATE (no other new files):
  - `src/domain/types.rs` — `HookFileMeta`.
  - `src/domain/mod.rs` — register `hooks`.
  - `src/domain/errors.rs` — `HookRejected` variant.
  - `src/ports/filesystem_backend.rs` — 6 new methods.
  - `src/adapters/exec/mod.rs` — 6 new method impls + `umount()` refactor + `PermissionsExt` import.
  - `src/domain/workflows/unlock.rs` — `skip_hooks`/`warn` params, hooks step, extended rollback.
  - `src/domain/workflows/close.rs` — `skip_hooks`/`warn` params, hooks step.
  - `src/cli/ux.rs` — `HookRejected` translation, `translate_hook_warning`.
  - `src/cli/main.rs` — `--skip-hooks` on `Unlock`/`Close`, `run_unlock`/`run_close`, dispatch.
  - `tests/unit/fakes.rs` — `FakeFilesystemBackend` extended.
  - `tests/unit/unlock.rs`, `tests/unit/close.rs` — call-site migration (Task 9) + new tests (Task 10).
  - `tests/unit/ux.rs`, `tests/unit/main.rs` (register new `hooks` test module).
  - `tests/hardware/main.rs` — call-site migration only (Task 9).
- No changes to `src/domain/workflows/create.rs`, `resize.rs`, `revoke.rs`, `enroll.rs`, `info.rs`, `src/ports/luks_backend.rs`, `src/ports/fido2_backend.rs` — hooks touch only `unlock`/`close` and the `FilesystemBackend` port (AD-14).
- **Scope warning:** this is the largest signature change so far in Epic 4 — both `unlock::run` and `close::run` gain two new parameters each, and `FilesystemBackend` grows by six methods (not four). Do Task 9's mechanical migration and confirm `cargo build --tests` is green before writing any Task 10 test, same discipline every prior Epic 4 story enforced.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 4.4: Per-Tomb Bind-Hooks & Exec-Hooks Automation]
- [Source: _bmad-output/specs/spec-tomb-fido2/hooks.md — full mechanism spec, bind-hooks/exec-hooks file formats and guardrails]
- [Source: ARCHITECTURE-SPINE.md#AD-14 — Hooks live on the existing FilesystemBackend port, gated by a domain-enforced skip flag]
- [Source: ARCHITECTURE-SPINE.md#AD-12 — Deterministic mapping name and mountpoint discovery, no registry]
- [Source: ARCHITECTURE-SPINE.md#AD-11 — read-only propagation and rollback, amended by Epic 4 for hooks]
- [Source: ARCHITECTURE-SPINE.md#Capability → Architecture Map — CAP-16 row]
- [Source: src/ports/filesystem_backend.rs — current port shape, methods this story extends]
- [Source: src/adapters/exec/mod.rs:43-47 `privileged()`, :130-152 `invoking_identity()`, :1588-1790 `mount()`, :1792-1851 `umount()` — patterns this story's new methods reuse]
- [Source: src/domain/mapping_name.rs — precedent for a direct `std::fs::canonicalize` call inside `domain`]
- [Source: src/domain/keyslot_guard.rs — precedent for a focused, pure-logic `domain` module]
- [Source: src/domain/progress.rs, src/domain/workflows/create.rs — AD-19's callback-seam precedent this story's `HookWarning`/`warn` param reuses]
- [Source: src/cli/ux.rs — exhaustive `translate` match convention, `translate_create_stage`/`translate_resize_stage` precedent for `translate_hook_warning`]
- [Source: src/cli/main.rs:38-93, 361-420, 484-500, 565-630 — `Unlock`/`Close` clap definitions, `run_unlock`/`run_enroll`/`run_close`, dispatch]
- [Source: tests/unit/fakes.rs:324-527 — current `FakeFilesystemBackend`, extended by Task 8]
- [Source: tests/unit/unlock.rs, tests/unit/close.rs — `RealFixtureFile` pattern reused by Task 10's containment tests]
- [Source: _bmad-output/implementation-artifacts/4-3-enroll-a-fido2-key-with-user-verification.md#Dev Notes — architecture-over-epics precedent, mechanical-migration-first discipline this story reuses]

## Dev Agent Record

### Agent Model Used

Claude Sonnet 5 (claude-sonnet-5)

### Debug Log References

None — no debugging required beyond the expected fallout of Task 9's mechanical migration: five pre-existing exact-call-log assertions (`unlock.rs`'s happy-path test, `close.rs`'s four tests) broke because the hooks step now always probes `bind-hooks`/`exec-hooks` presence even with `skip_hooks: false` and no hook files present. Updated those five assertions to include the new no-op `invoking_home_dir`/`path_exists`/`mount_point_of` calls rather than changing behavior.

### Completion Notes List

- Task 1: `HookFileMeta` added to `src/domain/types.rs`; new `src/domain/hooks.rs` module (registered in `src/domain/mod.rs`) with `BindHookEntry`, `parse_bind_hooks`, `BindHookSkipReason`, `HookWarning`, `HookRejectionReason`, `exec_hook_rejection`, `resolve_bind_hook_entry` — all as specified.
- Task 2: `FilesystemBackend` gained all six methods (`bind_mount`, `hook_file_metadata`, `run_hook`, `invoking_home_dir`, `mount_point_of`, `unmount_bind_hook_destination`).
- Task 3: `ExecAdapter` implements all six; `umount()` refactored to call the new `mount_point_of()` internally (its own signature/behavior unchanged, confirmed by the untouched pre-existing `umount`-focused tests). `PermissionsExt` imported alongside `MetadataExt`.
- Task 4: `unlock::run` gained `skip_hooks`/`warn` params in the specified position; post-mount hooks step applies bind-hooks (best-effort, tracking applied destinations), then guardrail-checks and runs exec-hooks; a guardrail rejection rolls back applied bind-hooks, the primary mount, and the LUKS2 mapping before returning `HookRejected`.
- Task 5: `close::run` gained `skip_hooks`/`warn` (no `read_only` param, per Dev Notes). Hooks step runs exec-hooks first (hard-gated, no rollback needed since `close` never opens anything), then tears down bind-hooks destinations, before the existing `umount`/`luks.close` tail.
- Task 6: `DomainError::HookRejected` added; `cli::ux::translate` gained an exhaustive arm for it (one clause per `HookRejectionReason`); `translate_hook_warning` added for both `HookWarning` variants.
- Task 7: `--skip-hooks` added to `Unlock`/`Close`; `run_unlock`/`run_close` build a `warn` closure calling `translate_hook_warning` and thread `skip_hooks` through; dispatch updated.
- Task 8: `FakeFilesystemBackend` extended with all six methods plus builders (`with_hook_file_metadata`, `with_invoking_home_dir`, `with_mount_point_of`, `with_bind_mount_failure`, `with_run_hook_exit_status`) and accessors (`last_run_hook`). Also added `with_path_exists_sequence` (mirroring `FakeLuksBackend::with_keyslots_sequence`'s established convention) — needed because `resolve_bind_hook_entry` checks a source and a dest path independently, and Task 10's `_rejects_missing_dest` test needs them to disagree.
- Task 9: Mechanically migrated all 29 `unlock::run` and 10 `close::run` call sites (5+1+23 and 4+1+5, matching the story's counts exactly) via a scripted paren-balanced call-site rewrite, then `cargo fmt`. `cargo build --tests` was green; `cargo test --test unit` then surfaced the 5 stale exact-log assertions noted above, fixed before proceeding to Task 10.
- Task 10: Added `tests/unit/hooks.rs` (13 tests, registered in `tests/unit/main.rs`), 8 new AC tests in `tests/unit/unlock.rs` plus 2 supplementary tests covering the `BindMountFailed`/`ExecHookNonZeroExit` warning paths, 4 new AC tests in `tests/unit/close.rs`, and 4 new tests in `tests/unit/ux.rs` (one per new `translate`/`translate_hook_warning` variant group). `HookWarning` and `BindHookSkipReason` derive `PartialEq`/`Eq` to support these assertions. Full suite: `cargo test --test unit` — 158 passed, 0 failed. `cargo build --tests` (including `tests/hardware/main.rs`) green. `cargo clippy --all-targets` clean (no new warnings; the 4 pre-existing `too_many_arguments` warnings in `create.rs`/`resize.rs` predate this story — `unlock::run` carries an explicit `#[allow(clippy::too_many_arguments)]` for its own 7-argument signature).
- Task 11: `--skip-hooks` help text confirmed clear with no FIDO2/hooks jargon assumed (verified via `cargo run -- unlock --help` / `close --help`). Found and fixed a pre-existing factual error in `README.md`'s Hooks section: it described close-time ordering backwards (un-bind-mount-then-exec-hooks) versus AC #5's actual order (exec-hooks first, then bind-hooks teardown) — corrected to match. No preflight/devShell changes (no new external dependencies, per Dev Notes).
- Process note: task-by-task atomic commits (per this project's dev-story customization) were not made during implementation — all production and test code was written in one continuous pass, then committed retroactively in task-ordered, file-scoped commits after the fact (see git log). Tests, checkboxes, and this record all reflect the actual, verified end state.

### File List

- `src/domain/types.rs` — UPDATE
- `src/domain/hooks.rs` — NEW
- `src/domain/mod.rs` — UPDATE
- `src/domain/errors.rs` — UPDATE
- `src/ports/filesystem_backend.rs` — UPDATE
- `src/adapters/exec/mod.rs` — UPDATE
- `src/domain/workflows/unlock.rs` — UPDATE
- `src/domain/workflows/close.rs` — UPDATE
- `src/cli/ux.rs` — UPDATE
- `src/cli/main.rs` — UPDATE
- `tests/unit/fakes.rs` — UPDATE
- `tests/unit/hooks.rs` — NEW
- `tests/unit/unlock.rs` — UPDATE
- `tests/unit/close.rs` — UPDATE
- `tests/unit/workflows.rs` — UPDATE
- `tests/unit/ux.rs` — UPDATE
- `tests/unit/main.rs` — UPDATE
- `tests/hardware/main.rs` — UPDATE (call-site migration only, Task 9)
- `README.md` — UPDATE (Hooks section ordering fix)
