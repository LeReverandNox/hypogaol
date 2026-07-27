---
name: 'tomb-fido2 — adversarial architecture review (v4, Epic 4 / AD-14..19)'
type: architecture-review
reviews: ../ARCHITECTURE-SPINE.md
created: '2026-07-27'
method: 'two-implementer divergence attack, focused on AD-14 through AD-19 and their interaction with AD-4/AD-5/AD-8/AD-12'
---

# Adversarial Review v4 — ARCHITECTURE-SPINE.md, Epic 4 additions (AD-14..19)

## Method

For each of AD-14 through AD-19, and for each flagged interaction with an existing AD (AD-14×AD-8's close ordering, AD-17/18×AD-5's last-keyslot guard, AD-17/18×AD-12's mapping-name determinism), I constructed two independently-built units that each read only the spine (plus SPEC.md/hooks.md) and each satisfy every applicable Rule to the letter, then asked whether their outputs interoperate or produce a safe, single, predictable end-state. A "finding" below is recorded only where I could pin down a concrete, reproducible divergence or an unsafe shared outcome that a to-the-letter reading of the current text permits — not mere prose looseness.

## Verdict

Epic 4's six new ADs are individually well-formed, but three real, load-bearing gaps survive literal compliance, one of them severe: **AD-14's "bind-hooks are torn down implicitly by closing the parent mount" claim is not true of Linux bind-mount semantics**, and the spine never states what a compliant `close`/`close_all`/`slam` does when the final `LuksBackend::close` call consequently fails — two compliant builds resolve that either by honestly erroring (confusing, but safe) or by silently reporting success while the tomb is still cryptographically open (unsafe, and a direct CAP-9 violation). Compounding that, **AD-17/AD-18 never state whether one mapping's failure inside the close_all/slam loop is isolated or fatal to the whole batch** — which matters most exactly when it collides with the bind-hooks bug above, since that failure is not a "busy" condition AD-18's signal-escalation mechanism is built to solve (no process holds the mapper — a kernel mount-table entry does), so slam cannot force its way past it either. A third, narrower gap: **AD-18 doesn't scope what "retry" re-runs** (the failed `umount` alone, or the whole hooks+umount sequence), which matters because exec-hooks is arbitrary, not-guaranteed-idempotent user code. A fourth, lower-severity gap in AD-19 leaves room for a "helpful" progress payload to smuggle secret-adjacent state past AD-3's wipe discipline, though the enum as literally listed is safe by construction. AD-15 and AD-16 were checked hard against the prompted attack angles (info's query reuse; UV enrollment threading into the wrong bootstrap phase) and hold up — see the "Checked, no fork found" section.

---

## Finding 1 — AD-14's "bind-hooks need no separate teardown" is factually wrong for Linux bind mounts, and no AD says what happens when `LuksBackend::close` then fails (Severity: Critical)

**The claim in question (AD-14):** "bind-hooks are never separately un-bind-mounted, since unmounting their parent filesystem already tears them down."

**Why this is false, not just underspecified:** `mount --bind tomb-mail $HOME/mail` creates a second, independent vfsmount that shares the tomb filesystem's superblock but is mounted at a path outside the tomb's own mount tree. Unmounting the tomb's *primary* mountpoint (`FilesystemBackend::umount` in AD-8's close sequence) removes only that one vfsmount entry; it does not, and cannot, remove the bind-mount at `$HOME/mail`, which continues to hold an open reference to the same underlying block device (the dm-crypt mapper). Device-mapper refuses to remove a mapping that still has any live reference (`dmsetup remove` / `cryptsetup luksClose` returns "device busy"). So: whenever a tomb has bind-hooks configured, AD-8's own close sequence — hooks, then `umount` (succeeds, the primary mountpoint really does go away), then `LuksBackend::close` — hits a busy-device failure at the very last step, *every time*, and the bind-mounted directories at `$HOME` continue to expose the tomb's (supposedly closed) contents indefinitely.

**The two units, and how they diverge on the (unspecified) failure:**
- **Unit A** propagates `LuksBackend::close`'s error verbatim as the workflow's result. `close` returns `Err`, `cli::ux` surfaces a raw "device busy" style message the user has no way to act on (nothing in AD-14 tells them a leftover bind mount is the cause, and no AD assigns anyone the job of un-bind-mounting it). The tomb is left in a state where the primary mountpoint is gone but the LUKS mapping is still open and the data is still reachable via `$HOME/mail`.
- **Unit B** reads AD-8's ordering ("then `LuksBackend::close`") as not mandating that a failure there be fatal to the *workflow's reported outcome* — nothing in AD-8/AD-9's close-symmetry text says `close` must return `Err` if the final step errors — and treats a busy-device result as a soft/logged condition, returning `Ok` to the CLI. The user is told the tomb closed successfully (satisfying nothing the text explicitly forbids), while the mapping is still open and still unlockable by anyone with access to `$HOME/mail`, directly violating CAP-9's success criterion ("the volume requires a FIDO2 key to unlock again") without any signal to the user that it didn't.

Both A and B are letter-compliant with AD-8 and AD-14 exactly as written; neither is safe, and they disagree on which kind of unsafe (confusing-but-honest vs. silently-false-success).

**Why slam can't bail this out either:** AD-18's escalation mechanism (`processes_using`/`signal_process`, i.e. `fuser`/`kill`) targets *processes holding a mountpoint open*. A stale bind mount is not a process holding a file descriptor — it's a second kernel mount-table entry referencing the same superblock. `fuser -m` on the (already-unmounted) primary mountpoint finds nothing to signal, so slam's SIGTERM→SIGHUP→SIGKILL ladder runs to completion, finds no holder, and — per AD-18's own stop condition ("move to the next mapping once `umount` succeeds or `processes_using` returns empty") — treats the mapping as done, even though `LuksBackend::close` itself will still fail identically to Unit A/B above. Slam, whose entire "Prevents" clause is "hanging indefinitely on one stuck mount instead of clearing everything it can and moving on," has no mechanism at all for this specific busy-cause, because it's structural (an extra mount), not process-held.

**AD to tighten:** AD-14 must drop the "already torn down" claim and instead require the close sequence to unmount every entry it bind-mounted (in reverse order) before `LuksBackend::close` runs — the reverse of the exact `bind_mount` calls `unlock`'s hooks step made, using the same recorded source/dest pairs (not re-derived from the tomb-root `bind-hooks` file at close time, in case it changed between open and close). AD-8/CAP-9 should also pin `close`'s return value explicitly: any failure in the hooks/umount/close chain is a workflow-level `Err`, never silently swallowed into a reported success.

---

## Finding 2 — close_all/slam's per-mapping error handling is unspecified: one bad mapping can abort the whole batch, and this collides directly with Finding 1 (Severity: Critical)

**The two units:** Both implement `domain::workflows::close_all` as "AD-8/AD-14's exact single-close sequence... to each [discovered mapping], with no special-casing" (AD-17's own words). Given N discovered mappings and mapping #2 hitting *any* per-mapping error — the Finding 1 busy-close failure, an AD-14 exec-hooks hard-error from one tomb's stale/non-compliant hook file, or simply a benign race where mapping #2 was already closed by someone else a moment earlier — the spine gives no rule for how the loop treats that.
- **Unit A** reads "no special-casing" maximally: the loop is a plain iteration with `?`-propagation, so mapping #2's error aborts the whole `close_all`/`slam` call. Mappings #3..N — genuinely open, no problem of their own — are never even attempted, and the CLI reports total failure after having (silently) touched only mapping #1.
- **Unit B** reads "no special-casing" as "each mapping gets the identical sequence," which is orthogonal to whether the *loop* isolates failures — and adds its own (unspecified-but-not-forbidden) catch-log-continue wrapper per mapping, so mappings #3..N still get closed and only #2 is reported as failed.

Both are letter-compliant with AD-17 as written; they produce different observable outcomes for the *other, unrelated, healthy* tombs in the batch. This matters most for `slam`: AD-18's own "Prevents" clause explicitly promises "clearing everything it can and moving on" rather than getting stuck — but that promise is scoped by AD-18's text only to busy-mount/signal-escalation failures. It is silent on any other error class, including the routine bind-hooks-close failure from Finding 1, which — because every hooked tomb hits it — is not a rare edge case but the expected outcome for any multi-tomb slam where even one tomb uses bind-hooks. Under Unit A's reading, that one tomb silently defeats slam's entire purpose for every other open tomb too.

**AD to tighten:** AD-17 should explicitly require per-mapping error isolation in `close_all`/`slam`'s loop — collect a per-mapping result, keep iterating regardless of individual failures, and report an aggregate (which mappings closed, which didn't, and why) rather than propagating the first error.

---

## Finding 3 — Hooks-step failure inside `unlock` has no defined effect on the workflow's overall result or on AD-11's rollback (Severity: High)

**Where this sits:** AD-8 sequences `unlock` as: `LuksBackend::open` → `FilesystemBackend::mount` → hooks step (AD-14). AD-11's rollback clause ("if `open` succeeds but `mount` fails, `close` the just-opened mapping") is scoped only to the mount step, which by construction already succeeded before the hooks step ever runs — AD-11 predates AD-14's insertion of a third step after it and was never extended to cover it.

**The two units, for a mid-list `bind_mount` failure or an AD-14 exec-hooks hard-error (bad ownership/permissions):**
- **Unit A** treats any hooks-step failure as fatal to `unlock`, propagating `Err` up — but since the mount and LUKS-open already succeeded, the tomb is, in fact, open and usable; A additionally decides (nothing tells it either way) to symmetrically tear the whole thing back down (umount + close) to match "unlock failed," surprising a user who now has to re-touch their key for something AD-14 calls a "skip with a warning, never aborting the whole open" condition when it's a *containment-check* failure, but never says whether the same forgiving treatment extends to a `bind_mount` syscall failure or is overridden by a *hard* exec-hooks error.
- **Unit B** treats hooks-step failures (including the exec-hooks "hard error") as non-fatal to `unlock`'s own success — logs/warns, returns `Ok`, tomb ends up open and mounted with some bind-hooks silently unapplied and/or the open-time exec-hook simply never having run.

AD-14 pins the outcome for a *containment-check* failure (skip-with-warning, never abort) and for the exec-hooks *metadata* check (hard error) — but "hard error" is never connected back to what `unlock` as a whole then does: does it still report success (tomb is mounted and usable, just without its open-hook automation) or failure (implying the mount should be undone, which AD-11 never authorizes for anything past the mount step)? A `bind_mount` syscall failure occurring *after* an entry has passed both containment checks is a third case AD-14's text doesn't address at all (containment-check language covers only the pre-mount validation, not the mount call itself) — is a mid-list `bind_mount` failure treated the same as a skip-with-warning (continue to the next entry, and to exec-hooks, and report overall success), or does it escalate to the same "hard error" tier as exec-hooks? Two compliant readings genuinely diverge on whether the user's tomb ends up open-with-a-warning or torn back down after they already touched their key.

**AD to tighten:** AD-14 (or AD-11, extended) should state explicitly: (a) a `bind_mount` call failure after containment checks pass is treated identically to a containment-check failure (skip with warning, continue), not escalated; (b) an exec-hooks hard error is fatal to `unlock`'s reported result (`Err`), but never triggers an umount/close rollback — the mount stays up, since the underlying tomb open genuinely succeeded and hooks are cosmetic to that fact — so the user isn't left guessing whether their data is accessible.

---

## Finding 4 — AD-18's "retry" doesn't say whether it re-runs hooks, and exec-hooks is not guaranteed idempotent (Severity: Medium)

**The two units:** Both implement AD-18's escalation ladder for a busy mapping (SIGTERM → pause → retry; SIGHUP → pause → retry; SIGKILL → pause → retry).
- **Unit A** reads "retry" narrowly: only the failed `FilesystemBackend::umount` call is retried after each signal round; AD-8/AD-14's hooks step ran exactly once, before the first `umount` attempt, and is never re-invoked.
- **Unit B** reads "retry" as "retry the mapping's close attempt," and — since AD-8 defines "close" as the three-step hooks→umount→close sequence, not `umount` in isolation — re-enters the sequence from the top on every round, re-running the tomb's `exec-hooks close` invocation up to four times total (once initially, once per escalation round) for a single stuck mapping in one `slam` run.

Nothing in AD-18 scopes "retry" to the specific failed sub-step. This is not cosmetic: `hooks.md` places no idempotency requirement on exec-hooks (it's arbitrary user-authored code, e.g. "send a notification," "sync to remote," "re-seal a GPG keyring") — Unit B's reading can run that script repeatedly within seconds of itself, with no guarantee the author wrote it to tolerate that.

**AD to tighten:** AD-18 should state explicitly that only the `umount` call (and, transitively, the signal round) is retried — the hooks step for a given mapping runs exactly once per `close`/`close_all`/`slam` invocation of that mapping, never once per escalation round.

---

## Finding 5 — AD-19's `Stage` enum is illustrated as payload-free but not pinned as such, leaving room to smuggle AD-3-guarded secret-adjacent state into a progress callback (Severity: Medium)

**The two units:** Both implement `progress: &dyn Fn(Stage)` for `create`.
- **Unit A** implements `CreateStage` exactly as the four bare variants listed (`AllocatingBackingFile`, `FormattingLuks2`, `CreatingFilesystem`, `EnrollingFido2Key`) — no payload on any variant, matching the illustration verbatim.
- **Unit B**, aiming to be more "helpful" to `cli::ux::translate_stage` (e.g. richer status text), adds fields to stage variants for extra context — nothing in AD-19's Rule forbids a variant from carrying data; the Rule only says "`Stage` a typed per-workflow enum," not "a typed, payload-free enum." Because `FormattingLuks2`'s stage boundary sits squarely inside AD-9's bootstrap window — the transient passphrase (`zeroize::Zeroizing` buffer, AD-3) is generated, consumed by `luksFormat`/`luksOpen`, and only wiped *after* this stage's port calls return — a generic "attach whatever local context is in scope" implementation pattern for the payload is one refactor away from capturing a reference to that buffer (or a debug `Derive(Debug)` on a struct that happens to include it) before AD-3's wipe point is reached. AD-19 never cross-references AD-3's secret-hygiene boundary, so nothing in the text stops this.

This project has already hit exactly this shape of gap once (AD-9's `CreateTarget`, per the v3 review: a prose-described shape that needed to become a pinned type to stop two compliant builds from diverging on scope). AD-19's `Stage` illustration is at the same risk: safe by construction only as long as no implementer adds a payload, which the Rule doesn't forbid.

**AD to tighten:** State explicitly that every `Stage`/`CreateStage`/`ResizeStage` variant is a bare, payload-free unit variant — no variant may carry data derived from anything AD-3 scopes as secret-adjacent (or, more simply: no variant carries any data at all; the enum discriminant alone is the entire message, exactly as the given lists show).

---

## Checked, no fork found

- **AD-16 / "wrong FIDO2 device role" during create's bootstrap.** Investigated directly: `create`'s bootstrap has only one FIDO2-touching call (`Fido2Backend::enroll_fido2_key`, carrying `user_verification: bool`); the "transient" half of create's two-keyslot bootstrap (AD-9's `bootstrap_format_and_open`) is a passphrase-seeded `luksFormat`/`luksOpen` pair that never touches `Fido2Backend` at all and has its exact signature already pinned by AD-9 (no room to add a UV parameter there without violating AD-9 itself). So there is no live FIDO2 device-role ambiguity. The one soft spot: AD-16 calls this "create's bootstrap-enrollment step," a term that doesn't actually appear in AD-9's text (which separately names `bootstrap_format_and_open` for the passphrase phase and "enrolls the real FIDO2 key" for the FIDO2 phase) — purely a naming/cross-reference precision issue worth a wording pass, not a type-level fork; recommend AD-16 say "create's `enroll_fido2_key` call (the real-key phase, not `bootstrap_format_and_open`'s transient passphrase)" to close even the wording risk.
- **AD-15 (info).** Single call (`preflight` then `list_fido2_keyslots`), no branching, no secret path, no registry — no two-unit divergence found.

---

## Summary Table

| # | Severity | Clash |
|---|----------|-------|
| 1 | Critical | AD-14's "bind-hooks torn down by parent umount" is false for Linux bind mounts; `LuksBackend::close` then routinely fails for any hooked tomb, and no AD says whether that failure is surfaced honestly (confusing) or swallowed (silently-false CAP-9 violation) — and slam's process-targeted escalation can't resolve it either, since the busy-ness is a second kernel mount, not a process |
| 2 | Critical | close_all/slam's per-mapping error handling is unspecified — one mapping's failure (including Finding 1's routine one) can abort the whole batch under a letter-compliant "no special-casing" reading, defeating slam's own stated purpose |
| 3 | High | `unlock`'s hooks-step failure (mid-list `bind_mount` error, or exec-hooks hard error) has no defined effect on the workflow's overall Ok/Err result, and AD-11's rollback clause predates and doesn't cover this later step |
| 4 | Medium | AD-18's "retry" doesn't scope whether hooks re-run per escalation round — a compliant reading can re-invoke non-idempotent exec-hooks code up to 4x in one slam |
| 5 | Medium | AD-19's `Stage` enum is illustrated payload-free but not pinned as such — same prose-vs-pinned-type gap this project already hit once with `CreateTarget` |
