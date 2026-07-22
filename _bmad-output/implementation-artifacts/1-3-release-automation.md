---
baseline_commit: 50e604a
---

# Story 1.3: Release Automation

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a maintainer,
I want versioned changelog generation and cross-platform release binaries published automatically,
so that users can download a ready-to-run binary without me manually cutting each release.

## Acceptance Criteria

1. **Given** commits in conventional-commit format merged to main, **when** release-please runs, **then** it proposes/maintains a release PR with version bump and changelog derived from those commits. [Source: epics.md#Story 1.3]
2. **Given** a release-please release is merged/tagged, **when** cargo-dist's workflow runs, **then** it builds and publishes release binaries to GitHub Releases across the target platforms. [Source: epics.md#Story 1.3]
3. **Given** this release tooling, **when** checking scope, **then** distro packaging (AUR, deb, etc.) beyond GitHub Releases prebuilt binaries and `cargo build --release` is explicitly out of scope for v1. [Source: epics.md#Story 1.3, AR-Dev4]

## Tasks / Subtasks

- [ ] Task 1: Wire release-please (AC: #1)
  - [ ] Add `.github/workflows/release-please.yml` — triggers on `push` to `main`, uses `googleapis/release-please-action@v4` (pins to release-please v17.10.4 per architecture, AR-Dev3), `permissions: contents: write, pull-requests: write` (release-please needs to open/update PRs and push tags — narrower than default but broader than CI's `contents: read`)
  - [ ] Add `release-please-config.json` at repo root: `{"release-type": "rust", "packages": {".": {}}}` — `release-type: rust` makes release-please bump the `version` field in `Cargo.toml` directly (no separate manifest-only bump needed for a single-crate repo)
  - [ ] Add `.release-please-manifest.json` at repo root: `{".": "0.0.0"}` — must match `Cargo.toml`'s current `version = "0.0.0"` exactly, or release-please's first run miscalculates the diff
  - [ ] Verify release-please's default tag format is `v${version}` (e.g. `v0.1.0`) — this is what cargo-dist's generated workflow must match in Task 2
- [ ] Task 2: Wire cargo-dist (AC: #2)
  - [ ] Add `[workspace.metadata.dist]` to `Cargo.toml` (cargo-dist ~0.32.x, per architecture) — set `cargo-dist-version` to the pinned `0.32.x`, `ci = ["github"]`, and pick target triples covering the platforms this tool runs on: `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` at minimum (Linux-only tool — see Dev Notes)
  - [ ] Set `create-release = false` in the same `[workspace.metadata.dist]` block — release-please (Task 1) already creates the GitHub Release and tag; cargo-dist must only attach build artifacts to that existing release, never create a second one
  - [ ] Generate cargo-dist's own CI via `cargo dist generate-ci github` (requires `cargo-dist` installed locally at the pinned version first — do not hand-write this workflow file, its structure is version-specific and regenerating it is the supported upgrade path) — this produces `.github/workflows/release.yml` (name it distinctly from Task 1's `release-please.yml`) with a tag-push trigger
  - [ ] Confirm the generated workflow's tag-match pattern (default `v[0-9]+.[0-9]+.[0-9]+*`) lines up with release-please's `v${version}` tag format from Task 1 — if `cargo dist init` prompted for a different tag pattern, reconcile it
- [ ] Task 3: Confirm scope fence against AC #3 (AC: #3)
  - [ ] Grep repo for any packaging config outside `[workspace.metadata.dist]`'s GitHub-Releases-only output (e.g. no `.deb`/PKGBUILD/AUR/`cargo-generate-rpm` config) — confirm zero matches
  - [ ] `cargo-dist`'s own installer-generation features (shell/powershell installer scripts, Homebrew tap, etc.) are optional add-ons on top of raw binaries — do not enable any of them unless a future story asks; this story's scope is raw GitHub Releases binaries only

## Dev Notes

- **This story is CI/release-config-only** — no `src/` changes, same shape as Story 1.2. New root-level files: `release-please-config.json`, `.release-please-manifest.json`, plus two new workflow files under `.github/workflows/`. `Cargo.toml` gets a new `[workspace.metadata.dist]` table appended, its `[package]` section is untouched by this story (release-please will edit `version` on its own subsequent PRs, not this story's commit).
- **Two independent, chained workflows** (AR-Dev3, ARCHITECTURE-SPINE.md Stack table): release-please (v17.10.4) proposes/merges version-bump PRs from conventional commits and creates the tag + GitHub Release; cargo-dist (~0.32.x) reacts to that tag being pushed and uploads cross-platform binaries to the *same* release. They must not both try to create the release — hence `create-release = false` in cargo-dist's config (see Task 2). This is the standard release-please + cargo-dist pairing; do not invent a different sequencing (e.g. do not have cargo-dist create its own tag/release).
- **Target platforms:** SPEC/architecture describes this as a Linux-only tool (`Linux only` in the Stack line, all backends shell out to `cryptsetup`/`systemd`/`fido2-token`/e2fsprogs — none of which are cross-platform). Do not add macOS/Windows targets to `[workspace.metadata.dist]` — there is nothing for this tool to do there. `x86_64-unknown-linux-gnu` is the minimum; add `aarch64-unknown-linux-gnu` for ARM Linux users unless that meaningfully complicates the CI matrix, in which case defer it and note why.
- **Do not hand-author cargo-dist's release workflow YAML.** Its shape (matrix jobs, plan/build/host/publish/announce phases) is generated and versioned by the `cargo-dist-version` pin; use `cargo dist init` / `cargo dist generate-ci github` locally to produce it, matching the pinned 0.32.x version noted in `ARCHITECTURE-SPINE.md`'s Stack table. Hand-writing it risks drifting from what that pinned version actually expects at runtime.
- **release-please-action pin:** use `googleapis/release-please-action@v4` (the current major tag), configured to use release-please core v17.10.4 behavior via the config files above — matches the version already recorded in `ARCHITECTURE-SPINE.md`'s Stack table (confirmed there as of this story's creation date, not re-verified here).
- **Permissions:** release-please's workflow needs `contents: write` + `pull-requests: write` (it pushes commits/tags and opens/updates PRs) — broader than Story 1.2's CI workflow (`contents: read`). Keep the two workflows' permission blocks scoped independently; do not widen CI's.
- AR-Dev4 (scope fence, epics.md Additional Requirements): distro packaging (AUR, deb, etc.) beyond GitHub Releases prebuilt binaries + `cargo build --release` is out of scope for v1 — this binds AC #3 directly.
- No previous-story code pattern conflicts: Story 1.2 only added `.github/workflows/ci.yml` (Nix-based `nix develop -c make test`); this story's new workflows are independent triggers (`push: main` and tag-push respectively) and don't need the Nix devShell — cargo-dist and release-please-action manage their own toolchains.

### Project Structure Notes

- New files: `.github/workflows/release-please.yml`, `.github/workflows/release.yml` (cargo-dist-generated, name may differ slightly if `cargo dist generate-ci` defaults to something else — keep whatever it generates rather than renaming), `release-please-config.json`, `.release-please-manifest.json`.
- Modified file: `Cargo.toml` — append-only `[workspace.metadata.dist]` table; do not touch `[package]` or `[dependencies]`.
- No conflicts with `ARCHITECTURE-SPINE.md`'s Structural Seed — it doesn't separately enumerate `.github/` (tooling, not domain structure), consistent with how Story 1.2 was scoped.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.3: Release Automation]
- [Source: _bmad-output/planning-artifacts/epics.md#Additional Requirements — Tooling / DevOps (AR-Dev3, AR-Dev4)]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#Stack table] (cargo-dist ~0.32.x, release-please v17.10.4)
- [Source: Cargo.toml] (current `version = "0.0.0"`, `publish = false` — must match manifest seed)
- [Source: _bmad-output/implementation-artifacts/1-2-ci-runs-the-mocked-unit-test-suite.md] (previous story — established `.github/workflows/` and the pattern of scoping each workflow's permissions independently)
- [cargo-dist docs](https://axodotdev.github.io/cargo-dist/) — `cargo dist init`/`generate-ci` usage, `create-release` config option

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

### File List
