# Review: ARCHITECTURE-SPINE.md (CAP-18..25) vs. SPEC.md

**Scope:** Does the spine's AD-9, AD-19, AD-20, AD-21, the AD-2/AD-3/AD-4/AD-7/AD-8 amendments, the CAP-20 Consistency Convention row, and the AR-Dev5 references accurately and completely reflect what SPEC.md commits to for CAP-18..25 and the related Constraints/Non-goals entries?

**Files reviewed:**
- `_bmad-output/specs/spec-tomb-fido2/SPEC.md`
- `_bmad-output/specs/spec-tomb-fido2/hooks.md`
- `_bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md`

## Verdict

Mostly faithful — CAP-18, CAP-21, CAP-22, CAP-25 and the bulk of CAP-23 are well covered. But two concrete divergences survive: (1) AD-9's device-backed marker-verified resume path silently drops SPEC's unconditional "always confirm before formatting any device target" constraint, and (2) CAP-19's scaffolded non-executable `exec-hooks` stub, as specified, will trip AD-14's own hard-error guardrail on the very next `open` — a self-contradiction the spine doesn't acknowledge or resolve. Three lower-confidence gaps are also worth a look before sign-off.

---

## Findings

### 1. [CONFIRMED] AD-9's device-backed marker-verified resume bypasses SPEC's unconditional confirmation constraint

- **SPEC.md quote** (Constraints, line 135): *"Device-backed create is inherently higher-risk than file-backed (the wrong device vs. a typo'd file path) — the tool must show an explicit warning describing the impending wipe and data loss and **require the user's explicit confirmation before formatting any device target, even when no existing LUKS2 header was detected**."*
- This constraint has no carve-out for the marker-token-resume case — contrast with the adjacent constraint (line 134, "Create refuses outright…") which explicitly *was* amended with a marker-token exception ("**except** when the existing LUKS2 header carries CAP-23's hypogaol-owned marker token…"). The presence of an exception clause on 134 and its absence on 135 reads as deliberate: refusal is waived on marker-verified resume, but confirmation is not.
- **Spine location:** AD-9, device-backed branch (`ARCHITECTURE-SPINE.md` line 97): *"If a header *is* present, the same `has_marker_token(path)` check as the file-backed branch decides the outcome: `true` proceeds to formatting **regardless of `confirmed`'s value** (marker-verified resume bypasses the confirmation gate entirely, same reasoning as the file-backed branch)…"*
- **Gap:** the spine extends the file-backed branch's "no confirmation needed on marker-verified resume" reasoning to the device-backed branch too, but SPEC's device-backed confirmation constraint is written as an unqualified "any device target" rule with an explicit "even when no header was detected" emphasis — i.e., SPEC treats device confirmation as orthogonal to header state entirely, never mentioning marker tokens. A user who accidentally interrupted `create` against the *wrong* device would, under the spine's rule, have that device silently re-wiped and reformatted with zero confirmation on the next `create` invocation, which is exactly the "wrong device vs. typo'd path" risk the constraint exists to guard against. This looks like a real, not merely cosmetic, safety regression relative to SPEC's stated intent.

### 2. [CONFIRMED] CAP-19's non-executable exec-hooks stub collides with AD-14's own hard-error guardrail

