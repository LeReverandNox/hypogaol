---
baseline_commit: fd6b5cf9b843e06dd133b90dab29b52f85c4476b
---

# Story 3.2: Grow an Existing Tomb's Capacity

Status: in-progress

## Story

As a user,
I want to grow an existing tomb's volume and filesystem to a larger size,
so that I can increase my storage without recreating the tomb or re-enrolling any FIDO2 keys.

## Acceptance Criteria

1. **Given** an existing tomb and a new size larger than its current size **When** I run resize **Then** for a file-backed tomb the backing file is grown first (the same `set_backing_file_size` primitive create uses), then the LUKS2 mapping is resized, then the filesystem is grown — in that order.
2. **Given** a raw device/partition target **When** I run resize **Then** the tool does not resize the partition table — it errors clearly if the partition isn't already large enough.
3. **Given** a requested size smaller than the current size **When** I run resize **Then** the tool rejects the request before calling any adapter — resize is grow-only.
4. **Given** resize completes **When** I check afterward **Then** the volume's usable capacity reflects the new size, all previously enrolled FIDO2 keys still unlock it, and no existing data is lost.
5. **Given** the filesystem type needed for growfs **When** resize runs **Then** it reads it from the `filesystem` token field written at create time — never re-asked of the user or sniffed via `blkid`.

## Tasks / Subtasks

- [x] **Task 0 (spike, do this FIRST — de-risks everything below): confirm `cryptsetup resize` works against a FIDO2-token-opened mapping** (AC: #1, #4)
  - [x] **Why this matters:** `create`'s own `bootstrap_format_and_open` (`src/adapters/exec/mod.rs:793-914`) already had to work around a real hardware surprise: `cryptsetup resize` normally re-authenticates via the LUKS2 kernel keyring, but that keyring lookup turned out to be scoped such that two separate `sudo cryptsetup` invocations in the same workflow couldn't see each other's key — `resize` fell back to an interactive passphrase prompt against unattached stdin ("Nothing to read on input."), confirmed empirically on real hardware. Create sidesteps this only because it still holds the transient plaintext bootstrap passphrase in scope and pipes it via `--key-file -`.
  - [x] This story's `resize` workflow has **no passphrase available at all** — its `LuksBackend::open` call authenticates via the enrolled FIDO2 token (`--token-only`, see `src/adapters/exec/mod.rs:1052` / `unlock.rs`), which never surfaces a plaintext key to `adapters::exec` (AD-3). Whether a subsequent `cryptsetup resize <name>` (no `--key-file`) in the **same process**, right after that FIDO2-token `open`, can reuse the kernel keyring entry the token-based open just populated is **unconfirmed** — do not assume either outcome.
  - [x] Spike: on real hardware, `luksOpen --token-only` (or equivalent) a test LUKS2 volume, then immediately run `cryptsetup resize <name>` in the same process/session with no `--key-file`. If it succeeds without re-prompting, `LuksBackend::resize`'s implementation is simple (just `cryptsetup resize <name>`, no size argument needed — the header's segment stays `"dynamic"`, per the empirical finding at `src/adapters/exec/mod.rs:852-861`, and recomputes from the now-larger backing storage). If it does NOT succeed non-interactively, this is a real blocker: FIDO2-token opens don't hand you a passphrase to pipe, so there is no equivalent workaround to create's — flag to the user/architect (Winston) before proceeding with a workaround guess (mirrors the AD-2 "unconfirmed, spike first" precedent and the Story 2.1 architect-consultation precedent, both already established in this codebase).
  - [x] Document the spike's outcome in this story's Dev Notes/Completion Notes before writing the real implementation, the same way prior stories recorded their hardware-confirmed quirks inline.

