---
baseline_commit: 9395b10
---

# Story 1.7: Unlock and Mount a Tomb

Status: review

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a user with no FIDO2 experience,
I want to unlock an existing tomb with my FIDO2 key and have it mounted automatically,
so that I can access its contents in one guided step.

## Acceptance Criteria

1. **Given** an existing tomb (loop-backed file or raw device) with at least one enrolled FIDO2 key, **when** I run the unlock command and touch my key when prompted, **then** the LUKS2 volume opens via `LuksBackend::open`, the mounted filesystem becomes accessible at a discoverable mount point (via the kernel's mount table), and prompts use plain language assuming zero FIDO2 knowledge. [Source: epics.md#Story 1.7]
2. **Given** the same unlock command, **when** run against a raw device/partition path instead of a loop-file path, **then** the identical command works unmodified — no different flags or behavior branch based on target type. [Source: epics.md#Story 1.7]
3. **Given** the mapping name needed to open the volume, **when** unlock runs, **then** it derives the dm-crypt mapping name deterministically from the canonicalized device/file path via the single shared helper — never user-supplied, random, or stored. [Source: epics.md#Story 1.7]

## Tasks / Subtasks

- [x] Task 1: Add `LuksBackend::open` — opens an existing LUKS2 volume via its FIDO2 token (AC: #1, #2)
  - [x] Signature: `fn open(&self, path: &Path, name: &str) -> Result<MapperHandle, DomainError>`. Mirrors `bootstrap_format_and_open`'s shape (`domain` derives `name` via the shared helper and passes it in — the port never computes it itself, AC #3), but performs no formatting: it only opens an *existing* header.
  - [x] Real implementation (`adapters::exec`): `privileged("cryptsetup").args(["open", "--token-only"]).arg(path).arg(name)`, run with **inherited stdio** (`.status()`, not `.output()`) so the systemd-fido2 plugin's touch/PIN prompt reaches the real terminal — the same interactive pattern `Fido2Backend::enroll_fido2_key`'s `systemd-cryptenroll` call already uses (`src/adapters/exec/mod.rs:688-695`).
  - [x] **`--token-only` is not optional — this exact gap was already found and documented by Story 1.6's own hardware run:** "`cryptsetup open <device> <name>` without `--token-only` prompts for a passphrase instead of going straight to the FIDO2 PIN/touch flow" [Source: _bmad-output/implementation-artifacts/1-6-create-a-device-backed-tomb.md Completion Notes, finding (1)]. Omitting it would silently break AC #1's "touch my key when prompted" flow.
  - [x] No `has_luks2_header`-style pre-check before calling `open` — unlike `create`'s refuse-before-touching-anything gate, there is no AC requiring a friendlier pre-flight error for "this isn't a tomb": if `open` fails, `cryptsetup`'s own stderr surfaces via the existing `AdapterFailure` path. Do not add a speculative pre-check the ACs don't ask for.
- [x] Task 2: Add `FilesystemBackend::mount` — mounts an opened mapping at a fresh, discoverable mount point (AC: #1)
  - [x] Signature: `fn mount(&self, mapper: &MapperHandle) -> Result<PathBuf, DomainError>`. No `Filesystem` parameter: let `mount` auto-detect the filesystem type from the superblock (standard, well-known kernel behavior, satisfying NFR10) rather than re-deriving `Filesystem::Ext4` from the LUKS2 token's stored `filesystem` field (AD-2) just to pass `-t` — that field's only documented reader is `resize` (Story 3.2, AD-2); do not give `unlock` a new, unneeded reason to parse token metadata.
  - [x] Mount point: create a fresh, uniquely-named directory under `std::env::temp_dir()` for every unlock — reuse `adapters::exec::generate_transient_passphrase`'s sibling pattern already in this file, `TempKeyFile::create`'s random-suffix generation (`src/adapters/exec/mod.rs:133-141`, `getrandom::fill` + hex-encode), just for a directory name instead of a temp-file name (e.g. `tomb-fido2-<mapper.name>-<suffix>`). **Do not make the mount point deterministic/derived-from-path** — per AD-12, "the mount point is not stored either... resolves the live mountpoint via the kernel's own mount table... never a remembered path." A fresh path each unlock, discoverable later only via `findmnt`/the mount table against the (deterministic) mapper device node, is exactly what AD-12 describes; Story 3.1 (`close`) is what will call `findmnt` to rediscover it — do not build that lookup now, it is out of scope for this story.
  - [x] Real implementation: `std::fs::create_dir(&mountpoint)` (as the invoking user — plain `/tmp`-backed directory creation needs no elevation), then `privileged("mount").arg(mapper.device_node()).arg(&mountpoint)`, matching the existing `mkfs`/`close` privilege pattern (only the actual mount syscall needs root). On mount failure, best-effort `std::fs::remove_dir(&mountpoint)` before returning `AdapterFailure` (mirrors `set_backing_file_size`'s cleanup discipline elsewhere in this file).
  - [x] Add `"mount"` to `FilesystemBackend::check_prerequisites`'s required-binaries list (`src/adapters/exec/mod.rs:711`, currently `["mkfs.ext4", "resize2fs", "blockdev"]`) — AD-4's preflight gate must cover every hard dependency this story introduces, same discipline Story 1.5 used to preemptively add `blockdev` ahead of Story 1.6's `device_capacity`.
- [x] Task 3: Implement `domain::workflows::unlock::run` (currently `todo!()` at `src/domain/workflows/unlock.rs:13`) (AC: #1, #2, #3)
  - [x] New signature: `pub fn run(path: &Path, luks: &dyn LuksBackend, fido2: &dyn Fido2Backend, fs: &dyn FilesystemBackend) -> Result<PathBuf, DomainError>` — returns the mount point on success (the CLI needs it to tell the user where the tomb landed). `fido2` stays in the signature only for `preflight::check` (AD-4's uniform three-port gate), unused elsewhere in this function — same as `close`/`resize`'s existing stub signatures.
  - [x] Body, in order: `preflight::check(luks, fido2, fs)?;` → `let name = mapping_name::mapping_name(path)?;` (AC #3 — the single shared helper, already used identically by `create`) → `let mapper = luks.open(path, &name)?;` → mount, with cleanup on failure:
    ```rust
    match fs.mount(&mapper) {
        Ok(mountpoint) => Ok(mountpoint),
        Err(err) => {
            let _ = luks.close(&mapper);
            Err(err)
        }
    }
    ```
  - [x] This close-on-mount-failure step is not optional: leaving a successfully-opened mapping dangling on a mount error is the exact class of bug Story 1.6's post-review found and fixed for `create` ("if `resize` fails, the caller's close-on-failure logic never runs and the open mapping leaks" — [Source: 1-6-create-a-device-backed-tomb.md Review Findings]). Apply the same discipline here from the start.
  - [x] AC #2 requires zero branching on target type — do not add any `if path.is_file()`-style check anywhere in this function or the adapter calls it makes. `mapping_name`, `LuksBackend::open`, and `FilesystemBackend::mount` already treat file- and device-backed targets identically (confirmed by Story 1.6's Dev Notes re: `mapping_name`), so satisfying AC #2 requires writing nothing target-type-specific, not adding a code path.
- [x] Task 4: Wire the CLI's `unlock` subcommand (AC: #1)
  - [x] Add `Commands::Unlock { path: PathBuf }` to `src/cli/main.rs`'s `Commands` enum (currently only `Create`), taking a single required `--path` (or positional — match `create`'s existing `--path` flag style for consistency).
  - [x] Print a plain-language line before calling `unlock::run` — e.g. `"Touch your security key now (you may also be asked for its PIN)."` — satisfying FR5/NFR3's "prompts use plain language assuming zero FIDO2 knowledge" for the tool's *own* messaging. `cryptsetup`'s own interactive text from the systemd-fido2 plugin still appears as-is via inherited stdio (same accepted limitation as `systemd-cryptenroll`'s prompt in `create`); translating *that* text is explicitly Story 1.8's job, not this one's — do not touch `src/cli/ux.rs` (stays empty, per Story 1.6's Project Structure Notes).
  - [x] On success, print the returned mount point, e.g. `"Tomb unlocked and mounted at {mountpoint}."`. On error, `eprintln!("{err}"); std::process::exit(1);` — identical pattern to `run_create`'s existing error handling.
- [x] Task 5: Fakes and unit tests (AC: #1, #2, #3)
  - [x] `FakeLuksBackend::open`: log `"open"`, honor the existing generic `fail_at`/`fail_if` mechanism (no new field needed — it's already call-name-keyed), and record the `(path, name)` it was called with (add `last_open: RefCell<Option<(PathBuf, String)>>` + a `last_open()` accessor, mirroring the existing `last_bootstrap_size()` pattern) so a test can assert AC #3's exact derived name without re-deriving it by hand.
  - [x] `FakeFilesystemBackend::mount`: log `"mount"`, honor `fail_at`/`fail_if`, return a deterministic `Ok(PathBuf::from(format!("/tmp/fake-mount-{}", mapper.name)))` by default.
  - [x] Update `tests/unit/workflows.rs`'s existing `unlock_run_stops_at_preflight_before_reaching_its_own_todo` call site — it currently calls `unlock::run(&luks, &fido2, &fs)` with no path; add a placeholder path argument (e.g. `std::path::Path::new("/tmp/does-not-matter")`) so it compiles against the new signature. Leave the test's name and its `PreflightFailed` assertion unchanged — this repo's convention (see `create_run_stops_at_preflight_before_reaching_its_own_todo`, which kept its "before_reaching_its_own_todo" name even after `create` was fully implemented in Story 1.5/1.6) is to fix call sites for signature changes without renaming these generic preflight-gate tests. *(Done as part of Task 3's commit — the signature change itself broke this call site immediately, same as Story 1.6's `d9d35ff` fixing `create.rs`'s own call site in the same commit that changed the port signature, rather than deferring the fix to a later task.)*
  - [x] New file `tests/unit/unlock.rs` (register it in `tests/unit/main.rs`'s `mod` list alongside `create`), using the same `RealFixtureFile` helper `tests/unit/create.rs` already defines for real, on-disk, canonicalizable paths (`mapping_name` calls `std::fs::canonicalize` directly, not behind a port — a fake-only path won't canonicalize):
    - [x] Happy path: passing fakes, a `RealFixtureFile`; assert `unlock::run(...)` returns `Ok(mountpoint)` matching the fake's deterministic mount path; assert the call log is exactly `["open", "mount"]`; assert `luks.last_open()`'s captured name equals `mapping_name::mapping_name(&fixture_path).unwrap()` (calling the same real, pure helper directly for comparison — same pattern already used elsewhere for `mapping_name` assertions).
    - [x] Mount-failure path: `FakeFilesystemBackend::passing().with_failure_at("mount")`; assert the result is `Err`, and the call log is `["open", "mount", "close"]` — proving the just-opened mapping gets closed on a mount failure (mirrors `create.rs`'s existing `enroll_failure_closes_the_mapping_and_removes_the_backing_file`-style failure-path test, minus the file-removal step which doesn't apply here).
  - [x] Confirm `cargo test --test unit` / `make test`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --check` all stay green.
- [x] Task 6: Hardware-gated integration test (manual-only, `make test-hardware`, AD-7) (AC: #1, #2)
  - [x] Extend `tests/hardware/main.rs` with a scenario that first creates a real tomb via `create::run` (reuse the existing file-backed pattern), then calls the new `domain::workflows::unlock::run` against it directly, and asserts: the returned mount point exists and is actually mounted (e.g. via `findmnt` against the mapper's device node), and the filesystem is readable/writable at that path (write a marker file, read it back).
  - [x] Cover AC #2 by adding a second scenario using the existing `LoopDevice` helper (Story 1.6) as the unlock target instead of a plain file — proving the identical `unlock::run` call works unmodified against a device-backed target too. Detach the loop device as the final manual/automated step, following Story 1.6's established loop-device lifecycle.
  - [x] Same hardware-run environment caveat as Stories 1.5/1.6 applies: build with `cargo test --test hardware --no-run` as the normal user, then run the compiled test binary under `sudo` outside the Nix devShell (this sandbox's devShell `cryptsetup`/`systemd` have an empty token-plugin search path) — do not assume `cargo test --test hardware -- --ignored` alone will work here.

## Dev Notes

- **This story adds the first genuinely new port methods since Story 1.6** — `LuksBackend::open` and `FilesystemBackend::mount` don't exist yet. Everything else needed (`mapping_name`, `preflight`, `MapperHandle`, `luks.close`) already exists and is reused unchanged. [Source: src/ports/{luks_backend,filesystem_backend}.rs; src/domain/{mapping_name,preflight}.rs]
- **`--token-only` is the single most important detail in this story.** It was discovered the hard way during Story 1.6's real-hardware verification (not from any spec/architecture doc) and is the one concrete piece of evidence that plain `cryptsetup open` does *not* go straight to the FIDO2 flow. Missing it would look like it compiles and passes unit tests (fakes don't care about the flag) but fail AC #1 on real hardware exactly the way Story 1.6 already hit and fixed. [Source: _bmad-output/implementation-artifacts/1-6-create-a-device-backed-tomb.md Completion Notes, "Two more findings from the user's real run"]
- **Do not add a `read_only` parameter to `open`/`mount`/`unlock::run` in this story**, even though the architecture's port sketch already shows the eventual shape as `open(read_only, AD-11, ...)`/`mount(read_only, AD-11)`. Story 1.6's own precedent for widening a port signature ahead of an AC (`bootstrap_format_and_open`'s `size` param) only did so because *that story's own* AC required it — not speculatively for a later story. Read-only unlock is Story 3.3's AC (CAP-11), which will extend these signatures then. Keep this story's surface to exactly what AC #1-#3 need. [Source: ARCHITECTURE-SPINE.md#AD-11, #Structural Seed; _bmad-output/implementation-artifacts/1-6-create-a-device-backed-tomb.md Task 2 rationale]
- **AD-12's mount-point-discovery model is load-bearing for Task 2's design:** the mount point is never stored or reconstructed from the path — only the mapping *name* is deterministic. Rediscovering the live mount point later (Story 3.1's `close`) is explicitly designed to go through `findmnt` against the mapper device node, not a remembered value. Don't invent a "remember where I mounted it" mechanism (sidecar file, deterministic path formula, etc.) — that would violate AD-2's no-side-channel-state rule as much as any other invented registry. [Source: ARCHITECTURE-SPINE.md#AD-12]
- **`mapping_name` needs no changes** — it already canonicalizes and hashes any path (file or device) uniformly; this is exactly what makes AC #2's "no branching on target type" achievable for free once Task 3 is written without any `is_file()`/`is_device()` check. [Source: src/domain/mapping_name.rs; 1-6 Dev Notes]
- **`AD-3`'s secret-material rule doesn't add any new constraint here.** Unlike `create`'s transient bootstrap passphrase, unlocking via FIDO2 involves no passphrase at all on this tool's side — the touch/PIN exchange happens entirely between the user, the physical key, and the `systemd-fido2` cryptsetup token plugin over inherited stdio. `adapters::exec` doesn't generate, hold, or wipe any secret for this workflow.
- **Privilege model:** `open` and `mount` both need `privileged()` (dm-crypt mapping creation and the mount syscall are both root-only operations), matching the existing `luksOpen`/`mkfs`/`close` calls. Creating the mount-point *directory* itself does not need elevation (plain `/tmp`-backed `mkdir`).
- **Testing (AD-7):** extend `tests/unit/fakes.rs` in place (no new struct fields beyond what's listed in Task 5 — reuse the existing generic `fail_at` mechanism rather than adding a per-method flag). New dedicated `tests/unit/unlock.rs` file, following `create.rs`'s established one-file-per-substantial-workflow convention. Hardware scenario stays `#[ignore]`d, `make test-hardware`-only, never default CI.
- **Git intelligence:** the last commit (`9395b10`, "create a device-backed tomb") is the baseline; no other files changed since. `unlock.rs`'s `todo!()` stub, and the exact current `LuksBackend`/`FilesystemBackend` trait contents quoted throughout this story, are accurate as of that commit.

### Project Structure Notes

- Modified: `src/ports/luks_backend.rs` (new `open`), `src/ports/filesystem_backend.rs` (new `mount`), `src/domain/workflows/unlock.rs` (`run` implemented, new signature/return type), `src/adapters/exec/mod.rs` (real `open`/`mount`, `"mount"` added to `FilesystemBackend::check_prerequisites`), `src/cli/main.rs` (new `Commands::Unlock` variant + match arm), `tests/unit/fakes.rs` (new fake methods), `tests/unit/workflows.rs` (fixed call site only), `tests/hardware/main.rs` (new scenario(s)).
- New file: `tests/unit/unlock.rs` (register in `tests/unit/main.rs`'s `mod` list).
- Do **not** touch `src/domain/workflows/{close,resize,enroll,revoke}.rs` (out of scope — Stories 1.8/2.x/3.x), `src/cli/ux.rs` (stays empty — Story 1.8's plain-language boundary), `src/domain/types.rs` (no new type needed — `MapperHandle`/`PathBuf` already sufficient), or any LUKS2 token metadata reading (that's `resize`'s concern per AD-2, not unlock's).
- Consistent with `ARCHITECTURE-SPINE.md`'s Structural Seed: `ports/*` gain their next AD-anticipated methods (`open`, `mount` — without their eventual `read_only` param, deferred to Story 3.3 per the Dev Notes above), `adapters/exec/` gains the corresponding real subprocess logic, `cli/main.rs` gains its second real clap subcommand, `tests/hardware/` gains its third scenario.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.7: Unlock and Mount a Tomb]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-3 — Secret material never enters tomb-fido2's own process]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-4 — Shared preflight gate, identical for create/close/resize/unlock incl. read-only]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-8 — FilesystemBackend port, mount/umount methods]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-11 — Read-only unlock propagation (deferred to Story 3.3, referenced only to explain why `read_only` is NOT added now)]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-12 — Deterministic mapping name and mountpoint discovery, no registry]
- [Source: _bmad-output/specs/spec-tomb-fido2/SPEC.md#CAP-1] (unlock: FIDO2 key opens LUKS2 volume, filesystem mounted and ready in the same operation)
- [Source: src/domain/workflows/unlock.rs] (current state as of baseline `9395b10`: `preflight::check` then `todo!()`)
- [Source: src/ports/{luks_backend,filesystem_backend}.rs] (current method sets — neither `open` nor `mount` exist yet)
- [Source: src/domain/mapping_name.rs] (shared canonicalize+hash helper, unchanged, reused as-is)
- [Source: src/adapters/exec/mod.rs] (`privileged()` helper `:42`; `TempKeyFile`'s random-suffix pattern to mirror for the mount-point directory name `:114-159`; `enroll_fido2_key`'s inherited-stdio `.status()` pattern to mirror for `open` `:688-695`; `check_prerequisites`'s required-binaries list to extend `:708-722`)
- [Source: src/cli/main.rs] (current single `Commands::Create` — `run_create`/error-handling pattern to mirror for `Unlock`)
- [Source: tests/unit/{fakes.rs,create.rs,workflows.rs}, tests/hardware/main.rs] (existing fake/test conventions to extend in place)
- [Source: _bmad-output/implementation-artifacts/1-6-create-a-device-backed-tomb.md] (previous story — `--token-only` gotcha, close-on-failure discipline, loop-device hardware-test pattern, hardware-run environment quirks)

## Dev Agent Record

### Agent Model Used

Claude Sonnet 5 (claude-sonnet-5)

### Debug Log References

### Completion Notes List

- Task 1: Added `LuksBackend::open`, real `ExecAdapter` impl using `cryptsetup open --token-only` with inherited stdio (`.status()`), and `FakeLuksBackend::open` (`last_open()` accessor) so the suite keeps compiling. `cargo test --test unit` green (31 passed).
- Task 2: Added `FilesystemBackend::mount`, real `ExecAdapter` impl (fresh temp-dir mount point via a `random_hex_suffix` helper factored out of `TempKeyFile::create`'s existing random-suffix generation, then `privileged("mount")`, with best-effort mount-point removal on failure), `"mount"` added to `check_prerequisites`'s required binaries, and `FakeFilesystemBackend::mount`. `cargo test --test unit` green (31 passed).
- Task 3: Implemented `unlock::run(path, luks, fido2, fs) -> Result<PathBuf, DomainError>` (preflight → `mapping_name` → `luks.open` → `fs.mount`, closing the mapping if `mount` fails). The signature change immediately broke `tests/unit/workflows.rs`'s existing preflight-gate test call site, so that call-site fix (Task 5's listed subtask) was applied here in the same commit to keep the build green, matching Story 1.6's precedent (`d9d35ff`). `cargo test --test unit` green (31 passed).
- Task 4: Added `Commands::Unlock { path }` to the CLI (`--path` flag, matching `create`'s style), printing a plain-language touch/PIN prompt before calling `unlock::run`, the mount point on success, and `eprintln!`/`exit(1)` on error (identical pattern to `run_create`). Manually verified `cargo run -- unlock --help`. `cargo test --test unit` green (31 passed).
- Task 5: New `tests/unit/unlock.rs` (happy path + mount-failure-closes-the-mapping path), registered in `tests/unit/main.rs`. `cargo test --test unit` (33 passed), `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --check` all green.
- Task 6: Added two `#[ignore]`d hardware scenarios to `tests/hardware/main.rs` — file-backed and loop-device-backed — each running real `create::run` then real `unlock::run`, asserting via `findmnt` that the returned mount point is actually mounted and via a written-then-read-back marker file that the filesystem is readable/writable, then cleaning up (`umount`/`cryptsetup close`, plus `losetup -d` for the device scenario) since `close` (Story 3.1) doesn't exist yet.
- **Pre-existing bug found via the user's real hardware run, fixed here (not part of this story's own task list, but blocking its hardware verification):** `ExecAdapter::bootstrap_format_and_open` (Story 1.6) unconditionally called `cryptsetup resize --device-size <size>` after every `luksOpen`, on the assumption (stated in its own doc comment) that resizing to the backing file's own exact size was a harmless no-op. The user's hardware run proved that assumption wrong: 3 of 4 hardware tests failed with `cryptsetup resize ... failed: Device is too small`, every time `size` equaled the *entire* raw capacity of the underlying file/device (no headroom for the LUKS2 header to live alongside a full-size payload) — including the pre-existing Story 1.5 hardware test, unrelated to this story's own changes, confirming this wasn't a Story 1.7 regression. The one hardware test with deliberate headroom (32MB requested out of a 64MB loop device, Story 1.6's own device-backed scenario) passed, isolating the exact failure condition. Fixed by adding `actual_raw_size` (file length, or `blockdev --getsize64` for a block device) and only calling `resize` when `size` is strictly less than the raw capacity — LUKS2's default dynamic sizing already gives the correct, header-excluded mapping for the full-capacity case with no resize needed at all. `cargo test --test unit` (33 passed), `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --check` all still green. Awaiting a second hardware run to confirm the fix.

### File List

- Modified: `src/ports/luks_backend.rs`
- Modified: `src/ports/filesystem_backend.rs`
- Modified: `src/adapters/exec/mod.rs`
- Modified: `src/domain/workflows/unlock.rs`
- Modified: `src/cli/main.rs`
- Modified: `tests/unit/fakes.rs`
- Modified: `tests/unit/workflows.rs`
- Modified: `tests/unit/main.rs`
- New: `tests/unit/unlock.rs`
- Modified: `tests/hardware/main.rs`

## Change Log

- 2026-07-24: Task 1 — `LuksBackend::open`
- 2026-07-24: Task 2 — `FilesystemBackend::mount`
- 2026-07-24: Task 3 — `unlock::run` implemented
- 2026-07-24: Task 4 — CLI `unlock` subcommand
- 2026-07-24: Task 5 — unit tests for `unlock`
- 2026-07-24: Task 6 — hardware-gated integration tests written and compiling
- 2026-07-24: fix — skip `bootstrap_format_and_open`'s `resize` call when `size` equals the full raw capacity (pre-existing Story 1.6 bug, found via the user's real hardware run of this story's Task 6 tests)
