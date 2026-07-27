---
baseline_commit: 7488f539147e1139e21cc1bcb7804eeb25053908
---

# Story 4.3: Enroll a FIDO2 Key with User-Verification

Status: ready-for-dev

## Story

As a user,
I want to enroll a FIDO2 key requiring user-verification (fingerprint/PIN),
so that unlocking with this key demands proof of physical identity beyond mere touch.

## Acceptance Criteria

1. **Given** an already-created tomb **When** I run enroll with the user-verification flag **Then** the new key is enrolled via `systemd-cryptenroll --fido2-with-user-verification=yes` **and** unlocking later with that key requires the device's own fingerprint/PIN check, not touch alone.
2. **Given** I run enroll without the flag **When** it completes **Then** the key continues to unlock with touch alone, unchanged from Epic 2 behavior.
3. **Given** I create a brand-new tomb with the user-verification flag set on its bootstrap enrollment **When** creation completes **Then** the first key enrolled is UV-required, same as a standalone enroll would produce.
4. **Given** a UV-enrolled key **When** unlock or resize runs **Then** no change is needed to the open call itself — cryptsetup's `systemd-fido2` token plugin reads the UV requirement from the stored credential automatically.

## Tasks / Subtasks

- [ ] Task 1: Add `user_verification: bool` to the `Fido2Backend::enroll_fido2_key` port (AC #1, #2, #3)
  - [ ] `src/ports/fido2_backend.rs:39-44` — add `user_verification: bool` as the 4th parameter to `enroll_fido2_key`, after `selection`. Update the doc comment to note it maps to `--fido2-with-user-verification=yes|no` (AD-16).
  - [ ] No change to `Fido2DeviceSelection` — UV is orthogonal to device selection, never embedded in it.

- [ ] Task 2: Wire the flag into `ExecAdapter`'s `systemd-cryptenroll` invocation (AC #1, #2)
  - [ ] `src/adapters/exec/mod.rs:1150-1158` — add `user_verification: bool` to `enroll_fido2_key`'s signature (matching Task 1's port change).
  - [ ] Add `.arg(format!("--fido2-with-user-verification={}", if user_verification { "yes" } else { "no" }))` to **both** `Command::new("systemd-cryptenroll")` branches (`mod.rs:1218-1222` — the passphrase/`--unlock-key-file` branch — and `mod.rs:1249-1253` — the `--unlock-fido2-device` branch). Always pass the flag explicitly (never omit it), matching AD-16's "passed to `systemd-cryptenroll` as `--fido2-with-user-verification=yes|no`" wording — do not rely on the tool's own default.
  - [ ] No change to token JSON metadata (`write_fido2_token_metadata`) — UV lives inside the FIDO2 credential itself, not a token field (architecture: "Token JSON metadata ... unchanged by Epic 4").

