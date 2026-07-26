---
baseline_commit: 11723e9307775ffba214e822fc60b6318afa85bf
---

# Story 3.3: Unlock a Tomb Read-Only

Status: review

## Story

As a user,
I want to unlock and mount an existing tomb in read-only mode,
so that I can inspect its contents without risking any writes, at both the block-device and filesystem level.

## Acceptance Criteria

1. **Given** an existing tomb **When** I run unlock with the read-only flag **Then** a single `read_only` bool is passed to both the LUKS2 open call (`--readonly`) and the mount call (`-o ro`) in the same operation, **and** never one without the other.
2. **Given** a read-only unlock **When** mounted **Then** both the underlying mapper device and the mounted filesystem reject write attempts, including a later remount attempt.
3. **Given** the read-only `luksOpen` succeeds but the subsequent mount fails **When** that happens **Then** the tool closes the just-opened mapping before returning the error, **and** no dangling open mapper is left behind.
4. **Given** a normal (non-read-only) unlock **When** I run it **Then** it continues to allow writes as before.
5. **Given** an existing tomb backed by a raw device/partition instead of a loop-backed file **When** I run unlock with the read-only flag **Then** the identical command works unmodified — read-only unlock makes no branching decision based on target type.

## Tasks / Subtasks

- [x] Task 1: Add `read_only: bool` parameter to `LuksBackend::open` (AC #1, #2)
  - [x] In `src/ports/luks_backend.rs:44`, change `fn open(&self, path: &Path, name: &str) -> Result<MapperHandle, DomainError>` to `fn open(&self, path: &Path, name: &str, read_only: bool) -> Result<MapperHandle, DomainError>`. Update the doc comment to note the new parameter maps to `cryptsetup open --readonly` (AD-11).
  - [x] Implement in `ExecAdapter::open` (`src/adapters/exec/mod.rs:1035`): when `read_only` is `true`, add `--readonly` to the `cryptsetup open --token-only` args (order doesn't matter to cryptsetup, but keep `--token-only` first for consistency with the existing call). When `false`, behavior is byte-for-byte identical to today.

- [x] Task 2: Add `read_only: bool` parameter to `FilesystemBackend::mount` (AC #1, #2)
  - [x] In `src/ports/filesystem_backend.rs:62`, change `fn mount(&self, mapper: &MapperHandle) -> Result<PathBuf, DomainError>` to `fn mount(&self, mapper: &MapperHandle, read_only: bool) -> Result<PathBuf, DomainError>`. Update the doc comment to note `-o ro` and reference AD-11.
  - [x] Implement in `ExecAdapter::mount` (`src/adapters/exec/mod.rs:1562`): when `read_only` is `true`, add `-o ro` to the `mount` invocation at `src/adapters/exec/mod.rs:1622`.
  - [x] **Required design decision — read this before touching the chown/chmod block (`src/adapters/exec/mod.rs:1649-1694`):** that block runs `chown`/`chmod 0700` against the just-created mount point, which — once `mount` has succeeded — resolves to the mounted filesystem's own root inode (persisted inside the encrypted volume, not the transient empty directory `create_mount_point` made). Both calls need write access to the filesystem. A filesystem mounted `-o ro` refuses metadata writes, so running `chown`/`chmod` unconditionally after a read-only mount would itself fail (`EROFS`), turning every read-only unlock into a hard error. **Skip the `chown`/`chmod` block entirely when `read_only` is `true`** — return `Ok(mountpoint)` right after the `mount` call succeeds. Document inline why: ownership/permissions on the volume's root inode are whatever a prior *writable* unlock already persisted there (normal unlock chowns to the invoking user on every mount, so any tomb that has ever been unlocked normally already carries correct ownership); a tomb that has *never* been unlocked writably will show root-owned, `mkfs.ext4`-default-mode (`0755`) permissions on its first-ever read-only unlock — world-readable/traversable, so the invoking user can still read (just not chown/chmod it), which is an acceptable, documented limitation rather than a bug. Do not attempt to work around this with a temporary rw-mount-then-remount-ro sequence — that would violate AD-11's "single `read_only` bool, same call, never one without the other" rule and briefly leaves a writable window this story exists to prevent.

- [x] Task 3: Thread `read_only` through `domain::workflows::unlock::run` (AC #1, #3, #4, #5)
  - [x] In `src/domain/workflows/unlock.rs`, add a `read_only: bool` parameter (mirroring `resize::run`'s `new_size` position — right after `path`): `pub fn run(path: &Path, read_only: bool, luks: &dyn LuksBackend, fido2: &dyn Fido2Backend, fs: &dyn FilesystemBackend) -> Result<PathBuf, DomainError>`.
  - [x] Pass `read_only` to both `luks.open(path, &name, read_only)?` and `fs.mount(&mapper, read_only)?` — the same single bool into both calls, per AD-11's letter (AC #1).
  - [x] The existing rollback discipline (`src/domain/workflows/unlock.rs:24-33`: `luks.close(&mapper)` if `mount` fails) needs **no logic change** — it already runs on any `mount` error regardless of `read_only`, satisfying AC #3 unmodified. Do not special-case the read-only path here.
  - [x] No target-type branching (AC #5) — `unlock::run` already takes one `path` for both file- and device-backed targets and makes no decision based on which; adding `read_only` doesn't change that.

- [x] Task 4: Update `resize::run`'s call site for the new `open` signature (compile fix, no AC — `resize` always needs a writable mapping)
  - [x] `src/domain/workflows/resize.rs:74`: change `luks.open(path, &name)?` to `luks.open(path, &name, false)?`. Resize must grow the volume, so it always opens read-write.

- [x] Task 5: Wire the `--read-only` CLI flag (AC #1, #4, #5)
  - [x] In `src/cli/main.rs`, add a `read_only: bool` field to the `Unlock` variant (`src/cli/main.rs:36-40`) with `#[arg(long)]` — clap 4.6's derive gives a bare `bool` field `ArgAction::SetTrue` automatically, so no explicit `action = ...` is needed, mirroring how every other flag in this file is declared. Field name `read_only` maps to `--read-only` via clap's automatic kebab-case conversion (consistent with `--fido2-device` etc.). Give it a short help string, e.g. "Unlock read-only — refuses all writes at both the block-device and filesystem level".
  - [x] Update `run_unlock` (`src/cli/main.rs:300-317`) to accept `read_only: bool`, pass it through to `unlock::run(&path, read_only, &adapter, &adapter, &adapter)`, and adjust the two printed lines to mention read-only mode when set — e.g. intro line: `"Touch your security key now (you may also be asked for its PIN)."` gets a trailing `" Unlocking read-only — no changes will be saved."` when `read_only`; success line: `"Tomb unlocked (read-only) and mounted at {}."` vs the existing `"Tomb unlocked and mounted at {}."`. Keep both variants plain-language (FR5/NFR3) — no jargon about dm-crypt/`--readonly`/`-o ro`.
  - [x] Update the `Commands::Unlock { path } => run_unlock(path)` match arm (`src/cli/main.rs:495`) to `Commands::Unlock { path, read_only } => run_unlock(path, read_only)`.

- [x] Task 6: Marker-bleed check for any new failure text (AC: none directly — CAP-5/NFR3 quality bar; flagged repeatedly by the Epic 2/3 retros as the most-repeated bug class in this codebase)
  - [x] This story adds no new `AdapterFailure` string shapes beyond what already exists — `cryptsetup open --readonly` failures still contain `"cryptsetup"` (falls into the existing generic `cryptsetup` bucket in `src/cli/ux.rs:187`) and a failed read-only `mount -o ro` still contains `"mount"` (existing bucket at `src/cli/ux.rs:240`). Confirm this holds once Tasks 1–2 land (i.e. no new distinct error-message prefix was introduced) rather than assuming it — if the `cryptsetup`/`mount` binaries produce a materially different error string for the `--readonly`/`-o ro` case that wouldn't already match those buckets, add a dedicated branch ordered before them, following the exact precedent of every prior story's marker-bleed fix.
  - [x] Do not add a new `DomainError` variant for this story — no new structured error information (sizes, paths) is available at any new failure point that existing variants don't already cover; every failure here is a plain adapter/subprocess failure, same shape as the existing (non-read-only) `unlock` path.

- [x] Task 7: Unit tests (AC #1, #2, #3, #4, #5)
  - [x] `tests/unit/fakes.rs`: update `FakeLuksBackend::open` (`tests/unit/fakes.rs:224`) to the new `(path, name, read_only)` signature — log the call as today, and additionally record the `read_only` value passed (extend `last_open`'s tuple from `(PathBuf, String)` to `(PathBuf, String, bool)`, updating `last_open()`'s return type and the one existing caller in `tests/unit/unlock.rs:47` accordingly). Update `FakeFilesystemBackend::mount` (`tests/unit/fakes.rs:474`) to the new `(mapper, read_only)` signature — log the call, and record the `read_only` value passed (add a `last_mount_read_only: RefCell<Option<bool>>` field plus an accessor, same pattern as `last_open`).
  - [x] `tests/unit/resize.rs`: no test changes expected beyond compiling against the new `open` signature — resize's fakes-based tests assert on the `log` call sequence (`"open"`, etc.), not on `open`'s arguments, so they should be unaffected once `resize::run`'s own call site (Task 4) passes `false`. Run the suite and fix compile errors only if any test does inspect `last_open()`'s new 3-tuple shape.
  - [x] `tests/unit/unlock.rs`: update both existing tests (`happy_path_opens_and_mounts_using_the_shared_mapping_name`, `mount_failure_closes_the_just_opened_mapping`) to call `unlock::run(&fixture.0, false, &luks, &fido2, &fs)` (explicit non-read-only, preserving today's behavior coverage). Add new tests:
    - `read_only_true_is_passed_to_both_open_and_mount`: call `unlock::run(&fixture.0, true, &luks, &fido2, &fs)`, assert it succeeds, and assert both `luks.last_open()`'s bool component and `fs`'s new `last_mount_read_only()` are `Some(true)`.
    - `read_only_false_is_passed_to_both_open_and_mount`: same shape asserting `Some(false)` for a normal unlock, locking in AC #4 at the fakes level (distinct from the two renamed tests above, which only check the happy-path return value/log, not the propagated bool).
    - `read_only_mount_failure_still_closes_the_just_opened_mapping`: same shape as the existing `mount_failure_closes_the_just_opened_mapping` but with `read_only: true`, proving AC #3 holds identically in the read-only path (the rollback code is unconditional, so this should pass with no production-code change beyond Tasks 1–3 — write it anyway as a regression guard, don't assume).
  - [x] `tests/unit/cli.rs`: add `unlock_help_lists_read_only_flag` asserting `help_text(&["tomb-fido2", "unlock", "--help"])` contains `"--read-only"`.

- [x] Task 8: Hardware tests (manual-only, `make test-hardware`, AD-7 — not run in default CI)
  - [x] Add scenarios in `tests/hardware/main.rs`, following the existing `unlock_*` scenarios' shape (`tests/hardware/main.rs:492-608`) and reusing `UnlockCleanup`/`assert_actually_mounted`/`assert_mountpoint_under_run_media`:
    - **File-backed read-only, writes rejected at both layers (AC #1, #2):** `create::run` a tomb, then `unlock::run(&path, false, ...)` once and write a marker file via `assert_readable_and_writable` (this establishes real chown/chmod ownership on the volume's root inode per Task 2's design note, so the read-only assertions below aren't confounded by the "never-writably-mounted" edge case), then `close::run`, then `unlock::run(&path, true, ...)`. Assert: the marker file is still readable with its original contents; a new write attempt inside the mountpoint fails (expect an I/O error, e.g. `std::fs::write` returning `Err` with `ErrorKind` indicating a read-only filesystem); a `mount -o remount,rw <mountpoint>` shell-out (via `Command::new("mount").args(["-o", "remount,rw"]).arg(&mountpoint)`) fails non-zero, proving the underlying dm-crypt mapping itself (not just the fs-level flag) refuses to become writable, per AC #2's explicit "including a later remount attempt". Ownership is still the invoking user's (persisted from the earlier writable unlock) — reuse `assert_owned_by_invoking_user`.
    - **Device-backed read-only, identical command (AC #5):** same shape as the file-backed scenario above but against a `LoopDevice`-backed target (mirror `unlock_works_unmodified_against_a_device_backed_tomb`'s setup, `tests/hardware/main.rs:537-595`) — same `unlock::run(&loop_device.path, true, ...)` call, no different flags/branches, confirming write-rejection and remount-rejection hold identically.
    - **Read-only luksOpen-succeeds-but-mount-fails rollback (AC #3):** this is hard to force organically on real hardware (a read-only `mount -o ro` of a valid, freshly-formatted ext4 filesystem essentially never fails). Rather than contriving a fragile real-hardware failure, rely on the unit-level `read_only_mount_failure_still_closes_the_just_opened_mapping` test (Task 7) for this AC's coverage, and note that explicitly in Completion Notes rather than silently skipping it.
    - **Normal (non-read-only) unlock still allows writes (AC #4):** already covered by the existing `unlock_mounts_a_file_backed_tomb_with_a_readable_writable_filesystem` scenario, which is unaffected by this story (its call site gets `unlock::run(&path, false, ...)`, Task 4's compile-fix equivalent for hardware tests — update all three existing hardware `unlock::run(...)` call sites at `tests/hardware/main.rs:514`, `:577`, and inside `unlock_falls_back_to_a_suffixed_mount_point_on_a_basename_collision` to pass `false` explicitly). No new scenario needed for this AC beyond that signature fix.

## Dev Notes

### Architecture requirements (binding, from ARCHITECTURE-SPINE.md AD-11)

- **AD-11 — Read-only unlock propagates atomically to both layers, with rollback on partial failure:** a single `read_only: bool` enters `domain::workflows::unlock` from the CLI flag and is passed to both `LuksBackend::open` (`cryptsetup luksOpen --readonly`) and `FilesystemBackend::mount` (`mount -o ro`) in the same call — no code path sets one without the other. If `open` succeeds but `mount` subsequently fails, `unlock` calls `LuksBackend::close` on the just-opened mapping before returning the error.
- Read-only unlock is explicitly the **same** `domain::workflows::unlock` function as normal unlock (AD-4's preflight text: "including its read-only variant, which is the same `domain::workflows::unlock` function (AD-11), not a separate one") — this story is a parameter addition to the existing function, never a new sibling workflow.
- NFR11 (SPEC): "Read-only unlock must refuse writes at both the LUKS2/dm-crypt mapping level and the filesystem mount level — a filesystem-level-only read-only mount over a read-write dm-crypt mapping does not satisfy this." This is exactly why both `--readonly` (block level) and `-o ro` (filesystem level) are mandatory together, not either/or.
- AD-12 (deterministic mapping name/mountpoint discovery) — unchanged by this story; `unlock::run` already derives the mapping name via the shared `mapping_name` helper and discovers the mount point via `FilesystemBackend::mount`'s own return value, exactly as today.
- AD-8 (`FilesystemBackend` port) already documents `mount(read_only, AD-11)` and `open(read_only, AD-11, ...)` in `ARCHITECTURE-SPINE.md`'s file-structure table (line 161/163) — this story is implementing an already-planned signature, not introducing a new architectural concern.

### The one real design call this story has to make

Unlike Story 3.2 (which had a genuinely ambiguous "how do we even read current size" question), this story's architecture is fully specified by AD-11 — the one place that needs a deliberate decision rather than a literal reading of the spine is **what happens to the existing post-mount `chown`/`chmod 0700` step (Story 1.9's ownership hardening, `src/adapters/exec/mod.rs:1649-1694`) when the mount is read-only.** See Task 2 above for the resolution (skip it entirely for `read_only: true`, relying on a prior writable unlock having already persisted correct ownership) and the reasoning behind it. If this reasoning doesn't hold up during implementation — e.g. hardware testing shows a first-ever read-only unlock of a brand-new tomb produces a confusing permission-denied experience that this reasoning didn't anticipate — stop and flag it rather than guessing further, same standard Story 3.2's Dev Notes set for its own open question.

### Prior-story precedent to reuse, not reinvent

- **Rollback-on-mid-flow-failure discipline:** `unlock::run` already closes a mapping it just opened if `mount` subsequently fails (`src/domain/workflows/unlock.rs:24-33`) — this story's AC #3 is already satisfied by that existing code path once `read_only` is threaded through; do not add a second, read-only-specific rollback branch.
- **"Identical command, no branching on target type" convention:** every prior workflow (`unlock`, `enroll`, `revoke`, `close`, `resize`) already established this; `read_only` is just one more parameter that behaves identically for file- and device-backed targets, per AC #5.
- **Marker-bleed guard:** called out as its own task (Task 6) — the single most-repeated lesson across Epics 1–3 in this codebase per the Epic 2/3 retros. This story is lower-risk than most on this front since it introduces no new error-message text, but confirm that holds rather than assuming it.

### Project Structure Notes

- Touches (existing files, all UPDATE not NEW — no new modules, no structural changes to the hexagonal layering):
  - `src/ports/luks_backend.rs` — add `read_only` param to `open`.
  - `src/ports/filesystem_backend.rs` — add `read_only` param to `mount`.
  - `src/adapters/exec/mod.rs` — implement `--readonly`/`-o ro` in `open`/`mount`; skip chown/chmod when read-only.
  - `src/domain/workflows/unlock.rs` — add `read_only` param, pass to both port calls.
  - `src/domain/workflows/resize.rs` — update its own `luks.open` call site for the new signature (always `false`).
  - `src/cli/main.rs` — add `--read-only` flag to `Unlock`, thread through `run_unlock`.
  - `tests/unit/fakes.rs`, `tests/unit/unlock.rs`, `tests/unit/cli.rs` — updates and new tests.
  - `tests/hardware/main.rs` — update 3 existing `unlock::run(...)` call sites for the new signature; add 2 new read-only scenarios.
- No new `DomainError` variants, no new CLI subcommand, no new port — purely additive parameters on two existing port methods and one existing workflow function.

### Testing standard (AD-7)

Unit tests against the shared fakes in `tests/unit/fakes.rs`, run in default CI. Hardware-gated scenarios in `tests/hardware/main.rs`, manual-only via `make test-hardware`, never in CI.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 3.3: Unlock a Tomb Read-Only]
- [Source: ARCHITECTURE-SPINE.md#AD-11 — Read-only unlock propagates atomically to both layers, with rollback on partial failure]
- [Source: ARCHITECTURE-SPINE.md#AD-4 — Mandatory shared preflight gate (explicitly names read-only unlock as the same function, not a lighter gate)]
- [Source: ARCHITECTURE-SPINE.md#AD-8 — FilesystemBackend port; file-structure table already documents `open(read_only, AD-11, ...)`/`mount(read_only, AD-11)`]
- [Source: ARCHITECTURE-SPINE.md#AD-12 — Deterministic mapping name, no registry — unchanged by this story]
- [Source: src/domain/workflows/unlock.rs — existing rollback-on-mount-failure discipline this story reuses unmodified]
- [Source: src/domain/workflows/resize.rs:74 — sibling `luks.open` call site needing a compile-fix for the new signature]
- [Source: src/adapters/exec/mod.rs:1035-1071 — `ExecAdapter::open`'s existing `--token-only` implementation]
- [Source: src/adapters/exec/mod.rs:1562-1695 — `ExecAdapter::mount`'s existing implementation, including the Story 1.9 chown/chmod ownership-hardening block this story must conditionally skip]
- [Source: _bmad-output/implementation-artifacts/1-9-mount-ux-and-ownership-hardening.md — origin of the chown/chmod-after-mount behavior this story must reconcile with read-only mounts]
- [Source: _bmad-output/implementation-artifacts/3-2-grow-an-existing-tombs-capacity.md — sibling workflow shape (rollback discipline, marker-bleed guard, "identical command" convention)]

## Dev Agent Record

### Agent Model Used

Claude Sonnet 5 (claude-sonnet-5)

### Debug Log References

### Completion Notes List

- Tasks 1-4: threaded `read_only: bool` through `LuksBackend::open`, `FilesystemBackend::mount`, `unlock::run`, and fixed `resize::run`'s call site. `ExecAdapter::mount` skips the chown/chmod block entirely when `read_only` is true, per the story's documented design decision. `cargo build` passes with the full signature change landed atomically across all callers (a partial version of this change does not compile, since `open`/`mount` are called from both `unlock::run` and `resize::run`).
- Task 5: added `--read-only` flag to the `Unlock` CLI subcommand; `run_unlock` threads it through and adjusts both the intro and success messages when set, per FR5/NFR3's plain-language standard.
- Task 6: confirmed no new `AdapterFailure` string shapes were introduced — read-only `cryptsetup --readonly`/`mount -o ro` failures fall into the existing generic `"cryptsetup"`/`"mount"` marker-bleed buckets in `src/cli/ux.rs`. No code change needed.
- Task 7: extended `FakeLuksBackend::open`/`FakeFilesystemBackend::mount` to record `read_only`; updated the two existing `unlock.rs` tests to pass explicit `false`, added 3 new `unlock.rs` tests (`read_only_true_is_passed_to_both_open_and_mount`, `read_only_false_is_passed_to_both_open_and_mount`, `read_only_mount_failure_still_closes_the_just_opened_mapping`) plus `cli.rs`'s `unlock_help_lists_read_only_flag`. Full `cargo test --test unit` suite: 109 passed, 0 failed. `cargo clippy --all-targets`: clean.
- Task 8: `unlock::run`'s signature change actually touched 15 existing call sites across `tests/hardware/main.rs` (revoke/resize/collision-fallback scenarios also call it, not just the 3 the story anticipated) — all updated to pass explicit `false`. Added the 2 new read-only scenarios (`unlock_read_only_rejects_writes_at_both_layers_including_remount`, `unlock_read_only_rejects_writes_at_both_layers_against_a_device_backed_tomb`), both `#[ignore]`'d per AD-7. AC #3's read-only rollback path is covered at the unit level only, as the story's Task 8 anticipated.
- **Real-hardware run (2026-07-27), post-review fix:** the user ran both new scenarios on real hardware and hit a bug in the test code itself (not production code) — both scenarios called `UnlockCleanup::run()` (which does its own raw `umount`+`cryptsetup close`) immediately before also calling `close::run(&path, ...)`, so the second close always failed with `"has no active mapping"`. Fixed by dropping the redundant `UnlockCleanup::run()` call before `close::run` in both scenarios, matching the pattern already used elsewhere in this file (e.g. the resize hardware tests at `tests/hardware/main.rs:1377-1379`) where `close::run` alone performs the full unmount+close. Re-verified: `cargo test --no-run`, `cargo fmt --check`, `cargo clippy --all-targets`, and `cargo test --test unit` (109 passed) all clean after the fix; re-run against real hardware still pending confirmation.

### File List

- src/ports/luks_backend.rs
- src/ports/filesystem_backend.rs
- src/adapters/exec/mod.rs
- src/domain/workflows/unlock.rs
- src/domain/workflows/resize.rs
- src/cli/main.rs
- tests/unit/fakes.rs
- tests/unit/unlock.rs
- tests/unit/cli.rs
- tests/unit/workflows.rs
- tests/hardware/main.rs
