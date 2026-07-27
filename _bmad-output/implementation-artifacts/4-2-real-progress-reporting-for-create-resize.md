---
baseline_commit: 5afa585bfac5a55a72a2981ab8e3d31c675e67cd
---

# Story 4.2: Real Progress Reporting for Create & Resize

Status: done

## Story

As a user,
I want to see a distinct message for each real stage of create and resize as it happens,
so that I have visibility into a long-running operation instead of one message before and after.

## Acceptance Criteria

1. **Given** I run create **When** it executes **Then** I see a distinct message as each real stage begins, in order: allocating the backing file, formatting as LUKS2, creating the filesystem, enrolling the FIDO2 key **and** no stage's message carries data beyond naming which stage is running.
   > **Order correction (see Dev Notes "Real execution order overrides this AC's prose order"):** the actual, security-mandated code order is allocate → format → **enroll the FIDO2 key → create the filesystem** — enrolling fires *before* creating the filesystem, not after. Implement the callback at the real call sites in that real order. Do not reorder `create.rs`'s existing `enroll_fido2_key`/`mkfs` calls to match this AC's prose; that would break AD-3/AD-9's wipe-before-mkfs security invariant.
2. **Given** I run resize **When** it executes **Then** I see a distinct message for each real stage in order: growing the backing file, resizing the LUKS2 mapping, growing the filesystem.
3. **Given** resize on a raw device/partition target **When** it executes **Then** only the two applicable stages fire (resizing the LUKS2 mapping, growing the filesystem) — no "growing the backing file" stage, since that only applies to file-backed tombs.
4. **Given** progress reporting **When** it's implemented **Then** `domain` performs no direct I/O for these messages — it invokes a callback, and `cli` is what actually translates and prints text.

