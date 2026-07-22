---
id: SPEC-tomb-fido2
companions: []
sources: [../../brainstorming/brainstorm-tomb-fido2-2026-07-21/brainstorm-intent.md]
---

> **Canonical contract.** This SPEC and the files in `companions:` are the complete, preservation-validated contract for what to build, test, and validate. Source documents listed in frontmatter are for traceability only — consult them only if you need narrative rationale or prose color this contract intentionally omits.

# FIDO2-Exclusive LUKS2 Tomb CLI

> Working title `tomb-fido2` is a placeholder pending a permanent name — see Constraints. Referred to as "the tool" throughout this document.

## Why

This tool exists to serve one job: store rarely-accessed, highly sensitive material (a master GPG key, a password-manager backup) as reliably as a bank safe opened once a year in a crisis — without depending on any third party or external service. It is both a pain to solve (the multi-tool memorization burden of `cryptsetup` + `fido2-token` + `systemd-cryptenroll`, each with its own flags, is unusable under crisis stress) and a vision to realize (a FIDO2-native reimagining of dyne/tomb's core insight: a thin wrapper over standard, boring primitives that survives the tool's own death). TrueCrypt's abrupt 2014 shutdown is the cautionary reference — for this class of use case, the design bets on LUKS/dm-crypt and FIDO2 hmac-secret precisely because they are low-level, standard, and multi-vendor, never on anything only the tool itself understands.

## Capabilities

- **CAP-1**
  - **intent:** User can unlock a LUKS2 volume using a FIDO2 security key via the tool, with its filesystem mounted and ready to use in the same operation.
  - **success:** A user with zero FIDO2 familiarity unlocks and mounts a LUKS2 volume on first try, guided only by the tool's own prompts, with no external documentation consulted.

- **CAP-2**
  - **intent:** User can enroll an additional FIDO2 key as an alternate unlock method on an already-created tomb, using LUKS2's native multi-keyslot support (up to 32 slots).
  - **success:** After enrollment, the volume unlocks successfully with either the original key or the newly enrolled key.

- **CAP-3**
  - **intent:** User can revoke a single FIDO2 key's keyslot, removing it as a valid unlock method, unless doing so would remove the last remaining valid keyslot.
  - **success:** After revocation, the removed key no longer unlocks the volume, while other enrolled keys still do. Attempting to revoke the last remaining valid keyslot is blocked with a clear explanation, and the volume remains unlockable.

- **CAP-4**
  - **intent:** User drives creation, unlock, enrollment, revocation, closing, resizing, and dependency checking through one unified CLI, replacing direct use of `cryptsetup`, `fido2-token`, `systemd-cryptenroll`, `mkfs`, and `mount`/`umount`.
  - **success:** No workflow in v1 scope requires the user to invoke the underlying tools directly.

- **CAP-5**
  - **intent:** Tool guides the user through every interactive step (e.g. the key-touch moment) in plain language, assuming zero FIDO2 knowledge.
  - **success:** Prompts and error messages never assume FIDO2 familiarity (e.g. "Please touch your security key button," never "Awaiting UP").

- **CAP-6**
  - **intent:** Tool verifies all hard dependencies (LUKS2 FIDO2/hmac-secret support, required binaries, kernel features) before any operation begins.
  - **success:** On a missing dependency, the tool exits cleanly before starting the operation, with a detailed, actionable error message — never a mid-operation failure.

- **CAP-7**
  - **intent:** User can run any operation (create, unlock, enroll, revoke, close, resize) directly against raw physical block devices/partitions, not only file-backed loop devices.
  - **success:** The same CLI commands work unmodified against a raw partition path and a loop-mounted file, for every operation — a capability original Tomb never exposed.

