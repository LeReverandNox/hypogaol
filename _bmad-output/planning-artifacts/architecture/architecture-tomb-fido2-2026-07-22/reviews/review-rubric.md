# Review — ARCHITECTURE-SPINE.md (tomb-fido2)

**Reviewed:** `_bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md`
**Against:** `_bmad-output/specs/spec-tomb-fido2/SPEC.md`
**Date:** 2026-07-22

## Verdict

The spine is well-scoped and its five ADs are individually sound, but it leaves the single mechanism its entire "no side-channel state" premise depends on (the token JSON write path) undecided, has one AD whose counting rule can silently violate a hard SPEC constraint, and is missing a decision on how the initiative verifies correctness — not a pass as-is.

---

## 1. Divergence points for the level below — mostly covered, one central gap

The spine correctly identifies and governs the big forks: never reimplement crypto (AD-1), never persist side-channel state (AD-2), never let secrets touch tomb-fido2's process (AD-3), one shared preflight (AD-4), last-keyslot guard in-domain (AD-5). All 7 capabilities are mapped to a governing AD or design element in the Capability → Architecture Map, and none are left unmapped.

**Gap — the token JSON write path is not actually decided.** AD-2's rule is "written into the LUKS2 header itself via cryptsetup's token JSON metadata," and the ER diagram shows a 3-field payload (`label`, `credential_id`, `created_at`). But:

- Real LUKS2 FIDO2 enrollment via `systemd-cryptenroll` creates a token of type `systemd-fido2`, whose JSON schema (`fido2-credential`, `fido2-rp`, etc.) is owned and parsed by systemd's own cryptsetup plugin, not by tomb-fido2. The spine never says whether tomb-fido2 (a) piggybacks custom fields onto that same `systemd-fido2` token object, (b) defines its own separate custom LUKS2 token type just to hold `label`/`created_at` alongside the `systemd-fido2` token, or (c) something else.
- The `LuksBackend` port in the Structural Seed only lists `open/close/add_key/remove_key/list_tokens` — there is no method for *writing* token metadata at all. It's unclear what `enroll.rs` is actually supposed to call to make AD-2 true.
- No schema-version field is present in the token payload, which matters given the product's own multi-year "bank safe" time horizon (SPEC's "Why") — a future tomb-fido2 change to the metadata shape has no migration hook.

This is exactly the kind of thing two independently-built units (enroll.rs authoring the token vs. revoke.rs/unlock.rs reading it, or a future contributor) could resolve incompatibly, and it sits at the foundation of AD-2. This is the spine's most important gap.

**Minor gap — FIDO2 device selection policy.** `ports::fido2_backend` is described only as "device discovery, capability probe." Nothing says what unlock/enroll do when zero or multiple FIDO2 devices are present (auto-pick the only one? error? interactively list and prompt?). Given CAP-5 requires plain-language guidance at "every interactive step," this is a real behavioral fork, not a cosmetic one, and it's currently silent.

## 2. AD Rule enforceability

| AD | Enforceable? | Notes |
|---|---|---|
| AD-1 | Yes | Checkable via Cargo.toml dependency audit / review. |
| AD-2 | Yes | Structurally clear (no sidecar file, explicit path arg). |
| AD-3 | Yes | Checkable in `adapters::exec` (stdio config per call). |
| AD-4 | **Weak** | See below. |
| AD-5 | **Ambiguous in practice** | See below. |

**AD-4 is stated but not structurally enforced.** The Rule says preflight is "invoked first by every workflow," but the Structural Seed puts `unlock.rs`, `enroll.rs`, and `revoke.rs` as three independent files with no shared entry point, wrapper, or type-level gate shown that would force the call. Nothing stops one workflow from forgetting to call preflight except code-review discipline — which is precisely the failure mode AD-4 exists to prevent for the *underlying tools*, now reintroduced one level up for the workflows themselves. Recommend naming the actual enforcement mechanism (e.g., a single `run_workflow(f: impl FnOnce(...))` wrapper in `cli/main.rs` that all three commands go through, or a marker type only obtainable after a passed preflight check).

**AD-5's counting rule doesn't specify what a "valid keyslot" is, and this can silently violate a hard SPEC constraint.** A real LUKS2 volume needs a non-FIDO2 (passphrase) keyslot to bootstrap before the first FIDO2 key can even be enrolled — the spine itself acknowledges this indirectly via AD-3's mention of "existing-passphrase authentication during enroll." SPEC's Constraints are explicit: "No fallback auth paths: FIDO2 is the exclusive unlock mechanism — no GPG or keyfile escape hatch." If AD-5's guard counts *all* keyslots (as literally written — "counts valid keyslots via `LuksBackend`"), a volume with one leftover bootstrap passphrase keyslot plus one FIDO2 keyslot would pass the `> 1` check when revoking the FIDO2 keyslot — leaving the passphrase as the sole remaining unlock method. That satisfies AD-5's literal rule while violating the SPEC's exclusivity constraint and defeating CAP-3's success criterion (which implicitly means "other enrolled FIDO2 keys still work," not "some other keyslot still works"). The spine needs to say explicitly: (a) the guard counts FIDO2-token keyslots specifically, not raw keyslot count, and (b) whether/when the bootstrap passphrase keyslot is removed to actually reach the "FIDO2-exclusive" end state the SPEC requires.

## 3. Deferred section — nothing that matters is silently left open

