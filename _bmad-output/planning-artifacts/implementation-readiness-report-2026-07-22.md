---
stepsCompleted:
  - step-01-document-discovery
  - step-02-prd-analysis
  - step-03-epic-coverage-validation
  - step-04-ux-alignment
  - step-05-epic-quality-review
  - step-06-final-assessment
documentsInScope:
  spec: _bmad-output/specs/spec-tomb-fido2/SPEC.md
  architecture: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md
  epics: _bmad-output/planning-artifacts/epics.md
  readme: README.md
  prd: null
  ux: null
---

# Implementation Readiness Assessment Report

**Date:** 2026-07-22
**Project:** tomb-fido2

## Step 1: Document Discovery

**Documents in scope for this assessment:**

- **SPEC (PRD-equivalent):** `_bmad-output/specs/spec-tomb-fido2/SPEC.md` (11,406 bytes, modified 2026-07-22 15:45)
  - Confirmed by user: project uses SPEC.md in place of a full PRD (small project, per Mary's recommendation).
- **Architecture:** `_bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md` (28,762 bytes, modified 2026-07-22 15:46)
  - Supporting review artifacts present in `reviews/` (adversarial, rubric, versions, reconcile — v2/v3).
- **Epics & Stories:** `_bmad-output/planning-artifacts/epics.md` (35,489 bytes, modified 2026-07-22 15:36)
- **README:** `README.md` (8,937 bytes, project root, modified 2026-07-22 14:58)

**Explicitly out of scope:**

- **PRD:** Not applicable — SPEC.md serves as the PRD-equivalent for this assessment.
- **UX:** Not applicable — tomb-fido2 is a CLI tool with no dedicated UI/UX surface. Confirmed by user.

**Duplicates:** None found — one canonical version per document type.

## Step 2: PRD Analysis (SPEC.md as PRD-equivalent)

**Note on mapping:** SPEC.md is capability-based (`CAP-*`), not FR/NFR-numbered. Each Capability is treated as a Functional Requirement below (intent = the requirement, success = its acceptance criterion). Quality-attribute Constraints are extracted as NFRs; remaining Constraints and Non-goals are captured as Additional Requirements/Boundaries.

### Functional Requirements Extracted

FR1 (CAP-1): User can unlock a LUKS2 volume using a FIDO2 security key via the tool, with its filesystem mounted and ready to use in the same operation.
- Success: A user with zero FIDO2 familiarity unlocks and mounts a LUKS2 volume on first try, guided only by the tool's own prompts, with no external documentation consulted.

FR2 (CAP-2): User can enroll an additional FIDO2 key as an alternate unlock method on an already-created tomb, using LUKS2's native multi-keyslot support (up to 32 slots).
- Success: After enrollment, the volume unlocks successfully with either the original key or the newly enrolled key.

FR3 (CAP-3): User can revoke a single FIDO2 key's keyslot, removing it as a valid unlock method, unless doing so would remove the last remaining valid keyslot.
- Success: After revocation, the removed key no longer unlocks the volume, while other enrolled keys still do. Attempting to revoke the last remaining valid keyslot is blocked with a clear explanation, and the volume remains unlockable.

FR4 (CAP-4): User drives creation, unlock, enrollment, revocation, closing, resizing, and dependency checking through one unified CLI, replacing direct use of `cryptsetup`, `fido2-token`, `systemd-cryptenroll`, `mkfs`, and `mount`/`umount`.
- Success: No workflow in v1 scope requires the user to invoke the underlying tools directly.

FR5 (CAP-5): Tool guides the user through every interactive step (e.g. the key-touch moment) in plain language, assuming zero FIDO2 knowledge.
- Success: Prompts and error messages never assume FIDO2 familiarity (e.g. "Please touch your security key button," never "Awaiting UP").

FR6 (CAP-6): Tool verifies all hard dependencies (LUKS2 FIDO2/hmac-secret support, required binaries, kernel features) before any operation begins.
- Success: On a missing dependency, the tool exits cleanly before starting the operation, with a detailed, actionable error message — never a mid-operation failure.

FR7 (CAP-7): User can run any operation (create, unlock, enroll, revoke, close, resize) directly against raw physical block devices/partitions, not only file-backed loop devices.
- Success: The same CLI commands work unmodified against a raw partition path and a loop-mounted file, for every operation.

FR8 (CAP-8): User can create a brand-new tomb in one operation, in either file-backed mode (tool allocates the backing file at a given destination/size, refusing if the destination already exists) or device-backed mode (tool targets an existing raw block device, defaulting to full capacity or a smaller user-supplied size, refusing if a LUKS2 header already exists, requiring explicit wipe-warning confirmation otherwise). Either way, the tool formats LUKS2, creates a user-selected filesystem, and bootstrap-enrolls the first FIDO2 key.
- Success: After creation, the new tomb unlocks and mounts successfully (CAP-1) using the newly enrolled key. File-backed: backing file exists at the requested destination/size, and create aborts pre-emptively if the path already existed. Device-backed: used capacity matches the requested size (or full device capacity) and never exceeds it; create aborts if a LUKS2 header already exists, and otherwise proceeds only after explicit wipe-warning confirmation.

FR9 (CAP-9): User can close an unlocked tomb — unmount its filesystem and re-lock the LUKS2 volume.
- Success: After closing, the mount point is no longer accessible and the volume requires a FIDO2 key to unlock again.

FR10 (CAP-10): User can grow an existing tomb's LUKS2 volume and filesystem to a larger size without recreating it or re-enrolling any FIDO2 keys.
- Success: After growing, the volume's usable capacity reflects the new size, all previously enrolled FIDO2 keys still unlock it, and no existing data is lost.

FR11 (CAP-11): User can unlock and mount an existing tomb in read-only mode, enforced at the LUKS2/dm-crypt mapping level, not merely the filesystem mount.
- Success: When read-only mode is requested, both the underlying mapper device and the mounted filesystem reject write attempts (including a later remount attempt), while a normal unlock continues to allow writes as before.

**Total FRs: 11**

### Non-Functional Requirements Extracted

NFR1 (Security): FIDO2 hmac-secret is the exclusive unlock mechanism — no GPG or keyfile fallback/escape hatch permitted.
NFR2 (Security): Tool must make a best-effort attempt to avoid decrypted key material leaking or persisting in process memory (specific mechanism left to architecture).
NFR3 (Usability): Zero-FIDO2-knowledge UX is mandatory across all prompts and errors, not just the happy path.
NFR4 (Reliability/Standardness): Only standard, low-level, well-tested primitives — LUKS/dm-crypt + FIDO2 hmac-secret; no proprietary formats or single-vendor-maintained crypto.
NFR5 (Reliability/Standardness): Filesystem operations (mkfs, growfs, mount/unmount) use only standard, well-known tools/syscalls — no custom or tool-proprietary filesystem handling.
NFR6 (Recoverability/Compliance): Break-glass recoverability — README must document manually unlocking/mounting using only bare `cryptsetup`, `mount`/`umount`, `fido2-token`, with zero dependency on the tool's own binary surviving.
NFR7 (Reliability): Physical key presence at the exact moment of unlock is fixed (not configurable) — limited to exactly what cryptsetup's native FIDO2 token mode offers.

**Total NFRs: 7**

### Additional Requirements / Boundaries

- **Distribution constraint:** Must ship as a real compiled binary (Go or Rust), not a shell script.
- **Scope boundary:** Tool stays entirely unaware of backup strategy — README may recommend a 3-2-1-style backup as a disclaimer only; no code/feature may touch backup concerns.
- **Safety behavior:** Tool must actively prevent revoking the last remaining valid keyslot (deliberate departure from raw cryptsetup).
- **Data-loss guardrails (CAP-8 related):** File-backed create never requires a pre-existing backing file (no manual `dd`/`fallocate`/`truncate` step); device-backed create defaults to full device capacity when no size given, must never exceed actual capacity; create refuses outright rather than overwriting existing destinations/headers; device-backed create requires explicit user confirmation of a wipe/data-loss warning.
- **Naming/branding constraint:** `tomb-fido2` is a placeholder pending a permanent name — CLI/binary name, package/module name, user-facing strings, and on-disk metadata field names must not hardcode it or assume permanence.
- **Non-goals (explicitly out of v1 scope):** Remote/delegated unlock beyond cryptsetup's native FIDO2 token mode; post-quantum-readiness features; shrinking an existing tomb (resize is grow-only for v1).
- **Assumption (documented in SPEC):** Should-Have items from the source brainstorm (break-glass README procedure, 3-2-1 backup disclaimer) were folded into Constraints/NFRs rather than kept as separate capabilities, since they bend documentation/design decisions rather than describing testable tool behavior.

### PRD (SPEC) Completeness Assessment

The SPEC is unusually precise for a "PRD-equivalent": every capability has a paired, testable success criterion, and edge-case behavior (last-keyslot revocation, existing-destination/header collisions, capacity bounds) is spelled out in the Constraints rather than left implicit. No ambiguity was found in the capability success criteria themselves. Two things to carry into coverage validation:
1. There is no explicit NFR/constraint around resize direction other than "grow-only" (Non-goals) — confirm epics/architecture don't silently imply shrink support anywhere.
2. Constraints are doing double duty (both true NFRs and business/scope boundaries) — coverage validation should check epics against the full Constraints list, not just the NFR subset, since several (e.g. last-keyslot protection, wipe-warning confirmation) are directly testable behaviors equivalent in weight to a Capability.

## Step 3: Epic Coverage Validation

epics.md restates the SPEC's Requirements Inventory verbatim (FR1–FR11, NFR1–NFR11, Additional Requirements, Non-goals) before decomposing into 3 epics / 13 stories, which makes traceability checking straightforward — every requirement below is quoted from the same source the SPEC itself uses, so no re-derivation error is possible here.

### Coverage Matrix

| FR Number | SPEC Requirement (short) | Epic/Story Coverage | Status |
| --------- | ------------------------ | -------------------- | ------ |
| FR1 | Unlock LUKS2 volume, filesystem mounted in same op | Epic 1 → Story 1.6 (Unlock and Mount a Tomb) | ✓ Covered |
| FR2 | Enroll additional FIDO2 key | Epic 2 → Story 2.1 (Enroll an Additional FIDO2 Key) | ✓ Covered |
| FR3 | Revoke a key's keyslot, guarded against last-keyslot lockout | Epic 2 → Story 2.2 (Revoke a FIDO2 Key, Guarded Against Last-Keyslot Lockout) | ✓ Covered |
| FR4 | Unified CLI replacing direct tool invocation | Epic 1 → Story 1.7 (Unified CLI Dispatch & Plain-Language Errors); extended implicitly in Epics 2 & 3 | ✓ Covered |
| FR5 | Zero-FIDO2-knowledge plain-language guidance | Epic 1 → Story 1.7; extended implicitly in Epics 2 & 3 | ✓ Covered |
| FR6 | Preflight dependency verification before any operation | Epic 1 → Story 1.3 (Dependency Preflight Check); reused per-workflow in Stories 3.1/3.2/3.3 and referenced in 2.1 | ✓ Covered |
| FR7 | Any operation works against raw device/partition, not just loop file | Epic 1 → Story 1.5 (device-backed create), Story 1.6 (explicit raw-device AC for unlock); Epic 3 → Story 3.2 (explicit raw-device AC for resize) | ⚠️ Partially covered (see note) |
| FR8 | Create a new tomb, file-backed or device-backed, format+fs+bootstrap-enroll | Epic 1 → Story 1.4 (file-backed) + Story 1.5 (device-backed) | ✓ Covered |
| FR9 | Close an unlocked tomb (unmount + re-lock) | Epic 3 → Story 3.1 (Close an Unlocked Tomb) | ✓ Covered |
| FR10 | Grow an existing tomb's volume and filesystem | Epic 3 → Story 3.2 (Grow an Existing Tomb's Capacity) | ✓ Covered |
| FR11 | Unlock/mount read-only, enforced at mapping + filesystem level | Epic 3 → Story 3.3 (Unlock a Tomb Read-Only) | ✓ Covered |

