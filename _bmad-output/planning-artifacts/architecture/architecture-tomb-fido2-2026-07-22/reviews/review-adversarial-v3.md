# Reviewer Gate v3 — Adversarial divergence-hunt (rewritten AD-9/10/12/13)

**Verdict:** one real ambiguity survived literal compliance (`confirmed: bool` scope) plus one secondary gap (AD-12 hash/canonicalization unpinned); both fixed at Finalize. No evidence the new checks can be routed around AD-4 preflight or made untestable against AD-7's fake ports.

## Findings

1. **`confirmed: bool` scope ambiguous (AD-9) — most severe.** The original clause read as function-wide rather than scoped to device-backed create, letting two to-the-letter builds diverge: one gates `confirmed` on device-backed only, another makes it mandatory on every create call (including file-backed, which AD-9 says only needs a `path_exists` refuse, no wipe warning). Incompatible domain-function signatures, incompatible CLI wiring, fake-port unit tests that don't compile against both shapes.
   **Fix applied:** `CreateTarget` split into `File { path, size }` / `Device { path, size, confirmed }` enum variants — `confirmed` only exists in the type where it's required, closing the gap at the type level, not just in prose.

2. **AD-12's "stable hash" left canonicalization/algorithm unpinned.** The prefix constant was pinned but not the hash function or canonicalization rule (symlink resolution vs. lexical normalize) — two independently-written call sites (create's initial naming vs. close/resize/unlock's re-derivation) could each be "deterministic" yet diverge on edge-case paths, breaking `close`'s ability to find `create`'s mapping.
   **Fix applied:** AD-12 now mandates one shared `domain::mapping_name` helper (realpath canonicalization + hash), called identically by every workflow that needs the mapping name — added to Structural Seed as `domain/mapping_name.rs`.

3. Minor nitpick, not a real fork: AD-9's intro phrase ("before touching `adapters::exec`") vs. the gate itself calling `has_luks2_header`/`device_capacity` (which are adapter-backed port methods) reads slightly imprecise — the detailed bullets are unambiguous about ordering, so left as-is.

4. Device-backed nonexistent-path behavior (no explicit existence check before `has_luks2_header`) is unspecified but is a legitimately-deferred error-handling detail, not an architectural fork — not added as a Deferred item since it doesn't risk build incompatibility (an absent/unreadable device path fails the header-detection call itself with a clear I/O error either way).
