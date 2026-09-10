# Sprint Change Proposal — 2026-09-10 (Story 7.3 Withdrawal)

**Trigger:** Story 7.3 (Touchless Enrollment & Flag Precedence, NO-UP mode) spec review, before dev-story started.
**Mode:** Batch
**Prepared by:** John (PM agent) via `bmad-correct-course`

## 1. Issue Summary

Story 7.3 was drafted (branch `story/7.3-touchless-enrollment-flag-precedence-no-up-mode`, never merged, no code written) to add a `--user-presence=false` flag delivering a fully touch-free "NO-UP" FIDO2 unlocking mode.

**Root cause:** Amelia's research into systemd's actual source (`src/shared/libfido2-util.c`, `src/cryptenroll/cryptenroll-fido2.c`) found that the CTAP2.1 hmac-secret extension — which systemd-cryptenroll requires for every enrollment — structurally prohibits disabling user-presence at GetAssertion time (the FIDO2 spec itself: `up=false` → `CTAP2_ERR_UNSUPPORTED_OPTION`). systemd-cryptenroll doesn't surface this as an error: its GetAssertion retry loop silently re-enables `up`, enrollment succeeds, and only the token's own `fido2-up-required` metadata field reveals the request was downgraded. This is not a hardware gap that might close later — any token capable of enrolling with this tool at all (i.e. hmac-secret-capable) is, by spec, incapable of honoring NO-UP.

**Evidence:**
- systemd source research, corroborated by a real user report (`systemd/systemd#23632`), captured in Story 7.3's own Dev Notes.
- Live confirmation by LeReverandNox against his own Yubikeys: UP cannot be disabled.
- FIDO2/CTAP2.1 spec text (fidoalliance.org): hmac-secret extension is documented as incompatible with `up=false`.

**Decision:** LeReverandNox (PM: John) decided to withdraw the flag entirely rather than ship it with a runtime "didn't actually work" notice on effectively every real invocation — shipping it would mislead users into choosing a mode that silently downgrades to one they didn't choose, which fails this project's own zero-fallback/zero-cognitive-overhead design posture (NFR3/NFR5). Consistent with the project's prior declined-feature reasoning (e.g. crates.io publishing, declined 2026-08-06 for not solving the real problem).

## 2. Impact Analysis

**Epic Impact:** Epic 7 only. Reduced from four planned unlocking modes (UV, PIN+UP, UP, NO-UP) to three (UV, PIN+UP, UP). Stories 7.1 (done) and 7.2 (done, unrelated bug fix) are unaffected. Story 7.4 (UV Capability Detection) is unaffected. Story 7.5 (Interactive Unlocking-Mode Menu) loses its fourth menu row and its NFR22-warning trigger narrows to UP-only. No other epic references NO-UP/`--user-presence`. No new epic needed — pure scope reduction, no replacement work.

**Story Impact:** Story 7.3 withdrawn (not renumbered — 7.1/7.2's own Dev Notes and `sprint-status.yaml` keys already reference "Story 7.3" by number; renumbering would create the exact stale-cross-reference confusion Story 7.2's own notes already warn about). Story 7.5's ACs need editing (see §4).