- Distro packaging beyond GitHub Releases: non-behavioral, fine to defer.
- FIDO2 PIN-required device UX: the security-relevant half (secret handling) is already governed by AD-3; only cosmetic prompt wording is deferred. Fine.
- Concurrent invocations against the same device: acceptable to defer for a single-user tool, though a one-line note on why this is safe (e.g., "cryptsetup takes an exclusive header lock") would remove reader doubt cheaply.

No item in Deferred lets two independently-built units diverge in a way that changes correctness or security — as long as the token-schema gap in §1 and the AD-5 ambiguity in §2 are actually closed (they are not currently deferred — they're just unaddressed, which is worse).

## 4. Named tech — verified against current versions (web-checked, 2026-07-22)

| Name | Spine version | Verified current | Verdict |
|---|---|---|---|
| Rust (rustc/cargo) | 1.90.0 | 1.97.1 (stable, released 2026-07-16) | **Stale** — ~7 releases / ~10 months behind, with no note that 1.90.0 is a deliberate MSRV pin rather than stale research. |
| clap | 4.6.2 | 4.6.2 | Current. |
| serde / serde_json | 1.0.228 | 1.0.228 | Current. |
| thiserror | 2.0.18 | 2.0.18 | Current. |
| anyhow | 1.0.103 | 1.0.103 (released 2026-06-25) | Current. |
| cargo-dist | ~0.32.x | 0.32.0 (released 2026-05-21) | Current. |
| release-please | "current" | 17.10.3 exists | **Fails the check outright** — no version is actually given, unlike every other row. "current" is not a verifiable pin. |
| cryptsetup | 2.8.6, verified locally | 2.8.6 stable | Current, and correctly marked as locally verified. |
| systemd | 261, verified locally | not independently re-checked | Marked as locally verified — acceptable per the same standard applied to cryptsetup. |

Two rows fail the "versions given, not stale-sounding" bar: Rust (stale) and release-please (no version given at all).

## 5. Spec coverage

All 7 capabilities (CAP-1..7) are explicitly mapped in the Capability → Architecture Map. Constraints are covered: standard-primitives-only → AD-1; memory hygiene → AD-3; last-keyslot → AD-5 (see caveat above); no sidecar/no backup-awareness → AD-2 and explicit non-scope; break-glass README → Structural Seed's README.md line; compiled-binary requirement → satisfied by choosing Rust; zero-FIDO2-knowledge UX → CAP-5/`cli::ux`; physical-presence-not-configurable → trivially satisfied by AD-1 (no custom logic layered on top of cryptsetup's native token mode). Non-goals aren't contradicted. Nothing from the SPEC is unaddressed.

## 6. Whole-initiative dimensions — one left silent

Per-dimension check for the initiative altitude:

- **Design paradigm / module boundaries:** decided (hexagonal, explicit).
- **Data & state:** decided (AD-2, token JSON — modulo the write-path gap above).
- **Error handling / logging:** decided (Consistency Conventions: typed enum, stderr-only, no telemetry).
- **Config:** decided (no config file, CLI-args-only).
- **Deployment & release packaging:** decided (cargo-dist + release-please, GitHub Releases; distro packaging explicitly deferred).
- **Platform/environment scope:** decided (Linux only; specific cryptsetup/systemd feature flags named).
- **Infra/provider strategy:** not applicable — the tool has no server/hosted component, so there is genuinely nothing to decide here. Worth one explicit line ("no infra: single local binary, no network calls") so a reader can tell this was considered and not merely forgotten, but not a real gap.
- **Operations / verification strategy:** **silent.** There is no decision, deferred item, or open question anywhere in the spine about how unlock/enroll/revoke get tested or verified before a release ships — no mention of CI, of testing against real (or loop-backed) LUKS2 volumes, or of how FIDO2 hardware interaction gets exercised without a physical key on every run (e.g., a virtual CTAP2 authenticator in CI). For a tool whose whole value proposition is being trustworthy under crisis stress on irreversible operations (revoke, last-keyslot removal), and which already commits to a release pipeline (cargo-dist/release-please) that presumably gates on *something*, this is a whole operational dimension left completely unaddressed. This should be decided, deferred with a stated reason, or raised as an explicit open question — right now it's just absent.

---

## Findings, ranked by severity

1. **(High)** Token JSON metadata write-path is undecided — no stated token type strategy (reuse `systemd-fido2` vs. custom type), no port method to write metadata, no schema-version field. This is the mechanism AD-2 depends on and it's the most likely source of real divergence between independently-built units.
2. **(High)** AD-5's last-keyslot guard doesn't specify "valid keyslot" = FIDO2-token keyslot specifically. As literally written it can be satisfied while a leftover bootstrap passphrase keyslot silently violates the SPEC's "FIDO2 exclusive, no fallback auth" constraint.
3. **(Medium)** Testing/CI/verification strategy for the whole initiative is entirely silent — not decided, not deferred, not flagged as an open question — despite the tool performing irreversible operations on real disk headers.
4. **(Medium)** AD-4 ("preflight invoked first by every workflow") has no structural enforcement mechanism named; it's convention across three independently-written files with nothing shown that actually guarantees it.
5. **(Low)** Two Stack entries fail "verified-current": Rust 1.90.0 is ~7 releases behind current stable (1.97.1) with no MSRV rationale given; release-please is listed as "current" with no actual version number, unlike every other row.

Minor/non-blocking: FIDO2 multi-device selection policy is unaddressed in `ports::fido2_backend`; infra/provider dimension would benefit from one explicit "not applicable" line rather than silence.
