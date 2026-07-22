# Deferred Work

## Deferred from: code review of 1-1-project-scaffolding-nix-devshell (2026-07-22)

- `ARCHITECTURE-SPINE.md`'s Stack table has a stale combined `serde`+`serde_json` version figure (`1.0.229`) that caused Story 1.1's `serde_json` pin deviation and will mislead future stories reading the table. [ARCHITECTURE-SPINE.md#Stack]
- Dev Agent Record's claim that `nix develop -c cargo build` (not a bare system `cargo build`) was used for Story 1.1's Task 1 verification isn't independently verifiable from repo state, since a matching system Rust toolchain happens to be present. [1-1-project-scaffolding-nix-devshell.md:82]
- `Makefile` targets (`build`/`test`/`test-hardware`) don't pass `--locked` to cargo, so a `Cargo.lock`/`Cargo.toml` drift would silently re-resolve rather than fail fast. [Makefile:3-10]
- No `LICENSE` file despite the README referencing GitHub Releases distribution. [repo root]
