---
baseline_commit: d7db5f8144a2af0851fa942a8417029c3a58d17d
---

# Story 1.2: CI Runs the Mocked Unit Test Suite

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a contributor,
I want CI to automatically run the mocked unit test suite on every push/PR,
so that I get fast feedback without needing physical FIDO2 hardware.

## Acceptance Criteria

1. **Given** a GitHub Actions workflow triggered on push/PR, **when** the workflow runs, **then** it executes `make test` (the mocked unit suite, AD-7) inside the Nix devShell, **and** a failing `make test` fails the workflow/check. [Source: epics.md#Story 1.2]
2. **Given** the same workflow, **when** it runs, **then** `make test-hardware` is explicitly excluded — never runs in CI, hardware-gated and manual-only. [Source: epics.md#Story 1.2]

## Tasks / Subtasks

- [x] Task 1: Author `.github/workflows/ci.yml` (AC: #1, #2)
  - [x] Trigger on `push` and `pull_request` (no branch filter needed — AR-Dev2 says "every push/PR")
  - [x] `actions/checkout@v4` first, then install Nix with `cachix/install-nix-action@v31` (flakes aren't enabled by default on this action's installed Nix — pass `extra_nix_config: | \n experimental-features = nix-command flakes`)
  - [x] Run the test step as `nix develop -c make test` — do not call `cargo test` directly in CI; the devShell is what provides the pinned toolchain (Story 1.1, AC #1/#2)
  - [x] Do not reference `make test-hardware`, `--test hardware`, or `--ignored` anywhere in the workflow file (AC #2)
  - [x] Suggested full file:
    ```yaml
    name: CI

    on:
      push:
      pull_request:

    jobs:
      test:
        runs-on: ubuntu-latest
        steps:
          - uses: actions/checkout@v4
          - uses: cachix/install-nix-action@v31
            with:
              extra_nix_config: |
                experimental-features = nix-command flakes
          - run: nix develop -c make test
    ```
- [x] Task 2: Verify AC #1's failure path locally before pushing (AC: #1)
  - [x] Confirm `nix develop -c make test` currently exits 0 (both `tests/unit/main.rs`'s placeholder passes and the workflow would go green)
  - [x] Temporarily break a test (e.g. `assert!(false)` in `tests/unit/main.rs`) and confirm `make test` exits non-zero; a non-zero exit from the `run:` step is what fails a GitHub Actions job — revert the temporary breakage before committing
- [x] Task 3: Confirm scope fence against AC #2 (AC: #2)
  - [x] Grep the finished workflow file for `test-hardware`/`--ignored` and confirm zero matches

### Review Findings

- [x] [Review][Patch] Duplicate CI runs on the same commit — `push` and `pull_request` both fire with no dedup [.github/workflows/ci.yml:3-5] — fixed via `concurrency` group
- [x] [Review][Patch] No `timeout-minutes` on the job — a hang (e.g. Nix fetch stall) runs until GitHub's default timeout [.github/workflows/ci.yml:8] — fixed, `timeout-minutes: 15`
- [x] [Review][Patch] No `permissions:` block — job inherits default `GITHUB_TOKEN` scope though it only needs read access [.github/workflows/ci.yml:7] — fixed, `permissions: contents: read`
- [x] [Review][Patch] Task 1's "Suggested full file" subtask left unchecked while its siblings and the actual shipped file are done [1-2-ci-runs-the-mocked-unit-test-suite.md:29] — fixed, checkbox checked
- [x] [Review][Defer] Nix binary version isn't pinned by `cachix/install-nix-action@v31`, only nixpkgs is pinned via `flake.lock` [.github/workflows/ci.yml:10] — deferred, pre-existing tooling choice not required by this story's ACs
- [x] [Review][Defer] No branch protection rule requires this check to pass before merge, so a red run doesn't yet block merges — deferred, repo-setting change outside this diff's scope

## Dev Notes

- This story is CI-only — no `src/` changes. The only new file is `.github/workflows/ci.yml`; nothing under `src/` or `tests/` should change.
- `make test` and `make test-hardware` already exist and are already correctly separated (built in Story 1.1, not something this story re-implements):
  ```makefile
  test:
      cargo test --test unit

  test-hardware:
      cargo test --test hardware -- --ignored
  ```
  `tests/unit/main.rs` and `tests/hardware/main.rs` are cargo's auto-discovered integration-test binaries (the `tests/<name>/main.rs` layout is equivalent to `tests/<name>.rs`) — this is why `--test unit` / `--test hardware` work as target names. This story's only job is wiring `make test` into CI; it must never invoke `make test-hardware` (AD-7, AC #2).
  [Source: Makefile] [Source: ARCHITECTURE-SPINE.md#AD-7]
- AD-7: mocked unit suite runs against fakes in default CI; the hardware-gated suite exercises real adapters + a real FIDO2 device and stays manual-only (`make test-hardware`), excluded from CI. [Source: ARCHITECTURE-SPINE.md#AD-7]
- AR-Dev2 (this story's source requirement): "A GitHub Actions workflow runs `make test` (mocked unit suite, per AD-7) on every push/PR. `make test-hardware` is explicitly excluded from this workflow — manual/local only." [Source: epics.md#Additional Requirements — Tooling / DevOps]
- Use the Nix devShell inside CI (`nix develop -c make test`), not a bare `cargo test` runner action — the whole point of Story 1.1's devShell is that CI and contributors build against the identical pinned toolchain (`flake.lock`'s locked `nixos-unstable` rev), not a GitHub-Actions-provided Rust that could drift from it.
- `cachix/install-nix-action@v31` is the current stable major version (semver-tagged, `v31` tracks latest minor/patch) as of this story's creation (2026-07-22) — confirmed via web search, not from training-data memory. It does not enable Nix flakes by default, hence the required `extra_nix_config` block.
- No caching (e.g. Cachix binary cache, `magic-nix-cache-action`) is required by either AC — out of scope for this story; only add if a future story explicitly asks for CI speed improvements.
- No previous-story code patterns apply beyond what's noted above — Story 1.1 didn't touch CI. Git log so far is `feat(1.1): project scaffolding & Nix devShell (#2)` plus prior BMad planning commits; no `.github/` directory exists yet, this story creates it for the first time.

### Project Structure Notes

- New file: `.github/workflows/ci.yml`. No existing project structure conflicts — `.github/workflows/` doesn't exist yet.
- Follows `ARCHITECTURE-SPINE.md`'s Structural Seed, which lists `Makefile` and the Nix devShell as the CI-relevant surface this workflow wires together; the seed doesn't separately enumerate `.github/` since it's tooling, not domain structure.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.2: CI Runs the Mocked Unit Test Suite]
- [Source: _bmad-output/planning-artifacts/epics.md#Additional Requirements — Tooling / DevOps (AR-Dev2)]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-7]
- [Source: Makefile], [Source: flake.lock] (existing devShell/test wiring from Story 1.1)
- [Source: _bmad-output/implementation-artifacts/1-1-project-scaffolding-nix-devshell.md] (previous story — established the devShell, Makefile, and unit/hardware test-binary split this story wires into CI)

## Dev Agent Record

### Agent Model Used

Claude Sonnet 5 (claude-sonnet-5)

### Debug Log References

### Completion Notes List

- Task 2: Verified `nix develop -c make test` exits 0 with the current placeholder test; temporarily set `assert!(false)` in `tests/unit/main.rs` and confirmed `make` reports `Error 101` (non-zero exit), then reverted — no net diff in `tests/unit/main.rs`.
- Task 3: `grep -n -E "test-hardware|--ignored" .github/workflows/ci.yml` returns zero matches (exit 1).

### File List

- `.github/workflows/ci.yml` (new)

## Change Log

- 2026-07-22: Implemented Story 1.2 — added `.github/workflows/ci.yml` running `nix develop -c make test` on push/PR; verified the failure path locally; confirmed no `test-hardware`/`--ignored` references in the workflow.