- **CAP-8**
  - **intent:** User can create a brand-new tomb in one operation, in either of two target modes: (a) file-backed — user gives a destination path and a size, and the tool allocates the backing file itself at that size (no manual pre-creation step, cf. `tomb dig`), refusing outright if the destination already exists; or (b) device-backed — user gives an existing raw block device/partition path and an optional size (defaulting to the device's full capacity, or a smaller value to leave free space for later growth via CAP-10), and the tool refuses outright if the device already carries a LUKS2 header, otherwise requiring the user to explicitly confirm a shown wipe/data-loss warning before proceeding. Either way, the tool formats the target as LUKS2, creates a user-selected filesystem inside it, and bootstrap-enrolls the first FIDO2 key.
  - **success:** After creation, the new tomb unlocks and mounts successfully (CAP-1) using the newly enrolled key, with the chosen filesystem ready to use. For a file-backed tomb, the backing file exists at the requested destination sized as requested, and create aborts before touching anything if that path already existed. For a device-backed tomb, the used capacity matches the requested size (or the device's full capacity if none was given) and never exceeds the device's actual capacity; create aborts before touching anything if the device already carries a LUKS2 header, and otherwise proceeds only after the user explicitly confirms the shown wipe/data-loss warning.

- **CAP-9**
  - **intent:** User can close an unlocked tomb — unmount its filesystem and re-lock the LUKS2 volume — as the symmetric counterpart to CAP-1's unlock+mount.
  - **success:** After closing, the mount point is no longer accessible and the volume requires a FIDO2 key to unlock again.

- **CAP-10**
  - **intent:** User can grow an existing tomb's LUKS2 volume and filesystem to a larger size without recreating it or re-enrolling any FIDO2 keys.
  - **success:** After growing, the volume's usable capacity reflects the new size, all previously enrolled FIDO2 keys still unlock it, and no existing data is lost.

- **CAP-11**
  - **intent:** User can unlock and mount an existing tomb in read-only mode, so the LUKS2/dm-crypt mapping itself refuses writes at the block-device level, not merely the filesystem mount.
  - **success:** When read-only mode is requested, both the underlying mapper device and the mounted filesystem reject write attempts (including a later remount attempt), while a normal (non-read-only) unlock continues to allow writes as before.

## Constraints

- Standard, low-level, well-tested primitives only — LUKS/dm-crypt + FIDO2 hmac-secret. No proprietary formats, no single-vendor-maintained crypto.
- Filesystem operations (mkfs at creation, growfs at resize, mount/unmount) use only standard, well-known tools/syscalls for the user-selected filesystem type — no custom or tool-proprietary filesystem handling.
- Break-glass recoverability: the README must document manually unlocking and mounting the volume using only bare `cryptsetup`, `mount`/`umount`, and `fido2-token` commands, with zero dependency on the tool's own binary surviving.
- Zero-FIDO2-knowledge UX is mandatory across all prompts and errors, not just the happy path.
- Physical key presence at the exact moment of unlock is fixed, not configurable — the tool supports exactly what cryptsetup's FIDO2 token mode natively offers, no more.
- No fallback auth paths: FIDO2 is the exclusive unlock mechanism — no GPG or keyfile escape hatch.
- Must ship as a real compiled binary (Go or Rust), not a shell script.
- Tool stays entirely unaware of backup strategy: the README may recommend a 3-2-1-style backup of volumes/keys as a disclaimer, but no code or feature may touch backup concerns.
- Tool must actively prevent revoking the last remaining valid keyslot, a deliberate departure from raw cryptsetup (which permits this and allows a user to lock themselves out).
- Tool must make a best-effort attempt to avoid decrypted key material leaking or persisting in process memory; the specific mechanism (zeroing strategy, memory locking, etc.) is an architecture decision, not specified here.
- Read-only unlock (CAP-11) must refuse writes at both the LUKS2/dm-crypt mapping level and the filesystem mount level — a filesystem-level-only read-only mount (plain `mount -o ro` over a read-write dm-crypt mapping) does not satisfy this capability.
- Creating a file-backed tomb (CAP-8) never requires a pre-existing backing file — the tool allocates it at the given destination path and size as part of create, ruling out any workflow that expects the user to run `dd`/`fallocate`/`truncate` manually beforehand.
- A device-backed tomb (CAP-8) defaults to the target device/partition's full capacity when no size is given; a user-supplied size is accepted and may be smaller than the device's actual capacity (to leave free space for a later CAP-10 grow), but must never exceed it.
- Create refuses outright rather than overwriting: a file-backed destination that already exists, or a device-backed target that already carries a LUKS2 header, both abort the operation before any formatting happens.
- Device-backed create is inherently higher-risk than file-backed (the wrong device vs. a typo'd file path) — the tool must show an explicit warning describing the impending wipe and data loss and require the user's explicit confirmation before formatting any device target, even when no existing LUKS2 header was detected.
- The name `tomb-fido2` is a placeholder, not final branding — a permanent name is still pending. CLI/binary name, package/module name, user-facing strings, and on-disk metadata field names must not hardcode it or assume its permanence, so a future rename requires no redesign.

## Non-goals

- Remote or delegated unlock beyond what cryptsetup's native FIDO2 token mode offers.
- Post-quantum-readiness features.
- Shrinking an existing tomb — resize is grow-only for v1.

## Success signal

A user who has never touched the tool before can, in a real crisis, unlock and mount a LUKS2 volume (loop-file or raw partition) using only a physical FIDO2 key and the tool's own prompts — no external notes, no web search — then close it again when done. Independently, with the tool's own binary assumed gone, the same volume can still be unlocked, mounted, and closed by following the README's break-glass procedure using only stock `cryptsetup`, `mount`/`umount`, and `fido2-token`. Before any of that, the same user can create a brand-new tomb from scratch — either by giving a destination path and size and letting the tool allocate the backing file (refusing if that path already exists), or by pointing at a raw device/partition and optionally reserving free space for later growth (refusing if it already carries a LUKS2 header, and otherwise only after confirming an explicit wipe warning) — choosing a filesystem, formatting it, and enrolling their first FIDO2 key, entirely through the tool, and later grow it as their storage needs increase.

## Assumptions

- The Should-Have items from the source brainstorm (break-glass README procedure, 3-2-1 backup disclaimer) are folded into Constraints here rather than kept as separate capabilities, since they bend documentation/design decisions rather than describing testable tool behavior.