**Total SPEC FRs: 11**
**FRs with at least one story: 11 (100%)**
**FRs with full acceptance-criteria parity across all applicable stories: 10 of 11 (see FR7 note)**

### Missing / Partial Coverage

No FR is entirely unimplemented. One partial-coverage note:

**FR7 — raw device/partition parity ("any operation... for every operation")**
- Impact: Minor. The requirement's success criterion is that *every* operation (create, unlock, enroll, revoke, close, resize) works identically against a raw device and a loop file. Explicit Given/When/Then acceptance criteria testing the raw-device path exist for create (Story 1.5), unlock (Story 1.6), and resize (Story 3.2). Stories 2.1 (enroll), 2.2 (revoke), 3.1 (close), and 3.3 (read-only unlock) have no acceptance criterion that explicitly exercises a raw-device target — their ACs are written path-agnostically ("an existing tomb"), which is defensible since the deterministic mapping-name derivation (AD-12) is path-type-agnostic by design, but it leaves no explicit test evidence of raw-device parity for those four operations.
- Recommendation: Not a blocking gap given AD-12's design makes path-type genuinely transparent to those workflows, but flag for the epic quality review (Step 5) whether at least one AC per remaining story should explicitly assert raw-device-path equivalence, or whether a single cross-cutting integration test covers it once for all six operations.

