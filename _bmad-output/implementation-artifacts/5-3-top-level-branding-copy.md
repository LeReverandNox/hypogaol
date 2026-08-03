---
baseline_commit: 2c5ab66f812e5a38777599239168bf6f8446f02b
---

# Story 5.3: Top-Level Branding Copy

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a first-time reader of the README or `--help` output,
I want the Hypogaol name, tagline, and pronunciation note presented clearly at the top level,
so that the brand identity lands without any flavor leaking into functional output.

## Acceptance Criteria

1. **Given** the README header, **when** Story 5.3 lands, **then** it carries the tagline "Sealed until touched." (verbatim — [Source: brand-identity.md#6. Tagline]) and a short pronunciation note for "gaol" (reads like "jail"; archaic spelling, not a typo — [Source: brand-identity.md#1. Name]).
2. **Given** the CLI `--help` banner, **when** it's shown, **then** it may carry the tagline/name treatment, but no subcommand name, flag, or error message anywhere changes to a themed/flavored word. (This clause is permissive, not mandatory — see Task 2.)
3. **Given** brand-identity.md's tone boundary (§7), **when** any copy is added under this story, **then** error messages and README body text remain plain, literal, and human-friendly — verified by an explicit grep-through check, not just a stated intention.

## Tasks / Subtasks

- [x] **Task 0: Read every file this story touches before changing anything** (AC: #1, #2, #3)
  - Read in full: `README.md` (lines 1-10 at minimum — the header region), `src/cli/main.rs` (the `Cli` struct and its `#[command(...)]` attribute, lines ~23-29), `Cargo.toml`'s `[package]` table, and `_bmad-output/brainstorming/brainstorm-project-naming-2026-08-02/brand-identity.md` §1/§6/§7 for the exact required tagline/pronunciation wording and tone rule.
  - Re-grep `README.md`'s first 10 lines before editing — Stories 5.1 and 5.2 both touched this file's header/body already (5.1: title + product-name self-references; 5.2: domain-noun rename), so confirm the exact current line numbers below haven't drifted.

- [x] **Task 1: Add tagline + pronunciation note to `README.md`'s header** (AC: #1)
  - Confirmed current state at story creation: line 1 is `# Hypogaol`, line 2 is blank, line 3 begins the descriptive paragraph ("A FIDO2-native reimagining of [dyne/tomb]..."). Insert the tagline and pronunciation note **between** line 1 and the existing line 3 paragraph — do not touch line 3 onward.
  - Tagline text is fixed and must appear verbatim: **"Sealed until touched."** ([Source: brand-identity.md#6. Tagline]) — do not paraphrase or rephrase it.
  - Pronunciation note must convey, in your own concise phrasing: "gaol" is pronounced like "jail" — archaic spelling, not a typo ([Source: brand-identity.md#1. Name]). Keep it to one short line; this is the one place top-level flavor/name-explanation is allowed, but keep the note itself plain and informative, not cute.
  - Do not add mascot ("Hypo"), mark, or palette description here — brand-identity.md §7's own scope note says actual artwork/visual production is out of scope for this story (and for this epic entirely); this story is copy only, and only the name/tagline/pronunciation triad, not the full visual identity.
  - Do not touch anything below the header — README body prose already reads "Hypogaol"/"volume" throughout from Stories 5.1/5.2; that is out of this story's scope.

- [x] **Task 2 (dev discretion — AC #2 is permissive, not mandatory): Tagline treatment on the CLI `--help` banner**
  - Confirmed at story creation: `Cargo.toml`'s `[package]` table has no `description` field, and `src/cli/main.rs`'s `Cli` struct uses a bare `#[command(version, about)]` with no `about = "..."` literal and no doc comment above the struct (see the existing comment at main.rs lines ~23-24: "`name`/`version`/`about` are populated by clap from this crate's own `CARGO_PKG_*` metadata (AD-13) — never a hardcoded product-name literal."). Net effect: today's `--help` banner shows no top-level description line at all — verify this with `cargo run -- --help` before deciding whether to act on this task.
  - **If you implement this:** add a `description = "..."` field to `Cargo.toml`'s `[package]` table containing the tagline treatment. Clap's bare `about` attribute auto-populates from `CARGO_PKG_DESCRIPTION` at compile time — no code change to `src/cli/main.rs` is needed, and this keeps the existing AD-13 single-source pattern intact (the same reasoning that already governs `name`/`version` there). Do **not** add an `about = "<text>"` literal directly to the `#[command(...)]` attribute in `main.rs` instead — that would introduce exactly the kind of hardcoded literal AD-13's comment at that call site explicitly says to avoid.
  - **If you skip this:** that fully satisfies AC #2 as written ("it may carry... but no..." is a negative constraint, not a positive requirement) — no further action needed, just note the decision in Completion Notes.
  - Either way, whatever text ends up in the banner must be name/tagline-level only — never a subcommand name, flag, or error string change (Task 3 verifies this holds).

- [x] **Task 3: Tone-boundary grep verification** (AC: #3)
  - After Task 1 (and Task 2 if implemented), run and record the output of:
    - `grep -rniE "sealed until touched|gargoyle|lantern|rose.window|hypo\b" src/cli/ux.rs src/domain/` — expect **zero** matches (error/status messages and domain logic must never carry the tagline or mascot/mark vocabulary).
    - `grep -n "///" src/cli/main.rs` — manually review every subcommand/arg doc comment clap surfaces as that subcommand's own `--help` text (e.g. `Create`, `Unlock`, `Enroll`, `Revoke`, ... and their fields) and confirm none picked up themed language.
    - `grep -n "gaol\|Sealed until touched" README.md` — confirm the tagline/pronunciation note appear exactly once each, only in the header block from Task 1, not duplicated elsewhere.
  - Record the exact commands run and their output in this story's Completion Notes List — AC #3 explicitly requires this as evidence, not just a stated intention that the tone boundary was respected.

- [x] **Task 4: Full regression pass**
  - `cargo build` succeeds (this story's edits are copy/config only — README.md, optionally `Cargo.toml`'s `description` field — zero behavioral surface).
  - `make test` passes unchanged (same test count as Story 5.2's final run — 174 tests) — this is a pure copy story, a clean `make test` is the acceptance bar, not a manual feature audit.
  - `cargo run -- --help` reviewed manually: confirm the top-level banner matches whatever Task 2 decided, and every subcommand's own `--help` text (`cargo run -- <subcommand> --help` for at least `create`, `unlock`, `revoke`) still reads plainly, with no flavor.
  - Manual read-through of `README.md`'s new header block (rendered, e.g. via a Markdown preview or just reading the raw lines) to confirm the tagline and pronunciation note read cleanly against the existing description paragraph immediately below them.

## Dev Notes

- **Scope boundary, same pattern as 5.1/5.2.** Epic 5 splits the rebrand into four stories (per `_bmad-output/planning-artifacts/sprint-change-proposal-2026-08-02.md` §4): 5.1 = product identity (done), 5.2 = domain-noun `tomb`→`volume` rename (done), **5.3 = this story**, 5.4 = `github-automation-reference.md` + SPEC.md/AD-13 addendum. Do not pull 5.4's automation-reference or SPEC.md/AD-13 edits in here.
- **This is the third story in a row to touch `README.md`.** 5.1 changed the title and product-name self-references in body prose; 5.2 changed the domain-noun "tomb"→"volume" throughout the body. Both are done — this story only adds a small header block between the title and the existing first paragraph. Re-grep the header's exact line numbers before editing (Task 0) rather than trusting the line numbers recorded above, since they're a snapshot from story creation.
- **Mascot/mark artwork is explicitly out of scope for this story and this entire epic.** `brand-identity.md` is the reference spec for whoever eventually draws the logo/mascot — no story in Epic 5 produces image assets (confirmed in the epic's own "Non-goals" note and in the sprint change proposal §4's Story 5.3 description: "actual mascot/mark artwork production is out of scope here per `brand-identity.md` §7 — this story is copy only"). Do not add mascot-name ("Hypo") or mark/palette descriptions to the README — only the name/tagline/pronunciation triad from AC #1.
- **The tone boundary (AC #3) is the part most likely to get an adversarial code-review finding.** `brand-identity.md` §7 is explicit that CLI subcommand names, error messages, and README *body* text must stay plain — the tagline/pronunciation note is the one narrow exception, confined to the README *header* (and optionally the CLI top-level banner, never per-subcommand help). Don't let the tagline's gothic register bleed into any other string while making this change; Task 3's grep check exists specifically to catch that.
- **AC #2 is a negative constraint dressed as a "may."** Re-read it carefully — it doesn't require adding anything to the CLI banner, it only requires that *if* something is added, it stays name/tagline-level. Task 2 is framed as dev-discretion for exactly this reason; don't feel obligated to touch `src/cli/main.rs` or `Cargo.toml` at all if you judge the README-only change sufficient.
- **Zero behavioral change, same acceptance bar as 5.1/5.2.** A clean `cargo build` + `make test` is the bar — this is copy/config, not a feature.

### Project Structure Notes

- Files touched: `README.md` (header block only, between the existing line 1 title and line 3 paragraph). Optionally `Cargo.toml` (`[package]` table's `description` field) if Task 2 is implemented — no `src/` code changes in that case, per AD-13's single-source pattern.
- No new files, modules, or structural changes.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 5.3: Top-Level Branding Copy] — acceptance criteria origin.
- [Source: _bmad-output/planning-artifacts/sprint-change-proposal-2026-08-02.md#4. Detailed Change Proposals] — per-story file-level scope breakdown for all of Epic 5.
- [Source: _bmad-output/brainstorming/brainstorm-project-naming-2026-08-02/brand-identity.md#1. Name, #6. Tagline, #7. Tone Boundary (Critical)] — exact tagline text, pronunciation-note requirement, and the tone boundary this story must respect.
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-13 — Placeholder-name isolation] — why the CLI banner's optional tagline treatment should route through `Cargo.toml`, not a new `main.rs` literal.
- [Source: _bmad-output/implementation-artifacts/5-1-product-identity-rename.md#Task 7] — confirms `src/cli/main.rs`'s bare `#[command(version, about)]` and the `CARGO_PKG_*`-sourcing comment already in place.
- [Source: README.md] — file this story edits directly.

## Dev Agent Record

### Agent Model Used

claude-sonnet-5

### Debug Log References

### Completion Notes List

- Task 0: Confirmed line numbers unchanged since story creation — `README.md` line 1 `# Hypogaol`, line 2 blank, line 3 begins the descriptive paragraph. `Cargo.toml`'s `[package]` table had no `description` field. `src/cli/main.rs`'s `Cli` struct used a bare `#[command(version, about)]` (lines 25-26), so the top-level `--help` banner showed no description line at all before this story.
- Task 1: Inserted a tagline blockquote (`> Sealed until touched.`) and an italicized pronunciation note between `README.md`'s line 1 title and the existing line 3 paragraph, per AC #1. Wrote the note as "The 'gaol' in Hypogaol is pronounced like 'jail' — archaic spelling, not a typo." — kept lowercase `gaol` deliberately (not sentence-initial capitalized) so it stays unambiguous under Task 3's literal grep check. No text below the header was touched.
- Task 2 (dev discretion): Verified with `cargo run -- --help` that the top-level banner showed no description line pre-change, as the story's Dev Notes predicted. Implemented the tagline treatment: added `description = "Hypogaol — sealed until touched."` to `Cargo.toml`'s `[package]` table. Clap's bare `about` in `src/cli/main.rs` auto-populates from `CARGO_PKG_DESCRIPTION` at compile time — no `main.rs` change needed, keeping AD-13's single-source pattern intact. Verified post-change: `cargo run -- --help` now prints "Hypogaol — sealed until touched." as the first line, followed by the unchanged `Usage:`/`Commands:`/`Options:` block.
- Task 3: Tone-boundary grep verification, commands and output recorded verbatim:
  - `grep -rniE "sealed until touched|gargoyle|lantern|rose.window|hypo\b" src/cli/ux.rs src/domain/` → **zero matches** (exit code 1). No flavor/mascot vocabulary in error/status messages or domain logic.
  - `grep -n "///" src/cli/main.rs` → manually reviewed every one of the 160 matched doc-comment lines (every subcommand and flag's clap-surfaced help text, e.g. `Create`, `Unlock`, `Enroll`, `Revoke`, plus internal fn-doc comments). All plain, technical, literal language — zero themed/flavored wording found. (Corrected during code review: original notes misstated this count as "~90"; re-run confirmed 160, and the re-review of the full set still found no themed language.)
  - `grep -n "gaol\|Sealed until touched" README.md` → matches on lines 1, 3, 5, and every body line that already contains the pre-existing product name "Hypogaol" (which itself contains the substring "gaol"), since the pattern is a case-sensitive substring match, not a word-boundary match. Disambiguated with `grep -c "Sealed until touched" README.md` → **1** (tagline appears exactly once, line 3), and `grep -noP '(?<!Hypo)gaol' README.md` → line 5 (the pronunciation note) plus one incidental false-positive on line 11's auto-generated lowercase GitHub anchor fragment (`#break-glass-recovery-no-hypogaol-required`, not new prose). Confirms the tagline and the pronunciation note each appear exactly once, both confined to the Task 1 header block — no duplication elsewhere.
- Task 4: Full regression pass. `cargo build`: clean. `make test`: 174/174 passed (identical count to Story 5.2's final run — zero regressions, as expected for a copy/config-only story). `cargo run -- --help` shows the new tagline banner line; `cargo run -- create --help`, `unlock --help`, and `revoke --help` manually reviewed — all read plainly, no flavor, unchanged from pre-story. Manual read-through of `README.md`'s new header block confirms the tagline and pronunciation note read cleanly against the existing description paragraph immediately below.

### File List

- README.md
- Cargo.toml

## Change Log

- 2026-08-03: Implemented Story 5.3 — added the "Sealed until touched." tagline and a "gaol"/"jail" pronunciation note to `README.md`'s header (AC #1), added the same tagline treatment to the CLI's top-level `--help` banner via `Cargo.toml`'s `description` field (AC #2, dev discretion exercised: implemented), and ran the tone-boundary grep verification confirming no flavor leaked into error/status messages, domain logic, or per-subcommand help text (AC #3). All 4 tasks complete, all 3 ACs satisfied. `cargo build` clean, `make test` 174/174 passed, zero regressions. Status moved to `review`.

### Review Findings

- [x] [Review][Patch] Task 3 Completion Notes misstate the `grep -n "///" src/cli/main.rs` match count (~90 claimed vs. 160 actual) [_bmad-output/implementation-artifacts/5-3-top-level-branding-copy.md:125] — fixed, count corrected to 160
- [x] [Review][Defer] Task 3's tone-boundary grep omits `src/cli/main.rs` (where the real runtime `println!`/`eprintln!` strings live) and its flavor-vocabulary pattern doesn't cover all of brand-identity.md's mascot/palette terms [_bmad-output/implementation-artifacts/5-3-top-level-branding-copy.md:76] — deferred, pre-existing verification-tooling gap; re-running with main.rs + expanded vocab included still returns zero matches today
- [x] [Review][Defer] No regression test guards the CLI `--help` banner's new description line (AC #2 change), so a future `Cargo.toml` edit could silently drop it [Cargo.toml:8] — deferred, optional hardening beyond this copy-only story's stated acceptance bar (build+test clean)
