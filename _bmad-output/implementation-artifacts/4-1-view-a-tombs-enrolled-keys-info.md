---
baseline_commit: 088d063a93c2f919c5c1bc57112bed196514f6cb
---

# Story 4.1: View a Tomb's Enrolled Keys (Info)

Status: review

## Story

As a user,
I want to view a tomb's technical info including its enrolled FIDO2 keys and labels,
so that I can check what's enrolled without unlocking the tomb.

## Acceptance Criteria

1. **Given** an existing tomb **When** I run the info command against it **Then** `domain::preflight` runs first, like every other workflow, **and** it lists each currently enrolled FIDO2 keyslot with its `key_label`, without performing any unlock/open call.
2. **Given** the info output **When** displayed **Then** it shows `key_label` per keyslot only — `credential_id`, `created_at`, and `filesystem` stay internal, not part of v1's info output.
3. **Given** an existing tomb on a raw device/partition instead of a loop-backed file **When** I run info **Then** the identical command works unmodified.

## Tasks / Subtasks

- [x] Task 1: Add `domain::workflows::info` (AC #1, #2)
  - [x] Create `src/domain/workflows/info.rs`, mirroring `revoke.rs`'s shape (`src/domain/workflows/revoke.rs:15-21`): `pub fn run(path: &Path, luks: &dyn LuksBackend, fido2: &dyn Fido2Backend, fs: &dyn FilesystemBackend) -> Result<Vec<KeyslotInfo>, DomainError>`. Body is exactly `preflight::check(luks, fido2, fs)?;` followed by `luks.list_fido2_keyslots(path)` — no other port calls. `fido2`/`fs` are unused beyond `preflight::check`, same documented pattern `revoke.rs`'s doc comment already uses for its own unused params (AD-4's uniform three-port gate).
  - [x] `KeyslotInfo` (`src/domain/types.rs:56-59`) already carries only `keyslot`/`key_label` — no header/token schema change needed, and no risk of leaking `credential_id`/`created_at`/`filesystem` since they aren't fields on this struct at all (AC #2 is satisfied by the existing type, not by any new filtering logic).
  - [x] `LuksBackend::list_fido2_keyslots` (`src/ports/luks_backend.rs:30`) already reads fresh from the header via `cryptsetup luksDump --dump-json-metadata` (`src/adapters/exec/mod.rs:917-957`, `dump_json_metadata`) with no `luksOpen`/mount call anywhere in that path — AC #1's "without performing any unlock/open call" is already true of the existing adapter method; this task only wires a new workflow entry point to it, it does not change the adapter.
  - [x] Register the new module in `src/domain/workflows/mod.rs:1-6` (add `pub mod info;` alphabetically before `pub mod resize;`).

- [x] Task 2: Wire the `info` CLI subcommand (AC #1, #3)
  - [x] In `src/cli/main.rs`, add an `Info` variant to `Commands` (after `Resize`, `src/cli/main.rs:87-97`), taking one positional `path: PathBuf` field with `#[arg(allow_hyphen_values = true)]`, same shape as `Close`/`Resize`. Doc-comment: `/// Show a tomb's technical info, including its enrolled FIDO2 keys`.
  - [x] Add `use crate::domain::workflows::info;` to the import block (`src/cli/main.rs:10-15`, alphabetically between `enroll` and `resize`).
  - [x] Add a `run_info(path: PathBuf)` function mirroring `run_close`'s shape (`src/cli/main.rs:441-458`): build `ExecAdapter::default()`, call `preflight::check` first (early exit via `ux::translate` + `process::exit(1)` on failure, matching every other `run_*` function's convention), then call `info::run(&path, &adapter, &adapter, &adapter)`. On `Ok(keyslots)`, print a header line naming the tomb path followed by one indented line per keyslot showing only its `key_label` (e.g. `"Enrolled FIDO2 keys for {path}:"` then `"  - {key_label}"` per entry) — no `credential_id`/`created_at`/`filesystem` in the output (AC #2). On `Err`, same `ux::translate` + `exit(1)` pattern as every other command. No "Running info..." intro line is needed before the call — unlike `unlock`/`resize`, info never touches a physical FIDO2 key, so there's no touch/PIN prompt to warn about beforehand (same reasoning `run_close`'s doc comment already gives for skipping that intro).
  - [x] Add `Commands::Info { path } => run_info(path)` to the `match` in `run()` (`src/cli/main.rs:487-540`).
  - [x] No target-type branching anywhere in this path (`info::run` takes one `path` for both file- and device-backed targets, same as `revoke`/`close`) — AC #3 is satisfied by construction, not by an explicit check.

- [x] Task 3: Marker-bleed check for any new failure text (AC: none directly — CAP-5/NFR3 quality bar, flagged repeatedly by the Epic 2/3 retros as the most-repeated bug class in this codebase)
  - [x] This story introduces no new `AdapterFailure` string shape — a failed `cryptsetup luksDump` on a non-existent or non-LUKS2 path already falls through `dump_json_metadata`'s existing error path (reused unmodified by `list_fido2_keyslots`) into the existing dedicated `"luksDump"` bucket in `src/cli/ux.rs:102` (shared with `enroll`/`revoke`, not the generic `"cryptsetup"` bucket). Confirmed by inspection: every `dump_json_metadata` error string (`src/adapters/exec/mod.rs:324-343`) contains the literal substring `"luksDump"`, so no new branch is needed.
  - [x] Do not add a new `DomainError` variant for this story — `info::run` has no new failure mode beyond preflight failure and whatever `list_fido2_keyslots` already returns (both already-typed).

- [x] Task 4: Unit tests (AC #1, #2)
  - [x] New `tests/unit/info.rs`, registered in `tests/unit/main.rs:1-13` (alphabetically before `keyslot_guard`). Mirror `tests/unit/revoke.rs`'s fakes-based style:
    - `preflight_failure_short_circuits_before_any_port_call`: same shape as `revoke.rs:10-33` — a failing `FakeFido2Backend`, assert `Err(DomainError::PreflightFailed(_))` and an empty call log (no `list_fido2_keyslots` call).
    - `happy_path_returns_every_enrolled_keyslots_label`: `FakeLuksBackend::passing().with_keyslots(vec![...two entries...])`, assert `info::run(...)` returns `Ok(keyslots)` equal to that same vec, and the call log is exactly `["list_fido2_keyslots"]` — no `open`/`remove_key`/other port calls (locks in AC #1's "no unlock/open call" at the fakes level, and that info makes exactly one port call, unlike revoke's two).
    - `empty_tomb_returns_an_empty_list`: `FakeLuksBackend::passing()` **corrected** — `passing()` actually seeds one default `"primary"` keyslot (`tests/unit/fakes.rs:51-54`), not none, so this test uses `.with_keyslots(vec![])` explicitly to get a genuinely empty tomb — assert `Ok(vec![])` rather than an error; info is a pure read with no last-keyslot-style guard of its own.
  - [x] `tests/unit/cli.rs`: add `info_help_lists_path_as_positional` following `revoke_help_lists_path_as_positional_and_label_as_a_flag`'s shape (`tests/unit/cli.rs:145-150`) — assert `help_text(&["tomb-fido2", "info", "--help"])` contains `"<PATH>"` and not `"--path"`. Add `top_level_help_lists_all_subcommands` coverage for `"info"` too (`tests/unit/cli.rs:80-87` currently only checks `create`/`unlock`/`enroll`/`revoke` — extend the same assertion list rather than adding a duplicate test).

- [x] Task 5: Hardware tests (manual-only, `make test-hardware`, AD-7 — not run in default CI)
  - [x] In `tests/hardware/main.rs`, add `info_lists_enrolled_keys_without_unlocking`: `create::run` a file-backed tomb (touch the key when prompted, mirroring `revoke_removes_a_key_without_affecting_others`'s setup at `tests/hardware/main.rs:1070-1093`), then call `info::run(&path, &adapter, &adapter, &adapter)` **without any preceding `unlock::run`/mount** — assert `Ok(keyslots)` with exactly one entry whose `key_label` is `"primary"`. This is the concrete proof of AC #1's "without performing any unlock/open call": if `info::run` accidentally required an open mapping, this test would hang waiting for a touch prompt that never comes, or fail outright since nothing was ever mounted.
  - [x] Add `info_works_unmodified_against_a_device_backed_tomb`: same shape via the existing `LoopDevice` helper (`tests/hardware/main.rs:26-30`, mirroring how `enroll`/`unlock`'s device-backed scenarios reuse it), same assertion — proves AC #3's "identical command" with no branch to test around, same reasoning `enroll`'s/`revoke`'s own AC #5 hardware-test doc comments already give for not needing anything fancier than calling the same function against a different target type.
  - [x] No enroll/second-key step needed for either scenario (unlike the revoke hardware tests) — info only needs to prove it reads what's already there, one key is sufficient.

## Dev Notes

### Architecture requirements (binding, from ARCHITECTURE-SPINE.md AD-15, AD-4)

- **AD-15 — Info reuses the existing keyslot-listing query, no new port method:** `domain::workflows::info(path)` calls `preflight` (AD-4) then `LuksBackend::list_fido2_keyslots(path)` directly — the exact same call `revoke`'s guard already makes — and hands the result to `cli` for display. No `unlock`/`open` call is made; `KeyslotInfo{keyslot, key_label}` already satisfies the success criterion with no header/token schema change. **Scoping is explicit, not a silence:** info surfaces `key_label` per keyslot only — `credential_id`, `created_at`, and `filesystem` stay internal, read only by the workflows that already consume them (revoke's display, resize's growfs selection), and are not part of `info`'s v1 output.
- **AD-4 — Mandatory shared pre-flight gate:** extends identically to `info` — it calls `preflight` first too, even though it's a read-only query; the dependency gate is orthogonal to and never waived by that.
- This is the smallest kind of story in this codebase: **no new port method, no new `DomainError` variant, no new type.** Everything Story 4.1 needs (`KeyslotInfo`, `list_fido2_keyslots`) already exists and is already exercised by `revoke`'s tests — this story only adds a thin new workflow entry point and a CLI subcommand on top of existing, already-correct plumbing.

### Prior-story precedent to reuse, not reinvent

- **Workflow shape:** `revoke.rs` (`src/domain/workflows/revoke.rs`) is the closest sibling — same preflight-then-`list_fido2_keyslots` call, same unused-`fido2`/`fs`-params-for-AD-4 doc-comment pattern. `info::run` is strictly simpler: no label matching, no guard, no mutation, just `preflight` then return the list.
- **CLI dispatch shape:** `run_close` (`src/cli/main.rs:441-458`) is the closest sibling among the `run_*` functions — no confirmation prompt, no physical-key touch warning, preflight-check-then-call-then-report. `run_info` follows the same shape one level simpler still (no "Closing this tomb." intro is needed either, though one may be added for symmetry if it reads naturally — not required by any AC).
- **"Identical command, no branching on target type" convention:** every prior workflow (`unlock`, `enroll`, `revoke`, `close`, `resize`) already established this; `info` inherits it for free since it takes the same one `path: &Path` shape as `revoke`, which has no target-type branch to test around either.
- **Marker-bleed guard:** called out as its own task (Task 3), the single most-repeated lesson across Epics 1–3 per the Epic 2/3 retros. This story is lower-risk than most on this front since `list_fido2_keyslots`'s error path is already exercised (unmodified) by `revoke`'s existing tests.

### Project Structure Notes

- Touches (mostly NEW, minimal UPDATE — first Epic 4 story, no existing behavior to preserve):
  - `src/domain/workflows/info.rs` — **NEW**, the workflow function.
  - `src/domain/workflows/mod.rs` — UPDATE, add `pub mod info;`.
  - `src/cli/main.rs` — UPDATE, add `Info` subcommand variant, `run_info` function, match arm, and the `info` module import.
  - `tests/unit/info.rs` — **NEW**, unit tests against the shared fakes.
  - `tests/unit/main.rs` — UPDATE, register the new test module.
  - `tests/unit/cli.rs` — UPDATE, add `info`'s help-text test and extend the top-level help-lists-subcommands assertion.
  - `tests/hardware/main.rs` — UPDATE, add 2 new manual-only scenarios.
- No changes to `src/ports/*`, `src/adapters/exec/mod.rs`, `src/domain/types.rs`, or `src/domain/errors.rs` — everything this story needs already exists there (AD-15's whole point).

### Testing standard (AD-7)

Unit tests against the shared fakes in `tests/unit/fakes.rs` (no fake changes needed — `FakeLuksBackend::list_fido2_keyslots`/`with_keyslots` already exist and are already exercised by `revoke.rs`'s tests), run in default CI. Hardware-gated scenarios in `tests/hardware/main.rs`, manual-only via `make test-hardware`, never in CI.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 4.1: View a Tomb's Enrolled Keys (Info)]
- [Source: ARCHITECTURE-SPINE.md#AD-15 — Info reuses the existing keyslot-listing query, no new port method]
- [Source: ARCHITECTURE-SPINE.md#AD-4 — Mandatory shared pre-flight gate (explicitly names `info` as getting the same gate, not a lighter one)]
- [Source: ARCHITECTURE-SPINE.md#Structural Seed — `info.rs` listed among `domain/workflows/`, `cli/main.rs` noted as gaining `info`/`close-all`/`slam` subcommands]
- [Source: src/domain/workflows/revoke.rs — closest sibling workflow shape (preflight + `list_fido2_keyslots`, unused-port-params doc pattern)]
- [Source: src/domain/types.rs:56-59 — `KeyslotInfo`, already scoped to `keyslot`/`key_label` only, satisfying AC #2 by construction]
- [Source: src/ports/luks_backend.rs:30 — `list_fido2_keyslots` trait method, already documented as reading fresh from the header]
- [Source: src/adapters/exec/mod.rs:917-957 — `list_fido2_keyslots`'s existing implementation via `cryptsetup luksDump --dump-json-metadata`, no `luksOpen` call anywhere in this path]
- [Source: src/cli/main.rs:441-458 — `run_close`, the closest sibling among the `run_*` CLI dispatch functions]
- [Source: tests/unit/revoke.rs — closest sibling unit-test shape against the shared fakes]
- [Source: tests/hardware/main.rs:1055-1138 — `revoke_removes_a_key_without_affecting_others`, closest sibling hardware-test shape and the `LoopDevice` helper's usage pattern for a device-backed variant]

## Dev Agent Record

### Agent Model Used

Claude Sonnet 5 (claude-sonnet-5)

### Debug Log References

### Completion Notes List

- Task 1: Added `domain::workflows::info::run`, mirroring `revoke.rs`'s preflight-then-`list_fido2_keyslots` shape. No new port method, type, or `DomainError` variant. Registered `pub mod info;` in `workflows/mod.rs`. `cargo build` passes.
- Task 2: Wired `Commands::Info { path }` and `run_info`, mirroring `run_close`'s preflight-then-call-then-report shape. Output prints only `key_label` per keyslot (AC #2), no target-type branching (AC #3). `cargo build` passes.
- Task 3: Verified by inspection (no code change) — `dump_json_metadata`'s error strings all contain `"luksDump"`, already routed by the existing dedicated bucket in `src/cli/ux.rs:102`; no marker-bleed risk introduced.
- Task 4: Added `tests/unit/info.rs` (3 tests) and `info_help_lists_path_as_positional` + extended `top_level_help_lists_all_subcommands` in `tests/unit/cli.rs`. Corrected the story's `empty_tomb_returns_an_empty_list` assumption: `FakeLuksBackend::passing()` seeds a default `"primary"` keyslot (`tests/unit/fakes.rs:51-54`), so the test calls `.with_keyslots(vec![])` explicitly rather than relying on a default that isn't actually empty. `cargo test` — 126 passed, 0 failed, 16 ignored (hardware). `cargo clippy --all-targets` — no issues.
- Task 5: Added `info_lists_enrolled_keys_without_unlocking` and `info_works_unmodified_against_a_device_backed_tomb` to `tests/hardware/main.rs`. Both run via `make test-hardware` against a real FIDO2 key — user confirmed both passed. Additionally manually verified the real CLI end-to-end against a tomb with two enrolled keys: `info` printed both `key_label`s (`primary`, `popsink`) with no `credential_id`/`created_at`/`filesystem` leakage, confirming AC #1/#2 outside the test suite too.

### File List

- src/domain/workflows/info.rs (NEW)
- src/domain/workflows/mod.rs (UPDATE)
- src/cli/main.rs (UPDATE)
- tests/unit/info.rs (NEW)
- tests/unit/main.rs (UPDATE)
- tests/unit/cli.rs (UPDATE)
- tests/hardware/main.rs (UPDATE)

### Change Log

- 2026-07-27: Implemented Story 4.1 end to end — `domain::workflows::info`, the `info` CLI subcommand, unit tests, and hardware tests. All ACs satisfied; 126 unit tests + 2 hardware tests passing; clippy clean.
