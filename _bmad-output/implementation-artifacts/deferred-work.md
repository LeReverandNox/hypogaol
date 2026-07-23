# Deferred Work

## Deferred from: code review of 1-1-project-scaffolding-nix-devshell (2026-07-22)

- `ARCHITECTURE-SPINE.md`'s Stack table has a stale combined `serde`+`serde_json` version figure (`1.0.229`) that caused Story 1.1's `serde_json` pin deviation and will mislead future stories reading the table. [ARCHITECTURE-SPINE.md#Stack]
- Dev Agent Record's claim that `nix develop -c cargo build` (not a bare system `cargo build`) was used for Story 1.1's Task 1 verification isn't independently verifiable from repo state, since a matching system Rust toolchain happens to be present. [1-1-project-scaffolding-nix-devshell.md:82]
- `Makefile` targets (`build`/`test`/`test-hardware`) don't pass `--locked` to cargo, so a `Cargo.lock`/`Cargo.toml` drift would silently re-resolve rather than fail fast. [Makefile:3-10]
- No `LICENSE` file despite the README referencing GitHub Releases distribution. [repo root]

## Deferred from: code review of 1-2-ci-runs-the-mocked-unit-test-suite (2026-07-22)

- Nix binary version isn't pinned by `cachix/install-nix-action@v31`, only nixpkgs is pinned via `flake.lock` — a future Nix release could silently change CI behavior without a diff to this repo. [.github/workflows/ci.yml:10]
- No branch protection rule requires the CI check to pass before merge, so a red `make test` run doesn't yet block merges — the AC is satisfied at the check-reporting level but not enforced at merge time. [.github/workflows/ci.yml]

## Deferred from: code review of 1-3-release-automation (2026-07-23)

- `release.yml`'s `pull_request:` trigger (no path filter) runs the `plan` job — including a curl-install of cargo-dist — on every PR, even docs-only ones. Standard `cargo dist generate-ci` output; Dev Notes forbid hand-authoring this file. [.github/workflows/release.yml:69-71]
- `release.yml` has no `concurrency:` guard, unlike `release-please.yml` — rapid tag pushes could race the `plan`/`host` jobs. Same generated-file constraint applies. [.github/workflows/release.yml]
- `release.yml` pins `ubuntu-22.04` runners (vs. `ubuntu-latest`) and has no dependency on Story 1.2's `ci.yml` passing before a tagged commit's binaries get built/published — the latter relies entirely on GitHub branch-protection settings outside this diff's scope. [.github/workflows/release.yml]
- cargo-dist is installed via `curl ... | sh` with no checksum/signature verification — standard cargo-dist-generated install step, same generated-file constraint. [.github/workflows/release.yml:94-95]
- `Cargo.toml`'s `[package]` section still has no `license`/`description` — relevant once this repo goes public, but out of this story's scope and would touch `[package]` further. [Cargo.toml]
- No live dry run of the full pipeline was completed (release-please `--dry-run` blocked by a CLI auth quirk per Completion Notes, not independently re-verified here); watch the first real merge-to-main closely.
- A pushed tag whose version doesn't match `Cargo.toml`'s version (no automated linkage enforced) surfaces as an opaque `dist plan` failure rather than a clear message.
- A manually pushed tag not created by release-please would fail outright at `gh release upload`/`edit` since `create-release = false` assumes the release already exists — inherent to the documented two-workflow chain; not a supported flow.
- release-please has no `bootstrap-sha`, so the very first release's changelog will include the entire commit history including internal story-process commits — acceptable for a project's first-ever release.

## Deferred from: code review of 1-5-create-a-file-backed-tomb (2026-07-23)

- `enroll_fido2_key` hardcodes `--fido2-device=auto`, with no way to target a specific device when multiple FIDO2 authenticators are attached — a known v1 scope limitation, not required by this story's ACs. [src/adapters/exec/mod.rs:434]
