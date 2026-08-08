---
baseline_commit: 6dde5fde519f191896bd568f149aa891039ffee0
---

# Story 6.1: Crash-Safe Create Resume

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a user,
I want to re-run create against the same destination after a crash or interruption during a prior create attempt,
so that I get a clean, fully-created volume instead of being stuck with an unrecoverable partial one or a false "already exists" refusal.

## Acceptance Criteria

1. **Given** a file-backed create that was interrupted after `luksFormat` but before final cleanup (the marker token is still present), **when** I re-run create against the same destination, **then** the tool detects the marker via `has_marker_token`, proceeds with no confirmation, and produces the same fully-created, unlockable volume as an uninterrupted run. [Source: epics.md#Story 6.1]
2. **Given** a device-backed create interrupted the same way, **when** I re-run create against the same device, **then** the same marker-verified resume applies, and mandatory size resolution against `device_capacity` still runs even though the confirmation prompt is skipped — a device shrunk since the crashed attempt is still caught. [Source: epics.md#Story 6.1]
3. **Given** a destination whose LUKS2 header has no marker token (a genuine pre-existing file/volume), **when** I run create against it, **then** it refuses exactly as before CAP-23, unchanged. [Source: epics.md#Story 6.1]
4. **Given** create completes successfully (interrupted-then-resumed, or a normal uninterrupted run), **when** final cleanup runs, **then** the marker token is removed first and the transient bootstrap keyslot second, in that order, matching AD-5's safe-ordering reasoning. [Source: epics.md#Story 6.1]
5. **Given** the corrected real execution order (AD-9 amendment), **when** create runs, interrupted or not, **then** FIDO2 key enrollment happens before `mkfs`, since the transient bootstrap passphrase is the only valid credential to authenticate `systemd-cryptenroll` with at that point. [Source: epics.md#Story 6.1]

## Tasks / Subtasks

- [x] **Task 0: Read every file this story touches before changing anything, then spike the marker-token cryptsetup mechanics** (AC: #1, #2, #3, #4)
  - Read in full: `src/domain/workflows/create.rs` (217 lines), `src/ports/luks_backend.rs` (79 lines), `src/adapters/exec/mod.rs` lines 686-960 (the `LuksBackend` impl block covering `has_luks2_header`, `bootstrap_format_and_open`, `write_fido2_token_metadata`), `src/domain/keyslot_guard.rs`, `src/domain/errors.rs`, `tests/unit/fakes.rs` (`FakeLuksBackend`, lines ~30-260), `tests/unit/create.rs`.
  - **Spike (throwaway, real hardware/devshell) before writing production code**, same discipline as prior stories' Task 0 spikes (Story 1.5's token-tolerance spike, Story 3.2's `resize --token-only` spike): confirm the exact `cryptsetup token import` invocation that adds a **brand-new** token (not replacing an existing one) with a custom `type` string and an empty `keyslots` array, and confirm whether omitting `--token-id` auto-assigns the next free slot or is rejected. AD-2 already flags this general mechanism ("fallback if [systemd-fido2 extra fields are] rejected... write a second, sibling LUKS2 token of a distinct custom type") as spike-then-implement, and CAP-23 is the first *real* use of it — nothing today already proves the exact command syntax. Record the confirmed command in Dev Notes before Task 1.
  - Confirm empirically that `cryptsetup token export --token-id <marker-id>` (or a `luksDump --dump-json-metadata` scan, matching `find_systemd_fido2_token_ids`'s existing pattern at `src/adapters/exec/mod.rs:397`) is a reliable way to detect the marker's presence/id for both the read (`has_marker_token`) and removal (`remove_marker_token`) paths.

- [x] **Task 1: Add `has_marker_token` and `remove_marker_token` to the `LuksBackend` port** (AC: #1, #2, #3, #4)
  - `src/ports/luks_backend.rs`: add two trait methods next to `has_luks2_header`:
    - `fn has_marker_token(&self, path: &Path) -> Result<bool, DomainError>` — doc-comment per AD-9's exact contract: "`true` only for a path with a valid LUKS2 header carrying the marker; `false` for no header, an unreadable/invalid header, or a valid header without the marker" — a single self-contained check, safe to call on any path regardless of what's already been verified about it.
    - `fn remove_marker_token(&self, path: &Path) -> Result<(), DomainError>` — removes the marker token written by `bootstrap_format_and_open`; called only after a fully successful create, before the bootstrap keyslot is removed (AC #4).
  - Update every other `LuksBackend` implementor: `tests/unit/fakes.rs`'s `FakeLuksBackend` needs both methods added (mirror the `has_luks2_header` field/builder pattern: a `has_marker_token: bool` field, `.with_has_marker_token(bool)` builder, both constructors (`passing()`/`failing()`) defaulting it `false`; log both calls the same way `"has_luks2_header"` is logged at `tests/unit/fakes.rs:209`; `remove_marker_token` just logs and respects `fail_at`, no return value to fake).

- [x] **Task 2: Write the marker token inside `bootstrap_format_and_open`** (AC: #1, #2)
  - `src/adapters/exec/mod.rs`, inside `impl LuksBackend for ExecAdapter { fn bootstrap_format_and_open ... }` (currently lines 826-947): immediately after the `luksFormat` call succeeds (after line 852) and before the `luksOpen` call (line 854), add the marker-token write using the command confirmed in Task 0's spike. Empty `keyslots` array, distinct custom `type` string (not `systemd-fido2` — this is the AD-2 fallback token type, not an extra field on the enrollment token).
  - Per the architecture spine: "the CAP-23 marker-token write is deliberately **not** a stage of its own — it happens inside `bootstrap_format_and_open`, folded into the existing `FormattingLuks2` progress window" — do not add a new `CreateStage` variant for it.

- [x] **Task 3: Implement `has_marker_token` and `remove_marker_token` in `ExecAdapter`** (AC: #1, #2, #3, #4)
  - `has_marker_token`: reuse the existing `dump_json_metadata`/`tokens_object` helpers (`src/adapters/exec/mod.rs:328-356`) the same way `systemd_fido2_token_ids` does (lines 383-390) — scan the tokens object for any token whose `type` matches the marker's custom type string. Must tolerate "no header at all" and "unreadable header" by returning `Ok(false)`, not propagating `AdapterFailure` — check how `has_luks2_header` (lines 799-824) distinguishes exit codes, but note this method's contract is looser (any non-marker-having state is `false`, not just "not LUKS2").
  - `remove_marker_token`: locate the marker token's id the same way, then `cryptsetup token remove --token-id <id>` (or equivalent already-established removal command if one exists in this codebase — check `remove_key`'s implementation for the token-removal half of AD-5's "token first, then keyslot" pattern before inventing a new command).

- [x] **Task 4: Wire marker-verified resume into `domain::workflows::create::run`'s File branch** (AC: #1, #3)
  - `src/domain/workflows/create.rs:53-64`: today, `if fs.path_exists(&path) { return Err(DomainError::DestinationExists(path)); }` unconditionally refuses. Change to: if `fs.path_exists(&path)` is true, call `luks.has_marker_token(&path)?` — `true` means proceed exactly as if the path hadn't existed (fall through to the existing `MIN_VOLUME_SIZE_BYTES` check and `AllocatingBackingFile` progress/`set_backing_file_size` call below, **no confirmation prompt**); `false` means refuse with `DestinationExists(path)`, unchanged from today.
  - Note `set_backing_file_size` on a resume path re-allocates over the existing (marker-only, valueless) backing file — this is correct and matches "produces the same fully-created, unlockable volume as an uninterrupted run" (AC #1), not a bug to guard against.

- [x] **Task 5: Wire marker-verified resume into the Device branch** (AC: #2, #3)
  - `src/domain/workflows/create.rs:100-118`: today, `if luks.has_luks2_header(&path)? { return Err(DomainError::DeviceAlreadyFormatted(path)); }` refuses unconditionally on any header. Change to: if `has_luks2_header` is true, call `luks.has_marker_token(&path)?` — `true` means marker-verified resume: skip straight past the `if !confirmed` check (do not evaluate it at all on this path) directly into size resolution; `false` means refuse `DeviceAlreadyFormatted(path)`, unchanged. If `has_luks2_header` is false, behavior is unchanged (falls through to the existing `if !confirmed` gate).
  - **Size resolution/`device_capacity` validation (lines 107-131) must run unconditionally on every path that reaches it — marker-verified resume included.** This is AC #2's explicit requirement: confirmation is the only thing that diverges by path; a device shrunk since the crashed attempt must still be caught by the existing `DeviceSizeExceedsCapacity`/`DeviceTooSmall` checks, unmodified.
  - Do not restructure the `match target { CreateTarget::Device { path, size, confirmed } => ... }` destructuring — `confirmed` is simply not read on the marker-verified branch, not removed from the type (`CreateTarget` is unchanged this story; no new field).

- [x] **Task 6: Final-cleanup ordering — marker token removed before the bootstrap keyslot** (AC: #4)
  - `src/domain/workflows/create.rs:189-217` (`finish_provisioning`): today it ends with `keyslot_guard::remove_keyslot_guarded(luks, &mapper.source_path, BOOTSTRAP_KEYSLOT)` as the function's return expression. Insert `luks.remove_marker_token(&mapper.source_path)?;` immediately before that call, so the marker is removed first and the guarded keyslot removal second — this exact order matters (see AD-9's reasoning: removing the marker first means a crash before keyslot cleanup leaves a harmless stray keyslot, correctly read as "genuine pre-existing volume" by a future create's `has_marker_token` check; the reverse order would let a future create misread a completed volume's surviving marker as "resumable" and silently wipe it).
  - Confirm this ordering is reachable from both branches (File and Device) — `finish_provisioning` is shared by both via `bootstrap_and_provision`, so one change covers both ACs #1 and #2's "produces the same fully-created ... volume" requirement.

- [x] **Task 7: Confirm AC #5 (FIDO2-before-mkfs ordering) needs no code change, only a regression guard** (AC: #5)
  - `finish_provisioning` (`src/domain/workflows/create.rs:189-217`) already enrolls FIDO2 (`fido2.enroll_fido2_key`, progress `EnrollingFido2Key`) before `fs.mkfs` (progress `CreatingFilesystem`) — this ordering already matches AD-9's amendment and predates this story. No production change needed here; this task is a checklist reminder not to accidentally reorder these two calls while touching this function for Task 6, and to keep or extend `happy_path_runs_every_port_call_once_in_order` (`tests/unit/create.rs:109`) as the regression guard proving the order.

- [ ] **Task 8: Unit tests** (AC: #1, #2, #3, #4)
  - Extend `tests/unit/create.rs` (uses `FakeLuksBackend`/`RealFixtureFile` from `tests/unit/fakes.rs`), following the file's existing naming/structure convention (`refuses_...`, `happy_path_...`, `..._failure_...`):
    - File-backed resume: destination exists + `has_marker_token` true → proceeds through the full happy path with no confirmation-related call, same port-call sequence as `happy_path_runs_every_port_call_once_in_order`.
    - File-backed non-resume unchanged: destination exists + `has_marker_token` false → still returns `DestinationExists`, `has_marker_token` is still called (proves the check runs, not just the old short-circuit).
    - Device-backed resume: `has_luks2_header` true + `has_marker_token` true + `confirmed: false` → proceeds (proves confirmation is genuinely skipped, not just defaulted).
    - Device-backed resume still enforces size: `has_marker_token` true but requested/defaulted size exceeds `device_capacity` → still returns `DeviceSizeExceedsCapacity`/`DeviceTooSmall`, proving AC #2's "mandatory size resolution ... still runs."
    - Device-backed header-without-marker unchanged: `has_luks2_header` true + `has_marker_token` false → `DeviceAlreadyFormatted`, unchanged even when `confirmed: true`.
    - Cleanup ordering: assert `remove_marker_token` is logged before `remove_key`/the guarded keyslot removal in the call log on a full happy-path run (both File and Device).
  - Add `FakeLuksBackend::with_has_marker_token(bool)` per Task 1; if a test needs marker removal itself to fail (to prove ordering matters), extend `fail_at` handling for `"remove_marker_token"` the same way other methods use it.

- [ ] **Task 9: Full regression pass**
  - `cargo build` succeeds and `make test` passes with all prior 174 tests plus this story's new ones green.
  - Note in Completion Notes the exact new total test count (mirrors the discipline established in Story 5.4's and Epic 5's completion notes of citing exact before/after counts).
  - If hardware is available: manually crash-simulate (e.g. `kill -9` the process, or a temporary early `return`/`panic!` inserted and reverted) a file-backed and a device-backed create between `luksFormat` and final cleanup, then re-run create against the same destination and confirm it resumes to a fully working, unlockable volume — this is the one behavior unit tests with fakes cannot prove end-to-end. If no hardware is available in this session, say so explicitly (per this project's standing convention — see Dev Notes) and flag it as a retrospective action item for `LeReverandNox` to verify later, same as Story 5.2's and 5.1's hardware-verification action items.

## Dev Notes

- **No new port, no new architectural layer.** Epic 6's spine section is explicit: CAP-23 "amend[s] `create`'s existing AD-9 sequence in place, closing AD-9's own documented gap rather than opening a parallel workflow." This story adds two methods to the existing `LuksBackend` trait and edits `create.rs` in place — it does not touch `CreateTarget`, `Filesystem`, `KeyMetadata`, or any CLI flag/signature. [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md:38]
- **The marker token is a second, distinct LUKS2 token type — not an extra field on the `systemd-fido2` token.** AD-2 documents this as its own previously-flagged fallback mechanism ("write a second, sibling LUKS2 token of a distinct custom type referencing the same keyslot number... still no sidecar file") and the spine explicitly marks it "**Realized (Epic 6, CAP-23)**": "that fallback mechanism is now used for real, for an unrelated purpose... This does **not** resolve this AD's own Open item — whether the `systemd-fido2` token itself tolerates unknown extra fields is still untested; CAP-23's marker sidesteps that question entirely by being a wholly separate token object." Do not attempt to fold marker state into the `systemd-fido2` token's own JSON — that is a different, still-open question this story does not need to touch. [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md:52]
- **The marker is inert by construction.** "An inert token with an empty `keyslots` array" — it references no keyslot, so its mere presence has zero effect on unlock/open behavior; it exists purely as a detectable flag for `create`'s own refuse-vs-resume decision. [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md:96]
- **Ordering is the whole point of AC #4 — get it backwards and the fix becomes a data-loss bug.** Quoting the spine directly: "removing the marker first means a crash before keyslot cleanup leaves a stray-but-harmless transient keyslot on an already-functional tomb... the next `create` attempt sees `has_marker_token = false` and correctly refuses, matching the genuine pre-existing volume case; removing the keyslot first would instead leave the marker sitting on a fully-completed tomb, and a future `create` would read that surviving marker as resumable and **silently wipe a working, fully-enrolled tomb**." This is the single highest-severity thing to get right in this story. [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md:101]
- **Existing rollback-on-failure discipline must keep working across both branches.** `bootstrap_and_provision` (`src/domain/workflows/create.rs:151-187`) already closes the mapping (or wraps a close failure via `with_rollback_cleanup_failure`) on any error from `finish_provisioning` — adding `remove_marker_token` as a new fallible call inside `finish_provisioning` means its errors flow through this exact same existing rollback path with no special-casing needed, since `finish_provisioning`'s signature and error type are unchanged.
- **File-backed resume re-allocates over the existing partial backing file — this is intentional, not a leak to guard against.** `fs.set_backing_file_size` is idempotent-by-design for this purpose; a resumed create calling it again on a path that already exists (with only a marker-carrying LUKS2 header, no other salvageable state) is the mechanism, not a side effect to suppress.
- **Recurring review-pattern watchlist from Epic 4/5 retros — apply proactively, do not wait for review to catch these:**
  - New pure parsing/logic functions shipping without a unit test hit 3 times in Epic 4 (retro action item, still open/watched). If Task 3's marker-id-extraction logic is factored into its own helper (mirroring `find_systemd_fido2_token_ids`), give it a direct unit test, not just indirect coverage via `has_marker_token`/`remove_marker_token`.
  - Self-reported completion-note claims not matching actual grep/build/test output hit in all 4 Epic 5 stories, caught only in review (Amelia-owned retro action item, still open). Verify every count/claim in Task 9's Completion Notes against real command output before writing it down.
  - Marker-bleed across workflows sharing `adapters::exec` error strings hit 3 times across Epic 2/3 (retro action item, in-progress). This story adds new adapter error paths (`has_marker_token`, `remove_marker_token`) — check `src/cli/ux.rs`'s plain-language translation doesn't accidentally swallow or misroute these into an unrelated workflow's error bucket.
- **Hardware verification convention:** this codebase's standing pattern (Stories 4.3, 5.1, 5.2 action items) is to explicitly state in Completion Notes whether real hardware was available and what was/wasn't verified on it, rather than silently claiming full verification from fakes/mocks alone. Follow that here for Task 9's crash-simulation check.

- **Task 0 spike findings (confirmed empirically, scratch 32 MiB LUKS2 file in `/tmp`, real `cryptsetup 2.8.6` from this repo's Nix devshell, 2026-08-08):**
  - `echo '{"type":"hypogaol-create-marker","keyslots":[]}' | cryptsetup token import <path>` with **no `--token-id`** creates a brand-new token and auto-assigns the next free token id (confirmed id `0` on an empty header, then id `1` for a second distinct-type token added afterward) — it does not require or default to replacing an existing token. `--token-replace` (used by `write_fido2_token_metadata` for editing an already-known id) is not needed here.
  - `cryptsetup token export --token-id <id> <path>` reliably reads back the exact JSON written, confirming the export-based detection pattern works the same way for the marker as it does for `write_fido2_token_metadata`'s existing systemd-fido2 use.
  - `cryptsetup token remove --token-id <id> <path>` cleanly removes just that token, leaving any other tokens (e.g. a `systemd-fido2` one) untouched.
  - Detection should follow the existing `dump_json_metadata`/`tokens_object` scan pattern (`find_systemd_fido2_token_ids`, `src/adapters/exec/mod.rs:397`), matching on `type == "hypogaol-create-marker"` — no new JSON-parsing approach needed.
  - Chosen marker token type string: `hypogaol-create-marker` (distinct from `systemd-fido2`, matches the project's rebranded name per Epic 5).

### Project Structure Notes

- Files touched (production): `src/ports/luks_backend.rs` (2 new trait methods), `src/adapters/exec/mod.rs` (implement the 2 new methods; edit `bootstrap_format_and_open` to write the marker), `src/domain/workflows/create.rs` (File/Device branch resume logic, final-cleanup ordering).
- Files touched (tests): `tests/unit/fakes.rs` (`FakeLuksBackend` gains the 2 new methods + builder), `tests/unit/create.rs` (new resume/ordering test cases).
- No new files, no new modules, no CLI flag/signature changes, no `Filesystem`/`CreateTarget`/`KeyMetadata` changes.
- Alignment with the documented source tree: `luks_backend.rs`'s doc-comment inventory at `ARCHITECTURE-SPINE.md:238` already lists `has_marker_token(AD-9, CAP-23)` as belonging to this trait — this story is implementing what the spine already scoped there, not introducing a new location.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 6.1: Crash-Safe Create Resume, lines 770-796] — acceptance criteria origin, verbatim.
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 6: Volume Resilience, Filesystem Choice & Everyday Polish, lines 766-768] — epic-level framing; confirms no new port/layer for any Epic 6 story.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-2 — No local state / LUKS2-header-only persistence, line 52] — marker token's status as AD-2's own previously-flagged fallback mechanism, "Realized (Epic 6, CAP-23)" addendum.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-9 — create workflow, lines 93-105] — the full CAP-23 rule: file/device branch resume logic, size-resolution-always-runs requirement, marker-then-keyslot cleanup ordering and its safety reasoning, `has_marker_token`'s exact contract.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#Progress reporting, line 171] — confirms the marker-token write is folded into the existing `FormattingLuks2` stage, not a new `CreateStage` variant.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#Capability-to-component map, line 299] — `CAP-23 (crash-safe create resume) | domain::workflows::create, LuksBackend | AD-2, AD-9`.
- [Source: src/domain/workflows/create.rs] — full current implementation of `run`/`bootstrap_and_provision`/`finish_provisioning`, read in full during story creation.
- [Source: src/ports/luks_backend.rs] — current `LuksBackend` trait, read in full during story creation.
- [Source: src/adapters/exec/mod.rs, lines 686-960] — `ExecAdapter`'s `LuksBackend` impl, including `bootstrap_format_and_open`, `has_luks2_header`, `write_fido2_token_metadata` (the closest existing precedent for token export/import mechanics), and the shared `dump_json_metadata`/`tokens_object`/`systemd_fido2_token_ids` helpers.
- [Source: src/domain/keyslot_guard.rs] — `remove_keyslot_guarded`, the existing AD-5 last-keyslot guard this story's Task 6 must run *after*, not instead of.
- [Source: tests/unit/fakes.rs] — `FakeLuksBackend`'s existing builder/field pattern for `has_luks2_header`, to mirror for `has_marker_token`.
- [Source: tests/unit/create.rs] — existing File/Device branch test coverage and naming convention to extend.
- [Source: _bmad-output/implementation-artifacts/sprint-status.yaml] — confirms this is the first story of Epic 6 (epic status flipped `backlog` → `in-progress` by this story's creation) and that Epic 5 is fully done.
- [Source: _bmad-output/implementation-artifacts/epic-4-retro-2026-07-28.md, epic-5-retro-2026-08-03.md] — open/in-progress retro action items on untested-parsing-functions and self-reported-completion-mismatch patterns, carried into this story's Dev Notes watchlist.

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

### File List
