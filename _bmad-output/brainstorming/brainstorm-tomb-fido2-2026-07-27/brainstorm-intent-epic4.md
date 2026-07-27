# tomb-fido2 — Epic 4 Candidate Features - Intent

Companion intent document for a `bmad-spec` **update** pass on the existing `SPEC.md` (`_bmad-output/specs/spec-tomb-fido2/SPEC.md`). Epics 1-3 (CAP-1..11) are complete and merged; this captures four new capabilities proposed after the Epic 3 retrospective (2026-07-27), plus one carried-over open item. None of this has been distilled into SPEC.md yet — that's the next session's job.

## Problem / Motivation

- **Volume/key inspection**: there's currently no way to see which FIDO2 keys are enrolled on a tomb, or basic technical info about it, without falling back to raw `cryptsetup luksDump`/`fido2-token` — which is exactly the multi-tool fallback CAP-4 exists to eliminate, just for a read-only inspection need instead of a mutating one.
- **FIDO2 user verification (fingerprint)**: `enroll`/`create` only ever use touch-based presence (UP). `systemd-cryptenroll` already supports `--fido2-with-user-verification=true`, and cryptsetup's token-based unlock re-authenticates however the token was enrolled — so a key enrolled with UV would require its own fingerprint/PIN verification, not just touch, without needing a new unlock mechanism. This isn't a new auth path, it's a stronger use of the existing one.
- **`close all` / `slam`**: today `close` operates on exactly one tomb at a time. dyne/tomb (the tool this project explicitly reimagines, per SPEC's own "Why") has both a bulk `close all` and an emergency `slam` that also force-clears whatever's blocking unmount. No equivalent exists here.
- **Bind-hooks / exec-hooks**: dyne/tomb also has a hooks mechanism for running user-defined logic around a tomb's lifecycle. No equivalent exists here. (Full mechanism detail below — fetched from the linked manpage, not from memory.)

## Must-Have (v1 candidate scope for Epic 4)

### 1. Volume/key info command
- Show the currently enrolled FIDO2 keys (with their label) for a given tomb.
- User raised this as an either/or, not yet decided: a dedicated "list keys" command, **or** a more general "show technical info about this tomb" command with enrolled keys as one section of it. This needs a decision (either from the user directly, or as an explicit `open_questions[]` entry for `bmad-spec` to surface).
- Note: this is a read-only inspection capability. It likely doesn't need the volume unlocked first — `list_fido2_keyslots`/token metadata are already read directly off the LUKS2 header by existing code (`enroll`/`revoke` do this today) without needing `luks.open`. Worth confirming that's still true for whatever this command's exact shape ends up being.

### 2. FIDO2 user-verification (fingerprint) support
- Add support for enrolling a key with `systemd-cryptenroll`'s `--fido2-with-user-verification=true`, available both:
  - at volume creation time (`create`'s bootstrap enrollment), and
  - via the standalone `enroll` command (additional keys).
- Once enrolled with UV, unlocking with that key should require the device's own fingerprint/PIN verification instead of a bare touch.
- **Open question, flagged for architecture, not decided here**: does `unlock`/`resize`'s existing token-based `open` call need any change to support a UV-enrolled key, or does cryptsetup's token machinery already handle it transparently based on how the credential itself was enrolled? This is exactly the class of external-tool-behavior assumption this codebase's own convention (established Epic 1, reinforced Epic 3's 3.2 spike) says to verify empirically before locking a design — don't assume either answer.

### 3. `close all` / `slam`
- `close all`: close every currently-open/unlocked tomb in one command (dyne/tomb precedent).
- `slam`: an emergency command that closes all open tombs **and** kills whatever process is holding a mount busy, so unmounting can't be blocked.
- **Open questions, not decided by the user yet, flagged for architecture/SPEC constraints:**
  - How does the tool discover "all currently open tombs"? There is no registry today — AD-2 deliberately keeps no side-channel state. Likely mechanism: enumerate active dm-crypt mappings (e.g. `dmsetup ls`) filtered by AD-12's deterministic mapping-name convention, but this needs a real architecture decision, not an assumption.
  - What exactly does "kill the process" mean in scope for `slam` — signal choice (SIGTERM then SIGKILL after a grace period? straight SIGKILL?), and how does it identify which process(es) to target (whatever holds the mountpoint busy, e.g. via `fuser`/`lsof`-equivalent)?
  - Does `slam`, being explicitly destructive to other processes' unsaved work, need its own confirmation gate — consistent with this codebase's existing pattern of confirmation prompts for `create`'s device wipe and `revoke`'s irreversible key removal, both examples of "the review layer added a confirmation gate even though no AC asked for one, because the action was irreversible and security/safety relevant" (Epic 2 retro)?

### 4. Bind-hooks / exec-hooks
dyne/tomb precedent, fetched from `https://dyne.org/docs/tomb/manpage/#hooks` (not from memory):

- **Bind-hooks**: a text file named `bind-hooks` in the tomb's root, a two-column list — first column a path relative to the tomb, second column a path relative to `$HOME`. On open, dyne/tomb bind-mounts (`mount -o bind`) each tomb-internal path onto the corresponding `$HOME`-relative path, making it directly accessible there. Example:
  ```
  mail          mail
  .gnupg        .gnupg
  .mozilla      .mozilla
  ```
- **Exec-hooks**: an executable file named `exec-hooks` in the tomb's root, run **as the invoking user** (not root) by tomb itself. First argument is the lifecycle step (`open` or `close`), second is the full mountpoint path; on `close`, tomb additionally appends the tomb name, loopback device, and dev-mapper device paths as further arguments.
- Disabling: dyne/tomb has a `-n` flag to skip processing both hooks entirely for a given invocation.
- User wants "something similar," explicitly not necessarily an exact port.
- **Open questions — this is the largest architectural departure of the four ideas, flagged for its own deliberate SPEC treatment, not silent inheritance of existing constraints:**
  - Execution identity: dyne/tomb runs exec-hooks as the invoking user, which lines up with this codebase's existing "most operations don't need root" posture (`ExecAdapter::privileged()`'s doc comment) — but tomb-fido2's own `close` already needs `sudo` for the unmount half; does the hook run before/after privilege is dropped, and does that even matter here given hooks run as a wholly separate subprocess?
  - Lifecycle points: dyne/tomb only has `open`/`close`. Does tomb-fido2 want the same two, or does its extra CAP-9/10/11 surface (close, resize, read-only unlock) suggest more granularity?
  - Config format/location and scope: per-tomb file in the tomb's own root (matching dyne/tomb exactly), or something else? Global vs. per-tomb?
  - Safety rails: since this is arbitrary user-authored code execution by design, does it need any guardrails beyond dyne/tomb's own model (e.g., must be a regular file, executable bit required, no implicit sourcing) — consistent with this project's "standard, boring, well-tested primitives" ethos (SPEC's own "Why")? dyne/tomb's hooks model predates a FIDO2-only, Rust-native, hexagonal-architecture design — worth treating as precedent to adapt, not a template to copy uncritically.

## Carried-Over Open Item (not new this session, folded in per user's own call)

- **Progress-reporting design question** — open since Story 1.5 (2026-07-23), explicitly deferred at every epic boundary since. No real step-by-step progress reporting exists today for long-running operations (`create`, `resize`); only minimal "Creating tomb.../Tomb created." stopgap messages. Resolve during this SPEC pass, or explicitly re-defer with a stated reason — a fourth silent deferral isn't a decision.

## Should-Have / Out of Scope

- Not addressed by the user this session — existing SPEC Non-goals (remote/delegated unlock beyond cryptsetup's native token mode, post-quantum-readiness, shrink) presumably still hold. `bmad-spec` should reconfirm rather than assume silently.

## Non-Negotiable Design Constraints likely affected

- CAP-4 ("no v1 workflow requires the user to invoke `cryptsetup`/`fido2-token`/`systemd-cryptenroll` directly") extends naturally to the info command and to `--fido2-with-user-verification`.
- "No fallback auth paths — FIDO2 is the exclusive unlock mechanism" is **not** violated by UV: it's a stronger verification mode of the same FIDO2 mechanism, not a new one.
- AD-2's "no side-channel state, everything live-queried" is the existing constraint `close all`/`slam` most directly bears on — either respect it (live-enumerate mappings) or explicitly amend it with a stated reason, don't quietly drift from it.
- Hooks introduce a genuinely new category of risk (arbitrary user-code execution) this SPEC has never had to reason about — needs its own explicit constraint statement, not silent inheritance of existing ones.

## Reference

- dyne/tomb hooks manpage: https://dyne.org/docs/tomb/manpage/#hooks (bind-hooks/exec-hooks mechanism detail above was fetched directly from this page)
