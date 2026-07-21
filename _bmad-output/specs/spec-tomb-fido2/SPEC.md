---
id: SPEC-tomb-fido2
companions: []
sources: [../../brainstorming/brainstorm-tomb-fido2-2026-07-21/brainstorm-intent.md]
---

> **Canonical contract.** This SPEC and the files in `companions:` are the complete, preservation-validated contract for what to build, test, and validate. Source documents listed in frontmatter are for traceability only — consult them only if you need narrative rationale or prose color this contract intentionally omits.

# tomb-fido2

## Why

tomb-fido2 exists to serve one job: store rarely-accessed, highly sensitive material (a master GPG key, a password-manager backup) as reliably as a bank safe opened once a year in a crisis — without depending on any third party or external service. It is both a pain to solve (the multi-tool memorization burden of `cryptsetup` + `fido2-token` + `systemd-cryptenroll`, each with its own flags, is unusable under crisis stress) and a vision to realize (a FIDO2-native reimagining of dyne/tomb's core insight: a thin wrapper over standard, boring primitives that survives the tool's own death). TrueCrypt's abrupt 2014 shutdown is the cautionary reference — for this class of use case, the design bets on LUKS/dm-crypt and FIDO2 hmac-secret precisely because they are low-level, standard, and multi-vendor, never on anything only tomb-fido2 itself understands.

## Capabilities

- **CAP-1**
  - **intent:** User can unlock a LUKS2 volume using a FIDO2 security key via tomb-fido2.
  - **success:** A user with zero FIDO2 familiarity unlocks a LUKS2 volume on first try, guided only by tomb-fido2's own prompts, with no external documentation consulted.

- **CAP-2**
  - **intent:** User can enroll an additional FIDO2 key as an alternate unlock method on an already-unlocked volume, using LUKS2's native multi-keyslot support (up to 32 slots).
  - **success:** After enrollment, the volume unlocks successfully with either the original key or the newly enrolled key.

- **CAP-3**
  - **intent:** User can revoke a single FIDO2 key's keyslot, removing it as a valid unlock method, unless doing so would remove the last remaining valid keyslot.
  - **success:** After revocation, the removed key no longer unlocks the volume, while other enrolled keys still do. Attempting to revoke the last remaining valid keyslot is blocked with a clear explanation, and the volume remains unlockable.

- **CAP-4**
  - **intent:** User drives unlock, enrollment, revocation, and dependency checking through one unified CLI, replacing direct use of `cryptsetup`, `fido2-token`, and `systemd-cryptenroll`.
  - **success:** No workflow in v1 scope requires the user to invoke the three underlying tools directly.

- **CAP-5**
  - **intent:** Tool guides the user through every interactive step (e.g. the key-touch moment) in plain language, assuming zero FIDO2 knowledge.
  - **success:** Prompts and error messages never assume FIDO2 familiarity (e.g. "Please touch your security key button," never "Awaiting UP").

- **CAP-6**
  - **intent:** Tool verifies all hard dependencies (LUKS2 FIDO2/hmac-secret support, required binaries, kernel features) before any operation begins.
  - **success:** On a missing dependency, the tool exits cleanly before starting the operation, with a detailed, actionable error message — never a mid-operation failure.

- **CAP-7**
  - **intent:** User can run unlock, enrollment, and revocation directly against raw physical block devices/partitions, not only file-backed loop devices.
  - **success:** The same CLI commands work unmodified against a raw partition path and a loop-mounted file — a capability original Tomb never exposed.

## Constraints

- Standard, low-level, well-tested primitives only — LUKS/dm-crypt + FIDO2 hmac-secret. No proprietary formats, no single-vendor-maintained crypto.
- Break-glass recoverability: the README must document unlocking the volume manually using only bare `cryptsetup` + `fido2-token` commands, with zero dependency on the tomb-fido2 binary surviving.
- Zero-FIDO2-knowledge UX is mandatory across all prompts and errors, not just the happy path.
- Physical key presence at the exact moment of unlock is fixed, not configurable — the tool supports exactly what cryptsetup's FIDO2 token mode natively offers, no more.
- No fallback auth paths: FIDO2 is the exclusive unlock mechanism — no GPG or keyfile escape hatch.
- Must ship as a real compiled binary (Go or Rust), not a shell script.
- Tool stays entirely unaware of backup strategy: the README may recommend a 3-2-1-style backup of volumes/keys as a disclaimer, but no code or feature may touch backup concerns.
- Tool must actively prevent revoking the last remaining valid keyslot, a deliberate departure from raw cryptsetup (which permits this and allows a user to lock themselves out).
- Tool must make a best-effort attempt to avoid decrypted key material leaking or persisting in process memory; the specific mechanism (zeroing strategy, memory locking, etc.) is an architecture decision, not specified here.

## Non-goals

- Remote or delegated unlock beyond what cryptsetup's native FIDO2 token mode offers.
- Post-quantum-readiness features.

## Success signal

A user who has never touched tomb-fido2 before can, in a real crisis, unlock a LUKS2 volume (loop-file or raw partition) using only a physical FIDO2 key and tomb-fido2's own prompts — no external notes, no web search. Independently, with the tomb-fido2 binary assumed gone, the same volume can still be unlocked by following the README's break-glass procedure using only stock `cryptsetup` and `fido2-token`.

## Assumptions

- The Should-Have items from the source brainstorm (break-glass README procedure, 3-2-1 backup disclaimer) are folded into Constraints here rather than kept as separate capabilities, since they bend documentation/design decisions rather than describing testable tool behavior.

