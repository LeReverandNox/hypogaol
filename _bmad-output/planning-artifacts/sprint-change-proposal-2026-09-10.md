# Sprint Change Proposal — 2026-09-10

**Trigger:** Bug found during manual troubleshooting alongside Story 7.1 (unrelated to 7.1's own changes — reproduced on a pre-7.1 build too).
**Mode:** Batch
**Prepared by:** Amelia (Dev agent) via `bmad-correct-course`

## 1. Issue Summary

`hypogaol unlock` fails with the generic message `"Your volume unlocked, but Hypogaol couldn't mount its filesystem."` on any machine where a desktop mount manager (udisks2/gvfs) is running.

**Root cause:** `FilesystemBackend::mount`'s real implementation (`src/adapters/exec/mod.rs:2864-2903`) only privilege-fixes ownership of the shared base directory `/run/media/<username>` when it doesn't already exist:
```rust
if !base.exists() {
    // privileged mkdir -p 0755 + chown uid:gid
}
let mountpoint = create_mount_point(&base, &volume_name)?;  // unprivileged create_dir
```
When udisks2 is present, it pre-creates and owns that directory as `751 root:root`. Hypogaol's `!base.exists()` guard then skips the ownership fix, and `create_mount_point`'s unprivileged `std::fs::create_dir` (`src/adapters/exec/mod.rs:174-201`) fails with `EACCES`. That failure text ("failed to create mount point …") contains the substring `"mount"`, so `ux.rs:402-403`'s generic mount-failure bucket swallows it — surfacing as an indistinguishable filesystem-mount error.

**Evidence:**
- `stat -c '%U:%G %a' /run/media/$USER` → `root:root 751` on the affected machine (user-owned on the reporter's regular machine).
- `mkdir /run/media/$USER/probe-test` → `Permission denied`, reproducing the exact failure mode outside Hypogaol.
- A manual `cryptsetup luksOpen` + `mount /dev/mapper/... /mnt/qcow2` (a path the user owns) succeeds cleanly — confirms the LUKS/mount machinery itself is fine; the break is specifically in Hypogaol's own mount-point bootstrap.

**Not a new gap — a regression of an already-fixed bug.** Story 1.9's own review record (`_bmad-output/implementation-artifacts/1-9-mount-ux-and-ownership-hardening.md:54`) documents this *exact* failure mode as a "[Review][Patch]" finding, marked `[x]` and claimed fixed: *"gate the mkdir+chown on a uid stat comparison instead of `base.exists()`, so a pre-existing wrong-owner directory is corrected instead of skipped."* `git blame -L 2864,2903 -- src/adapters/exec/mod.rs` shows every line of the current `base.exists()` block still traces to the original `9065e2c` (2026-07-25) commit, and the follow-up review-fix commit (`c005771`) never touched `src/adapters/exec/mod.rs` at all. The documented fix was never actually implemented — the story was marked done on a review finding that didn't land in code. No unit test could have caught this (Story 1.9's own Dev Notes: this logic is only provable by the hardware-gated test, which apparently didn't exercise a udisks2-managed machine).

**Agreed fix** (reference: `dyne/tomb`, a sibling project, hit this identical udisks2 conflict — their own comment at `tomb:2722` cites it as issue #461): make the per-volume mount-point directory creation in `create_mount_point` **unconditionally privileged** — always `sudo mkdir` + `sudo chown` the leaf directory to the invoking user, regardless of the parent's existing ownership. No dependence on `base.exists()` at all. Explicitly **out of scope**: tomb also grants a POSIX ACL on the shared parent (`setfacl -m u:$_USER:r-x`) for listability — declined; mount succeeding and the volume being accessible to the invoking user is sufficient.

## 2. Impact Analysis

- **Epic impact:** Epic 7 only. No epic becomes non-viable, no epic reordering beyond inserting one story. Epic 7's stated goal (FIDO2 unlocking-behavior flags) is unaffected — this fix is orthogonal, but blocks confident work on 7.2+ if left until later (an unrelated failure during 7.2/7.3/7.4's own hardware verification would otherwise be misdiagnosed the same way this one was).
- **Story impact:** Story 1.9 (done, historical) — no edit to that file; it stays as the historical record, but its review-finding/code mismatch is worth a watch-item (see §4) so this pattern gets checked, not just documented, next time. New story needed between 7.1 (done) and 7.2 (backlog, not started).
- **PRD/SPEC conflict:** None. `SPEC.md` and the Requirements Inventory (FR/CAP list) already cover this under existing CAP-1/CAP-11 (unlock, incl. read-only) — no new FR/CAP/NFR needed, this restores intended reliability rather than adding capability.
- **Architecture conflict:** None. AD-12 (deterministic mapping name and mountpoint discovery, no registry) is unaffected — the fix only changes *how* the mount-point directory gets created, not where it lives or how it's later discovered (`findmnt`-based, per AD-12, untouched). Confirmed via `ARCHITECTURE-SPINE.md` — no section requires updating.
- **UX conflict:** N/A — CLI-only tool, no UX spec exists.
- **Other artifacts:** None — no CI/deployment/monitoring impact. Test impact is contained to `src/adapters/exec/mod.rs`'s existing inline unit tests (`create_mount_point`) and the hardware-gated suite (same pattern as Story 1.9 itself — this logic can't be proven by a `domain`-level fake).

## 3. Recommended Approach

**Option 1 — Direct Adjustment.** Insert one new story into Epic 7, between 7.1 and 7.2. Existing Stories 7.2/7.3/7.4 renumber to 7.3/7.4/7.5 (all three are still `backlog` in `sprint-status.yaml` — no story files exist yet for any of them, so renumbering costs nothing beyond `epics.md`/`sprint-status.yaml` text).

- Effort: **Low** — single adapter function (`create_mount_point` + `mount`'s base-bootstrap branch), no port signature change, no domain-layer change.
- Risk: **Low** — root cause and fix shape are both already confirmed against a real reproduction and a proven reference implementation (tomb).

**Option 2 — Rollback:** Not viable. Nothing to roll back — the bug predates Story 7.1 and reproduces on a pre-7.1 build.

**Option 3 — MVP Review:** Not applicable. No PRD/SPEC scope or goal is affected.

**Selected: Option 1.**

## 4. Detailed Change Proposals

### 4.1 `epics.md` — renumber existing stories

`Story 7.2` → `Story 7.3`, `Story 7.3` → `Story 7.4`, `Story 7.4` → `Story 7.5`. Five cross-reference lines inside the (unchanged) body text of those stories update their self-references accordingly (`Story 7.4's menu` → `Story 7.5's menu`, `Story 7.3 reports` → `Story 7.4 reports` ×2, `Story 7.2/NFR22` → `Story 7.3/NFR22`, `foundation for Story 7.4` → `foundation for Story 7.5`). No AC text content changes, only numbers.

### 4.2 `epics.md` — insert new Story 7.2

```
### Story 7.2: Mount-Point Creation Resilient to Pre-Existing `/run/media` Ownership

As a user,
I want unlock's mount-point directory creation to succeed regardless of what already owns `/run/media/<username>`,
So that unlocking works the same whether or not a desktop mount manager (udisks2/gvfs) is running on my machine.

**Acceptance Criteria:**

**Given** `/run/media/<username>` does not yet exist
**When** `unlock` mounts a volume
**Then** behavior is unchanged from today — it's created and the per-volume mount point is chowned to the invoking user

**Given** `/run/media/<username>` already exists, owned by another user or root (e.g. pre-created by udisks2/gvfs at `751 root:root`)
**When** `unlock` mounts a volume
**Then** the per-volume mount-point directory is still created successfully — via an unconditionally privileged `mkdir` + `chown` of the leaf directory to the invoking user, regardless of the parent directory's ownership — and mount proceeds normally

**Given** the per-volume mount-point directory name is already in use (collision), under either ownership scenario above
**When** `create_mount_point`'s retry logic runs
**Then** the existing basename-then-suffixed-retry collision behavior (Story 1.9, AC #2) is unchanged

**Given** this fix
**When** implemented
**Then** it does not attempt to change ownership or ACLs of the shared `/run/media/<username>` parent directory itself — mount succeeding and the volume being accessible to the invoking user is sufficient (declined porting `dyne/tomb`'s ACL-on-parent behavior)
```

Placed immediately after Story 7.1, before the (renumbered) Story 7.3.

### 4.3 `sprint-status.yaml` — story keys

```
  epic-7: in-progress
  7-1-presence-only-enrollment-up-only-mode: done
  7-2-mount-point-creation-resilient-to-pre-existing-ownership: backlog   # NEW
  7-3-touchless-enrollment-flag-precedence-no-up-mode: backlog           # was 7-2
  7-4-uv-capability-detection: backlog                                   # was 7-3
  7-5-interactive-unlocking-mode-menu: backlog                           # was 7-4
  epic-7-retrospective: optional
```

### 4.4 `sprint-status.yaml` — new action_items entries

```yaml
  - epic: 7
    action: "New Story 7.2 (mount-point creation resilient to pre-existing /run/media ownership) inserted between 7.1 and 7.2 (renumbered to 7.3) — udisks2/gvfs-managed machines hit EACCES creating the per-volume mount point under a root-owned /run/media/<user>, surfacing as the generic 'couldn't mount its filesystem' message. Root-caused live via manual mkdir/stat reproduction; fix shape (unconditionally privileged mkdir+chown on the leaf, no reliance on base.exists()) taken from dyne/tomb's proven fix for the identical conflict (their issue #461)."
    owner: "Amelia"
    status: open
  - epic: 7
    action: "Process gap found while root-causing the Story 7.2 mount bug: Story 1.9's own review record (1-9-mount-ux-and-ownership-hardening.md:54) marked a '[Review][Patch]' finding [x] and claimed it fixed ('gate the mkdir+chown on a uid stat comparison instead of base.exists()') — but git blame on the current code shows that fix was never actually implemented; the block is untouched since the original 9065e2c commit, and the cited follow-up commit (c005771) never touched src/adapters/exec/mod.rs. Same family as the epic-5/epic-6 self-reported-count watch items already on file (a story's own claim didn't match a from-scratch re-check), but this instance is worse: it's a claimed *code fix*, not just a count, and it silently regressed real user-facing behavior for over six weeks with no test catching it. Worth a retro discussion: should code-review 'fixed' patch findings require a diff citation (file:line) in the story doc, not just a checkbox, so this class of gap is mechanically checkable later?"
    owner: "unassigned"
    status: open
```

## 5. Implementation Handoff

**Scope: Minor.** Single adapter-layer fix, no architecture/PRD change, no new port surface.

- **This workflow (Correct Course):** updates `epics.md` and `sprint-status.yaml` only, per this proposal, once approved.
- **Developer agent (Amelia):** creates the Story 7.2 file (`bmad-create-story`) with full context from this proposal, then implements test-first (`bmad-dev-story`) — red/green on `create_mount_point`'s unconditionally-privileged path, then the fix.
- **No PM/Architect/PO escalation needed** — scope, root cause, and fix shape are already fully determined and agreed.

**Success criteria:** `create_mount_point` no longer depends on `base.exists()`/pre-existing ownership; a hardware-gated scenario simulating a root-owned `751 /run/media/<user>` (or run on an actual udisks2-managed machine) passes; existing Story 1.9 collision-retry and ownership-chown coverage stays green.
