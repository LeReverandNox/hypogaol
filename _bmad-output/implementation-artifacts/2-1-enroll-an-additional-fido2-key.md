---
baseline_commit: c005771
---

# Story 2.1: Enroll an Additional FIDO2 Key

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a user with an already-created tomb,
I want to enroll an additional FIDO2 key as an alternate unlock method,
so that I have a backup way to unlock my tomb if I lose my primary key.

## Acceptance Criteria

1. **Given** an existing tomb with one enrolled FIDO2 key, **when** I run the enroll command with a second physical key and touch it when prompted, **then** a new LUKS2 keyslot is created and a `systemd-fido2` token is written with generic `key_label`/`credential_id`/`created_at` metadata, **and** these fields are never prefixed with the product's placeholder name. [Source: epics.md#Story 2.1; AD-2, AD-13]
2. **Given** the tomb now has two enrolled keys, **when** I unlock with either the original or the newly enrolled key, **then** the volume unlocks successfully with either one. [Source: epics.md#Story 2.1]
3. **Given** LUKS2's native multi-keyslot support (up to 32 slots), **when** enrolling, **then** the operation works within that limit, **and** enroll runs `domain::preflight` first like every other workflow. [Source: epics.md#Story 2.1; AD-4]
4. **Given** the enroll workflow needs an existing-passphrase or FIDO2 PIN prompt, **when** any such secret entry occurs, **then** it runs with inherited/passthrough stdio, never captured by the tool's own process, **and** it is always a separate subprocess call from any non-secret credential-id lookup (e.g. `fido2-token -L`). [Source: epics.md#Story 2.1; AD-3]
5. **Given** an existing tomb on a raw device/partition instead of a loop-backed file, **when** I run the enroll command, **then** the identical command works unmodified — enroll makes no branching decision based on target type, consistent with the deterministic mapping-name/mountpoint discovery (AD-12). [Source: epics.md#Story 2.1; AD-12]

## Tasks / Subtasks

- [ ] Task 1: **Critical fix required** — `find_systemd_fido2_token_id` (`src/adapters/exec/mod.rs:377-388`) picks the *first* `systemd-fido2` token it finds in `luksDump`'s JSON, with no way to distinguish which token is new. That's harmless today because `create`'s bootstrap flow is the only caller and there's exactly one such token at that point — but once this story adds a second enrolled key, a tomb can have *two* `systemd-fido2` tokens, and picking "the first one" risks silently overwriting the **original** key's `key_label`/`created_at` with the new key's metadata instead of writing the new token, corrupting the primary key's identification. (AC #1)
  - [ ] Before invoking `systemd-cryptenroll` in `enroll_fido2_key`, snapshot the set of existing `systemd-fido2` token ids (reuse `dump_json_metadata`/`tokens_object`, already in this file)
  - [ ] After `systemd-cryptenroll` succeeds, identify the token id present now but absent from that snapshot — that is the newly-created token, and the only one `write_fido2_token_metadata` may write to. Return a clear `AdapterFailure` if the diff doesn't yield exactly one new token id (defensive — should never happen if `systemd-cryptenroll` itself succeeded)
  - [ ] This fix subsumes `find_systemd_fido2_token_id`'s current job for *both* callers (create's bootstrap enroll and this story's standalone enroll) — in create's case the "before" snapshot is simply empty, so behavior there is unchanged, just no longer fragile
- [ ] Task 2: Implement `domain::workflows::enroll::run` (`src/domain/workflows/enroll.rs`, currently `todo!()`) (AC: #1, #3, #5)
  - [ ] Signature: `pub fn run(path: &Path, key_label: String, luks: &dyn LuksBackend, fido2: &dyn Fido2Backend, fs: &dyn FilesystemBackend) -> Result<(), DomainError>` — `fs` is unused beyond `preflight::check`, same documented pattern as `unlock.rs`'s unused `fido2` parameter (`src/domain/workflows/unlock.rs:10-12`)
  - [ ] Call `preflight::check(luks, fido2, fs)?` first (AD-4), like every other workflow
  - [ ] Derive `name = mapping_name::mapping_name(path)?` and build a `MapperHandle { name, source_path: path.to_path_buf() }` **without** calling `luks.open` or `fs.mount` — `systemd-cryptenroll`/`cryptsetup token *` operate directly on the LUKS2 header at `mapper.source_path`; the tomb never needs to be unlocked/mounted to enroll a new key (confirm by inspecting `enroll_fido2_key`'s current body: it only ever reads `mapper.source_path`, never `mapper.device_node()`). This is also why AC #5 (device-vs-file parity) falls out for free — there is no target-type branch to write
  - [ ] Build `KeyMetadata { key_label, filesystem: Filesystem::Ext4 }` — `Filesystem` has exactly one variant today (AD-8), so there is nothing to read back from the existing tomb or ask the user for; do **not** add a `--filesystem` flag to the CLI for this command (would misleadingly imply enroll can change the tomb's filesystem). If a second `Filesystem` variant is ever added later, this hardcoding would need to read the existing token's `filesystem` field instead — out of scope here, note it as a forward-looking gap only
  - [ ] Call `fido2.enroll_fido2_key(&mapper, metadata)` and propagate its result directly — no keyslot-count guard needed here (unlike revoke's last-keyslot guard); cryptsetup/systemd-cryptenroll already fail clearly on their own once 32 slots are full, and 32 is not a boundary this story's ACs ask `domain` to enforce itself
- [ ] Task 3: Make the real `Fido2Backend::enroll_fido2_key` (`src/adapters/exec/mod.rs:813-863`) work when there is **no** transient bootstrap passphrase (AC: #1, #4)
  - [ ] Today this function unconditionally does `self.transient_passphrase.borrow_mut().take().ok_or_else(...)` and errors out if `None` — that's exactly the state a standalone enroll call is in, since `bootstrap_format_and_open` was never called this run. This is the second concrete gap this story must close, not just a documentation update
  - [ ] Branch on whether a transient passphrase is present: if `Some` (create's bootstrap-enroll path, unchanged), keep today's `TempKeyFile` + `--unlock-key-file=<path>` flow exactly as-is. If `None` (this story's standalone path), skip the temp key file entirely and run `systemd-cryptenroll --fido2-device=auto <path>` with **no** `--unlock-key-file` argument, fully inherited stdio (`.status()`, not `.output()` — same pattern `LuksBackend::open` already uses at `:772-779`) — this lets `systemd-cryptenroll`'s own interactive prompt reach the real terminal to authenticate against the tomb's *existing* enrolled FIDO2 key before it accepts the new one, satisfying AC #4's "existing-key/PIN prompt via passthrough stdio" requirement with zero new code for the prompt itself (it's `systemd-cryptenroll`'s native behavior)
  - [ ] Print a plain-language line before this call from the `cli` layer (Task 4) telling the user they'll need to touch *both* keys in sequence — `systemd-cryptenroll`'s own prompt text is otherwise untranslated jargon, same accepted limitation `run_unlock` already documents for its own prompt (`src/cli/main.rs:176-180`)
- [ ] Task 4: Wire the CLI (AC: #1)
  - [ ] Add an `Enroll` variant to `Commands` (`src/cli/main.rs`) with a positional `path` (`#[arg(allow_hyphen_values = true)]`, matching the convention Story 1.9 established for `create`/`unlock` — do not make this a `--path` flag) and a `--label <LABEL>` flag for `key_label` (required — a meaningless default like "backup" would undermine the whole point of a label, since AD-2 says labels get displayed at revoke-time listing to tell keys apart)
  - [ ] Add a `run_enroll` function mirroring `run_unlock`'s shape (`src/cli/main.rs:171-198`): preflight check, a plain-language print ("Touch your existing security key to authorize this, then touch the new key you're adding."), call `enroll::run`, translate any error via `ux::translate`, print a plain success line on `Ok(())`
  - [ ] Dispatch the new `Commands::Enroll` arm in `run()`
- [ ] Task 5: Update `cli/ux.rs`'s stale scope comments (AC: none directly — documentation correctness)
  - [ ] The module doc comment (`:1-9`) and `translate_adapter_failure`'s doc comment (`:63-69`) both currently say translation scope is "bounded to `create`'s and `unlock`'s own call graphs" — update both to also name `enroll`. No new `ENROLLMENT_MARKERS` entries are needed: enroll reuses the exact same `enroll_fido2_key`/`write_fido2_token_metadata` functions create's bootstrap path already exercises, so the existing marker strings (`"systemd-cryptenroll"`, `"token export"`, `"token JSON"`, `"systemd-fido2 token"`, etc.) already cover every failure this story's new code path can produce
  - [ ] `DomainError::LastKeyslotGuard`'s comment ("only `domain::workflows::revoke` (Story 2.2) produces this") stays accurate — `enroll` never produces it — leave unchanged
- [ ] Task 6: Tests (AC: #1, #2, #3, #4, #5)
  - [ ] `tests/unit/enroll.rs` (new; register `mod enroll;` in `tests/unit/main.rs`): fake-backed tests for `domain::workflows::enroll::run` mirroring `tests/unit/create.rs`'s style — preflight-failure short-circuits before any mutating call; happy path calls `enroll_fido2_key` exactly once with the given `key_label`; an `enroll_fido2_key` failure propagates as `AdapterFailure` untouched. `FakeFido2Backend::enroll_fido2_key` (`tests/unit/fakes.rs:206-218`) already supports this with no changes needed
  - [ ] `tests/unit/cli.rs`: add a help-text test for `enroll --help` listing `path` as positional and `--label` as a flag, following the exact pattern of Story 1.9's `unlock_help_lists_path_as_positional`/`create_file_help_lists_path_as_positional`
  - [ ] `tests/hardware/main.rs`: add a scenario that (a) creates a tomb (existing helper), (b) runs the real `enroll::run` against it with a *second* physical FIDO2 key and a distinct `key_label`, (c) asserts **both** keys can independently unlock it (AC #2) via two separate `unlock::run` calls, and (d) asserts via `LuksBackend::list_fido2_keyslots`/`luksDump` that there are now two live keyslots and that the *second* token's `key_label` is the one just enrolled — not the primary's — which is the concrete regression test for Task 1's token-picking fix. Run this alongside a device-backed variant if practical (AC #5), or at minimum confirm the domain code has no target-type branch to test around
  - [ ] Confirm `cargo build --lib`, `cargo test --lib --test unit` (`make test`), `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --check` all stay green, then run `make test-hardware` by hand (root required, two physical FIDO2 keys needed) before considering this story done

## Dev Notes

- **The two real gaps are Tasks 1 and 3, not the domain workflow itself.** `domain::workflows::enroll::run` (Task 2) is a thin, mostly-mechanical orchestration once Tasks 1 and 3 exist — most of this story's actual risk lives in `adapters::exec`'s existing `enroll_fido2_key`, which was built *only* for create's bootstrap case (Story 1.5) and silently assumes a transient passphrase always exists. Don't treat Task 2 as the hard part; it isn't.
- **No port/signature changes needed anywhere in `ports/`.** `Fido2Backend::enroll_fido2_key(&self, mapper: &MapperHandle, metadata: KeyMetadata)`'s existing signature already carries everything needed — only its *body* (the transient-passphrase branch) and `find_systemd_fido2_token_id`'s *selection logic* need to change. Do not widen `KeyMetadata`, `MapperHandle`, or any trait method "just in case" — this project's established discipline (Stories 1.6/1.7/1.9) is to change only what an actual AC requires.
- **Enroll never opens or mounts the tomb.** `systemd-cryptenroll`/`cryptsetup token export|import` all operate on the LUKS2 header at the container path directly — there is no `LuksBackend::open` or `FilesystemBackend::mount` call anywhere in this workflow. This is also *why* AC #5 (raw device vs. loop file parity) requires no code: there's no target-type branch to have gotten wrong in the first place.
- **`Filesystem` has exactly one variant (`Ext4`) today (AD-8).** Hardcode it in `KeyMetadata` rather than adding a CLI flag or a new read-back mechanism — there's no ambiguity to resolve yet. This mirrors how `bootstrap_format_and_open`'s own `filesystem` parameter is currently a no-op (`src/adapters/exec/mod.rs:536-537`) for the identical reason.
- **AD-3 compliance for Task 3's new branch:** the existing-key authentication prompt (touch/PIN) must never be captured by this process — inherited stdio via `.status()`, exactly like `LuksBackend::open`'s existing call (`src/adapters/exec/mod.rs:772-779`), not `.output()` (which would capture and buffer it). This is a separate subprocess invocation from any non-secret lookup (e.g. a future `fido2-token -L` call) — there is no such lookup in this story's scope today, but if one is added later it must stay a distinct call, never combined with the enrollment call itself.
- **Known, deferred, not this story's job to fix:**
  - `enroll_fido2_key` hardcodes `--fido2-device=auto` (deferred from Story 1.5's review) — with two FIDO2 devices plugged in simultaneously, which one `systemd-cryptenroll` treats as "existing" vs. "new" is ambiguous. The expected UX (matching `systemd-cryptenroll`'s own native prompts) is the user has only the relevant device plugged in at each prompt, swapping when asked. Don't attempt device selection in this story.
  - Device-backed real block devices are typically `root:disk 660` — per Story 1.6's deferred finding, this effectively requires running the whole `tomb-fido2` binary under `sudo` for any device-backed operation, undocumented in `--help` (retro action item #5, unassigned, explicitly "not blocking Epic 2"). This applies identically to a device-backed `enroll`; no new handling needed here, same accepted limitation as `create device`.
- **Previous story's own discipline (1.9) still applies:** keep this diff scoped to exactly what the ACs above need. Don't touch `close`/`resize`/`revoke` (still `todo!()` stubs, later stories), don't touch `mapping_name.rs`, and don't touch `unlock.rs`'s or `create.rs`'s own logic beyond what Task 1's shared token-finding fix requires.
- **Git intelligence:** baseline is `c005771` ("apply code review findings from PR #20", Story 1.9's final merged state, current `main` HEAD as of this story's creation) — no commits since. All file:line references above are accurate as of that commit.

### Project Structure Notes

- Modified: `src/domain/workflows/enroll.rs` (implement `run`, replacing `todo!()`), `src/adapters/exec/mod.rs` (`enroll_fido2_key`'s branch for no-transient-passphrase auth; `find_systemd_fido2_token_id` replaced by a before/after diff), `src/cli/main.rs` (new `Enroll` subcommand + `run_enroll`), `src/cli/ux.rs` (scope-comment updates only, no logic change), `tests/unit/main.rs` (register new test module), `tests/unit/cli.rs`, `tests/hardware/main.rs`.
- New: `tests/unit/enroll.rs`.
- Do **not** touch `src/domain/mapping_name.rs`, `src/ports/*` (no signature changes), `src/domain/workflows/{close,resize,revoke}.rs` (later stories, still `todo!()`), or `src/domain/keyslot_guard.rs` (not used by enroll — that's revoke's/create's job).
- Consistent with `ARCHITECTURE-SPINE.md`'s Structural Seed: no new module, port, or domain type needed — this story lives entirely inside `domain::workflows::enroll`, `adapters::exec`, and `cli`.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 2.1: Enroll an Additional FIDO2 Key] (full AC text this story's Acceptance Criteria section is drawn from verbatim)
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-2] (token metadata schema, per-key label/credential_id/created_at/filesystem, same `systemd-fido2` token object)
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-3] (secret material never enters tomb-fido2's own process; inherited stdio requirement)
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-5] (last-keyslot guard — confirms revoke's job, not enroll's, per Dev Notes above)
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-12] (deterministic mapping name, no registry — why AC #5 needs no branch)
- [Source: _bmad-output/implementation-artifacts/epic-1-retro-2026-07-24.md] (Epic 2 Preview — confirms enroll/revoke primitives were hardware-verified during Epic 1; sudo/device-permission watch item)
- [Source: _bmad-output/implementation-artifacts/deferred-work.md] (1-5's `--fido2-device=auto` limitation; 1-6's device-`sudo` gap — both apply unchanged to this story)
- [Source: src/adapters/exec/mod.rs:377-388] (`find_systemd_fido2_token_id` — the function Task 1 must fix)
- [Source: src/adapters/exec/mod.rs:390-470] (`write_fido2_token_metadata` — consumes the token id Task 1 must correctly identify)
- [Source: src/adapters/exec/mod.rs:813-863] (`enroll_fido2_key`'s real implementation — the function Task 3 must branch)
- [Source: src/adapters/exec/mod.rs:764-792] (`LuksBackend::open` — the inherited-stdio pattern Task 3's new branch should mirror)
- [Source: src/domain/workflows/create.rs:127-150] (`finish_provisioning` — the only existing caller of `enroll_fido2_key`, shows the bootstrap-passphrase-dependent call shape Task 3 must not break)
- [Source: src/domain/workflows/unlock.rs] (shape/style precedent for a thin `domain::workflows::*::run` function, incl. the "unused port parameter" doc-comment convention)
- [Source: src/cli/main.rs:171-198] (`run_unlock` — the CLI wrapper shape/plain-language-print pattern `run_enroll` should mirror)
- [Source: src/cli/ux.rs:1-9,63-69] (stale scope comments Task 5 updates)
- [Source: tests/unit/fakes.rs:167-219] (`FakeFido2Backend` — already sufficient for Task 6's unit tests, no changes needed)
- [Source: _bmad-output/implementation-artifacts/1-9-mount-ux-and-ownership-hardening.md] (previous story — positional-path convention, help-text test pattern, hardware-test-only-proves-real-adapter-behavior precedent)

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

### File List
