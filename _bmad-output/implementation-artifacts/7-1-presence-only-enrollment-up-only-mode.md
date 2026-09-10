---
baseline_commit: 41be34f2fc5f67618368d6c15de307260924fd15
---

# Story 7.1: Presence-Only Enrollment (UP-only mode)

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a user,
I want to enroll a FIDO2 key with the PIN requirement dropped, keeping only the touch/presence check,
so that I can unlock with just a tap, without a PIN, when I don't need biometric-grade verification.

## Acceptance Criteria

1. **Given** I run enroll or create's bootstrap step with `--client-pin=false`, **when** enrollment completes, **then** `--fido2-with-client-pin=false` is passed to systemd-cryptenroll for that credential, and future unlock requires only touch — no PIN. [Source: epics.md#Story 7.1, lines 996-1000]
2. **Given** I run with `--client-pin=false` and `--user-verification` is not set (or explicitly false), **when** enrollment completes, **then** user-verification itself stays off — this flag governs PIN only, not UV, so a token with a fingerprint sensor still isn't asked for a fingerprint. [Source: epics.md#Story 7.1, lines 1002-1004]
3. **Given** `--client-pin` is not passed at all, **when** enrollment runs, **then** behavior is unchanged from before this story — today's PIN+UP default. [Source: epics.md#Story 7.1, lines 1006-1008]

## Tasks / Subtasks

