---
name: 'tomb-fido2 — adversarial architecture review'
type: architecture-review
reviews: ../ARCHITECTURE-SPINE.md
created: '2026-07-22'
method: 'two-implementer divergence attack'
---

# Adversarial Review — ARCHITECTURE-SPINE.md (tomb-fido2)

## Method

For each Architectural Decision (AD), I constructed two engineers (or two AI agents) each implementing a *different* piece of the system (typically: one on `enroll`, one on `revoke`, or one on the token schema, one on the CLI dispatch), each reading only the spine and its own capability slice, each satisfying every applicable AD to the letter. I then asked: do their outputs actually interoperate? Every "no" below is a hole — a place the spine states a rule but not the *shape* or *ownership* needed to make two independent, letter-compliant implementations converge.

## Verdict

The spine's five ADs are individually sound but under-specify **three shared contracts** that every workflow depends on: the token JSON schema (including who creates it and via which tool), the definition of "valid keyslot" the last-keyslot guard counts, and the call-site ownership of preflight and of the two-step keyslot+token mutation. Each gap lets two fully-compliant implementations produce code that is individually correct and jointly incompatible or unsafe.

---

## Finding 1 — Token JSON schema has no owner, no field contract, and no fixed `type` (Severity: Critical)

**The two units:** Engineer A implements `enroll` by shelling out to `systemd-cryptenroll --fido2-device=auto <device>`. Engineer B implements `enroll` by driving `fido2-token` directly to mint a credential, then using `cryptsetup token import` to hand-roll a bespoke LUKS2 token JSON blob.

