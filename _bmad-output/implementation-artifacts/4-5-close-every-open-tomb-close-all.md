---
baseline_commit: 1306fcb2dedc7bd19848e16fc3d6390946e0d80c
---

# Story 4.5: Close Every Open Tomb (Close-All)

Status: review

## Story

As a user,
I want to close every currently open/unlocked tomb in one command,
so that I don't have to close them one by one when I'm done using several.

## Acceptance Criteria

1. **Given** multiple tombs currently open/unlocked, **when** I run close-all, **then** the tool discovers them by enumerating live dm-crypt mappings carrying the tool's fixed mapping-name prefix (`dmsetup ls`, cross-checked with `cryptsetup status`) — never a stored registry — **and** applies the same sequence as a single close (hooks, then bind-hooks teardown, then primary unmount, then LUKS2 close) to each discovered mapping.
2. **Given** close-all is running against several tombs, **when** one tomb's close fails partway (e.g. still busy), **then** it continues on to the remaining tombs rather than aborting the whole batch, **and** reports every failure alongside every success at the end.
3. **Given** no tombs are currently open, **when** I run close-all, **then** it completes cleanly, reporting nothing to close.
4. **Given** close-all, **when** it runs, **then** `domain::preflight` runs first, like every other workflow, and the same `skip_hooks` flag threads uniformly to every mapping in the batch.

## Tasks / Subtasks

- [x] **Task 0: Read every file this story touches before changing anything** (prevents guessing at current shapes)
  - Read in full: `src/ports/luks_backend.rs`, `src/domain/workflows/close.rs`, `src/domain/mapping_name.rs`, `src/domain/workflows/mod.rs`, `src/domain/errors.rs`, `src/cli/main.rs` (the `Close` variant, `run_close`, dispatch), `src/adapters/exec/mod.rs` (`LuksBackend impl` ~line 763-1058, `privileged()` ~line 45, `check_prerequisites` for all three ports), `tests/unit/fakes.rs` (`FakeLuksBackend`, `FakeFilesystemBackend`), `tests/unit/close.rs`, `tests/unit/workflows.rs`, `flake.nix`.

