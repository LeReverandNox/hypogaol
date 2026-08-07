# Adversarial Review — ARCHITECTURE-SPINE.md (Epic 6 amendments: CAP-18..25 / AD-9, AD-20, AD-21 focus)

Reviewed: `architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md`, `updated: 2026-08-08`.

Method: for each candidate hole, state the ambiguous instruction verbatim, two letter-compliant interpretations two independent implementers could each honestly reach, and why the divergence is real (not stylistic). Findings are ranked by severity of the resulting incompatibility/race.

---

## Finding 1 (severe) — Final-cleanup sub-order of marker-token removal vs. transient-keyslot removal is unstated, and the "wrong" order silently destroys a completed tomb

**Quote (AD-9):** "`domain` then enrolls the real FIDO2 key via `Fido2Backend`/`systemd-cryptenroll`... then removes the transient passphrase keyslot via AD-5's guarded removal primitive **and removes the CAP-23 marker token, both as create's final cleanup step.**"

This sentence names two removals — transient keyslot, marker token — as "both" one final step, but never states which happens first. AD-9's own crash-safety argument for CAP-23 depends entirely on that sub-order, and the sentence's surface reading (keyslot named first, marker named second) points at the *dangerous* order.

**Interpretation A (safe):** remove the marker token first, transient keyslot second. If the process crashes between the two, `has_marker_token` now reads `false` on a fully-working, FIDO2-enrolled tomb. A later `create` on the same path sees `has_luks2_header = true`, `has_marker_token = false`, and refuses as "genuine foreign file" — annoying (the stray transient passphrase keyslot is now permanently un-removable through `create`'s own resume path, and it is invisible to `revoke`'s guard too, since AD-5 defines "valid keyslot" as one with an associated `systemd-fido2` token, which the transient keyslot never has), but no data is destroyed.