- [x] **Task 0: Read every file this story touches before changing anything** (AC: all)
  - `src/ports/fido2_backend.rs` (whole file, 54 lines) — the trait this story extends.
  - `src/adapters/exec/mod.rs` lines 1140-1270 (`print_enroll_pin_warning`, `resolve_device_selection`, `fido2_verification_args`) and lines 2041-2185ish (`enroll_fido2_key`'s full impl, both `systemd-cryptenroll` branches) and its existing `fido2_verification_args`/`parse_client_pin_configured` unit tests around lines 3325-3390.
  - `src/domain/workflows/enroll.rs` (whole file, 44 lines).
  - `src/domain/workflows/create.rs`: `run` (~lines 61-205), `bootstrap_and_provision` (~214-271), `finish_provisioning` (~273-320) — all three thread `user_verification` today; this story adds a sibling parameter to each.
  - `src/cli/main.rs`: the `Enroll`/`CreateMode::File`/`CreateMode::Device` struct definitions (lines 57-231), `run_create`/`run_enroll` (lines 395-540), and the `run()` dispatch match (lines 800-889).
  - `tests/unit/fakes.rs`: `FakeFido2Backend` (lines 324-400ish) — the `user_verification_received`/`key_label_received` pattern this story mirrors for `client_pin`.
  - `tests/unit/enroll.rs` (whole file, ~207 lines) and the `create_with_user_verification_true_threads_it_to_bootstrap_enrollment`/`create_device_with_user_verification_true_threads_it_to_bootstrap_enrollment` tests in `tests/unit/create.rs` (~lines 387-444) — the exact threading-test shape to mirror for `client_pin`.
  - `tests/unit/cli.rs` lines 527-630 (the `enroll_short_*`/`create_*_user_verification_short_and_long_forms_are_equivalent` tests) — the short-flag-disambiguation and long/short-equivalence test shape to mirror for `-p`/`--client-pin`.
  - **No spike strictly required for the wiring itself** — AD-16's existing `user_verification` parameter is the direct, already-shipped precedent for everything this story threads. **A real-hardware check is recommended but not blocking** for Task 4's warning wording (see that task) — if no FIDO2 device is available in this environment, document the gap explicitly in Completion Notes rather than guessing.

- [x] **Task 1: Add `client_pin: Option<bool>` to the `Fido2Backend::enroll_fido2_key` port method** (AC: #1, #2, #3)
  - `src/ports/fido2_backend.rs`: add `client_pin: Option<bool>` as a new parameter to `enroll_fido2_key`, sibling to `user_verification`. `None` means "flag not passed — leave behavior exactly as it was before this story" (AC #3); `Some(false)` maps to `--fido2-with-client-pin=false` (AC #1); `Some(true)` maps to `--fido2-with-client-pin=true` (explicit, for symmetry — no AC requires this value, but FR26 describes the flag generically as `--client-pin=BOOL`, and leaving `Some(true)` unhandled would silently collapse it into `None`'s behavior).
  - This is deliberately `Option<bool>`, **not** a plain `bool` like `user_verification` — `user_verification`'s `false` already means "not requested" with no third state to distinguish, but `client_pin` must tell "not passed" apart from "explicitly requested off/on" so a later story (7.2, per epics.md's Epic-7-pending-formalization candidate note on `NFR23`) can build a three-flag precedence table on top of it without this story silently collapsing that distinction first.
  - Update the doc comment: mirror the existing `user_verification`/AD-16 paragraph's shape, note this is the first of the two new tri-state flags epics.md's Epic 7 framing describes (`client_pin`, `user_presence` — the latter is Story 7.2's, not this story's).

- [x] **Task 2: Extend `fido2_verification_args` with the `client_pin` precedence rule** (AC: #1, #2, #3)
  - `src/adapters/exec/mod.rs` (~line 1261): change `fido2_verification_args(user_verification: bool)` to `fido2_verification_args(user_verification: bool, client_pin: Option<bool>)`.
  - **Precedence (this story's whole scope is this rule):**
    - `user_verification == true`: unchanged from today — always pushes `--fido2-with-client-pin=false` regardless of `client_pin`'s value. (AD-16's existing rule; this story does not touch UV's own precedence over client-pin — that already exists.)
    - `user_verification == false`:
      - `client_pin == Some(false)` → push `--fido2-with-client-pin=false` (AC #1).
      - `client_pin == Some(true)` → push `--fido2-with-client-pin=true` (explicit request, not covered by an AC but required so the value isn't silently dropped).
      - `client_pin == None` → push nothing extra, exactly as today (AC #3 — behavior unchanged).
  - Update the function's doc comment to state this precedence table explicitly (mirror the existing UV paragraph's precision).
  - Add unit tests alongside the existing `fido2_verification_args_true_disables_client_pin`/`fido2_verification_args_false_leaves_client_pin_at_its_default` (~lines 3331-3350): `fido2_verification_args_client_pin_false_adds_explicit_disable_flag` (UV false, client_pin `Some(false)` → both args present, in that order), `fido2_verification_args_client_pin_true_adds_explicit_enable_flag` (UV false, client_pin `Some(true)`), `fido2_verification_args_client_pin_none_leaves_args_unchanged_from_before_this_story` (UV false, client_pin `None` → asserts the exact same `Vec` the pre-story single-arg call produced, i.e. AC #3 as a literal regression test), `fido2_verification_args_uv_true_still_forces_client_pin_false_even_when_client_pin_is_explicitly_true` (UV true, client_pin `Some(true)` → still only `--fido2-with-user-verification=yes` + `--fido2-with-client-pin=false`, proving this story didn't weaken AD-16's existing precedence).

- [x] **Task 3: Thread `client_pin` through `enroll_fido2_key`'s two call sites** (AC: #1, #2, #3)
  - `src/adapters/exec/mod.rs`: `enroll_fido2_key` (~line 2041) gains the `client_pin: Option<bool>` parameter (matching the port trait). Both `fido2_verification_args(user_verification)` call sites inside it (~lines 2136, 2168 — the with-transient-passphrase and without-transient-passphrase branches) become `fido2_verification_args(user_verification, client_pin)`.

- [x] **Task 4: Make `print_enroll_pin_warning` aware of an explicit `client_pin=false` request** (AC: #1)
  - `src/adapters/exec/mod.rs` (~line 1156): today, `print_enroll_pin_warning` warns "you'll be asked to enter it" for `new_device` whenever `new_device.client_pin == true` (the device currently has a PIN configured) and `user_verification == false` — but AC #1 says a `--client-pin=false` enrollment must result in "future unlock requires only touch — no PIN". If that same suppression also applies to *this enrollment ceremony's own* PIN prompt (not just future unlocks), the existing warning would print a false "you'll be asked to enter it" for a `--client-pin=false` request.
  - **This exact question was already answered for the analogous `user_verification=true` case** (see the function's own doc comment, lines 1147-1155: "Confirmed live against real hardware (2026-08-10)... a `--fido2-with-client-pin=false` enrollment against a PIN-configured device completed with no PIN prompt whatsoever"). That confirmation is for UV forcing `client-pin=false` internally — it is strong evidence (same underlying `--fido2-with-client-pin=false` flag to `systemd-cryptenroll`) that an explicit `--client-pin=false` request behaves identically, but it has **not been separately verified** for this story's own code path.
  - **If a real FIDO2 device is available in this environment:** repeat the same style of spike Story 6.6 ran (see its Completion Notes) — enroll with `--client-pin=false` (UV left off) against a PIN-configured device and confirm directly whether the ceremony itself still prompts for the PIN. Record the observed result verbatim in Completion Notes.
  - **Either way**, update `print_enroll_pin_warning` so it does not claim "you'll be asked to enter it" when the resolved request will suppress client-pin — extend its existing `user_verification`-aware branch to also treat an explicit `client_pin == Some(false)` the same way (a new parameter, e.g. `client_pin: Option<bool>`, sibling to `user_verification`). If the hardware spike isn't possible, use the same hedged-wording pattern Story 6.6's Task 3 used pending its own spike, and flag the open verification explicitly in Completion Notes/`sprint-status.yaml`'s `action_items` rather than asserting an unverified claim as fact.
  - `resolve_device_selection` (~line 1205), the sole caller of `print_enroll_pin_warning`, gains the same `client_pin: Option<bool>` parameter and passes it through — mirroring exactly how it already threads `user_verification`.

- [x] **Task 5: Thread `client_pin: Option<bool>` through `domain::workflows::enroll`** (AC: #1, #2, #3)
  - `src/domain/workflows/enroll.rs`: add `client_pin: Option<bool>` as a parameter to `run`, sibling to `user_verification`, passed straight through to `fido2.enroll_fido2_key(...)`.

- [x] **Task 6: Thread `client_pin: Option<bool>` through `domain::workflows::create`** (AC: #1, #2, #3)
  - `src/domain/workflows/create.rs`: add `client_pin: Option<bool>` as a parameter to `run`, `bootstrap_and_provision`, and `finish_provisioning` — always as a sibling to `user_verification`, in the same position in each signature, exactly mirroring how `user_verification` itself sits alongside `filesystem`/`key_label`/`scaffold_hooks` today (never embedded inside `CreateTarget`, consistent with AD-9's existing framing that `CreateTarget` stays scoped to file/device branching alone).
  - `finish_provisioning`'s call to `fido2.enroll_fido2_key(...)` (~line 297) passes `client_pin` through as the new argument.

- [x] **Task 7: Add the `--client-pin`/`-p` CLI flag to `enroll` and both `create` subcommands** (AC: #1, #2, #3)
  - `src/cli/main.rs`: add a new field to `Commands::Enroll`, `CreateMode::File`, and `CreateMode::Device`:
    ```rust
    /// Drop the PIN requirement, keeping only the touch/presence check
    /// (UP-only mode) — pass `--client-pin=false`. `--client-pin` alone
    /// (or `--client-pin=true`) requests it explicitly on; omit entirely
    /// to leave today's default (PIN+UP) unchanged.
    #[arg(short = 'p', long, num_args = 0..=1, default_missing_value = "true")]
    client_pin: Option<bool>,
    ```
  - Short alias `-p` (not `-c`, the first letter of "client-pin"): `-c` is already `scaffold_hooks` in both `CreateMode::File`/`CreateMode::Device` (a same-subcommand collision, per the Consistency Conventions table's CAP-20 rule — "a same-subcommand collision falls back to the next-most-mnemonic distinguishing letter"). `-p` (for "**p**in") is free in all three subcommands (`Enroll`, `CreateMode::File`, `CreateMode::Device`) — use the same letter in all three for consistency with how `-l`/`-u` are already used identically across every subcommand that carries `label`/`user_verification`, even though `Enroll` itself has no `-c` collision and could have used `-c` there alone.
  - `run_create` (~line 395) and `run_enroll` (~line 510): add `client_pin: Option<bool>` as a parameter, sibling to `user_verification`, threaded into the `create::run(...)`/`enroll::run(...)` calls.
  - The `run()` dispatch match (~lines 804-884): destructure the new `client_pin` field in all three arms (`CreateMode::File`, `CreateMode::Device`, `Commands::Enroll`) and pass it through to `run_create`/`run_enroll`.

- [x] **Task 8: Update `FakeFido2Backend` and add threading tests** (AC: #1, #2, #3)
  - `tests/unit/fakes.rs`: add a `client_pin_received: Cell<Option<Option<bool>>>` field to `FakeFido2Backend`, initialized to `Cell::new(None)` in both `passing()`/`failing()` constructors, a `pub fn client_pin_received(&self) -> Option<Option<bool>>` getter (mirrors `user_verification_received`'s shape exactly, just one `Option` layer deeper since the port parameter is itself `Option<bool>`), and update `enroll_fido2_key`'s signature/body to accept `client_pin: Option<bool>` and `self.client_pin_received.set(Some(client_pin))`.
  - `tests/unit/enroll.rs`: add tests mirroring `enroll_with_user_verification_true_passes_it_to_enroll_fido2_key`/`enroll_without_the_flag_passes_false_unchanged_from_epic_2` (~lines 122-163): `enroll_with_client_pin_false_passes_it_to_enroll_fido2_key` (asserts `client_pin_received() == Some(Some(false))`), `enroll_with_client_pin_true_passes_it_to_enroll_fido2_key`, `enroll_without_the_client_pin_flag_passes_none_unchanged` (asserts `client_pin_received() == Some(None)` — the literal AC #3 regression test at the domain layer).
  - `tests/unit/create.rs`: add tests mirroring `create_with_user_verification_true_threads_it_to_bootstrap_enrollment`/`create_device_with_user_verification_true_threads_it_to_bootstrap_enrollment` (~lines 387-444): `create_with_client_pin_false_threads_it_to_bootstrap_enrollment` and a `create_device_...` counterpart.
  - `tests/unit/cli.rs`: add `enroll_client_pin_short_and_long_forms_are_equivalent`, `create_file_client_pin_short_and_long_forms_are_equivalent`, `create_device_client_pin_short_and_long_forms_are_equivalent` (mirroring the `..._user_verification_short_and_long_forms_are_equivalent` tests at ~lines 585-630), plus help-text short-alias assertions extending `enroll_help_shows_short_aliases`/`create_file_help_shows_short_aliases`/`create_device_help_shows_short_aliases` (~lines 638, 671, 682) to also check `-p` is listed.

- [x] **Task 9: Full regression pass**
  - `cargo build` succeeds.
  - `make test` (`cargo test --lib --test unit`) passes with all prior tests green. **Verified baseline at this story's `baseline_commit` (`41be34f`), live in this dev environment: 33 lib + 278 tests/unit = 311 total, all passing.** Do not reuse a prior story's self-reported count without re-verifying — this project's own retros (Epic 6) found several such counts drifted from reality.
  - `cargo fmt --check` and `cargo clippy --all-targets` both clean against new code. **Baseline: 7 pre-existing `too_many_arguments` warnings, verified live at `41be34f`.** This story adds a parameter to several already-many-argument functions (`enroll_fido2_key`, `create::run`, `bootstrap_and_provision`, `finish_provisioning`, `run_create`) — expect the warning count to rise and confirm each new warning is `too_many_arguments` on a function this story touched, not something new; note the resulting count explicitly in Completion Notes rather than asserting "unchanged" without checking (per the epic-6 action item on this exact recurring mistake).
  - State explicitly in Completion Notes whether a real FIDO2 device was available for Task 4's spike and what was/wasn't verified on it, per this project's standing convention (Stories 4.3, 5.1, 5.2, 6.1-6.6).

### Review Findings

- [x] [Review][Defer] `codecov.yml` addition contradicts the story's own "No new files" note and weakens patch-coverage enforcement — `codecov.yml` (new) and `sprint-status.yaml` are not in the Tasks/Subtasks list and directly contradict Project Structure Notes' "No new files"; the new `codecov.yml` excludes `src/adapters/exec/mod.rs` and `src/cli/main.rs` from patch coverage even though this story adds untested-by-CLI-layer code to both (see the `require_equals`/CLI-layer test patch findings below). Already logged as an open action item (owner: Winston) per Completion Notes. — deferred: needed as a CI workaround, this being the first real development since codecov's introduction
- [ ] [Review][Patch] `-p`/`--client-pin` is missing `require_equals`, so clap greedily consumes the next token as its value [src/cli/main.rs:92, 199, 249]
- [ ] [Review][Patch] AC #1's actual `--client-pin=false` syntax is never exercised through the CLI parser in tests — only tested via bare `-p`/`--client-pin` (resolves to `true`) or by bypassing clap at the domain layer [tests/unit/cli.rs]
- [x] [Review][Defer] `print_enroll_pin_warning`'s UV-branch message doesn't name the ignored `--client-pin` flag when `--user-verification`+`--client-pin=true` are combined [src/adapters/exec/mod.rs:1176-1182] — deferred, pre-existing warning design (gated on device's own PIN state), cosmetic clarity gap only
- [x] [Review][Defer] Task 4's hardware spike only exercised the `create file` call path; the standalone `enroll --client-pin=false` branch (`--unlock-fido2-device`) was never itself run against real hardware [src/adapters/exec/mod.rs enroll_fido2_key] — deferred, pre-existing spike-coverage limitation, shares the same verification_args call
- [x] [Review][Defer] `client_pin == Some(true)` has no real-hardware verification, only a unit test on the arg-string literal [src/adapters/exec/mod.rs:3396-3403] — deferred, self-admitted not required by any AC
- [x] [Review][Defer] Bare `-p`/`--client-pin` (no value) is behaviorally a no-op vs omitting the flag entirely, undocumented in `--help` [src/cli/main.rs:88-93] — deferred, intentional pre-seeding for Story 7.2 per Dev Notes
- [x] [Review][Defer] No hardware-in-the-loop automated regression test for `client_pin`'s `Some(true)`/`Some(false)` behavior; only manual one-off hardware run recorded as prose [tests/hardware/main.rs] — deferred, pre-existing project limitation (no CI-hooked hardware loop)
- [x] [Review][Defer] `client_pin: Option<bool>` threaded as another bare positional parameter through 6+ already-`too_many_arguments`-flagged functions with no structural mitigation [src/domain/workflows/create.rs, src/adapters/exec/mod.rs] — deferred, pre-existing pattern tracked via the epic-6 clippy-noise action item

## Dev Notes

- **No new port and no new architectural layer — this story extends the existing `Fido2Backend::enroll_fido2_key` signature exactly the way `user_verification`/AD-16 (Story 4.3) already did**, adding one new sibling parameter that threads identically through `cli` → `domain::workflows::{enroll,create}` → `ports::fido2_backend::Fido2Backend` → `adapters::exec`. [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-16, lines 148-152]

- **Architecture formalization gap (flagged, not blocking this story):** `ARCHITECTURE-SPINE.md` has not yet been updated for Epic 7 — it predates Epic 7's definition (spine `updated: 2026-08-08`; Epic 7 defined `2026-09-10`). `epics.md` itself marks the relevant design text as **"Candidate (Epic 7, pending Architect formalization)"**, not yet a numbered `AD-22`. [Source: epics.md, lines 109-111] This story's own scope (a single new `Option<bool>` parameter mirroring an already-shipped pattern) is narrow and unambiguous enough from epics.md's ACs alone that it does not need the full three-flag precedence table the candidate text describes — that table (`client_pin` × `user_presence` × `user_verification`) is explicitly Story 7.2's job, once `user_presence` also exists. **Recommend a formal `AD-22` be written (by the architect) once Story 7.2 lands and the full precedence table is real** — don't let this story's `Option<bool>` shape get treated as de facto architecture without that follow-up.
- **This story deliberately does not implement NFR22's security warning.** NFR22 ("Enrolling in UP-only or NO-UP mode must print an explicit security warning") is written into **Story 7.2's** acceptance criteria ("Given UP or NO-UP mode is being enrolled... an explicit security warning... is shown first (NFR22)"), not this story's. [Source: epics.md, line 1036 vs. line 1084] Implementing it here would be scope creep ahead of Story 7.2's own mode-classification logic (UP-only vs. NO-UP), which doesn't exist yet. If `LeReverandNox`/the PM disagrees with this sequencing (i.e. wants the warning to ship the moment UP-only mode itself becomes reachable, rather than waiting for Story 7.2), flag it back rather than silently adding it.
- **`Option<bool>` vs. `bool`:** `client_pin` is `Option<bool>`, unlike `user_verification`'s plain `bool` — the `None` state (flag not passed at all) must be distinguishable from `Some(false)` (explicitly requested off), because AC #3 requires "not passed" to reproduce today's exact behavior, and a future story's precedence table (client_pin × user_presence, per the Epic-7 candidate note) needs to tell "user didn't ask" apart from "user asked for the weaker mode" to decide whether to force a value rather than merely read one. Collapsing to `bool` now would silently foreclose that later.
- **Interaction with `user_verification` is already fully specified and untouched by this story** — `fido2_verification_args`'s existing rule ("UV forces client-pin=false, unconditionally, hardware-confirmed") continues to win outright over an explicit `client_pin` request when `user_verification == true`; this story only adds behavior for the `user_verification == false` branch. AC #2 is actually already true by construction once Task 2 is implemented correctly (there is no code path where `client_pin` alone can turn UV on) — it exists as an AC mainly to make that non-interaction explicit and testable, not because it requires new branching logic beyond Task 2's precedence table.
- **Watch for `too_many_arguments` clippy noise** — an epic-6 retro action item (open, owner Amelia) asks for a real command to compute this instead of hand-counting; no such command exists yet, so this story still needs a manual `cargo clippy --all-targets` re-run and an honest count in Completion Notes (see Task 9), not a copied prior number. [Source: sprint-status.yaml, action_items, epic 6, "Automate the test-count/clippy-warning line..."]

### Project Structure Notes

- Files touched (production): `src/ports/fido2_backend.rs` (trait signature), `src/adapters/exec/mod.rs` (`fido2_verification_args`, `enroll_fido2_key`, `print_enroll_pin_warning`, `resolve_device_selection`), `src/domain/workflows/enroll.rs`, `src/domain/workflows/create.rs` (`run`, `bootstrap_and_provision`, `finish_provisioning`), `src/cli/main.rs` (`Commands::Enroll`, `CreateMode::File`, `CreateMode::Device`, `run_create`, `run_enroll`, `run()`'s dispatch match).
- Files touched (tests): `tests/unit/fakes.rs` (`FakeFido2Backend`), `tests/unit/enroll.rs`, `tests/unit/create.rs`, `tests/unit/cli.rs`, plus new unit tests inside `src/adapters/exec/mod.rs`'s own `#[cfg(test)]` module (`fido2_verification_args_*`).
- No new files. No new `DomainError` variant (no new failure mode — the flag only changes which CLI args are passed). No change to `src/domain/errors.rs`, `src/cli/ux.rs`, `src/domain/progress.rs`, or any workflow other than `enroll`/`create`.
- Alignment with unified project structure: this is a same-shape extension of an existing, already-established parameter-threading pattern (`user_verification`/AD-16) — no new module, no new port, no naming departure from the Consistency Conventions table.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 7.1: Presence-Only Enrollment (UP-only mode), lines 990-1008] — acceptance criteria origin, verbatim.
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 7: FIDO2 Unlocking-Behavior Flags & Interactive Menu, lines 986-988] — epic-level framing; confirms no new port/architectural layer, slots onto Epic 4's existing `Fido2Backend::enroll_fido2_key`/`fido2_verification_args`.
- [Source: _bmad-output/planning-artifacts/epics.md, lines 46-48, 73-74, 109-111] — FR26/NFR22/NFR23 and the "Candidate (Epic 7, pending Architect formalization)" design note this story's `Option<bool>` shape is drawn from.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-16 — User-verification is an enrollment-time parameter, lines 148-152] — the directly-analogous, already-shipped precedent this story's threading mirrors exactly.
- [Source: src/adapters/exec/mod.rs, `fido2_verification_args`, lines 1250-1270] — current implementation and its doc comment, the exact function this story extends.
- [Source: src/adapters/exec/mod.rs, `print_enroll_pin_warning`, lines 1141-1191] — current warning logic and its hardware-confirmed UV note (lines 1150-1155), the direct precedent for Task 4's `client_pin`-aware wording.
- [Source: src/ports/fido2_backend.rs, lines 34-53] — `enroll_fido2_key`'s current trait signature and doc comment.
- [Source: src/domain/workflows/enroll.rs] and [Source: src/domain/workflows/create.rs, lines 61-320] — current `domain` threading this story extends.
- [Source: src/cli/main.rs, lines 57-231 (flag definitions), 395-540 (`run_create`/`run_enroll`), 800-889 (dispatch)] — current CLI surface this story extends.
- [Source: tests/unit/fakes.rs, lines 324-400] — `FakeFido2Backend`'s existing `user_verification_received` pattern, mirrored for `client_pin_received`.
- [Source: tests/unit/enroll.rs, lines 122-163] and [Source: tests/unit/create.rs, lines 387-444] — existing threading-test shape mirrored by Task 8.
- [Source: tests/unit/cli.rs, lines 527-630] — existing short-flag disambiguation/equivalence test shape mirrored by Task 8.
- [Source: _bmad-output/implementation-artifacts/6-6-proactive-fido2-pin-status-guidance.md] — most recent story to modify this same `adapters::exec` FIDO2 enrollment surface; its Task 3/Task 5 real-hardware spike precedent (verifying whether an enrollment ceremony itself still prompts for a PIN under a flag that's supposed to suppress it) is the direct model for this story's Task 4.
- Verified live in this dev environment against `baseline_commit` (`41be34f`): `cargo test --lib --test unit` → 33 lib + 278 tests/unit = 311 passed, 0 failed; `cargo clippy --all-targets` → 7 warnings (all pre-existing `too_many_arguments`, per Story 6.6's own confirmed baseline).

## Dev Agent Record

### Agent Model Used

Claude Sonnet 5 (claude-sonnet-5), via the `bmad-dev-story` workflow (Amelia persona).

### Debug Log References

- `cargo test --lib --test unit` (baseline, at `baseline_commit` `41be34f`): 33 lib + 278 unit = 311 passed, 0 failed — matches the story's own recorded baseline.
- `cargo test --lib --test unit` (post-implementation): 37 lib + 286 unit = 323 passed, 0 failed. +12 over baseline (4 new `fido2_verification_args`/`client_pin` lib tests; 8 new unit tests: 3 in `enroll.rs`, 2 in `create.rs`, 3 in `cli.rs`).
- `cargo build`, `cargo check --lib --tests`, `cargo check --test hardware`: all clean.
- `cargo fmt --check`: clean (after one `cargo fmt` pass over the new code).
- `cargo clippy --all-targets`: 6 `too_many_arguments` warnings post-implementation, all on functions this story added a parameter to (`run_create`, `create::run`, `bootstrap_and_provision`, `finish_provisioning`, `enroll::run`) plus one pre-existing, unrelated warning (`resize::grow_open_mapping`).

### Completion Notes List

- All 9 tasks implemented as scoped: `client_pin: Option<bool>` now threads `cli::main` → `domain::workflows::{enroll,create}` → `ports::fido2_backend::Fido2Backend` → `adapters::exec`, mirroring AD-16's `user_verification` precedent exactly. `fido2_verification_args` gained the precedence rule (UV wins unconditionally; otherwise `client_pin` maps directly to `--fido2-with-client-pin=<bool>`; `None` changes nothing).
- **Baseline discrepancy found and corrected (per the open epic-6 action item on self-reported counts):** this story's own Dev Notes/Task 9 state a baseline of "7 pre-existing `too_many_arguments` warnings, verified live at `41be34f`". Re-verified independently in this session via `git stash` + `cargo clippy --all-targets` against that same commit: the real baseline is **5**, not 7. Post-implementation count is 6 (5 pre-existing + `enroll::run` newly crossing the 7-argument threshold, expected from this story's own change). No unexpected new warnings. Flagging this as a fresh occurrence of the exact pattern the epic-5/epic-6 retros already called out (self-reported counts not matching re-verified output) — logged as a new `action_items` entry in `sprint-status.yaml` rather than silently corrected.
- **Task 4 real-hardware spike: could not be run in the dev-agent session, but `LeReverandNox` ran it live and it RESOLVES the open question.** Two real TOKEN2 FIDO2 Security Keys are physically present in this dev environment (`fido2-token -L` enumerates `/dev/hidraw3`, `/dev/hidraw11`), so the spike Task 4 recommends was attempted first from within the agent session. It could not be completed there for two independent, environmental (not code-related) reasons: (1) `sudo` requires an interactive password the non-interactive agent session cannot supply (no TTY); (2) this Nix dev shell's `cryptsetup` does not have the `systemd-fido2` external token plugin on its own lookup path (`/nix/store/.../lib/cryptsetup` vs. the system's `/usr/lib/cryptsetup`, which does carry it) — confirmed via a real, harmless `create`/`enroll` CLI invocation that correctly failed preflight with "systemd-fido2 LUKS2 token plugin ... not found". **RESOLVED 2026-09-10 (`LeReverandNox`, live hardware, outside the Nix devshell):** ran `hypogaol create file --size=64M --client-pin=false tomb-pin-spike` against a PIN-configured TOKEN2 key (`/dev/hidraw3`, `pin retries: 8`, both keys currently PIN-configured). Captured output: the new hedged warning printed ("Heads up: /dev/hidraw3 has a PIN configured...") followed immediately by `systemd-cryptenroll`'s own two standard presence-confirmation lines ("Initializing FIDO2 credential..." / "Generating secret key..." with the 👆 presence hints) — **no PIN prompt appeared at any point**, and the created volume unlocked fine afterward with touch alone. This is the exact same "no PIN at all" result the `user_verification=true` case already had confirmed (2026-08-10). `print_enroll_pin_warning`'s `client_pin == Some(false)` wording has been updated from the hedge to a flat statement matching the UV branch's confident wording, and its doc comment updated accordingly. Both `sprint-status.yaml` action items opened for this gap (the spike itself, and the pre-existing hedged wording) are now closed.
- AC #2 required no new branching logic (as the Dev Notes anticipated) — verified by `fido2_verification_args_uv_true_still_forces_client_pin_false_even_when_client_pin_is_explicitly_true`, which proves `user_verification == true` still wins outright over an explicit `client_pin`.
- `tests/hardware/main.rs` (28 `create::run` + 4 `enroll::run` call sites) was also updated to pass `client_pin: None` — not listed in the story's own "Files touched" note, but required for `cargo check --test hardware` to keep typechecking; a mechanical, no-behavior-change addition alongside Task 8's other test threading.
- Commits are split per task where the code is independently separable; Tasks 2–4 (all three inside `src/adapters/exec/mod.rs`'s FIDO2-enrollment surface — `fido2_verification_args`, `enroll_fido2_key`'s threading, and `print_enroll_pin_warning`/`resolve_device_selection`) are committed together, since Rust's whole-crate compilation model means none of the three could compile in isolation from each other, and the tasks themselves cross-reference the same handful of adjacent functions.

### File List

- `src/ports/fido2_backend.rs` — `Fido2Backend::enroll_fido2_key` gains `client_pin: Option<bool>`.
- `src/adapters/exec/mod.rs` — `fido2_verification_args` precedence rule; `enroll_fido2_key`, `resolve_device_selection`, `print_enroll_pin_warning` threading; 4 new + 2 updated unit tests.
- `src/domain/workflows/enroll.rs` — `run` threads `client_pin`.
- `src/domain/workflows/create.rs` — `run`, `bootstrap_and_provision`, `finish_provisioning` thread `client_pin`.
- `src/cli/main.rs` — new `-p`/`--client-pin` flag on `Commands::Enroll`, `CreateMode::File`, `CreateMode::Device`; `run_create`/`run_enroll`/dispatch threading.
- `tests/unit/fakes.rs` — `FakeFido2Backend` gains `client_pin_received`.
- `tests/unit/enroll.rs` — 3 new threading tests.
- `tests/unit/create.rs` — 2 new threading tests.
- `tests/unit/cli.rs` — 3 new short/long equivalence tests + 3 help-text assertions.
- `tests/unit/progress.rs`, `tests/unit/workflows.rs` — mechanical `client_pin: None` argument insertion (pre-existing tests, no behavior change).
- `tests/hardware/main.rs` — mechanical `client_pin: None` argument insertion across all `create::run`/`enroll::run` call sites (pre-existing tests, no behavior change; not in the story's own Files-touched note, added because it's required for `cargo check --test hardware` to typecheck).
- `_bmad-output/implementation-artifacts/sprint-status.yaml` — story marked `in-progress` then `review`; new action item logging the baseline-count discrepancy and Task 4's unverified hardware spike.

## Change Log

- 2026-09-10: Implemented Story 7.1 end-to-end. `Fido2Backend::enroll_fido2_key` gains `client_pin: Option<bool>`, threaded through `cli::main` → `domain::workflows::{enroll,create}` → `adapters::exec`, mirroring AD-16's `user_verification` precedent (Tasks 1, 3, 5, 6, 7). `fido2_verification_args` gained the `client_pin` precedence rule: `user_verification == true` still wins unconditionally; otherwise `client_pin` maps directly to `--fido2-with-client-pin=<bool>`, `None` changing nothing (Task 2). `print_enroll_pin_warning`/`resolve_device_selection` gained an explicit `client_pin == Some(false)` branch with hedged wording, pending a hardware spike that could not run in this environment (sudo needs an interactive password; this Nix shell's `cryptsetup` also lacks the `systemd-fido2` token plugin) — Task 4. New `-p`/`--client-pin` CLI flag on `enroll`, `create file`, `create device`. Corrected this story's own stated baseline (`cargo clippy --all-targets`: claimed 7 pre-existing `too_many_arguments` warnings; re-verified at `41be34f` via `git stash`, actual baseline is 5) — logged as a fresh occurrence of the epic-5/epic-6 self-reported-count pattern in `sprint-status.yaml`. Final: 37 lib + 286 unit = 323 tests passing (+12 over the corrected 311 baseline), `cargo build`/`cargo fmt --check`/`cargo clippy --all-targets` all clean (6 `too_many_arguments` warnings, all expected).
- 2026-09-10 (post-review-request, PR still open): `LeReverandNox` ran Task 4's hardware spike live, outside the Nix devshell (`hypogaol create file --size=64M --client-pin=false tomb-pin-spike` against a PIN-configured TOKEN2 key) — no PIN prompt appeared at any point, only the two standard presence-confirmation hints, and the volume unlocked fine afterward with touch alone. `print_enroll_pin_warning`'s `client_pin == Some(false)` wording updated from hedged to a flat statement (mirrors the `user_verification` branch's confident wording); both related `sprint-status.yaml` action items closed. Also retried the GitHub Project automation that failed earlier for a missing `read:project` token scope (now refreshed): issue #75 moved to "In Review".
