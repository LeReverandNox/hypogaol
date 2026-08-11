---
baseline_commit: 6ba3bde29208a985a8f8bbd8daa40d9506f04a9f
---

# Story 6.8: Repository Hygiene Badges

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a contributor or evaluator landing on the README,
I want to see build status, license, latest release, and MSRV badges at a glance,
so that I can judge the project's health without digging through CI or config files.

## Acceptance Criteria

1. **Given** the README header, **when** I view it, **then** it displays build-status, license (GPL-3.0-or-later), latest-release, and MSRV badges, each linking to the resource it reflects (Actions run, LICENSE file, GitHub Releases). [Source: epics.md#Story 6.8, lines 954-956]
2. **Given** CI does not yet run coverage instrumentation, **when** this story lands, **then** `cargo-llvm-cov` is added to CI, reporting to Codecov, and a coverage badge is added linking to it. [Source: epics.md#Story 6.8, lines 958-960]
3. **Given** CI does not yet run a security audit, **when** this story lands, **then** `cargo-audit` is added as a gating CI job (a known RustSec advisory fails the build, not just the badge), and a security-audit badge is added. [Source: epics.md#Story 6.8, lines 962-964]
4. **Given** all six badges, **when** they're added, **then** each is a live, working link — not a placeholder image. [Source: epics.md#Story 6.8, lines 966-968]

## Tasks / Subtasks

- [x] **Task 0: Read every file this story touches before changing anything** (AC: all)
  - Read in full: `README.md` (top ~40 lines, header/title area only — badges are inserted there), `Cargo.toml` (30 lines, no `rust-version` field today — confirmed via `grep -n rust-version Cargo.toml`, zero matches), `flake.nix` (37 lines), `Makefile` (10 lines), `.github/workflows/ci.yml` (23 lines, single `test` job running `nix develop -c make test`), `LICENSE` (GPL-3.0-or-later, matches `Cargo.toml`'s `license` field).
  - Confirm current gaps (verified during story creation): no badges exist anywhere in `README.md`; `Cargo.toml` has no `rust-version`; `flake.nix`'s devShell packages list has none of `cargo-llvm-cov`/`cargo-audit` — despite the architecture's own Structural Seed comment already (aspirationally) documenting them as belonging there (see Dev Notes below, this is a real gap this story closes, not a misread).
  - Confirm release/tag state: two GitHub Releases exist (`hypogaol-v0.2.0`, latest; `tomb-fido2-v0.1.0`, pre-rebrand) at `https://github.com/LeReverandNox/hypogaol/releases`. Repo is public.

- [x] **Task 1: Pin the MSRV in `Cargo.toml`** (AC: #1)
  - Add `rust-version = "1.90.0"` to `Cargo.toml`'s `[package]` table — this is the architecture's verified MSRV floor [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md, Stack table, line 203], and gives the MSRV badge (Task 5) a real source of truth to link to instead of a hardcoded, driftable number. `cargo build` will now hard-fail on a toolchain older than 1.90.0 — confirm the CI/dev Nix toolchain (nixpkgs unstable) is still >= 1.90.0 after this change (it was verified locally at story-creation time).

- [x] **Task 2: Add `cargo-llvm-cov` and `cargo-audit` to the Nix devShell** (AC: #2, #3)
  - `flake.nix`'s `devShells.default.packages` list (currently: `rustc`, `cargo`, `clippy`, `rustfmt`, `rust-analyzer`, `cryptsetup`, `systemd`, `libfido2`, `psmisc`, `lvm2`) — add `cargo-llvm-cov` and `cargo-audit` (both present in `nixpkgs`). This is required so the new CI jobs (Tasks 4-5) can invoke them the same way the existing `test` job invokes `cargo`/`make` — through `nix develop -c ...`, not a separate install step in the workflow.
  - This does **not** touch `xfsprogs`/`btrfs-progs`/`e2fsprogs` — those are a separate, pre-existing gap (hardware-test-only tooling, not CI-gated) out of this story's scope; don't fix it here.

- [x] **Task 3: Add `coverage` and `audit` Makefile targets** (AC: #2, #3)
  - `Makefile` currently has exactly three one-line targets (`build`, `test`, `test-hardware`). Add two more in the same style, e.g.:
    ```makefile
    coverage:
    	cargo llvm-cov --lib --test unit --lcov --output-path lcov.info

    audit:
    	cargo audit
    ```
  - Keep scope to the fake-backed suite (`--lib --test unit`), matching `make test`'s own scope — coverage of the manual/hardware-gated suite is out of scope (that suite isn't run in CI at all, per AD-7).
  - CI (Tasks 4-5) should call `make coverage`/`make audit`, not raw `cargo llvm-cov`/`cargo audit` inline — mirrors the existing `nix develop -c make test` convention in `ci.yml`, don't introduce a second convention.

- [x] **Task 4: Add a coverage CI job reporting to Codecov** (AC: #2)
  - Add a job to `.github/workflows/ci.yml` (or a new workflow file — either is acceptable, pick whichever keeps the file readable; the existing file has room for a second job) that runs `nix develop -c make coverage` to produce an lcov report, then uploads it via `codecov/codecov-action` (check the current major version at implementation time — this project's own convention for external CI actions is "latest stable, re-resolve, not a hard pin" [Source: ARCHITECTURE-SPINE.md, Stack table, lines 220-222]).
  - **External account dependency — cannot be completed non-interactively:** Codecov needs the `LeReverandNox/hypogaol` repo activated on codecov.io before uploads will succeed or a badge will render real data. This is an account-linking step only `LeReverandNox` can do (OAuth login to codecov.io), the same category of manual-only step this project already tracks as action items (e.g. Epic 3/4's "exercise the release pipeline against a real tag" item). **Add the CI job and badge markup regardless** (public-repo uploads via `codecov-action` v4+ support tokenless upload via GitHub OIDC, so no `CODECOV_TOKEN` secret should be required) — record in Completion Notes that repo activation on codecov.io is a follow-up the user must do, and add an epic-6 action item for it in `sprint-status.yaml` rather than blocking this story on it.

- [x] **Task 5: Add a gating security-audit CI job** (AC: #3)
  - Add a job to `.github/workflows/ci.yml` running `nix develop -c make audit`. This job must **fail the workflow** on a known RustSec advisory (`cargo audit`'s default exit code behavior already does this — do not add `continue-on-error` or `|| true`, which would silently defeat AC #3's "gating" requirement).
  - Verify locally before considering this task done: `nix develop -c make audit` currently exits 0 (no known advisories against `Cargo.lock` as of story creation) — this confirms the gate is real (would fail on a real advisory) without needing to fabricate one.

- [x] **Task 6: Add all six badges to `README.md`'s header** (AC: #1, #2, #3, #4)
  - Insert a badge row directly under the `# Hypogaol` title (above the `> Sealed until touched.` tagline) — six badges, each a real image linking to the resource it reflects, no placeholder images:
    - **Build status** → CI workflow run: image `https://github.com/LeReverandNox/hypogaol/actions/workflows/ci.yml/badge.svg`, link `https://github.com/LeReverandNox/hypogaol/actions/workflows/ci.yml`
    - **License** → `LICENSE` file: shields.io static badge reading `GPL--3.0--or--later` (double-dash-escaped per shields.io's static-badge syntax — verify exact syntax at implementation time), link to `LICENSE` in the repo
    - **Latest release** → GitHub Releases: image `https://img.shields.io/github/v/release/LeReverandNox/hypogaol`, link `https://github.com/LeReverandNox/hypogaol/releases/latest`
    - **MSRV** → `Cargo.toml`'s new `rust-version` field (Task 1): this crate has `publish = false` (never on crates.io), so the crates.io-backed MSRV badge shields.io normally offers will **not** work — use a static shields.io badge showing `1.90.0`, linking to `Cargo.toml` in the repo (not crates.io)
    - **Coverage** → Codecov project page: Codecov's own hosted badge image for this repo (`https://codecov.io/gh/LeReverandNox/hypogaol/branch/main/graph/badge.svg`), link `https://codecov.io/gh/LeReverandNox/hypogaol`
    - **Security audit** → the new audit CI job (Task 5): image following the same GitHub Actions badge-svg pattern as build status but scoped to the audit job/workflow, link to that job's Actions run
  - **Verify every link resolves** before calling this done (curl -I each URL, or open in a browser) — AC #4 explicitly requires "a live, working link — not a placeholder image." The Codecov badge/link will render "unknown"/empty data until Task 4's account-activation follow-up happens (expected, documented in Completion Notes) but must still be a real, live Codecov URL, not a static placeholder image.

- [x] **Task 7: Full regression pass**
  - `cargo build` succeeds with `rust-version = "1.90.0"` set (confirms the local/CI toolchain still satisfies it).
  - `make test` passes unchanged — this story touches no `.rs` files, so the full prior suite (**311 total: 33 lib + 278 tests/unit**, per Story 6.7's final verified count) must be unaffected. If any test fails, that is a signal something outside this story's stated scope was touched — stop and re-check.
  - `nix develop -c make coverage` and `nix develop -c make audit` both run successfully end-to-end locally (coverage producing a report file, audit exiting 0 against current `Cargo.lock`).
  - `cargo fmt --check` and `cargo clippy --all-targets` both clean, same baseline as Story 6.7 (5 pre-existing `too_many_arguments` warnings, unaffected by this story).
  - State explicitly in Completion Notes: the final verified test count (unchanged from 6.7 unless proven otherwise), confirmation that the audit job is genuinely gating (not soft-failing), and the Codecov account-activation follow-up status.

### Review Findings

- [x] [Review][Patch] Security Audit badge doesn't scope to the audit job — GitHub's `actions/workflows/ci.yml/badge.svg` endpoint ignores the `?job=` query param, so it's byte-identical to the Build Status badge above it (confirmed via live `curl`), violating Task 6's explicit "scoped to the audit job/workflow" instruction and AC #3's intent of a distinct security-audit badge [README.md:12, .github/workflows/ci.yml:42]. **Fixed:** split the `audit` job out into its own `.github/workflows/audit.yml` workflow file (permitted by Task 4/5's own "new workflow file is acceptable" language); README badge/link now point at `audit.yml`'s dedicated badge endpoint, which is genuinely scoped to that workflow instead of duplicating Build Status.
- [x] [Review][Dismiss] MSRV badge/`rust-version` asserted but not CI-verified against the 1.90.0 floor — real gap but not required by AC #1/Task 1, out of this story's scope
- [x] [Review][Dismiss] Coverage CI job is non-gating (`fail_ci_if_error: false`, no threshold) — as designed; only the audit job is required to gate per AC #2 vs #3 and the story's own Completion Notes
- [x] [Review][Dismiss] Coverage measurement excludes `tests/hardware` — explicitly in scope per Task 3 (`--lib --test unit`, matching `make test`)
- [x] [Review][Dismiss] `cargo audit` has no advisory-ignore/suppression mechanism configured — standard practice is to add ignore rules only once a real advisory needs a waiver, not preemptively
- [x] [Review][Dismiss] "Gating" audit job isn't enforced via GitHub branch protection — branch protection was already decided not applicable for this repo (personal-account plan limitation, resolved in Epic 1 retro); AC #3 only requires the job itself to fail the build, which it does
- [x] [Review][Dismiss] `CODECOV_TOKEN` repo-secret registration is self-attested in Completion Notes with no way to verify from the diff — inherent to secrets, not fixable from a code review
- [x] [Review][Dismiss] `flake.nix`'s `LLVM_COV`/`LLVM_PROFDATA` rely on a hand-verified, dated comment with no automated version-match check — real low-priority robustness gap, but no unambiguous mechanical fix and risk only materializes on a future `flake.lock` bump
- [x] [Review][Dismiss] No caching (`actions/cache`/nix store cache) in any of the three CI jobs — legitimate performance follow-up, not required by this story's scope
- [x] [Review][Dismiss] License/MSRV badges are hand-typed static text that can drift from `Cargo.toml`/`LICENSE` — inherent to the static-badge approach Task 6 explicitly mandates for this unpublished (`publish = false`) crate
- [x] [Review][Dismiss] `CODECOV_TOKEN` is unavailable to `pull_request` workflows triggered from forks, so external-contributor PRs silently skip the coverage upload — standard, accepted OSS pattern; deliberate `fail_ci_if_error: false` keeps fork PRs green, and it doesn't affect the README badge (which reads `branch/main` data only)
- [x] [Review][Dismiss] `codecov-action` wired with an explicit `CODECOV_TOKEN` rather than the tokenless OIDC upload Task 4 anticipated — reasonable, disclosed deviation (Codecov has since tightened tokenless-upload policy); already reconciled in Completion Notes

## Dev Notes

- **No new port, no new architectural layer, no `domain`/`ports`/`adapters` change at all — CI/config/docs only.** CAP-21's own row in the Capability → Architecture Map states it explicitly: *"CI workflows, `README.md` | CI tooling — not a domain AD; epics.md assigns the next `AR-Dev` number when it scopes this."* [Source: ARCHITECTURE-SPINE.md, Capability → Architecture Map, line 297] No such `AR-Dev`/AD number has in fact been assigned anywhere in the architecture doc (highest existing is AD-21, none reference CAP-21) — consistent with "not a domain AD," nothing to look up here.

- **`flake.nix` is currently missing tooling the architecture doc already describes it as having.** The Structural Seed's `flake.nix` comment reads: *"Nix devShell: Rust toolchain + cryptsetup/systemd/libfido2/e2fsprogs/psmisc/xfsprogs/btrfs-progs/**cargo-llvm-cov/cargo-audit**, no system-wide install"* [Source: ARCHITECTURE-SPINE.md, Structural Seed, line 250] — but the real `flake.nix` (verified during story creation, full file read) has none of `e2fsprogs`/`xfsprogs`/`btrfs-progs`/`cargo-llvm-cov`/`cargo-audit`. This story only needs to close the `cargo-llvm-cov`/`cargo-audit` half (Task 2) — the filesystem-tool half is an unrelated pre-existing gap (those are hardware-test-only, not CI-gated) and is explicitly out of scope here; don't "fix" it as a drive-by.

- **The Stack table already names the exact tools and hosting service this story wires up**, with an explicit "re-resolve, not a hard pin" posture matching how this project always treats external tool versions: `cargo-llvm-cov` (latest stable at CI-setup time), `cargo-audit` (latest stable at CI-setup time), Codecov (hosted, no version to pin). [Source: ARCHITECTURE-SPINE.md, Stack table, lines 220-222]

- **Codecov repo activation is an external, interactive, account-level step LeReverandNox must do — no dev agent can complete it non-interactively.** Ship the CI job and badge regardless (Task 4); don't block the story on it. This matches the project's existing pattern of landing infrastructure that needs a one-time manual activation and tracking the activation itself as a follow-up action item (e.g. Epic 3/4's "exercise the release pipeline against a real tag" item in `sprint-status.yaml`'s `action_items`) — add a similar epic-6 action item for "activate LeReverandNox/hypogaol on codecov.io" when this story completes.

- **`cargo-audit` needs `Cargo.lock` present to scan** — already committed and up to date (verified during story creation). No new setup needed there beyond adding the CI job itself.

- **Recurring review-pattern watchlist from Epic 4/5/6 retros — apply proactively:**
  - Self-reported completion-note claims (test counts, "it works" claims) not matching actual output — hit in Stories 5.1/5.2/5.3/6.4/6.5/6.6/6.7, each caught only in review. For this story specifically: don't claim a badge "works" without actually resolving its URL; don't claim the audit job "gates" without confirming it lacks any silent-failure flag (`continue-on-error`, `|| true`, `-w` suppressions).
  - New pure parsing/logic functions shipping without a unit test — not applicable here (no Rust logic added, CI config/README/`Cargo.toml` only).

### Project Structure Notes

- Files touched: `Cargo.toml` (add `rust-version`), `flake.nix` (add 2 devShell packages), `Makefile` (add `coverage`/`audit` targets), `.github/workflows/ci.yml` (add 2 jobs, or a new workflow file if that reads cleaner), `README.md` (badge row under the title).
- No changes to `src/`, `tests/unit/`, `tests/hardware/`, `_bmad/bmm/config.yaml`, or any `domain`/`ports`/`adapters` file.
- `sprint-status.yaml`'s `action_items` gains one new epic-6 entry for Codecov account activation (see Dev Notes) alongside the story's own status update.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 6.8: Repository Hygiene Badges, lines 946-968] — acceptance criteria origin, verbatim.
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 6: Volume Resilience, Filesystem Choice & Everyday Polish, lines 178-180, 766-768] — epic-level framing; confirms no new port/architectural layer for any Epic 6 story.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md, Capability → Architecture Map, line 297] — CAP-21 lives in CI workflows/README, explicitly "not a domain AD."
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md, Stack table, lines 220-222, 203] — `cargo-llvm-cov`/`cargo-audit`/Codecov tooling entries; Rust 1.90.0 MSRV floor.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md, Structural Seed, line 250] — `flake.nix` comment already (aspirationally) listing `cargo-llvm-cov`/`cargo-audit`, confirming this story is what actually adds them.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md, FR Coverage Map, line 150] — FR21: Epic 6 - README repository-hygiene badges.
- [Source: Cargo.toml, README.md, flake.nix, Makefile, .github/workflows/ci.yml, LICENSE] — full current state read in full during story creation; the gaps listed in Task 0/Dev Notes are derived directly from these files, not inferred.
- [Source: _bmad-output/implementation-artifacts/6-7-one-letter-cli-flag-shorthand.md] — previous story in this epic; confirms the verified regression baseline this story starts from (311 total: 33 lib + 278 tests/unit) and the project's recurring self-reported-count-drift watchlist.
- [Source: _bmad-output/implementation-artifacts/sprint-status.yaml] — confirms this is the eighth and final story of Epic 6 (epic already `in-progress` since Story 6.1).
- Verified locally (2026-08-11, story creation): `grep -n rust-version Cargo.toml` → zero matches; `grep -n "cargo-llvm-cov\|cargo-audit" flake.nix` → zero matches; two GitHub Releases exist (`hypogaol-v0.2.0` latest, `tomb-fido2-v0.1.0`); `Cargo.lock` present and current; repo is public.

## Dev Agent Record

### Agent Model Used

claude-sonnet-5

### Debug Log References

### Completion Notes List

- Task 1: Added `rust-version = "1.90.0"` to `Cargo.toml`. Local toolchain is `rustc 1.96.1`, well above the floor. `cargo build` confirmed clean.
- Task 2: Added `cargo-llvm-cov` and `cargo-audit` to `flake.nix`'s devShell packages. Verified via `nix develop -c bash -c 'cargo llvm-cov --version && cargo audit --version'` — both resolve from nixpkgs and run (`cargo-llvm-cov 0.8.7`, `cargo-audit-audit 0.22.2`).
- Task 3: Added `coverage`/`audit` Makefile targets, matching the existing `--lib --test unit` scope of `make test`. `cargo-llvm-cov` needed one extra wiring step beyond what the story anticipated: it looks for the `llvm-tools-preview` rustup component under rustc's sysroot, which nixpkgs's plain `rustc` doesn't provide. Fixed by adding `LLVM_COV`/`LLVM_PROFDATA` env vars to `flake.nix`'s devShell, pointing at `rustc.llvmPackages.llvm` (its version, 21.1.8, is confirmed to match `rustc --version --verbose`'s reported LLVM version exactly, so no ABI mismatch risk). Verified end-to-end: `nix develop -c make coverage` produces `lcov.info` (278 tests run, all passing); `nix develop -c make audit` exits 0 against current `Cargo.lock` (0 known advisories, 37 crates scanned). Added `lcov.info` to `.gitignore`.

- Task 4: Added a `coverage` job to `.github/workflows/ci.yml` running `nix develop -c make coverage`, uploading `lcov.info` via `codecov/codecov-action@v7` (latest stable major, `v7.0.0`). Wired `secrets.CODECOV_TOKEN` through explicitly — LeReverandNox already registered and activated the repo on codecov.io and stored the token as a repository secret before this story started, so the epic-6 action item for that manual step is resolved (see `sprint-status.yaml`). `fail_ci_if_error: false` so a Codecov-side outage doesn't block the build (coverage isn't a gating requirement per AC #3, only the audit job is).
- Task 5: Added an `audit` job to `.github/workflows/ci.yml` running `nix develop -c make audit`, no `continue-on-error`/`|| true` — confirmed genuinely gating: `cargo audit` exits non-zero on a known advisory by default and nothing here suppresses that.

- Task 6: Added a 6-badge row under the `# Hypogaol` title (build status, license, latest release, MSRV, coverage, security audit — audit badge scoped to the `audit` job via GitHub's `?job=` badge param). **Blocking discovery during verification: the repo was actually private** (`gh api repos/LeReverandNox/hypogaol` → `"private": true`), contradicting Task 0's story-creation-time note that it was public — every `github.com` badge/link 404'd anonymously as a result. Flagged to LeReverandNox rather than guessing; he confirmed and made the repo public. Re-verified all 11 badge-image + link-target URLs (`curl -s -o /dev/null -w "%{http_code}"` on each) — all return `200` post-fix. Codecov badge/link resolve (200) but will show "unknown" coverage data until the first CI run lands on `main` post-merge (expected, per Dev Notes).

- Task 7: Full regression pass, all green. `cargo build` succeeds with `rust-version = "1.90.0"` set. `nix develop -c make test`: **311 total (33 lib + 278 tests/unit)** — unchanged from Story 6.7's baseline, confirming this CI/config/docs-only story touched no `.rs` files. `nix develop -c make coverage` produces `lcov.info` end-to-end; `nix develop -c make audit` exits 0 (0 known advisories, 37 crates). `cargo fmt --check` clean. `cargo clippy --all-targets`: same 5 pre-existing `too_many_arguments` warnings as the 6.7 baseline, no new warnings. Confirmed the audit CI job is genuinely gating (no `continue-on-error`/`|| true`/suppression flags anywhere in its definition). Codecov account-activation follow-up: **done** — LeReverandNox registered, activated the repo, and stored `CODECOV_TOKEN` before this story began; the corresponding epic-6 action item in `sprint-status.yaml` is marked resolved.

### File List

- `Cargo.toml`
- `flake.nix`
- `Makefile`
- `.gitignore`
- `.github/workflows/ci.yml`
- `README.md`

## Change Log

- 2026-08-11: All 7 tasks implemented and verified. `Cargo.toml` pinned to MSRV 1.90.0; `flake.nix`/`Makefile` gained `cargo-llvm-cov`/`cargo-audit` tooling; `ci.yml` gained `coverage` (Codecov) and `audit` (gating) jobs; `README.md` gained 6 live badges. Mid-implementation discovery: the repo was private, breaking every `github.com`-hosted badge/link for anonymous visitors — LeReverandNox made it public to unblock AC #4. Codecov account activation (previously an open epic-6 action item) was already done by LeReverandNox before this story started. Status: review.
- 2026-08-11: Code review complete (0 decision-needed, 1 patch, 0 deferred, 11 dismissed as noise). Patch applied: split the `audit` CI job into its own `audit.yml` workflow file so the Security Audit badge is genuinely scoped to it instead of duplicating Build Status (commit `d389788`). Status: done.