- [x] **Task 1: `LuksBackend::list_open_mappings()` port method (AC #1)**
  - Add to `src/ports/luks_backend.rs`: `fn list_open_mappings(&self) -> Result<Vec<MapperHandle>, DomainError>;` with a doc comment stating this is AD-17's live-discovery method — never a registry — and that it recovers each mapping's original `source_path` from `cryptsetup status`'s reported device, not a raw `/dev/loopN` node (see Dev Notes: cryptsetup(8) confirms `status` reports the loop *backing file*, not the loop device itself, for a file-backed mapping).
  - In `src/domain/mapping_name.rs`, change `const MAPPING_NAME_PREFIX` to `pub(crate) const MAPPING_NAME_PREFIX` so `adapters::exec` can filter by it (currently private to the module).

- [x] **Task 2: Implement `list_open_mappings` in `ExecAdapter` (AC #1)**
  - Run `dmsetup ls`. Parse stdout: each mapping's name is the first whitespace-delimited field per line; treat an empty/no-entries result as zero mappings, not an error (confirm the exact no-entries string dmsetup prints against real `dmsetup ls` output during hardware verification — not confirmed in this story's authoring sandbox, no privileged `dmsetup`/`cryptsetup` access available there).
  - Keep only names starting with `"{MAPPING_NAME_PREFIX}-"` (the literal prefix plus separator — matches `mapping_name::mapping_name`'s own `format!("{MAPPING_NAME_PREFIX}-{hash:016x}")`, so an unrelated mapping name that merely starts with `"vault"` without the following `-` is correctly excluded).
  - For each surviving name, run `cryptsetup status <name>`, find the line whose trimmed key (text before the first `:`) is `device`, and use its trimmed value as `source_path`. A `cryptsetup status` failure for one discovered name (e.g. a race where it closed between the `dmsetup ls` and this call) propagates as this whole call's `Err` — do not silently skip it; this codebase already accepts this narrow TOCTOU window as a deferred, low-likelihood risk (ARCHITECTURE-SPINE.md's Deferred section), not something this story needs to newly handle per-entry.
  - Add `"dmsetup"` to `LuksBackend::check_prerequisites`'s checked binaries (`src/adapters/exec/mod.rs` ~line 764-789) — see Dev Notes: this is a genuine new dependency this story introduces that epics.md's Epic 4 dependency list omitted.
  - Add `lvm2` (the package providing `dmsetup` — confirmed not bundled with the `cryptsetup` package in nixpkgs) to `flake.nix`'s devShell `packages`, with a comment mirroring `psmisc`'s existing AD-18 comment.

- [x] **Task 3: Extract a shared `close_mapping` helper from `close::run` (AC #1)**
  - In `src/domain/workflows/close.rs`, split `run`'s body (everything after building `mapper`) into a new `pub(crate) fn close_mapping(mapper: &MapperHandle, skip_hooks: bool, warn: &dyn Fn(HookWarning), luks: &dyn LuksBackend, fs: &dyn FilesystemBackend) -> Result<(), DomainError>` — identical logic (hooks step with its "not currently mounted" tolerance, then `fs.umount`, then `luks.close`), no behavior change. `run` becomes: `preflight::check` → derive `mapper` via `mapping_name::mapping_name(path)` → `close_mapping(&mapper, ...)`.
  - This lets `close_all` apply the *exact same* per-mapping sequence without re-deriving a mapping name from a path it doesn't have (Story's whole discovery point is that no path is needed) and without duplicating the hooks/umount/close ordering logic.

- [x] **Task 4: New `domain::workflows::close_all` module (AC #1, #2, #3, #4)**
  - New `src/domain/workflows/close_all.rs`:
    ```rust
    pub fn run(
        skip_hooks: bool,
        warn: &dyn Fn(HookWarning),
        luks: &dyn LuksBackend,
        fido2: &dyn Fido2Backend,
        fs: &dyn FilesystemBackend,
    ) -> Result<Vec<(MapperHandle, Result<(), DomainError>)>, DomainError>
    ```
  - `fido2` is unused beyond `preflight::check` — same AD-4 uniform three-port gate convention `close::run` already documents for its own unused `fido2` parameter.
  - Body: `preflight::check(luks, fido2, fs)?;` then `let mappings = luks.list_open_mappings()?;` (a discovery failure here is the function's own `Err` — distinct from, and never confused with, a per-mapping close failure below) then map each discovered mapper through `close::close_mapping(&mapper, skip_hooks, warn, luks, fs)`, collecting `(mapper, result)` pairs into the returned `Vec` — one mapping's `Err` must never stop the loop (AC #2). An empty `mappings` list simply yields `Ok(vec![])` (AC #3).
  - Register `pub mod close_all;` in `src/domain/workflows/mod.rs`.

- [x] **Task 5: `cli` — `close-all` subcommand + `run_close_all` (AC #1, #2, #3, #4)**
  - In `src/cli/main.rs`'s `Commands` enum, add (clap's default kebab-case rename makes this `close-all` on the command line, no explicit rename needed):
    ```rust
    /// Close every currently open/unlocked tomb in one command
    CloseAll {
        /// Skip bind-hooks and exec-hooks processing for every tomb closed in this batch.
        #[arg(long)]
        skip_hooks: bool,
    },
    ```
    No `path` argument — discovery needs none (AD-17).
  - Add `fn run_close_all(skip_hooks: bool)`, mirroring `run_close`'s shape (preflight check first, same `print_hook_warning` seam), then reports the batch: if the returned `Vec` is empty, print `"No tombs are currently open."` and return; otherwise print one line per mapping — `"Closed {source_path}."` on success, `"Failed to close {source_path}: {translated error}"` (via `ux::translate`) to stderr on failure — then `std::process::exit(1)` if any mapping failed, after every line has been printed (AC #2's "reports every failure alongside every success" — never exit early on the first failure). No confirmation prompt, same reasoning `run_close`'s own doc comment already gives (closing is fully reversible).
  - Wire `Commands::CloseAll { skip_hooks } => run_close_all(skip_hooks),` into `run()`'s dispatch match.

- [x] **Task 6: Extend test fakes for close-all coverage (AC #1, #2, #3)**
  - `FakeLuksBackend` (`tests/unit/fakes.rs`): add `open_mappings: RefCell<Vec<MapperHandle>>` (default empty) + `pub fn with_open_mappings(self, mappings: Vec<MapperHandle>) -> Self`; implement `list_open_mappings` (log `"list_open_mappings"`, `fail_if("list_open_mappings")`, return the field).
  - The existing `fail_at: Option<&'static str>` is a single global failing-call-name — it cannot express "mapping A's `close` fails, mapping B's `close` succeeds" within one batch, which AC #2 needs. Add a second, independent mechanism: `close_failure_for: RefCell<HashSet<String>>` (matched against `MapperHandle.name`) + `pub fn with_close_failure_for(self, name: &str) -> Self`; `close()` checks this set in addition to (not instead of) `fail_at`.
  - `FakeFilesystemBackend`: same need for `umount` — `umount_failure_for: RefCell<HashSet<String>>` + `pub fn with_umount_failure_for(self, name: &str) -> Self`, checked alongside the existing `fail_at`/`umount_not_currently_mounted` logic in `umount()`.

- [x] **Task 7: Unit tests — new `tests/unit/close_all.rs` (AC #1, #2, #3, #4)**
  - Multiple discovered mappings each get the full close sequence (hooks → umount → luks.close), matching `close.rs`'s own per-call-log-assertion style; assert each `luks.close`/`fs.umount` call actually received the mapper `list_open_mappings` returned (identity, not a re-derived name).
  - One mapping's `close_failure_for`/`umount_failure_for` failure doesn't stop the loop: assert the returned `Vec` contains that mapping's `Err` *and* every other mapping's `Ok`, and that every other mapping's port calls still happened.
  - Zero open mappings → `Ok(vec![])`; assert no `umount`/`close`/hooks-related calls happened beyond `list_open_mappings` itself.
  - Preflight failure (`FakeLuksBackend::failing`) short-circuits before `list_open_mappings` is ever called — mirrors `close.rs`'s own preflight-adjacent tests.
  - `skip_hooks: true` skips the hooks step for *every* mapping in the batch, not just the first — assert across at least two discovered mappings (extends `close.rs`'s single-mapping `close_skips_all_hooks_when_skip_hooks_true` precedent).
  - A `list_open_mappings` failure itself (`with_failure_at("list_open_mappings")`) propagates as `close_all::run`'s own `Err`, distinct in kind from a per-mapping `Err` inside the returned `Vec`.
  - Register `mod close_all;` in `tests/unit/main.rs` (alphabetically between `close` and `create`).

- [x] **Task 8: `tests/unit/workflows.rs` — close-all's preflight-gate test (AC #4)**
  - Add `close_all_run_stops_at_preflight_before_touching_any_port`, mirroring the existing `close_run_stops_at_preflight_before_touching_any_port` (a failing `FakeLuksBackend` must short-circuit to `DomainError::PreflightFailed` before `list_open_mappings`/any workflow logic runs).

- [x] **Task 9: `cli` help text + docs pass (AC: all)**
  - Confirm `close-all --help` text is clear with no FIDO2/hooks jargon assumed (`cargo run -- close-all --help`), same standard Story 4.4's Task 11 applied.
  - README.md's "What it does" table already carries a "Close all" row describing this exact behavior (written ahead of implementation) — re-read it against what actually shipped and correct only if it's now inaccurate; do not restate it as new work if it already matches.
  - Run `cargo fmt`, `cargo build --tests`, `cargo test --test unit`, `cargo clippy --all-targets` (all must be clean, matching every prior Epic 4 story's exit bar) before marking this story done. `make test-hardware`'s manual run (verifying `dmsetup ls`'s real no-entries output text from Task 2, and a real multi-tomb close-all) is `LeReverandNox`'s step, not a new automated hardware test — same precedent Story 4.3/4.4 established for hardware-only verification.

## Dev Notes

### Resolved: `cryptsetup status` reports the loop *backing file*, not a raw `/dev/loopN` device

AD-17's text assumes `cryptsetup status <name>` can recover a file-backed tomb's *original* path (`source_path`) — but this codebase's `bootstrap_format_and_open`/`open` never set up an explicit loop device (Story 4.4's Dev Notes already established this: "cryptsetup luksOpen operates directly on the backing file or raw device, managing any loop association internally and opaquely"), so it wasn't obvious `status` wouldn't instead report an opaque `/dev/loop0`-style node with no way back to the original file. Checked directly against `cryptsetup(8)`'s "Notes on loopback device use" section: *"When device mapping is active, you can see the loop backing file in the status command output."* — confirming AD-17's approach works as written: `status`'s `device:` line is the original file path for a file-backed tomb, and the raw device/partition path directly for a device-backed one. No fallback mechanism is needed.

**Correction from hardware verification (LeReverandNox, 2026-07-28):** the above was wrong about *which field* carries the backing file. Real `cryptsetup status` output on a file-backed tomb shows two separate lines:
```
device:  /dev/loop0
loop:    /home/rlaidet/tmp/tombs/tomb-64m-1.img
```
`device:` is the opaque `/dev/loopN` node, exactly as this section originally worried it might be — the man page's "you can see the loop backing file in the status command output" describes the separate `loop:` line, not `device:`. The initial implementation parsed `device:` and shipped `close-all` printing `Closed /dev/loop0.` instead of the tomb's real path; caught in the same hardware pass that verified multi-tomb close-all itself, before merge. Fixed in `ExecAdapter::list_open_mappings` (`src/adapters/exec/mod.rs`) to prefer `loop:` when present, falling back to `device:` for device-backed tombs (which have no loop device at all, so `device:` already holds the correct raw device/partition path there). Confirmed fixed against real hardware: three file-backed tombs closed via `close-all` now report their actual paths.

### Resolved: `dmsetup` is a new dependency this story introduces, missing from epics.md's dependency list

Epics.md's "Additional Requirements" section states only *"New external tool dependencies (Epic 4): `psmisc` (`fuser`, AD-18) and `util-linux` `kill` (AD-18)"* — but AD-17 (this story, CAP-14) explicitly requires `dmsetup ls`, and `dmsetup` is neither already on this project's dependency list nor bundled with the `cryptsetup` nixpkgs package (confirmed: `nixpkgs#cryptsetup`'s `bin` output is only `cryptsetup`/`veritysetup`/`integritysetup`). `dmsetup` ships in the `lvm2` nixpkgs package, not currently in `flake.nix`'s devShell. This is a genuine gap in epics.md's dependency listing this story resolves in code (Task 2): add `dmsetup` to `LuksBackend::check_prerequisites` and `lvm2` to `flake.nix`. Mirrors Story 4.4's precedent of resolving an architecture-vs-epics gap directly in a story's Dev Notes rather than treating it as blocking.

### Resolved: `close_all` reuses `close::run`'s exact sequence via an extracted `close_mapping` helper, never a reimplementation

AD-17 requires close-all to apply "AD-8/AD-14's exact single-close sequence" per mapping. `close::run` today derives its `MapperHandle` from a `path` argument via `mapping_name::mapping_name` — but `close_all` only ever has what `list_open_mappings` returns (already-built `MapperHandle`s with no original path re-derivation needed or wanted, per AD-17: *"It needs no original device/file path, only what `list_open_mappings` returns"*). Task 3 splits `close::run` into path-resolution (unchanged) + a new `pub(crate) close_mapping(mapper: &MapperHandle, ...)` doing the actual hooks/umount/close work, called by both `close::run` (single) and `close_all::run` (batch) — one implementation of the ordering, not two independently-maintained copies that could drift.

### Resolved: a `list_open_mappings` failure is a hard stop, never a per-entry skip

If `cryptsetup status` fails for one name `dmsetup ls` reported (e.g. a mapping closes in the race window between the two calls), `list_open_mappings` propagates that as its own `Err`, aborting discovery entirely — it does not silently drop that one entry and continue. This is deliberately different from AC #2's per-*mapping-close* fault tolerance (which only applies once discovery has already succeeded and produced a list): the codebase's existing Deferred section already accepts this narrow TOCTOU window as a low-likelihood risk for a single-user tool, not something this story is asked to newly paper over with silent-skip logic.

### Testing note: fakes need per-mapping selective failure, not just global `fail_at`

Every existing port fake fails a *named call* globally (`FakeLuksBackend::with_failure_at("close")` fails **every** `close` call). AC #2 needs one specific mapping's `close`/`umount` to fail while a different mapping in the *same* batch succeeds — impossible to express with the existing mechanism alone. Task 6 adds a second, independent, name-keyed failure set (`with_close_failure_for`/`with_umount_failure_for`) rather than replacing `fail_at` — existing tests that use `fail_at` must keep working unmodified.

### No new `DomainError` variant, no new `cli::ux::translate` arm

`close_all::run`'s per-mapping errors are the same `DomainError` values `close::run` already produces (`AdapterFailure`, `HookRejected`) — `ux::translate` already handles them exhaustively. Only `list_open_mappings`'s own top-level failure is new, and it's an `AdapterFailure` too (no new variant needed).

### No confirmation prompt

Same reasoning `run_close`'s existing doc comment gives for single `close`: closing is fully reversible (a normal `unlock` gets you back in), so it doesn't fit the pattern that justifies `create`'s wipe warning or `revoke`'s irreversible-key warning. Not to be confused with `slam` (Story 4.6, AD-18), whose *zero-confirmation* framing is explicitly contrasted against every other mutating command including this one.

### Project Structure Notes

- New files: `src/domain/workflows/close_all.rs`, `tests/unit/close_all.rs`.
- UPDATE (no other new files):
  - `src/ports/luks_backend.rs` — `list_open_mappings` method.
  - `src/domain/mapping_name.rs` — `MAPPING_NAME_PREFIX` visibility (`pub(crate)`).
  - `src/domain/workflows/close.rs` — extract `close_mapping`.
  - `src/domain/workflows/mod.rs` — register `close_all`.
  - `src/adapters/exec/mod.rs` — `list_open_mappings` impl, `dmsetup` added to `LuksBackend::check_prerequisites`.
  - `src/cli/main.rs` — `CloseAll` variant, `run_close_all`, dispatch.
  - `flake.nix` — add `lvm2`.
  - `tests/unit/fakes.rs` — `FakeLuksBackend`/`FakeFilesystemBackend` extended (Task 6).
  - `tests/unit/workflows.rs` — new preflight-gate test (Task 8).
  - `tests/unit/main.rs` — register `close_all` module.
  - `README.md` — verify only, correct if inaccurate (Task 9).
- No changes to `src/domain/workflows/{create,unlock,enroll,revoke,resize,info}.rs`, `src/ports/{fido2_backend,filesystem_backend}.rs`, `src/domain/hooks.rs`, `src/domain/errors.rs`, `src/cli/ux.rs` — this story reuses every existing hooks/error/translation path unchanged; it only adds discovery + a batch loop.
- **Scope note:** this story is smaller than 4.4 (no new port, no new `DomainError` variant, no new `HookWarning`), but touches the same breadth of files (port, adapter, two workflow files, CLI, fakes, three test files, Nix) — do Task 6's fake-extension and confirm `cargo build --tests` is green before writing any Task 7 test, same discipline every prior Epic 4 story enforced.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 4.5: Close Every Open Tomb (Close-All)]
- [Source: _bmad-output/planning-artifacts/epics.md#Additional Requirements — AD-17, NFR12, "New external tool dependencies (Epic 4)" line]
- [Source: ARCHITECTURE-SPINE.md#AD-17 — Close-all/slam discover open tombs by live mapping-prefix enumeration, never a registry]
- [Source: ARCHITECTURE-SPINE.md#AD-8 — FilesystemBackend port; close's umount-then-luks.close ordering]
- [Source: ARCHITECTURE-SPINE.md#AD-14 — Hooks ordering on close, reused unchanged by this story]
- [Source: ARCHITECTURE-SPINE.md#Deferred — concurrent-invocation/TOCTOU risk, accepted as-is]
- [Source: ARCHITECTURE-SPINE.md#Capability → Architecture Map — CAP-14 row]
- [Source: cryptsetup(8) man page, "Notes on loopback device use" section — verified locally, 2026-07-28]
- [Source: nixpkgs `cryptsetup` package's `bin` output (`cryptsetup`/`veritysetup`/`integritysetup` only, no `dmsetup`) — verified locally via `nix-build '<nixpkgs>' -A cryptsetup`, 2026-07-28]
- [Source: src/ports/luks_backend.rs — current port shape, method this story adds]
- [Source: src/domain/workflows/close.rs — full current implementation, refactored by Task 3]
- [Source: src/domain/mapping_name.rs — `MAPPING_NAME_PREFIX` constant, visibility change]
- [Source: src/adapters/exec/mod.rs:45-49 `privileged()`, :764-789 `LuksBackend::check_prerequisites`, :1041-1058 `close()` — patterns this story's `list_open_mappings` impl follows/extends]
- [Source: src/cli/main.rs:93-102, 503-533, 646-665 — `Close` variant, `run_close`, dispatch — shape `CloseAll`/`run_close_all` mirror]
- [Source: tests/unit/fakes.rs:30-253 `FakeLuksBackend`, :326-697 `FakeFilesystemBackend` — extended by Task 6]
- [Source: tests/unit/close.rs — existing single-close test patterns Task 7 extends to a batch]
- [Source: tests/unit/workflows.rs — existing per-workflow preflight-gate test pattern, extended by Task 8]
- [Source: flake.nix — devShell package list, `psmisc`'s AD-18 comment as the precedent Task 2's `lvm2` addition mirrors]
- [Source: README.md — existing "Close all" row (What it does table) and break-glass section's `dmsetup ls | grep '^vault-'` example, both already anticipating this story's mechanism]
- [Source: _bmad-output/implementation-artifacts/4-4-per-tomb-bind-hooks-exec-hooks-automation.md#Dev Notes — architecture-over-epics gap-resolution precedent this story's Dev Notes reuse]

## Dev Agent Record

### Agent Model Used

Claude Sonnet 5 (Amelia persona, BMad dev-story workflow)

### Debug Log References

None — no failing test loop or crash required debugging; implementation matched the story's Dev Notes on the first pass. One out-of-scope stray edit to `src/domain/workflows/unlock.rs` (a rustfmt reformat picked up incidentally by a full-repo `cargo fmt --check` invocation) was caught via `git diff --stat` before commit and reverted with `git checkout -- src/domain/workflows/unlock.rs`, keeping the working tree scoped to this story's Project Structure Notes list.

### Completion Notes List

- AC #1: `LuksBackend::list_open_mappings` (port) + `ExecAdapter` impl added — `dmsetup ls` (run via `privileged()`, consistent with every other live device-mapper query in this adapter) filtered to `vault-`-prefixed names, then `cryptsetup status <name>`'s `device:` line recovers each mapping's `source_path`. A `cryptsetup status` failure for one name propagates as `list_open_mappings`'s own `Err`, per Dev Notes' "hard stop, never a per-entry skip" resolution.
- AC #1: `close::run` split into path-resolution + a new `pub(crate) close_mapping` helper (hooks → umount → luks.close), reused unchanged by `close_all::run` — one implementation of the ordering, not two.
- AC #1/#2/#3/#4: new `domain::workflows::close_all::run` — preflight first, then `list_open_mappings`, then applies `close_mapping` to every discovered mapping via `.map(...).collect()`, so one mapping's `Err` can never short-circuit the batch (`Iterator::map` isn't `Result`-short-circuiting here since the closure itself never returns `Result`). Empty discovery yields `Ok(vec![])`.
- AC #1: CLI `close-all` subcommand + `run_close_all` — no `path` arg (discovery needs none), reports every mapping's outcome (success to stdout, failure with `ux::translate`'d message to stderr) before a single `std::process::exit(1)` if any failed, never exiting early. "No tombs are currently open." for the empty case.
- Dependency gap resolved in code: `dmsetup` added to `LuksBackend::check_prerequisites`, `lvm2` added to `flake.nix`'s devShell (provides `dmsetup`, confirmed not bundled with nixpkgs' `cryptsetup` package).
- Test fakes extended per Task 6: `FakeLuksBackend::with_open_mappings`/`with_close_failure_for`, `FakeFilesystemBackend::with_umount_failure_for` — the name-keyed failure sets are additive to the existing global `fail_at`, so no existing test needed to change.
- New `tests/unit/close_all.rs` (7 tests) covers: full batch close sequence with mapper identity assertions, one mapping's `close` failure not stopping the batch, one mapping's `umount` failure not stopping the batch, zero-mappings empty-Ok, preflight short-circuit, `skip_hooks` applying to every mapping in the batch, and `list_open_mappings`'s own failure propagating distinctly from a per-mapping failure.
- `tests/unit/workflows.rs` gained `close_all_run_stops_at_preflight_before_touching_any_port`, mirroring the existing per-workflow preflight-gate tests.
- Added a `CloseAllResults` type alias in `close_all.rs` to clear a new clippy "very complex type" lint the story's literal signature would otherwise trip — same underlying `Vec<(MapperHandle, Result<(), DomainError>)>` type, no signature change in substance.
- README.md's existing "Close all" row and break-glass `dmsetup ls | grep '^vault-'` example were re-checked against the shipped implementation and found already accurate — no changes needed.
- Full validation clean: `cargo fmt` (story-touched files only — `unlock.rs`'s pre-existing, out-of-scope formatting drift confirmed present on baseline and left untouched), `cargo build --tests`, `cargo test --test unit` (166 passed), `cargo clippy --all-targets` (0 errors, same 4 pre-existing warnings as baseline, 0 new).
- Hardware verification (LeReverandNox, 2026-07-28): three file-backed tombs (`/home/rlaidet/tmp/tombs/tomb-64m-{1,2,3}.img`) created, unlocked, and closed via `close-all` against real hardware — `dmsetup ls`'s no-entries case, the discovery filter, and the fault-tolerant batch loop all confirmed working. This surfaced a real bug: `close-all` initially printed `Closed /dev/loop0.` instead of the tomb's real path — see the Dev Notes correction above. Fixed in `ExecAdapter::list_open_mappings` and reconfirmed against the same three tombs: `close-all` now reports the correct backing-file paths, and `dmsetup ls` shows zero `vault-*` mappings remaining afterward. This was the last open item from Task 2/9's Dev Notes (`LeReverandNox`'s manual step, same precedent as Stories 4.3/4.4) — story has no further known gaps.
- Process note: task-by-task atomic commits were not made live during implementation — all production and test code was written in one continuous pass, then committed retroactively in task-ordered, file-scoped commits after the fact (see git log), same precedent Story 4.4's own Completion Notes recorded. Tests, checkboxes, and this record all reflect the actual, verified end state.

### File List

- `src/ports/luks_backend.rs` (M) — `list_open_mappings` method added to `LuksBackend` trait.
- `src/domain/mapping_name.rs` (M) — `MAPPING_NAME_PREFIX` changed to `pub(crate)`.
- `src/domain/workflows/close.rs` (M) — `run` split into path-resolution + new `pub(crate) close_mapping` helper.
- `src/domain/workflows/close_all.rs` (A) — new batch-close workflow.
- `src/domain/workflows/mod.rs` (M) — registers `pub mod close_all;`.
- `src/adapters/exec/mod.rs` (M) — `list_open_mappings` impl (`dmsetup ls` + `cryptsetup status` parsing), `dmsetup` added to `check_prerequisites`.
- `src/cli/main.rs` (M) — `CloseAll` variant, `run_close_all`, dispatch wiring.
- `flake.nix` (M) — `lvm2` added to devShell packages.
- `tests/unit/fakes.rs` (M) — `FakeLuksBackend`/`FakeFilesystemBackend` extended (`with_open_mappings`, `with_close_failure_for`, `with_umount_failure_for`, `list_open_mappings` impl).
- `tests/unit/close_all.rs` (A) — new unit test suite for `close_all::run` (7 tests).
- `tests/unit/main.rs` (M) — registers `mod close_all;`.
- `tests/unit/workflows.rs` (M) — new preflight-gate test for `close_all::run`.
- `_bmad-output/implementation-artifacts/4-5-close-every-open-tomb-close-all.md` (M) — story file itself (frontmatter, tasks, Dev Agent Record, Change Log, Status).
- `_bmad-output/implementation-artifacts/sprint-status.yaml` (M) — story status transitions.
