# Rubric Review — ARCHITECTURE-SPINE.md (tomb-fido2, CAP-8..11 update)

**Reviewed:** `architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md` against `specs/spec-tomb-fido2/SPEC.md`
**Method:** good-spine checklist walk (6 items), evidence-based, no rewrite proposed — findings only.

## Verdict

The spine is well-constructed for the CAP-1..7 core (AD-1..7 remain sharp and enforceable), but the CAP-8..11 extension leaves at least two genuine divergence points completely undecided (mapping/mount-point discovery for `close`/`resize`, and backing-file/raw-device sizing ownership for `create`/`resize`), and rests one load-bearing mechanism (AD-2's shared-token-JSON metadata trick) on an unverified technical assumption. These are exactly the kind of gaps that let two independently-built units diverge incompatibly, so this should not ship as `final` without addressing them.

---

## 1. Fixes the real divergence points for the level below (misses none)

**Mostly yes, with two significant misses.**

- CAP-1..7's original divergence points (native-tool-only, side-channel-state, secret hygiene, preflight-first, last-keyslot guard, scope fences, test strategy) are all still fixed by AD-1..7 and remain sound.
- CAP-8..11's new divergence points are *mostly* fixed: bootstrap-keyslot lifecycle (AD-9), FilesystemBackend port shape (AD-8), resize ordering/grow-only (AD-10), atomic read-only propagation (AD-11) are all genuine, well-scoped fixed points.
- **Miss A — mapping/mount-point discovery is undecided (see Finding 1).** AD-2 mandates "no volume registry — every invocation takes an explicit device/file path argument." CAP-9 (`close`) and CAP-11/CAP-1 (`unlock`) all need, at close time, to go from "a device/file path" to "the currently active dm-crypt mapper name" and "its mount point" — with no registry to consult. Nothing in the spine decides *how* (deterministic naming convention derived from the path? live OS query via `/proc/mounts`/`dmsetup`/`cryptsetup status`?). The `LuksBackend` port's method list (`format/open/close/resize/add_key/remove_key/list_fido2_keyslots`) has no "find active mapping for this device path" primitive, and `FilesystemBackend` has no matching lookup either. Two units could each build a *self-consistent* but *mutually incompatible* answer (one hashes the canonical path into a fixed mapper name; another expects the CLI to also take a mapper-name/mount-point argument at `close` time) and neither would be wrong per the spine as written.
- **Miss B — who grows/creates the underlying container is undecided (see Finding 2).** `create` (CAP-8) needs a backing file or raw device already the right size before `luksFormat`; `resize` (CAP-10, AD-10) needs *more* raw space to exist before `LuksBackend` can grow the mapping into it. Neither the ports nor any AD assigns ownership of "make the backing file bigger" (truncate/fallocate) vs. "the user must already have grown the raw partition/file themselves." AD-10 opens mid-stream ("On a valid grow request, `LuksBackend` resizes the LUKS2 mapping first…") as if the extra space already exists, without saying where it came from or whose job it is to produce it.

## 2. Every AD's Rule is enforceable and actually prevents its stated divergence

AD-1, AD-4, AD-5, AD-6, AD-7, AD-8, AD-9, AD-11 all read as enforceable via code review / fake-backend call-order tests and each correctly closes the gap they name. Two exceptions:

- **AD-2 (Finding 3):** the rule "extra namespaced fields… added onto the same `systemd-fido2` token object" is stated as a fact, not as a verified capability. If `cryptsetup`'s LUKS2 token-plugin JSON-schema validation for `systemd-fido2` tokens is strict about unknown/extra keys (plausible, since systemd's own JSON validators are often exact-schema), this entire mechanism — and by extension AD-10's read of `tomb_fido2_filesystem` at resize time — silently fails at the first real enrollment. The Rule is not falsifiable from the spine alone and there's no fallback stated (e.g., a second, tomb-fido2-owned LUKS2 token slot of the *same* `systemd-fido2` type-string but a distinct token index) if the shared-object approach turns out to be rejected by cryptsetup's validator.
- **AD-3 (minor):** the general "inherited/passthrough stdio, never captures or pipes" rule and AD-9's bootstrap-passphrase exception are individually coherent, but the spine never states *how* the transient passphrase in AD-9 actually reaches `cryptsetup luksFormat`/`luksOpen` (presumably via a piped `--key-file=/dev/stdin`, which the letter of AD-3's general rule reads as forbidding). Not contradictory once you know AD-9 is an explicit exception, but worth one sentence tying the two together so a future implementer doesn't read AD-3 in isolation and conclude the bootstrap passphrase must also use inherited stdio (which would defeat AD-9's automation).