- **SPEC.md quote** (CAP-19 success, line 93): *"When requested, create writes commented-out example bind-hooks entries and **a non-executable exec-hooks stub** into the volume root; when not requested (default), no template files are written."*
- **Spine location 1:** AD-9 (`ARCHITECTURE-SPINE.md` line 99), realizing CAP-19: *"…calls `FilesystemBackend::scaffold_hook_templates(mountpoint)` to write the commented-out bind-hooks example and **the non-executable exec-hooks stub**…"* — faithfully carries SPEC's wording forward.
- **Spine location 2:** AD-14 (line 132), pre-existing rule unchanged by the Epic 6 amendments: *"If `exec-hooks` is present, `hook_file_metadata` must show a regular file, **executable bit set**, owned by the invoking user or root, and not world-writable — failing any check is a **hard error** (not a skip-with-warning, since this guards code execution) that aborts the whole open via AD-11's rollback clause…"*
- **Gap:** taken together, these two rules mean a freshly `create --scaffold-hooks`'d tomb ships with an `exec-hooks` file that is *present but not executable* — which is precisely the condition AD-14 defines as a hard error aborting `open` entirely. Unless the user manually `chmod +x`'s or deletes the stub before their first `unlock`, every subsequent open of that tomb fails outright. This directly undercuts CAP-19's own stated purpose ("hooks discoverable without consulting docs first" — SPEC line 92) and is not addressed anywhere in AD-9's realization, AD-19's `ScaffoldingHookTemplates` stage note, AD-14 itself, or the CAP-19 row of the Capability → Architecture Map. Either AD-14's guardrail needs an explicit carve-out for scaffolded-but-unedited stubs, or the "non-executable" framing needs a companion note (e.g., "scaffolded stub is written, but must be enabled before use, and the tool must not error on its mere presence") — none of which currently exists in the spine.

### 3. [PLAUSIBLE] AD-9's marker-verified device resume doesn't restate the size≤capacity check

- **SPEC.md quote** (Constraints, line 133): *"A device-backed tomb (CAP-8) defaults to the target device/partition's full capacity when no size is given; a user-supplied size is accepted and may be smaller than the device's actual capacity…, but must never exceed it."* This is stated as a blanket rule for "a device-backed tomb," not scoped to first-time creates only.
- **Spine location:** AD-9 device-backed branch (line 97). The size-resolution/capacity-validation logic ("a user-given size must not exceed `FilesystemBackend::device_capacity(path)`, rejected if it does…") is stated only inside the `has_luks2_header(path) == false` clause. The subsequent marker-verified-resume clause ("If a header *is* present, the same `has_marker_token(path)` check… `true` proceeds to formatting regardless of `confirmed`'s value…") never restates size resolution or the capacity ceiling.
- **Gap:** it's ambiguous by omission whether a resumed device-backed create re-validates a (possibly re-supplied, possibly different) `size` against `device_capacity` before calling `bootstrap_format_and_open`, or whether the resume path silently skips that validation. Worth an explicit line either way before implementation.

### 4. [PLAUSIBLE] AD-20's concurrency guard excludes unlock (CAP-1/CAP-11) from its binds

- **SPEC.md quote** (CAP-24 intent, line 112): *"Tool prevents **two simultaneous invocations** against the same tomb from racing past each other in a way that corrupts state or bypasses a safety guard, particularly AD-5's live-count last-keyslot guard."* The intent is phrased generically over "invocations," with the keyslot-guard example introduced by "particularly," implying it is the leading example, not the sole scope.
- **Spine location:** AD-20 (`ARCHITECTURE-SPINE.md` line 169): *"**Binds:** CAP-24, and transitively every mutating workflow (CAP-2, CAP-3, CAP-8, CAP-9, CAP-10, CAP-14, CAP-15, CAP-23)"* — CAP-1 and CAP-11 (unlock, including read-only) are not listed, and AD-20's rule text confirms the lock is acquired only in mutating `domain::workflows::*` functions.
- **Gap:** two concurrent `unlock` invocations against the same tomb (e.g., duplicate mount attempts, or concurrent hook execution racing bind-mounts/exec-hooks) are left entirely outside the new guard. This may well be a deliberate, defensible scoping decision (unlock doesn't write the LUKS2 header, so "corrupts state" arguably doesn't apply), but the spine doesn't state that reasoning explicitly anywhere — it's a silent exclusion rather than an argued one, which is worth confirming was intentional.

### 5. No finding possible — presumed Non-goals entry does not exist in SPEC.md

