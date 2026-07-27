---
stepsCompleted: [document-discovery, prd-analysis, epic-coverage-validation, ux-alignment, epic-quality-review, final-assessment]
---

# Implementation Readiness Assessment Report

**Date:** 2026-07-27
**Project:** tomb-fido2

## Document Inventory

**PRD-equivalent (SPEC kernel):**
- `_bmad-output/specs/spec-tomb-fido2/SPEC.md`
- `_bmad-output/specs/spec-tomb-fido2/hooks.md` (companion — bind-hooks/exec-hooks mechanism)

**Architecture:**
- `_bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md`
- Reviews: `reviews/review-{adversarial,reconcile,rubric,versions}-v4.md` (latest cycle, run against Epic 4 changes)

**Epics & Stories:**
- `_bmad-output/planning-artifacts/epics.md`

**UX:** None — CLI/backend tool, no UI surface. Confirmed N/A by user.

**No duplicates found.** User confirmed this document set for the assessment.

## PRD Analysis (SPEC.md + hooks.md as PRD-equivalent)

SPEC.md expresses requirements as numbered **Capabilities** (CAP-N, each with `intent` + `success`) rather than FR/NFR. For traceability against the epics/stories, they are mapped below to FR (functional) and NFR (non-functional, from the Constraints section) numbering, extracted independently from SPEC.md/hooks.md — not copied from epics.md — so Step 3 can validate the two against each other rather than assume they already agree.

### Functional Requirements Extracted

FR1 (CAP-1): User can unlock a LUKS2 volume using a FIDO2 security key, with its filesystem mounted in the same operation.
FR2 (CAP-2): User can enroll an additional FIDO2 key as an alternate unlock method on an existing tomb (LUKS2 multi-keyslot).
FR3 (CAP-3): User can revoke a single FIDO2 key's keyslot, blocked if it would remove the last remaining valid keyslot.
FR4 (CAP-4): User drives create, unlock, enroll (incl. UV), revoke, close (single/all), slam, resize, info/key inspection, and dependency checking through one unified CLI.
FR5 (CAP-5): Tool guides every interactive step in plain, zero-FIDO2-knowledge language.
FR6 (CAP-6): Tool verifies all hard dependencies before any operation begins, exiting cleanly with an actionable error on failure.
FR7 (CAP-7): Every operation works unmodified against raw block devices/partitions, not only loop-mounted files.
FR8 (CAP-8): User can create a new tomb in one operation, file-backed (tool allocates the backing file, refuses if destination exists) or device-backed (defaults to full capacity, refuses if a LUKS2 header already exists, requires explicit wipe-warning confirmation), formatting LUKS2 + filesystem + bootstrap-enrolling the first FIDO2 key.
FR9 (CAP-9): User can close an unlocked tomb (unmount + re-lock), the symmetric counterpart to CAP-1.
FR10 (CAP-10): User can grow an existing tomb's LUKS2 volume + filesystem without recreating it or re-enrolling keys.
FR11 (CAP-11): User can unlock/mount a tomb read-only, with writes refused at both the LUKS2/dm-crypt mapping level and the filesystem level.
FR12 (CAP-12): User can view a tomb's technical info, including enrolled FIDO2 keys with labels, without unlocking it first.
FR13 (CAP-13): User can enroll a FIDO2 key with user-verification (fingerprint/PIN) instead of touch-only, at create's bootstrap enrollment and via standalone enroll.
FR14 (CAP-14): User can close every currently open tomb in one command (close-all).
FR15 (CAP-15): User can run "slam" — closes every open tomb and force-clears busy-mount processes via signal escalation (TERM→HUP→KILL), firing immediately with no confirmation prompt.
FR16 (CAP-16): User can define per-tomb bind-hooks (auto bind-mount on open) and an exec-hooks executable (run at open/close as the invoking user), with a per-invocation option to skip hook processing. Mechanism detail in hooks.md.
FR17 (CAP-17): Create and resize report real, named-stage progress messages as each stage begins/completes, replacing the prior single start/end message.

Total FRs: 17 (FR12–FR17 new for Epic 4)

### Non-Functional Requirements Extracted