**Artifact Conflicts:**
- `epics.md` (this project's combined PRD+Epics doc — no separate PRD file exists): Epic 7 description, FR27/NFR22/NFR23, FR Coverage Map, Story 7.3 body, Story 7.5 ACs, and the two Epic-7 architecture candidate notes all reference the withdrawn flag.
- Architecture (`ARCHITECTURE-SPINE.md`): no change needed — `AD-22` (the three-flag precedence table) was never formally written, so there's nothing to un-write; AD-16's existing two-flag precedence already covers what remains.
- UX: none — CLI-only project, no UX design contract.
- Other: one shipped source file, `src/ports/fido2_backend.rs:47-48`, has a Story-7.1-authored doc comment forward-referencing `user_presence` as "Story 7.2's" (now-stale numbering AND a dead reference). `spec-tomb-fido2/SPEC.md:150` lists `--fido2-with-user-presence` among "deferred, not declined" flags — now inaccurate for this specific flag (it was tried and structurally can't work), though the other three flags in that same line are still genuinely just deferred. Both flagged in §4 as optional cleanup, not required for this proposal's core scope. Completed story docs (7.1, 7.2) are left untouched — they're point-in-time historical records, and 7.2's own Dev Notes already warn readers about the stale numbering.

**Technical Impact:** None — no shipped code implements Story 7.3. This is a planning-doc and tracking-doc change only.

## 3. Recommended Approach

**Selected: Option 1 (Direct Adjustment)** — edit `epics.md` and `sprint-status.yaml` to reflect the withdrawal; no rollback needed (nothing shipped), no MVP redefinition needed (Epic 7 is post-MVP polish, not part of Epics 1-4's core scope).

- Effort: Low (doc edits only).
- Risk: Low (no code/behavior change; Stories 7.4/7.5 can proceed once drafted, just with reduced surface).
- Rationale: subtractive-only change with zero code sunk cost; keeps Epic 7's remaining stories internally consistent without disturbing already-shipped Story 7.1/7.2 numbering or docs.

## 4. Detailed Change Proposals

### 4.1 `epics.md` — Epic 7 description (line 988)

**OLD:**
> Users can enroll a FIDO2 key using any of systemd-cryptenroll's remaining unlock-behavior flags — presence-only (no PIN/UV) via `--client-pin`, or the weakest touch-free mode via `--user-presence` — alongside the existing `--user-verification` flag, giving four selectable unlocking modes in total (UV, PIN+UP, UP, NO-UP). When enrolling without specifying any of the three FIDO2 flags, the tool presents an interactive menu so regular users can choose a mode without memorizing flags, and newcomers can discover modes they didn't know existed — defaulting to UV when the connected token supports it, annotating (not hiding) any mode it can't offer, and warning clearly wherever a weaker mode is chosen. This closes out the "expose the full FIDO2 flag set" item previously carried as a non-goal/backlog item. No new port or architectural layer — slots onto Epic 4's existing FIDO2 enrollment surface (`Fido2Backend::enroll_fido2_key`, `fido2_verification_args`).

**NEW:**
> Users can enroll a FIDO2 key using systemd-cryptenroll's presence-only unlock-behavior flag — `--client-pin` — alongside the existing `--user-verification` flag, giving three selectable unlocking modes in total (UV, PIN+UP, UP). A fourth mode, fully touch-free NO-UP unlock via `--user-presence`, was investigated and withdrawn (Story 7.3, Sprint Change Proposal 2026-09-10): the CTAP2 hmac-secret extension systemd-cryptenroll requires structurally prohibits disabling user-presence at GetAssertion time, so real FIDO2 hardware silently re-enables touch regardless of the flag — confirmed against real hardware. When enrolling without specifying either remaining FIDO2 flag, the tool presents an interactive menu so regular users can choose a mode without memorizing flags, and newcomers can discover modes they didn't know existed — defaulting to UV when the connected token supports it, annotating (not hiding) any mode it can't offer, and warning clearly wherever a weaker mode is chosen. This closes out the "expose the full FIDO2 flag set" item previously carried as a non-goal/backlog item, for the flags that are actually deliverable. No new port or architectural layer — slots onto Epic 4's existing FIDO2 enrollment surface (`Fido2Backend::enroll_fido2_key`, `fido2_verification_args`).

**Rationale:** Epic-level framing must stop promising a mode that can't exist on real hardware.

### 4.2 `epics.md` — Story 7.3 body (lines 1034-1060)

**OLD:** Full story text (As a user.../Acceptance Criteria, 5 ACs).

**NEW:**
> ### Story 7.3: ~~Touchless Enrollment & Flag Precedence (NO-UP mode)~~ — Withdrawn
>
> **Withdrawn 2026-09-10** (Sprint Change Proposal 2026-09-10). Investigation (Amelia, corroborated live by LeReverandNox against real Yubikey hardware) found the CTAP2 hmac-secret extension — required by systemd-cryptenroll for every enrollment — structurally prohibits disabling user-presence at GetAssertion time. systemd-cryptenroll doesn't error in this case; it silently re-enables touch and reports success, so `--user-presence=false`'s advertised "zero-interaction unlock" would not actually occur on real hardware. Shipping the flag would mislead users into choosing a mode that silently downgrades to one they didn't choose. No code was written for this story — only the story doc, on branch `story/7.3-touchless-enrollment-flag-precedence-no-up-mode` (never merged). FR27/CAP-27 withdrawn accordingly; see Requirements Inventory. Story numbering intentionally left unchanged (not renumbered) to avoid invalidating existing cross-references in Stories 7.1/7.2's own Dev Notes and `sprint-status.yaml`.

**Rationale:** Preserves the decision trail in place, rather than deleting it outright; avoids a renumbering cascade.

### 4.3 `epics.md` — Story 7.5 ACs (lines 1086-1117)

- Title line "So that I can pick without memorizing three separate flags..." → "...two separate flags..."
- AC1: "none of `--user-verification`, `--client-pin`, or `--user-presence` passed" → "neither `--user-verification` nor `--client-pin` passed"; menu list "UV, PIN+UP, UP, NO-UP" → "UV, PIN+UP, UP"
- AC4: "UP or NO-UP is selected... same security warning as Story 7.3/NFR22" → "UP is selected... same security warning as NFR22"
- AC5: "at least one of the three flags" → "at least one of the two flags"
- Append a closing note: "**Note (withdrawn scope):** NO-UP was originally a fourth menu row; dropped alongside Story 7.3's withdrawal (Sprint Change Proposal 2026-09-10) — see Requirements Inventory, FR27/CAP-27."

**Rationale:** Story 7.5 hasn't been drafted into a full story file yet — cheapest possible point to fix its epics.md source ACs before a story doc inherits the stale four-mode framing.

### 4.4 `epics.md` — Requirements Inventory

- **FR27** (line 47): mark withdrawn in place — `FR27: ~~User can enroll a FIDO2 key with the presence check itself disabled via `--user-presence=false`~~ — **Withdrawn 2026-09-10** (CAP-27; see Story 7.3 withdrawal, Sprint Change Proposal 2026-09-10 — FIDO2 spec structurally prohibits this on real hardware).`
- **FR28** (line 48): "none of the three FIDO2 flags (`--user-verification`, `--client-pin`, `--user-presence`)... UV / PIN+UP / UP / NO-UP" → "neither of the two FIDO2 flags (`--user-verification`, `--client-pin`)... UV / PIN+UP / UP"
- **NFR22** (line 73): "Enrolling in UP-only or NO-UP mode..." → "Enrolling in UP-only mode..."
- **NFR23** (line 74): mark withdrawn in place — folded back into AD-16's existing two-flag precedence, already shipped by Story 7.1; no separate precedence work needed.
- **Architecture candidate note** (line 109, the `client_pin`/`user_presence` precedence-table candidate): mark resolved/no-longer-a-candidate — `client_pin` already shipped under AD-16 as-is; the three-flag table is moot. Binds only CAP-26 now.
- **Architecture candidate note** (line 111, the interactive-menu candidate): "UV/PIN+UP/UP/NO-UP... none of the three FIDO2 flags" → "UV/PIN+UP/UP... neither of the two FIDO2 flags."
- **Backlog note** (lines 129-131, "one further item... now in scope via Epic 7"): reword to credit only FR26/FR28 as delivered scope, and note FR27 attempted-then-withdrawn.
- **FR Coverage Map** (lines 164-166): FR27 row → "~~Enroll a FIDO2 key with the presence check itself disabled~~ — Withdrawn 2026-09-10 (Story 7.3)"; FR28 row drops "/NO-UP".

**Rationale:** Requirements Inventory is this project's PRD-equivalent (no separate PRD file exists) — it's the canonical FR/NFR list and must not describe capabilities that no longer exist.

### 4.5 `sprint-status.yaml`

- Line 104: `7-3-touchless-enrollment-flag-precedence-no-up-mode: backlog` → `7-3-touchless-enrollment-flag-precedence-no-up-mode: withdrawn`
- New `action_items` entry (epic 7, owner `LeReverandNox`, status `done` — decision itself is resolved, only the git/gh cleanup remains and is owned separately, outside this workflow):
  > "Story 7.3 (Touchless Enrollment & Flag Precedence, NO-UP mode) withdrawn 2026-09-10 by LeReverandNox (PM: John) after Amelia's research into systemd's source confirmed the CTAP2 hmac-secret extension structurally prohibits disabling user-presence at GetAssertion time on real hardware — confirmed live against LeReverandNox's own Yubikeys. No code written (only the story doc, on an unmerged branch). Epic 7 scope reduced from four unlocking modes to three (UV, PIN+UP, UP); epics.md updated (Epic 7 description, FR27/NFR22/NFR23, Story 7.5 menu ACs, architecture candidate notes) via this Sprint Change Proposal. GitHub Issue #81 and branch `story/7.3-touchless-enrollment-flag-precedence-no-up-mode` cleanup owned directly by LeReverandNox, outside this workflow."

**Rationale:** `backlog` implies still-plannable; `withdrawn` is this file's accurate terminal state, and the action item preserves the decision trail per this project's own standing convention (every other Epic 7 finding above is recorded the same way).

### 4.6 Optional cleanup (outside this proposal's required scope — your call)

- `src/ports/fido2_backend.rs:47-48`: Story 7.1's doc comment says "`client_pin` is the first of two new tri-state flags Epic 7 adds... the second, `user_presence`, is Story 7.2's" — stale on two counts (renumbering, and now a dead reference entirely). One-line doc-comment fix, zero behavior change. I can apply it directly now, or hand it to Amelia as a trivial follow-up — your call.
- `_bmad-output/specs/spec-tomb-fido2/SPEC.md:150`: backlog note lists `--fido2-with-user-presence` alongside three still-genuinely-deferred flags as "deferred, not declined." Could add a one-line addendum noting this specific flag was tried and found structurally undeliverable, rather than merely deferred. Low priority — flagging so it doesn't silently drift.

## 5. Implementation Handoff

**Scope classification: Minor** — direct doc edits, no code, no backlog reorganization.

- **John (this workflow):** apply the `epics.md` and `sprint-status.yaml` edits in §4.1-4.5 directly, pending your approval below.
- **LeReverandNox:** close GitHub Issue #81 and delete branch `story/7.3-touchless-enrollment-flag-precedence-no-up-mode` (confirmed as your own action, outside this workflow).
- **Optional (§4.6):** your call whether John applies the `fido2_backend.rs` doc-comment fix now, hands it to Amelia, or it's skipped.

**Success criteria:** `epics.md` and `sprint-status.yaml` no longer describe or track NO-UP/`--user-presence` as live scope; Story 7.1/7.2's existing cross-references remain valid (no renumbering); Story 7.4/7.5 remain draftable as-is once reached.
