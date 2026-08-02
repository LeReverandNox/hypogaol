---
baseline_commit: 0ce7593d9c675fae7350c3035a438f132b23add2
---

# Story 5.1: Product Identity Rename

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a maintainer,
I want the product's name updated to Hypogaol everywhere it's read from a single source,
so that Cargo, the CLI, and the README present one consistent, permanent product identity instead of the `tomb-fido2` placeholder.

## Acceptance Criteria

1. **Given** the GitHub repository has already been renamed to `hypogaol`, **when** Story 5.1 lands, **then** `Cargo.toml`'s `name` and `repository` fields, `_bmad/bmm/config.yaml`'s `project_name`, `README.md`'s title/badges, and `CHANGELOG.md`'s header all read `Hypogaol`/`hypogaol`, and `flake.nix` references are updated to match.
2. **Given** AD-13's placeholder-name isolation (CLI/binary name sourced from exactly one place — the Cargo package name), **when** the package name changes, **then** the compiled binary and `--help` banner automatically reflect the new name with no additional literal to update, confirming AD-13's guarantee held.
3. **Given** the renamed repository, **when** any CI/release config (GitHub Actions workflows, cargo-dist config) references the old repo path or name, **then** those references are updated to the new path.
4. **Given** `README.md`'s body prose refers to the product by name (e.g. "tomb-fido2 exists to do one job well...", the "Break-glass recovery (no tomb-fido2 required)" heading), **when** Story 5.1 lands, **then** every such self-reference reads `Hypogaol` instead — but references to the *domain concept* (the encrypted container, e.g. "close every tomb-fido2-managed tomb") keep the word "tomb" untouched (Story 5.2's job), and references to the unrelated `dyne/tomb` project (e.g. "a capability the original Tomb never had") are left alone.

## Tasks / Subtasks

