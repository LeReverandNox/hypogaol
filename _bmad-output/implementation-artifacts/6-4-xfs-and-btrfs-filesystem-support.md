---
baseline_commit: 25c655d823c4fcd790c70fc82b659a1ff6e50d5a
---

# Story 6.4: XFS and Btrfs Filesystem Support

Status: review

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a user,
I want to create or resize a volume using XFS or Btrfs, not just ext4,
so that I can pick the filesystem that best fits my use case.

## Acceptance Criteria

1. **Given** I run create with `--filesystem xfs` or `--filesystem btrfs`, **when** creation completes, **then** the volume is formatted with `mkfs.xfs` or `mkfs.btrfs --mixed` respectively, and the chosen type is recorded in the `filesystem` token field. [Source: epics.md#Story 6.4, lines 850-852]
2. **Given** a Btrfs volume as small as ~20 MiB, **when** it's created, **then** it succeeds, since `--mixed` mode is used unconditionally for every Btrfs volume this tool creates, not just small ones. [Source: epics.md#Story 6.4, lines 854-856]
3. **Given** an existing XFS or Btrfs volume, **when** I run resize, **then** `growfs` uses `xfs_growfs` or `btrfs filesystem resize` respectively, selected from the same `filesystem` token field, never re-asked or sniffed. [Source: epics.md#Story 6.4, lines 858-860]
4. **Given** preflight for a create/resize targeting XFS or Btrfs, **when** it runs, **then** it checks presence of that filesystem's toolchain (`xfsprogs` or `btrfs-progs`) — only the toolchain the requested operation actually needs, not always both. [Source: epics.md#Story 6.4, lines 862-864]
5. **Given** an ext4 volume (today's default), **when** create or resize runs, **then** behavior is unchanged from before this story. [Source: epics.md#Story 6.4, lines 866-868]

## Tasks / Subtasks

- [x] **Task 0: Read every file this story touches before changing anything** (AC: all)
  - Read in full: `src/domain/types.rs` (`Filesystem` enum, currently `Ext4`-only), `src/domain/preflight.rs` (`check`, the AD-4 shared gate), `src/ports/filesystem_backend.rs` (full trait — `check_prerequisites`/`mkfs`/`growfs`/`filesystem_size` doc comments), `src/adapters/exec/mod.rs` — specifically `mkfs`, `growfs`, `filesystem_size`, `check_prerequisites` (the `FilesystemBackend` impl, ~line 1591), `read_filesystem` (the `LuksBackend` impl, ~line 1291), `enroll_fido2_key`'s `filesystem_name` match (~line 787), and `mount`/`umount`/`create_mount_point`/`invoking_identity` (the precedent for any mount-point handling), `src/domain/workflows/create.rs` (`run`'s single `preflight::check` call), `src/domain/workflows/resize.rs` (full file — tier-1/tier-2 grow-only check, `read_filesystem` call site, `preflight::check` call site), `src/domain/workflows/unlock.rs`/`close.rs`/`close_all.rs`/`revoke.rs`/`enroll.rs`/`slam.rs`/`info.rs` (each one's single `preflight::check` call site — mechanical, but every one must be found and updated), `src/cli/main.rs` (`CliFilesystem` enum + `From` impl, `CreateMode::File`/`Device`'s `filesystem` field, and all 8 direct `preflight::check` call sites in `run_unlock`/`run_enroll`/`run_revoke`/`run_close`/`run_close_all`/`run_slam`/`run_resize`/`run_info`), `src/cli/ux.rs` (`translate`'s substring-bucket chain — read the whole function, not just the top, to understand the marker-bleed guard pattern already in place for `resize2fs`/`e2fsck`/`umount`), `tests/unit/fakes.rs` (`FakeFilesystemBackend`'s `check_prerequisites` impl and `FakeLuksBackend`'s `read_filesystem`/`with_read_filesystem`), `tests/unit/preflight.rs` (all 3 existing tests), `tests/unit/workflows.rs` (the preflight-first regression tests, especially `resize_run_stops_at_preflight_before_touching_any_port`), `tests/unit/cli.rs` (the `--label`/`--scaffold-hooks`-in-help test pattern to mirror), `tests/hardware/main.rs` (the existing `resize_grows_a_file_backed_volume_preserving_data_and_keys`/`resize_grows_a_device_backed_volume_into_its_own_headroom` scenarios, as the pattern to mirror for XFS/Btrfs).
  - No spike needed for the core `Filesystem` enum extension — AD-8 already anticipated it and ARCHITECTURE-SPINE.md's AD-8 "Realized (Epic 6, CAP-22)" text already specifies the exact adapter commands. **One genuine design gap this story must close that the architecture spine does not fully resolve** — see "Load-Bearing Design Decision: XFS/Btrfs growfs and filesystem_size need a live mount" in Dev Notes below. Read that section before starting Task 4.

- [x] **Task 1: Extend the `Filesystem` enum** (AC: #1, #3)
  - `src/domain/types.rs`: add `Xfs` and `Btrfs` variants to `Filesystem`, alongside `Ext4`. Keep the existing `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` — no new derives needed. Update the enum's doc comment (currently says "v1 supports only ext4 (AD-8); additional variants are additive later") to reflect that this story is the "additive later."

- [x] **Task 2: Add `--filesystem xfs`/`btrfs` to the CLI** (AC: #1)
  - `src/cli/main.rs`: add `Xfs` and `Btrfs` variants to `CliFilesystem` (line ~296) and matching arms to `impl From<CliFilesystem> for Filesystem` (line ~300). Clap's `ValueEnum` derive auto-lowercases variant names to kebab-case, so `Xfs`/`Btrfs` become `--filesystem xfs`/`--filesystem btrfs` with no `#[value(name = ...)]` override needed — exactly matching AC #1's flag spelling. No other CLI change needed: `filesystem: CliFilesystem` is already a field on both `CreateMode::File`/`Device` and already threaded into `run_create`/`create::run` (Story 6.1/6.2/6.3 didn't touch this path). `resize` gets no new flag — AC #3 is explicit that the filesystem is read from the token, never re-asked (see Task 8).

- [x] **Task 3: `mkfs` — add Xfs/Btrfs adapter match arms** (AC: #1, #2)
  - `src/adapters/exec/mod.rs`, `mkfs` (~line 1740): add arms mirroring `Ext4`'s shape (`privileged(...)`, `.output()`, map to `DomainError::AdapterFailure` on spawn failure or non-success exit, same message format `"{tool} failed: {stderr}"`):
    - `Filesystem::Xfs => privileged("mkfs.xfs").arg("-f").arg(mapper.device_node())` — `-f` forces creation without an interactive "are you sure" prompt on any ambiguous existing-signature detection (same reasoning `mkfs.ext4` already uses `-F` for).
    - `Filesystem::Btrfs => privileged("mkfs.btrfs").args(["-f", "--mixed"]).arg(mapper.device_node())` — `--mixed` is **unconditional for every Btrfs volume this tool creates**, not size-gated (AC #2; ARCHITECTURE-SPINE.md AD-8's "Realized" text already made this exact call: mixed mode drops the viable minimum from standard mode's ~109 MiB floor to ~16 MiB, deliberately not adding a size-threshold branch nobody needs for this tool's small-cold-storage use case).
  - Real command syntax confirmed via current `mkfs.btrfs`/`xfs_growfs` documentation (2026-08-09): `--mixed` cannot be combined with other profile options and is creation-time-only (irrelevant here — this is the only mkfs call any volume ever gets), consistent with the architecture note.

- [x] **Task 4: `growfs`/`filesystem_size` — add Xfs/Btrfs adapter match arms via a shared transient-mount helper** (AC: #3)
  - **Read "Load-Bearing Design Decision" in Dev Notes first** — unlike ext4, XFS and Btrfs tools require a live mountpoint argument; there is no offline/unmounted equivalent for either.
  - Add a new private helper in `src/adapters/exec/mod.rs` (module-level function, not a trait method — never exposed to `domain`/`ports`):
    ```rust
    /// Mounts `mapper`'s device node at a private, transient mount point
    /// (never `/run/media/<user>` — that's `mount()`'s job for a volume the
    /// user is actively using; this one exists only for the duration of a
    /// single fs-tool invocation), runs `f` against the mountpoint, then
    /// always unmounts and removes the scratch directory before returning.
    /// XFS's `xfs_growfs`/`xfs_info` and Btrfs's `btrfs filesystem
    /// resize`/`usage` all require a live mountpoint argument — confirmed via
    /// their upstream docs, 2026-08-09 — unlike ext4's `resize2fs`/`dumpe2fs`,
    /// which operate on the raw device node unmounted. Error messages
    /// deliberately avoid the bare substrings "mount"/"umount" standing alone
    /// — see `cli::ux::translate`'s marker-bleed guard for why (Dev Notes).
    fn with_transient_mount<T>(
        mapper: &MapperHandle,
        f: impl FnOnce(&Path) -> Result<T, DomainError>,
    ) -> Result<T, DomainError> {
        let mountpoint = PathBuf::from(format!("/run/hypogaol-fsop-{}", mapper.name));
        if let Err(e) = std::fs::create_dir_all(&mountpoint) {
            return Err(DomainError::AdapterFailure(format!(
                "failed to prepare {} for a filesystem operation: {e}",
                mapper.device_node().display()
            )));
        }
        let mount_output = privileged("mount").arg(mapper.device_node()).arg(&mountpoint).output();
        match mount_output {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                let _ = std::fs::remove_dir(&mountpoint);
                return Err(DomainError::AdapterFailure(format!(
                    "failed to prepare {} for a filesystem operation: {}",
                    mapper.device_node().display(),
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
            Err(e) => {
                let _ = std::fs::remove_dir(&mountpoint);
                return Err(DomainError::AdapterFailure(format!(
                    "failed to prepare {} for a filesystem operation: {e}",
                    mapper.device_node().display()
                )));
            }
        }

        let result = f(&mountpoint);

        let umount_output = privileged("umount").arg(&mountpoint).output();
        let _ = std::fs::remove_dir(&mountpoint);
        match umount_output {
            Ok(output) if output.status.success() => result,
            Ok(output) => {
                let umount_err = DomainError::AdapterFailure(format!(
                    "failed to conclude a filesystem operation on {}: {}",
                    mapper.device_node().display(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
                result.and(Err(umount_err))
            }
            Err(e) => {
                let umount_err = DomainError::AdapterFailure(format!(
                    "failed to conclude a filesystem operation on {}: {e}",
                    mapper.device_node().display()
                ));
                result.and(Err(umount_err))
            }
        }
    }
    ```
    (`result.and(Err(umount_err))` deliberately surfaces the umount failure when `f` itself succeeded, but keeps `f`'s own error if `f` failed — matching this codebase's established "don't hide the real failure behind a cleanup failure" discipline, simplified from `create.rs`'s `with_rollback_cleanup_failure` pattern since this helper is adapter-internal only and never crosses into `domain`.)
  - `growfs` (~line 1763): add arms —
    - `Filesystem::Xfs => with_transient_mount(mapper, |mp| { let output = privileged("xfs_growfs").arg(mp).output()...; if success { Ok(()) } else { Err(AdapterFailure("xfs_growfs failed: ...")) } })` — no explicit size argument, matching `resize2fs`'s existing no-arg "grow to fill" convention; `xfs_growfs` defaults to growing the data section to fill the full underlying block device.
    - `Filesystem::Btrfs => with_transient_mount(mapper, |mp| { privileged("btrfs").args(["filesystem", "resize", "max"]).arg(mp)...})` — `max` grows to fill all remaining free space on the device, the Btrfs equivalent of `resize2fs`'s/`xfs_growfs`'s no-arg behavior (confirmed via `btrfs-filesystem(8)`, 2026-08-09).
  - `filesystem_size` (~line 1816): add arms, each parsing the tool's own reported total size (not used space) into bytes:
    - `Filesystem::Xfs => with_transient_mount(mapper, |mp| { run "xfs_info" on mp, parse the "data" line's "bsize=" and "blocks=" tokens, return bsize * blocks })` — `xfs_info <mountpoint>` (mounted) prints a `data     =    bsize=4096   blocks=131072, imaxpct=25` line; find the line whose first whitespace-separated token is `data`, then within it find the `bsize=` and `blocks=` substrings (strip a trailing comma off the `blocks=` value before parsing), mirroring `dumpe2fs`'s existing "Block count:"/"Block size:" line-scan style immediately above this in the same file.
    - `Filesystem::Btrfs => with_transient_mount(mapper, |mp| { run "btrfs filesystem usage --raw" on mp, parse the "Device size:" line's trailing byte count })` — `btrfs filesystem usage --raw <mountpoint>` prints a `Device size:  <bytes>` line with no unit suffix (confirmed via `btrfs-filesystem(8)`, 2026-08-09); find the line starting with `Device size:` and parse the remaining trimmed text as `u64`.
  - Every new arm maps subprocess-spawn failure and non-success exit to `DomainError::AdapterFailure`, same shape as every existing arm in these two functions.

- [x] **Task 5: `read_filesystem` and `enroll_fido2_key`'s token-write — round-trip Xfs/Btrfs through the LUKS2 token** (AC: #1, #3)
  - `src/adapters/exec/mod.rs`, `read_filesystem` (~line 1291): the `filesystem_str` match currently only accepts `"ext4"`. Add `"xfs" => Ok(Filesystem::Xfs)` and `"btrfs" => Ok(Filesystem::Btrfs)` arms, keeping the existing `other => Err(...)` catch-all for genuinely unrecognized values.
  - Same file, the `enroll_fido2_key` implementation's `filesystem_name` match (~line 787, currently `Filesystem::Ext4 => "ext4"`): add `Filesystem::Xfs => "xfs"` and `Filesystem::Btrfs => "btrfs"`. This is the single write side of the same round-trip `read_filesystem` reads back later (AD-2: written once by `create`, read by `resize`, never re-asked or sniffed — AC #3's literal requirement).

- [x] **Task 6: `check_prerequisites` becomes filesystem-aware** (AC: #4, #5)
  - `src/ports/filesystem_backend.rs`: change the trait signature to `fn check_prerequisites(&self, filesystem: Option<Filesystem>) -> Result<(), Vec<String>>;`. Update its doc comment to explain the parameter: `None` means "no mkfs/growfs toolchain needed for this operation" (every non-create/resize workflow); `Some(fs)` means "check exactly `fs`'s toolchain, not the others" (create passes the requested type, resize the existing one).
  - `src/adapters/exec/mod.rs`, `FilesystemBackend::check_prerequisites` (~line 1592): split the current flat binary list. Keep `["mount", "umount", "blockdev", "findmnt", "id", "fuser", "kill"]` unconditional (these are needed regardless of filesystem type — mount/umount/hooks/slam machinery). Move `"mkfs.ext4"`, `"resize2fs"`, `"e2fsck"`, `"dumpe2fs"` out of the unconditional list and gate them behind `filesystem`:
    ```rust
    match filesystem {
        None => {}
        Some(Filesystem::Ext4) => for binary in ["mkfs.ext4", "resize2fs", "e2fsck", "dumpe2fs"] { ... }
        Some(Filesystem::Xfs) => for binary in ["mkfs.xfs", "xfs_growfs", "xfs_info"] { ... }
        Some(Filesystem::Btrfs) => for binary in ["mkfs.btrfs", "btrfs"] { ... }
    }
    ```
    This is why AC #5 ("an ext4 volume — behavior unchanged") holds: `create`/`resize` targeting ext4 pass `Some(Filesystem::Ext4)`, producing the exact same missing-binary list as today. AC #4's "not always both" is satisfied structurally — `Some(Xfs)` can never mention `btrfs-progs` binaries and vice versa, and neither ever mentions `mkfs.ext4` unless ext4 is what's actually requested.

- [x] **Task 7: Thread `Option<Filesystem>` through `domain::preflight::check` and every workflow** (AC: #4, #5)
  - `src/domain/preflight.rs`: `check` gains a fourth parameter, `filesystem: Option<Filesystem>`, forwarded as `fs.check_prerequisites(filesystem)`. Update its doc comment.
  - `src/domain/workflows/create.rs` (~line 52): `filesystem: Filesystem` is already a required parameter of `create::run` — change the call to `preflight::check(luks, fido2, fs, Some(filesystem))?`. No ordering change; still the literal first statement.
  - `src/domain/workflows/resize.rs`: **two calls, not one — read "Load-Bearing Design Decision" in Dev Notes for why.**
    1. The existing call (~line 43) becomes `preflight::check(luks, fido2, fs, None)?` — unchanged position (still the literal first statement — this keeps `resize_run_stops_at_preflight_before_touching_any_port` passing with only the new argument added, no other change).
    2. Immediately after the existing `let filesystem = luks.read_filesystem(path)?;` line (~line 95, still in its current position — do **not** move it earlier), add `preflight::check(luks, fido2, fs, Some(filesystem))?;` — a second, narrower call that fails fast on a missing xfs/btrfs toolchain before `luks.open` spends a real FIDO2 touch. This is non-mutating and cheap (same "re-check, not a bypass" reasoning `run_unlock`'s doc comment already uses for its own CLI-level pre-check), and it satisfies AD-4's Epic-6 amendment text ("resize passes the requested/existing type") without disturbing tier-1's established "an obviously-invalid request never reaches a real adapter call" guarantee, which depends on nothing but `is_block_device`/`device_capacity`/stdlib-stat running before it.
  - `src/domain/workflows/unlock.rs`, `close.rs`, `close_all.rs`, `revoke.rs`, `enroll.rs`, `slam.rs`, `info.rs`: each has exactly one `preflight::check(luks, fido2, fs)` call — change every one to `preflight::check(luks, fido2, fs, None)`. Mechanical; the compiler will flag every site that's missed.
  - `src/cli/main.rs`: all 8 direct `preflight::check(&adapter, &adapter, &adapter)` calls (in `run_unlock`, `run_enroll`, `run_revoke`, `run_close`, `run_close_all`, `run_slam`, `run_resize`, `run_info`) become `preflight::check(&adapter, &adapter, &adapter, None)` — including `run_resize`'s, which cannot know the volume's filesystem type at that point (it hasn't read the token yet) and deliberately relies on `resize::run`'s own second, authoritative call to catch a missing xfs/btrfs toolchain. `run_create` has no direct `preflight::check` call today and needs none added — `create::run`'s own internal call already covers it.

- [x] **Task 8: Update fakes and preflight unit tests for the new signature** (AC: #4, #5)
  - `tests/unit/fakes.rs`, `FakeFilesystemBackend::check_prerequisites` (~line 714): change to `fn check_prerequisites(&self, filesystem: Option<Filesystem>) -> Result<(), Vec<String>>`. Log `"check_prerequisites"` to the existing `CallLog` (it currently logs nothing) and push `filesystem` onto a new field so a test can inspect every call's argument in order — add `check_prerequisites_filesystem_calls: RefCell<Vec<Option<Filesystem>>>` (initialized empty in both `passing()`/`failing()`) and `pub fn check_prerequisites_filesystem_calls(&self) -> Vec<Option<Filesystem>>` accessor, following this file's existing `last_scaffold_hook_templates_mountpoint`/`signal_calls` convention. Keep the existing canned `self.prerequisites.clone()` return behavior unchanged — the fake still doesn't need to vary its *result* by filesystem type, only record what it was asked.
  - `tests/unit/preflight.rs`: update all 3 existing calls to `preflight::check(&luks, &fido2, &fs)` → add a 4th argument (use `None` — the exact value doesn't matter to these 3 tests, which only exercise missing-binary aggregation). Add one new test proving the argument actually reaches the port: `preflight_forwards_the_filesystem_argument_to_check_prerequisites_unchanged`, e.g. `preflight::check(&luks, &fido2, &fs, Some(Filesystem::Xfs))`, then `assert_eq!(fs.check_prerequisites_filesystem_calls(), vec![Some(Filesystem::Xfs)])`.

- [x] **Task 9: Workflow-level tests proving resize's two-call preflight ordering** (AC: #3, #4)
  - `tests/unit/workflows.rs`: extend `resize_run_stops_at_preflight_before_touching_any_port` only by adding the new `None` argument if it constructs the call directly (it calls `resize::run(...)`, whose own public signature is unchanged — verify no edit is actually needed there beyond confirming it still passes).
  - Add a new test (in `tests/unit/resize.rs` if that file exists as its own module, otherwise alongside resize's other tests — check via Task 0's read which file currently hosts `resize::`-prefixed tests) proving both preflight calls happen with the right arguments in the right order: configure `FakeLuksBackend::passing().with_read_filesystem(Filesystem::Xfs)`, `FakeFilesystemBackend::passing()`, run `resize::run(...)` to completion (or far enough that both preflight calls have fired), then assert `fs.check_prerequisites_filesystem_calls() == vec![None, Some(Filesystem::Xfs)]` — proving both the unconditional first call and the type-specific second call, in that exact order, with the value `read_filesystem` actually reported (not a stand-in), the same "prove the real value flows through" discipline `key_label_received`/`last_scaffold_hook_templates_mountpoint` established in Stories 6.2/6.3.
  - Add a second test: `FakeFilesystemBackend::failing(&["mkfs.xfs"])` with `FakeLuksBackend::passing().with_read_filesystem(Filesystem::Xfs)` — assert `resize::run(...)` returns `Err(DomainError::PreflightFailed(_))` (proving the second, type-specific call is actually load-bearing, not just logged) and that `luks`'s `open`/`resize` never appear in its own `CallLog` (proving the failure aborts before the FIDO2 touch — check `FakeLuksBackend`'s `CallLog` accessor name via Task 0's read).

- [x] **Task 10: `ux::translate` — close the marker-bleed gap for the new transient-mount error paths** (AC: none directly — regression prevention, per this project's own recurring-pattern watchlist)
  - `src/cli/ux.rs`: the `with_transient_mount` helper's own error messages ("failed to prepare ... for a filesystem operation" / "failed to conclude a filesystem operation on ...") deliberately avoid the bare "mount"/"umount" substrings so they don't fall into the existing unlock-flavored `"mount"/"mkfs"` bucket ("Your volume unlocked, but Hypogaol couldn't mount its filesystem.") or the `"umount"/"findmnt"` bucket — both would be wrong here since these failures only ever originate from `resize`'s growfs/filesystem_size step, never from unlock. Add one new branch, checked **before** the generic `"mount"/"mkfs"` bucket (same reasoning as the existing `resize2fs`/`e2fsck`/`umount` branches immediately above it):
    ```rust
    if inner.contains("for a filesystem operation") {
        return "Hypogaol couldn't access this volume's filesystem to check or grow it.".to_string();
    }
    ```
  - Add a test mirroring `translates_adapter_failure_umount_failure_is_not_swallowed_by_the_unlock_mount_message`: `translates_adapter_failure_transient_mount_failure_is_not_swallowed_by_the_unlock_mount_message`, asserting a `DomainError::AdapterFailure("failed to prepare /dev/mapper/foo for a filesystem operation: some stderr".to_string())` produces the new message, not the unlock-flavored one.

- [x] **Task 11: Unit tests for the pure/mechanical additions** (AC: #1, #2)
  - `tests/unit/main_helpers.rs` or wherever `CliFilesystem`'s `From` impl would naturally be tested (check Task 0's read for the right file — likely alongside `parse_size`/`parse_label` tests in `tests/unit/cli.rs`, since `CliFilesystem`/`From<CliFilesystem>` live in `src/cli/main.rs`): if `CliFilesystem`/its `From` impl are `pub`/`pub(crate)` and reachable from the test binary, add a direct test; otherwise prove it indirectly via CLI parsing (see next bullet) — don't make anything more visible than it needs to be just to unit-test it directly.
  - `tests/unit/cli.rs`: add `create_file_help_lists_xfs_and_btrfs_as_filesystem_values` and a Device-backed equivalent, mirroring `create_file_help_lists_label_as_a_flag`: assert `help.contains("xfs")` and `help.contains("btrfs")` for `["hypogaol", "create", "file"/"device", "--help"]` (clap's `value_enum` help text lists all possible values, e.g. `[possible values: ext4, xfs, btrfs]`).

- [x] **Task 12: Update the remaining call sites** (compile correctness only, no new assertions required)
  - Any other test file constructing `preflight::check(...)` directly, or the real `ExecAdapter`'s `check_prerequisites()` with no argument, will fail to compile — fix every one the compiler flags, mirroring Story 6.2/6.3's Task 10 precedent. `mkfs`/`growfs`/`filesystem_size`'s own signatures (`fs: Filesystem`, not `Option`) are unchanged, so no call site passing a concrete `Filesystem` value needs updating for those three.

- [x] **Task 13: Hardware end-to-end tests for XFS and Btrfs** (AC: #1, #2, #3)
  - Extend `tests/hardware/main.rs` with new `#[ignore]`d scenarios, mirroring the existing `resize_grows_a_file_backed_volume_preserving_data_and_keys` shape:
    - `create_file_with_xfs_filesystem_succeeds_and_is_readable` / `create_file_with_btrfs_filesystem_succeeds_and_is_readable`: `create file --filesystem xfs`/`--filesystem btrfs`, then `unlock`, write a file, `close`, `unlock` again, confirm the file survived.
    - `create_file_with_btrfs_filesystem_at_a_small_size_succeeds` (AC #2 directly): create at ~20 MiB (near `MIN_VOLUME_SIZE_BYTES`'s ~16 MiB post-header payload) with `--filesystem btrfs`, confirm it succeeds — this is the case standard-mode Btrfs (~109 MiB floor) would fail, proving `--mixed` is actually taking effect, not just accepted as a no-op flag.
    - `resize_grows_a_file_backed_xfs_volume` / `resize_grows_a_file_backed_btrfs_volume`: create small, `resize` larger, confirm the grow succeeds and prior data survives — mirrors `resize_grows_a_file_backed_volume_preserving_data_and_keys`'s existing structure exactly, just with `--filesystem xfs`/`btrfs` at create time.
  - If no hardware is available in this session, state that explicitly (this project's standing convention — Stories 4.3, 5.1, 5.2, 6.1, 6.2, 6.3) and flag it as a retrospective action item for `LeReverandNox`, same pattern as those stories. This story's hardware verification is unusually load-bearing: the mount-requirement design decision in Task 4 has never been exercised against a real kernel XFS/Btrfs implementation until this runs.

- [x] **Task 14: Full regression pass**
  - `cargo build` succeeds and `make test` passes with all prior tests (baseline **224 total: 17 lib + 207 `tests/unit`**, verified by direct `make test` output at this story's `baseline_commit` — not from memory; Story 6.3's Completion Notes claimed 223 (17+206), one short of what this story's own verification found, consistent with the recurring self-reported-count-discrepancy pattern flagged below) plus this story's new tests green. Verify the exact new total from real command output.
  - `cargo fmt --check` and `cargo clippy --all-targets` both clean. `preflight::check` gains a parameter (4 call sites in `domain/workflows/*` become 5-arg-adjacent calls internally, no signature growth on the workflow functions themselves — their own public signatures are untouched, only their internal `preflight::check` call sites change) and `resize::run`/`create::run`'s own argument counts are **unchanged** by this story (no new parameters added to either) — if a new `too_many_arguments` warning appears anywhere, note it explicitly in Completion Notes rather than silently suppressing it.
  - If hardware is available: run Task 13's scenarios end-to-end. If not, flag per Task 13's note.

## Dev Notes

- **No new port, no new architectural layer.** This story is a pure extension of the existing `FilesystemBackend` port (AD-8) and the existing `domain::preflight` gate (AD-4) — exactly as ARCHITECTURE-SPINE.md's Epic 6 framing promises ("no new port and no new architectural layer... every capability slots onto Epics 1-4's existing `create`/`FilesystemBackend`/`Fido2Backend` surface"). [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md:38]

- **`Filesystem` gains `Xfs`/`Btrfs` exactly as AD-8 pre-specified**, including the exact adapter commands (`mkfs.xfs`, `mkfs.btrfs --mixed` unconditionally, `xfs_growfs`, `btrfs filesystem resize`) and the Btrfs mixed-mode minimum-size rationale (~16 MiB vs. standard mode's ~109 MiB floor, deliberately unconditional — this tool's use case is small cold-storage volumes, not a size-threshold decision to revisit). [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md:89]

- **AD-4's Epic-6 amendment is the load-bearing spec for Task 6/7**: "`preflight` takes an `Option<Filesystem>` and checks only the mkfs/growfs toolchain the operation actually needs (`create`/`resize` pass the requested/existing type; other workflows pass `None`, unchanged)." [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md:65]

- **Load-Bearing Design Decision: XFS/Btrfs `growfs`/`filesystem_size` need a live mount — the architecture spine does not spell this out, and it is not optional.** Confirmed via `xfs_growfs`/`xfs_info`/`btrfs-filesystem(8)` documentation (2026-08-09): unlike ext4's `resize2fs`/`dumpe2fs`, which operate directly on an unmounted block device node, **XFS and Btrfs have no offline grow or offline size-query equivalent** — `xfs_growfs`/`xfs_info` and `btrfs filesystem resize`/`usage` all take a *mountpoint*, not a device, and require the filesystem to actually be mounted. This workflow (`resize::run`) never mounts anything today (see `grow_open_mapping`'s own doc comment: "this workflow never mounts the filesystem"). Task 4's resolution keeps this entirely inside `adapters::exec` via a new private `with_transient_mount` helper: `growfs`/`filesystem_size`'s Xfs/Btrfs arms each mount to a throwaway, non-`/run/media` scratch path, run the tool, then unmount — invisible to `domain`, so AD-10's resize ordering (backing file → LUKS2 mapping → filesystem) and its "never mounts" framing stay conceptually true at the `domain` level; the mount is a private implementation detail of two individual port-method calls, not a workflow-level state change. Each of `growfs`/`filesystem_size` mounts and unmounts independently (two separate transient mounts per resize of an Xfs/Btrfs volume, not one shared session) — simpler and safer than threading mount state between two otherwise-independent port calls, and resize is not a hot path where this matters.

- **Why resize calls `preflight::check` twice, not once, unlike every other workflow.** AD-4's base rule requires `preflight` to run as "the *first statement*" of every `domain::workflows::*` function — and `resize_run_stops_at_preflight_before_touching_any_port` (`tests/unit/workflows.rs`) encodes exactly that for `resize`, using `FakeLuksBackend::failing(&["cryptsetup"])` and asserting the very first thing that goes wrong is `PreflightFailed`, with no other port method called first. But AD-4's Epic-6 amendment separately requires resize to pass the *existing* filesystem type — which can only be known by calling `luks.read_filesystem(path)`, an adapter call. These two requirements can't both be satisfied by a single call in a single position: hoisting `read_filesystem` (and the tier-1 grow-only checks ahead of it) to before `preflight` would break the existing regression test (a `/tmp/does-not-matter`-style path with no `cryptsetup` at all would now hit a raw `AdapterFailure` from tier-1's own file-stat instead of a clean `PreflightFailed`) and would reintroduce the exact "an obviously-invalid request reaches a real adapter call" problem a 2026-07-26 review finding already fixed for tier-1. The resolution: keep the existing unconditional `preflight::check(..., None)` exactly where it is (first statement, unchanged position, satisfies the base rule and the existing test verbatim), and add a second, narrower `preflight::check(..., Some(filesystem))` immediately after the existing `read_filesystem` call — still well before `luks.open`'s real FIDO2 touch, still non-mutating, still "a cheap, side-effect-free re-check, not a bypass" (the same phrase `run_unlock`'s own doc comment already uses to justify a similar early-and-authoritative double-check pattern between `cli` and `domain`).

- **Marker-bleed guard, proactively applied (Task 10).** This project's Epic 2 retro flagged, and its Epic 6 sprint status still carries as an open/in-progress action item, "when a story adds a new workflow reusing shared `adapters::exec` error strings, explicitly check for marker-bleed into other workflows' plain-language translations before merge (3 real bugs from this pattern across 2 stories so far)." `with_transient_mount`'s errors are exactly this shape: if worded with a bare "mount"/"umount", they would silently and wrongly render as `run_unlock`'s "Your volume unlocked, but Hypogaol couldn't mount its filesystem" message even though they only ever fire from `resize`. Task 4 and Task 10 close this proactively, before merge, rather than waiting for it to be caught in review a fourth time. [Source: _bmad-output/implementation-artifacts/sprint-status.yaml, epic 2 action item, status in-progress]

- **`mkfs.xfs -f` / `mkfs.btrfs -f` mirror `mkfs.ext4`'s existing `-F`** — this codebase already force-flags every `mkfs` call so a non-interactive process never hangs on an "are you sure" prompt (`mkfs.ext4` uses `-F` today); `-f`/`--force` are the equivalent flags for `xfsprogs`/`btrfs-progs`. Not strictly required for correctness here (every `mkfs` call in this codebase targets a mapper device that was just freshly `luksFormat`-ed, with no prior filesystem signature to detect), but matches established convention and removes any risk of an interactive prompt appearing on a device with residual signature bytes from an earlier crash-resumed attempt.

- **No new `CreateStage`/`ResizeStage` variants needed.** `CreateStage::CreatingFilesystem` and `ResizeStage::GrowingFilesystem` are already filesystem-type-agnostic (they fire around the `mkfs`/`growfs` port calls regardless of which `Filesystem` variant is in play) — do not add a new stage for this story.

- **`resize.rs`'s tier-2 block-flooring constant (`EXT4_BLOCK_SIZE_BYTES = 4096`) is reused unchanged for Xfs/Btrfs, deliberately, not overlooked.** It floors `new_size_as_payload` to the largest whole block `growfs` could actually reach, generically, for whichever filesystem is in play. 4096 bytes is also XFS's and Btrfs's default block/sector size on the platforms this tool targets, so the existing constant stays correct for all three types without renaming or parameterizing it — do not introduce a per-filesystem block-size lookup for this story; if a future filesystem needs a different value, that's that filesystem's story to handle, not this one's.

- **No new `DomainError` variant needed.** Every new failure path (missing xfs/btrfs toolchain, `mkfs.xfs`/`mkfs.btrfs` failure, transient-mount failure, `xfs_growfs`/`btrfs filesystem resize` failure, `xfs_info`/`btrfs filesystem usage` parse failure) maps to the existing `DomainError::AdapterFailure`/`PreflightFailed`, same as every other adapter-level failure in this codebase.

- **Recurring review-pattern watchlist from Epic 4/5/6 retros — apply proactively:**
  - Self-reported completion-note claims not matching actual grep/build/test output — verify every test-count claim in Task 14's Completion Notes against real `make test` output, not memory (this story's own baseline count above was itself corrected this same way against Story 6.3's claim).
  - New pure parsing/logic functions shipping without a direct unit test — this story's `xfs_info`/`btrfs filesystem usage --raw` output parsers are exactly this shape, but they live inside real subprocess-calling adapter code with no existing unit-test precedent for their ext4 siblings (`dumpe2fs` parsing) either — per AD-7's testing strategy, this class of adapter-internal parsing is proven by the hardware suite (Task 13), not a unit test with canned stdout. Do not invent a unit test that fakes subprocess output just to satisfy this watchlist item; that would test a hand-written fixture string, not the real parser's behavior against real tool output.
  - Hardware verification convention: state explicitly in Completion Notes whether real hardware was available and what was/wasn't verified on it (Stories 4.3, 5.1, 5.2, 6.1, 6.2, 6.3). This story's Task 4 mount-based design is unusually load-bearing on real hardware — flag prominently if it could not be verified.

### Previous Story Intelligence (Story 6.3)

- Baseline test count going into this story, verified fresh against this story's own `baseline_commit` (not carried over from 6.3's own claim, which was off by one — see Task 14): **224 total (17 lib + 207 `tests/unit`)**.
- Story 6.3 confirmed the compiler is the authoritative source for "which call sites need updating" whenever a shared function gains a parameter (`create::run`'s `scaffold_hooks` addition needed updates in `tests/unit/workflows.rs` and `tests/hardware/main.rs` beyond what its own task list originally enumerated) — this story's `preflight::check` signature change touches even more call sites (8 CLI + 8 domain-workflow + 3 preflight-test), so treat Task 12 the same way: fix every compiler error, don't trust this story's own enumeration as exhaustive.
- Story 6.3 also reconfirmed the "self-reported completion-note claims not matching real output" pattern a second time (its own 209→212 and 209-vs-actual-212 baseline correction, and its arg-count correction from 6.2's claimed 8/9/8/7 to the real 9/10/9/8) — this story's Task 14 explicitly re-verifies rather than trusting 6.3's own count.
- Story 6.3's hardware environment had a real FIDO2 key present (`fido2-token -L` enumerates one device) — expect the same for this story's Task 13, but note Task 13 needs no *interactive* touch/PIN beyond what `create`/`resize`/`unlock` already require elsewhere (no new FIDO2 interaction is introduced by XFS/Btrfs support itself).

### Project Structure Notes

- Files touched (production): `src/domain/types.rs` (`Filesystem` gains `Xfs`/`Btrfs`), `src/domain/preflight.rs` (`check` gains `filesystem: Option<Filesystem>`), `src/ports/filesystem_backend.rs` (`check_prerequisites` gains the same parameter), `src/adapters/exec/mod.rs` (`mkfs`/`growfs`/`filesystem_size`/`check_prerequisites`/`read_filesystem`/`enroll_fido2_key`'s filesystem-name match all gain Xfs/Btrfs arms; new private `with_transient_mount` helper), `src/domain/workflows/create.rs` (1 call site), `src/domain/workflows/resize.rs` (1 call site becomes 2), `src/domain/workflows/unlock.rs`/`close.rs`/`close_all.rs`/`revoke.rs`/`enroll.rs`/`slam.rs`/`info.rs` (1 call site each), `src/cli/main.rs` (`CliFilesystem` gains 2 variants + `From` arms; 8 `preflight::check` call sites), `src/cli/ux.rs` (1 new branch in `translate`).
- Files touched (tests): `tests/unit/fakes.rs` (`FakeFilesystemBackend::check_prerequisites` signature + new capture field/accessor), `tests/unit/preflight.rs` (3 call sites updated + 1 new test), `tests/unit/workflows.rs`/wherever resize-specific tests live (2 new tests), `tests/unit/cli.rs` (2 new help-text tests), `tests/unit/ux.rs` (1 new marker-bleed test), `tests/hardware/main.rs` (5 new `#[ignore]`d scenarios).
- No new files, no new modules, no new port, no `CreateTarget`/`KeyMetadata` shape changes. `resize`/`create`'s own public function signatures (parameter lists) are unchanged by this story — only their internal `preflight::check` call sites change.
- Alignment with the documented source tree: `ARCHITECTURE-SPINE.md`'s Stack table already lists `xfsprogs`/`btrfs-progs` as "preflight (AD-4) checks presence only when `Filesystem::Xfs`/`Filesystem::Btrfs` is requested," and its Structural Seed already lists `filesystem_backend.rs`'s port "parameterized by Filesystem enum (AD-8: Ext4/Xfs/Btrfs, CAP-22)" and `main.rs`'s `--filesystem xfs/btrfs (CAP-22)` flag — this story implements exactly what the spine already scoped. The one gap the spine leaves for this story to resolve on its own is the mount-requirement design decision above; consider proposing an AD-10 amendment note (mirroring how AD-9 carries its own "Amended"/"Realized" annotations) once this story lands, so a future reader of AD-10 doesn't rediscover the same gap.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 6.4: XFS and Btrfs Filesystem Support, lines 842-868] — acceptance criteria origin, verbatim.
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 6: Volume Resilience, Filesystem Choice & Everyday Polish, lines 766-768] — epic-level framing; confirms no new port/layer for any Epic 6 story.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-4 — Mandatory shared pre-flight gate, lines 60-65] — the `Option<Filesystem>` preflight amendment this story implements verbatim.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-8 — FilesystemBackend port, parameterized by filesystem type, lines 85-89] — the exact adapter commands and Btrfs mixed-mode sizing rationale this story implements.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-10 — Resize ordering and grow-only enforcement, lines 107-111] — the domain-level ordering this story's Task 4 design decision must not disturb (unchanged by this story; the mount stays adapter-internal).
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#Stack, lines 218-219] — `xfsprogs`/`btrfs-progs` conditional-preflight table entries.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#Structural Seed, lines 240, 244] — `filesystem_backend.rs`/`main.rs`'s documented inventory already lists this story's additions by name.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#Capability-to-component map, line 298] — `CAP-22 (XFS/Btrfs) | FilesystemBackend, adapters::exec | AD-4, AD-8`.
- [Source: _bmad-output/specs/spec-tomb-fido2/SPEC.md#CAP-22, lines 103-105] — intent/success framing, including the ~18 MiB Btrfs-mixed-mode figure (this story's own web research on 2026-08-09 found ~16 MiB via current upstream docs — a minor, non-load-bearing discrepancy from SPEC.md's earlier estimate; not worth reconciling, both comfortably clear this tool's 32 MiB `MIN_VOLUME_SIZE_BYTES` floor).
- [Source: src/domain/workflows/resize.rs] — full current implementation, read in full during story creation; the exact tier-1/tier-2 grow-only check structure and `read_filesystem`/`preflight::check` call sites this story's Task 7 and its Dev Notes design decision are built around.
- [Source: src/domain/workflows/create.rs] — confirms `filesystem: Filesystem` is already a `create::run` parameter, threaded unchanged since Epic 1; only the `preflight::check` call site needs updating.
- [Source: src/adapters/exec/mod.rs] — `mkfs`/`growfs`/`filesystem_size`/`check_prerequisites`/`read_filesystem`/`enroll_fido2_key`'s current ext4-only implementations, read in full during story creation; the exact precedent every new match arm mirrors.
- [Source: src/cli/ux.rs] — `translate`'s existing marker-bleed guard branches (`resize2fs`, `e2fsck`, `umount`/`findmnt`), read in full during story creation; the direct precedent Task 10's new branch follows.
- [Source: tests/unit/workflows.rs] — `resize_run_stops_at_preflight_before_touching_any_port`, the existing regression test this story's Dev Notes design decision is built to keep passing unmodified in position.
- [Source: tests/unit/fakes.rs] — `FakeLuksBackend::with_read_filesystem`, `FakeFilesystemBackend`'s existing `CallLog`/capture-field conventions (`last_scaffold_hook_templates_mountpoint`, `signal_calls`), the direct precedent for this story's new capture field.
- [Source: tests/unit/cli.rs, lines 211-220] — existing `--label`-in-help assertion pattern mirrored for the new `--filesystem` possible-values tests.
- [Source: tests/hardware/main.rs] — `resize_grows_a_file_backed_volume_preserving_data_and_keys`/`resize_grows_a_device_backed_volume_into_its_own_headroom`, the direct pattern this story's new hardware scenarios mirror.
- [Source: _bmad-output/implementation-artifacts/6-3-hook-template-scaffolding-at-create.md] — previous story in this epic; confirms baseline test/hardware-test counts (corrected in this story after re-verification) and the hardware-verification-statement convention.
- [Source: _bmad-output/implementation-artifacts/sprint-status.yaml] — confirms this is the fourth story of Epic 6 (epic already `in-progress` since Story 6.1) and the still-open Epic 2 marker-bleed watchlist action item this story's Task 10 addresses proactively.
- xfs_growfs(8)/xfs_info(8)/btrfs-filesystem(8) upstream documentation — web-verified 2026-08-09: XFS/Btrfs grow and size-query tools require a live mountpoint argument (no offline equivalent), `btrfs filesystem resize max <path>` grows to fill all available device space, `btrfs filesystem usage --raw <path>` reports a parseable `Device size: <bytes>` line, `mkfs.btrfs -f`/`mkfs.xfs -f` force-create without an interactive prompt — the direct source for Task 4's design decision and exact command syntax.

## Dev Agent Record

### Agent Model Used

Claude Sonnet 5 (claude-sonnet-5), via Amelia (bmad-dev-story workflow).

### Debug Log References

None — no HALT conditions encountered; all tasks completed straight through.

### Completion Notes List

- Tasks 1–7 (production code) are compiler-forced into a single build-green unit: extending `Filesystem` with `Xfs`/`Btrfs` makes every existing exhaustive `match` on it (adapter `mkfs`/`growfs`/`filesystem_size`/`read_filesystem`/`enroll_fido2_key`'s filesystem-name match, `CliFilesystem`'s `From` impl) fail to compile until every arm is added — there is no intermediate state where only e.g. Task 1 lands and `cargo build` still passes. Implemented and verified as one unit rather than as 7 separately-buildable commits.
- Task 8's literal instruction ("log `check_prerequisites` to the existing CallLog") had a wider ripple than its own task description implies: `preflight::check` calls `fs.check_prerequisites` as the *first* thing every workflow does, and many existing tests share one `CallLog` across all three fake ports via `.with_log(log.clone())` — so this one logging change broke 54 pre-existing test assertions across 9 test files (`close.rs`, `close_all.rs`, `create.rs`, `enroll.rs`, `info.rs`, `resize.rs`, `revoke.rs`, `slam.rs`, `unlock.rs`) that asserted exact call-log sequences or `log.borrow().is_empty()`. All 54 were fixed by prepending/inserting `"check_prerequisites"` at the correct position(s) in each expected sequence (resize gets two insertions — one per preflight call). Re-verified every fix against real `cargo test` output, not by inspection.
- Task 9's second test (`resize_aborts_before_opening_when_preflight_finds_a_missing_toolchain`) is named to describe its actual observable behavior rather than the task's own "proves the second call is load-bearing" framing: `FakeFilesystemBackend`'s `check_prerequisites` returns the same canned `Result` regardless of the `filesystem` argument it's given, so with `.failing(&["mkfs.xfs"])` the *first* (unconditional, `None`) preflight call already fails — the test still correctly proves a missing toolchain aborts resize before `luks.open`, just not specifically via the second call in isolation (the fakes have no mechanism to make only the second call fail while the first passes).
- Task 14 baseline re-verified directly against this story's own `baseline_commit` (25c655d) via `make test`: **224 total (17 lib + 207 tests/unit)** — confirmed, not from memory. Final count after this story: **230 total (17 lib + 213 tests/unit)**, +6 new tests (`preflight.rs` +1, `ux.rs` +1, `resize.rs` +2, `cli.rs` +2).
- `cargo fmt --check` and `cargo clippy --all-targets` both clean. `cargo clippy --all-targets` reports 5 `too_many_arguments` warnings (8/7, 10/7, 11/7, 10/7, 9/7 in `run_create`/`create::run`/`bootstrap_and_provision`/`finish_provisioning`/`grow_open_mapping`) — confirmed via `git stash`/re-run against the unmodified `baseline_commit` tree that all 5 are pre-existing and unchanged by this story (`create::run`/`resize::run`'s own public argument counts are untouched, per Task 14's own note; `preflight::check`'s new 4th argument only affects its own internal call sites, not any workflow function's signature).
- Hardware verification: this session itself could not run `make test-hardware` (no interactive sudo, no physical FIDO2 touch), but `LeReverandNox` ran the 5 new scenarios directly afterward and found two real issues, both now fixed and re-verified via `cargo build`/`make test` (not yet re-confirmed on hardware — see below):
  - `mkfs.xfs` refuses any filesystem at or below 300MB ("Filesystem must be larger than 300MB") — a hard XFS floor this story didn't previously know about. The two XFS hardware scenarios used 64/128 MiB volumes (fine for ext4/Btrfs, far under XFS's real minimum); bumped to 400/700 MiB (test-only fix).
  - `growfs`'s Btrfs arm's `btrfs filesystem resize max <mountpoint>` failed with a real `Invalid argument`. First diagnosis (targeting the device explicitly via `1:max`, based on a documented btrfs-progs devid-resolution pattern) was **wrong** — re-running with the fix still failed identically. A second, isolated repro (plain loop device, no LUKS at all, `btrfs filesystem resize` to an explicit absolute size instead of `max`) found the real cause: `btrfs-progs` itself warns "the new size ... is < 256MiB, this may be rejected by kernel," and the kernel does reject it — Btrfs's resize ioctl refuses any resize whose *resulting* size is under 256 MiB, a real floor separate from and much higher than `mkfs.btrfs --mixed`'s own creation-time minimum (~16 MiB). The `1:max` change is reverted (see the follow-up scope addition below for the actual fix).
  - `create_file_with_xfs_filesystem_succeeds_and_is_readable`/`create_file_with_btrfs_filesystem_succeeds_and_is_readable`/`create_file_with_btrfs_filesystem_at_a_small_size_succeeds` passed as originally written; only the two XFS/Btrfs-resize scenarios needed fixes.
  - Still flagging as a retrospective action item for `LeReverandNox`: re-run `resize_grows_a_file_backed_xfs_volume` and `resize_grows_a_file_backed_btrfs_volume` once more against the fixes below to close the loop — this AI agent has no way to verify either against real hardware itself.
- Follow-up scope addition #1 (still within this story, PR not yet merged): the XFS-too-small failure above surfaced a real gap — `create::run` only ever checked the generic `MIN_VOLUME_SIZE_BYTES` (32 MiB) floor, so a size between that and XFS's real ~300MB minimum previously sailed through preflight, LUKS formatting, and FIDO2 enrollment before failing at `mkfs.xfs` with a message `ux::translate`'s existing mount/mkfs bucket renders inaccurately (it implies an unlock failed, when this happens during `create`). Added `MIN_XFS_VOLUME_SIZE_BYTES` (350 MiB total, ~334 MiB post-header payload) and a `size_floor_for(filesystem)` helper in `src/domain/workflows/create.rs`, checked in both the File and Device branches before any adapter call — same position and discipline as the existing generic-floor check. Reuses `DomainError::DeviceTooSmall` rather than adding a new variant. 3 new unit tests in `tests/unit/create.rs` (XFS File-branch refusal, XFS Device-branch refusal, and a regression test proving Btrfs at the generic floor is unaffected).
- Follow-up scope addition #2 (still within this story, PR not yet merged): the Btrfs resize "Invalid argument" failure, once correctly diagnosed as a real 256 MiB kernel floor (not a devid-resolution issue), is the resize-time analog of scope addition #1 — a real user growing a small Btrfs volume (this tool's whole point per AC #2) to anything still under ~272 MiB total would hit this exact opaque failure. Added `MIN_BTRFS_RESIZE_PAYLOAD_BYTES` (260 MiB, with margin above the literal 256 MiB) in `src/domain/workflows/resize.rs`, checked in `grow_open_mapping` right after the existing grow-only comparison — that's the earliest point the exact post-header payload size is known without guessing at the LUKS2 header size, since it reuses the same `header_size`/`new_size_as_payload` values the grow-only check itself already computes. No equivalent floor exists for ext4/XFS: XFS's own floor only ever matters at create time, since resize is grow-only and can never shrink a volume back below a floor it already cleared at creation. Reuses `DomainError::DeviceTooSmall`. 2 new unit tests in `tests/unit/resize.rs` (Btrfs refusal before any mutating call, and a regression test proving ext4 at the same target size is unaffected). Also reverted the `1:max` adapter change (see above) and bumped the Btrfs resize hardware test's `grown_size` to 320 MiB (payload ~304 MiB, clears the floor).
- Final test count after both follow-up additions: **235 total (17 lib + 218 tests/unit)**, up from the original story's 230 (+5 more: 3 from addition #1, 2 from addition #2). `cargo fmt --check`/`cargo clippy --all-targets` re-verified clean after both additions, same 5 pre-existing warnings, no new ones.

### File List

**Production:**
- `src/domain/types.rs`
- `src/domain/preflight.rs`
- `src/ports/filesystem_backend.rs`
- `src/adapters/exec/mod.rs`
- `src/domain/workflows/create.rs`
- `src/domain/workflows/resize.rs`
- `src/domain/workflows/unlock.rs`
- `src/domain/workflows/close.rs`
- `src/domain/workflows/close_all.rs`
- `src/domain/workflows/revoke.rs`
- `src/domain/workflows/enroll.rs`
- `src/domain/workflows/slam.rs`
- `src/domain/workflows/info.rs`
- `src/cli/main.rs`
- `src/cli/ux.rs`

**Tests:**
- `tests/unit/fakes.rs`
- `tests/unit/preflight.rs`
- `tests/unit/resize.rs`
- `tests/unit/cli.rs`
- `tests/unit/ux.rs`
- `tests/unit/close.rs`
- `tests/unit/close_all.rs`
- `tests/unit/create.rs`
- `tests/unit/enroll.rs`
- `tests/unit/info.rs`
- `tests/unit/revoke.rs`
- `tests/unit/slam.rs`
- `tests/unit/unlock.rs`
- `tests/hardware/main.rs`

**Sprint tracking:**
- `_bmad-output/implementation-artifacts/sprint-status.yaml`
