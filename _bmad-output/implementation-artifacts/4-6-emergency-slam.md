---
baseline_commit: ef7cadfcaf54ebdfef874f810da59826afbe8d85
---

# Story 4.6: Emergency Slam

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a user,
I want an emergency command that force-closes every open tomb immediately with no confirmation,
so that in a genuine crisis I can clear everything blocking unmount without being asked to confirm anything.

## Acceptance Criteria

1. **Given** one or more open tombs, with at least one mount currently busy (a process holding it open), **when** I run slam, **then** it discovers open tombs the same way close-all does, and for a busy mount signals every holding process `SIGTERM`, pauses briefly, retries the close; if still busy escalates to `SIGHUP`, pauses, retries; if still busy escalates to `SIGKILL`, pauses, retries — **and** moves on to the next mapping once the close succeeds or no holding process remains, whichever comes first.
2. **Given** slam, **when** it runs, **then** it fires with zero confirmation prompt — unlike every other mutating command in the tool.
3. **Given** a mapping with hooks configured, **when** slam processes it, **then** the hooks step (exec-hooks/bind-hooks teardown) runs exactly once at the start of that mapping's close attempt, never re-run across escalation rounds.
4. **Given** slam is processing several open tombs and one never clears (a process keeps re-acquiring the mount), **when** that happens, **then** it's reported as that one mapping's failure, without blocking slam from completing the rest of the batch.

## Tasks / Subtasks