- SPEC.md's Non-goals section (lines 144–149) contains exactly four items: remote/delegated unlock beyond cryptsetup's native FIDO2 mode, post-quantum-readiness, shrink-not-supported, and deferred exposure of the full `systemd-cryptenroll` FIDO2 flag set. **There is no Non-goals entry about "mixed-mode filesystem support"** anywhere in the current SPEC.md. The only occurrences of "mixed" in the whole document are in CAP-22's own intent/success text, describing `mkfs.btrfs --mixed` as the mechanism used, not a non-goal.
- Since the premise for this check (a Non-goals entry the AD-8 amendment might be in tension with) isn't present in the file as it currently stands, there is no divergence to report here. Flagging this explicitly rather than fabricating a finding against text that doesn't exist — worth double-checking whether such a Non-goals entry was intended to be added to SPEC.md and is simply missing, since AD-8's CAP-22 realization (multiple filesystem types via one additive port) is exactly the kind of design a "no mixed filesystem support" non-goal would normally be paired with.

### 6. [PLAUSIBLE] "AR-Dev5" is referenced but never defined in this document

- The spine attributes CAP-21 governance to "AR-Dev5" in the Capability → Architecture Map (line 288: *"CAP-21 (repo-hygiene badges, coverage, security audit) | CI workflows, `README.md` | AR-Dev5 (tooling, not a domain AD)"*) and tags three Stack-table rows with `AR-Dev5/CAP-21` (lines 211–213: `cargo-llvm-cov`, `cargo-audit`, `Codecov`).
- No `### AR-Dev5` (or any AR-Dev5) section exists anywhere in `ARCHITECTURE-SPINE.md` — unlike every AD it sits alongside (AD-1 through AD-21), there is no rule text backing the label. This isn't necessarily wrong (CI/badge specifics are legitimately out of the spine's scope per the review brief), and the substantive CAP-21 commitments — cargo-audit as a *gating* CI job, cargo-llvm-cov reported to Codecov — are in fact captured in the Stack table prose. But the label "AR-Dev5" reads as a cross-reference to a decision defined elsewhere (a workflow file or PRD Architecture Requirements list not covered in this review), and a reader of the spine alone cannot verify what AR-Dev5 actually commits to. Worth confirming that AR-Dev5 is genuinely defined in its home document and that its content matches what's implied here.

---

## Not flagged (in-scope check, no divergence found)

- CAP-18 (custom bootstrap label): AD-9's `key_label: Option<String>` with default-fallback-on-`None` matches SPEC's success criterion exactly.
- CAP-20 (short aliases): the Consistency Conventions row's derivation rule doesn't explicitly restate SPEC's "`--help` documents the alias" half of the success criterion, but this is standard `clap` behavior once short flags are defined — not treated as a genuine gap.
- CAP-21 (badges): the full badge set, links, and the "cargo-audit gates the build" requirement are captured (Stack table + map row); CI/badge wiring specifics are correctly left to a workflow file.
- CAP-22 (XFS/Btrfs): AD-8's realization matches SPEC's `--mixed`-unconditionally, ~18 MiB floor, and matching-growfs-tool requirements closely, including reading filesystem type from the token field rather than re-asking/sniffing.
- CAP-23 (crash-safe resume): AD-2's "Realized" note and AD-9's marker-token write/removal timing ("immediately after `luksFormat` succeeds," "removed as the last step… alongside the existing bootstrap-keyslot cleanup") match SPEC's wording closely, including using AD-2's already-documented custom-token-type fallback mechanism rather than the primary systemd-fido2 token — consistent with SPEC's parenthetical "the same token mechanism AD-2 already uses."
- CAP-24 (concurrency guard, mutating workflows): the SPEC success criterion explicitly OR's "serializes behind the first" vs. "fails cleanly with a clear error" — AD-20 picks the latter (non-blocking `flock`, clear error message) which is a compliant choice, not a gap.
- CAP-25 (PIN detection): AD-21's proactive `client_pin` enumeration plus AD-3's stderr-parsing amendment for the reactive wrong-PIN-retry warning together cover both halves of CAP-25's success criterion.
