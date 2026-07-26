---
baseline_commit: 08eb4c0a5c56d5d37ae3a885635c00c5b159dbec
---

# Story 3.1: Close an Unlocked Tomb

Status: in-progress

## Story

As a user,
I want to close an unlocked tomb,
so that its filesystem is unmounted and the LUKS2 volume is re-locked, as the symmetric counterpart to unlock.

## Acceptance Criteria

1. **Given** an unlocked, mounted tomb **When** I run the close command **Then** the filesystem is unmounted first, then the LUKS2 mapping is closed **And** reversing that order would fail on a still-busy mapping.
2. **Given** close needs to find the mapping/mount point **When** it runs **Then** it reconstructs the deterministic mapping name from the path argument via the shared helper (no registry) **And** resolves the mount point via the kernel's mount table.
3. **Given** closing succeeds **When** I check afterward **Then** the mount point is no longer accessible and the volume requires a FIDO2 key to unlock again.
4. **Given** close **When** it runs **Then** `domain::preflight` runs first, like every other workflow.
5. **Given** an unlocked, mounted tomb backed by a raw device/partition instead of a loop-backed file **When** I run the close command **Then** the identical command works unmodified — close makes no branching decision based on target type.

## Tasks / Subtasks

- [x] Task 1: Add `FilesystemBackend::umount` port method (AC: #1, #2, #5)
  - [x] Add `fn umount(&self, mapper: &MapperHandle) -> Result<(), DomainError>` to `src/ports/filesystem_backend.rs`. Takes the mapper (device path), **not** a mountpoint — AD-12 is explicit that the mountpoint is never stored or passed in; `umount` resolves it itself.
  - [x] Implement in `ExecAdapter` (`src/adapters/exec/mod.rs`): resolve the live mountpoint for `mapper.device_node()` via `findmnt` (e.g. `findmnt -n -o TARGET <device_node>`), then run privileged `umount` against it. If `findmnt` reports nothing, the tomb isn't currently mounted — return a distinct `AdapterFailure` for this (see Task 5's ux guard — this message must not collide with the generic mount-failure bucket).
  - [x] **Design decision (resolves the open Epic 2 retro action item — do not leave unaddressed):** after a successful `umount`, remove the now-empty mount-point directory (`std::fs::remove_dir`), mirroring the cleanup-on-failure pattern `FilesystemBackend::mount` already uses on its own error paths (`src/adapters/exec/mod.rs:1405,1413,1433,1441,1457`). Rationale: `mount`'s `create_mount_point` (same file, ~line 163) creates a fresh, uniquely-named directory per unlock with a collision-suffix fallback — these directories are meant to be ephemeral, not accumulate. Without this, re-unlocking the same tomb after a close would permanently fall back to a suffixed directory name (the plain basename never frees up), which is the exact regression the existing hardware test `unlock_falls_back_to_a_suffixed_mount_point_on_a_basename_collision` (`tests/hardware/main.rs:608`) asserts against for the *first* unlock — rmdir-on-close keeps that guarantee true across repeated unlock/close cycles too.

