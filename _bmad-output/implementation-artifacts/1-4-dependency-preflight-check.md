---
baseline_commit: 852bd91
---

# Story 1.4: Dependency Preflight Check

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a user,
I want the tool to verify all hard dependencies before starting any operation,
so that I get a clear, actionable error before anything is touched, never a mid-operation failure.

## Acceptance Criteria

1. **Given** all hard dependencies present (LUKS2 FIDO2/hmac-secret support, required binaries, kernel/hidraw features), **when** any `domain::workflows::*` function begins, **then** `domain::preflight` runs as its first statement, passes, and the workflow proceeds. [Source: epics.md#Story 1.4]
2. **Given** a required binary or kernel feature is missing, **when** the user runs any operation, **then** the tool exits cleanly before starting the operation, with a detailed, actionable error naming the missing dependency, **and** no mutating action (LUKS format/open, filesystem changes) occurs. [Source: epics.md#Story 1.4]
3. **Given** `preflight`'s implementation, **when** it is invoked, **then** it is defined once in `domain::preflight` and called identically inside `create`, `unlock` (including read-only), `close`, and `resize`, **and** none of these workflows get a lighter gate than the others. [Source: epics.md#Story 1.4, AD-4]

## Tasks / Subtasks

- [x] Task 1: Give the three ports a `check_prerequisites` method each (AC: #1, #3)
  - [x] Add `fn check_prerequisites(&self) -> Result<(), Vec<String>>` to `LuksBackend`, `Fido2Backend`, `FilesystemBackend` (`src/ports/{luks_backend,fido2_backend,filesystem_backend}.rs`) — `Err` carries one human-readable string per missing/unsupported dependency that *this* port's real adapter would need, `Ok(())` means all of this port's prerequisites are satisfied
  - [x] This keeps preflight itself free of any direct binary/kernel probing (AD-1: domain never shells out or touches the OS directly) — each port owns knowing what its own real adapter requires
- [x] Task 2: Implement `domain::preflight::check` (AC: #1, #2, #3)
  - [x] Change `pub fn check() -> Result<(), DomainError>` to accept the three port trait objects: `pub fn check(luks: &dyn LuksBackend, fido2: &dyn Fido2Backend, fs: &dyn FilesystemBackend) -> Result<(), DomainError>`
  - [x] Call `check_prerequisites()` on all three — do not short-circuit on the first failure; collect every missing dependency from all three so the user sees the full list in one pass, not one-at-a-time across repeated runs
  - [x] Aggregate any failures into one new `DomainError` variant, e.g. `DomainError::PreflightFailed(Vec<String>)`, added to the currently-empty `enum DomainError` in `src/domain/errors.rs` — the Display/message must name every missing dependency so the CLI can surface a single detailed, actionable error (AC #2)
  - [x] Return `Ok(())` only if all three ports report no missing prerequisites
- [x] Task 3: Wire `preflight::check` as the first statement in every current workflow stub (AC: #1, #3)
  - [x] Update `src/domain/workflows/{create,unlock,close,resize}.rs` — each currently has a zero-argument stub `pub fn run() -> Result<(), DomainError> { todo!() }`; change the signature to accept the three port trait objects (matching Task 2's `check` signature) and make the first line of the body `preflight::check(luks, fido2, fs)?;`, followed by the existing `todo!()` for the rest of the workflow's not-yet-implemented logic
  - [x] Do **not** touch `src/domain/workflows/{enroll,revoke}.rs` — those are Epic 2 stories (2.1, 2.2) and will wire their own `preflight::check` call when implemented; AC #3 only scopes create/unlock/close/resize
  - [x] Confirm all four updated functions call `preflight::check` identically (same argument order, same immediate-first-line placement) — no workflow gets a lighter or differently-shaped gate than another
- [x] Task 4: Real `adapters::exec` prerequisite checks, scoped to what's checkable without hardware (AC: #1, #2)
  - [x] `LuksBackend::check_prerequisites`: verify the `cryptsetup` binary is on `PATH`; verify `systemd-cryptenroll` is on `PATH` (owns the `systemd-fido2` token plugin per AD-1); verify LUKS2 FIDO2/hmac-secret support is actually present — during implementation, confirm the concrete detection mechanism empirically in the Nix devShell (candidates: the `libcryptsetup-token-systemd-fido2.so` plugin file existing on the loader's search path, or `cryptsetup --help`/`cryptsetup --version` reporting token-plugin support) and document whichever one is used and why in Dev Notes/Completion Notes
  - [x] `Fido2Backend::check_prerequisites`: verify `fido2-token` is on `PATH`; verify kernel `hidraw` support is present (e.g. `/sys/class/hidraw` exists, or equivalent — confirm the concrete check empirically, same as above)
  - [x] `FilesystemBackend::check_prerequisites`: verify `mkfs.ext4`, `resize2fs` (e2fsprogs, AD-8 v1 ext4-only), and `blockdev` (util-linux, AD-9's `device_capacity`) are all on `PATH`
  - [x] Binary-presence checks should share one small internal helper (e.g. `which`-style PATH lookup) inside `adapters::exec` rather than three copies of the same logic
- [x] Task 5: Fake test-support ports + preflight unit tests (AC: #1, #2, #3)
  - [x] Create the dedicated test-support module referenced by AD-7 (not yet created by any prior story) — e.g. `src/domain/workflows/mod.rs`-adjacent or a `tests/unit/`-local fakes module — providing one fake `LuksBackend`/`Fido2Backend`/`FilesystemBackend` whose `check_prerequisites` is controllable per-test (all-pass, or fail with specific named-missing-dependency lists)
  - [x] Unit test: all three fakes pass → `preflight::check` returns `Ok(())`
  - [x] Unit test: one fake reports one missing dependency → `preflight::check` returns `Err` naming exactly that dependency
  - [x] Unit test: multiple fakes each report failures → the resulting error names dependencies from *all* of them, not just the first encountered (validates the no-short-circuit aggregation from Task 2)
  - [x] Unit test (per workflow): calling `create::run`/`unlock::run`/`close::run`/`resize::run` with a failing fake immediately returns the preflight error without reaching the workflow's own `todo!()` — proves preflight truly gates as the first statement, not merely running before some but not all logic
  - [x] Register/confirm these land under `make test` (`cargo test --test unit`) per AD-7 — no hardware-gated test needed for this story since nothing here touches a real FIDO2 device

### Project Structure Notes

- Modified files: `src/ports/luks_backend.rs`, `src/ports/fido2_backend.rs`, `src/ports/filesystem_backend.rs` (each gains `check_prerequisites`); `src/domain/errors.rs` (new `PreflightFailed` variant); `src/domain/preflight.rs` (real implementation, new signature); `src/domain/workflows/{create,unlock,close,resize}.rs` (new signature, preflight call wired in, `todo!()` remains for the rest of the workflow body); `src/adapters/exec/mod.rs` (first real logic in this file — currently empty).
- New: a fake-ports test-support module under `tests/unit/` (AD-7's "dedicated test-support module", referenced by architecture but not yet created by any prior story).
- Do **not** touch `src/domain/workflows/{enroll,revoke}.rs`, `src/cli/*` (no CLI subcommands are wired yet — Story 1.8 consolidates CLI surface; this story's workflow signatures change but nothing currently calls them from `cli::main::run`, which stays `pub fn run() {}`), or `Cargo.toml` (no new dependencies needed — binary-presence checks use only `std::env`/`std::path`, no new crate).
- Consistent with `ARCHITECTURE-SPINE.md`'s Structural Seed: `domain/preflight.rs` (CAP-6 gate), `ports/*` gain their first real trait methods, `adapters/exec/` gains its first real code.

## Dev Notes

- **This is the first story with real `domain`/`ports`/`adapters` logic.** Stories 1.1–1.3 were scaffolding/CI/release-config only ([Source: 1-1-project-scaffolding-nix-devshell.md], [Source: 1-2-ci-runs-the-mocked-unit-test-suite.md], [Source: 1-3-release-automation.md]) — there is no prior in-repo pattern for port method shapes, domain error variants, or the AD-7 fake test-support module to follow. The design calls made here (port method shape, error variant shape, fake module location) set the pattern later stories (1.5+, Epic 2, Epic 3) will follow — keep them simple and easy to extend, since every later workflow will add its own real methods to these same three ports.
- **AD-4 (mandatory shared pre-flight gate):** one `domain::preflight` check runs as the *first statement* inside each `domain::workflows::*` function itself — not merely a CLI-side convention a future caller could bypass. Binds CAP-6 and transitively CAP-1/2/3/8/9/10/11, but this story's AC only requires wiring `create`, `unlock` (incl. read-only — it's the same function per AD-11, not a separate one), `close`, and `resize`; `enroll`/`revoke` wire their own call when Epic 2 implements them. [Source: ARCHITECTURE-SPINE.md#AD-4]
- **Why the check lives behind the three existing ports, not a new fourth port or direct OS calls:** AD-1 forbids `domain` from shelling out or touching the OS directly — that's `adapters::exec`'s job. AD-7 requires all `domain` logic (including guardrails, which is exactly what `preflight` is) to be unit-testable against fake ports in default CI, with no physical FIDO2 hardware. The three named ports (`LuksBackend`, `Fido2Backend`, `FilesystemBackend`) each already correspond 1:1 to one external tool family (cryptsetup/systemd-cryptenroll; fido2-token/libfido2; mkfs.ext4+resize2fs+blockdev) — giving each a `check_prerequisites` method lets each real adapter own verifying exactly what it itself needs, with no new port and no domain-level OS access. This is a design decision made by this story, not dictated verbatim by `ARCHITECTURE-SPINE.md` (which lists preflight's three concerns in prose but doesn't specify the port shape) — if a materially better shape becomes obvious during implementation, prefer it, but keep the "one check per existing port, no new port, no direct domain-level OS access" invariant.
- **What "LUKS2 FIDO2/hmac-secret support" concretely means to check** is intentionally left for implementation-time empirical verification rather than specified here: the Nix devShell already provides `cryptsetup`/`systemd`/`libfido2` (`flake.nix`), so confirm the actual detection mechanism (plugin `.so` presence vs. a `cryptsetup`/`systemd-cryptenroll` capability flag) hands-on inside that shell, and record what was used and why in Completion Notes — do not guess a path/flag that hasn't been verified to exist on the pinned toolchain.
- **Error shape (AC #2 — "detailed, actionable error naming the missing dependency"):** `DomainError::PreflightFailed(Vec<String>)` (or equivalent) must carry enough detail that `cli::ux`'s future plain-language boundary (Story 1.8) has something concrete to translate per-dependency — avoid collapsing to one generic "dependencies missing" string with no names attached.
- **No-short-circuit aggregation:** collect failures from all three ports before returning, so a user with e.g. both `cryptsetup` and `fido2-token` missing sees both in one run, not one per invocation. This mirrors the spirit of AD-5/AD-10's "read live state, don't assume" pattern (verify everything relevant before making a decision) even though those ADs cover different mechanisms.
- **Testing (AD-7):** this story creates the first fake port implementations and the first entries in `tests/unit/` (currently a single `#[test] fn placeholder() {}` in `tests/unit/main.rs`) — establish these as reusable fakes future stories will extend with their own methods, not a one-off written only for preflight. `make test` (`cargo test --test unit`) must stay green; nothing in this story touches `tests/hardware/` (still a placeholder, correctly excluded from CI per Story 1.2).
- **No CLI wiring in this story.** `cli::main::run` stays `pub fn run() {}` — Story 1.8 is the CLI consolidation pass. This story only proves the gate works at the `domain` level via unit tests against fakes.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.4: Dependency Preflight Check]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-4 — Mandatory shared pre-flight gate]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-7 — Testing strategy]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#Structural Seed] (`domain/preflight.rs`, `ports/*`, `adapters/exec/`, `tests/unit/`)
- [Source: _bmad-output/specs/spec-tomb-fido2/SPEC.md] (CAP-6 intent/success criteria — dependency check before any operation, detailed actionable error on failure)
- [Source: src/domain/preflight.rs, src/domain/errors.rs, src/domain/workflows/*.rs, src/ports/*.rs, src/adapters/exec/mod.rs, tests/unit/main.rs] (current stub state as of baseline commit 852bd91 — all bodies are `todo!()`/empty, all three port traits are empty, no fake test-support module exists yet)
- [Source: _bmad-output/implementation-artifacts/1-3-release-automation.md] (previous story — CI/release-config only, no `src/` pattern precedent to follow for this story's domain/ports work)

## Dev Agent Record

### Agent Model Used

Claude Sonnet 5 (claude-sonnet-5)

### Debug Log References

- `cargo build`, `cargo test --test unit`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `cargo build --release`, and `make test` all green as of the final commit.

### Completion Notes List

- Task 1: added `check_prerequisites(&self) -> Result<(), Vec<String>>` to all three ports. No test-worthy behavior yet (pure trait-signature addition, no implementors); validated with `cargo build`.
- Task 2: `domain::preflight::check` now takes the three port trait objects, calls `check_prerequisites()` on each without short-circuiting, and aggregates all failures into the new `DomainError::PreflightFailed(Vec<String>)`. Behavioral unit tests land in Task 5 alongside the fake ports needed to exercise it in isolation.
- Task 3: `create`, `unlock`, `close`, `resize` workflow stubs now take the same three port trait objects and call `preflight::check(luks, fido2, fs)?;` as their first line, identically across all four, before the existing `todo!()`. `enroll`/`revoke` intentionally untouched (Epic 2 scope).
- Task 4: added `ExecAdapter` (`src/adapters/exec/mod.rs`) implementing `check_prerequisites` for all three ports, plus a shared `binary_on_path` PATH-lookup helper.
  - **LUKS2 FIDO2/hmac-secret detection mechanism (empirically verified in the Nix devShell):** neither pure candidate from the story was sufficient on its own. `cryptsetup --help` reports "LUKS2 external token plugin support is enabled." even when its own configured plugin directory is completely empty (verified: `cryptsetup`'s reported `LUKS2 external token plugin path:` pointed at an empty dir in this devShell, while the actual `libcryptsetup-token-systemd-fido2.so` lives in systemd's own package path). So text-only "support is enabled" is a false positive. The check implemented instead parses cryptsetup's own reported plugin-path line from `cryptsetup --help` and verifies `libcryptsetup-token-systemd-fido2.so` actually exists there — asking cryptsetup exactly where *it* will look, then confirming the file is really there, rather than guessing a fixed path or trusting the capability text alone.
  - **Known devShell-only quirk, not a bug:** in this Nix flake's devShell specifically, nixpkgs' `cryptsetup` and `systemd` are two independent store paths, so nix-provided `cryptsetup`'s own plugin directory is empty and this check reports the dependency missing *inside the devShell*. On a normal end-user Linux system (verified against this machine's host-installed cryptsetup/systemd, outside the devShell), the distro's package manager coordinates the two so the plugin lands exactly where cryptsetup expects — confirmed by inspecting `/usr/lib/cryptsetup/` locally, which does contain the plugin. This quirk is harmless for this story: no automated test exercises the real `ExecAdapter` (Task 5's tests use fake ports only, per AD-7), so `make test` is unaffected either way.
  - No new dependencies: binary-presence checks use only `std::env`/`std::path`; the FIDO2 plugin check uses only `std::process::Command` to invoke `cryptsetup --help` (already a hard runtime dependency), no new crate.
- Task 5: added a lib/bin split (`src/lib.rs` re-exporting `adapters`/`cli`/`domain`/`ports`; `src/main.rs` now just calls `tomb_fido2::cli::main::run()`) — **not itself a story subtask, but required to satisfy AD-7's mandate that `domain` be unit-testable from `tests/unit/*`**: an integration test binary cannot reach modules that only exist inside a binary-only crate. This uses Cargo's automatic target detection (a `src/lib.rs` present is auto-registered as the `tomb_fido2` lib target) so it needed no `Cargo.toml` edit, honoring the Project Structure Notes' "do not touch `Cargo.toml`" constraint.
  - Added `tests/unit/fakes.rs`: `FakeLuksBackend`/`FakeFido2Backend`/`FakeFilesystemBackend`, each constructible via `::passing()` or `::failing(&["dep", ...])`, independently controllable per test — the AD-7 dedicated test-support module, designed to be extended (not rewritten) by later stories as these ports grow more methods.
  - Added `tests/unit/preflight.rs` (3 tests) and `tests/unit/workflows.rs` (4 tests, one per `create`/`unlock`/`close`/`resize`) declared via `mod fakes; mod preflight; mod workflows;` in `tests/unit/main.rs`, replacing the now-superseded `placeholder` test.
  - Verified green: `cargo test --test unit` (7 passed), `make test`, `cargo clippy --all-targets -- -D warnings` (no warnings), `cargo fmt --check`, `cargo build --release`.

### File List

- src/lib.rs
- src/main.rs
- src/ports/luks_backend.rs
- src/ports/fido2_backend.rs
- src/ports/filesystem_backend.rs
- src/domain/errors.rs
- src/domain/preflight.rs
- src/domain/workflows/create.rs
- src/domain/workflows/unlock.rs
- src/domain/workflows/close.rs
- src/domain/workflows/resize.rs
- src/adapters/exec/mod.rs
- tests/unit/main.rs
- tests/unit/fakes.rs
- tests/unit/preflight.rs
- tests/unit/workflows.rs

## Change Log

- 2026-07-23: Implemented Story 1.4 end to end — ports gained `check_prerequisites`, `domain::preflight::check` aggregates all three ports' failures into `DomainError::PreflightFailed`, `create`/`unlock`/`close`/`resize` wire it as their first statement, `adapters::exec::ExecAdapter` provides the real checks, and a lib/bin split plus fake ports enable full unit-test coverage (7 new tests, all green).
