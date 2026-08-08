---
baseline_commit: 919c684d43a5917ffd4497a7360376fefc76aa0b
---

# Story 6.2: Custom Key Label at Create

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a user,
I want to supply a custom label for the first FIDO2 key enrolled during create's bootstrap step,
so that my newly created volume's key is labeled the same way I'd label any key I enroll later.

## Acceptance Criteria

1. **Given** I run create with `--label`, **when** the bootstrap key is enrolled, **then** the volume's `key_label` metadata equals the supplied value. [Source: epics.md#Story 6.2, lines 806-808]
2. **Given** I run create without `--label`, **when** the bootstrap key is enrolled, **then** the tool falls back to today's default label, unchanged. [Source: epics.md#Story 6.2, lines 810-812]
3. **Given** `--label` is supplied, **when** info or revoke later lists this volume's keys, **then** the custom label displays exactly as supplied, the same as any other enrolled key's label. [Source: epics.md#Story 6.2, lines 814-816]

## Tasks / Subtasks

- [x] **Task 0: Read every file this story touches before changing anything** (AC: #1, #2, #3)
  - Read in full: `src/domain/workflows/create.rs` (256 lines — `run`/`bootstrap_and_provision`/`finish_provisioning`), `src/ports/fido2_backend.rs` (`KeyMetadata`, `enroll_fido2_key`), `src/domain/workflows/enroll.rs` (existing precedent: how `enroll` already threads a caller-supplied `key_label: String` into `KeyMetadata`), `src/cli/main.rs` (the `Create`/`CreateMode` clap definitions, `parse_label`, `run_create`, `run()`'s dispatch match), `tests/unit/create.rs` (every existing `create::run(...)` call site — there are ~20), `tests/unit/fakes.rs` (`FakeFido2Backend`, its existing `user_verification_received` capture pattern at lines 323-388).
  - No spike needed — this story is a pure plumbing change (an `Option<String>` threaded through an existing call chain), not new adapter/subprocess mechanics.

- [x] **Task 1: Add `key_label: Option<String>` as a sibling parameter to `create::run`** (AC: #1, #2)
  - `src/domain/workflows/create.rs`: per `ARCHITECTURE-SPINE.md` AD-9, `key_label: Option<String>` is a sibling to `CreateTarget`/`filesystem`/`user_verification` in `run`'s signature — never embedded inside `CreateTarget` (which stays scoped to AD-9's own file/device branching alone). Insert it immediately after `user_verification: bool` and before `fido2_selection: Fido2DeviceSelection`, matching the order AD-9's Rule lists these siblings in (`filesystem`, `user_verification`, `key_label`).
  - Thread `key_label` unchanged through both call sites inside `run` (`bootstrap_and_provision` for the File branch, and the Device branch) down into `bootstrap_and_provision`'s own signature, then into `finish_provisioning`'s signature — mirroring exactly how `user_verification: bool` is already threaded through all three functions today.
  - In `finish_provisioning` (currently lines 219-255): replace the hardcoded `key_label: "primary".to_string()` (line 237) with `key_label: key_label.unwrap_or_else(|| "primary".to_string())` — `"primary"` is today's existing default label (already the literal used here and matched verbatim across `tests/unit/{info,revoke,keyslot_guard,fakes}.rs` and `tests/hardware/main.rs`); do not introduce a new constant or change the string itself, only make it a fallback (AC #2).

- [x] **Task 2: Add `--label` to both `create file` and `create device` CLI subcommands** (AC: #1, #2)
  - `src/cli/main.rs`: add `label: Option<String>` to both `CreateMode::File` and `CreateMode::Device` variants' fields, using `#[arg(long, value_parser = parse_label)]` — reuse the existing `parse_label` function (line ~201, already used by `enroll`/`revoke`'s required `--label`) unchanged; do not write a second validator. Since this field is `Option<String>`, clap runs `parse_label` only when the flag is actually supplied (omitting `--label` yields `None` directly, never calling the validator on an absent value) — no `default_value` needed.
  - Field placement: insert `label` after `filesystem` and before `fido2_device` in both variants' struct bodies, grouping it with the other bootstrap-enrollment-related flags (`fido2_device`, `user_verification`) rather than the volume-shape flags (`path`, `size`, `filesystem`).
  - Wire it through `run()`'s dispatch match (currently lines 762-819): destructure `label` out of both `CreateMode::File { .. }` and `CreateMode::Device { .. }` patterns and pass it to `run_create` as a new parameter.
  - `run_create` (currently lines 357-386): add a `key_label: Option<String>` parameter (placed next to `user_verification`, mirroring `create::run`'s own new parameter order from Task 1) and pass it straight through to `create::run`'s new parameter — `run_create` does no validation or transformation of its own, exactly like it already does for `user_verification`/`fido2_selection`.

- [x] **Task 3: Extend `FakeFido2Backend` to capture the received `key_label`** (AC: #1, #2)
  - `tests/unit/fakes.rs`: add a `key_label_received: RefCell<Option<String>>` field to `FakeFido2Backend` (a `String` needs `RefCell`, not `Cell`, unlike the existing `Cell<Option<bool>>` used for `user_verification_received` — `bool` is `Copy`, `String` is not), initialized to `RefCell::new(None)` in both `passing()`/`failing()` constructors, and a `pub fn key_label_received(&self) -> Option<String>` accessor that clones out of the `RefCell` (mirrors `user_verification_received`'s doc comment and purpose, lines 359-364).
  - In `enroll_fido2_key`'s impl (lines 372-387), before returning, set `*self.key_label_received.borrow_mut() = Some(metadata.key_label.clone());` — capture it from the real `metadata` parameter (currently named `_metadata` and ignored; rename to `metadata` since it's now read).

- [x] **Task 4: Unit tests for the domain-level threading** (AC: #1, #2)
  - Extend `tests/unit/create.rs`: add `create_with_label_threads_it_into_the_enrolled_key_metadata` (File-backed, `Some("backup".to_string())` passed to `create::run`, asserts `fido2.key_label_received() == Some("backup".to_string())`) and `create_without_label_falls_back_to_the_default_label` (File-backed, `None` passed, asserts `fido2.key_label_received() == Some("primary".to_string())`) — mirror `create_with_user_verification_true_threads_it_to_bootstrap_enrollment`'s exact shape (lines 213-238).
  - Add the same pair for the Device-backed branch, mirroring `create_device_with_user_verification_true_threads_it_to_bootstrap_enrollment` (lines 240-266).
  - Update **every** existing `create::run(...)` call site in this file (~20 occurrences) to pass the new `key_label` argument in its new position (use `None` for every test that doesn't care about labeling, matching how these same tests already pass `false`/`Fido2DeviceSelection::Interactive` for parameters they don't care about) — a compile error from a missed call site is expected and exhaustive; fix every one, don't silence with a default.

- [x] **Task 5: CLI-level tests for the new flag** (AC: #1, #2)
  - Extend `tests/unit/cli.rs`: add `create_file_help_lists_label_as_a_flag` and `create_device_help_lists_label_as_a_flag`, mirroring `enroll_help_lists_path_as_positional_and_label_as_a_flag`/`revoke_help_lists_path_as_positional_and_label_as_a_flag` (lines 168-182) — assert `help.contains("--label")` for `["hypogaol", "create", "file", "--help"]` and `["hypogaol", "create", "device", "--help"]`.
  - If `run_create`'s signature changed in a way that breaks any existing `tests/unit/cli.rs` coverage of `run_create`/`device_create_confirmation` (Story 6.1 added `device_create_confirmation` tests there), update call sites the same way Task 4 does for `tests/unit/create.rs` — check before assuming none are affected.

- [x] **Task 6: Confirm AC #3 needs no production code change, only a regression note** (AC: #3)
  - `domain::workflows::info` and `domain::workflows::revoke` both already display `key_label` read straight from the LUKS2 token's stored metadata (`AD-15`, `src/domain/workflows/info.rs`; `KeyslotInfo.key_label` printed verbatim by `run_info` in `cli/main.rs`) — neither workflow's code path changes because of this story. AC #3 is automatically satisfied once Task 1's `finish_provisioning` writes whatever `key_label` was resolved (custom or default) into the same `KeyMetadata` that `enroll_fido2_key` already persists onto the token (AD-2) — there is no separate write path for create's bootstrap key vs. a later `enroll`'s key.
  - No new test is required beyond Task 4's assertion that the correct `key_label` reaches `enroll_fido2_key`'s `metadata` argument — `info`/`revoke`'s existing unit tests (`tests/unit/info.rs`, `tests/unit/revoke.rs`) already prove those workflows print whatever `key_label` a `KeyslotInfo`/token carries, using the fixed literal `"primary"`; they do not need a custom-label variant added, since the display logic itself is untouched by this story.

- [x] **Task 7: Full regression pass**
  - `cargo build` succeeds and `make test` passes with all prior tests (baseline 203 total: 17 lib + 186 `tests/unit`, per Story 6.1's Completion Notes) plus this story's new ones green. Verify the exact new total by running the test suite, not from memory (per the Epic 5 retro watchlist item — see Dev Notes).
  - `cargo fmt --check` and `cargo clippy --all-targets` both clean (matching Story 6.1's baseline of 4 pre-existing `too_many_arguments` warnings — this story adds one more parameter to `create::run`/`bootstrap_and_provision`/`finish_provisioning`/`run_create`, all of which may already be at or near that clippy threshold; if a **new** `too_many_arguments` warning appears on a function that didn't have one before, note it explicitly in Completion Notes rather than silently suppressing it).
  - If hardware is available: run `create file --label mykey` and `create device --label mykey` end-to-end, then `info`/`revoke` against the resulting volume to visually confirm `mykey` displays exactly as supplied (AC #3's one true end-to-end proof — unit tests only prove the value reaches `enroll_fido2_key`'s argument, not that a real `systemd-cryptenroll --fido2-with-user-verification`-token round-trip preserves an arbitrary label string byte-for-byte). If no hardware is available in this session, state that explicitly (per this project's standing convention) and flag it as a retrospective action item for `LeReverandNox`, same pattern as Story 6.1's hardware-verification item.

### Review Findings

- [x] [Review][Patch] `--label` help text never states the actual default value [src/cli/main.rs:161-165, 195-199] — fixed: both help strings now read `Defaults to "primary" when omitted.`
- [x] [Review][Patch] AC #3's automated coverage only ever used trivial single-word ASCII labels (`"backup"`/`"mykey"`), leaving no proof that the domain-level plumbing passes an unusual label through unmodified [tests/unit/create.rs] — fixed: added `create_with_an_unusual_label_threads_it_through_unmodified`, asserting a label with spaces/punctuation/non-ASCII characters reaches `enroll_fido2_key`'s `metadata` byte-for-byte. Correction to the original finding text: `tests/unit/info.rs`/`tests/unit/revoke.rs` already exercised a non-`"primary"` label (`"backup"`) prior to this review, so the claim that those files only used `"primary"` was inaccurate — no change was needed there. The full create → LUKS2 token → `info`/`revoke` round-trip (the on-disk encode/decode) remains provable only on real hardware, since the fakes used by unit tests don't model token persistence — this matches Task 6's original reasoning and is not something a fake-backed test can close.
- [x] [Review][Patch] No negative-path test for an empty/whitespace `--label` on `create`'s two new call sites — `parse_label`'s rejection branch was only ever exercised via `enroll`/`revoke` [tests/unit/cli.rs] — fixed: added `create_file_rejects_an_empty_label` and `create_device_rejects_a_whitespace_only_label`, mirroring the existing `Cli::try_parse_from(...).is_err()` pattern used by the `enroll` device-flag-pairing tests.
- [x] [Review][Defer] `parse_label` accepts values that are unsafe as identifiers — untrimmed leading/trailing whitespace, invisible/zero-width-only strings pass the not-empty check, no length cap, and control characters (e.g. embedded newline) are not rejected [src/cli/main.rs:213-219] — deferred, pre-existing (shared by `enroll`/`revoke`, unchanged by this diff, explicitly out of scope per this story's own Dev Notes: "reuse `parse_label` unchanged — do not write a second validator"). Consequences if exploited: whitespace padding can defeat the exact-match duplicate-label check (`src/adapters/exec/mod.rs:1413-1420`); an embedded newline can inject a fake extra line into `info`'s one-bullet-per-line output (`src/cli/main.rs:762`); an oversized label fails late inside `cryptsetup token import` instead of at CLI parse time.

## Dev Notes

- **No new port, no new architectural layer, no new adapter code.** This story is pure `domain`/`cli` plumbing: an `Option<String>` threaded from a new CLI flag down to the one place (`finish_provisioning`) that already constructs the `KeyMetadata` passed to `Fido2Backend::enroll_fido2_key`. `adapters::exec`'s `enroll_fido2_key` implementation already writes whatever `key_label` it's given onto the `systemd-fido2` token (AD-2) — it needs no changes, since `enroll`'s existing caller-supplied-label path already exercises that exact code. [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md:38, :294]
- **`key_label: Option<String>` is a sibling to `CreateTarget`, never embedded inside it** — this is AD-9's explicit Rule, stated for exactly this capability: "`key_label: Option<String>` (CAP-18 — falls back to today's default label when `None`)... are all siblings to `CreateTarget` in the signature, never embedded inside it — `CreateTarget` stays scoped to AD-9's own file/device branching alone." Do not add a `label` field to `CreateTarget::File`/`CreateTarget::Device`. [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md:95]
- **The default label is `"primary"` — an existing literal, not a new constant to invent.** It's already hardcoded at `src/domain/workflows/create.rs:237` and matched verbatim in five existing test files (`tests/unit/info.rs`, `tests/unit/revoke.rs`, `tests/unit/keyslot_guard.rs`, `tests/unit/fakes.rs`, `tests/hardware/main.rs`). This story only makes it a fallback (`.unwrap_or_else(|| "primary".to_string())`), never changes the string itself.
- **`enroll`'s existing `key_label: String` (required, not `Option`) is the closest precedent, not a pattern to copy verbatim.** `src/domain/workflows/enroll.rs:18-38` already shows the shape of threading a caller label into `KeyMetadata` — but `enroll`'s label is mandatory (a user adding a key later always names it), while `create`'s bootstrap label is optional with a sensible default (a user creating a volume shouldn't be forced to think of a label immediately) — hence `Option<String>` here, `String` there. Don't make `create`'s `--label` required.
- **Reuse `parse_label` unchanged — do not write a second validator.** `src/cli/main.rs:201` (`pub fn parse_label`) already rejects an empty/whitespace-only label for `enroll`/`revoke`'s required `--label`; clap only invokes a field's `value_parser` when that flag is actually present on the command line, so applying the same `value_parser = parse_label` to an `Option<String>` field correctly validates a supplied label while leaving an omitted one as `None` with no validator call at all.
- **`FakeFido2Backend`'s existing `user_verification_received`/`Cell<Option<bool>>` pattern (`tests/unit/fakes.rs:323-364`) is the direct precedent for this story's `key_label_received`/`RefCell<Option<String>>` addition** — same shape, different interior-mutability primitive because `String` isn't `Copy`. This is the only test-infrastructure gap: today `FakeFido2Backend::enroll_fido2_key` receives `metadata: KeyMetadata` but discards it as `_metadata`, so no existing test can observe what label a workflow actually passed through — Task 3 closes that gap the same way `user_verification_received` was added for Story 4.3's UV threading.
- **Recurring review-pattern watchlist from Epic 4/5/6 retros — apply proactively:**
  - Self-reported completion-note claims not matching actual grep/build/test output hit in all 4 Epic 5 stories and remains an open Amelia-owned action item — verify every test-count claim in Task 7's Completion Notes against real command output, not memory.
  - New pure parsing/logic functions shipping without a direct unit test hit 3 times in Epic 4 — not directly triggered by this story (no new parsing function is introduced; `parse_label` already has coverage), but if any small helper is factored out during Task 1/2, give it its own test rather than only indirect coverage.
- **Hardware verification convention:** this codebase's standing pattern (Stories 4.3, 5.1, 5.2, 6.1) is to explicitly state in Completion Notes whether real hardware was available and what was/wasn't verified on it, rather than silently claiming full verification from fakes/mocks alone. Follow that here for Task 7's end-to-end label round-trip check.

### Project Structure Notes

- Files touched (production): `src/domain/workflows/create.rs` (`run`/`bootstrap_and_provision`/`finish_provisioning` gain the `key_label: Option<String>` parameter; `finish_provisioning`'s `KeyMetadata` construction changes from a hardcoded literal to a fallback), `src/cli/main.rs` (`CreateMode::File`/`CreateMode::Device` gain a `label` field; `run_create` gains a parameter; `run()`'s dispatch match threads it through).
- Files touched (tests): `tests/unit/fakes.rs` (`FakeFido2Backend` gains `key_label_received`), `tests/unit/create.rs` (every existing `create::run` call site gains the new argument; new label-threading tests), `tests/unit/cli.rs` (new `--label`-in-help tests for both `create` subcommands).
- No new files, no new modules, no new port methods, no `CreateTarget`/`Filesystem`/`KeyMetadata` type changes — `KeyMetadata.key_label` already exists as a plain `String` field and needs no schema change; only what value gets put into it at construction time changes.
- Alignment with the documented source tree: `ARCHITECTURE-SPINE.md`'s `cli/main.rs` inventory (line 244) already lists `--label (CAP-18, create)` as belonging there — this story implements what the spine already scoped, not introducing a new location.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 6.2: Custom Key Label at Create, lines 798-816] — acceptance criteria origin, verbatim.
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 6: Volume Resilience, Filesystem Choice & Everyday Polish, lines 766-768] — epic-level framing; confirms no new port/layer for any Epic 6 story.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-9 — create workflow, lines 93-105] — `key_label: Option<String>` as a `CreateTarget`-sibling parameter, falls back to today's default label when `None`.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-2 — No side-channel state, line 52] — `key_label` is a generic, already-existing token metadata field (AD-2/AD-13), no new field needed.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-15 — Info reuses the existing keyslot-listing query, line 146] — confirms `info`'s display of `key_label` is unaffected by this story (AC #3 needs no code change there).
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#Capability-to-component map, line 294] — `CAP-18 (custom bootstrap label) | domain::workflows::create | AD-9`.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#Structural Seed, line 244] — `cli/main.rs`'s documented flag inventory already lists `--label (CAP-18, create)`.
- [Source: src/domain/workflows/create.rs] — full current implementation of `run`/`bootstrap_and_provision`/`finish_provisioning`, read in full during story creation; line 237 is the exact hardcoded `"primary"` literal this story makes a fallback.
- [Source: src/domain/workflows/enroll.rs] — existing precedent for threading a caller-supplied `key_label` into `KeyMetadata`, read in full during story creation.
- [Source: src/ports/fido2_backend.rs] — `KeyMetadata`/`enroll_fido2_key` signature, confirms no port change needed.
- [Source: src/cli/main.rs] — current `Create`/`CreateMode` clap definitions, `parse_label` (line 201), `run_create` (lines 357-386), and `run()`'s dispatch match, all read in full during story creation.
- [Source: tests/unit/fakes.rs, lines 323-388] — `FakeFido2Backend`'s existing `user_verification_received`/`Cell<Option<bool>>` pattern, the direct precedent for this story's `key_label_received` addition.
- [Source: tests/unit/create.rs] — existing test structure/naming convention and every current `create::run` call site needing an updated argument list.
- [Source: tests/unit/cli.rs, lines 154-189] — existing `--help`-content assertion pattern (`create_file_help_lists_path_as_positional`, `enroll_help_lists_path_as_positional_and_label_as_a_flag`, etc.) to extend for `create`'s new `--label` flag.
- [Source: _bmad-output/implementation-artifacts/6-1-crash-safe-create-resume.md] — previous story in this epic; confirms baseline test count (203 total: 17 lib + 186 `tests/unit`), the hardware-verification-statement convention, and the Epic 4/5 retro watchlist items carried forward into Epic 6.
- [Source: _bmad-output/implementation-artifacts/sprint-status.yaml] — confirms this is the second story of Epic 6 (epic already `in-progress` since Story 6.1).

## Dev Agent Record

### Agent Model Used

Claude Sonnet 5 (claude-sonnet-5)

### Debug Log References

### Completion Notes List

- Task 0: Read `create.rs`, `fido2_backend.rs`, `enroll.rs`, `cli/main.rs`, `tests/unit/create.rs`, `tests/unit/fakes.rs` in full before any change, per the story's own instruction.
- Task 1: `key_label: Option<String>` added as a sibling parameter to `create::run`/`bootstrap_and_provision`/`finish_provisioning`, inserted after `user_verification: bool` and before `fido2_selection`. `finish_provisioning`'s `KeyMetadata` construction now uses `key_label.unwrap_or_else(|| "primary".to_string())` instead of the hardcoded literal.
- Task 2: `--label` (`Option<String>`, `value_parser = parse_label`) added to both `CreateMode::File` and `CreateMode::Device`, placed after `filesystem` and before `fido2_device`. Wired through `run()`'s dispatch match and `run_create`'s new `key_label` parameter straight into `create::run`. `cargo build` confirmed green after Tasks 1+2 landed together (the lib crate only compiles once both the domain signature and its CLI-side caller agree — `tests/unit/create.rs`'s ~20 call sites are a separate compilation target and are updated in Task 4).

- Task 3: `FakeFido2Backend` gained a `key_label_received: RefCell<Option<String>>` field/accessor, mirroring `user_verification_received`'s `Cell` pattern but using `RefCell` since `String` isn't `Copy`. `enroll_fido2_key`'s `metadata` parameter is now read (renamed from `_metadata`).
- Task 4: Added `create_with_label_threads_it_into_the_enrolled_key_metadata`, `create_without_label_falls_back_to_the_default_label`, and the Device-backed equivalents to `tests/unit/create.rs`. Updated every existing `create::run(...)` call site to pass the new `key_label` argument (`None` where the test doesn't care) — this included **three files the story didn't originally enumerate**: `tests/unit/workflows.rs` (1 site) and `tests/unit/progress.rs` (3 sites, both submodules of the `unit` test binary), plus `tests/hardware/main.rs` (22 sites, its own gated binary) — all fixed the same way (compile-error-driven, as the story anticipated for the ~20 sites it did call out). `cargo build --tests` is green across every test binary.
- `cargo test --lib --bins` (17 passed) and `cargo test --test unit` (190 passed = 186 baseline + 4 new) both green.
- Task 5: Added `create_file_help_lists_label_as_a_flag`/`create_device_help_lists_label_as_a_flag` to `tests/unit/cli.rs`, mirroring `enroll_help_lists_path_as_positional_and_label_as_a_flag`. `run_create`/`device_create_confirmation` have no direct `tests/unit/cli.rs` coverage (`run_create` is a private, un-unit-tested fn; `device_create_confirmation`'s own signature is unchanged by this story), so no call sites needed updating there. `cargo test --test unit` now at 192 passed (190 + 2 new).
- Task 6: Confirmed by re-reading `src/domain/workflows/info.rs` (`run` returns `Vec<KeyslotInfo>` straight from `luks.list_fido2_keyslots`, unmodified by this story) and `src/domain/workflows/revoke.rs` (matches on `info.key_label` read from the same source) — neither workflow's code changes because of this story. No production code change; AC #3 is satisfied automatically since Task 1's `finish_provisioning` writes the resolved `key_label` into the same `KeyMetadata` `enroll_fido2_key` already persists onto the token for every other enrolled key.
- Task 7: `cargo build` clean. `make test` (`cargo test --lib --test unit`) green: 17 lib + 192 unit = **209 total** (baseline 203 + 6 new: 4 domain label-threading tests + 2 CLI help-text tests), verified by real command output, not memory. `cargo fmt --check` clean. `cargo clippy --all-targets` shows exactly the same 4 pre-existing `too_many_arguments` warnings as Story 6.1's baseline (`create::run` 8→9, `bootstrap_and_provision` 9→10, `finish_provisioning` 8→9 args — all three were already over clippy's 7-arg default threshold before this story added `key_label`, so none is a *new* warning site; `resize.rs:161` is an unrelated pre-existing warning; `run_create` reaches 7 args, still at the threshold, not over it — no warning there).
  - **Hardware verification:** a real FIDO2 hardware token was detected on this machine (`fido2-token -L` → TOKEN2 FIDO2 Security Key on `/dev/hidraw5`), but per this project's standing convention (Stories 4.3, 5.1, 5.2, 6.1), stated explicitly rather than silently claimed: the end-to-end `create file --label mykey`/`create device --label mykey` round-trip (AC #3's one true end-to-end proof) requires interactively touching the physical key mid-enrollment, which this non-interactive dev-agent session cannot provide. **No real-hardware end-to-end label round-trip was performed this session.** Flagged as a retrospective action item for `LeReverandNox` to verify manually.
  - **Manual hardware verification, performed by `LeReverandNox` post-review (2026-08-09):** ran `create` with `--label`, then `info` and `revoke` against the resulting volume — all three displayed the custom label exactly as supplied, and `revoke` successfully removed the keyslot by that label. AC #3's end-to-end proof is now confirmed on real hardware; the action item above is resolved.

### File List

- `src/domain/workflows/create.rs` (modified)
- `src/cli/main.rs` (modified)
- `tests/unit/fakes.rs` (modified)
- `tests/unit/create.rs` (modified)
- `tests/unit/workflows.rs` (modified)
- `tests/unit/progress.rs` (modified)
- `tests/hardware/main.rs` (modified)
- `tests/unit/cli.rs` (modified)