- [x] **Task 0: Read every file this story touches before changing anything** (prevents guessing at current shapes)
  - Read in full: `src/ports/filesystem_backend.rs`, `src/domain/types.rs`, `src/domain/workflows/close.rs`, `src/domain/workflows/close_all.rs`, `src/domain/workflows/mod.rs`, `src/domain/errors.rs`, `src/domain/preflight.rs`, `src/cli/main.rs` (the `CloseAll`/`Close` variants, `run_close_all`, `run_close`, `print_hook_warning`, dispatch), `src/adapters/exec/mod.rs` (`privileged()` ~line 45, `FilesystemBackend::check_prerequisites` ~line 1416-1436, `umount` ~line 1885, `mount_point_of` ~line 2028, `list_open_mappings` ~line 1163 for the discovery-loop style this story's `slam::run` mirrors), `tests/unit/fakes.rs` (`FakeLuksBackend`, `FakeFilesystemBackend`), `tests/unit/close_all.rs`, `tests/unit/workflows.rs`, `flake.nix`.

- [x] **Task 1: `Pid`/`Signal` domain types + two new `FilesystemBackend` port methods (AC #1)**
  - Add to `src/domain/types.rs`, mirroring `KeyslotRef(pub u32)`'s style:
    ```rust
    /// A holding process's PID, as reported by `fuser -m` (AD-18) — used only
    /// by slam's busy-mount escalation.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Pid(pub u32);

    /// The three signals slam's escalation loop sends, in order (AD-18) — a
    /// typed enum so only `adapters::exec` maps each variant to its `kill -s`
    /// argument; `domain` never handles a raw signal name/number.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Signal {
        Sigterm,
        Sighup,
        Sigkill,
    }
    ```
  - Add to `src/ports/filesystem_backend.rs`'s `FilesystemBackend` trait (both mechanism-only, per AD-18 — the escalation *policy* lives in `domain::workflows::slam`, not here):
    ```rust
    /// Every process ID currently holding `mountpoint` open (`fuser -m`),
    /// used by slam's busy-mount escalation (AD-18) to know who to signal.
    /// `Ok(vec![])` means nothing holds it open — not an error — since the
    /// escalation loop uses an empty result as its own "no holders remain,
    /// stop escalating" exit condition.
    fn processes_using(&self, mountpoint: &Path) -> Result<Vec<Pid>, DomainError>;

    /// Sends `signal` to `pid` (`kill -s <signal> <pid>`). Best-effort from
    /// the caller's perspective — slam's escalation loop ignores a single
    /// failed signal (e.g. the process already exited between
    /// `processes_using` and this call) rather than treating it as fatal.
    fn signal_process(&self, pid: Pid, signal: Signal) -> Result<(), DomainError>;
    ```

- [x] **Task 2: Implement `processes_using`/`signal_process` in `ExecAdapter` (AC #1)**
  - `processes_using`: run `privileged("fuser").arg("-m").arg(mountpoint)` (privileged, same rationale as `list_open_mappings`'s `dmsetup`/`cryptsetup status` calls — seeing every holding process regardless of owning user needs elevation). `fuser` writes only the matched PIDs to stdout (whitespace-separated), everything else (filenames, access-type letters) to stderr — confirmed against `fuser(1)`'s OUTPUT section, not yet confirmed against this exact psmisc build's real output (flag for hardware verification, same precedent as Story 4.5's `dmsetup ls` no-entries text). Parse stdout: `split_whitespace()`, `parse::<u32>()` each token (skip tokens that don't parse — defensive against an unexpected suffix character), map to `Pid`. A **nonzero exit status is not this call's own error** — `fuser` exits 1 when no process matches, which is the normal "no holders" case (empty stdout, empty `Vec`); only a spawn failure (`Command::output()`'s own `Err`) is this method's `Err`.
  - `signal_process`: run `privileged("kill").arg(format!("-{name}", name = signal_name)).arg(pid.0.to_string())` where `signal_name` maps `Signal::Sigterm -> "TERM"`, `Sighup -> "HUP"`, `Sigkill -> "KILL"`. Nonzero exit *is* this call's own `Err` (mirrors `bind_mount`'s pattern) — `domain::workflows::slam` is the one that decides to treat it as best-effort (ignore the `Err`), not this adapter.
  - Add `"fuser"` and `"kill"` to `FilesystemBackend::check_prerequisites`'s checked binaries list (`src/adapters/exec/mod.rs` ~line 1419-1429), matching Story 4.5's precedent of adding `dmsetup` there for its own new dependency. `psmisc` (provides `fuser`) is already in `flake.nix`'s devShell (added ahead of time when the architecture spine was extended for Epic 4, commit `c9732cd`) — confirm it's still there, no new addition needed. `kill` ships in `util-linux`, already present via `coreutils`/base system in the devShell — confirm `kill` resolves on `PATH` in the Nix devShell before assuming no `flake.nix` change is needed; add `util-linux` explicitly if it doesn't (mirrors Story 4.5's `lvm2` addition pattern if this turns out to be necessary).

- [x] **Task 3: Expose `close::run_hooks_step` for reuse (AC #3)**
  - In `src/domain/workflows/close.rs`, change `fn run_hooks_step(...)` (currently private) to `pub(crate) fn run_hooks_step(...)` — no other change. This is the exact function `close_mapping` already calls for the hooks step; `slam::run` (Task 4) needs to call it once per mapping too, before its own escalation loop, without duplicating the hooks/bind-hooks-teardown logic.

- [x] **Task 4: New `domain::workflows::slam` module (AC #1, #2, #3, #4)**
  - New `src/domain/workflows/slam.rs`:
    ```rust
    pub fn run(
        warn: &dyn Fn(HookWarning),
        luks: &dyn LuksBackend,
        fido2: &dyn Fido2Backend,
        fs: &dyn FilesystemBackend,
    ) -> Result<CloseAllResults, DomainError>
    ```
    Reuses `close_all::CloseAllResults` (the `Vec<(MapperHandle, Result<(), DomainError>)>` alias) rather than defining a duplicate type — the per-mapping outcome shape is identical to `close_all`'s.
  - Body: `preflight::check(luks, fido2, fs)?;` then `let mappings = luks.list_open_mappings()?;` (AD-17, same discovery `close_all` uses — a discovery failure is this function's own `Err`) then `.into_iter().map(|mapper| { let result = slam_mapping(&mapper, warn, luks, fs); (mapper, result) }).collect()` — identical batch-isolation shape to `close_all::run` (one mapping's `Err` never stops the others, AC #4).
  - **No `skip_hooks` parameter** — deliberate: epics.md's Story 4.6 ACs never mention a skip-hooks flag for slam (unlike every other close/unlock variant), and slam's whole framing is a zero-configuration panic button — no flags at all, not even the confirmation every other mutating command gets (AC #2). See Dev Notes for the full reasoning; don't add one on the assumption it was just forgotten.
  - New private `fn slam_mapping(mapper: &MapperHandle, warn: &dyn Fn(HookWarning), luks: &dyn LuksBackend, fs: &dyn FilesystemBackend) -> Result<(), DomainError>`:
    1. Run the hooks step **exactly once**: `close::run_hooks_step(mapper, warn, fs)`, tolerating the same `"not currently mounted"` `AdapterFailure` substring `close_mapping` already tolerates (a prior partial close), propagating any other `Err` immediately (AC #3 — this call happens once, before any escalation, and is never repeated inside the loop below).
    2. Attempt `fs.umount(mapper)`. `Ok(())` or a `"not currently mounted"` `AdapterFailure` → proceed straight to `luks.close(mapper)` and return its result (busy-escalation was never needed).
    3. Otherwise (busy), fetch `let mountpoint = fs.mount_point_of(mapper)?;` **once**, before the escalation loop (the mount is still live at this point — this call doesn't need repeating each round).
    4. Loop over `[Signal::Sigterm, Signal::Sighup, Signal::Sigkill]` in order (AC #1): each round, call `fs.processes_using(&mountpoint)?`; if empty, `break` immediately (AC #1's "no holding process remains" exit — do not attempt remaining signals, do not retry `umount` again for this reason: an empty-holders-but-still-busy state is treated as this mapping's failure, per Dev Notes). Otherwise, `fs.signal_process(pid, signal)` for every returned `Pid`, ignoring each call's individual `Result` (`let _ = ...`, best-effort — Task 2's note). Sleep exactly 1 second (`std::thread::sleep(std::time::Duration::from_secs(1))`, a fixed constant, not user-configurable — AD-18). Retry `fs.umount(mapper)`: success or `"not currently mounted"` → `luks.close(mapper)` and return; otherwise keep the error and continue to the next signal in the loop.
    5. After the loop ends (every signal tried and still busy, or broken early on empty holders), return the last-seen `umount` error as this mapping's `Err` (AC #4).
  - Register `pub mod slam;` in `src/domain/workflows/mod.rs`.

- [x] **Task 5: `cli` — `slam` subcommand + `run_slam` (AC #1, #2, #3, #4)**
  - In `src/cli/main.rs`'s `Commands` enum, add a zero-argument variant (no `path`, no `skip_hooks`, no confirmation flag — nothing to configure, per Task 4's design decision):
    ```rust
    /// Emergency: force-close every open tomb immediately, escalating past
    /// any busy mount, with zero confirmation
    Slam,
    ```
  - Add `fn run_slam()`, mirroring `run_close_all`'s shape (preflight check first, same `print_hook_warning` seam, same per-mapping success/failure reporting loop with a single `std::process::exit(1)` after every line is printed) but with **no confirmation prompt of any kind and no pre-run "checking" framing** — the intro line should read as immediate/urgent plain language (NFR3), e.g. `"Slamming every open tomb — closing immediately, no confirmation."`, never implying a pause or a check step exists. Reuse `close_all::CloseAllResults`' reporting shape verbatim: `"Closed {source_path}."` on success, `"Failed to close {source_path}: {translated error}"` (via `ux::translate`) to stderr on failure, `"No tombs are currently open."` for the empty case.
  - Wire `Commands::Slam => run_slam(),` into `run()`'s dispatch match.

- [x] **Task 6: Extend test fakes for slam coverage (AC #1, #3, #4)**
  - `FakeFilesystemBackend` (`tests/unit/fakes.rs`):
    - `processes_using_result: RefCell<Vec<Pid>>` (default empty) + `pub fn with_processes_using(self, pids: Vec<Pid>) -> Self`; implement `processes_using` (log `"processes_using"`, `fail_if("processes_using")`, return the field's clone).
    - `umount_fail_times: Cell<u32>` (default `0`) + `pub fn with_umount_fail_times(self, times: u32) -> Self` — when `> 0`, each `umount` call decrements it and returns a generic busy `AdapterFailure` (**not** the `"not currently mounted"` marker — this must look like a real busy-mount failure, distinct from the idempotent-retry case); once it reaches `0`, `umount` falls through to the existing checks (`umount_not_currently_mounted`, `umount_failure_for`, `fail_at`) unaffected — this is a new, independent mechanism, default `0` means zero existing tests change behavior.
    - `last_signal_calls: RefCell<Vec<(Pid, Signal)>>` + `pub fn signal_calls(&self) -> Vec<(Pid, Signal)>`; implement `signal_process` (log `"signal_process"`, push `(pid, signal)`, `fail_if("signal_process")`).
    - Import `Pid`/`Signal` from `tomb_fido2::domain::types` at the top of `fakes.rs`.
  - `FakeLuksBackend` needs no changes — `list_open_mappings`/`close`/`close_failure_for` (Story 4.5) already cover everything `slam::run`'s discovery and final-close calls need.

- [ ] **Task 7: Unit tests — new `tests/unit/slam.rs` (AC #1, #3, #4)**
  - `escalates_through_sigterm_sighup_sigkill_until_umount_succeeds`: one mapping, `with_umount_fail_times(2)` (busy on the initial attempt and the SIGTERM-round retry, succeeds on the SIGHUP-round retry — asserts escalation stops as soon as it clears, not always running all three), `with_processes_using(vec![Pid(111)])`. Assert `fs.signal_calls()` is exactly `[(Pid(111), Signal::Sigterm), (Pid(111), Signal::Sighup)]` (SIGKILL never needed) and the mapping's final result is `Ok(())`. Assert the call log shows `mount_point_of` (hooks step) exactly once and a second, separate `mount_point_of` (escalation prep) exactly once — never once per round.
  - `no_holders_remaining_stops_escalation_and_reports_that_mappings_failure`: one mapping, `with_umount_fail_times(u32::MAX)` (always busy) and default empty `processes_using`. Assert zero `signal_process` calls happened (breaks before ever signaling), and the mapping's result is `Err`.
  - `hooks_step_runs_exactly_once_never_repeated_across_escalation_rounds` (AC #3): one mapping needing 2 rounds to clear (`with_umount_fail_times(1)`, non-empty `processes_using`) with a real `bind-hooks`/`exec-hooks` setup (or simplest: assert the hooks-step's own call signature — `mount_point_of` + 2×`path_exists`, both hooks files absent — appears exactly once in the full log, not duplicated per round).
  - `one_mappings_never_clearing_does_not_stop_the_batch` (AC #4): two discovered mappings, mapping A `with_umount_fail_times(u32::MAX)` + empty `processes_using` (never clears), mapping B passing normally — assert A's result is `Err`, B's is `Ok`, and B's full sequence (hooks, umount, close) still ran.
  - `zero_open_mappings_yields_ok_empty_vec`: mirrors `close_all.rs`'s own version.
  - `preflight_failure_short_circuits_before_list_open_mappings_is_called`: mirrors `close_all.rs`'s own version.
  - `list_open_mappings_failure_propagates_as_slams_own_err`: mirrors `close_all.rs`'s own version.
  - Register `mod slam;` in `tests/unit/main.rs` (alphabetically after `resize`, before `unlock` — confirm exact ordering against the current file, it's currently alphabetical).

- [ ] **Task 8: `tests/unit/workflows.rs` — slam's preflight-gate test (AC #2 implicitly, consistency)**
  - Add `slam_run_stops_at_preflight_before_touching_any_port`, mirroring `close_all_run_stops_at_preflight_before_touching_any_port` (a failing `FakeLuksBackend` must short-circuit to `DomainError::PreflightFailed` before `list_open_mappings`/any workflow logic runs).

- [ ] **Task 9: `cli` help text + docs pass (AC: all)**
  - Confirm `slam --help` text is clear with no jargon assumed (`cargo run -- slam --help`), same standard every prior Epic 4 story applied.
  - README.md already has "Slam" and "Close all" rows in its "What it does" table (written ahead of implementation, see line 24) — re-read against what actually ships and correct only if inaccurate; do not restate as new work if already matching.
  - Run `cargo fmt`, `cargo build --tests`, `cargo test --test unit`, `cargo clippy --all-targets` (all must be clean, matching every prior Epic 4 story's exit bar) before marking this story done. `make test-hardware`'s manual run (verifying `fuser -m`'s real stdout format from Task 2, and a real busy-mount escalation against a process actually holding a tomb open) is `LeReverandNox`'s step, not a new automated hardware test — same precedent Stories 4.3/4.4/4.5 established for hardware-only verification. This is also Epic 4's **last** story — flag to `LeReverandNox` that `epic-4-retrospective` (currently `optional` in `sprint-status.yaml`) becomes eligible to run once this story reaches `done`.

## Dev Notes

### Resolved: slam takes no `skip_hooks` flag, unlike every other close/open workflow

Every other mutating workflow (`unlock`, `close`, `close_all`) threads a `skip_hooks: bool` end-to-end from a CLI flag. Story 4.6's epics.md ACs (re-read in full before writing this story) never mention one for slam — its ACs only ever discuss the escalation loop, zero confirmation, and once-per-mapping hooks execution. Slam's entire framing (NFR13, AD-18) is "the emergency button" with the *general* confirm-before-irreversible-action pattern deliberately overridden — extending that same "no configuration surface, just go" reasoning to flags (not only prompts) keeps the command a true one-word panic button. A hung/misbehaving hook script blocking slam is a pre-existing risk category `close_all` already carries unaddressed (hooks run synchronously, no timeout anywhere in this codebase) — not something this story introduces or is asked to fix. If `LeReverandNox` wants a `--skip-hooks` escape hatch on slam later, it's a small additive change, not a redesign.

### Resolved: escalation prep's `mount_point_of` call happens once, not per round

The hooks step (`close::run_hooks_step`) already resolves the mountpoint once internally for its own purposes (building exec-hook argv, locating `bind-hooks`). After it returns, the mount is still live (nothing has unmounted it yet) — so `slam_mapping` only needs a second, independent `fs.mount_point_of` call once, right before the escalation loop starts, to get the path `processes_using` needs. Re-fetching it every round would be redundant (the mount doesn't move mid-escalation) and would make call-count assertions in tests misleading about how many distinct queries are actually happening.

### Resolved: an empty `processes_using` result stops escalation immediately, even before the first signal

AD-18's text: "moving to the next mapping once umount succeeds or no holders remain, whichever comes first." If `processes_using` ever returns empty (including on the very first check, before SIGTERM is sent) while `umount` is still failing, there is nothing left to signal — continuing to loop through SIGHUP/SIGKILL would accomplish nothing. This is treated as this mapping's failure (per Dev Notes precedent language from Story 4.5's "a mapping that never clears is one of AD-17's per-mapping failures, not a fatal error for the batch") rather than an infinite/extended retry.

### Resolved: `signal_process` failures are best-effort at the `domain` layer, not the adapter's

A single process might already have exited between `processes_using` reporting it and `signal_process` targeting it (a real, expected race in an emergency-close scenario) — `kill`'s own nonzero exit for "no such process" would otherwise abort the whole mapping's escalation attempt for no good reason. `ExecAdapter::signal_process` still reports a genuine `Err` (consistent with every other port method's honesty), but `domain::workflows::slam` is the one that decides to ignore it (`let _ = ...`) and keep escalating the remaining PIDs/rounds — same "policy in domain, mechanism in the adapter" split AD-18 states explicitly for the whole slam feature.

### No new `DomainError` variant, no new `cli::ux::translate` arm

Every error `slam::run`/`slam_mapping` can produce (`PreflightFailed`, `AdapterFailure` from `list_open_mappings`/`umount`/`processes_using`/`hooks step`) is already a variant `ux::translate` handles. `signal_process`'s own `Err` never escapes `slam_mapping` (ignored per the note above), so it never reaches `ux::translate` at all.

### `psmisc`/`fuser` already anticipated in `flake.nix`, `kill` needs confirming

`flake.nix` already carries `psmisc # fuser/kill, slam's busy-mount escalation (AD-18)` (added in commit `c9732cd`, the architecture-spine extension for Epic 4, well ahead of this story) — but that comment's claim that `psmisc` also provides `kill` is very likely wrong: `kill` (the standalone binary, distinct from any shell builtin) ships in `util-linux`, not `psmisc`. Verify which package actually provides a `kill` binary on `PATH` inside the devShell before assuming Task 2 needs no `flake.nix` change — if `kill` isn't already resolvable (e.g. via `coreutils`/`util-linux` already present transitively), add `util-linux` explicitly and correct the stale/inaccurate `psmisc` comment to stop claiming it provides `kill`.

### Project Structure Notes

- New files: `src/domain/workflows/slam.rs`, `tests/unit/slam.rs`.
- UPDATE (no other new files):
  - `src/domain/types.rs` — `Pid`, `Signal` added.
  - `src/ports/filesystem_backend.rs` — `processes_using`, `signal_process` methods.
  - `src/domain/workflows/close.rs` — `run_hooks_step` visibility `fn` → `pub(crate) fn` (one-line change, no logic change).
  - `src/domain/workflows/mod.rs` — registers `pub mod slam;`.
  - `src/adapters/exec/mod.rs` — `processes_using`/`signal_process` impls, `fuser`/`kill` added to `FilesystemBackend::check_prerequisites`.
  - `src/cli/main.rs` — `Slam` variant, `run_slam`, dispatch wiring.
  - `flake.nix` — only if Task 2's `kill`-provenance check finds it missing; otherwise verify-only (correct the stale comment either way).
  - `tests/unit/fakes.rs` — `FakeFilesystemBackend` extended (Task 6).
  - `tests/unit/workflows.rs` — new preflight-gate test (Task 8).
  - `tests/unit/main.rs` — registers `mod slam;`.
  - `README.md` — verify only, correct if inaccurate (Task 9).
- No changes to `src/domain/workflows/{create,unlock,enroll,revoke,resize,info,close_all}.rs`, `src/ports/{fido2_backend,luks_backend}.rs`, `src/domain/hooks.rs`, `src/domain/errors.rs`, `src/cli/ux.rs` — this story reuses `close_all`'s discovery/batch-collection shape and `close::run_hooks_step` unchanged; it adds only the escalation loop and two new port methods.
- **Scope note:** this is Epic 4's last story — after it reaches `done`, `epic-4-retrospective` in `sprint-status.yaml` (currently `optional`) becomes eligible to run (Task 9 flags this to `LeReverandNox`, not something the dev agent should initiate itself).

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 4.6: Emergency Slam]
- [Source: _bmad-output/planning-artifacts/epics.md#Additional Requirements — AD-18, NFR13, "New external tool dependencies (Epic 4)" line]
- [Source: ARCHITECTURE-SPINE.md#AD-18 — Slam's busy-mount escalation: policy in domain, mechanism in the adapter, no confirmation]
- [Source: ARCHITECTURE-SPINE.md#AD-17 — reused verbatim for discovery/batch-collection shape]
- [Source: ARCHITECTURE-SPINE.md#Deferred — "Slam's 1-second inter-escalation pause is a fixed default, not user-configurable in v1"; the TOCTOU note on a process reacquiring a mount between the last kill and final umount retry]
- [Source: ARCHITECTURE-SPINE.md#Capability → Architecture Map — CAP-15 row]
- [Source: ARCHITECTURE-SPINE.md external-deps table — psmisc/fuser, util-linux/kill rows]
- [Source: src/ports/filesystem_backend.rs — current port shape, two methods this story adds]
- [Source: src/domain/workflows/close.rs — `run_hooks_step`/`close_mapping`, visibility change and reuse]
- [Source: src/domain/workflows/close_all.rs — `CloseAllResults` type alias and `run`'s discovery/collect shape, both reused verbatim]
- [Source: src/adapters/exec/mod.rs:45-49 `privileged()`, :1163-1234 `list_open_mappings` (discovery-loop precedent), :1885-1924 `umount`, :2028-2055 `mount_point_of`, :1416-1436 `FilesystemBackend::check_prerequisites` — patterns this story's new methods follow/extend]
- [Source: src/cli/main.rs:105-110, 550-598 — `CloseAll` variant, `run_close_all` — shape `Slam`/`run_slam` mirror]
- [Source: tests/unit/fakes.rs:30-360 `FakeLuksBackend`/`FakeFilesystemBackend` — extended by Task 6]
- [Source: tests/unit/close_all.rs — existing batch-workflow test patterns Task 7 extends with escalation-specific scenarios]
- [Source: tests/unit/workflows.rs — existing per-workflow preflight-gate test pattern, extended by Task 8]
- [Source: flake.nix — `psmisc` line added in commit c9732cd, ahead of this story; comment's `kill`-provenance claim needs verifying]
- [Source: README.md — existing "Close all"/"Slam" rows (What it does table), both already anticipating this story's mechanism]
- [Source: _bmad-output/implementation-artifacts/4-5-close-every-open-tomb-close-all.md — direct predecessor: `list_open_mappings`, `CloseAllResults`, `close_mapping`/`run_hooks_step`, and the fakes' per-mapping selective-failure pattern this story builds on]
- [Source: fuser(1) man page — OUTPUT section (PIDs to stdout, diagnostics to stderr); confirm against real psmisc build during hardware verification, same precedent as Story 4.5's `dmsetup ls` no-entries text]

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

### File List