### Coverage Statistics

- Total SPEC FRs: 11
- FRs covered in epics (at least one story): 11
- Coverage percentage: 100%
- FRs with full multi-story AC parity: 10/11 (91%) — see FR7 partial-coverage note above

## Step 4: UX Alignment Assessment

### UX Document Status

Not Found. Search patterns `*ux*.md` (whole) and `*ux*/index.md` (sharded) under planning-artifacts returned no results — confirmed with the user (this is a CLI-only tool, no dedicated UI/UX surface).

### Is UX Implied?

No web/mobile/GUI component exists or is implied anywhere in SPEC.md, ARCHITECTURE-SPINE.md, or epics.md. The only "user experience" surface is CLI interaction (prompts, error messages, subcommand ergonomics), and this is explicitly and consistently handled as a first-class requirement without a separate UX document:

- **SPEC.md:** CAP-5 (plain-language guidance) and NFR3-equivalent "Zero-FIDO2-knowledge UX is mandatory across all prompts and errors" are stated directly as Capability/Constraint, not deferred to a UX artifact.
- **epics.md:** Explicitly states "No UX design contract exists — this is a CLI-only tool; interaction/plain-language requirements are captured under FR5/NFR3 instead," and Story 1.7 (Unified CLI Dispatch & Plain-Language Errors) carries concrete acceptance criteria for this (e.g., "Please touch your security key," never "Awaiting UP").
- **Architecture:** A dedicated `cli::ux` boundary is named in the epics' Additional Requirements ("domain errors are a typed enum, translated to plain-language text only at the cli boundary") and reinforced by Story 1.7's AC that `cli` never touches a port directly — i.e., the architecture has a real seam dedicated to UX-quality translation, not an afterthought.