## 3. Nothing under Deferred could let two units diverge on correctness/security

- Distro packaging, PIN-UX wording, ext4-only — all fine, no correctness/security exposure.
- **Concurrent invocations (Finding 4):** deferred as "low-likelihood… revisit if multi-operator," but this isn't just a feature gap — it directly threatens AD-5's already-claimed guarantee. AD-5's last-keyslot guard reads "live header state… immediately before the removal decision," which is a classic TOCTOU pattern: a second concurrent `revoke`/`enroll` invocation between the read and the mutating call can make the guard's count stale, defeating the very lockout protection AD-5 exists to provide. This deferral should at minimum be cross-referenced from AD-5 as a known limitation of its safety argument, not filed as an unrelated future nice-to-have.

## 4. Stack table plausibility

Everything is either a specific version marked "verified locally" (cryptsetup 2.8.6, systemd 261, libfido2 1.17.0, Rust 1.90.0) or has an explicit reason for not being pinned (e2fsprogs — "preflight checks presence, not a specific version"). One exception:

- **Finding 5 (low):** `release-please | current` is the one row with no concrete version, inconsistent with every other row's pinned-version discipline. Should read like `cargo-dist`'s `~0.32.x` treatment, or state explicitly why it's intentionally unpinned.

## 5. Coverage of SPEC capabilities/constraints

All eleven CAPs are bound and appear in the Capability → Architecture Map. Constraints are mapped except one:

- **Finding 6 (medium):** "Break-glass recoverability" (SPEC Constraints, bullet 3 — README must document manual unlock via bare `cryptsetup`/`mount`/`fido2-token`) has no row in the Capability → Architecture Map table, unlike the other four constraints which are each explicitly tied to an AD. It's *substantively* supported by AD-1 (native-tool-only) + AD-2 (no side-channel state, same token type) working together, and the Structural Seed lists a README with a "break-glass bare-cryptsetup procedure" — so this is a documentation-completeness gap in the map table, not a design gap.

## 6. Structural dimensions owned by initiative altitude — decided / deferred / open question?

| Dimension | Status |
| --- | --- |
| Port boundaries (Luks/Fido2/Filesystem) | Decided (AD-8) |
| Metadata storage location | Decided (AD-2) — but see Finding 3 on its technical soundness |
| Bootstrap-secret lifecycle | Decided (AD-9) |
| Resize ordering / grow-only | Decided (AD-10) |
| Read-only atomicity | Decided (AD-11) |
| Last-keyslot safety ordering | Decided (AD-5) |
| **Mapping/mount-point discovery given "no registry"** | **Silently skipped — Finding 1** |
| **Backing-file/raw-device sizing ownership (create + resize)** | **Silently skipped — Finding 2** |
| Concurrency safety | Explicitly deferred (but see Finding 4 on its interaction with AD-5) |
| Filesystem types beyond ext4 | Explicitly deferred, additive design shown |

---

## Findings (ranked)

1. **[High]** No decided mechanism for going from "a device/file path" (AD-2's only permitted input) to "the active dm-crypt mapping + its mount point" for `close` (CAP-9) and any post-open operation — no registry, no naming convention, no port method for it. Two units will diverge incompatibly.
2. **[High]** No owner assigned for growing/creating the backing file or raw device capacity itself (create's initial sizing, resize's extra space) before the LUKS2/filesystem layers act on it — AD-10 assumes the space already exists.
3. **[High]** AD-2's core mechanism (extra custom fields on the shared `systemd-fido2` LUKS2 token object) is asserted, not verified against cryptsetup's actual token-JSON schema validation strictness; if wrong, AD-2 and AD-10's fs-type lookup both fail with no stated fallback.
4. **[Medium]** Concurrency is deferred as an isolated future concern, but it directly undermines AD-5's last-keyslot-guard TOCTOU safety claim; should be cross-referenced, not filed separately.
5. **[Medium]** "Break-glass recoverability" constraint isn't listed in the Capability → Architecture Map table (documentation completeness only — substantively covered by AD-1/AD-2).
6. **[Low]** Stack row `release-please | current` is vague relative to every other pinned-version row.
7. **[Low]** AD-3's general "inherited stdio, never piped" rule and AD-9's piped-bootstrap-passphrase exception aren't explicitly cross-referenced, risking a future misreading.