**Interpretation B (catastrophic, and the order the sentence's word order literally suggests):** remove the transient keyslot first, marker token second. If the process crashes in that window, the marker token is still present on a tomb that is otherwise complete and in use (FIDO2 key enrolled, transient keyslot already gone). AD-9 states the marker is "structural proof nothing of value survived" and that when it's present, resume "proceeds to formatting **regardless of `confirmed`'s value**" with "no confirmation prompt." Re-running `create` against this path therefore re-runs `luksFormat` and silently wipes a fully-functional tomb with no warning — the exact scenario CAP-23 exists to prevent, reintroduced by the ordering CAP-23 itself never pins.

Both implementers are reading the same sentence and each can defend their order as compliant, because the sentence names two actions in one clause without a stated sequence — unlike AD-5, which is explicit and reasoned about revoke's token-then-keyslot order for exactly this class of crash-safety concern, AD-9 never makes the analogous statement for its own final cleanup pair.

---

## Finding 2 (severe) — `has_marker_token`'s precondition contract is defined two different (and contradictory) ways by the file-backed vs. device-backed branches

**Quote, file-backed branch:** "`false` (no valid LUKS2 header at all, or one without the marker) means a genuine foreign file" — stated as the direct, unconditional result of calling `has_marker_token(path)` with **no prior `has_luks2_header` check** in that branch's control flow at all.

**Quote, device-backed branch:** "if `LuksBackend::has_luks2_header(path)` is `false`, resolve the size... If a header *is* present, the same `has_marker_token(path)` check... decides the outcome" — `has_marker_token` is only ever invoked **after** `has_luks2_header` has already confirmed a header exists.

These two branches embed two different, incompatible assumptions about the same shared port method:

**Interpretation A:** `has_marker_token(path)` is self-contained and safe to call on any existing path, including one with no valid LUKS2 header at all (a plain file, a truncated/corrupted header) — it internally does the equivalent of `has_luks2_header` and returns `false` cleanly if that fails. This is what the file-backed branch's parenthetical requires, since it calls `has_marker_token` directly on any `path_exists == true` target without a preceding header check.

**Interpretation B:** `has_marker_token(path)` requires a valid header as a precondition and its behavior on a header-less path is undefined/unspecified (it may error, panic on an unwrap of `luksDump` output, or return a `Result` that the device-backed branch's explicit prior gate exists specifically to avoid triggering). This is what the device-backed branch's control flow implies by never calling it without a preceding `has_luks2_header == true`.

An implementer who builds the real `has_marker_token` to Interpretation B (plausible, since it's the *only* branch of AD-9 that shows explicit defensive gating) breaks the file-backed branch's documented behavior the moment it's invoked directly against a `path_exists == true` target with a corrupted/truncated header or an unrelated non-LUKS2 file — instead of the promised "no confirmation, refuse" outcome, the call errors in a way AD-9 never names or routes anywhere (there is no `DomainError` variant for this scenario named in the doc). This is not hypothetical: a truncated/corrupted LUKS2 header (explicitly one of the two scenarios the prompt calls out) is exactly the case where `has_luks2_header`'s own behavior is itself unspecified (does it do a full parse-validate, or a cheap magic-byte check that would return `true` on a header that's present-but-corrupt, versus `false`?) — so the file-backed and device-backed branches can genuinely diverge on which outcome ("refuse cleanly" vs "unhandled error") a corrupted header produces, purely as a function of which branch the target happens to hit.

---

## Finding 3 (moderate-severe) — Device-backed marker-resume branch never restates size resolution/validation, leaving a shrunk-device case unhandled

**Quote:** "if `LuksBackend::has_luks2_header(path)` is `false`, resolve the size (a user-given size must not exceed `FilesystemBackend::device_capacity(path)`, rejected if it does... if omitted, default to that full capacity) and require `confirmed == true`... If a header *is* present, the same `has_marker_token(path)` check... decides the outcome: `true` proceeds to formatting **regardless of `confirmed`'s value**."

Size resolution/validation against `device_capacity` is stated only inside the `has_luks2_header == false` sub-branch. The `has_luks2_header == true` + `has_marker_token == true` (marker-resume) sub-branch says only "proceeds to formatting" — it never restates whether size is re-resolved/re-validated. Contrast with the file-backed branch, which explicitly says "**Either way**, once past this gate, call `set_backing_file_size`" — file-backed spells out that both sub-branches converge on the same allocation step; device-backed has no equivalent "either way" sentence for size resolution.

**Interpretation A:** size resolution is a universal precondition of `bootstrap_format_and_open` regardless of which sub-branch got there, so an implementer re-resolves/re-validates size against current `device_capacity` even on the marker-resume path, naturally handling a device that has shrunk since the original crashed attempt (e.g., the path now points at a smaller LUN/loop device than when the marker was written).

**Interpretation B:** size resolution is scoped, by the text's own structure, to the `has_luks2_header == false` sub-branch only — since the marker-resume sub-branch "proceeds... regardless of `confirmed`," an implementer reasonably reads that as "skip the whole preceding gate block, including size resolution, and reuse whatever size argument was already passed in" (or `None`), passing a stale/unvalidated size straight into `bootstrap_format_and_open`. On a device that has shrunk since the marker was written, this either makes `luksFormat` fail confusingly mid-resume (better case) or, if the size argument silently defaults/threads through as a smaller-than-declared LUKS2 payload spec, produces a header describing more space than the device now has.

---

## Finding 4 (moderate-severe) — AD-20's per-mapping lock in `close_all`/`slam` is not pinned to per-iteration acquire/drop, and the doc's own phrasing points at the wrong model

**Quote:** "Every mutating `domain::workflows::*` function (`create`, `enroll`, `revoke`, `close`, `resize`, and **transitively `close_all`/`slam` per mapping**) acquires this lock as its **second statement, immediately after `preflight` (AD-4) passes**." Compare AD-4's own statement about `close_all`/`slam`: "`info`, `close_all`, and `slam`... each call `preflight` **first too**" — describing one `preflight` call at the top of the batch function, not one per mapping. AD-17 describes the per-mapping sequence starting from "hooks, then bind-hooks teardown, then primary `umount`, then `LuksBackend::close`" — it never mentions a lock acquisition step in that per-mapping sequence at all, and `close_all`/`slam` are explicitly said to need "no original device/file path" (AD-17), so there is no single path `lock_target` could be called against once, at the batch level, the way `preflight` is.

**Interpretation A (correct-by-necessity):** because there is no single batch-level path, the lock must be acquired and dropped once per mapping, inside the loop, scoped to that mapping's own close attempt — matching AD-17's per-mapping error-isolation model (each mapping's `Result` is independent) and the parenthetical "per mapping."

**Interpretation B:** an implementer pattern-matching AD-4's explicit "preflight runs once, at the top, for `close_all`/`slam`" onto AD-20's "immediately after preflight passes" (since AD-20 is phrased as directly following AD-4's placement rule, and AD-4's Epic-6 amendment text — "AD-20's per-invocation lock is acquired immediately after this gate passes, never before it and never folded into it" — reads as talking about *the same, single* gate-then-lock pair AD-4 describes for `close_all`) treats "transitively... per mapping" as loosely meaning "this AD's guarantee extends to close_all/slam in aggregate" rather than "re-executed on every loop iteration," and either (a) skips locking for the batch workflows entirely (there being no batch-level path to lock against, and the spine never shows the loop body literally calling `lock_target`), or (b) locks only the first mapping discovered and holds that single guard for the whole loop, satisfying "acquired... as its second statement" read as a single event in the function's lifetime. Neither reading is contradicted by any sentence that explicitly writes out "call `lock_target` inside the per-mapping loop body, drop it before advancing to the next mapping" — that sentence does not exist anywhere in the document; it is inferred, not stated. Since two concurrent `close_all` invocations, or a `close_all` racing an individual `revoke`, on overlapping mappings, is precisely the class of bug AD-20 exists to close, an implementer following Interpretation B reopens the race AD-20 claims is closed, while remaining consistent with every sentence actually written.

---

## Finding 5 (moderate) — `lock_target`'s canonicalization is pinned to "the same helper," but that helper's export shape (one atomic function vs. two composable pieces) is never specified, and `lock_target`'s real implementation sits in a different layer (`adapters::exec`) than the helper it must reuse (`domain::mapping_name`)

**Quote (AD-20):** "`flock(2)`... on an open fd to the path, canonicalized via **the same helper** AD-12's mapping-name derivation already uses." **Quote (Structural Seed, `mapping_name.rs`):** "**single shared canonicalize+hash helper** (AD-12)." **Quote (Structural Seed, `adapters/exec/`):** "`lock_target` (AD-20) uses `flock(2)` on an open fd."

AD-12 places its canonicalize+hash helper in `domain` specifically because "no subprocess is involved" — implying `domain` hosts pure helpers of this kind. `lock_target`'s real implementation, per the Structural Seed, lives in `adapters::exec`, alongside the actual `flock(2)` syscall. For `adapters::exec`'s `lock_target` to canonicalize "via the same helper" as `domain::mapping_name`, either (a) `mapping_name.rs` must expose its canonicalize step as a separately callable function distinct from the full hash-producing call, which `adapters::exec` then imports and calls, or (b) "the same helper" is read loosely as "the same canonicalization *semantics*," and `adapters::exec` re-implements realpath resolution locally (e.g. `std::fs::canonicalize` called directly inside `lock_target`), never actually calling into `domain::mapping_name`.

**Interpretation A:** `mapping_name.rs` is refactored to export a public `canonicalize(path) -> PathBuf` used both by `mapping_name()` internally and by `adapters::exec::lock_target` — a true single source, satisfying the letter and the intent.

**Interpretation B:** since the Structural Seed calls `mapping_name.rs` a "**single** shared canonicalize+hash helper" (singular, one function, no mention of a separately exported canonicalize step), and `lock_target` sits in a different module (`adapters::exec`) that has no stated dependency on `domain::mapping_name`, an implementer reasonably treats "the same helper" as describing equivalent behavior, not a shared call site, and writes `lock_target`'s canonicalization independently. This compiles, passes every test AD-7 describes (fakes don't exercise real canonicalization), and is functionally identical for ordinary symlinked device paths today — but it is a second, textually distinct canonicalization routine that can silently drift from `domain::mapping_name`'s the moment either one gains any device-specific normalization AD-12 doesn't yet have (e.g. resolving `/dev/disk/by-id/...` aliases, handling a path with a trailing slash, or normalizing relative-to-CWD arguments differently) — at which point the mapping name and the lock target for the *same underlying device* could diverge, which is exactly the two-independently-written-call-sites failure AD-12 was written to prevent, now reopened one layer down by AD-20.

---

## Finding 6 (moderate) — AD-21's proactive PIN warning is specified to print from inside `adapters::exec`'s private (non-port) resolver/loop code, breaking the architecture's own "plain-language text only at the `cli` boundary" convention and leaving it outside AD-7's unit-test harness

**Quote (Consistency Conventions):** "domain errors are a typed enum... translated to plain-language text only at the `cli` boundary — **never inside `domain`**; progress stages follow the same translate-at-the-boundary shape." **Quote (AD-21):** "Both existing call sites **print** a plain-language PIN warning for any device about to be used, whenever `client_pin` is `true`, before the blocking touch/PIN subprocess call" — where "both existing call sites" are named as living inside `adapters::exec`'s private enumeration/resolver machinery ("none of this enumeration machinery is itself a port method... FIDO2 device-selection UX lives inside `adapters::exec`, not `domain`/`cli`").

The Structural Seed's `cli/ux.rs` entry lists exactly what plain-language text the `cli` layer owns: "domain-error -> plain-language translation (CAP-5, incl. lock-contention CAP-24 and **PIN-retry CAP-25** errors); `translate_stage` (AD-19)" — the *reactive* PIN-retry warning (captured from stderr) is explicitly routed through `cli::ux`. The *proactive* `client_pin` warning AD-21 introduces is never added to that list, and AD-21's own text has it printed directly, in place, inside `adapters::exec`.

**Interpretation A:** treat AD-21's "print" literally — `adapters::exec` performs the actual `println!`/output call itself, directly at the two (or three — see Finding 7) resolver/loop sites, bypassing `cli::ux` entirely. This is what the paragraph's plain language most directly supports, and it avoids inventing a new port method (which AD-21 explicitly rules out) — but it means a real, user-facing message now originates from the one layer (`adapters::exec`) that AD-7's unit-test suite never exercises (fakes stand in for the whole port; the real `adapters::exec` resolver/print code is only reached by the manual hardware suite), and it is architecturally the only user-facing text in the whole system that does not flow through `cli::ux::translate`/`translate_stage`.

**Interpretation B:** an implementer who takes the "never inside `domain`... translate-at-the-boundary" convention as a whole-of-architecture rule (not scoped narrowly to `domain`) instead threads `client_pin` as data up through the already-returned `Fido2Device`/selection result to `domain`/`cli`, and prints the warning at the `cli` boundary alongside the existing PIN-retry warning — consistent with every other user-facing string in the document, but requiring `domain::workflows::unlock`'s presence-wait interaction and `enroll`'s device-selection flow to each surface a new "warn before touch" seam that doesn't exist today, which AD-21 never describes and which is a materially different code shape (a data/callback path) than "print... at the call site."

Both are defensible: A follows AD-21's literal words at the cost of contradicting the doc's own stated boundary convention; B follows the boundary convention at the cost of inventing plumbing AD-21 never mentions and arguably violates AD-21's explicit "no new port method" intent if implemented via a return-value change to a port trait method's signature.

---

## Finding 7 (minor-moderate) — "Both existing call sites" undercounts by name, and coverage of `create`'s bootstrap enrollment is inferred, not stated

**Quote (AD-21):** "already shared by `LuksBackend::open`'s presence-wait loop **and** `Fido2Backend::enroll_fido2_key`'s `Fido2DeviceSelection::Interactive`**/**`Explicit` resolvers... **Both** existing call sites print a plain-language PIN warning."

The parenthetical names what reads as three distinct blocking-call sites — `open`'s loop, the `Interactive` resolver, and the `Explicit` resolver — then the operative sentence collapses them to "both." An implementer who reads "enroll's device-selection resolvers" as one unit (because `Interactive`/`Explicit` share a common entry function) naturally warns both; an implementer who treats them as genuinely separate code paths (plausible — `Explicit` skips the interactive menu and waits on a directly-named device, a materially different control flow) could add the warning only to the more commonly-exercised `Interactive` path and consider "the enroll call site" (singular, as "both" implies) done, silently missing PIN warnings on `--device`-style explicit enrollment.

Separately, AD-16 states `create`'s bootstrap-enrollment step calls the same `enroll_fido2_key` — implying it funnels through the same resolvers and is therefore covered "for free" if the warning lives in the shared resolver code (Finding 6, Interpretation A). But AD-21 never says this explicitly, and `create`'s CLI flags (per the Structural Seed's `cli/main.rs` list) show no `--device`-style flag for `create` at all, leaving it ambiguous whether `create`'s bootstrap step reuses `Interactive`/`Explicit` verbatim or calls a simpler, bespoke "just use the only/first device" helper that bypasses both resolvers — in which case neither call site's warning fires for `create`, and a spine reader checking only AD-21 (not cross-referencing AD-16 and AD-9) would have no way to know coverage is incomplete for `create`.

---

## Finding 8 (minor) — PIN-warning repetition inside `unlock`'s presence-wait loop is unspecified: once vs. every poll tick

**Quote (AD-21):** "print a plain-language PIN warning... before the blocking touch/PIN subprocess call." `open`'s presence-wait loop (per AD-1/CAP-1's existing design, reused unmodified here) polls repeatedly until the device is touched, and `Fido2Device`/`client_pin` is populated by a `fido2-token -I` call that AD-21 says happens "per enumerated device" — i.e., potentially on every poll iteration if the loop re-enumerates each tick.

**Interpretation A:** the warning prints once, before the loop is entered.
**Interpretation B:** each poll iteration is itself "the blocking... subprocess call" the sentence refers to, so the warning (correctly, per the letter) prints on every retry — spamming the same PIN warning to the terminal once per poll interval until the user touches the key.

Neither reading is excluded by the text; the difference is a real, user-visible behavioral divergence between two compliant implementations, though lower-severity than Findings 1–6 since it affects UX polish rather than correctness or data safety.

---

## Non-findings (explicitly checked, found adequately specified)

- **CAP-19 scaffold mount/write/unmount vs. CAP-23 marker write/removal, overall total order:** the AD-9 paragraph states one continuous linear narrative — bootstrap (incl. marker **write**) → `mkfs` → optional scaffold mount/write/unmount → FIDO2 enroll → transient-keyslot removal + marker **removal** ("final cleanup step"). Scaffold is textually and logically pinned strictly before marker removal in every reading; there is no plausible alternate ordering here. (The real hole in this territory is Finding 1 — the sub-order *within* the final cleanup pair — not the scaffold step's placement.)
- **AD-20's canonicalization *source*:** the spine does correctly pin `lock_target` to "the same helper" as AD-12 rather than leaving canonicalization unaddressed — the residual ambiguity is only in the export/module-boundary mechanics (Finding 5), not a from-scratch omission.

---

## Summary table

| # | Severity | Locus | One-line risk |
|---|---|---|---|
| 1 | Severe | AD-9 final cleanup | Unstated marker-vs-keyslot removal order; literal reading enables silent destruction of a completed tomb on crash |
| 2 | Severe | AD-9 `has_marker_token` | File- and device-backed branches assume contradictory preconditions for the same port method |
| 3 | Moderate-severe | AD-9 device resume | Size re-validation on marker-resume path (shrunk device) never restated, unlike file-backed's explicit "either way" |
| 4 | Moderate-severe | AD-20 + close_all/slam | Per-mapping lock acquire/drop scoping inferred, not stated; wording nudges toward batch-level/skipped locking |
| 5 | Moderate | AD-20 canonicalization | "Same helper" doesn't pin export shape; adapters-layer reimplementation is letter-compliant and can drift |
| 6 | Moderate | AD-21 print boundary | Proactive PIN warning specified to print from `adapters::exec`, contradicting the doc's own cli-boundary convention |
| 7 | Minor-moderate | AD-21 call-site count | "Both" undercounts Interactive/Explicit/create-bootstrap; coverage of create is inferred not stated |
| 8 | Minor | AD-21 loop repetition | Warning-once vs. warning-per-poll-tick inside unlock's presence-wait loop is unspecified |