**Implied by construction (not a separate epics.md AC, but required for internal consistency with AC #1/#3):** a device-backed `create` never fires `AllocatingBackingFile` either — `CreateTarget::Device`'s branch never calls `fs.set_backing_file_size`, the same reason AC #3 excludes resize's backing-file stage for device targets. Cover this with a unit test (Task 4) the same way AC #3's device-resize case is covered.

## Tasks / Subtasks

- [x] Task 1: Add `domain::progress` — the `Stage` enums (AC #1, #2, #3, #4)
  - [x] Create `src/domain/progress.rs`. Two enums, each `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`, every variant **bare and payload-free** — this is a pinned architectural rule (AD-19's Rule, sharpened by the architecture's own adversarial review Finding 5: "no variant may carry any data at all; the enum discriminant alone is the entire message"), not a style preference. A payload-carrying variant is exactly the shape that could smuggle AD-3-guarded secret-adjacent state (e.g. the transient bootstrap passphrase, still live during `FormattingLuks2`/`EnrollingFido2Key`) past the wipe boundary.
    ```rust
    pub enum CreateStage {
        AllocatingBackingFile,
        FormattingLuks2,
        EnrollingFido2Key,
        CreatingFilesystem,
    }
    pub enum ResizeStage {
        GrowingBackingFile,
        ResizingLuks2Mapping,
        GrowingFilesystem,
    }
    ```
    Note `CreateStage`'s variant order here is the *real execution order* (`EnrollingFido2Key` before `CreatingFilesystem`), not epics.md's prose order — see AC #1's correction above and this story's Dev Notes.
  - [x] Register in `src/domain/mod.rs:1-6`: add `pub mod progress;`, alphabetically after `preflight`.

- [x] Task 2: Thread progress through `domain::workflows::create` (AC #1, per-target correctness)
  - [x] `src/domain/workflows/create.rs` — add `progress: &dyn Fn(CreateStage)` to `run`'s signature (`create.rs:33-40`), placed after `fido2_selection` and before the three port params, matching the established param-ordering convention (workflow-specific args first, ports always last — e.g. `unlock::run`'s `read_only: bool` placement, `src/domain/workflows/unlock.rs:13-19`).
  - [x] Thread `progress` through to `bootstrap_and_provision` (`create.rs:130-154`) and `finish_provisioning` (`create.rs:156-180`) — both need the new param added and passed through.
  - [x] In the `CreateTarget::File` arm only (`create.rs:44-69`), call `progress(CreateStage::AllocatingBackingFile);` immediately before `fs.set_backing_file_size(&path, size)?;` (`create.rs:57`). Do **not** call it in the `CreateTarget::Device` arm (`create.rs:70-127`) — that branch never allocates a backing file, so there is no real stage boundary to name (mirrors AC #3's device-resize exclusion).
  - [x] In `bootstrap_and_provision`, call `progress(CreateStage::FormattingLuks2);` immediately before `luks.bootstrap_format_and_open(...)` (`create.rs:140`).
  - [x] In `finish_provisioning`, call `progress(CreateStage::EnrollingFido2Key);` immediately before `fido2.enroll_fido2_key(...)` (`create.rs:175`), then call `progress(CreateStage::CreatingFilesystem);` immediately before `fs.mkfs(...)` (`create.rs:177`) — this is the real order; do not swap it to match epics.md's prose (see AC #1's correction and Dev Notes).

- [x] Task 3: Thread progress through `domain::workflows::resize` (AC #2, #3)
  - [x] `src/domain/workflows/resize.rs` — add `progress: &dyn Fn(ResizeStage)` to `run`'s signature (`resize.rs:20-26`), same placement convention as Task 2 (after `new_size`, before `luks`).
  - [x] Thread `progress` through to `grow_open_mapping` (`resize.rs:128-165`), which is where every mutating call already lives.
  - [x] Inside `grow_open_mapping`'s `if !device_backed` block (`resize.rs:158-160`), call `progress(ResizeStage::GrowingBackingFile);` immediately before `fs.set_backing_file_size(path, new_size)?;` — placing the call *inside* that existing `if` block is what makes AC #3 true by construction (device-backed resize never enters this block, so the stage never fires).
  - [x] Call `progress(ResizeStage::ResizingLuks2Mapping);` immediately before `luks.resize(mapper)?;` (`resize.rs:162`).
  - [x] Call `progress(ResizeStage::GrowingFilesystem);` immediately before `fs.growfs(mapper, filesystem)` (`resize.rs:164` — currently a tail expression; convert to a statement + explicit `fs.growfs(...)` return so the `progress` call can precede it).

- [x] Task 4: `cli::ux` stage translation (AC #1, #2, #4)
  - [x] AD-19 names the function `translate_stage` in prose, but Rust has no function overloading and `CreateStage`/`ResizeStage` are two distinct enums (per the architecture's own Structural Seed) — implement as two sibling functions, `translate_create_stage(stage: &CreateStage) -> &'static str` and `translate_resize_stage(stage: &ResizeStage) -> &'static str`, in `src/cli/ux.rs`, following the same "exhaustive match, no default arm" discipline `translate` (`ux.rs:17-72`) already uses for `DomainError`. Suggested text (exact wording not AC-mandated, only "no data beyond naming which stage is running" is):
    - `AllocatingBackingFile` → `"Allocating the backing file..."`
    - `FormattingLuks2` → `"Formatting as LUKS2..."`
    - `EnrollingFido2Key` → `"Enrolling your FIDO2 key — touch it now (you may also be asked for its PIN)..."`
    - `CreatingFilesystem` → `"Creating the filesystem..."`
    - `GrowingBackingFile` → `"Growing the backing file..."`
    - `ResizingLuks2Mapping` → `"Resizing the LUKS2 mapping..."`
    - `GrowingFilesystem` → `"Growing the filesystem..."`

- [x] Task 5: Wire the CLI closures (AC #1, #2, #4)
  - [x] `src/cli/main.rs`'s `run_create` (`main.rs:276-302`) — pass `&|stage| println!("{}", ux::translate_create_stage(&stage))` as the new `progress` argument to `create::run` (`main.rs:289-296`), positioned to match `create::run`'s new signature from Task 2.
  - [x] `run_resize` (`main.rs:476-493`) — pass `&|stage| println!("{}", ux::translate_resize_stage(&stage))` as the new `progress` argument to `resize::run` (`main.rs:486`). Keep the existing `"Growing this tomb. Touch your security key now..."` intro line as-is — it covers the upfront `luks.open` touch, which happens before any `ResizeStage` fires; the two don't conflict.
  - [x] Add the two new imports: `use crate::domain::progress::{CreateStage, ResizeStage};` (or import path each closure needs) to `main.rs`'s import block (`main.rs:1-17`).

- [x] Task 6: Update every existing `create::run`/`resize::run` call site for the new parameter (mechanical, no behavior change — required for the crate to compile)
  - [x] Add a shared no-op test helper to `tests/unit/fakes.rs`: `pub fn no_progress<S>(_stage: S) {}` — generic over both `CreateStage` and `ResizeStage` via inference at each call site (`&no_progress` coerces to `&dyn Fn(CreateStage)` or `&dyn Fn(ResizeStage)` depending on context), so one helper covers both workflows instead of two near-duplicates.
  - [x] Update all `create::run(...)` call sites to pass `&no_progress` in the new parameter position: `tests/unit/create.rs` (13 call sites), `tests/unit/workflows.rs` (1 call site).
  - [x] Update all `resize::run(...)` call sites the same way: `tests/unit/resize.rs` (10 call sites), `tests/unit/workflows.rs` (1 call site).
  - [x] `tests/hardware/main.rs` is a separate test binary (no access to `tests/unit/fakes.rs`) — add an equivalent local `fn no_progress<S>(_stage: S) {}` near its other shared helpers, and update every `create::run`/`resize::run` call site there (grep the file for exact count and locations before starting; do not assume the unit-test count applies).

- [x] Task 7: Progress-ordering unit tests (AC #1, #2, #3, #4)
  - [x] New `tests/unit/progress.rs` (register in `tests/unit/main.rs:1-13`, alphabetically after `preflight`), using a `Rc<RefCell<Vec<CreateStage>>>` / `Rc<RefCell<Vec<ResizeStage>>>` captured by the progress closure to record firing order (same `Rc<RefCell<_>>` idiom `CallLog` already uses in `tests/unit/fakes.rs:16-20`, applied here to stages instead of port-call names):
    - `create_file_backed_fires_all_four_stages_in_real_order`: `FakeLuksBackend::passing()` etc., a `CreateTarget::File` target, assert the captured `Vec<CreateStage>` equals `[AllocatingBackingFile, FormattingLuks2, EnrollingFido2Key, CreatingFilesystem]` — this is the test that locks in AC #1's corrected order and would fail if a future change reordered `enroll`/`mkfs` back to epics.md's literal prose order.
    - `create_device_backed_never_fires_allocating_backing_file`: `CreateTarget::Device` target (confirmed), assert the captured vec equals `[FormattingLuks2, EnrollingFido2Key, CreatingFilesystem]` — no `AllocatingBackingFile` entry at all.
    - `resize_file_backed_fires_all_three_stages_in_order`: assert `[GrowingBackingFile, ResizingLuks2Mapping, GrowingFilesystem]`.
    - `resize_device_backed_skips_growing_backing_file`: device-backed fixture (mirror `tests/unit/resize.rs`'s existing device-backed setup), assert `[ResizingLuks2Mapping, GrowingFilesystem]` only — this is AC #3's direct unit-level proof.
  - [x] No new tests needed for `translate_create_stage`/`translate_resize_stage` beyond what exhaustive-match compilation already guarantees (every variant handled or the crate fails to build) — consistent with this codebase's existing minimalism (`translate`'s own exhaustiveness isn't separately tested per-variant either; `tests/unit/ux.rs` tests behavior/content, not coverage).

- [x] Task 8: Marker-bleed check (AC: none directly — CAP-5/NFR3 quality bar, repeatedly flagged by the Epic 2/3 retros as the most-repeated bug class in this codebase)
  - [x] Confirm by inspection that this story introduces no new `AdapterFailure` string and no new `DomainError` variant — `translate_create_stage`/`translate_resize_stage` operate on the new `CreateStage`/`ResizeStage` enums directly, entirely outside the `translate`/`translate_adapter_failure` marker-matching path (`ux.rs:17-288`), so there is no bucket to collide with.

### Review Findings

- [x] [Review][Patch] Progress callback panic (e.g. broken stdout pipe) mid-transaction skips explicit mapper cleanup — resolved as: make the two CLI progress closures in `main.rs` panic-proof (non-panicking stdout write) instead of using `println!` [src/cli/main.rs:294, src/cli/main.rs:232]
- [x] [Review][Patch] No unit test coverage for `translate_create_stage`/`translate_resize_stage` [src/cli/ux.rs:75-97]
- [x] [Review][Patch] No test proves a stage's progress message doesn't fire for work that never completed — all 4 new tests use only passing fakes [tests/unit/progress.rs]
- [x] [Review][Patch] `create::run`/`resize::run` doc comments don't document when/why each stage does or doesn't fire [src/domain/workflows/create.rs:34, src/domain/workflows/resize.rs:21]

## Dev Notes

### Real execution order overrides this AC's prose order (read this before Task 1/2)

`epics.md`'s Story 4.2 AC #1 and the architecture's own illustrative `CreateStage` listing (`ARCHITECTURE-SPINE.md` AD-19) both list stages as: allocating the backing file, formatting as LUKS2, **creating the filesystem, enrolling the FIDO2 key**. The actual, already-implemented, already-shipped code in `src/domain/workflows/create.rs`'s `finish_provisioning` (`create.rs:156-180`) calls `fido2.enroll_fido2_key(...)` (`create.rs:175`) **before** `fs.mkfs(...)` (`create.rs:177`) — the opposite order — and says exactly why in its own comment (`create.rs:164-170`): `systemd-cryptenroll` can only add a keyslot by authenticating with the transient bootstrap passphrase, which must be wiped strictly before `mkfs` runs (AD-3/AD-9's secret-hygiene requirement). This is not negotiable and not something this story should — or safely could — change; reordering `enroll`/`mkfs` to match the epics.md prose would either violate AD-3's wipe-before-mkfs guarantee or require re-deriving a passphrase that's designed to be single-use.

AD-19 itself resolves this in the implementer's favor without saying so explicitly: "`domain::workflows::create`/`::resize` each take a `progress: &dyn Fn(Stage)` parameter, invoking it synchronously at each real stage boundary **as they sequence their existing port calls**" — the callback fires in whatever order the real port calls already happen in, not in whatever order a later doc's prose happened to list them. Treat the epics.md/architecture ordering as a documentation slip, not a requirement to satisfy. This story's AC #1 above has been annotated with the correction; Task 7's `create_file_backed_fires_all_four_stages_in_real_order` test is the concrete guardrail against silently regressing this if someone "fixes" the order later to match the prose.

### Architecture requirements (binding, from ARCHITECTURE-SPINE.md AD-19)

- **AD-19 — Progress reporting is a callback seam, never I/O inside domain:** `create`/`resize` each take a `progress: &dyn Fn(Stage)` parameter (concretely, two distinct enums per this story's Task 1/4 resolution — see below), invoked synchronously at real stage boundaries; `domain` performs zero I/O for these messages, `cli` supplies the closure and does the actual `println!`. This is the same translate-at-the-boundary shape the existing `DomainError -> cli::ux::translate` convention already established (AD-19 explicitly says so) — reuse that shape, don't invent a new one.
- **Payload-free by construction:** every `CreateStage`/`ResizeStage` variant is a bare unit variant, no fields, ever — pinned explicitly by the architecture's own adversarial review (Finding 5, `reviews/review-adversarial-v4.md:77-85`) after finding that AD-19's prose alone ("a typed per-workflow enum") didn't forbid a payload, and a payload-carrying variant at the `FormattingLuks2` boundary could smuggle the still-live transient bootstrap passphrase (AD-3) past its wipe point. Do not add fields to any variant, even ones that seem harmless (e.g. a size or path) — the enum discriminant alone is the entire message, on purpose.
- **Two enums, not one `Stage` type:** the Structural Seed (`ARCHITECTURE-SPINE.md:204`) lists `progress.rs # Stage enum (CreateStage/ResizeStage, AD-19)` — read as two sibling enums under one module, matching how `translate`'s AD-19 extension is phrased as `translate_stage` in prose but must become two functions in real Rust (no overloading). This story's Task 1/4 make that concrete.
- **No new port, no new `DomainError` variant, no new type beyond the two `Stage` enums** — CAP-17's entire architectural footprint is `domain::workflows::create`/`::resize`, `domain::progress`, `cli::ux` (Capability → Architecture Map, `ARCHITECTURE-SPINE.md:261`).

### Prior-story precedent to reuse, not reinvent

- **Param-ordering convention:** every existing workflow places workflow-specific args first, then the three ports last, always in `luks, fido2, fs` order (`unlock::run`'s `read_only: bool`, `resize::run`'s `new_size: u64`, `create::run`'s `target`/`filesystem`/`fido2_selection`). `progress` follows this: last non-port argument, immediately before `luks`.
- **Rollback/close discipline is unaffected:** `create.rs`'s `bootstrap_and_provision` (close-on-failure around `finish_provisioning`) and `resize.rs`'s `run` (close-on-every-exit-path around `grow_open_mapping`) keep their existing error-handling shape untouched — `progress` calls are pure notifications with no `Result`, threaded through as an extra plain parameter, never part of any `?`-propagated chain.
- **Translate-at-the-boundary precedent:** `cli::ux::translate` (`ux.rs:17-72`) is the direct model for `translate_create_stage`/`translate_resize_stage` — exhaustive `match`, no default arm, so a future third `Stage` variant fails to compile until translated (same guarantee `translate`'s own doc comment calls out at `ux.rs:12-16`).
- **`unlock_intro_message`-style pure-function extraction was considered and is not needed here** — unlike `unlock`'s read-only-conditional intro text, `translate_create_stage`/`translate_resize_stage` have no conditional logic to unit-test independent of the closure wiring; the closures in `run_create`/`run_resize` stay untested one-liners, consistent with how `run_create`'s existing `"Creating tomb at {display_path}..."` line is untested today.

### Project Structure Notes

- Touches (mix of NEW and UPDATE — first Epic 4 story to modify existing `create`/`resize` workflow code, not just add a new sibling):
  - `src/domain/progress.rs` — **NEW**, `CreateStage`/`ResizeStage` enums.
  - `src/domain/mod.rs` — UPDATE, register `pub mod progress;`.
  - `src/domain/workflows/create.rs` — UPDATE, new `progress` param threaded through `run`/`bootstrap_and_provision`/`finish_provisioning`, four call sites added.
  - `src/domain/workflows/resize.rs` — UPDATE, new `progress` param threaded through `run`/`grow_open_mapping`, three call sites added.
  - `src/cli/ux.rs` — UPDATE, add `translate_create_stage`/`translate_resize_stage`.
  - `src/cli/main.rs` — UPDATE, wire closures into `run_create`/`run_resize`, new imports.
  - `tests/unit/fakes.rs` — UPDATE, add `no_progress` helper.
  - `tests/unit/create.rs`, `tests/unit/resize.rs`, `tests/unit/workflows.rs` — UPDATE, every existing `create::run`/`resize::run` call site gains the new argument.
  - `tests/unit/progress.rs` — **NEW**, ordering tests (Task 7).
  - `tests/unit/main.rs` — UPDATE, register `mod progress;`.
  - `tests/hardware/main.rs` — UPDATE, every existing `create::run`/`resize::run` call site gains the new argument (local `no_progress` helper, separate binary from `tests/unit`).
  - No changes to `src/ports/*`, `src/adapters/exec/*`, `src/domain/types.rs`, `src/domain/errors.rs` — CAP-17 adds no port and no `DomainError` variant.
- **Scope warning:** this is a signature-breaking change to two existing, widely-called public `domain::workflows` functions. Expect to touch on the order of 25+ existing call sites across `tests/unit/*.rs` and `tests/hardware/main.rs` purely mechanically (adding `&no_progress`) before any new-behavior code is written. Do this pass first and get `cargo build --tests` (hardware tests are `#[ignore]`d but still must compile) green before starting Task 7's new tests — a half-migrated signature change makes it hard to tell a real failure from a stale call site.

### Testing standard (AD-7)

Unit tests against the shared fakes in `tests/unit/fakes.rs`, run in default CI — this story's new coverage (Task 7) needs no fake changes, since `progress` is a plain closure parameter, not a port method; the existing `FakeLuksBackend`/`FakeFido2Backend`/`FakeFilesystemBackend` are unaffected. Hardware-gated scenarios in `tests/hardware/main.rs` need no *new* scenarios for this story (no new capability is being exercised against real hardware, just new stdout messages around existing calls) — only the mechanical signature-migration from Task 6.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 4.2: Real Progress Reporting for Create & Resize]
- [Source: ARCHITECTURE-SPINE.md#AD-19 — Progress reporting is a callback seam, never I/O inside domain]
- [Source: ARCHITECTURE-SPINE.md#Structural Seed — `progress.rs` listed under `domain/`, `translate_stage` listed under `cli/ux.rs`]
- [Source: ARCHITECTURE-SPINE.md#Capability → Architecture Map — CAP-17 row]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/reviews/review-adversarial-v4.md:77-85 — Finding 5, pins every `Stage` variant as payload-free]
- [Source: src/domain/workflows/create.rs:156-180 — `finish_provisioning`, the real enroll-before-mkfs order and its AD-3/AD-9 rationale]
- [Source: src/domain/workflows/resize.rs:128-165 — `grow_open_mapping`, the existing `if !device_backed` gate this story's `GrowingBackingFile` call reuses]
- [Source: src/domain/workflows/unlock.rs:13-19 — param-ordering precedent (`read_only: bool` placed after path, before the three ports)]
- [Source: src/cli/ux.rs:17-72 — `translate`, the exhaustive-match model for `translate_create_stage`/`translate_resize_stage`]
- [Source: src/cli/main.rs:276-302, 476-493 — `run_create`/`run_resize`, the CLI dispatch functions gaining the new closures]
- [Source: tests/unit/fakes.rs:16-20 — `CallLog`'s `Rc<RefCell<_>>` idiom, reused for stage-ordering assertions in Task 7]

## Dev Agent Record

### Agent Model Used

Claude Sonnet 5 (claude-sonnet-5)

### Debug Log References

None — no debugging required; implementation matched the story's Dev Notes and all tests passed on first run after each task's mechanical wiring.

### Completion Notes List

- Task 1: Added `domain::progress` with `CreateStage`/`ResizeStage`, both bare payload-free enums per AD-19/Finding 5. Registered alphabetically in `domain/mod.rs`.
- Task 2: Threaded `progress: &dyn Fn(CreateStage)` through `create::run` → `bootstrap_and_provision` → `finish_provisioning`, firing at the real (not epics.md-prose) order: `AllocatingBackingFile` (File arm only) → `FormattingLuks2` → `EnrollingFido2Key` → `CreatingFilesystem`.
- Task 3: Threaded `progress: &dyn Fn(ResizeStage)` through `resize::run` → `grow_open_mapping`, firing `GrowingBackingFile` only inside the existing `if !device_backed` block (making AC #3 true by construction), then `ResizingLuks2Mapping`, then `GrowingFilesystem`.
- Task 4: Added `translate_create_stage`/`translate_resize_stage` in `cli/ux.rs` as two sibling exhaustive-match functions (no function overloading in Rust), mirroring `translate`'s existing discipline.
- Task 5: Wired `println!`-based closures into `run_create`/`run_resize` in `cli/main.rs`; kept `run_resize`'s existing upfront touch-warning line as-is since it covers `luks.open`, not a `ResizeStage`.
- Task 6: Added `no_progress<S>` no-op helper to `tests/unit/fakes.rs` and a local duplicate in `tests/hardware/main.rs` (separate test binary). Updated all 25 existing `create::run`/`resize::run` call sites (13 create.rs + 10 resize.rs + 2 workflows.rs in `tests/unit`, plus 18 create + 4 resize in `tests/hardware/main.rs`) to pass `&no_progress`.
- Task 7: Added `tests/unit/progress.rs` with the 4 ordering tests specified, using `Rc<RefCell<Vec<Stage>>>` closures, registered alphabetically in `tests/unit/main.rs`. All 4 pass, confirming AC #1's corrected order and AC #3's device-backed exclusions for both workflows.
- Task 8: Confirmed by inspection — `domain::progress` introduces no `DomainError` variant and no new `AdapterFailure` string; `translate_create_stage`/`translate_resize_stage` operate entirely outside the `translate`/`translate_adapter_failure` marker-matching path.
- Full suite: 121 unit tests pass (117 pre-existing + 4 new), `cargo build --tests` and `cargo fmt --check` both clean.

### File List

- `src/domain/progress.rs` — NEW
- `src/domain/mod.rs` — UPDATE
- `src/domain/workflows/create.rs` — UPDATE
- `src/domain/workflows/resize.rs` — UPDATE
- `src/cli/ux.rs` — UPDATE
- `src/cli/main.rs` — UPDATE
- `tests/unit/fakes.rs` — UPDATE
- `tests/unit/create.rs` — UPDATE
- `tests/unit/resize.rs` — UPDATE
- `tests/unit/workflows.rs` — UPDATE
- `tests/unit/progress.rs` — NEW
- `tests/unit/main.rs` — UPDATE
- `tests/hardware/main.rs` — UPDATE
