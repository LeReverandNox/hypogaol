---
baseline_commit: 20cd6f5
---

# Story 2.2: Revoke a FIDO2 Key, Guarded Against Last-Keyslot Lockout

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a user,
I want to revoke a single FIDO2 key's keyslot,
so that a lost or compromised key stops being able to unlock my tomb.

## Acceptance Criteria

1. **Given** a tomb with two or more valid keyslots (each with an associated `systemd-fido2` token), **when** I run revoke targeting one key, **then** its token metadata is removed first, then its keyslot, **and** afterward that key no longer unlocks the volume while other enrolled keys still do. [Source: epics.md#Story 2.2; AD-5]
2. **Given** a tomb with only one valid keyslot remaining, **when** I run revoke targeting that last key, **then** the tool aborts with a clear explanation before touching anything, **and** the volume remains unlockable. [Source: epics.md#Story 2.2; AD-5, NFR8]
3. **Given** the "valid keyslot" count, **when** revoke evaluates it, **then** it counts live LUKS2 header state immediately before the decision, never a cached or prior view, **and** a stale token pointing at an already-gone keyslot is never counted as live. [Source: epics.md#Story 2.2; AD-5]
4. **Given** the revoke command, **when** I target a key that isn't enrolled, **then** the tool reports a clear error rather than silently succeeding or crashing. [Source: epics.md#Story 2.2]
5. **Given** an existing tomb on a raw device/partition instead of a loop-backed file, **when** I run the revoke command, **then** the identical command works unmodified — revoke makes no branching decision based on target type. [Source: epics.md#Story 2.2; AD-12]

## Tasks / Subtasks

- [x] Task 1: Give `KeyslotInfo` enough identity to target a key by label, and add a "key not enrolled" domain error (AC: #4)
  - [x] `src/domain/types.rs`: add `pub key_label: String` to `KeyslotInfo`. This is a required field addition, not additive-only — every existing construction site must be updated (see Project Structure Notes).
  - [x] `src/adapters/exec/mod.rs`'s `list_fido2_keyslots`: for each `systemd-fido2` token contributing a live keyslot, also read `token.get("key_label").and_then(Value::as_str)`. Default to an empty string (`.unwrap_or_default()`) rather than erroring if absent — a foreign/corrupted token missing this field should never crash a listing, and an empty label can never collide with a user-supplied `--label` anyway (CLI's `parse_label` already rejects empty/whitespace-only labels, reused for revoke — see Task 3).
  - [x] `src/domain/errors.rs`: add `KeyNotFound(String)` to `DomainError` (carries the label that wasn't found), e.g. `#[error("no FIDO2 key labeled {0:?} is enrolled on this tomb")] KeyNotFound(String)`.
  - [x] `src/cli/ux.rs`'s `translate`: add the new match arm (the exhaustive match won't compile until you do) — plain-language, e.g. "No enrolled key is labeled {label:?}. Check the label (case-sensitive) and try again."
- [x] Task 2: Implement `domain::workflows::revoke::run` (`src/domain/workflows/revoke.rs`, currently `pub fn run() -> Result<(), DomainError> { todo!() }`) (AC: #1, #2, #3, #4, #5)
  - [x] Signature: `pub fn run(path: &Path, key_label: &str, luks: &dyn LuksBackend, fido2: &dyn Fido2Backend, fs: &dyn FilesystemBackend) -> Result<(), DomainError>`. Unlike `enroll::run`'s `key_label: String`, `&str` is enough here — revoke only compares it, never stores or serializes it.
  - [x] Call `preflight::check(luks, fido2, fs)?` first (AD-4), like every other workflow.
  - [x] Call `luks.list_fido2_keyslots(path)?`, find the entry whose `key_label` matches `key_label` exactly (case-sensitive `==`, consistent with `enroll`'s existing `existing_key_labels` uniqueness-check comparison), and get its `KeyslotRef`. If none matches, return `DomainError::KeyNotFound(key_label.to_string())` (AC #4) — **before** calling anything mutating.
  - [x] Call `crate::domain::keyslot_guard::remove_keyslot_guarded(luks, path, target)` and return its result directly. Do **not** reimplement the last-keyslot count/abort logic or the token-then-keyslot removal ordering — `keyslot_guard.rs`'s own doc comment already states this primitive exists precisely so `Story 2.2 (CAP-3/revoke)` reuses it (AD-5 binds CAP-3 to this exact guard). This one call satisfies AC #1 (removal ordering — already implemented inside `remove_key`), AC #2 (last-keyslot abort), and AC #3 (live, uncached counting, stale-token exclusion — all already implemented inside `list_fido2_keyslots`/`remove_keyslot_guarded`).
  - [x] Do **not** build a `MapperHandle` or call `mapping_name::mapping_name` — unlike `enroll::run`, revoke has no port call that needs one: `list_fido2_keyslots`/`remove_key` both take `path` directly (confirm by inspecting their signatures in `src/ports/luks_backend.rs`). This is also why AC #5 (device-vs-file parity) needs no code — there is no target-type branch to write, the same reasoning `enroll`'s own story gave for its identical AC.
  - [x] `fido2`/`fs` are unused beyond `preflight::check` — document with the same convention `unlock.rs`/`enroll.rs` already use for their own unused port parameters.
- [x] Task 3: Wire the CLI (AC: #1, #2, #4)
  - [x] Add a `Revoke` variant to `Commands` (`src/cli/main.rs`): positional `path` (`#[arg(allow_hyphen_values = true)]`, matching every other subcommand's convention — do not make this a `--path` flag) and a required `--label <LABEL>` flag reusing the existing `parse_label` value parser (empty/whitespace-only labels are already rejected there — no new validator needed).
  - [x] Add `run_revoke(path: PathBuf, label: String)` mirroring `run_enroll`'s shape: preflight check first (same pre-check-then-recheck-inside-`revoke::run` convention `run_enroll`/`run_unlock` already use), a plain-language line before the call (e.g. `"Revoking key \"{label}\" from this tomb."`), call `revoke::run(&path, &label, &adapter, &adapter, &adapter)`, translate any error via `ux::translate`, print a plain success line on `Ok(())` (e.g. `"Key \"{label}\" revoked."`).
  - [x] Dispatch the new `Commands::Revoke` arm in `run()`.
  - [x] Unlike `enroll`, revoke needs **no** `Fido2DeviceSelection`/device-picker flags at all — it never touches a physical key or calls `systemd-cryptenroll`; it only operates on the LUKS2 header/token metadata via `cryptsetup token remove`/`luksKillSlot`. Do not add `--fido2-device`-style flags here.
- [ ] Task 4: Fix a real message-misattribution bug this story's new `revoke` call path will trigger, and update stale scope comments (`src/cli/ux.rs`) (AC: none directly — plain-language correctness, NFR3)
  - [ ] **Real bug, not speculative:** `translate_adapter_failure`'s `ENROLLMENT_MARKERS` array currently includes `"luksDump"` and `"keyslot id"`. Both markers match error strings produced by `dump_json_metadata`/`tokens_object`/`live_keyslot_numbers` (e.g. `"cryptsetup luksDump --dump-json-metadata failed"`, `"luksDump JSON missing a tokens object"`, `"unrecognized keyslot id {id} in luksDump JSON"`) — helpers shared by `enroll`'s label-uniqueness check **and** this story's `list_fido2_keyslots` call inside `revoke::run`. Today a failure there always renders as "Enrolling your security key didn't complete…", which will now be **wrong** whenever the failure actually came from `revoke`. This is the exact same class of cross-workflow marker bleed already fixed once for `unlock` (commit `e0e44cc`, the `"fido2-token"` marker).
    - Remove `"luksDump"` and `"keyslot id"` from `ENROLLMENT_MARKERS` (verified: every string containing `"keyslot id"` in this codebase also contains `"luksDump"`, so dropping both is safe and non-redundant).
    - Add a new, workflow-neutral bucket checked before `ENROLLMENT_MARKERS` (alongside the existing `"fido2-token"` check): if `inner.contains("luksDump")`, return a message that doesn't assume enrollment, e.g. "tomb-fido2 couldn't read this tomb's key information. Make sure the path points at a valid tomb, then try again."
  - [ ] **Second real bug in the same family:** `remove_key`'s own failure strings ("failed to run cryptsetup token remove"/"cryptsetup token remove failed"/"failed to run cryptsetup luksKillSlot"/"cryptsetup luksKillSlot failed") match none of `ENROLLMENT_MARKERS`, so they fall through to the generic `inner.contains("cryptsetup")` bucket — whose message ("your security key or its PIN may not have been accepted in time") is about touching/PIN-entry, meaningless for revoke's non-interactive header edit. Add a new bucket checked **before** the generic `cryptsetup` bucket: if `inner.contains("luksKillSlot") || inner.contains("token remove")`, return e.g. "Revoking that key didn't complete. Nothing has been changed — try again."
  - [ ] Update `translate`'s module-level doc comment and `translate_adapter_failure`'s doc comment (both currently say "create"/"unlock"/"enroll") to also name `revoke`.
  - [ ] Update `DomainError::LastKeyslotGuard`'s match-arm comment — it currently reads "Not reachable from `create`/`unlock` today — only `domain::workflows::revoke` (Story 2.2) produces this," written as a forward reference before this story existed. Now that `revoke` is implemented, reword to state plainly it's the only producer (drop the "(Story 2.2)" forward-looking phrasing).
  - [ ] Update `src/domain/keyslot_guard.rs`'s doc comment the same way — it currently says "Story 2.2 (CAP-3/revoke) reuses this same primitive," also written before this story existed; reword to name `domain::workflows::revoke::run` directly now that it exists.
- [ ] Task 5: Tests (AC: #1, #2, #3, #4, #5)
  - [ ] Update every existing `KeyslotInfo { keyslot: ... }` construction site for the new required `key_label` field (`tests/unit/fakes.rs`'s `FakeLuksBackend::passing()` default keyslot, `tests/unit/keyslot_guard.rs`'s three tests) — pick any non-empty label per test's own intent (e.g. `"primary"`).
  - [ ] `tests/unit/fakes.rs`: add a `last_removed_keyslot()` accessor to `FakeLuksBackend` (mirroring the existing `last_open()`/`last_bootstrap_size()` pattern) so a test can assert *which* `KeyslotRef` was actually passed to `remove_key` — needed to prove label-to-keyslot resolution picked the right one, not just that removal happened.
  - [ ] New `tests/unit/revoke.rs` (register `mod revoke;` in `tests/unit/main.rs`), fake-backed, mirroring `tests/unit/enroll.rs`'s style:
    - Preflight failure short-circuits before any port call.
    - Happy path: given two keyslots with distinct labels (e.g. `KeyslotRef(0)` labeled `"primary"`, `KeyslotRef(1)` labeled `"backup"`), revoking `"backup"` calls `remove_key` with `KeyslotRef(1)` specifically (assert via `last_removed_keyslot()`), and the call log shows `list_fido2_keyslots` before `remove_key`.
    - Targeting a label that matches no enrolled keyslot returns `DomainError::KeyNotFound("nonexistent".to_string())` and never calls `remove_key` (assert via the call log).
    - Revoking the sole remaining keyslot (single-entry `with_keyslots`) returns `DomainError::LastKeyslotGuard` and never calls `remove_key` — this exercises `keyslot_guard::remove_keyslot_guarded`'s existing, already-tested guard through `revoke::run`'s own call path, not a reimplementation.
    - An underlying `remove_key` failure (`with_failure_at("remove_key")`) propagates as `DomainError::AdapterFailure` untouched.
  - [ ] `tests/unit/cli.rs`: add `revoke_help_lists_path_as_positional_and_label_as_a_flag`, following `enroll_help_lists_path_as_positional_and_label_as_a_flag`'s exact pattern.
  - [ ] `tests/hardware/main.rs` (manual, `#[ignore]`, per AD-7):
    - `revoke_removes_a_key_without_affecting_others`: create a tomb (primary key), enroll a second key labeled distinctly (reuse the existing enroll scenario's setup shape), revoke the **primary** by label, then assert via `list_fido2_keyslots` that exactly one live keyslot remains and via `--dump-json-metadata` that only the backup's label is present (mirrors Story 2.1's own label-survival assertion style) — the concrete regression check for Task 1/2's label-to-keyslot resolution. Also assert the backup key still unlocks the tomb via `unlock::run` (AC #1's "other enrolled keys still do").
    - `revoke_aborts_on_the_last_remaining_key`: create a tomb (one key), attempt `revoke::run` targeting that key's label, assert it returns `DomainError::LastKeyslotGuard`, then assert the tomb is still unlockable via `unlock::run` (AC #2's explicit "volume remains unlockable").
    - No separate device-backed variant needed (AC #5) — `revoke::run` has no target-type branch to test around, confirmed by inspection (same reasoning as `enroll`'s AC #5).
  - [ ] Confirm `cargo build --lib`, `make test` (`cargo test --lib --test unit`), `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --check` all stay green, then run `make test-hardware` by hand (root required, two physical FIDO2 keys needed for the first scenario) before considering this story done.

## Dev Notes

- **The real risk in this story is Task 1's `KeyslotInfo` field addition, not the domain workflow.** `domain::workflows::revoke::run` (Task 2) is almost entirely a thin orchestration over two already-implemented, already-hardware-verified primitives: `LuksBackend::list_fido2_keyslots`/`remove_key` (used since Story 1.5/1.6's bootstrap-cleanup path and Story 2.1's own reuse) and `keyslot_guard::remove_keyslot_guarded` (AD-5's guard, written specifically anticipating this story). Don't over-engineer Task 2 — it should end up close to enroll's `run` in length, arguably shorter since there's no `MapperHandle`/mapping-name involved at all.
- **`remove_keyslot_guarded` already exists and is already unit-tested** (`src/domain/keyslot_guard.rs`, `tests/unit/keyslot_guard.rs`) — it was originated by Story 1.5 for `create`'s transient-bootstrap-keyslot cleanup, with its doc comment explicitly anticipating this story's reuse. Call it; do not reimplement the count-then-abort-or-remove logic inline in `revoke::run`.
- **Why `KeyslotInfo` needs a new field:** today it only carries `keyslot: KeyslotRef` — enough for `keyslot_guard`'s counting, but not enough for revoke to resolve a user-supplied `--label` to a specific keyslot. `AD-2` already anticipated this need ("`credential_id` is read... so `revoke` can display which physical key a keyslot belongs to"), but only asks for identification-by-label here, not a full listing UI — adding `credential_id`/`created_at` display is explicitly **out of scope** for this story's ACs (none of them ask for a listing view) and is left as a forward-looking gap, not a task. Do not build a `list`/`show-keys` subcommand — it isn't in any AC.
- **A missing/empty `key_label` on a foreign or corrupted token must never accidentally match a real target.** The adapter defaults an absent `key_label` to `""` (Task 1) rather than erroring the whole listing — and the CLI's `--label` flag already rejects empty/whitespace labels via the existing `parse_label` validator (reused, not reimplemented), so `""` can never be what a user types. This is a deliberate, cheap safety property — call it out in review if changed.
- **Two real, not speculative, plain-language bugs surface because of this story specifically** (Task 4) — both are instances of the same "a shared error-string helper gets misclassified by a workflow-specific marker" class that Story 2.1's own review already fixed once for `unlock` (commit `e0e44cc`, the `"fido2-token"` marker). This story's `list_fido2_keyslots`/`remove_key` calls reuse `dump_json_metadata`/`tokens_object` and produce `"luksKillSlot"`/`"token remove"` failures respectively — neither was reachable from any workflow except `enroll` until now, so nobody hit this until `revoke` exists. Fix both (see Task 4) rather than leaving revoke's own failures mislabeled as enrollment or touch/PIN problems.
- **Case-sensitive exact-match label lookup**, consistent with `enroll`'s existing label-uniqueness check (`existing_key_labels`'s `label == &metadata.key_label` comparison) — don't introduce case-insensitive or fuzzy matching; that would be new, unrequested scope.
- **Previous story's own scope discipline still applies:** keep this diff scoped to exactly what the ACs above need. Do **not** touch `close`/`resize` (still `todo!()` stubs, later stories) or `mapping_name.rs`. Unlike Story 2.1 (which the user explicitly approved widening into `create.rs`), there is no forcing signature change here that would require touching `create`/`unlock`/`enroll`'s own workflow bodies — `KeyslotInfo`'s new field only requires updating call/construction *sites* (tests, the one adapter function), not those workflows' logic.
- **Git intelligence:** baseline is `20cd6f5` ("feat(2.1): enroll an additional FIDO2 key (#23)", current `main` HEAD as of this story's creation) — no commits since. All file:line references above are accurate as of that commit.

### Project Structure Notes

- Modified: `src/domain/types.rs` (`KeyslotInfo` gains `key_label: String`), `src/domain/errors.rs` (new `DomainError::KeyNotFound(String)`), `src/domain/workflows/revoke.rs` (implement `run`, replacing the current no-arg `todo!()`), `src/adapters/exec/mod.rs` (`list_fido2_keyslots` populates the new field; no other adapter logic changes — `remove_key`/`dump_json_metadata`/etc. are reused as-is), `src/cli/main.rs` (new `Revoke` subcommand + `run_revoke`), `src/cli/ux.rs` (`KeyNotFound` translation arm; `ENROLLMENT_MARKERS` fix; two new revoke/neutral message buckets; stale scope-comment updates), `src/domain/keyslot_guard.rs` (doc-comment update only, no logic change), `tests/unit/main.rs` (register new test module), `tests/unit/fakes.rs`, `tests/unit/keyslot_guard.rs`, `tests/unit/cli.rs`, `tests/hardware/main.rs`.
- New: `tests/unit/revoke.rs`.
- Do **not** touch `src/domain/workflows/{close,resize}.rs` (still `todo!()`, later stories), `src/domain/mapping_name.rs`, `src/ports/fido2_backend.rs`, or `src/domain/workflows/{create,enroll,unlock}.rs`'s own logic (only their *tests'* `KeyslotInfo` construction sites, if any — confirm by grep before assuming none exist).
- Consistent with `ARCHITECTURE-SPINE.md`'s Structural Seed: no new module, no new port, no architecture-spine amendment — `revoke.rs` fills in an already-declared file, `KeyslotInfo` gains one field the port trait signatures don't need to change for (`list_fido2_keyslots`'s return type `Vec<KeyslotInfo>` already accommodates it).

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 2.2: Revoke a FIDO2 Key, Guarded Against Last-Keyslot Lockout] (full AC text this story's Acceptance Criteria section is drawn from verbatim)
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-5] (last-keyslot guard definition, ordering, and explicit call-out that CAP-3/revoke reuses `keyslot_guard`'s primitive)
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-2] (token metadata schema; the `credential_id`/`created_at` "displayed at revoke-time listing" forward reference this story deliberately does not implement)
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-12] (deterministic mapping name/no registry — confirms why AC #5 needs no branch, and why revoke needs no mapping-name derivation at all)
- [Source: src/domain/keyslot_guard.rs] (`remove_keyslot_guarded` — the primitive Task 2 must call, not reimplement)
- [Source: tests/unit/keyslot_guard.rs] (existing, already-passing tests for the guard logic revoke now exercises indirectly)
- [Source: src/ports/luks_backend.rs] (`list_fido2_keyslots`/`remove_key` signatures — both take `path` directly, confirming no `MapperHandle` is needed)
- [Source: src/adapters/exec/mod.rs:353-408] (`live_keyslot_numbers`/`systemd_fido2_token_ids`/`existing_key_labels` — the existing label/liveness-reading helpers Task 1 extends)
- [Source: src/adapters/exec/mod.rs:916-950] (`list_fido2_keyslots`'s current implementation — the function Task 1 must extend with `key_label`)
- [Source: src/adapters/exec/mod.rs:952-1008] (`remove_key`'s current implementation — already AD-5-ordered, reused unchanged; also the source of Task 4's second marker bug)
- [Source: src/cli/ux.rs] (`translate`/`translate_adapter_failure` — Task 1's `KeyNotFound` arm and Task 4's marker fixes both land here)
- [Source: src/domain/workflows/enroll.rs] (shape/style precedent for a thin `domain::workflows::*::run` function with unused-port-parameter documentation)
- [Source: src/cli/main.rs] (`Commands::Enroll`/`run_enroll` — the CLI wrapper shape `Commands::Revoke`/`run_revoke` should mirror)
- [Source: tests/unit/enroll.rs] (test-style precedent for the new `tests/unit/revoke.rs`)
- [Source: tests/unit/fakes.rs:22-107] (`FakeLuksBackend` — extend with `last_removed_keyslot()`, same pattern as `last_open()`/`last_bootstrap_size()`)
- [Source: tests/hardware/main.rs:716-836] (`enroll_adds_an_independent_second_key_without_corrupting_the_primary` — setup/assertion style precedent for this story's two hardware scenarios, including the `pause()` helper for key-swap-dependent unlock proofs)
- [Source: _bmad-output/implementation-artifacts/2-1-enroll-an-additional-fido2-key.md] (previous story — `Fido2DeviceSelection` design history, the `e0e44cc` marker-bleed fix this story's Task 4 repeats for a new pair of markers, and the token-diffing/rollback precedent Task 1 must not disturb)

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

- Task 1: Added `key_label: String` to `KeyslotInfo`; `list_fido2_keyslots` now reads `key_label` from each token (defaulting to `""` if absent); added `DomainError::KeyNotFound(String)` and its `ux::translate` arm.
- Task 2: Implemented `domain::workflows::revoke::run` — preflight, resolve `key_label` to a `KeyslotRef` via `list_fido2_keyslots`, delegate to `keyslot_guard::remove_keyslot_guarded`.
- Task 3: Added `Commands::Revoke` (positional path + required `--label`) and `run_revoke`, mirroring `run_enroll`'s shape; dispatched in `run()`.

### File List

- src/domain/types.rs
- src/adapters/exec/mod.rs
- src/domain/errors.rs
- src/cli/ux.rs
- src/domain/workflows/revoke.rs
- src/cli/main.rs