NFR1: Only standard, low-level, well-tested primitives — LUKS/dm-crypt + FIDO2 hmac-secret; no proprietary formats, no single-vendor crypto.
NFR2: Filesystem operations use only standard tools/syscalls for the chosen filesystem type — no custom/tool-proprietary handling.
NFR3: Break-glass recoverability — README must document manual unlock/mount via bare `cryptsetup`/`mount`/`fido2-token`, zero dependency on the tool's own binary.
NFR4: Zero-FIDO2-knowledge UX mandatory across all prompts and errors, not just the happy path.
NFR5: Physical key presence at the exact moment of unlock is fixed, not configurable.
NFR6: No fallback auth paths — FIDO2 exclusive; no GPG or keyfile escape hatch.
NFR7: Must ship as a compiled binary (Go or Rust), not a shell script.
NFR8: Tool stays entirely unaware of backup strategy (README disclaimer only, no code/feature touches it).
NFR9: Tool must actively prevent revoking the last remaining valid keyslot.
NFR10: Best-effort avoidance of decrypted key material leaking/persisting in process memory.
NFR11: Read-only unlock (CAP-11) must refuse writes at both the dm-crypt mapping level and the filesystem level.
NFR12: Device-backed create defaults to the target's full capacity when no size given; a smaller user-supplied size is accepted but must never exceed device capacity.
NFR13: Create refuses rather than overwrites — aborts before any formatting if a file-backed destination exists or a device-backed target already carries a LUKS2 header.
NFR14: Device-backed create must show an explicit wipe/data-loss warning and require explicit user confirmation before formatting, even absent an existing LUKS2 header.
NFR15: The name `tomb-fido2` is a placeholder — CLI/binary name, package/module name, user-facing strings, and on-disk metadata must not hardcode it or assume permanence.
NFR16 (new, CAP-13): UV enrollment is a stronger verification mode of the existing FIDO2 mechanism, not a new auth path — does not violate NFR6.
NFR17 (new, CAP-14/15): Close-all/slam must discover open tombs by live-querying system state only — never a stored registry or lock file.
NFR18 (new, CAP-15): Slam fires with no confirmation prompt by design — emergency/panic-button framing overrides the general confirm-before-irreversible-action pattern (NFR14, revoke's last-keyslot guard).
NFR19 (new, CAP-16): Hooks run only at open/close (never create/resize/read-only-unlock); exec-hooks always runs as the invoking user, never elevated; hook files live per-tomb in the tomb root; exec-hooks must be a regular file (not a symlink), executable, owned by the invoking user or root, and not world-writable; bind-hooks entries must resolve within the tomb root (source) and `$HOME` (destination), rejecting path traversal; a per-invocation skip option must exist.
NFR20 (new, CAP-17): Create/resize must report distinct named-stage progress messages as each real stage occurs, not a single before/after message — reinforces FR17 rather than adding independent scope.

Total NFRs: 20 (NFR16–NFR20 new for Epic 4)

### Additional Requirements / Constraints

- **Non-goals (unchanged, reconfirmed for Epic 4):** remote/delegated unlock beyond cryptsetup's native FIDO2 token mode; post-quantum-readiness; shrinking an existing tomb (resize is grow-only).
- **Deferred HOW questions (explicitly flagged in SPEC.md Assumptions, owned by architecture):**
  1. Whether unlock/resize's existing token-based `open` call needs any change to support a UV-enrolled key (CAP-13), or cryptsetup's token machinery already handles it transparently.
  2. The concrete mechanism by which close-all/slam (CAP-14/15) live-enumerates all currently open tombs without a registry.
- **hooks.md companion:** fully specifies bind-hooks (two-column tomb-root→`$HOME` mapping file, containment + existence guardrails) and exec-hooks (regular-file/executable/ownership guardrails, `open`/`close` argument contracts) referenced by FR16/NFR19.

### PRD Completeness Assessment

SPEC.md is internally consistent and each capability has a testable `intent`/`success` pair. The two HOW-level questions above are explicitly named as open and deferred to architecture rather than silently missing — Step 3/4 should confirm ARCHITECTURE-SPINE.md actually resolves both. No FR/NFR text is vague or unmeasurable. One redundancy to note: NFR20 restates FR17's scope rather than adding new obligations — not a defect, just worth flagging so it isn't double-counted as two separate coverage gaps in epics.

## Epic Coverage Validation

### Epic FR Coverage Extracted

epics.md carries its own "Requirements Inventory" (FR1–17, NFR1–16) restating SPEC.md's capabilities, plus an explicit "FR Coverage Map":

FR1: Epic 1 | FR2: Epic 2 | FR3: Epic 2 | FR4: Epic 1 (established, extended 2/3) | FR5: Epic 1 (established, extended 2/3) | FR6: Epic 1 (established, extended 2/3) | FR7: Epic 1 (established, extended 2/3) | FR8: Epic 1 | FR9: Epic 3 | FR10: Epic 3 | FR11: Epic 3 | FR12: Epic 4 | FR13: Epic 4 | FR14: Epic 4 | FR15: Epic 4 | FR16: Epic 4 | FR17: Epic 4

Total FRs in epics: 17

### FR Coverage Analysis

| FR Number | PRD Requirement (summary) | Epic Coverage | Status |
|---|---|---|---|
| FR1 | Unlock + mount in one operation | Epic 1, Story 1.7 | ✓ Covered |
| FR2 | Enroll additional FIDO2 key | Epic 2, Story 2.1 | ✓ Covered |
| FR3 | Revoke a key, last-keyslot guard | Epic 2, Story 2.2 | ✓ Covered |
| FR4 | Unified CLI dispatch | Epic 1 (Story 1.8), extended 2/3 | ✓ Covered |
| FR5 | Plain-language guidance | Epic 1 (Story 1.8), extended 2/3 | ✓ Covered |
| FR6 | Preflight dependency check | Epic 1, Story 1.4 | ✓ Covered |
| FR7 | Raw device/partition support | Epic 1 (Stories 1.5–1.7), extended 2/3 | ✓ Covered |
| FR8 | Create tomb, file- or device-backed | Epic 1, Stories 1.5 & 1.6 | ✓ Covered |
| FR9 | Close an unlocked tomb | Epic 3, Story 3.1 | ✓ Covered |
| FR10 | Grow an existing tomb | Epic 3, Story 3.2 | ✓ Covered |
| FR11 | Read-only unlock | Epic 3, Story 3.3 | ✓ Covered |
| FR12 | Info / key inspection | Epic 4, Story 4.1 | ✓ Covered |
| FR13 | UV enrollment | Epic 4, Story 4.3 | ✓ Covered |
| FR14 | Close-all | Epic 4, Story 4.5 | ✓ Covered |
| FR15 | Slam (emergency close-all) | Epic 4, Story 4.6 | ✓ Covered |
| FR16 | Bind-hooks/exec-hooks | Epic 4, Story 4.4 | ✓ Covered |
| FR17 | Named-stage progress reporting | Epic 4, Story 4.2 | ✓ Covered |

### Missing Requirements

None. All 17 SPEC.md capabilities (CAP-1..17 → FR1..17) have a traceable epic/story home, and the 6 new Epic 4 FRs (FR12–17) each map to exactly one of the 6 new Epic 4 stories with acceptance criteria that echo the corresponding CAP's `success` text.

### Secondary Observation (NFR traceability, non-blocking)

epics.md's Requirements Inventory lists 16 NFRs against the 20 I independently extracted from SPEC.md's Constraints (Step 2). The 4 not carried as standalone NFR line items in epics.md:
- Device-backed create defaults to full device capacity when no size given; a user size must never exceed it (my NFR12).
- Create refuses rather than overwrites an existing file-backed destination or a device already carrying a LUKS2 header (my NFR13).
- Device-backed create requires an explicit wipe/data-loss warning + confirmation before formatting (my NFR14).
- Placeholder-name isolation — `tomb-fido2` must not be hardcoded/assumed permanent anywhere (my NFR15).

These aren't functionally dropped: all four are enforced in Epic 1's Story 1.6 acceptance criteria (device-backed create ACs cover default capacity, size cap, refuse-on-header, mandatory confirmation) and AD-13 (placeholder-name isolation), and this gap predates the Epic 4 commits — it's inherited from the original Epic 1-3 Requirements Inventory, not something the Epic 4 update introduced or should be expected to fix. Flagging for traceability completeness only; does not block Epic 4 readiness.

### Coverage Statistics

- Total PRD FRs (SPEC.md CAP-1..17): 17
- FRs covered in epics: 17
- Coverage percentage: 100%

## UX Alignment Assessment

### UX Document Status

Not Found — confirmed in Step 1 as expected, not a gap: `tomb-fido2` is a CLI-only tool with no web/mobile/GUI surface. epics.md's own "UX Design Requirements" section states this explicitly and redirects interaction/plain-language requirements to FR5/NFR3.

### Alignment Issues

None. The "UX" concerns that do exist for a CLI tool (plain-language prompts/errors, per-stage progress messages) are:
- Specified in SPEC.md as FR5 (CAP-5, zero-FIDO2-knowledge guidance), NFR4 (zero-FIDO2-knowledge UX mandatory across all prompts/errors), and FR17/NFR20 (named-stage progress reporting, new for Epic 4).
- Architecturally supported by the existing `cli::ux` translation boundary (established Epic 1, Story 1.8: "no internal jargon leaks to the user") and extended for Epic 4 by AD-19's `progress: &dyn Fn(Stage)` callback seam + `cli::ux::translate_stage`, which explicitly keeps `domain` free of direct I/O and routes all stage text through the same translate-at-the-boundary convention.
- Covered by story acceptance criteria: Story 4.2 (progress messages, exact stage lists for create/resize, including the raw-device create exception of only 2 resize stages) and Story 4.1/4.3/4.4/4.5/4.6 all inherit the plain-language error convention from Epic 1.

No CLI-level interaction pattern is introduced by Epic 4 without a corresponding architecture seam or story AC.

### Warnings

None. UX is not implied beyond CLI text interaction, which is fully traced through FR/NFR → architecture (AD-19, cli::ux) → stories.

## Epic Quality Review

Scope: all 4 epics checked for structural compliance; Epic 4 (the new work) reviewed in full depth since Epics 1–3 are already shipped/merged (per project history) and re-litigating their structure doesn't change Epic 4 readiness.

### Epic Structure Validation

| Epic | User value focus | Independence |
|---|---|---|
| Epic 1: Create & Open a Tomb | User-centric title/goal. **Pre-existing concern:** Stories 1.1–1.3 (Nix devShell, CI, release automation) are technical/infra stories with no direct end-user value — a red-flag pattern per this review's own rubric. Inherited from the already-shipped Epic 1; not something Epic 4 introduced or can retroactively fix. | Stands alone. ✓ |
| Epic 2: Manage Tomb Access | User-centric. ✓ | Uses only Epic 1 output. ✓ |
| Epic 3: Tomb Lifecycle & Advanced Access | User-centric. ✓ | Uses only Epic 1 output. ✓ |
| Epic 4: Advanced Operations & Automation | User-centric goal statement — inspect keys, bulk close, panic-button slam, hooks automation, progress visibility are all user-facing outcomes, not technical milestones. ✓ | Uses only Epic 1–3 outputs (create/unlock, enroll, close, resize). No forward dependency on any future epic — Epic 4 is currently the last epic in the backlog. ✓ |

### Story Quality Assessment (Epic 4 — the new work)

**Sizing & independence:**
- Stories 4.1–4.6 each deliver standalone user value (info, progress, UV enrollment, hooks, close-all, slam) and can each be verified independently against a built Epic 1–3 tomb.
- Within-epic dependencies are backward-only, consistent with BMad sequencing rules: Story 4.5 (close-all) reuses Story 3.1's close sequence and Story 4.4's hooks step ("applies the same sequence as a single close"); Story 4.6 (slam) reuses Story 4.5's discovery mechanism ("discovers open tombs the same way close-all does"). No story references a not-yet-described later story.
- Story 4.4 (bind-hooks + exec-hooks) is the largest story in the epic — it bundles two mechanisms, guardrail checks for both, a skip flag, and ordered open/close sequencing into one story. This is defensible (hooks.md treats bind-hooks/exec-hooks as one integrated feature, and CAP-16 is a single capability), but it's the epic's biggest single unit of work — flagged as a sizing observation, not a violation.

**Acceptance criteria quality:**
- All 6 stories use consistent Given/When/Then structure.
- Error/edge paths are explicitly covered, not just happy paths: Story 4.4 covers guardrail failures (rollback via AD-11's pattern) and path-traversal rejection; Story 4.5 covers a mid-batch failure continuing rather than aborting, and the zero-open-tombs case; Story 4.6 covers a process that never releases the mount, reported as an isolated failure without blocking the rest of the batch.
- No vague or unmeasurable criteria found (e.g. Story 4.2 names the exact stage list per operation, including the raw-device create exception of 2 vs 3 resize stages).

**SPEC deferred-HOW resolution check:** SPEC.md's Assumptions section explicitly deferred two HOW questions to architecture (Step 2 of this review). Both are now resolved: AD-16 confirms cryptsetup's `systemd-fido2` token plugin reads UV requirement automatically, no `open`/`resize` change needed; AD-17 resolves close-all/slam discovery via `list_open_mappings()` (`dmsetup ls` filtered by the fixed prefix). Story 4.3 and 4.5/4.6 ACs reflect both resolutions correctly.

### Dependency Analysis

No forward dependencies found in Epic 4. No database/entity timing issues (no database in this project — LUKS2 token fields are written when first needed: `key_label`/`filesystem` at Story 1.5 create, read back at Story 3.2 resize; UV's `user_verification` field added at enrollment time in Story 4.3, consistent with the "create tables only when needed" analogue).

### Special Implementation Checks

- **Starter template:** Architecture explicitly states "No starter template — greenfield project," and Story 1.1 is scaffolding, not a template clone — consistent, no violation.
- **Greenfield indicators:** Dev environment (1.1), CI (1.2), and release automation (1.3) are all present early in Epic 1 — matches expected greenfield pattern.

### Quality Findings by Severity

**🔴 Critical Violations:** None.

**🟠 Major Issues:** None.

**🟡 Minor Concerns:**
1. Epic 1 Stories 1.1–1.3 are technical/infra stories with no direct end-user value (pre-existing, not introduced by Epic 4; no action needed since Epic 1 already shipped).
2. epics.md's FR Coverage Map entries for FR4/FR5/FR6/FR7 read "Epic 1 (established), extended in Epics 2 & 3" but omit Epic 4, even though Epic 4 stories demonstrably extend all four (Story 4.1/4.5 explicitly run `domain::preflight` — FR6; every Epic 4 story adds a new CLI subcommand — FR4; hooks/slam/close-all all inherit plain-language error translation — FR5; every Epic 4 story works identically against raw devices — FR7). Recommend updating the coverage-map annotation text to include Epic 4 for consistency — cosmetic, not a functional gap since the underlying ACs already cover it.
3. Story 4.4 is the largest single story in Epic 4 (bind-hooks + exec-hooks + guardrails + skip flag + ordering) — worth a sizing gut-check by the team before starting implementation, though the review does not consider a split mandatory.

## Summary and Recommendations

### Overall Readiness Status

**READY**

Epic 4 (CAP-12..17 / FR12-17) is implementation-ready. SPEC.md → ARCHITECTURE-SPINE.md → epics.md are aligned: 100% FR coverage (17/17), both SPEC-deferred HOW questions are resolved in architecture (AD-16, AD-17), the v4 reviewer gate already ran against these changes (adversarial/reconcile/rubric/version-verification, per commit `c9732cd`), and no epic/story carries a forward dependency, a technical-milestone epic, or an untestable acceptance criterion. Zero critical or major issues were found.

### Critical Issues Requiring Immediate Action

None.

### Recommended Next Steps

1. (Optional, cosmetic) Update epics.md's FR Coverage Map so FR4/FR5/FR6/FR7 read "extended in Epics 2, 3 & 4" instead of stopping at Epic 3 — the underlying story ACs already cover Epic 4's extension, only the summary annotation is stale.
2. (Optional, traceability) Add the 4 SPEC.md constraints not currently listed as standalone NFR line items in epics.md's Requirements Inventory (device-backed default-capacity/size-cap, create refuse-on-existing, mandatory wipe-warning confirmation, placeholder-name isolation) — they're already enforced via Story 1.6's ACs and AD-13, so this is a documentation-completeness nice-to-have, not a rework.
3. Before starting Story 4.4, do a quick team sizing check — it's the largest story in the epic (bind-hooks + exec-hooks + guardrails + skip-flag + ordering) and may split cleanly if it proves too large in practice, though the review found no correctness reason it must.
4. Proceed to implementation of Epic 4 in story order (4.1 → 4.6) as sequenced — no reordering required.

### Final Note

This assessment identified 4 issues, all minor and non-blocking, across 2 categories (traceability-documentation completeness, story-sizing observation). None require rework before implementation — they're optional polish. Epic 4 may proceed to implementation as-is.

---
**Assessor:** John (PM) via bmad-check-implementation-readiness
**Date:** 2026-07-27
