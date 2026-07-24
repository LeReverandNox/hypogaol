---
baseline_commit: e797d3c
---

# Story 1.8: Unified CLI Dispatch & Plain-Language Errors

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a user,
I want to drive create and unlock through one CLI with prompts/errors in plain language,
so that I never need to fall back to cryptsetup/fido2-token flags directly, even when something goes wrong.

> **Sequencing note (from epics.md):** Stories 1.5–1.7 each already wired and exposed their own CLI subcommand with baseline error handling as they were built — none of them were blocked on this story. This story is the consolidation pass: it finalizes complete `--help` coverage across the Epic 1 subcommands and audits every error path accumulated so far (Stories 1.5, 1.6, 1.7) for consistent plain-language translation at the `cli::ux` boundary.

## Acceptance Criteria

1. **Given** the CLI, **when** I run `--help` or a subcommand, **then** `create` (with its file/device target flags) and `unlock` are both available as first-class subcommands of one binary. [Source: epics.md#Story 1.8]
2. **Given** any domain error surfaced by create or unlock (missing dependency, path-exists refusal, LUKS2-header refusal, missing wipe confirmation, FIDO2 touch timeout, etc.), **when** the CLI displays it, **then** the message is translated to plain language at the `cli::ux` boundary (e.g. "Please touch your security key," never "Awaiting UP") — no internal jargon leaks to the user. [Source: epics.md#Story 1.8]
3. **Given** the `cli` layer, **when** it calls into `domain`, **then** it never touches a `LuksBackend`/`Fido2Backend`/`FilesystemBackend` port directly — all port access goes through `domain::workflows::*`. [Source: epics.md#Story 1.8]

## Tasks / Subtasks

- [x] Task 1: Implement `cli::ux::translate(err: &DomainError) -> String` (AC: #2)
  - [x] `src/cli/ux.rs` is currently an empty file — deliberately left blank by Stories 1.5–1.7, which all noted this story as the one that fills it in. This is the plain-language translation boundary the Design Paradigm and Consistency Conventions sections of `ARCHITECTURE-SPINE.md` describe: "domain errors are a typed enum (`thiserror`), translated to plain-language text only at the `cli` boundary — never inside `domain`."
  - [x] `DomainError` (`src/domain/errors.rs`) has exactly 8 variants. `translate` must `match` all 8 exhaustively — Rust's exhaustiveness check is a feature here: a 9th variant added by a later epic (e.g. Story 2.x/3.x) will fail to compile against this function until it's given a translation, so jargon can never silently leak through an unhandled arm.
  - [x] The 6 structured variants each carry typed data and get a direct, one-to-one plain-language template — no string parsing needed:
    - `DestinationExists(path)` — e.g. "A tomb already exists at {path}. Choose a different location, or unlock the existing one instead."
    - `DeviceAlreadyFormatted(path)` — e.g. "{path} already has an encrypted tomb on it. If you meant to unlock it, use the unlock command instead."
    - `DeviceConfirmationRequired` — e.g. "Creating a tomb on a device erases everything on it. Please confirm the warning to continue."
    - `DeviceSizeExceedsCapacity { path, requested, capacity }` — state both numbers in plain terms (e.g. "You asked for {requested} bytes, but {path} only has {capacity} bytes available.").
    - `DeviceTooSmall { path, size }` — explain the tomb would be too small to be usable.
    - `PreflightFailed(missing)` — `missing` is a `Vec<String>` of items like `"cryptsetup binary not found on PATH"`, `"kernel hidraw support not found (/sys/class/hidraw missing)"` (see `src/adapters/exec/mod.rs:408-423,730-735,801-804`). These per-item strings already name real, installable/checkable things — don't rewrite each one individually; wrap the list in one friendly framing sentence (e.g. "tomb-fido2 can't run yet — a few things are missing:") followed by the list as-is. Chasing a bespoke rewrite of every dependency string is out of proportion for this story.
  - [x] `LastKeyslotGuard` — not reachable from `create` or `unlock` today (only `domain::workflows::revoke`, Story 2.2, will ever produce it), but the exhaustive match still requires an arm. Give it its plain-language text now (e.g. "That's the last key that can unlock this tomb — revoking it would lock you out permanently, so this was refused."); this story only needs its behavior *exercised* via create/unlock's own paths, not this specific arm.
  - [x] `AdapterFailure(String)` is the catch-all every other failure (subprocess spawn/exit failures, JSON parse failures, filesystem I/O failures) currently funnels through, and it is the one variant with no structured fields — the string content varies by call site. Since this story's scope is bounded to create's and unlock's own call graphs (not enroll/revoke/close/resize, which are still `todo!()` stubs), the reachable message shapes are finite and enumerable. Known categories to translate, each as one friendly template covering the whole category (not a bespoke string per exact message):
    - **Missing/unusable LUKS2 FIDO2 support** — `luks2_fido2_token_plugin_present`'s error text (`src/adapters/exec/mod.rs:59-90`, surfaced via `PreflightFailed`, not `AdapterFailure` — cross-check it's covered by the `PreflightFailed` arm above, not missed).
    - **`cryptsetup` subprocess failures** during create's bootstrap (`bootstrap_format_and_open`'s `luksFormat`/`luksOpen`/`resize` calls, `src/adapters/exec/mod.rs:470-569`) and unlock's `open` (`:695-723`) — these come in two shapes: `run_piping_stdin`'s generic `"{cmd:?} failed: {stderr}"` / `"failed to spawn {cmd:?}: {e}"` (`:222-251`, where `{cmd:?}` is Rust's `Debug` format of the whole `Command` — the single ugliest, most jargon-heavy string in the codebase, e.g. `"cryptsetup" "luksFormat" "--type" "luks2" ...`), and `open`'s own `"cryptsetup open --token-only failed for {path} as {name}"` (`:718-721`). Translate the whole category to something like "Something went wrong while unlocking/creating your tomb — your security key or its PIN may not have been accepted in time." Do not echo `{cmd:?}`'s raw argv dump into the primary sentence.
    - **FIDO2 enrollment failures** (`enroll_fido2_key`'s `systemd-cryptenroll` failure and its token export/import/parse failures, `src/adapters/exec/mod.rs:744-794,308-410`) — e.g. "Enrolling your security key didn't complete. Make sure it's plugged in and touch it when prompted, then try again."
    - **Mount/filesystem failures** (`mkfs`, `mount`, `chmod`, mount-point create/remove, `:844-955`) — these already carry real stderr text via `.output()` capture (unlike `open`, which uses `.status()` with inherited stdio and so has no stderr to show) — e.g. "Your tomb unlocked, but tomb-fido2 couldn't mount its filesystem." Keep the captured detail available (see fallback note below) rather than discarding it.
    - **Device/file sizing failures** (`blockdev --getsize64`, `set_backing_file_size`, `remove_backing_file`, `:818-871`) — e.g. "tomb-fido2 couldn't determine or set the size needed for this tomb."
    - **`mapping_name::mapping_name`'s canonicalization failure** (`src/domain/mapping_name.rs:36-38`, `"failed to canonicalize {path}: {e}"`) — e.g. "tomb-fido2 couldn't find {path}. Check the path and try again."
    - **Anything else (fallback):** wrap with a generic frame that still surfaces the original text as a labeled technical detail rather than presenting it as the primary message, e.g. `format!("Something unexpected happened while working with your tomb.\nTechnical detail: {inner}")` — this keeps AC #2's "no jargon leaks" promise for the primary line while not discarding information a bug report would need, without requiring this story to chase every possible future `AdapterFailure` string.
  - [x] **Resolve one ambiguity in AC #2 explicitly, do not attempt a stdio-capture rewrite:** the AC's illustrative example ("never 'Awaiting UP'") refers to text the `systemd-fido2` cryptsetup token plugin prints *directly to the real terminal* during `LuksBackend::open` (`src/adapters/exec/mod.rs:703-707`, `.status()`, inherited stdio — same pattern as `enroll_fido2_key`'s `systemd-cryptenroll` call, `:778-782`) and during `systemd-cryptenroll`'s own prompt. That text never enters tomb-fido2's own process at all — there is no captured string for `cli::ux::translate` to intercept, because `.status()` deliberately does not pipe stdout/stderr (piping it would break the interactive touch/PIN flow itself, per Stories 1.5's and 1.7's own Dev Notes). Read the AC's example as illustrative of "no cryptsetup/systemd jargon in tomb-fido2's *own* printed messages," not as a literal requirement to rewrite cryptsetup's live prompt output — doing so would require capturing stdio and relaying it, a significant unrequested architecture change to a pattern this codebase already relies on. The AC's "FIDO2 touch timeout" case is instead the *eventual* `AdapterFailure` from `open`'s non-zero exit once the user fails to touch in time (translated per the "`cryptsetup` subprocess failures" category above) — that final string, not the live prompt, is what gets translated.
- [x] Task 2: Route `src/cli/main.rs`'s error printing through `ux::translate` (AC: #2)
  - [x] Add `use crate::cli::ux;` to `src/cli/main.rs`.
  - [x] `run_create` (`:162-165`) currently does `eprintln!("{err}")` — change to `eprintln!("{}", ux::translate(&err));`.
  - [x] `run_unlock` has two raw-`Display` sites: the preflight check (`:183-186`) and `unlock::run`'s error arm (`:192-195`) — change both the same way.
- [x] Task 3: `--help` coverage verification, made testable (AC: #1)
  - [x] Doc comments (`///`) already exist on `Commands::Create`, `Commands::Unlock`, `CreateMode::File`, `CreateMode::Device`, and their fields (`src/cli/main.rs:21-74`) — clap's `#[derive(Parser)]`/`#[derive(Subcommand)]` already surfaces these in generated `--help` text. This task is primarily verification, not new doc-comment authoring — only add/adjust a doc comment if a gap is actually found while writing the test below.
  - [x] Make `struct Cli` (`src/cli/main.rs:14-19`) `pub` so `tests/unit` (a separate integration-test crate) can call it directly — matching the existing pattern of `pub fn parse_size`, `pub fn confirms_wipe`, `pub const MIN_TOMB_SIZE_BYTES` in the same file, already exposed purely for test access. The `command: Commands` field itself can stay private — `Commands`/`CreateMode` do **not** need to become `pub`, since no `pub` method on `Cli` (`parse`, `try_parse_from`, or the derived `CommandFactory::command`) exposes them in its signature.
  - [x] Extend `tests/unit/cli.rs` (which already imports from `tomb_fido2::cli::main`) with tests using `clap::Parser`'s `Cli::try_parse_from` (bring `use clap::Parser;` into scope) or the derived `Cli::command()` (`use clap::CommandFactory;`):
    - Top-level help mentions both subcommands, e.g. assert the rendered help text (from a `--help`-triggered `clap::Error`, or from `Cli::command().render_help().to_string()`) contains `"create"` and `"unlock"`.
    - `create --help` mentions both `file` and `device`.
    - `unlock --help` mentions `--path`.
- [ ] Task 4: Audit — confirm `cli` still never touches a port directly (AC: #3)
  - [ ] This already holds true today by construction and this story introduces no new port access: `run_create`/`run_unlock` only ever call `create::run`/`unlock::run`/`preflight::check` (all `domain` entry points), passing `ExecAdapter` instances as `&dyn LuksBackend`/`&dyn Fido2Backend`/`&dyn FilesystemBackend` — never invoking a trait method on them directly from `cli`.
  - [ ] No test can mechanically enforce this in Rust (there's no visibility barrier stopping `cli::main` from calling a port method on the `ExecAdapter` it already constructs) — treat this as a manual re-read of the finished diff for Tasks 1–3, not a task requiring new code or a new test.
- [ ] Task 5: Unit tests for `ux::translate` (AC: #2)
  - [ ] New file `tests/unit/ux.rs` (register in `tests/unit/main.rs`'s `mod` list, alongside the existing `cli`/`create`/`unlock`/etc. entries).
  - [ ] One assertion per structured variant (6 total) plus `LastKeyslotGuard`: construct the `DomainError` directly, call `translate`, assert the result is non-empty and does not contain jargon markers (`"cryptsetup"`, `"AdapterFailure"`, `"luksFormat"`, `"systemd-cryptenroll"`), and does surface the variant's key user-relevant detail (e.g. the path for `DestinationExists`/`DeviceAlreadyFormatted`, both numbers for `DeviceSizeExceedsCapacity`).
  - [ ] For `AdapterFailure`, build several `DomainError::AdapterFailure(String)` values using message text copied verbatim from the real call sites cited in Task 1 (e.g. `"cryptsetup open --token-only failed for /tmp/foo as vault-abc123"`, `"mount failed: some real stderr"`, a `{cmd:?}`-shaped string like `r#""cryptsetup" "luksFormat" "--type" "luks2" "--batch-mode" "--key-file" "-" "/tmp/foo" failed: some stderr"#`) — assert each lands in its intended category's template and the primary sentence never echoes `"cryptsetup"`/`"--token-only"`/the raw mapping name.
  - [ ] Include one `AdapterFailure` input that matches no known category — assert the generic fallback still returns non-panicking, non-empty output (proves the fallback arm is actually exercised, not just theoretical).
  - [ ] Confirm `cargo test --test unit`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --check` all stay green.

## Dev Notes

- **Scope is CLI/UX-layer only.** No changes to `src/domain/errors.rs` (no new `DomainError` variant needed — all 8 already exist), `src/domain/workflows/{create,unlock}.rs` (their logic is unchanged; only how their `Err` is *displayed* changes, and only at the `cli` boundary), or `src/adapters/exec/mod.rs` (the raw message text inside `AdapterFailure` stays exactly as-is — translation happens entirely in `cli::ux`, matching `ARCHITECTURE-SPINE.md`'s Design Paradigm: "translated to plain-language text only at the `cli` boundary"). Do not touch `src/domain/workflows/{enroll,revoke,close,resize}.rs` — still `todo!()` stubs for later epics, out of scope.
- **Why `translate` must be exhaustive even though only 2 of 8 variants are reachable from create/unlock's happy paths today:** `LastKeyslotGuard` belongs to `revoke` (Story 2.2), not yet wired to any CLI command, but it already exists in the enum. Since `translate` takes `&DomainError` (the whole type), Rust forces a match arm for it regardless of what's CLI-reachable today. This is a feature, not scope creep: it means Story 2.x/3.x, when they wire `enroll`/`revoke`/`close`/`resize` to the CLI, will get a compile error here if a translation is still missing, rather than a silent `Display` leak.
- **The `{cmd:?}` Debug-format leak is the single worst offender to translate.** `run_piping_stdin` (`src/adapters/exec/mod.rs:222-251`), used by `bootstrap_format_and_open`'s `luksFormat`/`luksOpen`/`resize` calls, formats failures as `"{cmd:?} failed: {stderr}"` — `{cmd:?}` dumps the entire `std::process::Command` argv, e.g. `"cryptsetup" "luksFormat" "--type" "luks2" "--batch-mode" "--key-file" "-" "/path/to/tomb"`. This is by far the most jargon-dense string reachable from `create`, and exactly what AC #2 means by "no internal jargon leaks."
- **`open`'s failure has no captured stderr at all** (`.status()`, inherited stdio, `src/adapters/exec/mod.rs:703-710`) — unlike `mount`'s failure, which does capture stderr via `.output()` (`:910-929`). This is why `open`'s category translation can't quote "the real reason," only offer a general "your key/PIN wasn't accepted in time" framing — there is genuinely no more specific information available to the Rust process for that one call.
- **Git intelligence:** baseline is `e797d3c` ("unlock and mount a tomb", Story 1.7's merge, current `main` HEAD) — no other commits since. `src/cli/ux.rs` is a single blank line as of this baseline; nothing else has touched it since scaffolding.
- **AD-13 compliance:** don't introduce any new hardcoded product-name string in `ux::translate`'s output — the existing `#[command(version, about)]` on `Cli` already sources the binary name from Cargo package metadata; this story adds no new naming surface.
- **Testing (AD-7):** `ux::translate` is a pure function over an owned `DomainError` value — no fakes, no adapters, no hardware gate needed. `tests/unit/ux.rs` constructs `DomainError` values directly (all variants are public via `pub enum DomainError` and public fields, already used the same way in `tests/unit/workflows.rs`/`create.rs`).

### Project Structure Notes

- Modified: `src/cli/ux.rs` (implemented — was previously an empty placeholder file), `src/cli/main.rs` (`use crate::cli::ux;` added; `run_create`/`run_unlock` route their two error-printing sites through `ux::translate`; `struct Cli` made `pub`), `tests/unit/cli.rs` (extended with `--help` coverage tests), `tests/unit/main.rs` (new `mod ux;` entry).
- New: `tests/unit/ux.rs`.
- Do **not** touch `src/domain/errors.rs`, `src/domain/workflows/{create,unlock}.rs` (logic unchanged), `src/domain/workflows/{enroll,revoke,close,resize}.rs` (later epics), `src/adapters/exec/mod.rs` (raw message text stays as-is — it's the input to `translate`, not something this story rewrites), `src/ports/*`, or `tests/hardware/main.rs` (no hardware-observable behavior changes — this is a pure string-translation feature at the CLI boundary).
- Consistent with `ARCHITECTURE-SPINE.md`'s Structural Seed: `cli/ux.rs` goes from an intentionally-empty placeholder (per every prior story's Project Structure Notes) to its designed purpose — "domain-error -> plain-language translation (CAP-5)."

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.8: Unified CLI Dispatch & Plain-Language Errors]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#Design Paradigm — domain errors translated to plain-language text only at the cli boundary, never inside domain]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#Consistency Conventions — error shape/translation-boundary convention]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#Capability → Architecture Map — CAP-5 (plain-language UX) lives in cli::ux]
- [Source: _bmad-output/specs/spec-tomb-fido2/SPEC.md#CAP-5] (zero-FIDO2-knowledge UX mandatory across all prompts and errors)
- [Source: src/domain/errors.rs] (all 8 current `DomainError` variants, exhaustively matched)
- [Source: src/cli/main.rs] (current `Cli`/`Commands`/`CreateMode` definitions, `run_create`/`run_unlock`'s two raw-`Display` error-printing sites)
- [Source: src/cli/ux.rs] (currently a single blank line — this story's primary target)
- [Source: src/adapters/exec/mod.rs] (every `AdapterFailure`-producing call site cited in Task 1: `:59-90` FIDO2 plugin check, `:222-251` `run_piping_stdin`'s `{cmd:?}` leak, `:308-410` token export/import, `:459-580` `bootstrap_format_and_open`, `:695-723` `open`, `:744-795` `enroll_fido2_key`, `:798-955` `FilesystemBackend` methods including `mount`)
- [Source: src/domain/mapping_name.rs] (canonicalization-failure `AdapterFailure` message)
- [Source: tests/unit/{cli.rs,main.rs}] (existing test conventions to extend)
- [Source: _bmad-output/implementation-artifacts/1-7-unlock-and-mount-a-tomb.md] (`src/cli/main.rs`'s `run_unlock` doc comment explicitly deferring translation of unlock's errors to this story; deferred-work.md's matching entry)
- [Source: _bmad-output/implementation-artifacts/deferred-work.md#Deferred from: code review of 1-7-unlock-and-mount-a-tomb] ("No plain-language wrapping of unlock failure paths ... explicitly Story 1.8's scope")

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

- Task 1: Implemented `cli::ux::translate` in `src/cli/ux.rs`, exhaustively matching all 8 `DomainError` variants. The 6 structured variants use direct one-to-one templates; `AdapterFailure`'s free-form string is classified into categories (FIDO2 enrollment, cryptsetup subprocess, mount/filesystem, path canonicalization, device/file sizing, generic fallback) by substring markers found verbatim at the real call sites, checked in an order that avoids the enrollment-vs-cryptsetup marker overlap (e.g. "cryptsetup token export failed"). `cargo build`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --test unit` all pass (33 existing tests, no regressions; dedicated `ux::translate` tests land in Task 5).
- Task 2: `src/cli/main.rs` now imports `crate::cli::ux` and routes all 3 raw-`Display` error-printing sites (`run_create`'s single site, `run_unlock`'s preflight-check and `unlock::run` sites) through `ux::translate(&err)`. `cargo build`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --test unit` all pass (33 tests, no regressions).
- Task 3: Made `struct Cli` `pub` (the `command: Commands` field and `Commands`/`CreateMode` themselves stay private, matching the story's guidance — no `pub` method on `Cli` exposes them). Added 3 tests to `tests/unit/cli.rs` using `Cli::try_parse_from` against a `--help`-triggered `clap::Error`'s rendered text (helper `fn help_text` matches on the `Result` rather than `unwrap_err()`, since `Cli` doesn't derive `Debug`): top-level help contains `"create"`/`"unlock"`, `create --help` contains `"file"`/`"device"`, `unlock --help` contains `"--path"`. No doc-comment gaps found — existing `///` comments already cover all clap-surfaced help text. `cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and `cargo test --test unit` all pass (36 tests, no regressions).

### File List

- Modified: `src/cli/ux.rs`, `src/cli/main.rs`, `tests/unit/cli.rs`
