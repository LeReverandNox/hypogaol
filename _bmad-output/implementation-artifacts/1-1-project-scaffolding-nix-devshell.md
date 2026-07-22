# Story 1.1: Project Scaffolding & Nix DevShell

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a contributor,
I want a reproducible Nix devShell environment,
so that I can build and test the tool without installing cryptsetup/systemd/libfido2 system-wide.

## Acceptance Criteria

1. **Given** a fresh clone of the repository, **when** I run `nix develop`, **then** a devShell activates providing the pinned Rust toolchain plus `cryptsetup`, `systemd`, and `libfido2` on PATH, **and** `cargo build` succeeds inside the devShell with no additional system-wide package installation. [Source: epics.md#Story 1.1]
2. **Given** `flake.lock`, **when** the devShell is entered on a different machine, **then** the same pinned nixpkgs-unstable revision is used, so the environment is reproducible across machines. [Source: epics.md#Story 1.1]
3. **Given** the greenfield structural seed (`src/domain/{workflows,preflight.rs,errors.rs}`, `src/ports/*`, `src/adapters/exec/`, `src/cli/*`, `tests/{unit,hardware}/`), **when** the scaffolding is created, **then** the crate compiles (`cargo build`) with stub module bodies and no logic yet. [Source: epics.md#Story 1.1]

## Tasks / Subtasks

- [ ] Task 1: Verify the existing Nix devShell satisfies AC #1 and AC #2 (AC: #1, #2)
  - [ ] `flake.nix` and `flake.lock` already exist at repo root — do not recreate them, only amend if a gap is found
  - [ ] Confirm `devShells.default` packages include `rustc`, `cargo`, `cryptsetup`, `systemd`, `libfido2` (already present) — [Source: flake.nix]
  - [ ] Confirm `flake.lock`'s `nixpkgs` input is a locked `nixos-unstable` revision (already present, locked 2025-11 timestamp) — this is what makes the environment reproducible across machines (AC #2)
  - [ ] After Task 2/3 create `Cargo.toml`/`src/`, run `nix develop -c cargo build` and confirm it succeeds with zero system-wide installs
- [ ] Task 2: Create `Cargo.toml` for a binary crate named after the placeholder product (AC: #1, #3)
  - [ ] `name = "tomb-fido2"`, `edition = "2021"` — this name is the **single source** the CLI binary/user-facing name must read from later (AD-13); never hardcode `"tomb-fido2"` as a separate string literal elsewhere in `src/`
  - [ ] Declare dependencies pinned to the exact versions the architecture already resolved — they may be unused by the stub bodies in this story, that is expected and not a build error: `clap = "4.6.4"`, `serde = "1.0.229"` (with `derive` feature), `serde_json = "1.0.229"`, `thiserror = "2.0.19"`, `anyhow = "1.0.104"`, `zeroize = "1.9.0"` [Source: ARCHITECTURE-SPINE.md#Stack]
  - [ ] Do not add a `[[bin]]` section — a crate-root `src/main.rs` makes the binary name default to the package name, which is what AD-13 requires
- [ ] Task 3: Create the structural seed with stub bodies only, no logic (AC: #3)
  - [ ] `src/main.rs` — thin entry point only: wires `mod` declarations and delegates to `cli` (Cargo requires this file at crate root even though the architecture's own module map lists the CLI entry as `src/cli/main.rs`; keep this file to a couple of lines, all real CLI wiring belongs in `src/cli/main.rs`)
  - [ ] `src/domain/workflows/{create,unlock,enroll,revoke,close,resize}.rs` — one stub function per workflow, e.g. `pub fn run() -> Result<(), crate::domain::errors::DomainError> { todo!() }`
  - [ ] `src/domain/preflight.rs` — stub `pub fn check() -> Result<(), DomainError> { todo!() }` (CAP-6 gate, implemented in Story 1.4)
  - [ ] `src/domain/mapping_name.rs` — stub for the shared canonicalize+hash helper (AD-12); listed in the architecture's Structural Seed tree though not spelled out in this story's AC #3 shorthand — include it now so Story 1.7/3.1/3.2 don't need to touch the module skeleton
  - [ ] `src/domain/errors.rs` — the typed domain error enum (`thiserror`), even if it starts with zero variants
  - [ ] `src/ports/{luks_backend,fido2_backend,filesystem_backend}.rs` — one empty trait declaration per port, no methods required yet
  - [ ] `src/adapters/exec/mod.rs` — empty module, will implement the three ports starting in later stories
  - [ ] `src/cli/main.rs`, `src/cli/ux.rs` — stub `pub fn run() {}` / empty module
  - [ ] `tests/unit/main.rs` — one trivial passing test (e.g. `#[test] fn placeholder() {}`); this is where AD-7's shared fake-port test-support module will live starting Story 1.4
  - [ ] `tests/hardware/main.rs` — one trivial passing test, gated so it never runs under plain `cargo test`/`make test` (e.g. behind a `hardware` Cargo feature, or `#[ignore]` — dev's choice, but default `cargo test` must not execute it, per AD-7 / Story 1.2 AC)
  - [ ] Run `cargo build` (inside the devShell) and confirm it succeeds with no warnings-as-errors

## Dev Notes

- This is the first story in Epic 1 and the first implementation story in the repo — there is no previous story, no established code patterns, and no prior code commits to learn from (git log so far is BMad planning-artifact commits only: `chore(bmad): ...`).
- `flake.nix`, `flake.lock`, and `README.md` were already created during an earlier planning/bootstrap pass and already satisfy AC #1/#2 as written — this story's actual remaining work is the Rust crate skeleton (Tasks 2-3), plus a verification pass on the devShell (Task 1). Do not regenerate `flake.nix`/`flake.lock` from scratch.
- Hexagonal (ports & adapters): `domain` depends only on the three trait-defined ports; `adapters::exec` will implement those ports for real starting in later stories; `cli` sits above `domain` and must never touch a port directly (AD-1, enforced starting Story 1.8, but the module boundary starts here). [Source: ARCHITECTURE-SPINE.md#Design Paradigm]
- AD-7 testing strategy: `domain` workflows are unit-tested against one shared fake port implementation in default CI (`tests/unit`); a separate hardware-gated suite (`tests/hardware`) exercises real adapters + a real FIDO2 device and is excluded from default CI, run manually via `make test-hardware`. This story only needs the two test-binary skeletons to exist and pass trivially — the real fake-port test-support module and real hardware harness are built out starting Story 1.4 (preflight) and Story 1.5 (create). [Source: ARCHITECTURE-SPINE.md#AD-7]
- AD-13 placeholder-name isolation: the product name (`tomb-fido2`) is a placeholder pending a permanent name. The Cargo package name is the **one** source of truth for the CLI binary name / user-facing product string — never duplicate it as a separate literal in `cli`, error messages, or elsewhere, so a future rename touches one line. [Source: ARCHITECTURE-SPINE.md#AD-13]
- No PRD file exists for this project — the SPEC (`_bmad-output/specs/spec-tomb-fido2/SPEC.md`) is the PRD-equivalent contract, and epics.md already carries the relevant SPEC content inline; SPEC.md itself has no scaffolding-specific detail beyond what's already folded into epics.md and the architecture Stack table.
- No UX design contract exists — this is a CLI-only tool (per epics.md UX Design Requirements: None).
- A Makefile is not required by this story's AC's literal text, but the architecture's Structural Seed lists one (`make build` → `cargo build --release`; `make test`; `make test-hardware`) and Story 1.2 (CI) needs `make test` to already exist to wire into the GitHub Actions workflow. Recommend creating a minimal `Makefile` now with those three targets even though it's not strictly gated by an AC here — flag this decision in the PR/commit if deferring it instead.

### Project Structure Notes

- Follows `ARCHITECTURE-SPINE.md`'s Structural Seed tree exactly: `src/domain/{workflows/,preflight.rs,mapping_name.rs,errors.rs}`, `src/ports/{luks_backend,fido2_backend,filesystem_backend}.rs`, `src/adapters/exec/`, `src/cli/{main.rs,ux.rs}`, `tests/{unit,hardware}/`.
- No starter template — this is a fully greenfield crate; nothing existing to preserve compatibility with inside `src/`.
- `flake.nix`/`flake.lock`/`README.md` at repo root already exist and are out of scope to recreate — only touch them if Task 1's verification finds an actual gap against AC #1/#2.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.1: Project Scaffolding & Nix DevShell]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-1, #AD-7, #AD-13, #Stack, #Structural Seed]
- [Source: _bmad-output/specs/spec-tomb-fido2/SPEC.md#Constraints] (placeholder-name constraint, Rust-or-Go → Rust per architecture)
- [Source: flake.nix], [Source: flake.lock] (already-existing devShell definition)

## Dev Agent Record

### Agent Model Used

_To be filled by the dev agent during implementation._

### Debug Log References

### Completion Notes List

- Ultimate context engine analysis completed - comprehensive developer guide created.

### File List