- [ ] Task 3: Update `FakeFido2Backend` to accept and record the new parameter (AC #1, #2, #3 — test support)
  - [ ] `tests/unit/fakes.rs:253-306` — add `user_verification_received: Cell<Option<bool>>` field (needs `use std::cell::Cell;` if not already imported — check the file's existing imports first). Initialize to `Cell::new(None)` in both `passing()` and `failing()`.
  - [ ] `enroll_fido2_key`'s signature gains `user_verification: bool`; store it via `self.user_verification_received.set(Some(user_verification));` before the existing log-push/fail-at logic.
  - [ ] Add `pub fn user_verification_received(&self) -> Option<bool>` getter, returning `self.user_verification_received.get()`.

- [ ] Task 4: Thread `user_verification` through `domain::workflows::enroll` (AC #1, #2)
  - [ ] `src/domain/workflows/enroll.rs:17-24` — add `user_verification: bool` to `run`'s signature, placed immediately after `selection` and before the three ports (mirrors this codebase's established "workflow-specific args first, ports last" convention — the same placement `create`/`resize`'s `progress` param uses immediately before `luks`, per Story 4.2's Dev Notes).
  - [ ] `enroll.rs:40` — pass `user_verification` through to `fido2.enroll_fido2_key(&mapper, metadata, selection, user_verification)`.

- [ ] Task 5: Thread `user_verification` through `domain::workflows::create` (AC #3)
  - [ ] `src/domain/workflows/create.rs:40-48` — add `user_verification: bool` to `run`'s signature, placed **immediately after `filesystem` and before `fido2_selection`** (AD-16: "a sibling to `CreateTarget`/`filesystem`" — grouping the two enrollment-behavior params, `user_verification` and `fido2_selection`, together while keeping `progress` as the last non-port argument immediately before `luks`, per the existing convention). Full new order: `target, filesystem, user_verification, fido2_selection, progress, luks, fido2, fs`.
  - [ ] Thread `user_verification` through both call sites of `bootstrap_and_provision` (`create.rs:72-81` and `create.rs:134-143`) and its own signature (`create.rs:148-157`), same parameter position.
  - [ ] Thread `user_verification` through to `finish_provisioning`'s signature (`create.rs:184-192`) and its call site (`create.rs:166-174`), same position.
  - [ ] `finish_provisioning` (`create.rs:205`) — pass it to `fido2.enroll_fido2_key(mapper, metadata, fido2_selection, user_verification)`.

- [ ] Task 6: CLI — add `--user-verification` flag and wire it through (AC #1, #2, #3)
  - [ ] `src/cli/main.rs`'s `Enroll` variant (`main.rs:50-69`) — add `#[arg(long)] user_verification: bool` (same bare-bool idiom as `Unlock`'s `read_only`, `main.rs:45-46`).
  - [ ] `CreateMode::File` and `CreateMode::Device` (`main.rs:113-156`) — add the identical `#[arg(long)] user_verification: bool` field to both variants.
  - [ ] `run_enroll` (`main.rs:370-387`) — add `user_verification: bool` parameter, pass through to `enroll::run(&path, label, fido2_selection, user_verification, &adapter, &adapter, &adapter)` (position matches Task 4's new signature).
  - [ ] `run_create` (`main.rs:277-304`) — add `user_verification: bool` parameter (positioned to match Task 5's new `create::run` signature), pass through in the `create::run(...)` call.
  - [ ] `run()`'s dispatch (`main.rs:532-586`): `Commands::Create { mode }`'s two arms destructure the new `user_verification` field and pass it to `run_create`; `Commands::Enroll { .. }` destructures it and passes it to `run_enroll`.

- [ ] Task 7: Update every existing `enroll::run`/`create::run` call site for the new parameter (mechanical, no behavior change)
  - [ ] `enroll::run` call sites — add `false` (the pre-existing, unchanged-behavior default) in the new position: `tests/unit/enroll.rs` (3 call sites: lines ~38-45, ~72-79, ~93-100) and `tests/hardware/main.rs` (2 call sites — grep the file for exact locations before starting, do not assume unit-test line numbers apply).
  - [ ] `create::run` call sites — add `false` in the new position: `tests/unit/create.rs` (13 call sites), `tests/unit/workflows.rs` (1 call site), `tests/unit/progress.rs` (3 call sites), `tests/hardware/main.rs` (18 call sites — grep for exact locations).
  - [ ] `cargo build --tests` must be green (hardware tests are `#[ignore]`d but still must compile) before starting Task 8's new tests — get this mechanical pass fully done first, same discipline Story 4.2's Dev Notes called out for its own signature change.

- [ ] Task 8: New tests proving UV threads correctly (AC #1, #2, #3)
  - [ ] `tests/unit/enroll.rs` — new test `enroll_with_user_verification_true_passes_it_to_enroll_fido2_key`: call `enroll::run(...)` with `user_verification: true` against a `FakeFido2Backend::passing()`, then assert `fido2.user_verification_received() == Some(true)`.
  - [ ] `tests/unit/enroll.rs` — new test `enroll_without_the_flag_passes_false_unchanged_from_epic_2`: call with `user_verification: false`, assert `fido2.user_verification_received() == Some(false)`.
  - [ ] `tests/unit/create.rs` — new test `create_with_user_verification_true_threads_it_to_bootstrap_enrollment`: a `CreateTarget::File` happy path with `user_verification: true`, assert `fido2.user_verification_received() == Some(true)` — proves AC #3 (create's bootstrap enrollment is the same call path as standalone enroll, not a divergent one).
  - [ ] No new tests needed for the `ExecAdapter`'s `--fido2-with-user-verification` arg construction beyond what's already covered by `tests/hardware/main.rs`'s existing enroll/create scenarios (AD-7: real-adapter behavior is hardware-gated, not unit-tested) — do not add a unit test that inspects `Command` args, since no existing test does that for any other `systemd-cryptenroll` flag either.

- [ ] Task 9: Marker-bleed check (AC: none directly — CAP-5/NFR3 quality bar, repeatedly flagged by the Epic 2/3 retros as the most-repeated bug class in this codebase)
  - [ ] Confirm by inspection that this story introduces no new `AdapterFailure` string and no new `DomainError` variant — `user_verification` is a plain `bool` threaded as a new parameter, entirely outside the `translate`/`translate_adapter_failure` marker-matching path in `cli/ux.rs`, so there is no bucket to collide with.

## Dev Notes

### Architecture requirement (binding, from ARCHITECTURE-SPINE.md AD-16)

- **AD-16 — User-verification is an enrollment-time parameter; unlock is unaffected:** `Fido2Backend::enroll_fido2_key` gains a `user_verification: bool` parameter, passed to `systemd-cryptenroll` as `--fido2-with-user-verification=yes|no`; `domain::workflows::enroll` and `create`'s bootstrap-enrollment step both thread it from the `cli` flag through to this one call. `create`'s signature gains the field as a sibling to `CreateTarget`/`filesystem` — never embedded inside `CreateTarget`. **Already resolved** (web-verified against `systemd-cryptenroll(1)`, 2026-07-27): the UV requirement is baked into the FIDO2 credential itself at enrollment time; unlock-time behavior is read automatically from that stored credential by cryptsetup's `systemd-fido2` token plugin. **`LuksBackend::open`/`resize` need no change and take no UV-related parameter** — do not touch `unlock.rs` or `resize.rs` for this story; AC #4 is satisfied by construction, with nothing to implement.
- CAP-13's entire architectural footprint is `domain::workflows::enroll`, `create`'s bootstrap step, and `Fido2Backend` (Capability → Architecture Map, `ARCHITECTURE-SPINE.md:257`) — no new port, no new `DomainError` variant, no new type. This is a narrower, more contained change than Story 4.2's (single new `bool` param vs. two new enums).

### Why this differs from Story 4.2's device-selection precedent

The prior enrollment-related story (2.1, `Fido2DeviceSelection`) resolved a genuinely hard problem (temporal-diff device identification racing/hanging). This story is not that: UV is a single boolean, fully resolved by web research already recorded in the architecture, with **zero unlock-side changes**. Do not over-engineer — no new enum, no new port method, no new CLI subcommand. The only novelty is one `bool` threaded through five call layers (`cli` flag → `enroll`/`create` workflow → `Fido2Backend::enroll_fido2_key` → `systemd-cryptenroll` arg) plus the mechanical call-site migration that any new parameter on `enroll::run`/`create::run` requires.

### Prior-story precedent to reuse, not reinvent

- **Param-ordering convention** (established across `unlock`'s `read_only`, `resize`'s `new_size`, `create`'s `target`/`filesystem`/`fido2_selection`, and Story 4.2's `progress`): workflow-specific args first, in a logical grouping, then the three ports last, always `luks, fido2, fs` order. This story's `user_verification` follows it: for `enroll::run`, immediately after `selection` (the other enrollment-specific arg) and before the ports; for `create::run`, immediately after `filesystem` and before `fido2_selection` — keeping `progress` as the last non-port argument immediately before `luks`, exactly as Story 4.2 established.
- **`Fido2DeviceSelection` is unaffected** — this story adds a sibling parameter, not a variant on that enum. Resist any urge to fold `user_verification` into `Fido2DeviceSelection::Explicit`/`Interactive`; AD-16 explicitly scopes it as its own parameter.
- **Rollback/close discipline is unaffected** — `create.rs`'s `bootstrap_and_provision`/`finish_provisioning` error-handling shape stays untouched; `user_verification` is a plain value parameter with no `Result`, no different from threading `filesystem` through the same call chain today.
- **`no_progress`/mechanical call-site migration precedent**: Story 4.2's Task 6 is the direct precedent for this story's Task 7 — get every existing call site compiling with the new parameter (using `false`, the behavior-preserving default) before writing any new test.

### Testing standard (AD-7)

Unit tests against the shared fakes in `tests/unit/fakes.rs`, run in default CI. `FakeFido2Backend` needs the one new field/getter from Task 3 — no other fake changes required (`FakeLuksBackend`/`FakeFilesystemBackend` are untouched by this story, since AD-16 confirms `LuksBackend::open`/`resize` need no change). Hardware-gated scenarios in `tests/hardware/main.rs` need no *new* scenarios for this story's happy path beyond the mechanical Task 7 signature migration — real-hardware verification of an actual UV-enrolled key requiring fingerprint/PIN at unlock is valuable but is a manual verification step for `LeReverandNox`, not a new automated `#[ignore]`d test (no existing story in this codebase adds a hardware test purely to eyeball a UX difference cryptsetup itself already guarantees per AD-16).

### Project Structure Notes

- Touches (all UPDATE — no new files, unlike Story 4.2's `progress.rs`):
  - `src/ports/fido2_backend.rs` — UPDATE, `enroll_fido2_key` gains `user_verification: bool`.
  - `src/adapters/exec/mod.rs` — UPDATE, `ExecAdapter`'s impl gains the param and the new `systemd-cryptenroll` arg in both branches.
  - `src/domain/workflows/enroll.rs` — UPDATE, `run` gains and threads `user_verification`.
  - `src/domain/workflows/create.rs` — UPDATE, `run`/`bootstrap_and_provision`/`finish_provisioning` gain and thread `user_verification`.
  - `src/cli/main.rs` — UPDATE, new `--user-verification` flag on `Enroll`, `CreateMode::File`, `CreateMode::Device`; `run_enroll`/`run_create`/dispatch wiring.
  - `tests/unit/fakes.rs` — UPDATE, `FakeFido2Backend` gains the recording field + getter.
  - `tests/unit/enroll.rs` — UPDATE (3 call sites) + 2 new tests.
  - `tests/unit/create.rs` — UPDATE (13 call sites) + 1 new test.
  - `tests/unit/workflows.rs` — UPDATE (1 call site).
  - `tests/unit/progress.rs` — UPDATE (3 call sites).
  - `tests/hardware/main.rs` — UPDATE (2 `enroll::run` + 18 `create::run` call sites — grep exact locations, do not assume unit-test counts/positions apply).
  - No changes to `src/domain/workflows/unlock.rs`, `src/domain/workflows/resize.rs`, `src/domain/errors.rs`, `src/domain/types.rs`, `src/cli/ux.rs` — CAP-13 adds no port, no `DomainError` variant, no new type, and no error-translation surface.
- **Scope warning:** like Story 4.2, this is a signature-breaking change to two existing, widely-called functions (`enroll::run`, `create::run`) plus the `Fido2Backend::enroll_fido2_key` port itself. Expect ~37 existing call sites total (6 `enroll::run` + 35 `create::run`, per current grep) needing the mechanical `false` addition — smaller in kind than 4.2's but touching more files (both workflows' test suites at once). Do the mechanical pass first, confirm `cargo build --tests` is green, then write Task 8's new tests.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 4.3: Enroll a FIDO2 Key with User-Verification]
- [Source: ARCHITECTURE-SPINE.md#AD-16 — User-verification is an enrollment-time parameter; unlock is unaffected]
- [Source: ARCHITECTURE-SPINE.md#Capability → Architecture Map — CAP-13 row]
- [Source: ARCHITECTURE-SPINE.md#Token JSON metadata note — "unchanged by Epic 4: UV enrollment (AD-16) lives inside the FIDO2 credential itself, not a new field here"]
- [Source: src/ports/fido2_backend.rs:29-45 — current `Fido2Backend::enroll_fido2_key`/`Fido2DeviceSelection` shapes]
- [Source: src/domain/workflows/enroll.rs:17-41 — current `enroll::run`, the function this story threads `user_verification` through]
- [Source: src/domain/workflows/create.rs:40-211 — current `create::run`/`bootstrap_and_provision`/`finish_provisioning`, the chain this story threads `user_verification` through]
- [Source: src/adapters/exec/mod.rs:1150-1294 — current `ExecAdapter::enroll_fido2_key`, both `systemd-cryptenroll` command-construction branches this story extends]
- [Source: src/cli/main.rs:50-69, 112-156, 277-304, 370-387, 532-586 — `Enroll`/`CreateMode` clap definitions, `run_enroll`/`run_create`, and dispatch]
- [Source: tests/unit/fakes.rs:253-306 — current `FakeFido2Backend`, extended by Task 3]
- [Source: _bmad-output/implementation-artifacts/4-2-real-progress-reporting-for-create-resize.md#Dev Notes — param-ordering convention and mechanical-migration-first discipline this story reuses]

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

### File List