**Why both satisfy the letter:** AD-1 only requires that no crypto/FIDO2 library is linked and that only `cryptsetup`/`systemd-cryptenroll`/`fido2-token` are invoked — both A and B qualify, they just pick different tools from the allowed set. AD-2 only requires that per-key state live in "cryptsetup's token JSON metadata" — both do that too. The ER-diagram sketch (`label`, `credential_id`, `created_at`) is described as illustrative ("Token JSON metadata... schema below") but is not a JSON Schema: no field casing convention, no required `type` string, no `keyslots` array (which LUKS2's own token spec *mandates* on every token, and which the ER diagram omits entirely), no encoding for `credential_id` (base64 vs hex vs opaque systemd blob), no timestamp format.

**Where they diverge:**
- Engineer A's token, produced by `systemd-cryptenroll`, has `type: "systemd-fido2"` and systemd's own field set (`fido2-credential`, `fido2-salt`, `fido2-rp`, `fido2-client-pin-required`, …). There is no `label` field anywhere in that schema — systemd-cryptenroll has no such option — so CAP-5's "per-key label" has nowhere to live unless A does a *second*, separate `cryptsetup token import` to bolt a label onto systemd's token, which is a step the spine never describes.
- Engineer B's token has `type: "tomb-fido2"`, snake_case fields, `credential_id` as base64, `created_at` as ISO-8601.
- A's `unlock`/`revoke` code (written expecting to parse `type: "systemd-fido2"`) cannot read B's tokens, and vice versa. Worse: cryptsetup itself dispatches unlock behavior by `type` at the C-plugin level for `systemd-fido2` tokens (it can auto-unlock via its bundled plugin) but would silently ignore a `tomb-fido2`-typed token as "unknown," meaning A's device unlocks via cryptsetup's own machinery while B's requires tomb-fido2 to fully reimplement the unlock token-read/hmac-secret-request dance in `adapters::exec` — a fundamentally different `unlock` workflow shape, not just a data format difference.

**AD to tighten:** Pin AD-1/AD-2 (or add a new AD-6) to state explicitly: which tool creates the token (`systemd-cryptenroll` vs raw `cryptsetup token import`), the exact `type` string, and a versioned JSON Schema (field names, casing, encodings, and the mandatory `keyslots` array) that every workflow reads/writes.

---

## Finding 2 — "Valid keyslot" in the last-keyslot guard is undefined; the guard's own port has no keyslot-counting method (Severity: Critical)

**The two units:** Engineer A (implements `LuksBackend` for `revoke`) adds a new `list_keyslots()` method that parses `cryptsetup luksDump` and counts every enabled keyslot in the header (0–31), FIDO2-backed or plain-passphrase alike. Engineer B (implements `revoke` against the Structural Seed's trait list, which only lists `list_tokens` — not `list_keyslots`) satisfies AD-5's wording ("counts valid keyslots via `LuksBackend`") by counting `list_tokens().len()` instead, since that's the only enumeration method the seed actually specifies.

**Why both satisfy the letter:** AD-5 says only "counts valid keyslots via `LuksBackend`... aborts if count `<= 1`." It never says the count must come from keyslot enumeration rather than token enumeration, and the Structural Seed's trait signature (`open/close/add_key/remove_key/list_tokens`) doesn't even offer a keyslot-listing primitive — so B's reading is arguably *more* letter-compliant than A's, since A had to invent a method the seed doesn't list.

**Where they diverge, concretely:**
- Tokens and keyslots are not 1:1 in LUKS2: a keyslot can exist with no token (e.g., the original passphrase slot from `luksFormat`, before any FIDO2 enrollment), and a token can go stale (its `keyslots` array can point at a slot ID that a bare `cryptsetup luksKillSlot` — run outside tomb-fido2, which AD-2's break-glass design explicitly permits — has already removed).
- B's guard (`list_tokens().len() <= 1`) will **refuse** to revoke a FIDO2 credential down to one remaining token even when three untouched passphrase keyslots survive it (false-positive block — annoying but safe).
- The dangerous direction: if a user has 2 FIDO2 tokens but one token has gone stale (its keyslot was already killed via raw cryptsetup break-glass), B's count reads "2 valid" and permits removing the second — leaving **zero working keyslots**, a full lockout, while technically never violating "`<=1` aborts" because B counted tokens, not live keyslots.
- A's implementation, counting actual header keyslots, would not have this failure mode but silently treats a token-less legacy passphrase slot as "1 more valid slot," which changes whether a revoke of the *last FIDO2 key* is permitted (A permits it if a passphrase slot survives; a stricter reading of the tool's threat model — cold-storage, FIDO2-only — might want that blocked too).

**AD to tighten:** AD-5 must define "valid keyslot" precisely (does it mean "enabled in the LUKS2 header" or "has a live, tool-verifiable credential/device behind it") and must name the exact `LuksBackend` method it's counted from — adding that method to the Structural Seed's trait list, not leaving it to be inferred.

---

## Finding 3 — Interrupted revoke: no pinned order between `remove-key` and `token-remove`, so a crash produces two different, opposite failure modes (Severity: High)

**The two units:** Engineer A's `revoke` calls `remove-key` (kill the keyslot) first, then `token-remove` (delete the now-stale token JSON) second. Engineer B does the reverse: `token-remove` first, then `remove-key`.

**Why both satisfy the letter:** AD-5 only says the count check happens "before any mutating adapter call" and names both calls as a pair ("remove-key/token-remove") without ordering them. Both A and B run the count check first, then both mutating calls — fully compliant.

**Where they diverge:** If the process is killed, the device sleeps, or the second subprocess call fails between the two steps:
- **A's failure mode:** a token JSON entry survives referencing a keyslot ID that no longer exists — a "dangling token." Any later `list_tokens()`-based enumeration (which, per Finding 2, may be the *only* keyslot count Engineer B's guard uses) now overcounts valid keyslots, potentially green-lighting a subsequent revoke that produces an actual lockout.
- **B's failure mode:** the keyslot survives but its token metadata is gone — an "orphaned keyslot." Per AD-2, tomb-fido2's only bookkeeping surface is token JSON, so this keyslot becomes permanently invisible to every tomb-fido2 workflow (no label, unlistable, unrevokable through the tool) while still being a live, working unlock method nobody can see or account for — and it still consumes a "valid keyslot" count slot in whichever counting scheme (Finding 2) is used, further muddying future guard decisions.

Both are real, opposite-shaped bugs, and the spine gives no idempotent-recovery rule (e.g., "on next invocation, detect and reconcile dangling tokens / orphaned keyslots before proceeding") and no atomicity requirement.

**AD to tighten:** Add an AD specifying (a) the mandatory order of the two mutating calls in `revoke` (and the analogous two calls in `enroll` — see Finding 5), and (b) a reconciliation/self-heal step preflight (AD-4) must run to detect dangling tokens or orphaned keyslots before any workflow proceeds.

---

## Finding 4 — Preflight's call site is unpinned: "domain enforces its own invariant" vs "cli enforces it once" are both AD-4-compliant but give different safety guarantees (Severity: Medium)

**The two units:** Engineer A puts `preflight::run()?` as the first statement inside each of `domain::workflows::{unlock,enroll,revoke}` (defense-in-depth: any caller of the domain function, from any entry point, is protected). Engineer B puts a single `preflight::run()?` call in `cli/main.rs` before dispatch, and leaves the three workflow functions assuming it already ran.

**Why both satisfy the letter:** AD-4 says preflight "is invoked first by every workflow" and "no mutating call proceeds unless it passes" — both readings deliver that outcome as observed from the CLI. Nothing in the spine states which layer is *responsible* for the call, only that it happens before mutation.

**Where they diverge:** In the hexagonal paradigm the spine itself declares, domain is supposed to be safely callable independent of any particular driving adapter. Under B's design, calling `domain::workflows::revoke()` directly — from a test harness, a future second front-end, or from another workflow that internally reuses `unlock` logic — silently skips the dependency/capability gate, because the guarantee lives one layer up in `cli`. Under A's design it's actually enforced at the boundary the spine cares about (domain, not cli). Two engineers each building one workflow under B's assumption, then integrated with a caller who assumed A's, get an ungated mutating path with nobody having written the gate at all.

**AD to tighten:** AD-4 should explicitly assign the call site to `domain` (each workflow function calls it internally, not cli), consistent with the hexagonal principle that domain enforces its own invariants regardless of caller.

---

## Finding 5 — AD-3 (passthrough stdio) and AD-2 (must capture credential_id into token JSON) collide inside enrollment, and the two ways to resolve the collision produce different port shapes (Severity: Medium)

**The two units:** Engineer A runs FIDO2 credential creation + LUKS keyslot add as a **single** subprocess call with fully inherited/passthrough stdio (satisfying AD-3 for that call, since PIN entry and possibly existing-passphrase entry happen inside it) and, because passthrough means no stdout is captured, does a **second, separate, non-secret** call afterward (`fido2-token -L` / `cryptsetup luksDump`) to reconstruct the credential_id for the token JSON. Engineer B instead splits enrollment into an explicit first call that creates the FIDO2 credential with **captured** stdout to get the credential_id programmatically (still passing the PIN through some side channel), followed by a separate cryptsetup call to add the keyslot and write the token.

**Why both satisfy the letter:** AD-3 only constrains "any subprocess invocation that may involve secret entry" to passthrough stdio and forbids capturing secret-adjacent output *in that call*. Neither design captures secrets in the same call as a PIN/passphrase prompt — both comply.

**Where they diverge:** A's `Fido2Backend`/`LuksBackend` need a "read back what was just enrolled" query method that must run after the fact and correctly correlate "the token that was just created" (race-prone if two enrollments could ever be in flight, and the spine's own Deferred section admits concurrent invocations are unhandled). B's ports need a "create-credential-with-captured-id" method that must still isolate the PIN entry from the captured stdout stream within the same logical operation — a materially different subprocess/pty handling strategy. The two `Fido2Backend` implementations are not interchangeable, and neither is dictated or ruled out by the spine.

**AD to tighten:** AD-3 (or a new AD) should specify the concrete enrollment call sequence — which call captures what, and how credential_id is obtained without ever needing to demux a single stream that mixes secret passthrough and captured structured output.

---

## Summary Table

| # | Severity | Clash |
|---|----------|-------|
| 1 | Critical | Token JSON schema/type/owner-tool unspecified — enroll via systemd-cryptenroll vs raw `cryptsetup token import` produce non-interoperable, differently-typed tokens |
| 2 | Critical | "Valid keyslot" undefined + `LuksBackend` has no keyslot-counting method — token-count vs keyslot-count guards diverge on exactly the case AD-5 exists to prevent |
| 3 | High | No pinned order for `remove-key`/`token-remove` (or the analogous enroll pair) — interruption produces dangling tokens (A) or invisible orphaned keyslots (B), both corrupting future guard counts |
| 4 | Medium | Preflight call-site unpinned (domain-internal vs cli-only) — changes whether the gate holds when domain is called other than through cli |
| 5 | Medium | AD-3 passthrough vs AD-2 capture-for-token-JSON collide in enroll; two resolutions imply two incompatible port method shapes |