- [ ] **Task 0: Read every file this story touches before changing anything** (AC: #1, #2, #3)
  - Read in full: `Cargo.toml`, `Cargo.lock` (just the `tomb-fido2` package block, line ~265), `_bmad/bmm/config.yaml`, `README.md`, `CHANGELOG.md`, `flake.nix`, `src/cli/main.rs` (the `Cli` struct's `#[command(...)]` attribute), `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `.github/workflows/release-please.yml`, `release-please-config.json`, `.release-please-manifest.json`.

- [ ] **Task 1: Verify the pre-check for this story — GitHub repo rename** (AC: #1)
  - This story's AC1 is explicitly conditioned on "the GitHub repository has already been renamed to `hypogaol`" — that rename is a manual, user-owned action (`LeReverandNox/tomb-fido2` → `LeReverandNox/hypogaol`) outside any agent's scope, per the sprint change proposal §5.
  - Check whether it's done: `git remote get-url origin` and/or `gh repo view --json nameWithOwner`. If the remote still resolves to `tomb-fido2`, **stop and ask the user to confirm/perform the rename before proceeding** — don't guess or skip this gate, since `Cargo.toml`'s `repository` URL and any README links depend on it being correct in one pass (rather than needing a second pass later).

- [x] **Task 2: Rename the Cargo package identity** (AC: #1)
  - In `Cargo.toml`: `name = "tomb-fido2"` → `name = "hypogaol"`; `repository = "https://github.com/LeReverandNox/tomb-fido2"` → `"https://github.com/LeReverandNox/hypogaol"`.
  - Run `cargo build` (or `cargo check`) afterward so `Cargo.lock`'s `tomb-fido2` package entry (~line 265) regenerates to `hypogaol` automatically — do not hand-edit `Cargo.lock`.
  - Do **not** touch `[package.metadata.dist]` (`dist = true`), `[workspace.metadata.dist]`, or `[profile.dist]` — none of them contain the product name; they key off the package name that just changed above, so they need no edits of their own.

- [x] **Task 3: Update `_bmad/bmm/config.yaml`** (AC: #1)
  - `project_name: tomb-fido2` → `project_name: hypogaol`. Nothing else in this file references the product name.

- [ ] **Task 4: Update `flake.nix`** (AC: #1)
  - Line 2: `description = "tomb-fido2 development environment";` → `"hypogaol development environment"`.
  - Line 23's comment (`# Runtime tools tomb-fido2 orchestrates (AD-1) — ...`) also names the product — update to `hypogaol` for consistency, since it's read as prose describing the live tool, not a frozen historical reference.
  - No other lines in this file mention the product name.

- [ ] **Task 5: Update `README.md`'s title, working-title disclaimer, and every body-prose self-reference to the product name** (AC: #1, #4)
  - Line 1: `# tomb-fido2` → `# Hypogaol`.
  - Line 3 currently reads: `> Working title. \`tomb-fido2\` is a placeholder pending a permanent name — don't read anything into it.` The name is now permanent, so this line is now false if left as-is — replace or remove it rather than leaving a contradictory disclaimer next to the new title. Keep this edit to the disclaimer sentence itself; do **not** add tagline/pronunciation/mascot copy here — that's Story 5.3's scope, not this story's.
  - There are no badges or CI-status images currently in `README.md` (confirmed: no `![...]` badge markup) — AC1's "badges" clause is a no-op for now, not a missing requirement.
  - **Product-name self-references in body prose (AC #4) — rename these `tomb-fido2` → `Hypogaol`** (verified full list via `grep -n "tomb-fido2\|Tomb\b" README.md` during story creation; re-grep before editing in case the file has drifted): lines 7, 9 (two occurrences, plus the anchor-link text `Break-glass recovery` — see next bullet for the anchor itself), 16, 20 (two occurrences), 23 ("Close every **tomb-fido2**-managed tomb currently open" — rename only the `tomb-fido2` half of this one; the trailing `tomb` is the domain noun, Story 5.2's job), 30, 36, 38, 42, 43, 63 (two occurrences), 65, 67, 69 (two occurrences), 73, 77 (section heading "Break-glass recovery (no tomb-fido2 required)"), 79, 102, 106 (three occurrences), 110 (two occurrences).
  - **Markdown anchor link on line 9**: `[Break-glass recovery](#break-glass-recovery-no-tomb-fido2-required)` — once line 77's heading text changes, its auto-generated GitHub anchor slug changes too; update this link's anchor fragment to match (e.g. `#break-glass-recovery-no-hypogaol-required`) so the in-page link doesn't break. Verify the exact slug GitHub generates from the new heading text rather than guessing.
  - **Leave untouched — do not rename:**
    - Line 30: "a capability the original Tomb never had" — this is a proper-noun reference to the unrelated `dyne/tomb` project, not our product or the domain noun.
    - Line 38: the `[dyne/tomb](https://dyne.org/docs/tomb/manpage/#hooks)` link — external project name and URL, not ours.
    - Every plain domain-noun "tomb" (the container concept — e.g. "create a brand-new tomb", "before unmounting the tomb itself", "your tomb was created with") — those are Story 5.2's job, not this story's. When a single sentence contains both a product-name self-reference and a domain-noun "tomb" (line 23's "Close every tomb-fido2-managed tomb currently open" is the clearest example; line 36's bind-hooks paragraph and line 63's/line 67's paragraphs also mix both), rename only the product-name half here and leave the domain noun for 5.2.
  - Renaming the product-name self-references here (rather than deferring them) removes the ambiguous `tomb-fido2` token from the file before Story 5.2 does its domain-noun sweep — 5.2 will then only have one meaning of "tomb" left to consider in this file, not two tangled together.

- [ ] **Task 6: Update `CHANGELOG.md`'s header** (AC: #1)
  - The literal header is `# Changelog` (line 1) — it does not contain the product name, so there is nothing to rename there. Do not touch the historical entries below it (lines like "**1.5:** create a file-backed tomb ([#11](https://github.com/LeReverandNox/tomb-fido2/issues/11))...") — those are release-please-generated historical records tied to real, already-merged PR/commit URLs on the old repo path; they remain valid (GitHub redirects renamed-repo URLs) and rewriting them would falsify history for no functional benefit. Confirm this understanding rather than bulk-replacing `LeReverandNox/tomb-fido2` across the whole file.

- [ ] **Task 7: Verify AD-13's guarantee holds for the CLI banner — no code change expected** (AC: #2)
  - `src/cli/main.rs`'s `Cli` struct uses `#[derive(Parser)] #[command(version, about)]` with no explicit `name(...)` — clap's default behavior sources the binary/CLI name from `CARGO_PKG_NAME` (i.e., `env!("CARGO_PKG_NAME")` under the hood) automatically. After Task 2 renames the package, rebuild and run `cargo run -- --help` to confirm the banner now reads `hypogaol` with zero source changes in `src/cli/`.
  - If you find any hardcoded `"tomb-fido2"` (or `"tomb_fido2"`) string literal anywhere in `src/` used for the CLI name/banner/version output, that is an AD-13 violation that must be fixed as part of this AC — but a repo-wide check during story creation found none, so expect this task to be verify-only.

- [ ] **Task 8: Verify CI/release config for hardcoded repo/name references** (AC: #3)
  - Checked during story creation: `.github/workflows/ci.yml`, `.github/workflows/release.yml` (cargo-dist generated), `.github/workflows/release-please.yml`, `release-please-config.json`, and `.release-please-manifest.json` contain **no** hardcoded `tomb-fido2` or `LeReverandNox` literals — they all derive repo/package identity from `Cargo.toml` or GitHub Actions' own context (`${{ github.repository }}`, etc.) at run time. Re-verify this after Task 2's `Cargo.toml` rename (grep the same file set for `tomb-fido2`/`tomb_fido2`); if still clean, this AC requires no edits — do not invent changes to satisfy it.

- [ ] **Task 9: Full regression pass** (AC: #1, #2, #3, #4)
  - `cargo build` succeeds with the new package name.
  - `make test` (mocked unit suite, AD-7) passes unchanged — this story is a pure naming/config change with zero behavioral diff, so a clean `make test` run is the acceptance bar per the sprint change proposal, not a manual audit.
  - `cargo run -- --help` shows `hypogaol` in the banner (Task 7).
  - Re-run `grep -n "tomb-fido2" README.md` — it should return zero matches (every self-reference renamed in Task 5). A separate `grep -n "tomb" README.md` will still return plenty of matches (domain-noun/hooks/dyne-tomb references, all correctly left for Story 5.2) — that's expected, not a regression.

## Dev Notes

- **Scope boundary is the whole point of this story.** Epic 5 splits the rebrand into four stories deliberately (per `_bmad-output/planning-artifacts/sprint-change-proposal-2026-08-02.md` §4): 5.1 = product identity (this story — Cargo/config/flake/README-title/README-body-self-references/CHANGELOG-header/CI-verify), 5.2 = domain-noun `tomb`→`volume` rename across `src/`/`tests/`/README-body/`hooks.md`, 5.3 = tagline/pronunciation/mascot top-level branding copy, 5.4 = `github-automation-reference.md` + SPEC.md/AD-13 addendum. **Do not pull work from 5.2/5.3/5.4 into this story** — leave `hooks.md`, every domain-noun "tomb" occurrence (README or elsewhere), tagline/pronunciation copy, and `_bmad/custom/github-automation-reference.md`'s repo path untouched here. README body prose *is* in scope for this story, but narrowly: only the product-name self-references (AC #4), not the domain noun living in the same paragraphs — see Task 5's line-by-line breakdown.
- **AD-13 (Placeholder-name isolation)** — see `_bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md` around line 116 — is why this story is cheap: the CLI/binary name was deliberately never hardcoded as a string literal anywhere, only ever read from the Cargo package name. This story's Task 7 exists to *confirm* that invariant held across 4 epics of implementation, not to newly implement it.
- **No functional/behavioral change is in scope.** The acceptance bar is a clean `cargo build` + `make test`, not a manual feature audit — per the sprint change proposal's Technical Impact section.
- **Historical docs stay frozen.** `_bmad-output/planning-artifacts/{SPEC.md,epics.md,ARCHITECTURE-SPINE.md}` and all Epic 1-4 retros/story files keep referring to `tomb-fido2` for continuity (AD-13's own doc-continuity carve-out) — this story must not touch any of them. That includes not touching this same story's neighboring files under `_bmad-output/implementation-artifacts/` from Epics 1-4.
- **This story has no predecessor within Epic 5** (it's Story 5.1, the epic's first) — there is no prior-story Dev Notes/learnings to inherit here.

### Project Structure Notes

- Files touched by this story: `Cargo.toml`, `Cargo.lock` (regenerated, not hand-edited), `_bmad/bmm/config.yaml`, `flake.nix`, `README.md` (title, disclaimer line, and ~25 body-prose product-name self-references — see Task 5), `CHANGELOG.md` (verify-only, no edit expected). No `src/` or `tests/` files are expected to change (Task 7 is verification-only, per AD-13).
- No new files, modules, or structural changes — this is a pure identifier/string rename within existing files.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 5.1: Product Identity Rename] — acceptance criteria origin.
- [Source: _bmad-output/planning-artifacts/sprint-change-proposal-2026-08-02.md#4. Detailed Change Proposals] — per-story file-level scope breakdown for all of Epic 5.
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-13 — Placeholder-name isolation] — why the CLI banner needs no code change.
- [Source: Cargo.toml], [Source: flake.nix], [Source: README.md], [Source: CHANGELOG.md], [Source: _bmad/bmm/config.yaml] — files this story edits directly.

## Dev Agent Record

### Agent Model Used

Claude Sonnet 5 (claude-sonnet-5)

### Debug Log References

### Completion Notes List

- Task 2: Renamed Cargo package (`name`, `repository`) to `hypogaol`; `cargo build` regenerated `Cargo.lock`'s package entry automatically. Discovered a consequence the story's Dev Notes didn't anticipate: since no `[lib]` section overrides it, the library crate identifier also derives from the package name, so every `use tomb_fido2::...` in `src/main.rs` and `tests/**/*.rs` broke at compile time. Fixed by mechanically renaming `tomb_fido2::` → `hypogaol::` at each import site (pure identifier rename, zero behavioral change) — required for Task 2's own `cargo build` step and Task 9's regression gate to pass. Left the one prose doc-comment mention of `` `tomb_fido2` `` in `src/adapters/exec/mod.rs:122` untouched (not a CLI-name/banner literal, so not an AD-13 violation per Task 7's carve-out). `make test` (174 tests) passes unchanged.
- Task 3: `_bmad/bmm/config.yaml`'s `project_name` updated to `hypogaol`.

### File List

- `Cargo.toml`
- `Cargo.lock`
- `_bmad/bmm/config.yaml`
- `src/main.rs`
- `tests/hardware/main.rs`
- `tests/unit/cli.rs`
- `tests/unit/close.rs`
- `tests/unit/close_all.rs`
- `tests/unit/create.rs`
- `tests/unit/enroll.rs`
- `tests/unit/fakes.rs`
- `tests/unit/hooks.rs`
- `tests/unit/info.rs`
- `tests/unit/keyslot_guard.rs`
- `tests/unit/mapping_name.rs`
- `tests/unit/preflight.rs`
- `tests/unit/progress.rs`
- `tests/unit/resize.rs`
- `tests/unit/revoke.rs`
- `tests/unit/slam.rs`
- `tests/unit/unlock.rs`
- `tests/unit/ux.rs`
- `tests/unit/workflows.rs`
