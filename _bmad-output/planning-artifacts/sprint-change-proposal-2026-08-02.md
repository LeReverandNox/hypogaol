# Sprint Change Proposal — Rebrand to Hypogaol

**Date:** 2026-08-02
**Trigger commit:** `9ad916d` — "docs: add project-naming brainstorm outputs for the Hypogaol rename"
**Prepared by:** Amelia (Developer agent), via `bmad-correct-course`

## 1. Issue Summary

The project's brainstorming session (`_bmad-output/brainstorming/brainstorm-project-naming-2026-08-02/`) landed on a final permanent name and full brand identity, replacing the `tomb-fido2` placeholder that every epic to date (1–4, all shipped in v0.1.0) has explicitly carried as provisional:

- **Name:** Hypogaol (Bloodborne "Hypogean Gaol" + hypogeum blend; zero OSS collision confirmed)
- **Mascot:** Hypo, a stone gargoyle warden
- **Mark:** gothic rose-window tracery circle with a FIDO2-key-in-keyhole center
- **Palette:** stone grey / moss green, lantern-amber accent reserved for the unlock/success state
- **Tagline:** "Sealed until touched."
- **Tone rule:** flavor confined to name/mascot/mark/tagline/top-level banner — CLI subcommands, error messages, and README body stay plain and literal

The brainstorm's own intent doc explicitly bundles **two separate renames** and says not to conflate them:

1. **Product name:** `tomb-fido2` → `Hypogaol` (package, binary, repo, README title/badges, CLI banner)
2. **Domain term:** `tomb` → `volume` throughout source code, wherever it's used as the noun for the encrypted-container concept (this is deliberately name-independent, so it doesn't need to move again if branding ever changes again)

## 2. Impact Analysis

### Epic impact
None of the four completed epics' delivered functionality (CAP-1..17) is invalidated. This is a naming/branding layer, not a functional or architectural change — no requirement, acceptance criterion, or design decision needs to change in substance.

Critically, the architecture already anticipated this exact moment:

- **AD-13 (Placeholder-name isolation)** was written specifically so "a future rename touches a handful of scattered literals instead of one" — the CLI binary/package name is read from exactly one source (the Cargo package name), never duplicated as a string literal. This makes the **product-name** half of the rename cheap.
- AD-13 explicitly does **not** cover the domain word "tomb" itself — that word is the ordinary noun used throughout the codebase for the encrypted-container concept, not an "implementation identifier" in AD-13's sense.

A repo-wide search confirms the asymmetry: `tomb` (case-insensitive) appears **537 times** across every `src/` module, every test file, `Cargo.toml`/`Cargo.lock`, `flake.nix`, `README.md`, `CHANGELOG.md`, and the BMAD custom config — i.e., cross-cutting through all four completed epics' code and every planning artifact. That's the bulk of the effort, and it's exactly the deliberately-decoupled "domain term" rename the brainstorm called out.

### Story / artifact impact
- No existing story's acceptance criteria change in substance — behavior is untouched.
- **Historical planning docs** (SPEC.md, epics.md, ARCHITECTURE-SPINE.md, all four epic retros) already carry a standing exception: "architecture and planning documents may keep referring to the project as `tomb-fido2` for continuity until renamed" (AD-13). Recommendation: leave Epics 1–4's existing text as frozen historical record (matches how the retros already treat past decisions), and add one short addendum note to SPEC.md/AD-13 confirming the rename executed — not a rewrite of history.
- **Live, non-historical artifacts** that must change: `_bmad/bmm/config.yaml` (`project_name: tomb-fido2`), `_bmad/custom/github-automation-reference.md` (repo path, GitHub Project display name "Tomb FIDO2"), `Cargo.toml`, `README.md`, `CHANGELOG.md`, `flake.nix`, and all of `src/`/`tests/`.
- **External/shared-system action:** renaming the actual GitHub repository (`LeReverandNox/tomb-fido2` → `.../hypogaol`) is outside any agent's scope — it's a one-time, user-owned action with real blast radius (existing clone URLs, stars, any external links). Flagging this explicitly rather than having Dev silently attempt it.

### Technical impact
Mechanical, low-risk, large-surface rename. No functional/behavioral change is intended. The main risk is a naive blanket find-replace catching incidental uses of the word "tomb" that aren't the product/domain term (unlikely here, but worth a story-level acceptance criterion) — the existing unit test suite (AD-7) is the regression safety net; a clean `make test` pass after the rename is the acceptance bar, not a manual audit.

