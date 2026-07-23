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

## Deferred from: code review of 1-6-create-a-device-backed-tomb (2026-07-23)

- `has_luks2_header`/`device_capacity`/`luksFormat` shell out unprivileged against real block devices that are typically `root:disk` mode `660`, making `create device` effectively require the whole CLI run under `sudo` — undocumented in `--help`/output. [src/adapters/exec/mod.rs:395-732]
- `has_luks2_header` only detects an existing LUKS2 header, not other filesystem/partition signatures (ext4, xfs, LVM PV, etc.) a device might already carry — matches AC #4's literal scope exactly; broader signature detection is a candidate for a future story. [src/adapters/exec/mod.rs:395-405]

## Deferred from: code review of 1-7-unlock-and-mount-a-tomb (2026-07-24)

- Resize guard (`raw_size > size`) fixes only the exact observed failure mode ("requested size equals full raw capacity"), not the general "not enough headroom for the LUKS2 header" constraint — a size just a few KB under full capacity could plausibly still fail with the same error. [src/adapters/exec/mod.rs:540]
- No automated (fake-backed unit) regression coverage for the `actual_raw_size`/resize-guard fix — it was only caught and confirmed via a manual hardware run, and a future refactor could silently reintroduce it with CI staying green. [tests/unit]
- Mount-directory name (`tomb-fido2-<mapper.name>-<suffix>`) embeds the same deterministic mapping-name hash used for the dm-crypt mapping, a minor local fingerprinting/correlation side channel for any user who can list a world-traversable `/tmp`. [src/adapters/exec/mod.rs:888]
- No plain-language wrapping of unlock failure paths (wrong/missing key, PIN mismatch, path isn't a LUKS2 header at all) — only cryptsetup's own raw stderr plus a generic `AdapterFailure` surfaces today; explicitly Story 1.8's scope per this story's own Dev Notes.
- No forward story currently closes the mount-exposure window (world-readable mount point, see the decision-needed finding on this story) until Story 3.1's `close` ships — process observation, not itself a code defect.
