---
baseline_commit: 714c282c6597735a5c8ed5c901b4e5481020e35c
---

# Story 5.2: Domain Term Rename (tomb → volume)

Status: in-progress

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a contributor reading or modifying the codebase,
I want the encrypted-container concept called "volume" everywhere instead of "tomb",
so that the domain vocabulary is generic and independent of whatever the product happens to be branded as.

## Acceptance Criteria

1. **Given** every `src/` module, `tests/unit`/`tests/hardware` file, and their identifiers/comments/error strings that currently say "tomb" as the domain noun, **when** Story 5.2 lands, **then** all of them read "volume" instead, with no change in behavior.
2. **Given** the full rename, **when** `cargo build` and `make test` are run afterward, **then** both pass with zero behavioral difference from before the rename; `make test-hardware` should also be run manually at least once against real hardware before considering this story done, since it is the only suite that exercises the renamed temp-file/mapper-name fixtures end-to-end.
3. **Given** README body text and `_bmad-output/specs/spec-tomb-fido2/hooks.md`, **when** they reference the container concept, **then** they also say "volume", consistent with the code.
4. **Given** historical planning docs (`SPEC.md`, `epics.md` Epics 1-4, `ARCHITECTURE-SPINE.md`, all four epic retros) and `CHANGELOG.md`'s historical entries, **when** Story 5.2 is scoped, **then** none of them are rewritten — they remain frozen historical record per AD-13's existing continuity exception, and CHANGELOG.md isn't in this story's file list at all.
5. **Given** the two prior product-name items Story 5.1 deferred here (`src/adapters/exec/mod.rs`'s temp-file literals at lines ~251/2251, and `tests/unit/ux.rs`'s matching fixture at line ~184, plus `tests/unit/mapping_name.rs`'s nonexistent-path fixture and every `tomb-fido2-hardware-test*` literal in `tests/hardware/main.rs`), **when** Story 5.2 lands, **then** each embedded "tomb" token in those literals reads "volume" too (mechanical `tomb` → `volume`, keep the adjacent `fido2`/`unit-test`/`hardware-test` tokens as-is).
6. **Given** `dyne/tomb` references anywhere (README lines ~3, 28, 36; `hooks.md`'s references to `dyne/tomb`'s hook model and manpage URL) and `src/adapters/exec/mod.rs:122`'s doc-comment mention of the old crate name `` `tomb_fido2` `` (a product-name reference Story 5.1 deliberately left alone, not the domain noun), **when** Story 5.2 is scoped, **then** none of them are touched — they are not the domain concept this story renames.

## Tasks / Subtasks

- [ ] **Task 0: Read every file this story touches before changing anything** (AC: #1, #3, #5)
  - Read in full: every file listed in Task 1-6 below. Re-grep `-i "tomb"` in each immediately before editing it — the counts below are a snapshot from story creation and may have drifted.
  - Note the one cross-file trap up front: `MIN_TOMB_SIZE_BYTES` (defined in `src/domain/workflows/create.rs:32`) is a `pub const` consumed in `src/cli/main.rs`, `tests/unit/create.rs`, `tests/unit/cli.rs`, and `tests/unit/progress.rs`. Renaming it to `MIN_VOLUME_SIZE_BYTES` requires touching all five files atomically in the same pass or `cargo build`/`make test` won't compile in between.

- [x] **Task 1: Rename in `src/domain/` (workflows, hooks, errors, ports)** (AC: #1)
  - Files: `src/domain/workflows/{create,unlock,enroll,revoke,close,resize,slam,close_all}.rs`, `src/domain/hooks.rs`, `src/domain/errors.rs`, `src/domain/keyslot_guard.rs`, `src/ports/{luks_backend,filesystem_backend}.rs`.
  - Rename the domain-noun "tomb" wherever it appears in doc comments, inline comments, and error-message strings (e.g. `src/domain/errors.rs`'s `"resolved size {size} bytes for {} is too small for a viable tomb"` and `"no FIDO2 key labeled {0:?} is enrolled on this tomb"`).
  - Identifier renames in this layer (not just prose — these are real symbols, rename every use site):
    - `src/domain/workflows/create.rs:32`: `MIN_TOMB_SIZE_BYTES` → `MIN_VOLUME_SIZE_BYTES` (see Task 0's cross-file note).
    - `src/domain/hooks.rs`: the `tomb_root` parameter of the bind-hook containment-check function, its local `canonical_tomb_root`, and the `BindHookSkipReason::SourceEscapesTombRoot` enum variant (line ~31, constructed at line ~139/144) → `volume_root`, `canonical_volume_root`, `SourceEscapesVolumeRoot`. This variant is matched on in `src/cli/ux.rs:114` (Task 2) — rename both sides together.
    - `src/domain/workflows/close.rs:93,115`: local `tomb_name` → `volume_name`.

- [x] **Task 2: Rename in `src/cli/main.rs` and `src/cli/ux.rs`** (AC: #1)
  - `src/cli/main.rs`: every `--help`/doc-comment mention of "tomb" as the container noun (subcommand docs, arg docs), every `println!`/`format!` user-facing string ("Creating tomb...", "Tomb created at...", "Tomb unlocked...", "Closing this tomb.", "No tombs are currently open.", "Growing this tomb...", "Tomb grown to...", etc.), and the `MIN_TOMB_SIZE_BYTES` import/uses from Task 1.
    - `create_mount_point`'s `tomb_name` parameter (`src/adapters/exec/mod.rs`, see Task 4) is a separate identifier from anything in `main.rs` — don't conflate the two while renaming.
  - `src/cli/ux.rs`: every user-facing error/status string containing "tomb" (lines ~22, 26, 30, 42, 51, 65, 85, 114-126), including the `BindHookSkipReason::SourceEscapesTombRoot` match arm (rename to match Task 1's enum-variant rename) and its associated message `"its source path escapes the tomb"` → `"its source path escapes the volume"`. Leave `ux.rs:274`'s comment referencing `` `MIN_TOMB_SIZE_BYTES` `` in sync with Task 1's rename too.

- [x] **Task 3: Rename in `src/adapters/exec/mod.rs`** (AC: #1, #5)
  - Doc comments and the `create_mount_point` function: rename its `tomb_name` parameter (line ~161) and every call-site local named `tomb_name` (lines ~163, 171, 1715, 1761) to `volume_name`. Update the doc comment above it (line ~157, "Creates a fresh directory named `tomb_name`...").
  - Test-only fixture literals (AC #5, deferred from Story 5.1): line ~251's `.tomb-fido2-bootstrap-{suffix_hex}` → `.volume-fido2-bootstrap-{suffix_hex}`; line ~2251's `tomb-fido2-unit-test-{name}-{suffix}` → `volume-fido2-unit-test-{name}-{suffix}`.
  - Test function names containing `_tomb` (e.g. `cryptsetup_status_field_prefers_loop_over_device_for_file_backed_tomb`, `..._device_backed_tomb`) → rename the `_tomb` suffix to `_volume`.
  - **Leave untouched:** line ~122's doc comment mentioning the old crate name `` `tomb_fido2` `` (AC #6 — a product-name reference already resolved/carved-out by Story 5.1, not this story's domain-noun target).

- [x] **Task 4: Rename across `tests/unit/*.rs`** (AC: #1, #5)
  - Files (grep count of "tomb" at story creation, re-verify before editing): `cli.rs`(23), `ux.rs`(23), `create.rs`(19), `hooks.rs`(15), `close_all.rs`(8), `resize.rs`(7), `close.rs`(7), `revoke.rs`(6), `unlock.rs`(6), `progress.rs`(6), `slam.rs`(5), `info.rs`(4), `enroll.rs`(2), `fakes.rs`(2), `keyslot_guard.rs`(3), `mapping_name.rs`(1).
  - Rename every test function name's `_tomb`/`tomb_` segment to `_volume`/`volume_`, every comment, every assertion string, and every `MIN_TOMB_SIZE_BYTES` reference (`create.rs`, `cli.rs`, `progress.rs` — must land in the same pass as Task 1's const rename).
  - `mapping_name.rs:16`'s nonexistent-path fixture `tomb-fido2-mapping-name-does-not-exist` → `volume-fido2-mapping-name-does-not-exist` (AC #5).
  - `ux.rs:184`'s fixture string `.tomb-fido2-bootstrap-abc123` → `.volume-fido2-bootstrap-abc123`, matching Task 3's `mod.rs` rename exactly (this is a hardcoded mirror of that literal, not derived from it — both must change together or the test that asserts on this string breaks).

- [ ] **Task 5: Rename across `tests/hardware/main.rs`** (AC: #1, #5)
  - This file is the densest single file (116 "tomb" occurrences at story creation). It is a manual/hardware-gated suite (`make test-hardware`), not run by CI — rename it carefully but note that `cargo build`/`make test` won't catch mistakes here; only a manual `make test-hardware` run will (AC #2).
  - Rename every doc comment, test function name (`create_a_file_backed_tomb_...`, `unlock_mounts_a_file_backed_tomb_...`, etc.), and every fixture literal: temp-dir names (`tomb-fido2-hardware-test*` → `volume-fido2-hardware-test*`), backing-file names (`tomb.img` → `volume.img`), mapper names passed to `cryptsetup open`/`dumpe2fs`/`cryptsetup close` in the printed manual-verification instructions, marker filenames (`tomb-fido2-marker.txt`, `tomb-fido2-write-attempt.txt`) and their file contents (`b"tomb-fido2 hardware test"`), and the `tomb_name` collision-fallback comment (~line 903, matching Task 3's identifier rename).

- [ ] **Task 6: Rename in `README.md` body and `hooks.md`** (AC: #3)
  - `README.md`: rename every domain-noun "tomb" in the feature table and body prose (e.g. "Format a new tomb", "Unlock and mount a tomb", "a tomb's enrolled FIDO2 keys", "Per-tomb bind-mounts", "close every currently open tomb", "a tomb has a `bind-hooks` file", "the tomb itself", "known tombs", "before creating a brand-new tomb", etc.) to "volume".
  - **Leave untouched (AC #6):** line ~3's `[dyne/tomb](https://github.com/dyne/tomb)` link and prose, line ~28's "a capability the original Tomb never had", line ~36's `[dyne/tomb](https://dyne.org/docs/tomb/manpage/#hooks)` link — all proper-noun references to the unrelated upstream project, not this project's domain noun.
  - `_bmad-output/specs/spec-tomb-fido2/hooks.md`: rename every domain-noun "tomb" (e.g. "in the tomb's own root", "bind-mounts the tomb-relative path", "the tomb root", "escape the tomb"). Leave its `dyne/tomb` references (manpage URL, "Adapted from dyne/tomb's hooks model", the `tomb#L2824-2910` source citation) untouched — same exception as README.
  - **Note the sibling-file split:** `_bmad-output/specs/spec-tomb-fido2/SPEC.md` lives in the same directory as `hooks.md` but is one of the frozen historical docs (AC #4) — do not touch it even though it's adjacent and easy to sweep in by accident.

- [ ] **Task 7: Full regression pass** (AC: #1, #2, #3, #5)
  - `cargo build` succeeds with all renamed identifiers (this is what catches any `MIN_TOMB_SIZE_BYTES`/`tomb_name`/`SourceEscapesTombRoot`-style rename left half-done across files).
  - `make test` passes with the same 174 tests as before the rename (confirmed count as of story creation — reverify, don't assume it still holds after your edits).
  - `cargo run -- --help` and a manual walk of each subcommand's `--help` text shows no remaining "tomb" domain-noun wording.
  - Re-run `grep -rniE "tomb" src/ tests/ README.md _bmad-output/specs/spec-tomb-fido2/hooks.md` — every remaining hit must be one of the three explicit exceptions (AC #6: `dyne/tomb` references, and `mod.rs:122`'s carved-out product-name doc comment) or nothing; anything else is a miss.
  - Run `make test-hardware` manually at least once against real hardware (AC #2) — this is the only suite that actually creates a file named after the renamed fixtures and shells real `cryptsetup`/mapper-name commands against them, so it's the real end-to-end check that the rename didn't silently break a literal a human is meant to type by hand.

## Dev Notes

- **Scope boundary, again the whole point.** Epic 5 splits the rebrand into four stories (per `_bmad-output/planning-artifacts/sprint-change-proposal-2026-08-02.md` §4): 5.1 = product identity (done), **5.2 = this story**, 5.3 = tagline/pronunciation/mascot top-level branding copy, 5.4 = `github-automation-reference.md` + SPEC.md/AD-13 addendum. Do not pull 5.3/5.4 work in here — leave `_bmad/custom/github-automation-reference.md` and any tagline/pronunciation copy untouched.
- **This is a pure mechanical identifier/prose rename with zero behavioral change.** The acceptance bar is a clean `cargo build` + `make test`, not a manual feature audit — same posture as 5.1, per the sprint change proposal's Technical Impact section. The only real risk is a naive blanket find-replace catching a `dyne/tomb` reference or a historical doc — Tasks 1-6 call out every known instance of that trap explicitly.
- **Three categories of "tomb" exist in this codebase, and only one is this story's job:**
  1. The domain noun (the encrypted container concept) — **rename this**, everywhere in `src/`, `tests/`, README body, `hooks.md`.
  2. The old product name (`tomb-fido2`/`tomb_fido2`) — Story 5.1's job, already done, except two carve-outs it deliberately deferred to this story (AC #5) and one it deliberately resolved as a non-issue and left alone (the `mod.rs:122` doc comment, AC #6) — don't re-litigate that one.
  3. References to the unrelated upstream `dyne/tomb` project — never renamed, ever (AC #6).
- **Previous story's deferred items are this story's Task 3/4 (AC #5).** Story 5.1's code review found `src/adapters/exec/mod.rs:251,2251`, `tests/hardware/main.rs`, and `tests/unit/*.rs` still embedding the old product name in internal fixtures, and explicitly deferred fixing them here since this story already touches the same files. Treat each as a plain `tomb` → `volume` substring rename (keep `fido2`/`unit-test`/`hardware-test` tokens as-is) — they're internal fixture literals, not the CLI banner or any AD-13-governed surface.
- **`MIN_TOMB_SIZE_BYTES` is the one identifier with real cross-file compile risk.** It's `pub` and consumed by 4 other files across `src/` and `tests/`. Rename it in one commit-worthy pass, not incrementally, or `cargo build` breaks mid-task.
- **No git-history rewrite.** `CHANGELOG.md`'s historical entries (release-please-generated, tied to real merged PR/issue URLs like `.../tomb-fido2/issues/11`) are not in this story's AC list at all — leave the whole file alone, same reasoning 5.1 already established for it.

### Project Structure Notes

- Files touched: `src/domain/workflows/{create,unlock,enroll,revoke,close,resize,slam,close_all}.rs`, `src/domain/hooks.rs`, `src/domain/errors.rs`, `src/domain/keyslot_guard.rs`, `src/ports/{luks_backend,filesystem_backend}.rs`, `src/cli/main.rs`, `src/cli/ux.rs`, `src/adapters/exec/mod.rs`, all of `tests/unit/*.rs` that currently mention "tomb" (16 files), `tests/hardware/main.rs`, `README.md`, `_bmad-output/specs/spec-tomb-fido2/hooks.md`.
- No new files, modules, or structural changes — pure identifier/string rename within existing files. No filenames need renaming (none of the touched files have "tomb" in their own filename).

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 5.2: Domain Term Rename (tomb → volume)] — acceptance criteria origin.
- [Source: _bmad-output/planning-artifacts/sprint-change-proposal-2026-08-02.md#4. Detailed Change Proposals] — per-story file-level scope breakdown for all of Epic 5.
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-13 — Placeholder-name isolation] — the continuity exception for historical docs, and confirmation that "tomb" the domain noun is explicitly outside AD-13's own scope.
- [Source: _bmad-output/implementation-artifacts/5-1-product-identity-rename.md#Review Findings] — origin of the two deferred fixture-literal items (AC #5).

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

- Task 1: Case-preserving `tomb`/`Tomb`/`TOMB` -> `volume`/`Volume`/`VOLUME` rename across `src/domain/` and `src/ports/`. Includes `MIN_TOMB_SIZE_BYTES` -> `MIN_VOLUME_SIZE_BYTES` (consumers in `src/cli/main.rs`, `src/cli/ux.rs` comment, and `tests/unit/{create,cli,progress}.rs` intentionally left for Task 2/4 in the same build-passing pass) and `hooks.rs`'s `tomb_root`/`canonical_tomb_root`/`SourceEscapesTombRoot` -> `volume_root`/`canonical_volume_root`/`SourceEscapesVolumeRoot`. `cargo build` will not pass until Task 2/4 land (expected, cross-file rename).
- Task 2: Case-preserving rename across `src/cli/main.rs` (help/doc text, user-facing strings, `MIN_TOMB_SIZE_BYTES` use-site) and `src/cli/ux.rs` (error/status strings, `SourceEscapesTombRoot` match arm now `SourceEscapesVolumeRoot`, consistent with Task 1). `cargo build` still pending Task 3/4 (`src/adapters/exec/mod.rs`'s `tomb_name` param and `tests/unit/*.rs`'s `MIN_TOMB_SIZE_BYTES` use-sites).
- Task 3: Renamed whole file except the AC#6 carve-out at line 122 (`tomb_fido2` old crate name doc comment). `create_mount_point`'s `tomb_name` param/locals -> `volume_name`; AC#5 fixture literals at lines 251/2251 renamed; test fn names `..._file_backed_tomb`/`..._device_backed_tomb` -> `..._volume`; the `/home/user/tombs/tomb.img` parser-test fixture at 2307/2311 renamed too since it's plain domain-noun text, not old-product-name branding.
- Task 4: Renamed domain-noun usages (fn names, comments, assertion strings, plain-domain-noun placeholder paths like `/tmp/tomb`, `/tomb/a.img`) across all 16 `tests/unit/*.rs` files, plus `MIN_TOMB_SIZE_BYTES` use-sites (`create.rs`, `cli.rs`, `progress.rs`) and `SourceEscapesTombRoot` use-sites (`hooks.rs`, `ux.rs`) to match Task 1/3's identifier renames. AC#5's two explicit fixtures (`mapping_name.rs:16`, `ux.rs:184`) renamed. Deliberately left untouched: every other `tomb-fido2`-branded fixture literal (temp-dir names in `enroll/close/create/hooks/unlock/progress.rs`, argv[0] in `cli.rs`) — these are old-product-name references (category 2, Story 5.1's domain, not re-opened by AC#5's closed exception list) not this story's job. `cargo build` and `make test` (174 tests) both pass clean.

### File List

- src/domain/errors.rs
- src/domain/hooks.rs
- src/domain/keyslot_guard.rs
- src/domain/workflows/close.rs
- src/domain/workflows/close_all.rs
- src/domain/workflows/create.rs
- src/domain/workflows/enroll.rs
- src/domain/workflows/resize.rs
- src/domain/workflows/slam.rs
- src/domain/workflows/unlock.rs
- src/ports/filesystem_backend.rs
- src/ports/luks_backend.rs
- src/cli/main.rs
- src/cli/ux.rs
- src/adapters/exec/mod.rs
- tests/unit/cli.rs
- tests/unit/ux.rs
- tests/unit/create.rs
- tests/unit/hooks.rs
- tests/unit/close_all.rs
- tests/unit/resize.rs
- tests/unit/close.rs
- tests/unit/revoke.rs
- tests/unit/unlock.rs
- tests/unit/progress.rs
- tests/unit/slam.rs
- tests/unit/info.rs
- tests/unit/enroll.rs
- tests/unit/fakes.rs
- tests/unit/keyslot_guard.rs
- tests/unit/mapping_name.rs