## 3. Recommended Approach

**Option 1 — Direct Adjustment via a new Epic.** Not Option 2 (nothing failed, nothing to roll back) and not Option 3 (no MVP/PRD scope change — no capability is added, removed, or altered).

**This is a new Epic, not a story bolted onto an existing one** — Epics 1–4 are complete and shipped (v0.1.0 released); reopening any of them to fold in unrelated rebrand work would misrepresent their status. The rebrand is a single coherent, cross-cutting unit of work with its own acceptance criteria (does it compile, does `make test` pass, is the tone boundary respected), which is exactly what an epic is for.

- **Effort:** Medium — large mechanical surface (537 occurrences), but zero design ambiguity; every naming/tone decision is already locked in the brainstorm docs.
- **Risk:** Low — no functional or architectural change; strong existing test coverage catches accidental regressions from the rename.

## 4. Detailed Change Proposals

### New Epic 5: Rebrand to Hypogaol

> Users and contributors see the project consistently as Hypogaol everywhere it presents itself — package, binary, repo, README, and CLI banner — while the codebase's internal vocabulary moves from the placeholder-era "tomb" to the generic, brand-independent "volume." No functional behavior changes.

**Story 5.1 — Product Identity Rename**
`Cargo.toml` (`name`, `repository`), `_bmad/bmm/config.yaml` (`project_name`), README title/badges/CI links, CLI `--help` banner (already sourced from the package name per AD-13 — verify, don't reintroduce a literal), CHANGELOG header, `flake.nix` references. Depends on the GitHub repo rename happening first (see §5) so link updates land correctly in one pass.

**Story 5.2 — Domain Term Rename (`tomb` → `volume`)**
All `src/` identifiers, comments, error/log/CLI-output strings; all `tests/unit`/`tests/hardware` file names, identifiers, and fixtures; README body; `hooks.md`. Acceptance: `cargo build`, `make test`, and `make test-hardware` (manual) all pass unchanged in behavior; no remaining "tomb" outside historical `_bmad-output` docs and the frozen AD-13 placeholder-isolation note.

**Story 5.3 — Top-Level Branding Copy**
Apply the tagline ("Sealed until touched."), the "gaol reads like jail" pronunciation note, and mascot/mark *descriptive* placeholders (actual artwork production is out of scope here per `brand-identity.md` §7 — this story is copy only) to the README header and `--help` banner. Enforce the brand-identity.md tone boundary: flavor stays out of CLI subcommands, error messages, and README body text.

**Story 5.4 — Update Live Planning/Automation Pointers**
`_bmad/custom/github-automation-reference.md` (repo path, GitHub Project display name), and a short addendum appended to SPEC.md's Constraints / ARCHITECTURE-SPINE.md's AD-13 noting the rename executed on 2026-08-02 (append-only, not a rewrite of the historical text).

### Non-changes (explicitly out of scope for Epic 5)
- Epics 1–4 text, their story files, and retros — left as historical record.
- Actual mascot/mark artwork production — brand-identity.md is the reference spec for whoever eventually draws it; no story here produces image assets.
- Any functional/behavioral change to any capability (CAP-1..17).

## 5. Implementation Handoff

**Scope classification: Moderate** — requires backlog reorganization (new Epic 5 + stories) but no fundamental PM/Architect replan; the architecture explicitly pre-approved this scenario via AD-13.

**Routing:**
- **Product Owner / Developer (Amelia):** formalize Epic 5 in `epics.md` and add `epic-5` backlog entries to `sprint-status.yaml`, then run `bmad-create-story` → `bmad-dev-story` per story as usual.
- **LeReverandNox (owner action, before Story 5.1 lands):** rename the GitHub repository `LeReverandNox/tomb-fido2` → `LeReverandNox/hypogaol` (and the GitHub Project display name if desired). This is manual and external — no agent should attempt it.

**Success criteria:** `Hypogaol` is the only product name surfaced anywhere live (code, config, README, CLI banner, GitHub repo); `volume` fully replaces `tomb` as the domain term in all live code/docs; `cargo build` + `make test` pass with zero behavioral diff; brand-identity.md's tone boundary holds (no flavor leaking into CLI/errors/README body).
