# Reviewer Gate v3 — Rubric walker (good-spine checklist)

**Verdict:** passes cleanly — thorough, internally consistent, every AD's rule concrete and checkable, all CAP-1..11 covered in both `binds` and the Capability→Architecture Map, matches the current SPEC.md. No critical or high findings.

## Findings (all low, addressed where cheap; none block implementation)

1. **Orphan schema field (`created_at`)** — the token-JSON ER diagram and AD-13 listed `created_at`/`credential_id` without stating which workflow writes/reads them.
   **Fix applied:** AD-2 now states `credential_id` is read non-secret at enroll/create for revoke-time key identification; `created_at` is stamped at enroll/create and shown at revoke-time listing, purely informational.

2. **AD-4 wording vs. AD-11 mechanism** — AD-4 listed "create, close, resize, and read-only unlock" as if four workflow functions, implicitly undercutting AD-11's point that read-only unlock is the *same* `unlock` function.
   **Fix applied:** AD-4 reworded to name three distinct functions (create/close/resize) plus unlock, explicitly noting the read-only variant is the same function per AD-11.

3. Not fixed (cosmetic, pre-existing pattern): AD-2's "Open item" (systemd-fido2 token-schema tolerance) sits under `status: final` — mitigated by the documented fallback, left as an explicit open item per existing project convention (mirrors how AD-9's partial-create gap is handled under Deferred).

4. Not fixed (out of scope for this pass): CI runner (e.g. GitHub Actions) is implied by cargo-dist/release-please but never named explicitly in Stack/AD-7 — low-value addition, deferred to a future pass if it becomes load-bearing.

Checklist items that passed without qualification: cross-references (AD-9↔AD-3/4/5/13, AD-10↔AD-9/AD-5, AD-12↔AD-13) line up; frontmatter (`binds`, `status`, `updated`) matches the body; operational envelope for a CLI tool is backed by explicit decisions (cargo-dist + release-please + GitHub Releases + Nix devShell, distro packaging explicitly deferred) rather than silence; Deferred section's items are all genuinely low-risk or structurally fenced off.