- [x] Task 1: Add `LuksBackend::resize` port method (AC: #1, #4)
  - [x] Add `fn resize(&self, mapper: &MapperHandle) -> Result<(), DomainError>` to `src/ports/luks_backend.rs`. No explicit size parameter — per Task 0's finding, the header's segment sizing is `"dynamic"` (confirmed at create time) and recomputes from the backing file/device's actual current size at resize time, as long as that backing storage was already grown (Task 1 of the domain workflow, see Task 3 below) before this call runs.
  - [x] Implement in `ExecAdapter`: `cryptsetup resize <name>` against the already-open mapping (subject to Task 0's spike outcome — if the keyring doesn't carry over, this implementation needs a different mechanism entirely; do not guess here).
  - [x] Add a live "current provisioned size" query the workflow needs for AC #3's grow-only check. Reuse `FilesystemBackend::device_capacity` (already a generic `blockdev --getsize64 <path>` call — it works unmodified against `/dev/mapper/<name>` once the mapping is open, no port change needed there) rather than inventing a new method.

- [x] Task 2: Add `LuksBackend::read_filesystem` port method (AC: #5)
  - [x] Add `fn read_filesystem(&self, path: &Path) -> Result<Filesystem, DomainError>` to `src/ports/luks_backend.rs` — reads the `filesystem` field off the `systemd-fido2` token (written once by `create`'s `write_fido2_token_metadata`, `src/adapters/exec/mod.rs:661-744`; AD-2). Reuse the existing private `dump_json_metadata`/`tokens_object` helpers already in `src/adapters/exec/mod.rs` (used by `list_fido2_keyslots`, `enrolled_key_labels`) — do not re-implement JSON parsing.
  - [x] Only one `Filesystem` variant exists today (`Ext4`) — parse the stored string back to the enum, returning `DomainError::AdapterFailure` for any unrecognized value (defensive; should never happen given `write_fido2_token_metadata` is the only writer).
  - [x] This can run either before or after `luks.open` — it reads header/token state, not the live mapping, so it does not require the mapping to be open. Prefer running it early (alongside the AC #3 grow-only check) since a read failure here should abort before anything is touched.

- [x] Task 3: Add `FilesystemBackend::growfs` port method (AC: #1, #4)
  - [x] Add `fn growfs(&self, mapper: &MapperHandle, fs: Filesystem) -> Result<(), DomainError>` to `src/ports/filesystem_backend.rs`, mirroring `mkfs`'s shape (match on `fs`, v1 `Ext4`-only per AD-8).
  - [x] Implement in `ExecAdapter`: `resize2fs <device_node>` with no explicit size (grows to fill the now-larger mapping) — `privileged("resize2fs")`. `resize2fs` is already in `check_prerequisites`'s binary list (`src/adapters/exec/mod.rs:1245`, added ahead of this story), so no preflight change needed.
  - [x] `resize2fs` supports online (mounted) growth for ext4, but this workflow does not mount the filesystem — it operates directly on `mapper.device_node()` while the tomb is unmounted (see Task 4's workflow shape). Confirm this works unmounted on real hardware in Task 8's hardware test (ext4 online-resize is well-supported in both mounted and unmounted states, but verify rather than assume).

- [x] Task 4: Extend `FilesystemBackend::set_backing_file_size` to also grow an existing file (AC: #1)
  - [x] **This is a required behavior change, not new code** — AD-10's own architecture text is explicit that resize reuses "the same `set_backing_file_size` primitive create uses" (confirmed in `ARCHITECTURE-SPINE.md`'s file-structure table: `set_backing_file_size(shared by create AD-9 and resize AD-10)`). The current implementation (`src/adapters/exec/mod.rs:1294-1315`) uses `OpenOptions::new().create_new(true)`, which **fails if the file already exists** — the opposite of what resize needs (grow an existing file, refuse if it's missing).
  - [x] Change the implementation to branch: if `path` does not exist, keep today's `create_new(true)` behavior unchanged (create's call site never hits the other branch, since `create::run` already checks `path_exists` first). If `path` already exists, open it with `.write(true)` (no `create_new`, no `truncate`) and call `set_len(size)` to grow it — but **first check `std::fs::symlink_metadata(path)` confirms a regular file, not a symlink**, refusing with `DomainError::AdapterFailure` otherwise. This preserves the existing symlink-clobber protection the doc comment on this method already calls out ("a plain `File::create` would instead follow a symlink and silently truncate/write through it") for the *grow* path too, not just the *create* path.
  - [x] Do not add a strict `size > current_len` guard inside this method itself — trust the domain-level AC #3 check (Task 5) to have already rejected a shrink request before this runs, consistent with this codebase's established "trust internal callers, validate only at the boundary" convention (`create.rs`'s own `MIN_TOMB_SIZE_BYTES` check is the one exception, and it's there because the CLI's own validation isn't trusted as the sole gate either — see `parse_size` vs `create::run`'s own re-check).

- [x] Task 5: Implement `domain::workflows::resize::run` (AC: #1, #2, #3, #4, #5)
  - [x] Replace the `todo!()` stub in `src/domain/workflows/resize.rs`. Current stub signature is `run(luks, fido2, fs)` with **no `path`/`new_size` parameters** — extend to `run(path: &Path, new_size: u64, luks: &dyn LuksBackend, fido2: &dyn Fido2Backend, fs: &dyn FilesystemBackend) -> Result<(), DomainError>`, mirroring `unlock::run`/`close::run`'s existing `path`-first shape.
  - [x] Body, in order:
    1. `preflight::check(luks, fido2, fs)?` (AD-4, AC applies uniformly like every other workflow).
    2. `let name = mapping_name::mapping_name(path)?` (AD-12).
    3. Determine whether `path` is a raw device/partition or a file-backed target. **This codebase has no existing helper for this distinction** (`create`'s two modes are chosen explicitly via the CLI's separate `Create::File`/`Create::Device` subcommands, not inferred). For `resize`, there's only one `path` argument for both target types (matching `unlock`/`close`/`enroll`/`revoke`'s existing "identical command works unmodified" convention, AC's own device-vs-file ACs #1/#2 for this story). Recommend inferring via a filesystem stat: a raw block device path (`/dev/...`) vs a regular file — check `std::fs::metadata(path)?.file_type()` for `is_block_device()` (via `std::os::unix::fs::FileTypeExt`) or similar, rather than sniffing the path string. This needs a small new domain- or adapter-level helper; keep it minimal, don't overbuild a general "target type" abstraction beyond what this one branch needs.
    4. Read the current provisioned size and reject the request before mutating anything if `new_size` is not strictly greater (AC #3, AC #2). See the **Open Design Question** in Dev Notes below for exactly how "current size" should be read — this is the one piece of this story that isn't fully pinned down by the architecture spine and needs a deliberate call, not a guess.
    5. `let mapper = luks.open(path, &name)?` — same FIDO2-token open `unlock::run` uses (this workflow needs the mapping active for `resize`/`growfs` to operate on).
    6. **Rollback discipline (mirrors `unlock::run`'s existing pattern, `src/domain/workflows/unlock.rs:24-33`):** once `open` succeeds, every subsequent failure must `luks.close(&mapper)` before returning the error, or the mapping leaks open indefinitely. There is no partial "undo" of a successful `set_backing_file_size`/`resize`/`growfs` step — like `create`'s own bootstrap sequence, if a later step fails after an earlier one succeeded, just close and propagate; don't attempt to shrink back.
    7. If file-backed: `fs.set_backing_file_size(path, new_size)?` (Task 4's grown behavior) — **must happen while the mapping is already open**, per AD-10's ordering (backing file grows, then the LUKS mapping is resized), so this call comes after step 5, not before.
    8. `luks.resize(&mapper)?` (Task 1).
    9. `let filesystem = luks.read_filesystem(path)?` (Task 2) — can be read any time before this point too; placing it here keeps the happy path linear, but moving it earlier (e.g. right after step 2) is also fine if that reads cleaner once the rollback plumbing is in place.
    10. `fs.growfs(&mapper, filesystem)?` (Task 3).
    11. `luks.close(&mapper)` — always close on the way out, success or failure (same discipline as `create::run`'s `bootstrap_and_provision`, `src/domain/workflows/create.rs:128-135`), returning whichever `Result` reflects the actual outcome.
  - [x] No target-type branching beyond step 3's file-vs-device dispatch for the size-growth mechanism itself — everything else (preflight, mapping-name derivation, open, resize, growfs, close) is identical for both, consistent with every other workflow's "identical command works unmodified" convention (AC #2's device wording, mirrored from Stories 1.7/2.1/2.2/3.1).

- [ ] Task 6: Wire the `resize` CLI subcommand (AC: #1, #2, #3)
  - [ ] Add a `Resize { path: PathBuf, size: u64 }` variant to `Commands` in `src/cli/main.rs`, same `#[arg(allow_hyphen_values = true)]` convention on `path`, and reuse the existing `parse_size` value-parser on `size` (already enforces `MIN_TOMB_SIZE_BYTES` and K/M/G/T suffixes — no new parsing logic needed). Flag name for the size argument: reuse the same `--size` convention `create`'s subcommands use, but note `resize` needs it as the *new total* size, not a delta — make that explicit in the flag's help text (e.g. "New total size for the tomb (e.g. 20G) — must be larger than its current size").
  - [ ] Add a `run_resize(path: PathBuf, new_size: u64)` function mirroring `run_close`'s shape (`src/cli/main.rs:397-414`): preflight check first, a plain-language intro line (FR5/NFR3) noting a FIDO2 touch will be needed (mirrors `run_unlock`'s "Touch your security key now..." line, since `resize` also calls `luks.open`), then call `resize::run`, success/error reporting via `ux::translate`. No confirmation prompt — like `close`, growing is non-destructive (existing data is preserved, AC #4), so it doesn't fit the pattern that justifies `create`'s wipe warning or `revoke`'s irreversible-key warning.
  - [ ] Wire the new match arm in `run()`.

- [ ] Task 7: Plain-language translation for resize's failures (AC: none directly — CAP-5/NFR3 quality bar)
  - [ ] **Marker-bleed guard — check this before merge, not after (explicitly flagged by the Epic 2 retro as a recurring bug class, 3 real bugs so far across Epics 1–3):** add dedicated branches in `translate_adapter_failure` (`src/cli/ux.rs`) for `resize`'s own new failure shapes — `cryptsetup resize` failures and `resize2fs` failures — **ordered before** the generic `cryptsetup` bucket (line ~149) and the generic `mount`/`mkfs` bucket (line ~180) respectively, the same way `close`'s `"cryptsetup close"`/`"umount"`/`"findmnt"` branches were ordered ahead of those same generic buckets in Story 3.1's review findings. Watch specifically for: a plain `"resize"` failure string being caught by nothing appropriate and falling through to the generic `cryptsetup` bucket's touch/PIN-entry framing (nonsensical for a resize failure, exactly the class of bug Story 3.1's review caught for `close`); and `"resize2fs"` containing no `"mount"`/`"mkfs"` substring today, so it would currently fall all the way to the unhelpful generic fallback unless a dedicated branch is added.
  - [ ] Give a clear, distinct message for the too-small-partition case (AC #2) and the grow-only rejection case (AC #3) if those surface as `DomainError::AdapterFailure` strings rather than structured variants — decide during Task 5/8 whether these need their own `DomainError` variants (similar to `DeviceSizeExceedsCapacity`/`DeviceTooSmall`) instead of falling through the generic adapter-failure string-matching path. Structured variants are preferable when the information (requested size, actual capacity/current size) is known at the point of failure and worth surfacing precisely to the user, matching the existing precedent set by `create`'s own device-sizing errors.

- [ ] Task 8: Unit tests
  - [ ] `tests/unit/fakes.rs`: add `resize` and `read_filesystem` to `FakeLuksBackend`'s `LuksBackend` impl (log the call, support `with_failure_at`, a settable return value for `read_filesystem`, same pattern as `has_luks2_header`/`open`). Add `growfs` to `FakeFilesystemBackend`'s `FilesystemBackend` impl (log the call, support `with_failure_at("growfs")`). Extend `set_backing_file_size`'s fake behavior if the test needs to distinguish grow-vs-create calls (optional — the existing fake just logs and can fail-if, which is likely sufficient).
  - [ ] `tests/unit/workflows.rs`: update the existing stub test `resize_run_stops_at_preflight_before_reaching_its_own_todo` for the new `(path, new_size, ...)` signature (it will otherwise fail to compile once Task 5 lands) — rename it to match Story 3.1's convention (`..._before_touching_any_port`, not `..._before_reaching_its_own_todo`, since the `todo!()` will no longer exist).
  - [ ] Add `tests/unit/resize.rs` (register `mod resize;` in `tests/unit/main.rs`), covering at minimum: happy path for a file-backed target (call order `["set_backing_file_size", "resize", "growfs"]` after `open`, then `close`; `mapping_name` consistency across `open`/`resize`/`close`); happy path for a device-backed target (no `set_backing_file_size` call, `resize`/`growfs` still run); the grow-only rejection (`new_size` <= current — assert no port call happens beyond whatever the chosen current-size-check design requires, per the Open Design Question below); the too-small-partition rejection for a device-backed target; a mid-flow failure (e.g. `growfs` fails after `resize` succeeded) asserting `close` is still called (rollback discipline, mirroring `close.rs`'s own `luks_close_failure_after_a_successful_umount_still_propagates_as_an_error` precedent from Story 3.1).

- [ ] Task 9: Hardware tests (manual-only, `make test-hardware`, AD-7 — not run in default CI)
  - [ ] Add scenarios in `tests/hardware/main.rs`: growing a file-backed tomb (create small, write a marker file, resize larger, unlock, confirm the marker file and previously enrolled key(s) still work, confirm new capacity is usable); growing a device-backed tomb that has headroom from creation (create with `size` smaller than the loop device's capacity, then resize into that headroom); the too-small-partition error path (device-backed, request a size larger than the raw device actually has); the grow-only rejection path (request a smaller size, confirm no data/state is touched).
  - [ ] This is also where Task 0's spike observation gets a permanent regression test, if the resolution requires anything non-obvious (e.g. an explicit re-authentication step) — don't leave a hardware-confirmed quirk undocumented in code, matching this codebase's established practice (see `bootstrap_format_and_open`'s inline comments).

## Dev Notes

### Task 0 spike finding (confirmed on real hardware, 2026-07-26)

`cryptsetup resize <name>` with no `--key-file` does **not** reuse the kernel keyring entry a preceding `cryptsetup open --token-only <path> <name>` populated, even though both ran against the same mapping in quick succession — it fell back to an interactive passphrase prompt ("Enter passphrase for..."), confirmed empirically. This is the same keyring-scoping limitation `bootstrap_format_and_open` already worked around for the passphrase case (`src/adapters/exec/mod.rs:863-872`), now confirmed to also apply to a FIDO2-token-based open.

However, `resize` accepts the same `--token-only` flag `open` uses: `cryptsetup resize --token-only <name>` re-authenticates via the enrolled `systemd-fido2` token (re-touch + PIN, same interaction shape as `open`) and succeeds non-interactively w.r.t. any passphrase — confirmed working end-to-end on real hardware (`Enter token PIN:` → touch → `Key slot 1 unlocked. Command successful.`). **This is the mechanism `LuksBackend::resize` must use** — `privileged("cryptsetup").args(["resize", "--token-only"]).arg(&mapper.name)`, invoked with inherited stdio (`.status()`, not `.output()`) so the FIDO2 PIN/touch prompt reaches the real terminal, mirroring `LuksBackend::open`'s existing pattern (`src/adapters/exec/mod.rs:1051-1058`). No `--device-size` argument needed — the header's segment stays `"dynamic"` and recomputes from the now-larger backing storage once `set_backing_file_size`/the raw device has actually grown.

### Open Design Question: how does `resize` read "current size" for the grow-only check (AC #3)?

This is the one piece of this story the architecture spine states as a rule but doesn't fully resolve at the implementation level — work through it deliberately, don't guess:

- **AD-10's text:** "`domain::workflows::resize` reads the current mapping/filesystem size live (never cached) and rejects any request smaller than that size **before calling any adapter**." For a raw device/partition, "current size" is unambiguous — it's whatever `device_capacity(path)` already reports (a pure, adapter-based but non-mutating query), and AC #2's "too small" case is exactly this comparison.
- **The actual ambiguity is device-backed tombs that used Story 1.6's headroom feature** (create with a `size` smaller than the device's full capacity, "leaving the remaining capacity free for a later resize/grow" — Story 1.6 AC #2). For such a tomb, `device_capacity(path)` returns the *raw partition's total capacity*, which is **not** the same as the tomb's currently-provisioned/mkfs'd size — the whole point of leaving headroom is that those two numbers differ. If "current size" for the AC #3 grow-only check is read as `device_capacity(path)`, a genuine legitimate grow-into-headroom request (new size < raw device capacity, but > the smaller current provisioned size) would be **wrongly rejected as a shrink** — this would silently defeat Story 1.6's entire reason for supporting a smaller-than-capacity device create.
- **Why this can't be resolved with a single pre-adapter-call check:** the currently-provisioned size for a device-backed tomb is not stored anywhere (AD-2 has no persisted size field, deliberately) and the LUKS2 header's own segment-size metadata stays the literal string `"dynamic"` even for a tomb that was constrained smaller at create time (confirmed on real hardware — see `src/adapters/exec/mod.rs:852-861`) — reading the header at rest cannot distinguish "this tomb currently uses less than the raw device's capacity" from "this tomb uses all of it." The only way to learn the true currently-provisioned size is to query the *active* mapping (`device_capacity` against `/dev/mapper/<name>` once opened) or the ext4 superblock's own block count (which lives inside the encrypted payload, so it's equally unreachable without opening first).
- **Recommended resolution — a two-tier check, not a single one:**
  1. **Before opening anything** (true zero-adapter-call tier, satisfies AC #2's letter directly and AC #3's letter for the common/obvious cases): for a **file-backed** target, read the backing file's current length via a plain `std::fs::metadata(path)?.len()` (no port call at all — this is a pure stdlib call, arguably not "an adapter" in the AD-10 sense) and reject if `new_size` isn't larger. For a **device-backed** target, compare `new_size` against `fs.device_capacity(path)` and reject as "too small" if it exceeds raw capacity (AC #2) — this alone does *not* catch a device-backed shrink-into-still-valid-headroom-range request.
  2. **After opening** (unavoidable — `resize`/`growfs` need the mapping active regardless): re-derive the true current provisioned size via `fs.device_capacity(&mapper.device_node())` and re-check `new_size` against it, closing the mapping and returning an error before calling `resize`/`growfs` if the request doesn't actually grow anything. This happens after one adapter call (`open`, which is an authentication/read operation, not a mutation) but strictly before any *mutating* call (`set_backing_file_size`/`resize`/`growfs`) — arguably satisfies AD-10's real intent ("never reaches a mutating adapter on a shrink request") even though it doesn't satisfy a maximally literal zero-calls reading of "before calling any adapter."
- **If this reasoning doesn't hold up under implementation (e.g. `device_capacity` behaves unexpectedly against `/dev/mapper/<name>`, or the two-tier design feels like it's fighting the architecture rather than fitting it), stop and flag it** — this is exactly the kind of question Story 2.1 resolved via a direct architect consultation (documented in that story's own Dev Notes as "Architect consultation resolved") rather than the dev agent guessing and risking a rework cycle after review.

### Architecture requirements (binding, from ARCHITECTURE-SPINE.md AD-10)

- Ordering is exact and non-negotiable, mirroring AD-8's precedent for `close`: file-backed grows the backing file **first**, then the LUKS2 mapping is resized, then the filesystem is grown — reversing any of these risks growing a filesystem onto space the LUKS mapping doesn't have yet (data corruption/truncation risk the spine explicitly calls out).
- Device-backed: **never** touches the partition table. Errors clearly if the partition isn't already large enough for the requested size — out of scope, not deferred-with-a-workaround.
- `resize` is grow-only, full stop — this directly implements a stated SPEC non-goal ("Shrinking an existing tomb — resize is grow-only for v1").
- `domain::preflight::check` runs as `resize`'s first statement, identical to every other workflow (AD-4) — no lighter gate.
- No side-channel state (AD-2): nothing about the current provisioned size is ever written anywhere for `resize` to read back later — every check is a live query, by design (this is *why* the Open Design Question above exists — the whole point of AD-2 is that there's nothing cached to read).
- Deterministic mapping name via the single shared `mapping_name` helper (AD-12) — same call as every other workflow, no reimplementation.

### Prior-story precedent to reuse, not reinvent

- **Rollback-on-mid-flow-failure discipline:** `unlock::run` closes a mapping it just opened if `mount` subsequently fails (`src/domain/workflows/unlock.rs:24-33`); `create`'s `bootstrap_and_provision` always calls `luks.close` on the way out regardless of success/failure (`src/domain/workflows/create.rs:128-135`). `resize` needs the same shape: once `luks.open` succeeds, every exit path must close the mapping.
- **Idempotent-retry consideration:** Story 3.1's review added idempotent-retry handling for `close` (a repeat call after a partial failure self-heals rather than getting stuck). Consider on implementation whether `resize` needs anything similar — e.g. what happens if `set_backing_file_size` grows the file but `resize` then fails: is a repeat `resize` call with the same `new_size` safe (yes, `set_backing_file_size`'s grow branch from Task 4 is idempotent — growing an already-larger-or-equal file to the same size is a no-op via `set_len`)? Confirm this holds and note it in Completion Notes if relevant, rather than leaving it as an unstated assumption.
- **Marker-bleed guard:** called out as its own task (Task 7) above — this is the single most repeated lesson across Epics 1–3 in this codebase (3 real bugs from this exact pattern per the Epic 2 retro and Story 3.1's own review findings). Do not skip checking it before merge.

### Project Structure Notes

- Touches (existing files, all UPDATE not NEW except the new test file):
  - `src/ports/luks_backend.rs` — add `resize` and `read_filesystem` to the trait.
  - `src/ports/filesystem_backend.rs` — add `growfs` to the trait.
  - `src/adapters/exec/mod.rs` — implement `resize`/`read_filesystem`/`growfs`; change `set_backing_file_size`'s behavior to support growing an existing file (Task 4).
  - `src/domain/workflows/resize.rs` — replace stub with real implementation, new `path`/`new_size` parameters.
  - `src/domain/errors.rs` — possibly new variant(s) for the too-small-partition/grow-only-rejection cases (Task 7's call).
  - `src/cli/main.rs` — new `Resize` subcommand + `run_resize`.
  - `src/cli/ux.rs` — new marker-ordered translation branch(es) for resize/growfs failures.
  - `tests/unit/fakes.rs`, `tests/unit/workflows.rs`, `tests/unit/main.rs` — updates.
  - `tests/hardware/main.rs` — new scenarios.
  - New file: `tests/unit/resize.rs`.
- No new modules, no structural changes to the hexagonal layering — `domain::workflows::resize` already exists as a stub, `LuksBackend`/`FilesystemBackend` already exist as ports. This story only extends existing traits/adapters, consistent with AD-8's "adding a filesystem/port capability later is additive" design intent (though here it's adding *methods* to existing ports, not a new `Filesystem` enum arm — still additive, no signature-breaking change to existing methods other than `set_backing_file_size`'s behavior, which is the one deliberate, spine-mandated exception, AD-10).
- No detected conflicts or variances from the structural seed beyond the `set_backing_file_size` behavior change flagged in Task 4 — that change is required by the architecture spine's own file-structure table (it explicitly lists this method as "shared by create AD-9 and resize AD-10"), not a deviation from it.

### Testing standard (AD-7)

Unit tests against the shared fakes in `tests/unit/fakes.rs`, run in default CI. Hardware-gated scenarios in `tests/hardware/main.rs`, manual-only via `make test-hardware`, never in CI.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 3.2: Grow an Existing Tomb's Capacity]
- [Source: ARCHITECTURE-SPINE.md#AD-10 — Resize ordering and grow-only enforcement]
- [Source: ARCHITECTURE-SPINE.md#AD-2 — No side-channel state; `filesystem` token field written by create, read by resize]
- [Source: ARCHITECTURE-SPINE.md#AD-4 — Mandatory shared preflight gate]
- [Source: ARCHITECTURE-SPINE.md#AD-8 — FilesystemBackend port, mkfs/growfs/mount/umount methods]
- [Source: ARCHITECTURE-SPINE.md#AD-12 — Deterministic mapping name, no registry]
- [Source: ARCHITECTURE-SPINE.md file-structure table — `set_backing_file_size(shared by create AD-9 and resize AD-10)`, confirming Task 4's required behavior change is spec-mandated]
- [Source: src/adapters/exec/mod.rs:793-914 — `bootstrap_format_and_open`'s empirical findings on `cryptsetup resize`'s keyring/key-file behavior and the "dynamic" segment-size quirk; essential prior context for Task 0's spike]
- [Source: _bmad-output/implementation-artifacts/3-1-close-an-unlocked-tomb.md — sibling workflow shape (rollback discipline, marker-bleed guard, idempotent-retry precedent)]
- [Source: _bmad-output/implementation-artifacts/epic-2-retro-2026-07-26.md — marker-bleed bug-class warning, action item on checking it before every merge that reuses `adapters::exec` string-matched errors]
- [Source: src/domain/workflows/unlock.rs, src/domain/workflows/create.rs — sibling workflow shape/conventions for open/rollback and bootstrap/close-always patterns]

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

- Task 0 spike (real hardware, 2026-07-26): `cryptsetup resize <name>` alone does not reuse the keyring across a preceding `--token-only` open (falls back to an interactive passphrase prompt, confirmed). `cryptsetup resize --token-only <name>` does work — re-authenticates via the FIDO2 token (touch + PIN) and completes non-interactively w.r.t. passphrase. `LuksBackend::resize` will use this flag. See Dev Notes for full detail.
- Task 5: implemented the two-tier grow-only check exactly as reasoned through in the Open Design Question — tier 1 (file: plain `std::fs::metadata` stat, no adapter call; device: `fs.device_capacity(path)` against the raw path) runs before `luks.open`; tier 2 (`fs.device_capacity(&mapper.device_node())`, re-checked against `new_size`) runs immediately after `open` but before any mutating call, closing the mapping and returning `ResizeMustGrow` if it doesn't actually grow. Reused `DomainError::DeviceSizeExceedsCapacity` for the too-small-partition case (AC #2) rather than adding a duplicate variant — it's already a structured, non-string-matched variant with the right shape. Added `DomainError::ResizeMustGrow` for the grow-only rejection (AC #3) with its own `ux::translate` arm. Pulled forward the minimal `tests/unit/fakes.rs` support (resize/read_filesystem/growfs on the fakes) needed to keep the suite compiling once the stub signature changed, and fixed the pre-existing stub test's signature/name in `tests/unit/workflows.rs` (`resize_run_stops_at_preflight_before_touching_any_port`) — the dedicated `tests/unit/resize.rs` behavioral tests are still Task 8's own work.

### File List

- `src/ports/luks_backend.rs` — added `resize`/`read_filesystem` to `LuksBackend`.
- `src/ports/filesystem_backend.rs` — added `growfs` to `FilesystemBackend`.
- `src/adapters/exec/mod.rs` — implemented `resize`/`read_filesystem`/`growfs` in `ExecAdapter`; extended `set_backing_file_size` to grow an existing file; added unit tests.
- `src/domain/workflows/resize.rs` — replaced the `todo!()` stub with the full workflow (grow-only two-tier check, open/resize/growfs ordering, close-always rollback).
- `src/domain/errors.rs` — added `DomainError::ResizeMustGrow`.
- `src/cli/ux.rs` — added a `translate` arm for `ResizeMustGrow`.
- `tests/unit/fakes.rs` — added `resize`/`read_filesystem` to `FakeLuksBackend`, `growfs` to `FakeFilesystemBackend`.
- `tests/unit/workflows.rs` — updated/renamed the `resize::run` preflight stub test for the new signature.