### Alignment Issues

None. The CLI-interaction "UX" concern is traceable end-to-end: SPEC (CAP-5) → epics FR5/NFR3 → Story 1.7 acceptance criteria → architecture's `cli::ux` boundary. No gap between what's promised and what's architected/storied.

### Warnings

None. Absence of a dedicated UX document is appropriate for this project's scope (CLI-only, no GUI/web/mobile surface) and was confirmed by the user. No architectural gap identified for the CLI's plain-language requirement.

## Step 5: Epic Quality Review

Applying create-epics-and-stories standards rigorously across all 3 epics / 13 stories.

### Epic Structure Validation

| Epic | User-Value Title? | Stands Alone? | Forward Dependency on Later Epic? |
| --- | --- | --- | --- |
| Epic 1: Create & Open a Tomb (Foundation) | ✓ user-centric | ✓ fully self-contained | None |
| Epic 2: Manage Tomb Access (Key Lifecycle) | ✓ user-centric | ✓ (depends only on Epic 1's output — an existing tomb) | None |
| Epic 3: Tomb Lifecycle & Advanced Access | ✓ user-centric | ✓ (depends only on Epic 1's output; does not require Epic 2) | None |

No epic is a disguised technical milestone, and no epic requires a later epic to function — the dependency direction is strictly backward (2←1, 3←1) throughout.

### Story Quality Assessment

Acceptance criteria across all 13 stories are consistently Given/When/Then, specific, and testable, and error paths are well covered (existing-destination refusal, LUKS2-header refusal, missing-confirmation refusal, oversized-request refusal, last-keyslot lockout, unenrolled-key targeting, mount-failure rollback). No vague criteria ("user can login"-style) were found. No database/entity-creation-timing checklist item applies — this project has no database; per-key metadata is stored as LUKS2 token JSON fields (AD-2), created only when `create`/`enroll` runs, so the equivalent "create only when first needed" principle is already satisfied.

### 🔴 Critical Violations

None found.

### 🟠 Major Issues

**1. Story 1.7 (Unified CLI Dispatch & Plain-Language Errors) is sequenced after stories that already presuppose it.**
- Evidence: Story 1.4's AC reads "When I run the create command in file mode," and Story 1.6's AC reads "When I run the unlock command" — both phrased as already-wired CLI subcommands — yet Story 1.7, which establishes "`create`...and `unlock` are both available as first-class subcommands of one binary" and the plain-language error-translation boundary, comes after them in the sequence (1.4, 1.5, 1.6, then 1.7).
- Impact: As written, a reader implementing strictly in story order would hit Stories 1.4–1.6 before the CLI subcommands they exercise are formally established, and before the plain-language translation boundary (the epic's own zero-FIDO2-knowledge promise, CAP-5/FR5) exists. This is a soft forward dependency — not a hard blocker, since implementers will naturally wire minimal CLI plumbing incrementally, but the story sequence as documented doesn't reflect that.
- Recommendation: Either resequence Story 1.7 to be the second story in Epic 1 (right after 1.3's preflight, before the create/unlock domain stories build on it), or add an explicit note to Stories 1.4–1.6 clarifying that each story wires its own minimal subcommand plumbing and that 1.7 is strictly an additive polish/consistency pass (the `--help` completeness check and a cross-command error-translation audit) rather than a prerequisite.

**2. Story 3.4 (Release Automation) has no relationship to Epic 3's stated goal and is inconsistently placed relative to the project's other DevOps items.**
- Evidence: Epic 3's goal is "close an unlocked tomb... grow an existing tomb's capacity... unlock a tomb read-only" — end-user tomb-lifecycle capabilities. Story 3.4 ("As a maintainer, I want versioned changelog generation and cross-platform release binaries published automatically") serves project maintainers, not tomb users, and shares no functional dependency with 3.1–3.3. Meanwhile, epics.md's own "Tooling / DevOps" grouping (AR-Dev1–4) lists devShell (AR-Dev1), CI (AR-Dev2), release automation (AR-Dev3), and packaging scope-fence (AR-Dev4) together as one logical unit — yet AR-Dev1/AR-Dev2 were placed in Epic 1 (as Stories 1.1/1.2) while AR-Dev3 was placed in Epic 3 (as Story 3.4), splitting one coherent DevOps concern across two unrelated feature epics.
- Impact: Epic 3's title/goal and its story list diverge, and the DevOps/tooling work is scattered rather than grouped, weakening epic-to-goal traceability for a reviewer or new contributor scanning epic summaries.
- Recommendation: Either (a) move Story 3.4 next to Stories 1.1/1.2 in Epic 1 (all three are DevOps/tooling setup, and release automation has no functional coupling to grow/close/read-only-unlock), or (b) carve out a small dedicated "Release & Distribution" epic for AR-Dev1–4 collectively, or (c) if kept in Epic 3, retitle the epic (e.g. "Tomb Lifecycle, Advanced Access & Release Readiness") so the story genuinely matches the stated scope.

### 🟡 Minor Concerns

**1. Stories 1.1 (Nix DevShell) and 1.2 (CI Suite) are contributor-facing, not end-user-facing.**
This is expected and sanctioned for a greenfield project per this step's own guidance (initial setup + dev environment + CI/CD early are listed as *expected* greenfield indicators, not violations) — flagged only as a deliberate pattern to confirm, not a defect.

**2. FR7 raw-device-parity AC gap (carried forward from Step 3).**
Stories 2.1 (enroll), 2.2 (revoke), 3.1 (close), and 3.3 (read-only unlock) have no acceptance criterion explicitly exercising a raw-device target, unlike 1.5/1.6/3.2. Likely rendered moot by AD-12's path-type-agnostic mapping-name design, but no AC makes that explicit for those four stories.

### Best Practices Compliance Checklist

| Check | Epic 1 | Epic 2 | Epic 3 |
| --- | --- | --- | --- |
| Delivers user value | ✓ | ✓ | ✓ (except Story 3.4, see Major #2) |
| Functions independently (no forward epic dependency) | ✓ | ✓ | ✓ |
| Stories appropriately sized | ✓ | ✓ | ✓ |
| No forward dependencies | ⚠️ (see Major #1) | ✓ | ✓ |
| Entity/metadata creation only when needed | ✓ (N/A — no DB, LUKS2 token fields) | ✓ | ✓ |
| Clear acceptance criteria | ✓ | ✓ | ✓ |
| Traceability to FRs maintained | ✓ | ✓ | ✓ (Story 3.4 traces to AR-Dev3, not an FR — expected, it's tooling not a capability) |

## Summary and Recommendations

### Overall Readiness Status

**READY** — with 2 non-blocking Major refinements recommended before or during Epic 1/3 implementation.

This is an unusually tight planning set for a small project: SPEC.md's 11 capabilities are 100% traceable through epics.md into 13 stories with specific, testable Given/When/Then acceptance criteria; error paths and edge cases (last-keyslot lockout, destination/header collisions, capacity bounds, mount-failure rollback) are consistently specified rather than left implicit; and the SPEC → epics → architecture chain for the one "UX-shaped" requirement (zero-FIDO2-knowledge CLI language) is fully closed with no document-format mismatch (PRD replaced deliberately by SPEC.md, UX correctly omitted for a CLI-only tool).

### Critical Issues Requiring Immediate Action

None. No epic is a disguised technical milestone, no epic has a hard forward dependency on a later epic, and no FR is entirely uncovered.

### Major Issues (address before/during implementation, not blocking to start)

1. **Story sequencing in Epic 1:** Story 1.7 (Unified CLI Dispatch & Plain-Language Errors) is listed after Stories 1.4–1.6, which already phrase their ACs as if the CLI subcommands they exercise are already wired ("When I run the create command..."). Resequence 1.7 earlier or add a note clarifying each story wires its own minimal CLI plumbing incrementally and 1.7 is a final polish/consistency pass.
2. **Epic 3 goal/story mismatch:** Story 3.4 (Release Automation) serves maintainers, not tomb users, and has no functional relationship to Epic 3's stated goal (close/resize/read-only unlock). It also splits the "Tooling/DevOps" concern (AR-Dev1–4) across two unrelated epics (1 and 3). Recommend relocating it next to Stories 1.1/1.2, spinning out a small "Release & Distribution" epic, or retitling Epic 3 to explicitly include release scope.

### Minor Concerns (optional polish)

1. FR7's "any operation works identically against raw device or file" claim has explicit AC evidence for create/unlock/resize (Stories 1.5, 1.6, 3.2) but not for enroll/revoke/close/read-only-unlock (Stories 2.1, 2.2, 3.1, 3.3). Likely moot given AD-12's path-agnostic design, but no AC states this explicitly for those four stories.
2. Stories 1.1/1.2 are contributor-facing rather than end-user-facing — expected and sanctioned for a greenfield project, flagged only to confirm it's deliberate.

### Recommended Next Steps

1. Resequence or annotate Story 1.7 relative to Stories 1.4–1.6 (Major #1) — quick edit to epics.md, no new content needed.
2. Decide where Story 3.4 (Release Automation) belongs — Epic 1, its own epic, or a retitled Epic 3 — and move/retitle accordingly (Major #2).
3. Optionally add one raw-device-path AC to Stories 2.1, 2.2, 3.1, and 3.3, or add a single note in epics.md explaining why AD-12 makes this unnecessary to test per-story (Minor #1).
4. No SPEC, Architecture, or PRD/UX-equivalent content changes are needed — proceed to implementation once (or in parallel with) the above are resolved.

### Final Note

This assessment identified 4 issues (0 Critical, 2 Major, 2 Minor) across epic-sequencing and epic-scope-boundary categories — no gaps in requirements coverage, document completeness, or UX/architecture alignment were found. The project is implementation-ready; the Major items are quick epics.md edits, not planning rework, and can reasonably be fixed in the next few minutes or addressed opportunistically during Epic 1/3 development without blocking Story 1.1 kickoff.

## Resolution Log (2026-07-22, same-day follow-up)

All 3 addressable findings were applied directly to `epics.md` at the user's request:

1. **Major #1 (Story 1.7 sequencing) — fixed via annotation, not resequencing.** Reordering Story 1.7 earlier would have created the mirror-image problem (its plain-language error-translation ACs reference domain errors — path-exists refusal, LUKS2-header refusal, missing wipe confirmation — that don't exist until the create/unlock stories run). Instead, kept it last (renumbered 1.8 after Major #2's insertion) and added a "Sequencing note" callout clarifying Stories 1.5–1.7 each wire and expose their own subcommand incrementally and aren't blocked by it; 1.8 is explicitly the consolidation/audit pass.
2. **Major #2 (Story 3.4 epic placement) — fixed by relocation.** Moved Release Automation from Epic 3 into Epic 1 as new Story 1.3, alongside Stories 1.1 (Nix DevShell) and 1.2 (CI) — reuniting the AR-Dev1–4 "Tooling/DevOps" grouping that epics.md's own Additional Requirements section already treats as one unit. Epic 1 and Epic 3 description text updated accordingly (Epic 1 now mentions release automation in its foundation-setup sentence; Epic 3's "finalizes release packaging..." clause removed since it no longer contains that story). Downstream renumbering: old 1.3→1.4, 1.4→1.5, 1.5→1.6, 1.6→1.7, 1.7→1.8. Epic 3's remaining stories (Close, Resize, Read-only unlock) keep their original numbers (3.1–3.3).
3. **Minor #1 (FR7 raw-device parity ACs) — fixed by addition.** Added one explicit Given/When/Then AC to each of Stories 2.1 (Enroll), 2.2 (Revoke), 3.1 (Close), and 3.3 (Read-only unlock), asserting the identical command works unmodified against a raw device/partition target, matching the phrasing already used in Story 1.7 (Unlock)/3.2 (Resize). FR7 now has explicit AC-level evidence across all 6 operations, not just 3 of 6.

`epics.md` still totals 13 stories across 3 epics; no FR/NFR/Capability content changed, only story placement, numbering, and the additive ACs/notes above. Story-number references elsewhere in this report (Steps 3 and 5 above) reflect the **pre-fix** numbering and are left as-is since they document the findings as originally observed — see this Resolution Log for the current, corrected numbering.