- [x] Task 2: Implement `domain::workflows::close::run` (AC: #1, #2, #3, #4, #5)
  - [x] Replace the `todo!()` stub in `src/domain/workflows/close.rs`. Current stub signature is `run(luks, fido2, fs)` with **no path parameter** — add `path: &Path` as the first argument, mirroring `unlock::run`'s signature shape (`src/domain/workflows/unlock.rs`).
  - [x] Body: `preflight::check(luks, fido2, fs)?` first (AD-4, AC #4) → derive the mapping name via `mapping_name::mapping_name(path)?` (AD-12, AC #2) → build a `MapperHandle { name, source_path: path.to_path_buf() }` directly (no `luks.open` call — close acts on an already-open mapping, it doesn't open one) → `fs.umount(&mapper)?` → `luks.close(&mapper)?` (AD-8's explicit ordering, AC #1). `LuksBackend::close` already exists (`src/ports/luks_backend.rs:37`) and needs no changes.
  - [x] No target-type branching (AC #5) — same path argument works for loop-file or raw device, exactly like `unlock`/`enroll`/`revoke` already do.
  - [x] If `umount` fails, return the error immediately without calling `luks.close` — do not attempt to lock a mapping that may still be busy (AC #1's ordering rationale). There is nothing to roll back on this failure path (unlike `unlock`'s mount-failure rollback, which closes a mapping *it* just opened) since `close` never opens anything itself.

- [x] Task 3: Extend preflight dependency check (AC: #4)
  - [x] `FilesystemBackend::check_prerequisites` in `src/adapters/exec/mod.rs` (~line 1243) currently checks `["mkfs.ext4", "resize2fs", "blockdev", "mount", "id"]`. Add `"umount"` and `"findmnt"` — both ship in `util-linux`, the same package already providing `mount`/`blockdev`, so no new external dependency.

- [x] Task 4: Wire the `close` CLI subcommand (AC: #1, #3)
  - [x] Add a `Close { path: PathBuf }` variant to `Commands` in `src/cli/main.rs`, same `#[arg(allow_hyphen_values = true)]` convention as `Unlock`/`Enroll`/`Revoke`.
  - [x] Add a `run_close(path: PathBuf)` function mirroring `run_unlock`'s shape (`src/cli/main.rs:279`): preflight check first, a short plain-language intro line (FR5/NFR3), then call `close::run`, printing a success line on `Ok(())` and `ux::translate`+exit(1) on `Err`. No confirmation prompt — unlike `create`'s wipe warning or `revoke`'s irreversible-key warning, closing is fully reversible (re-unlock with the same FIDO2 key), so it doesn't need one.
  - [x] Wire the new match arm in `run()`.

- [x] Task 5: Plain-language translation for close's failures (AC: none directly — CAP-5/NFR3 quality bar)
  - [x] **Marker-bleed guard (explicitly flagged by the Epic 2 retro as a recurring bug class — check this before merge, not after):** `src/cli/ux.rs`'s existing generic bucket at `inner.contains("mount") || inner.contains("mkfs")` (line ~146) returns "Your tomb unlocked, but tomb-fido2 couldn't mount its filesystem" — but `"umount".contains("mount")` is `true` as a plain substring, so any of `close`'s new `umount`-related `AdapterFailure` text would silently fall into that *unlock*-flavored message. Add a dedicated check for `close`'s own failures (matching on markers like `"umount"`/`"findmnt"`/`"not currently mounted"` — whatever exact strings the Task 1 adapter code produces) **ordered before** the existing `mount`/`mkfs` bucket, the same way `luksDump`/`fido2-token`/enrollment markers are already ordered ahead of the generic buckets for the same reason.
  - [x] Give the "tomb isn't currently mounted" case (from `findmnt` finding nothing) its own clear message distinct from a generic umount failure.
  - [x] `ux::translate`'s `match` is exhaustive over `DomainError` variants (compiler-enforced) — no new variant is needed for this story since `AdapterFailure` covers both new failure shapes; only the `translate_adapter_failure` string-marker logic needs updating.

- [x] Task 6: Unit tests
  - [x] `tests/unit/fakes.rs`: add `umount` to `FilesystemBackend` impl for `FakeFilesystemBackend` (log the call, support `with_failure_at("umount")`, same pattern as its other methods).
  - [x] `tests/unit/workflows.rs`: the existing stub test `close_run_stops_at_preflight_before_reaching_its_own_todo` calls `close::run(&luks, &fido2, &fs)` with no path — update the call site for the new `path: &Path` parameter (it will otherwise fail to compile once Task 2 lands).
  - [x] Add `tests/unit/close.rs` (register `mod close;` in `tests/unit/main.rs`), mirroring `tests/unit/unlock.rs`'s structure (including its `RealFixtureFile` helper for a real path to canonicalize): happy path asserts call order `["umount", "close"]` and that `mapping_name::mapping_name` derived the same name passed to both port calls; a `with_failure_at("umount")` case asserts `close::run` returns `Err` **without** `luks.close` being called (log should show only `["umount"]`).

- [x] Task 7: Hardware tests (manual-only, `make test-hardware`, AD-7 — not run in default CI)
  - [x] Add scenarios in `tests/hardware/main.rs` verifying: closing an unlocked file-backed tomb unmounts and re-locks it (mount point gone, device node gone, a subsequent `unlock::run` on the same path succeeds again with the *same* FIDO2 key); the identical close command against a device-backed tomb (AC #5); after close, the freed mount-point directory is gone (not just unmounted) so a repeat unlock gets the plain basename back, not a `-<suffix>` fallback.
  - [x] Optional, not required for these ACs: several existing hardware scenarios (e.g. `unlock_mounts_a_file_backed_tomb_...`) currently clean up via a hand-rolled `unmount_and_close` helper with a comment noting "`close` (Story 3.1) doesn't exist yet" (`tests/hardware/main.rs:383-386`). Now that real `close::run` exists, those helpers could call it instead — left as-is per the story's own "not required" note (no trivial swap attempted; the existing helper is proven and low-risk to leave untouched).

## Dev Notes

- **Architecture ordering is exact and non-negotiable (AD-8):** "`domain::workflows::close` (CAP-9) calls `FilesystemBackend::umount` **before** `LuksBackend::close` — reversing that order would fail on a still-busy mapping." [Source: ARCHITECTURE-SPINE.md#AD-8]
- **Mountpoint discovery, no registry (AD-12):** "The mount point is not stored either: `FilesystemBackend::umount` takes the mapper device path and resolves the live mountpoint via the kernel's own mount table (e.g. `findmnt` against `/dev/mapper/<name>`), never a remembered path." — this is why `umount`'s port signature takes `&MapperHandle`, not a `PathBuf` mountpoint. [Source: ARCHITECTURE-SPINE.md#AD-12]
- **Preflight, uniformly (AD-4):** close gets the identical `domain::preflight::check` gate as every other workflow, as the first statement in `run`, no lighter gate. [Source: ARCHITECTURE-SPINE.md#AD-4]
- **No side-channel state (AD-2):** nothing about the mount point or mapping is persisted anywhere close needs to read back — it's all rederived. Don't introduce a lookup file/registry.
- **This is the first story in Epic 3** — no prior story in this epic to inherit dev notes from. The relevant carried-forward context instead comes from the Epic 2 retrospective (`_bmad-output/implementation-artifacts/epic-2-retro-2026-07-26.md`), both items already folded into Tasks 1 and 5 above:
  1. The mount-directory `rmdir` decision (Task 1) was explicitly flagged as "must be a deliberate decision when 3.1 is scoped, not an oversight" — resolved above.
  2. The "marker bleed" bug class (Task 5) has caused 3 real bugs across Epics 1-2 (`unlock` swallowed by `enroll`'s marker, `revoke` swallowed by `enroll`'s markers, `revoke`'s own failures mislabeled) — `close` is exactly the kind of new workflow reusing shared `adapters::exec` string-matched errors that the retro calls out as high-risk for this pattern. Check it before merge.
- **Symmetry with `unlock`, not a copy of its rollback logic:** `unlock::run` closes a mapping *it just opened* if `mount` fails afterward (`src/domain/workflows/unlock.rs:24-33`) — that rollback exists because `unlock` is the one that opened it. `close` never opens anything, so there's no equivalent rollback needed on `umount` failure; just propagate the error and stop before `luks.close`.
- **No confirmation prompt for `close`** (unlike `create`'s wipe warning or `revoke`'s irreversible-key warning) — closing is always reversible via a normal `unlock`, so it doesn't fit the pattern that justified those two interactive gates.
- **Testing standard (AD-7):** unit tests against the shared fakes in `tests/unit/fakes.rs`, run in default CI; hardware-gated scenarios in `tests/hardware/main.rs`, manual-only via `make test-hardware`, never in CI.

### Project Structure Notes

- Touches (existing files, all UPDATE not NEW except the new test file):
  - `src/ports/filesystem_backend.rs` — add `umount` to the trait.
  - `src/adapters/exec/mod.rs` — implement `umount` (findmnt + umount + rmdir), extend `check_prerequisites`'s binary list.
  - `src/domain/workflows/close.rs` — replace stub with real implementation, new `path` parameter.
  - `src/cli/main.rs` — new `Close` subcommand + `run_close`.
  - `src/cli/ux.rs` — new marker-ordered translation branch.
  - `tests/unit/fakes.rs`, `tests/unit/workflows.rs`, `tests/unit/main.rs` — updates.
  - `tests/hardware/main.rs` — new scenarios.
  - New file: `tests/unit/close.rs`.
- No new modules, no structural changes to the hexagonal layering — this story is pure wiring within the existing seed (`domain::workflows::close` already exists as a stub; `FilesystemBackend` already exists as a port).
- No detected conflicts or variances from the structural seed.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 3.1: Close an Unlocked Tomb]
- [Source: ARCHITECTURE-SPINE.md#AD-8 — FilesystemBackend port, parameterized by filesystem type]
- [Source: ARCHITECTURE-SPINE.md#AD-12 — Deterministic mapping name and mountpoint discovery, no registry]
- [Source: ARCHITECTURE-SPINE.md#AD-4 — Mandatory shared pre-flight gate]
- [Source: _bmad-output/implementation-artifacts/epic-2-retro-2026-07-26.md — Epic 3 Preview and Action Items #4/#5]
- [Source: src/domain/workflows/unlock.rs, src/domain/workflows/revoke.rs — sibling workflow shape/conventions]
- [Source: src/adapters/exec/mod.rs:1338 — `FilesystemBackend::mount`'s existing mount-point creation/cleanup pattern this story's `umount` mirrors]
- [Source: tests/hardware/main.rs:608 — `unlock_falls_back_to_a_suffixed_mount_point_on_a_basename_collision`, the test whose guarantee `rmdir`-on-close preserves across repeated cycles]

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

- Task 1/3: Added `FilesystemBackend::umount` to the port trait and implemented it in `ExecAdapter` (findmnt to resolve the live mountpoint from the mapper device node, privileged `umount`, then best-effort `rmdir` of the now-empty mount point). Extended `check_prerequisites`'s binary list with `umount`/`findmnt`. `cargo build` and full `cargo test` (79 passed) both green; no regressions.
- Task 2: Replaced the `close::run` stub with the real implementation — preflight, derive mapping name, build `MapperHandle` directly (no `luks.open`), `fs.umount` then `luks.close`, no rollback on umount failure. Updated the existing preflight stub test's call site for the new `path` parameter. Full `cargo test` (79 passed) still green.
- Task 4: Wired the `Close { path }` CLI subcommand and `run_close`, mirroring `run_unlock`'s shape (preflight, plain-language intro, success/error reporting) with no confirmation prompt. Full `cargo test` (79 passed) still green.
- Task 5: Added a marker-ordered `close`-specific branch in `translate_adapter_failure` (checked before the generic `mount`/`mkfs` bucket) covering `"umount"`/`"findmnt"` markers, with a distinct message for the `"not currently mounted"` case. Added 3 new RED-then-GREEN ux tests proving the marker-bleed guard actually blocks the misclassification (a plain `"umount failed"` string previously fell into unlock's "Your tomb unlocked, but..." message). Full `cargo test` (82 passed) green.
- Task 6: Added `tests/unit/close.rs` (registered in `tests/unit/main.rs`) mirroring `unlock.rs`'s structure: happy path asserts call order `["umount", "close"]` and that both port calls received the same derived mapping name (added `last_umount`/`last_close` capture accessors to the fakes for this, following the existing `last_open`/`last_removed_keyslot` pattern); a `with_failure_at("umount")` case asserts `close::run` returns `Err` without `luks.close` being called. `cargo fmt`/`cargo clippy --all-targets` clean, full `cargo test` (84 passed) green.
- Task 7: Added two manual-only (`#[ignore]`) hardware scenarios in `tests/hardware/main.rs`: `close_unmounts_and_relocks_a_file_backed_tomb_allowing_a_clean_repeat_unlock` (creates + unlocks a file-backed tomb, closes it via `close::run`, asserts the mount-point directory and dm-crypt device node are both gone, then unlocks a second time with the same key and asserts the *exact same*, unsuffixed mount point is reclaimed) and `close_works_unmodified_against_a_device_backed_tomb` (AC #5, identical close call against a loop-backed device target). Left the existing `unmount_and_close` hand-rolled cleanup helper as-is per the story's own "not required" note. Confirmed both new tests are registered and `#[ignore]`d via `cargo test --test hardware -- --list`; default `cargo test` (84 passed, hardware scenarios excluded), `cargo fmt --check`, and `cargo clippy --all-targets` all clean.

### File List

- `src/ports/filesystem_backend.rs` — added `umount` to `FilesystemBackend` trait.
- `src/adapters/exec/mod.rs` — implemented `umount`; extended `check_prerequisites` binary list.
- `src/domain/workflows/close.rs` — replaced `todo!()` stub with real implementation, new `path` parameter.
- `tests/unit/fakes.rs` — added `umount` to `FakeFilesystemBackend`'s trait impl.
- `tests/unit/workflows.rs` — updated `close::run` call site for the new `path` parameter.
- `src/cli/main.rs` — added `Close` subcommand and `run_close`.
- `src/cli/ux.rs` — new marker-ordered `close`-failure translation branch.
- `tests/unit/ux.rs` — new tests for the marker-bleed guard and the not-currently-mounted message.
- `tests/unit/fakes.rs` — added `last_umount`/`last_close` capture accessors.
- `tests/unit/main.rs` — registered `mod close;`.
- `tests/unit/close.rs` — new file: unit tests for `close::run`.
- `tests/hardware/main.rs` — new manual-only scenarios for `close::run` (file-backed and device-backed).
