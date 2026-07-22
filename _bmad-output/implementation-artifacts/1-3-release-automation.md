---
baseline_commit: 50e604a
---

# Story 1.3: Release Automation

Status: review

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

- [x] Task 1: Wire release-please (AC: #1)
  - [x] Add `.github/workflows/release-please.yml` — triggers on `push` to `main`, uses `googleapis/release-please-action@v4` (pins to release-please v17.10.4 per architecture, AR-Dev3), `permissions: contents: write, pull-requests: write` (release-please needs to open/update PRs and push tags — narrower than default but broader than CI's `contents: read`)
  - [x] Add `release-please-config.json` at repo root: `{"release-type": "rust", "packages": {".": {}}}` — `release-type: rust` makes release-please bump the `version` field in `Cargo.toml` directly (no separate manifest-only bump needed for a single-crate repo)
  - [x] Add `.release-please-manifest.json` at repo root: `{".": "0.0.0"}` — must match `Cargo.toml`'s current `version = "0.0.0"` exactly, or release-please's first run miscalculates the diff
  - [x] Verify release-please's default tag format is `v${version}` (e.g. `v0.1.0`) — this is what cargo-dist's generated workflow must match in Task 2
- [x] Task 2: Wire cargo-dist (AC: #2)
  - [x] Add `[workspace.metadata.dist]` to `Cargo.toml` (cargo-dist ~0.32.x, per architecture) — set `cargo-dist-version` to the pinned `0.32.x`, `ci = ["github"]`, and pick target triples covering the platforms this tool runs on: `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` at minimum (Linux-only tool — see Dev Notes)
  - [x] Set `create-release = false` in the same `[workspace.metadata.dist]` block — release-please (Task 1) already creates the GitHub Release and tag; cargo-dist must only attach build artifacts to that existing release, never create a second one
  - [x] Generate cargo-dist's own CI via `cargo dist generate-ci github` (requires `cargo-dist` installed locally at the pinned version first — do not hand-write this workflow file, its structure is version-specific and regenerating it is the supported upgrade path) — this produces `.github/workflows/release.yml` (name it distinctly from Task 1's `release-please.yml`) with a tag-push trigger
  - [x] Confirm the generated workflow's tag-match pattern (default `v[0-9]+.[0-9]+.[0-9]+*`) lines up with release-please's `v${version}` tag format from Task 1 — if `cargo dist init` prompted for a different tag pattern, reconcile it
- [x] Task 3: Confirm scope fence against AC #3 (AC: #3)
  - [x] Grep repo for any packaging config outside `[workspace.metadata.dist]`'s GitHub-Releases-only output (e.g. no `.deb`/PKGBUILD/AUR/`cargo-generate-rpm` config) — confirm zero matches
  - [x] `cargo-dist`'s own installer-generation features (shell/powershell installer scripts, Homebrew tap, etc.) are optional add-ons on top of raw binaries — do not enable any of them unless a future story asks; this story's scope is raw GitHub Releases binaries only

### Review Findings

- [x] [Review][Decision] Cargo.toml `[package]` section modified beyond the story's stated scope — Dev Notes say "its `[package]` section is untouched by this story" / "do not touch `[package]` or `[dependencies]`", but the diff adds `repository = "..."` and a new `[package.metadata.dist]\ndist = true` table inside/adjacent to `[package]`. Both are functionally required for `cargo dist generate-ci` to succeed (documented in Completion Notes: `publish = false` hides the crate from cargo-dist without `dist = true`; GitHub CI generation hard-requires a `repository` URL). [Cargo.toml:6-9] — resolved: accepted as a necessary, already-justified deviation, no code change
- [x] [Review][Decision] Commit `f1b2898` ("feat(1.3): confirm release-tooling scope fence and mark story ready for review") was internal process bookkeeping labeled as a user-facing `feat`, which release-please would have surfaced verbatim in the generated CHANGELOG. — resolved: amended to `chore(1.3): ...` (now `df581d3`) and force-pushed to `origin/story/1.3-release-automation`
- [x] [Review][Patch] `release-please-config.json` missing `"draft": true` — release-please's default (`draft: false`) publishes the GitHub Release live the moment the release PR merges/tags, before cargo-dist has built or attached any binaries. `release.yml`'s own header comment assumes "a GitHub Release with this tag is assumed to exist as a draft ... and will be undrafted for you" — this is the documented release-please+cargo-dist integration contract. [release-please-config.json] — fixed
- [x] [Review][Patch] Neither new workflow has a `concurrency:` group (existing `ci.yml` does) — add one to `release-please.yml` (hand-authored, safe to edit) to prevent overlapping runs on rapid successive pushes to `main`. [.github/workflows/release-please.yml] — fixed
- [x] [Review][Defer] `release.yml`'s `pull_request:` trigger (no path filter) runs the `plan` job — including a curl-install of cargo-dist — on every PR, even docs-only ones. Standard `cargo dist generate-ci` output; Dev Notes forbid hand-authoring this file. [.github/workflows/release.yml:69-71] — deferred, generated-file constraint
- [x] [Review][Defer] `release.yml` has no `concurrency:` guard either, for the same generated-file reason — rapid tag pushes could race the `plan`/`host` jobs. [.github/workflows/release.yml] — deferred, generated-file constraint
- [x] [Review][Defer] `release.yml` pins `ubuntu-22.04` runners (vs. `ubuntu-latest`) and has no dependency on Story 1.2's `ci.yml` passing before a tagged commit's binaries get built/published — the latter currently relies entirely on GitHub branch-protection settings outside this diff's scope. [.github/workflows/release.yml] — deferred, generated-file constraint / out-of-scope config
- [x] [Review][Defer] cargo-dist is installed via `curl ... | sh` with no checksum/signature verification — standard cargo-dist-generated install step, same generated-file constraint. [.github/workflows/release.yml:94-95] — deferred, generated-file constraint
- [x] [Review][Defer] `Cargo.toml`'s `[package]` section still has no `license`/`description` — relevant once this repo goes public, but out of this story's scope and would touch `[package]` further. [Cargo.toml] — deferred, future story
- [x] [Review][Defer] No live dry run of the full pipeline was completed (release-please `--dry-run` blocked by a CLI auth quirk per Completion Notes, not independently re-verified here); exercising it for real requires a live tag/PR push, not appropriate from a review pass. — deferred, watch first real merge-to-main closely
- [x] [Review][Defer] A pushed tag whose version doesn't match `Cargo.toml`'s version (no automated linkage enforced) surfaces as an opaque `dist plan` failure rather than a clear message. — deferred, edge case outside sanctioned flow
- [x] [Review][Defer] A manually pushed tag not created by release-please would fail outright at `gh release upload`/`edit` since `create-release = false` assumes the release already exists — inherent to the documented two-workflow chain; Dev Notes forbid inventing a different sequencing. — deferred, not a supported flow
- [x] [Review][Defer] release-please has no `bootstrap-sha`, so the very first release's changelog will include the entire commit history including internal story-process commits. — deferred, acceptable for a project's first-ever release

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

claude-sonnet-5

### Debug Log References

### Completion Notes List

- Task 1: Validated `release-please-config.json` and `.release-please-manifest.json` as syntactically valid JSON (`jq .`), and `.github/workflows/release-please.yml` as valid YAML (`python3 -c "import yaml; yaml.safe_load(...)"`) with `permissions: {contents: write, pull-requests: write}` asserted programmatically. Cross-checked `.release-please-manifest.json`'s `"."` version (`0.0.0`) against `Cargo.toml`'s `version` field — exact match. A live `release-please --dry-run` against the (private) GitHub repo was attempted but blocked by an auth quirk in the release-please CLI's `defaultBranch` lookup (token not attached to that specific request, confirmed via unauthenticated `x-ratelimit-limit: 60` on the resulting 404) — not pursued further as it's a CLI-auth wiring issue orthogonal to this story's config correctness. Tag format `v${version}` (no component prefix) for a single package at path `.` is release-please's documented default (matches architecture's Stack table note and this task's own guidance) — not independently re-derived via dry-run for the reason above.
- Task 2: Installed the pinned `cargo-dist 0.32.0` binary locally (via the official `cargo-dist-installer.sh` for that exact tag, confirmed with `cargo dist --version`) to run `cargo dist generate-ci` for real rather than hand-writing the workflow. Two deviations beyond the literal task text were required for `cargo dist generate-ci` to succeed at all, both scoped narrowly and documented here:
  - Added `[package.metadata.dist]\ndist = true` — cargo-dist excludes non-publishable (`publish = false`) packages from its release set by default; without this override it errors with "workspace doesn't have anything for dist to Release".
  - Added `repository = "https://github.com/LeReverandNox/tomb-fido2"` to `[package]` — cargo-dist's GitHub CI generation hard-requires a repository URL to target; without it, generation errors immediately.
  - `cargo dist generate-ci` (no `github` positional arg — that syntax is deprecated in 0.32.0; CI target is read from `[workspace.metadata.dist].ci` instead) produced `.github/workflows/release.yml` unmodified from its generated form, per Dev Notes ("do not hand-author").
  - Confirmed `create-release = false` took effect: the generated `host` job only runs `dist host --steps=upload --steps=release` and `gh release edit --draft=false` against an already-existing release — it never creates one, matching Task 1's release-please-owned release/tag.
  - Confirmed the generated tag trigger (`push: tags: ['**[0-9]+.[0-9]+.[0-9]+*']`) matches release-please's `v${version}` tags per the workflow's own header comment (explicitly lists `"v0.1.0-prerelease.1"` as a matching example) — no reconciliation needed.
  - `aarch64-unknown-linux-gnu` build/cross-compile logic is entirely computed at runtime by `dist plan` (matrix is data-driven, not hand-maintained in the generated YAML), so keeping both target triples doesn't add hand-authored CI complexity.
- Task 3: `grep -rniE "\.deb\b|PKGBUILD|AUR\b|cargo-generate-rpm|\.rpm\b|debian/|homebrew|\.wxs\b|msi" --include="*.toml" --include="*.json" --include="*.yml" --include="*.yaml"` across the repo (excluding `target/`) returns zero matches (grep exit 1). Confirmed no `installers` key is set in `[workspace.metadata.dist]` — cargo-dist's optional installer generators (shell/powershell/npm/homebrew/msi) are all off; only raw archived binaries are produced.

### File List

- `.github/workflows/release-please.yml` (new)
- `release-please-config.json` (new)
- `.release-please-manifest.json` (new)
- `Cargo.toml` (modified — added `repository`, `[package.metadata.dist]`, `[workspace.metadata.dist]`)
- `.github/workflows/release.yml` (new, generated by `cargo dist generate-ci`)

## Change Log

- 2026-07-22: Implemented Story 1.3 — wired `release-please-action@v4` (config-as-code, manifest seeded at `0.0.0`) to propose version-bump/changelog PRs and tag releases from conventional commits; wired `cargo-dist` 0.32.0 (`create-release = false`) to attach `x86_64`/`aarch64` Linux binaries to those releases via a generated `.github/workflows/release.yml`; confirmed zero out-of-scope distro-packaging config (AC #3). Regression suite (`nix develop -c make test`) still passes; no `src/` changes.
