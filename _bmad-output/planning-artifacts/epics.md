---
stepsCompleted: [step-01-validate-prerequisites, step-01-refresh-2026-07-22, step-02-design-epics, step-01-refresh-2026-07-22-b, step-03-epic-1-stories, step-03-epic-2-stories, step-03-epic-3-stories, step-03-create-stories, step-01-refresh-2026-07-27-epic4, step-02-design-epics-epic4, step-03-epic-4-stories, step-01-refresh-2026-08-08-epic6, step-02-design-epics-epic6, step-03-epic-6-stories, step-01-refresh-2026-09-10-epic7, step-02-design-epics-epic7, step-03-epic-7-stories]
inputDocuments:
  - _bmad-output/specs/spec-tomb-fido2/SPEC.md
  - _bmad-output/specs/spec-tomb-fido2/hooks.md
  - _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md
---

# FIDO2-Exclusive LUKS2 Tomb CLI — Epic Breakdown

> Working title `tomb-fido2` is a placeholder pending a permanent name — see SPEC.md Constraints. Referred to as "the tool" throughout this document.

## Overview

This document provides the complete epic and story breakdown for the tool, decomposing the requirements from the SPEC (acting as PRD-equivalent contract), and Architecture Spine into implementable stories. No UX design contract exists — this is a CLI-only tool.

## Requirements Inventory

### Functional Requirements

FR1: User can unlock a LUKS2 volume using a FIDO2 security key via the tool, with its filesystem mounted and ready to use in the same operation. (CAP-1)
FR2: User can enroll an additional FIDO2 key as an alternate unlock method on an already-created tomb, using LUKS2's native multi-keyslot support (up to 32 slots). (CAP-2)
FR3: User can revoke a single FIDO2 key's keyslot, removing it as a valid unlock method, unless doing so would remove the last remaining valid keyslot. (CAP-3)
FR4: User drives creation, unlock, enrollment, revocation, closing, resizing, and dependency checking through one unified CLI, replacing direct use of `cryptsetup`, `fido2-token`, `systemd-cryptenroll`, `mkfs`, and `mount`/`umount`. (CAP-4)
FR5: Tool guides the user through every interactive step (e.g. the key-touch moment) in plain language, assuming zero FIDO2 knowledge. (CAP-5)
FR6: Tool verifies all hard dependencies (LUKS2 FIDO2/hmac-secret support, required binaries, kernel features) before any operation begins. (CAP-6)
FR7: User can run any operation (create, unlock, enroll, revoke, close, resize) directly against raw physical block devices/partitions, not only file-backed loop devices. (CAP-7)
FR8: User can create a brand-new tomb in one operation, in either of two target modes: (a) file-backed — user gives a destination path and a size, and the tool allocates the backing file itself (no manual pre-creation step), refusing outright if the destination already exists; or (b) device-backed — user gives an existing raw device/partition and an optional size (defaults to full device capacity; a smaller value leaves headroom for a later resize), refusing outright if the device already carries a LUKS2 header, and otherwise requiring the user to explicitly confirm a wipe/data-loss warning before proceeding. Either way, the tool formats the target as LUKS2, creates the user-selected filesystem inside it, and bootstrap-enrolls the first FIDO2 key. (CAP-8)
FR9: User can close an unlocked tomb — unmount its filesystem and re-lock the LUKS2 volume — as the symmetric counterpart to CAP-1's unlock+mount. (CAP-9)
FR10: User can grow an existing tomb's LUKS2 volume and filesystem to a larger size without recreating it or re-enrolling any FIDO2 keys. (CAP-10)
FR11: User can unlock and mount an existing tomb in read-only mode, so the LUKS2/dm-crypt mapping itself refuses writes at the block-device level, not merely the filesystem mount. (CAP-11)
FR12: User can view technical info about a tomb, including its currently enrolled FIDO2 keys with their labels, through one info command, without needing to unlock it first. (CAP-12)
FR13: User can enroll a FIDO2 key with user-verification (fingerprint/PIN) required instead of touch-only presence, both at tomb creation's bootstrap enrollment and via the standalone enroll command. (CAP-13)
FR14: User can close every currently-open/unlocked tomb in one command. (CAP-14)
FR15: User can run an emergency command that closes every open tomb and forcibly clears any process holding a mount busy, firing immediately with no confirmation prompt. (CAP-15)
FR16: User can define per-tomb bind-hooks (auto bind-mount tomb-internal paths onto `$HOME`-relative paths on open) and an exec-hooks executable (run as the invoking user at open/close), with an option to skip hook processing for a given invocation. (CAP-16)
FR17: User gets real step-by-step progress messages during create and resize, naming each real stage as it begins and completes, replacing the Story 1.5 stopgap of one message before and one after the whole operation. (CAP-17)
FR18: User can supply a custom label for the first FIDO2 key enrolled during create's bootstrap step, mirroring enroll's existing required `--label` flag, instead of always getting the tool's default label. (CAP-18)
FR19: User can optionally have create scaffold example bind-hooks/exec-hooks template files into a new volume, so hooks are discoverable without consulting docs first. (CAP-19)
FR20: User can use one-letter shorthand aliases for each subcommand's primary flags, in addition to the existing long forms, across the whole CLI. (CAP-20)
FR21: Contributors and users see repository-hygiene badges (build status, license, latest release, MSRV) in the README header at a glance, with code-coverage and security-audit badges added once their underlying instrumentation exists. (CAP-21)
FR22: User can create/resize a volume using XFS or Btrfs as the filesystem, in addition to today's ext4-only support, with Btrfs always using `mkfs.btrfs --mixed` mode. (CAP-22)
FR23: User can re-run create against the same destination after a crash/interruption during a prior create attempt, and have the tool detect the partial attempt and restart it clean, rather than being left with an unrecoverable partial volume or a false "destination already exists" refusal. (CAP-23)
FR24: Tool prevents two simultaneous invocations against the same volume from racing past each other in a way that corrupts state or bypasses a safety guard, particularly the last-keyslot guard. (CAP-24)
FR25: Tool gives plain-language guidance when the FIDO2 device involved in an operation has a device-level PIN configured, detected proactively before the touch prompt, distinct from user-verification. (CAP-25)
FR26: User can enroll a FIDO2 key with presence-only unlock (no PIN/UV) via `--client-pin=false`, mapping to `--fido2-with-client-pin=BOOL`. (CAP-26)
FR27: User can enroll a FIDO2 key with the presence check itself disabled via `--user-presence=false`, mapping to `--fido2-with-user-presence=BOOL` — the weakest supported mode, requiring the token itself be configured to allow it. (CAP-27)
FR28: When enrolling (create's bootstrap step or standalone `enroll`) and none of the three FIDO2 flags (`--user-verification`, `--client-pin`, `--user-presence`) are explicitly passed, the tool presents an interactive menu offering UV / PIN+UP / UP / NO-UP, defaulting to UV when the token supports it, with unavailable or unverifiable options shown annotated rather than hidden. (CAP-28)

### NonFunctional Requirements

NFR1: Standard, low-level, well-tested primitives only — LUKS/dm-crypt + FIDO2 hmac-secret. No proprietary formats, no single-vendor-maintained crypto.
NFR2: Break-glass recoverability — the README must document manually unlocking and mounting the volume using only bare `cryptsetup`, `mount`/`umount`, and `fido2-token` commands, with zero dependency on the tool's own binary surviving.
NFR3: Zero-FIDO2-knowledge UX is mandatory across all prompts and errors, not just the happy path.
NFR4: Physical key presence at the exact moment of unlock is fixed, not configurable.
NFR5: No fallback auth paths — FIDO2 is the exclusive unlock mechanism; no GPG or keyfile escape hatch.
NFR6: Must ship as a real compiled binary (Rust), not a shell script.
NFR7: Tool stays entirely unaware of backup strategy — README may carry a 3-2-1-style disclaimer, but no code or feature may touch backup concerns.
NFR8: Tool must actively prevent revoking the last remaining valid keyslot.
NFR9: Tool must make a best-effort attempt to avoid decrypted key material leaking or persisting in process memory.
NFR10: Filesystem operations (`mkfs` at creation, `growfs` at resize, `mount`/`umount`) must use only standard, well-known tools/syscalls for the user-selected filesystem type — no custom or tool-proprietary filesystem handling.
NFR11: Read-only unlock must refuse writes at both the LUKS2/dm-crypt mapping level and the filesystem mount level — a filesystem-level-only read-only mount over a read-write dm-crypt mapping does not satisfy this.
NFR12: Close-all/slam must discover open tombs by live-querying system state only (`dmsetup`/`cryptsetup status`) — never a stored registry or lock file.
NFR13: Slam fires with zero confirmation prompt, by design — its emergency/panic-button framing overrides the general confirm-before-irreversible-action pattern used elsewhere (create's wipe warning, revoke's last-keyslot guard).
NFR14: Hooks introduce an arbitrary-code-execution risk surface — exec-hooks must always run unprivileged as the invoking user; hook files must pass regular-file/executable-bit/ownership/not-world-writable checks (hard error if failed, aborting the operation); bind-hooks entries must pass path-containment checks (tomb-root source, `$HOME` destination), skipped with a warning if failed, never silently applied.
NFR15: Create and resize must report distinct named-stage progress messages as each real stage occurs, not merely a single message before and after the whole operation.
NFR16: UV enrollment is a stronger verification mode of the existing FIDO2 mechanism, not a new auth path — must not violate the no-fallback-auth-paths constraint (NFR5).
NFR17: A device-backed create's confirmation-prompt exception for a marker-verified resume must stay consistent with the pre-existing refuse-exception for the same case (SPEC Constraints, 2026-08-08 addendum) — both carve-outs rest on the same "surviving marker is structural proof nothing of value occupies that device" reasoning.
NFR18: `mkfs.btrfs --mixed` is unconditional for every Btrfs volume this tool creates, a deliberate choice for this tool's small-cold-storage use case, not a size-threshold branch.
NFR19: The concurrent-invocation lock (`flock`, `LOCK_EX | LOCK_NB`) never blocks/serializes silently — on contention it returns immediately with a plain-language "another operation is already in progress" error.
NFR20: Read-only workflows (`info`, `unlock` including its read-only variant) never acquire the invocation lock — scope stays strictly limited to mutating workflows, per CAP-24's own success criterion.
NFR21: FIDO2 PIN-status detection is proactive — queried during device enumeration/preflight — and any resulting warning is always shown before the blocking touch/PIN subprocess call, never only after a failure.
NFR22: Enrolling in UP-only or NO-UP mode must print an explicit security warning naming the weaker guarantee, consistent with the existing enroll-time PIN-warning convention (AD-16).
NFR23: Combining the three FIDO2 flags (`--user-verification`, `--client-pin`, `--user-presence`) must resolve through one deterministic, documented precedence rule — no undefined/silent behavior for any combination.

### Additional Requirements

**Architecture / structure:**

- Hexagonal (ports & adapters) design: `domain` core (workflows `create`/`unlock`/`enroll`/`revoke`/`close`/`resize`, `preflight`, typed error enum) depends only on three trait-defined ports (`LuksBackend`, `Fido2Backend`, `FilesystemBackend`); `adapters::exec` implements those ports by shelling out to `cryptsetup`, `systemd-cryptenroll`, `fido2-token`, and standard filesystem tooling (`mkfs.ext4`, `resize2fs`, `mount`/`umount`); `cli` layer sits above domain and never touches a port directly. Ports carry AD-9's create-support methods: `LuksBackend::has_luks2_header`; `FilesystemBackend::path_exists`/`device_capacity`/`set_backing_file_size` (the last shared with AD-10's resize).
- AD-1: No crypto/FIDO2 protocol library linked directly — all LUKS2/FIDO2 operations happen via subprocess calls; the actual hmac-secret exchange is performed by systemd's `systemd-fido2` LUKS2 token plugin. (binds all CAP-1..11)
- AD-2: No side-channel state — per-key label and the tomb's filesystem type persisted as generic namespaced fields (`key_label`, `filesystem`), never prefixed with the product's own (placeholder) name (AD-13), on the same `systemd-fido2` token object `systemd-cryptenroll` creates, never a separate custom token type; no local sidecar file/db; no "known tombs" registry. `filesystem` is written once by `create` and read by `resize` to select the right growfs tool. **Open item:** whether cryptsetup's `systemd-fido2` token plugin tolerates these unknown extra JSON fields is unconfirmed — resolve with a throwaway `cryptsetup token export`/`import` spike before CAP-2/CAP-8 implementation; documented fallback is a second sibling token of a distinct custom type referencing the same keyslot if rejected. (binds all CAP-1..11, especially CAP-2/CAP-3/CAP-10)
- AD-3: Secret material (existing passphrase, FIDO2 PIN) never enters the tool's own process — those subprocess calls run with inherited/passthrough stdio and are always separate calls from non-secret information-gathering calls (e.g. `fido2-token -L`). **Bounded exception (CAP-8 create only):** `luksFormat` needs a seed keyslot and no user passphrase exists yet for a brand-new tomb, so `adapters::exec` generates one transient random passphrase in-process (`zeroize::Zeroizing` buffer), wiped immediately after the `luksOpen` call that consumes it, before `mkfs` runs — no other code path may hold a secret buffer. (binds CAP-1, CAP-2, CAP-8)
- AD-4: One shared `domain::preflight` check (now including the filesystem tooling AD-8 needs) runs as the first statement inside each `domain::workflows::*` function itself, identically for `create`, `close`, `resize`, and read-only `unlock` — none of the new workflows get a lighter gate than the original three. (binds CAP-6, transitively CAP-1/2/3/8/9/10/11)
- AD-5: Last-keyslot guard lives in the domain — "valid keyslot" = live LUKS2 keyslot with an associated `systemd-fido2` token, counted from live header state immediately before the revoke decision; abort if count ≤ 1; on revoke, token metadata removed first, keyslot second. `create`'s removal of its own transient bootstrap passphrase keyslot (AD-9) reuses this same guarded primitive rather than a separate unguarded removal path. (binds CAP-3, reused by CAP-8's bootstrap cleanup)
- AD-6: Scope fences enforced at the CLI surface — no command to enroll/unlock via non-FIDO2 method exists in code; no module/flag/prompt references backup/replication/sync; key-presence timing never configurable. The transient bootstrap passphrase (AD-9) is not an exception — never user-facing, never accepted as input, destroyed within the same workflow. (binds all CAP-1..11)
- AD-7: Testing strategy — `domain` workflows/guardrails unit-tested against one shared fake `LuksBackend`/`Fido2Backend`/`FilesystemBackend` implementation, run in default CI; a separate hardware-gated integration suite exercises real adapters + a real FIDO2 device + real filesystem tooling, excluded from default CI, run manually (`make test-hardware`). (binds all CAP-1..11)
- AD-8 (new): `FilesystemBackend` port — a third port alongside `LuksBackend`/`Fido2Backend`, with `mkfs`/`growfs`/`mount`/`umount` methods each parameterized by a `Filesystem` enum. v1's `adapters::exec` implements only `Filesystem::Ext4`; adding a second filesystem later is additive (new enum arm + adapter match arm), no port-signature change. `unlock` (CAP-1/CAP-11) calls `mount` after `LuksBackend::open` succeeds; `close` (CAP-9) calls `umount` **before** `LuksBackend::close` — reversed order fails on a still-busy mapping. (binds CAP-1, CAP-8, CAP-9, CAP-10, CAP-11)
- AD-9 (new): Create's two target modes, refuse/confirm gating, and bootstrap-keyslot lifecycle — `domain::workflows::create` runs `preflight` first (AD-4), then takes one `CreateTarget` enum argument — never a flat signature with optional/overlapping fields for both modes — so file-vs-device branching and the confirmation requirement are enforced at the type level. **File-backed** (`CreateTarget::File { path, size }`): if `FilesystemBackend::path_exists(path)` is true, refuse before touching anything; otherwise call `FilesystemBackend::set_backing_file_size(path, size)` to allocate the backing file itself (no `dd`/`fallocate`/`truncate` by the user first) — this branch carries no confirmation field, since there is nothing to confirm. **Device-backed** (`CreateTarget::Device { path, size: Option<u64>, confirmed: bool }`): if `LuksBackend::has_luks2_header(path)` is true, refuse before touching anything; otherwise resolve the size (a user-given size must not exceed `FilesystemBackend::device_capacity(path)`, rejected if it does; defaults to full capacity if omitted); refuses to proceed unless `confirmed` is `true`, regardless of whether a header was found — the `cli` layer always renders the wipe/data-loss warning and obtains explicit user confirmation before constructing this variant, with the refusal enforced in `domain` itself, not merely as a `cli`-side courtesy. Both modes then proceed into one atomic port method, `LuksBackend::bootstrap_format_and_open(path, size, filesystem) -> MapperHandle` (internally: generate transient passphrase → `luksFormat` → `luksOpen` → wipe passphrase, entirely inside `adapters::exec`, never crossing into `domain`). `domain` then calls `FilesystemBackend::mkfs`, enrolls the real FIDO2 key (writing `key_label`/`filesystem`, AD-2/AD-13), then removes the transient keyslot via AD-5's guarded primitive. **Known gap (deferred):** a crash between bootstrap and FIDO2 enrollment/cleanup leaves an unrecoverable tomb — v1 does not support resuming a partial create. (binds CAP-8)
- AD-10 (new): Resize ordering and grow-only enforcement — `domain::workflows::resize` reads current mapping/filesystem size live and rejects any request smaller than that size before calling any adapter. For a file-backed tomb, calls `FilesystemBackend::set_backing_file_size(path, new_size)` — the same primitive AD-9's create uses to allocate the file initially — before `LuksBackend::resize`; for a raw block device/partition, does not resize the partition table — errors clearly if not already large enough. Once space is confirmed: `LuksBackend::resize` first, then `FilesystemBackend::growfs` (fs type from the `filesystem` token field, AD-2) second. (binds CAP-10)
- AD-11 (new): Read-only unlock propagates atomically to both layers, with rollback on partial failure — a single `read_only: bool` is passed to both `LuksBackend::open` (`cryptsetup luksOpen --readonly`) and `FilesystemBackend::mount` (`mount -o ro`) in the same call; no code path sets one without the other. If `open` succeeds but `mount` fails, `unlock` closes the just-opened mapping before returning the error. (binds CAP-11)
- AD-12 (new): Deterministic mapping name and mountpoint discovery, no registry — the dm-crypt mapping name is derived deterministically from the canonicalized input device/file path (stable hash, prefixed with a fixed constant defined once in code — kept independent of the product's own name, per AD-13), never user-supplied/random/stored; any workflow needing it (`close`, `resize`, a later `unlock`) reconstructs the identical name from the path argument alone. The mount point is resolved via the kernel's own mount table (e.g. `findmnt`), never remembered. (binds CAP-1, CAP-9, CAP-10, CAP-11)
- AD-13 (new): Placeholder-name isolation — the product name is a placeholder pending a permanent choice, and no implementation identifier assumes its permanence: LUKS2 token JSON field names are generic (`key_label`, `filesystem`, `credential_id`, `created_at`, AD-2), never prefixed with the product name; AD-12's mapping-name prefix is a separate fixed constant, not derived from the product name; the CLI binary name and any user-facing product-name string are read from exactly one source (the Cargo package name), never duplicated as a string literal in `cli`, error messages, or elsewhere. (binds all CAP-1..11)
- AD-14 (Epic 4): Hooks (bind-hooks/exec-hooks) live on the existing `FilesystemBackend` port — no new port, per Rule of Three. Four new methods: `bind_mount(source, dest)` (privileged), `hook_file_metadata(path)`, `run_hook(path, args)` (deliberately unprivileged), `invoking_home_dir()`. `unlock`/`close` gain a `skip_hooks: bool`; `read_only: true` forces hook-skip unconditionally. On open: bind-hooks entries failing containment/existence are skipped with a warning; a failing exec-hooks guardrail check is a hard error that aborts via AD-11's rollback. On close: bind-hooks teardown (umount each destination) happens before the primary mount's umount, since a bind mount is never implicitly released by unmounting its source. (binds CAP-16)
- AD-15 (Epic 4): `info` reuses the existing keyslot-listing query — `domain::workflows::info` calls `preflight` then `LuksBackend::list_fido2_keyslots` directly, no new port method. v1 output is `key_label` per keyslot only (matching CAP-12's success criterion word for word); `credential_id`/`created_at`/`filesystem` stay internal. (binds CAP-12)
- AD-16 (Epic 4): User-verification is an enrollment-time-only parameter — `Fido2Backend::enroll_fido2_key` gains `user_verification: bool`, passed to `systemd-cryptenroll --fido2-with-user-verification=yes|no`; `enroll` and `create`'s bootstrap step both thread it. Resolved (web-verified against `systemd-cryptenroll(1)`): unlock-time behavior is read automatically from the stored credential by cryptsetup's `systemd-fido2` token plugin — `LuksBackend::open`/`resize` need no change. (binds CAP-13)
- AD-17 (Epic 4): Close-all/slam discover open tombs via new `LuksBackend::list_open_mappings()` — enumerates live dm-crypt mappings carrying AD-12's fixed mapping-name prefix (`dmsetup ls` filtered by prefix, cross-checked with `cryptsetup status` to recover `source_path`), never a lookup table. `close_all` loops over discovered mappings applying AD-8/AD-14's exact single-close sequence to each; one mapping's failure never aborts the batch — per-mapping outcomes are collected and all reported. `slam` is `close_all` with AD-18's escalation added per mapping. (binds CAP-14, CAP-15)
- AD-18 (Epic 4): Slam's busy-mount escalation — two new mechanism-only `FilesystemBackend` methods, `processes_using(mountpoint)` (`fuser -m`) and `signal_process(pid, Signal)` (`kill -s`); the escalation *policy* (SIGTERM → pause 1s → retry umount → SIGHUP → pause 1s → retry → SIGKILL → pause 1s → retry, moving to the next mapping once umount succeeds or no holders remain) lives in `domain::workflows::slam`, not the adapter, so it stays unit-testable against AD-7's fake ports. Zero confirmation prompt, by design (NFR13). (binds CAP-15)
- AD-19 (Epic 4): Progress reporting is a callback seam, never I/O inside `domain` — `create`/`resize` each take a `progress: &dyn Fn(Stage)` parameter, invoked synchronously at each real stage boundary; `Stage` is a typed, payload-free per-workflow enum (`CreateStage::{AllocatingBackingFile, FormattingLuks2, CreatingFilesystem, EnrollingFido2Key}`, `ResizeStage::{GrowingBackingFile, ResizingLuks2Mapping, GrowingFilesystem}`). `cli` supplies the closure and translates each stage via new `cli::ux::translate_stage`, the same translate-at-the-boundary shape as the existing `DomainError -> ux::translate` convention. (binds CAP-17)
- New external tool dependencies (Epic 4): `psmisc` (`fuser`, AD-18) and `util-linux` `kill` (AD-18) — added to preflight's checked binaries and the Nix devShell.
- AD-4 amended (Epic 6, CAP-22): `preflight` takes an `Option<Filesystem>` and checks only the mkfs/growfs toolchain the operation actually needs (`create`/`resize` pass the requested/existing type; other workflows pass `None`) — one shared gate, not a per-filesystem fork. AD-20's per-invocation lock is acquired immediately after this gate passes, never before it and never folded into it. (binds CAP-22, transitively CAP-24)
- AD-8 amended (Epic 6, CAP-22): `Filesystem` gains `Xfs` and `Btrfs` arms — `mkfs`'s adapter match arm runs `mkfs.xfs` or `mkfs.btrfs --mixed` (unconditional for every Btrfs volume, dropping the minimum viable size from Btrfs's standard-mode ~109 MiB floor to mixed-mode's ~16 MiB); `growfs` runs `xfs_growfs` or `btrfs filesystem resize`; both selected the same way ext4's tools already are, read from the `filesystem` token field (AD-2), never re-asked or sniffed. (binds CAP-22)
- AD-9 amended (Epic 6, CAP-18/19/23): `create` gains `key_label: Option<String>` (CAP-18, falls back to today's default when `None`) and `scaffold_hooks: bool` (CAP-19, default `false`) as siblings to `CreateTarget`, never embedded inside it. **File-backed:** an existing destination is no longer an automatic refusal — `LuksBackend::has_marker_token(path)` distinguishes a resumable partial attempt (proceed with no confirmation — the marker is structural proof nothing of value survived) from a genuine foreign file (refuse, unchanged). **Device-backed:** same marker check after `has_luks2_header`; on a marker-verified resume, size resolution against `device_capacity` is still mandatory (a shrunk device must still be caught) but the wipe-confirmation prompt is skipped regardless of `confirmed`'s value — a header-without-marker still refuses unconditionally. **Corrected real execution order** (fixing a documented drift): FIDO2 key enrollment happens **before** `mkfs` (the transient bootstrap passphrase is the only valid credential to authenticate `systemd-cryptenroll` with); if `scaffold_hooks` is true, the still-open mapper is mounted, `scaffold_hook_templates` writes `exec-hooks.example` (never the live `exec-hooks` name, so a scaffolded volume never fails AD-14's executable-bit guardrail) and a fully-commented `bind-hooks` file, then unmounted — this happens **after** `mkfs`, before final cleanup. Final cleanup order matters: the CAP-23 marker token is removed **first**, the transient bootstrap keyslot **second** (mirrors AD-5's own safe-ordering reasoning) — reversing this order would let a crash leave a marker on a fully-completed volume, causing a future `create` to silently wipe working data. `has_marker_token` closes AD-9's previously-documented "known gap" (a crash between bootstrap and enrollment leaving an unrecoverable volume). (binds CAP-18, CAP-19, CAP-23)
- AD-19 amended (Epic 6, CAP-19): `CreateStage` gains one payload-free variant, `ScaffoldingHookTemplates`, firing after `CreatingFilesystem` only when `scaffold_hooks: true`. The CAP-23 marker-token write is deliberately not its own stage — folded into the existing `FormattingLuks2` window, keeping progress granularity at real user-meaningful steps. (binds CAP-19)
- AD-20 (new, Epic 6): Concurrent-invocation guard — one new `FilesystemBackend::lock_target(path) -> Result<LockGuard, DomainError>` method, `flock(2)` (`LOCK_EX | LOCK_NB`) on an `O_CLOEXEC` fd (required so the locking fd never leaks into a spawned subprocess) opened against the target's canonicalized path (resolving the parent directory when the path itself doesn't yet exist, e.g. a genuinely fresh file-backed create — an accepted over-serialization trade-off, not a gap). Every mutating workflow (`create`, `enroll`, `revoke`, `close`, `resize`) acquires this lock as its second statement, immediately after `preflight` (AD-4) passes. `close_all`/`slam` acquire and drop a separate `LockGuard` per mapping, scoped to that single mapping's close attempt — never one lock for the whole batch, preserving AD-17's existing batch-isolation. `info` and `unlock` (including read-only) acquire no lock at all — explicitly out of scope, per CAP-24's own success criterion. On contention, returns immediately (never blocks) with a plain-language "another operation is already in progress" error; the kernel releases the lock automatically on process exit/crash, so there is no stale-lock state to detect or clean up, consistent with AD-2's no-side-channel-state posture. (binds CAP-24, transitively every mutating workflow)
- AD-21 (new, Epic 6): FIDO2 PIN-status detection — the existing `Fido2Device` struct (already shared by `LuksBackend::open`'s presence-wait loop and `Fido2Backend::enroll_fido2_key`'s device-selection resolvers) gains a `client_pin: bool` field, populated via one additional `fido2-token -I` call per enumerated device — no new `Fido2Backend` port method. `enroll_fido2_key`'s resolvers, once a specific device is selected, warn naming that device; `LuksBackend::open`'s presence-wait loop (which never selects a device — cryptsetup itself matches the token to whichever device answers) warns with a blanket list of every currently-enumerated PIN-required device instead. Both warnings print before the blocking touch/PIN subprocess call. The reactive wrong-PIN-retry warning is translated from that same call's stderr (see AD-3 amendment below), never from stdin/stdout. (binds CAP-25)
- AD-3 amended (Epic 6, CAP-25): stdin/stdout stay strictly inherited/passthrough for the actual secret-entry exchange, unchanged — but stderr on that same subprocess call may now be piped and parsed, for non-secret diagnostic text only (wrong-PIN retry-count warning, PIN-required hint). Nothing captured from stderr may ever contain or derive the secret itself; if a given authenticator's diagnostic text can't be cleanly distinguished from secret material, that call falls back to the original passthrough-only rule. Implementation must read stderr concurrently (separate thread, or non-blocking interleaved reads) rather than only after the child exits, to avoid a pipe-buffer deadlock on a long touch/PIN-blocking call. (binds CAP-25)
- New external tool dependencies (Epic 6): `xfsprogs` (`mkfs.xfs`, `xfs_growfs`, CAP-22) and `btrfs-progs` (`mkfs.btrfs`, `btrfs`, CAP-22) — added to preflight's checked binaries (only when the corresponding `Filesystem` variant is requested) and the Nix devShell. `cargo-llvm-cov` (coverage instrumentation) and `cargo-audit` (RustSec advisory scan, gating CI job) plus hosted Codecov (CAP-21) — CI/badge tooling only, not part of preflight or the runtime devShell.
- **Candidate (Epic 7, pending Architect formalization):** `Fido2Backend::enroll_fido2_key`/`fido2_verification_args` extended to accept the two new tri-state flags (`client_pin`, `user_presence`) alongside the existing `user_verification`, with AD-16's precedence precedent (UV forces `client-pin=false`) extended into a full precedence table covering all three, satisfying NFR23. (binds CAP-26, CAP-27)
- **Candidate (Epic 7, pending Architect formalization):** UV capability detection — parse the bare `uv` token from `fido2-token -I`'s CTAP2 `options:` line, same technique as the existing `parse_client_pin_configured` (clientPin). Three outcomes: capable, not-capable (absent/false), check-error — feeds CAP-28's default selection. (binds CAP-28)
- **Candidate (Epic 7, pending Architect formalization):** Interactive unlocking-mode menu (UV/PIN+UP/UP/NO-UP), triggered only when none of the three FIDO2 flags are explicitly passed, reusing the existing hand-rolled prompt style (`resolve_interactive_selection`, no external menu crate). Unavailable/unverifiable UV is shown as an annotated, unselectable row (not omitted) — discoverability over a shrinking menu — falling back to PIN+UP as the pre-selected default whenever UV can't be offered. (binds CAP-28, NFR22)
- Data/error conventions: per-key label + metadata stored as JSON in the LUKS2 token slot (`key_label`, `credential_id`, `created_at`, `filesystem`); domain errors are a typed enum (`thiserror`), translated to plain-language text only at the `cli` boundary; no persistent logging/telemetry (stderr-only, ephemeral); no config file.
- Stack: Rust 1.90.0, clap 4.6.4, serde/serde_json 1.0.229, thiserror 2.0.19, anyhow 1.0.104, zeroize 1.9.0 (AD-3/AD-9 bootstrap-passphrase wipe only); external: cryptsetup 2.8.6, systemd 261 (+FIDO2 +LIBCRYPTSETUP_PLUGINS), fido2-token/libfido2 1.17.0, e2fsprogs (`mkfs.ext4`/`resize2fs`, AD-8 v1 ext4-only), util-linux `blockdev` (AD-9 `device_capacity`); Linux only.
- Structural seed: `src/domain/{workflows/{create,unlock,enroll,revoke,close,resize}.rs, preflight.rs, errors.rs}`, `src/ports/{luks_backend,fido2_backend,filesystem_backend}.rs`, `src/adapters/exec/`, `src/cli/{main,ux}.rs`, `tests/{unit,hardware}/`, `Makefile`, `flake.nix`/`flake.lock`, `README.md`. No starter template — greenfield project.

**Tooling / DevOps:**

- AR-Dev1 (Nix devShell): `flake.nix`/`flake.lock` provides a reproducible dev environment (nixpkgs-unstable + flake-utils) bundling the Rust toolchain plus `cryptsetup`/`systemd`/`libfido2`, so contributors never install these system-wide. Epic 1 setup work.
- AR-Dev2 (CI workflow): A GitHub Actions workflow runs `make test` (mocked unit suite, per AD-7) on every push/PR. `make test-hardware` is explicitly excluded from this workflow — manual/local only.
- AR-Dev3 (Release automation): Two release-time GitHub Actions workflows — `release-please` (versioning/changelog from conventional commits) and `cargo-dist` (~0.32.x, builds and publishes release binaries to GitHub Releases across target platforms). Epic 1 setup work, alongside AR-Dev1/AR-Dev2, since it has no functional dependency on any capability story.
- AR-Dev4 (Packaging scope fence): Distro packaging (AUR, deb, etc.) beyond GitHub Releases prebuilt binaries + `cargo build --release` is explicitly deferred/out of scope for v1.

**Non-goals (for reference, not build scope):**

- Remote or delegated unlock beyond cryptsetup's native FIDO2 token mode.
- Post-quantum-readiness features.
- Shrinking an existing tomb — resize is grow-only for v1.

> Four items previously listed here as deferred are now in scope via Epic 6: FIDO2 PIN-required-device UX (FR25/CAP-25), the concurrent-invocation guard (FR24/CAP-24), resuming a partial/crashed `create` (FR23/CAP-23), and filesystem types beyond ext4 (FR22/CAP-22).
>
> One further item previously listed here as deferred is now in scope via Epic 7: exposing the rest of `systemd-cryptenroll`'s FIDO2 flag set during enrollment (FR26–FR28/CAP-26–28).

### UX Design Requirements

None — no UX design contract exists. This is a CLI-only tool; interaction/plain-language requirements are captured under FR5/NFR3 instead.

### FR Coverage Map

FR1: Epic 1 - Unlock a LUKS2 volume with filesystem mounted in the same operation
FR2: Epic 2 - Enroll an additional FIDO2 key on an already-created tomb
FR3: Epic 2 - Revoke a FIDO2 key's keyslot, guarded against last-keyslot lockout
FR4: Epic 1 (established) - Unified CLI dispatch, extended in Epics 2 & 3 for enroll/revoke/close/resize/read-only unlock
FR5: Epic 1 (established) - Plain-language prompts/errors, extended in Epics 2 & 3
FR6: Epic 1 (established) - Shared preflight dependency gate, extended in Epics 2 & 3
FR7: Epic 1 (established) - Raw device/loop file support, extended in Epics 2 & 3
FR8: Epic 1 - Create a brand-new tomb, file-backed or device-backed (format, filesystem, bootstrap-enroll first key)
FR9: Epic 3 - Close an unlocked tomb (unmount + re-lock)
FR10: Epic 3 - Grow an existing tomb's volume and filesystem
FR11: Epic 3 - Unlock and mount an existing tomb read-only
FR12: Epic 4 - View a tomb's enrolled FIDO2 keys via info, without unlocking
FR13: Epic 4 - Enroll a FIDO2 key requiring user-verification (fingerprint/PIN)
FR14: Epic 4 - Close every open tomb in one command (close-all)
FR15: Epic 4 - Emergency slam: close-all + busy-mount signal escalation, no confirmation
FR16: Epic 4 - Per-tomb bind-hooks/exec-hooks automation on open/close, skippable
FR17: Epic 4 - Named-stage progress reporting during create and resize
FR18: Epic 6 - Custom `--label` for create's bootstrap-enrolled key
FR19: Epic 6 - Optional hook-template scaffolding on create
FR20: Epic 6 - One-letter shorthand aliases across the CLI
FR21: Epic 6 - README repository-hygiene badges
FR22: Epic 6 - XFS/Btrfs filesystem support alongside ext4
FR23: Epic 6 - Crash-safe create resume via marker-token detection
FR24: Epic 6 - Concurrent-invocation guard against racing mutating workflows
FR25: Epic 6 - Proactive FIDO2 device-PIN guidance
FR26: Epic 7 - Enroll a FIDO2 key with presence-only unlock (`--client-pin=false`)
FR27: Epic 7 - Enroll a FIDO2 key with the presence check itself disabled (`--user-presence=false`)
FR28: Epic 7 - Interactive unlocking-mode menu (UV/PIN+UP/UP/NO-UP) when no FIDO2 flag is passed

## Epic List

### Epic 1: Create & Open a Tomb (Foundation)
Users can create a brand-new tomb from scratch — file-backed (the tool allocates the backing file itself) or device-backed (an existing raw device/partition, with mandatory wipe confirmation) — formatting it as LUKS2, creating the chosen filesystem inside it, and bootstrap-enrolling the first FIDO2 key — then unlock it with the filesystem mounted and ready to use, all through the tool's own CLI with zero FIDO2 knowledge required. This epic also stands up the project foundation (Nix devShell, CI, release automation) and the shared infrastructure every later epic depends on: the `LuksBackend`/`Fido2Backend`/`FilesystemBackend` ports, the `domain::preflight` gate, deterministic mapping-name/mountpoint discovery (AD-12), and the CLI/UX translation boundary. Each capability story (Create file-backed, Create device-backed, Unlock) wires and exposes its own CLI subcommand incrementally as it's built; the final story in this epic consolidates full `--help` coverage and audits plain-language error translation across all of them.
**FRs covered:** FR8, FR1, FR4, FR5, FR6, FR7 (established)

### Epic 2: Manage Tomb Access (Key Lifecycle)
Users can enroll an additional FIDO2 key as a backup unlock method on an already-created tomb, and revoke a single key's access when it's lost or compromised — with a hard guarantee that they can never revoke their way into a locked-out volume.
**FRs covered:** FR2, FR3

### Epic 3: Tomb Lifecycle & Advanced Access
Users can close an unlocked tomb (unmount + re-lock) as the clean counterpart to Epic 1's unlock, grow an existing tomb's capacity without recreating it or re-enrolling keys, and unlock a tomb read-only when they only need to inspect its contents safely.
**FRs covered:** FR9, FR10, FR11

### Epic 4: Advanced Operations & Automation
Users can inspect a tomb's enrolled keys without unlocking it, enroll keys with stronger fingerprint/PIN verification, manage every open tomb in bulk (routine close-all or panic-button slam), automate per-tomb setup/teardown via bind- and exec-hooks, and see real progress as create/resize actually happen — turning the tool from single-tomb basics into something usable for someone managing several tombs under real operational and emergency conditions.
**FRs covered:** FR12, FR13, FR14, FR15, FR16, FR17

### Epic 5: Rebrand to Hypogaol
Users and contributors see the project consistently as Hypogaol everywhere it presents itself — package, binary, repository, README, and CLI banner — while the codebase's internal vocabulary moves from the placeholder-era "tomb" to the generic, brand-independent "volume." No functional behavior changes; see `sprint-change-proposal-2026-08-02.md` for full impact analysis and rationale.
**FRs covered:** None — non-functional rename/rebrand, orthogonal to the SPEC's FR list. Triggered by the 2026-08-02 naming brainstorm (`_bmad-output/brainstorming/brainstorm-project-naming-2026-08-02/`).

### Epic 6: Volume Resilience, Filesystem Choice & Everyday Polish
Users get a more resilient, flexible, and ergonomic tool: create survives a crash/interruption and resumes cleanly instead of leaving an unrecoverable partial volume or a false refusal, users choose XFS or Btrfs alongside ext4, label their first key at creation time, and optionally scaffold example hook files so hooks are discoverable without reading docs; two invocations against the same volume can no longer race past a safety guard; a FIDO2 device with a PIN configured warns the user before the touch prompt, not after a confusing failure; and both end users (one-letter flag shorthand across the whole CLI) and contributors/evaluators (README health badges) get quality-of-life polish. No new port or architectural layer — every capability slots onto Epics 1-4's existing `create`/`FilesystemBackend`/`Fido2Backend` surface.
**FRs covered:** FR18, FR19, FR20, FR21, FR22, FR23, FR24, FR25

### Epic 7: FIDO2 Unlocking-Behavior Flags & Interactive Menu
Users can enroll a FIDO2 key using any of systemd-cryptenroll's remaining unlock-behavior flags — presence-only (no PIN/UV) via `--client-pin`, or the weakest touch-free mode via `--user-presence` — alongside the existing `--user-verification` flag, giving four selectable unlocking modes in total (UV, PIN+UP, UP, NO-UP). When enrolling without specifying any of the three FIDO2 flags, the tool presents an interactive menu so regular users can choose a mode without memorizing flags, and newcomers can discover modes they didn't know existed — defaulting to UV when the connected token supports it, annotating (not hiding) any mode it can't offer, and warning clearly wherever a weaker mode is chosen. This closes out the "expose the full FIDO2 flag set" item previously carried as a non-goal. No new port or architectural layer — slots onto Epic 4's existing FIDO2 enrollment surface (`Fido2Backend::enroll_fido2_key`, `fido2_verification_args`).
**FRs covered:** FR26, FR27, FR28

## Epic 1: Create & Open a Tomb (Foundation)

Users can create a brand-new tomb from scratch — file-backed (the tool allocates the backing file itself) or device-backed (an existing raw device/partition, with mandatory wipe confirmation) — formatting it as LUKS2, creating the chosen filesystem inside it, and bootstrap-enrolling the first FIDO2 key — then unlock it with the filesystem mounted and ready to use, all through the tool's own CLI with zero FIDO2 knowledge required. This epic also stands up the project foundation (Nix devShell, CI, release automation) and the shared infrastructure every later epic depends on: the `LuksBackend`/`Fido2Backend`/`FilesystemBackend` ports, the `domain::preflight` gate, deterministic mapping-name/mountpoint discovery (AD-12), and the CLI/UX translation boundary. Each capability story (Create file-backed, Create device-backed, Unlock) wires and exposes its own CLI subcommand incrementally as it's built; the final story in this epic consolidates full `--help` coverage and audits plain-language error translation across all of them.

### Story 1.1: Project Scaffolding & Nix DevShell

As a contributor,
I want a reproducible Nix devShell environment,
So that I can build and test the tool without installing cryptsetup/systemd/libfido2 system-wide.

**Acceptance Criteria:**

**Given** a fresh clone of the repository
**When** I run `nix develop`
**Then** a devShell activates providing the pinned Rust toolchain plus `cryptsetup`, `systemd`, and `libfido2` on PATH
**And** `cargo build` succeeds inside the devShell with no additional system-wide package installation

**Given** `flake.lock`
**When** the devShell is entered on a different machine
**Then** the same pinned nixpkgs-unstable revision is used, so the environment is reproducible across machines

**Given** the greenfield structural seed (`src/domain/{workflows,preflight.rs,errors.rs}`, `src/ports/*`, `src/adapters/exec/`, `src/cli/*`, `tests/{unit,hardware}/`)
**When** the scaffolding is created
**Then** the crate compiles (`cargo build`) with stub module bodies and no logic yet

### Story 1.2: CI Runs the Mocked Unit Test Suite

As a contributor,
I want CI to automatically run the mocked unit test suite on every push/PR,
So that I get fast feedback without needing physical FIDO2 hardware.

**Acceptance Criteria:**

**Given** a GitHub Actions workflow triggered on push/PR
**When** the workflow runs
**Then** it executes `make test` (the mocked unit suite, AD-7) inside the Nix devShell
**And** a failing `make test` fails the workflow/check

**Given** the same workflow
**When** it runs
**Then** `make test-hardware` is explicitly excluded — never runs in CI, hardware-gated and manual-only

### Story 1.3: Release Automation

As a maintainer,
I want versioned changelog generation and cross-platform release binaries published automatically,
So that users can download a ready-to-run binary without me manually cutting each release.

**Acceptance Criteria:**

**Given** commits in conventional-commit format merged to main
**When** release-please runs
**Then** it proposes/maintains a release PR with version bump and changelog derived from those commits

**Given** a release-please release is merged/tagged
**When** cargo-dist's workflow runs
**Then** it builds and publishes release binaries to GitHub Releases across the target platforms

**Given** this release tooling
**When** checking scope
**Then** distro packaging (AUR, deb, etc.) beyond GitHub Releases prebuilt binaries and `cargo build --release` is explicitly out of scope for v1

### Story 1.4: Dependency Preflight Check

As a user,
I want the tool to verify all hard dependencies before starting any operation,
So that I get a clear, actionable error before anything is touched, never a mid-operation failure.

**Acceptance Criteria:**

**Given** all hard dependencies present (LUKS2 FIDO2/hmac-secret support, required binaries, kernel/hidraw features)
**When** any `domain::workflows::*` function begins
**Then** `domain::preflight` runs as its first statement, passes, and the workflow proceeds

**Given** a required binary or kernel feature is missing
**When** the user runs any operation
**Then** the tool exits cleanly before starting the operation, with a detailed, actionable error naming the missing dependency
**And** no mutating action (LUKS format/open, filesystem changes) occurs

**Given** `preflight`'s implementation
**When** it is invoked
**Then** it is defined once in `domain::preflight` and called identically inside `create`, `unlock` (including read-only), `close`, and `resize`
**And** none of these workflows get a lighter gate than the others

### Story 1.5: Create a File-Backed Tomb

As a user with no prior FIDO2 experience,
I want to create a new tomb by giving a destination path and a size,
So that I don't need to manually pre-allocate a backing file before creating my tomb.

**Acceptance Criteria:**

**Given** a destination path that does not yet exist and a size
**When** I run the create command in file mode
**Then** the tool allocates the backing file at that path sized as requested (`FilesystemBackend::set_backing_file_size`), formats it as LUKS2, creates the user-selected filesystem, and bootstrap-enrolls my first FIDO2 key
**And** the resulting LUKS2 volume is independently unlockable via bare `cryptsetup`/`fido2-token` (break-glass path) using the newly enrolled key, with the chosen filesystem mountable and ready to use

**Given** a destination path that already exists
**When** I run the create command in file mode
**Then** the tool refuses before touching anything — no file overwritten, no formatting attempted — with a clear message

**Given** the create workflow internally
**When** it runs
**Then** it generates a transient random passphrase in a `zeroize::Zeroizing` buffer to seed `luksFormat`/`luksOpen`, wipes it immediately after use and before `mkfs` runs, and removes the transient bootstrap keyslot via the guarded last-keyslot-safe removal primitive once the real FIDO2 key is enrolled
**And** the enrolled FIDO2 key's metadata is written as generic token fields `key_label`/`filesystem` — never prefixed with the product's placeholder name

### Story 1.6: Create a Device-Backed Tomb

As a user,
I want to create a new tomb on an existing raw device or partition, optionally reserving free space,
So that I can use the tool directly against physical storage without an intermediate file.

**Acceptance Criteria:**

**Given** a device/partition path with no existing LUKS2 header and no size given
**When** I run the create command in device mode and confirm the wipe/data-loss warning
**Then** the tool defaults to the device's full capacity, formats it as LUKS2, creates the filesystem, and bootstrap-enrolls my first FIDO2 key

**Given** a size smaller than the device's actual capacity
**When** I confirm the wipe warning and proceed
**Then** the tool uses exactly the requested size, leaving the remaining capacity free for a later resize/grow

**Given** a size argument larger than the device's actual capacity
**When** I run the create command in device mode
**Then** the tool refuses with a clear error before touching anything

**Given** a device/partition that already carries a LUKS2 header
**When** I run the create command in device mode
**Then** the tool refuses before touching anything, regardless of whether I attempt to confirm the wipe warning

**Given** a device/partition with no existing LUKS2 header
**When** I run the create command in device mode without confirming the wipe/data-loss warning
**Then** the tool refuses to proceed — confirmation is mandatory for every device-backed create, not just when a header is detected

### Story 1.7: Unlock and Mount a Tomb

As a user with no FIDO2 experience,
I want to unlock an existing tomb with my FIDO2 key and have it mounted automatically,
So that I can access its contents in one guided step.

**Acceptance Criteria:**

**Given** an existing tomb (loop-backed file or raw device) with at least one enrolled FIDO2 key
**When** I run the unlock command and touch my key when prompted
**Then** the LUKS2 volume opens via `LuksBackend::open`, the mounted filesystem becomes accessible at a discoverable mount point (via the kernel's mount table), and prompts use plain language assuming zero FIDO2 knowledge

**Given** the same unlock command
**When** run against a raw device/partition path instead of a loop-file path
**Then** the identical command works unmodified — no different flags or behavior branch based on target type

**Given** the mapping name needed to open the volume
**When** unlock runs
**Then** it derives the dm-crypt mapping name deterministically from the canonicalized device/file path via the single shared helper — never user-supplied, random, or stored

### Story 1.8: Unified CLI Dispatch & Plain-Language Errors

As a user,
I want to drive create and unlock through one CLI with prompts/errors in plain language,
So that I never need to fall back to cryptsetup/fido2-token flags directly, even when something goes wrong.

> **Sequencing note:** Stories 1.5–1.7 (Create file-backed, Create device-backed, Unlock) each wire and expose their own CLI subcommand with baseline error handling as they're implemented — none of them are blocked waiting on this story. This story is the consolidation pass: it finalizes complete `--help` coverage across the Epic 1 subcommands and audits every error path accumulated so far for consistent plain-language translation at the `cli::ux` boundary.

**Acceptance Criteria:**

**Given** the CLI
**When** I run `--help` or a subcommand
**Then** `create` (with its file/device target flags) and `unlock` are both available as first-class subcommands of one binary

**Given** any domain error surfaced by create or unlock (missing dependency, path-exists refusal, LUKS2-header refusal, missing wipe confirmation, FIDO2 touch timeout, etc.)
**When** the CLI displays it
**Then** the message is translated to plain language at the `cli::ux` boundary (e.g. "Please touch your security key," never "Awaiting UP") — no internal jargon leaks to the user

**Given** the `cli` layer
**When** it calls into `domain`
**Then** it never touches a `LuksBackend`/`Fido2Backend`/`FilesystemBackend` port directly — all port access goes through `domain::workflows::*`

## Epic 2: Manage Tomb Access (Key Lifecycle)

Users can enroll an additional FIDO2 key as a backup unlock method on an already-created tomb, and revoke a single key's access when it's lost or compromised — with a hard guarantee that they can never revoke their way into a locked-out volume.

### Story 2.1: Enroll an Additional FIDO2 Key

As a user with an already-created tomb,
I want to enroll an additional FIDO2 key as an alternate unlock method,
So that I have a backup way to unlock my tomb if I lose my primary key.

**Acceptance Criteria:**

**Given** an existing tomb with one enrolled FIDO2 key
**When** I run the enroll command with a second physical key and touch it when prompted
**Then** a new LUKS2 keyslot is created and a `systemd-fido2` token is written with generic `key_label`/`credential_id`/`created_at` metadata
**And** these fields are never prefixed with the product's placeholder name

**Given** the tomb now has two enrolled keys
**When** I unlock with either the original or the newly enrolled key
**Then** the volume unlocks successfully with either one

**Given** LUKS2's native multi-keyslot support (up to 32 slots)
**When** enrolling
**Then** the operation works within that limit
**And** enroll runs `domain::preflight` first like every other workflow

**Given** the enroll workflow needs an existing-passphrase or FIDO2 PIN prompt
**When** any such secret entry occurs
**Then** it runs with inherited/passthrough stdio, never captured by the tool's own process
**And** it is always a separate subprocess call from any non-secret credential-id lookup (e.g. `fido2-token -L`)

**Given** an existing tomb on a raw device/partition instead of a loop-backed file
**When** I run the enroll command
**Then** the identical command works unmodified — enroll makes no branching decision based on target type, consistent with the deterministic mapping-name/mountpoint discovery (AD-12)

### Story 2.2: Revoke a FIDO2 Key, Guarded Against Last-Keyslot Lockout

As a user,
I want to revoke a single FIDO2 key's keyslot,
So that a lost or compromised key stops being able to unlock my tomb.

**Acceptance Criteria:**

**Given** a tomb with two or more valid keyslots (each with an associated `systemd-fido2` token)
**When** I run revoke targeting one key
**Then** its token metadata is removed first, then its keyslot
**And** afterward that key no longer unlocks the volume while other enrolled keys still do

**Given** a tomb with only one valid keyslot remaining
**When** I run revoke targeting that last key
**Then** the tool aborts with a clear explanation before touching anything
**And** the volume remains unlockable

**Given** the "valid keyslot" count
**When** revoke evaluates it
**Then** it counts live LUKS2 header state immediately before the decision, never a cached or prior view
**And** a stale token pointing at an already-gone keyslot is never counted as live

**Given** the revoke command
**When** I target a key that isn't enrolled
**Then** the tool reports a clear error rather than silently succeeding or crashing

**Given** an existing tomb on a raw device/partition instead of a loop-backed file
**When** I run the revoke command
**Then** the identical command works unmodified — revoke makes no branching decision based on target type

## Epic 3: Tomb Lifecycle & Advanced Access

Users can close an unlocked tomb (unmount + re-lock) as the clean counterpart to Epic 1's unlock, grow an existing tomb's capacity without recreating it or re-enrolling keys, and unlock a tomb read-only when they only need to inspect its contents safely.

### Story 3.1: Close an Unlocked Tomb

As a user,
I want to close an unlocked tomb,
So that its filesystem is unmounted and the LUKS2 volume is re-locked, as the symmetric counterpart to unlock.

**Acceptance Criteria:**

**Given** an unlocked, mounted tomb
**When** I run the close command
**Then** the filesystem is unmounted first, then the LUKS2 mapping is closed
**And** reversing that order would fail on a still-busy mapping

**Given** close needs to find the mapping/mount point
**When** it runs
**Then** it reconstructs the deterministic mapping name from the path argument via the shared helper (no registry)
**And** resolves the mount point via the kernel's mount table

**Given** closing succeeds
**When** I check afterward
**Then** the mount point is no longer accessible and the volume requires a FIDO2 key to unlock again

**Given** close
**When** it runs
**Then** `domain::preflight` runs first, like every other workflow

**Given** an unlocked, mounted tomb backed by a raw device/partition instead of a loop-backed file
**When** I run the close command
**Then** the identical command works unmodified — close makes no branching decision based on target type

### Story 3.2: Grow an Existing Tomb's Capacity

As a user,
I want to grow an existing tomb's volume and filesystem to a larger size,
So that I can increase my storage without recreating the tomb or re-enrolling any FIDO2 keys.

**Acceptance Criteria:**

**Given** an existing tomb and a new size larger than its current size
**When** I run resize
**Then** for a file-backed tomb the backing file is grown first (the same `set_backing_file_size` primitive create uses), then the LUKS2 mapping is resized, then the filesystem is grown — in that order

**Given** a raw device/partition target
**When** I run resize
**Then** the tool does not resize the partition table — it errors clearly if the partition isn't already large enough

**Given** a requested size smaller than the current size
**When** I run resize
**Then** the tool rejects the request before calling any adapter — resize is grow-only

**Given** resize completes
**When** I check afterward
**Then** the volume's usable capacity reflects the new size, all previously enrolled FIDO2 keys still unlock it, and no existing data is lost

**Given** the filesystem type needed for growfs
**When** resize runs
**Then** it reads it from the `filesystem` token field written at create time — never re-asked of the user or sniffed via `blkid`

### Story 3.3: Unlock a Tomb Read-Only

As a user,
I want to unlock and mount an existing tomb in read-only mode,
So that I can inspect its contents without risking any writes, at both the block-device and filesystem level.

**Acceptance Criteria:**

**Given** an existing tomb
**When** I run unlock with the read-only flag
**Then** a single `read_only` bool is passed to both the LUKS2 open call (`--readonly`) and the mount call (`-o ro`) in the same operation
**And** never one without the other

**Given** a read-only unlock
**When** mounted
**Then** both the underlying mapper device and the mounted filesystem reject write attempts, including a later remount attempt

**Given** the read-only `luksOpen` succeeds but the subsequent mount fails
**When** that happens
**Then** the tool closes the just-opened mapping before returning the error
**And** no dangling open mapper is left behind

**Given** a normal (non-read-only) unlock
**When** I run it
**Then** it continues to allow writes as before

**Given** an existing tomb backed by a raw device/partition instead of a loop-backed file
**When** I run unlock with the read-only flag
**Then** the identical command works unmodified — read-only unlock makes no branching decision based on target type

## Epic 4: Advanced Operations & Automation

Users can inspect a tomb's enrolled keys without unlocking it, enroll keys with stronger fingerprint/PIN verification, manage every open tomb in bulk (routine close-all or panic-button slam), automate per-tomb setup/teardown via bind- and exec-hooks, and see real progress as create/resize actually happen — turning the tool from single-tomb basics into something usable for someone managing several tombs under real operational and emergency conditions.

### Story 4.1: View a Tomb's Enrolled Keys (Info)

As a user,
I want to view a tomb's technical info including its enrolled FIDO2 keys and labels,
So that I can check what's enrolled without unlocking the tomb.

**Acceptance Criteria:**

**Given** an existing tomb
**When** I run the info command against it
**Then** `domain::preflight` runs first, like every other workflow
**And** it lists each currently enrolled FIDO2 keyslot with its `key_label`, without performing any unlock/open call

**Given** the info output
**When** displayed
**Then** it shows `key_label` per keyslot only — `credential_id`, `created_at`, and `filesystem` stay internal, not part of v1's info output

**Given** an existing tomb on a raw device/partition instead of a loop-backed file
**When** I run info
**Then** the identical command works unmodified

### Story 4.2: Real Progress Reporting for Create & Resize

As a user,
I want to see a distinct message for each real stage of create and resize as it happens,
So that I have visibility into a long-running operation instead of one message before and after.

**Acceptance Criteria:**

**Given** I run create
**When** it executes
**Then** I see a distinct message as each real stage begins, in order: allocating the backing file, formatting as LUKS2, creating the filesystem, enrolling the FIDO2 key
**And** no stage's message carries data beyond naming which stage is running

**Given** I run resize
**When** it executes
**Then** I see a distinct message for each real stage in order: growing the backing file, resizing the LUKS2 mapping, growing the filesystem

**Given** resize on a raw device/partition target
**When** it executes
**Then** only the two applicable stages fire (resizing the LUKS2 mapping, growing the filesystem) — no "growing the backing file" stage, since that only applies to file-backed tombs

**Given** progress reporting
**When** it's implemented
**Then** `domain` performs no direct I/O for these messages — it invokes a callback, and `cli` is what actually translates and prints text

### Story 4.3: Enroll a FIDO2 Key with User-Verification

As a user,
I want to enroll a FIDO2 key requiring user-verification (fingerprint/PIN),
So that unlocking with this key demands proof of physical identity beyond mere touch.

**Acceptance Criteria:**

**Given** an already-created tomb
**When** I run enroll with the user-verification flag
**Then** the new key is enrolled via `systemd-cryptenroll --fido2-with-user-verification=yes`
**And** unlocking later with that key requires the device's own fingerprint/PIN check, not touch alone

**Given** I run enroll without the flag
**When** it completes
**Then** the key continues to unlock with touch alone, unchanged from Epic 2 behavior

**Given** I create a brand-new tomb with the user-verification flag set on its bootstrap enrollment
**When** creation completes
**Then** the first key enrolled is UV-required, same as a standalone enroll would produce

**Given** a UV-enrolled key
**When** unlock or resize runs
**Then** no change is needed to the open call itself — cryptsetup's `systemd-fido2` token plugin reads the UV requirement from the stored credential automatically

### Story 4.4: Per-Tomb Bind-Hooks & Exec-Hooks Automation

As a user,
I want to define per-tomb bind-hooks and an exec-hooks executable,
So that opening and closing a tomb also carries out my own automation (e.g. bind-mounting my `.gnupg` into `$HOME`), without running separate commands.

**Acceptance Criteria:**

**Given** a tomb with a `bind-hooks` file listing valid tomb-root-to-`$HOME`-relative mappings
**When** I open it
**Then** each valid mapping is bind-mounted onto its `$HOME`-relative destination after the primary mount succeeds

**Given** a `bind-hooks` entry whose source or destination path doesn't exist, or that uses `..`/an absolute path to escape the tomb root or `$HOME`
**When** open runs
**Then** that entry is skipped with a warning — the rest of open continues normally, this is not a hard failure

**Given** a tomb with an `exec-hooks` file present
**When** I open it
**Then** the tool verifies it's a regular file (not a symlink), has the executable bit set, is owned by the invoking user or root, and is not world-writable, before running it with `open <mountpoint>` as the invoking user, never elevated

**Given** `exec-hooks` fails any of those guardrail checks
**When** open runs
**Then** it's a hard error that aborts the whole open — rolling back (closing the just-opened mapping, unmounting first if the primary mount had already succeeded) rather than leaving a partially set-up tomb mounted

**Given** a tomb with hooks configured
**When** I close it
**Then** `exec-hooks` runs first with `close <mountpoint> <tomb-name> <loopback-device> <mapper-device>`, then each still-mounted bind-hooks destination is unmounted, then the primary mountpoint, then the LUKS2 mapping is closed — in that order

**Given** I pass the skip-hooks flag, or unlock with read-only
**When** I open or close the tomb
**Then** neither bind-hooks nor exec-hooks runs at all — read-only unlock forces this regardless of the flag, since hooks never apply to read-only unlock

### Story 4.5: Close Every Open Tomb (Close-All)

As a user,
I want to close every currently open/unlocked tomb in one command,
So that I don't have to close them one by one when I'm done using several.

**Acceptance Criteria:**

**Given** multiple tombs currently open/unlocked
**When** I run close-all
**Then** the tool discovers them by enumerating live dm-crypt mappings carrying the tool's fixed mapping-name prefix (`dmsetup ls`, cross-checked with `cryptsetup status`) — never a stored registry
**And** applies the same sequence as a single close (hooks, then bind-hooks teardown, then primary unmount, then LUKS2 close) to each discovered mapping

**Given** close-all is running against several tombs
**When** one tomb's close fails partway (e.g. still busy)
**Then** it continues on to the remaining tombs rather than aborting the whole batch
**And** reports every failure alongside every success at the end

**Given** no tombs are currently open
**When** I run close-all
**Then** it completes cleanly, reporting nothing to close

**Given** close-all
**When** it runs
**Then** `domain::preflight` runs first, like every other workflow, and the same `skip_hooks` flag threads uniformly to every mapping in the batch

### Story 4.6: Emergency Slam

As a user,
I want an emergency command that force-closes every open tomb immediately with no confirmation,
So that in a genuine crisis I can clear everything blocking unmount without being asked to confirm anything.

**Acceptance Criteria:**

**Given** one or more open tombs, with at least one mount currently busy (a process holding it open)
**When** I run slam
**Then** it discovers open tombs the same way close-all does, and for a busy mount signals every holding process `SIGTERM`, pauses briefly, retries the close; if still busy escalates to `SIGHUP`, pauses, retries; if still busy escalates to `SIGKILL`, pauses, retries
**And** moves on to the next mapping once the close succeeds or no holding process remains, whichever comes first

**Given** slam
**When** it runs
**Then** it fires with zero confirmation prompt — unlike every other mutating command in the tool

**Given** a mapping with hooks configured
**When** slam processes it
**Then** the hooks step (exec-hooks/bind-hooks teardown) runs exactly once at the start of that mapping's close attempt, never re-run across escalation rounds

**Given** slam is processing several open tombs and one never clears (a process keeps re-acquiring the mount)
**When** that happens
**Then** it's reported as that one mapping's failure, without blocking slam from completing the rest of the batch

## Epic 5: Rebrand to Hypogaol

Users and contributors see the project consistently as Hypogaol everywhere it presents itself — package, binary, repository, README, and CLI banner — while the codebase's internal vocabulary moves from the placeholder-era "tomb" to the generic, brand-independent "volume." No functional behavior changes.

> **Prerequisite:** the GitHub repository is renamed `tomb-fido2` → `hypogaol` (owner action, done outside this epic) before Story 5.1 lands, so link/URL updates in that story are accurate in one pass.
>
> **Non-goals for this epic:** rewriting Epics 1-4's text, their story files, or past retros (left as frozen historical record per AD-13's existing continuity exception); producing actual mascot/mark artwork (brand-identity.md is the reference spec for whoever eventually draws it — no story here produces image assets); any change to CAP-1..17 behavior.

### Story 5.1: Product Identity Rename

As a maintainer,
I want the product's name updated to Hypogaol everywhere it's read from a single source,
So that Cargo, the CLI, and the README present one consistent, permanent product identity instead of the `tomb-fido2` placeholder.

**Acceptance Criteria:**

**Given** the GitHub repository has already been renamed to `hypogaol`
**When** Story 5.1 lands
**Then** `Cargo.toml`'s `name` and `repository` fields, `_bmad/bmm/config.yaml`'s `project_name`, `README.md`'s title/badges, and `CHANGELOG.md`'s header all read `Hypogaol`/`hypogaol`, and `flake.nix` references are updated to match

**Given** `README.md`'s body prose refers to the product by name (e.g. "tomb-fido2 exists to do one job well...", the "Break-glass recovery (no tomb-fido2 required)" heading)
**When** Story 5.1 lands
**Then** every such self-reference reads `Hypogaol` instead — but references to the *domain concept* (the encrypted container, e.g. "close every tomb-fido2-managed tomb") keep the word "tomb" untouched (that's Story 5.2's job), and references to the unrelated `dyne/tomb` project (e.g. "a capability the original Tomb never had") are left alone

**Given** AD-13's placeholder-name isolation (CLI/binary name sourced from exactly one place — the Cargo package name)
**When** the package name changes
**Then** the compiled binary and `--help` banner automatically reflect the new name with no additional literal to update, confirming AD-13's guarantee held

**Given** the renamed repository
**When** any CI/release config (GitHub Actions workflows, cargo-dist config) references the old repo path or name
**Then** those references are updated to the new path

### Story 5.2: Domain Term Rename (tomb → volume)

As a contributor reading or modifying the codebase,
I want the encrypted-container concept called "volume" everywhere instead of "tomb",
So that the domain vocabulary is generic and independent of whatever the product happens to be branded as.

**Acceptance Criteria:**

**Given** every `src/` module, `tests/unit`/`tests/hardware` file, and their identifiers/comments/error strings that currently say "tomb"
**When** Story 5.2 lands
**Then** all of them read "volume" instead, with no change in behavior

**Given** the full rename
**When** `cargo build`, `make test`, and (manually) `make test-hardware` are run afterward
**Then** all pass with zero behavioral difference from before the rename

**Given** README body text and `hooks.md`
**When** they reference the container concept
**Then** they also say "volume", consistent with the code

**Given** historical planning docs (SPEC.md, Epics 1-4 above, ARCHITECTURE-SPINE.md, past retros)
**When** Story 5.2 is scoped
**Then** none of them are rewritten — they remain frozen historical record per AD-13's existing continuity exception

### Story 5.3: Top-Level Branding Copy

As a first-time reader of the README or `--help` output,
I want the Hypogaol name, tagline, and pronunciation note presented clearly at the top level,
So that the brand identity lands without any flavor leaking into functional output.

**Acceptance Criteria:**

**Given** the README header
**When** Story 5.3 lands
**Then** it carries the tagline "Sealed until touched." and a short pronunciation note for "gaol" (reads like "jail")

**Given** the CLI `--help` banner
**When** it's shown
**Then** it may carry the tagline/name treatment, but no subcommand name, flag, or error message anywhere changes to a themed/flavored word

**Given** brand-identity.md's tone boundary
**When** any copy is added under this story
**Then** error messages and README body text remain plain, literal, and human-friendly — verified by an explicit grep-through check, not just a stated intention

### Story 5.4: Update Live Planning/Automation Pointers

As a maintainer,
I want the BMAD tooling's own config and automation references updated to the new name,
So that future BMAD workflow runs (sprint-status, GitHub automation) operate against accurate, current pointers rather than stale `tomb-fido2` references.

**Acceptance Criteria:**

**Given** `_bmad/custom/github-automation-reference.md`
**When** Story 5.4 lands
**Then** its repo path and GitHub Project display name reflect the renamed repository

**Given** SPEC.md's Constraints section and ARCHITECTURE-SPINE.md's AD-13
**When** Story 5.4 lands
**Then** a short addendum is appended (not a rewrite) noting the rename executed on 2026-08-02, referencing `sprint-change-proposal-2026-08-02.md`

## Epic 6: Volume Resilience, Filesystem Choice & Everyday Polish

Users get a more resilient, flexible, and ergonomic tool: create survives a crash/interruption and resumes cleanly instead of leaving an unrecoverable partial volume or a false refusal, users choose XFS or Btrfs alongside ext4, label their first key at creation time, and optionally scaffold example hook files so hooks are discoverable without reading docs; two invocations against the same volume can no longer race past a safety guard; a FIDO2 device with a PIN configured warns the user before the touch prompt, not after a confusing failure; and both end users (one-letter flag shorthand across the whole CLI) and contributors/evaluators (README health badges) get quality-of-life polish. No new port or architectural layer — every capability slots onto Epics 1-4's existing `create`/`FilesystemBackend`/`Fido2Backend` surface.

### Story 6.1: Crash-Safe Create Resume

As a user,
I want to re-run create against the same destination after a crash or interruption during a prior create attempt,
So that I get a clean, fully-created volume instead of being stuck with an unrecoverable partial one or a false "already exists" refusal.

**Acceptance Criteria:**

**Given** a file-backed create that was interrupted after `luksFormat` but before final cleanup (the marker token is still present)
**When** I re-run create against the same destination
**Then** the tool detects the marker via `has_marker_token`, proceeds with no confirmation, and produces the same fully-created, unlockable volume as an uninterrupted run

**Given** a device-backed create interrupted the same way
**When** I re-run create against the same device
**Then** the same marker-verified resume applies, and mandatory size resolution against `device_capacity` still runs even though the confirmation prompt is skipped — a device shrunk since the crashed attempt is still caught

**Given** a destination whose LUKS2 header has no marker token (a genuine pre-existing file/volume)
**When** I run create against it
**Then** it refuses exactly as before CAP-23, unchanged

**Given** create completes successfully (interrupted-then-resumed, or a normal uninterrupted run)
**When** final cleanup runs
**Then** the marker token is removed first and the transient bootstrap keyslot second, in that order, matching AD-5's safe-ordering reasoning

**Given** the corrected real execution order (AD-9 amendment)
**When** create runs, interrupted or not
**Then** FIDO2 key enrollment happens before `mkfs`, since the transient bootstrap passphrase is the only valid credential to authenticate `systemd-cryptenroll` with at that point

### Story 6.2: Custom Key Label at Create

As a user,
I want to supply a custom label for the first FIDO2 key enrolled during create's bootstrap step,
So that my newly created volume's key is labeled the same way I'd label any key I enroll later.

**Acceptance Criteria:**

**Given** I run create with `--label`
**When** the bootstrap key is enrolled
**Then** the volume's `key_label` metadata equals the supplied value

**Given** I run create without `--label`
**When** the bootstrap key is enrolled
**Then** the tool falls back to today's default label, unchanged

**Given** `--label` is supplied
**When** info or revoke later lists this volume's keys
**Then** the custom label displays exactly as supplied, the same as any other enrolled key's label

### Story 6.3: Hook-Template Scaffolding at Create

As a user,
I want create to optionally scaffold example bind-hooks/exec-hooks files into a new volume,
So that I can discover the hooks format without consulting docs first.

**Acceptance Criteria:**

**Given** I run create with the scaffold-hooks flag
**When** creation completes
**Then** the volume contains a commented-out example `bind-hooks` file (parses to zero live entries) and a non-executable `exec-hooks.example` stub — not the live `exec-hooks` filename

**Given** I run create without the flag (default)
**When** creation completes
**Then** no template files are written

**Given** scaffold-hooks is requested
**When** create runs
**Then** scaffolding happens after `mkfs` (so a filesystem exists to hold the files) and before final marker/keyslot cleanup, via a mount → write → unmount sequence on the still-open mapper — no separate FIDO2 selection needed

**Given** a scaffolded volume
**When** I later rename `exec-hooks.example` to `exec-hooks` and `chmod +x` it
**Then** it activates as a normal exec-hooks file, passing AD-14's existing guardrail checks

### Story 6.4: XFS and Btrfs Filesystem Support

As a user,
I want to create or resize a volume using XFS or Btrfs, not just ext4,
So that I can pick the filesystem that best fits my use case.

**Acceptance Criteria:**

**Given** I run create with `--filesystem xfs` or `--filesystem btrfs`
**When** creation completes
**Then** the volume is formatted with `mkfs.xfs` or `mkfs.btrfs --mixed` respectively, and the chosen type is recorded in the `filesystem` token field

**Given** a Btrfs volume as small as ~20 MiB
**When** it's created
**Then** it succeeds, since `--mixed` mode is used unconditionally for every Btrfs volume this tool creates, not just small ones

**Given** an existing XFS or Btrfs volume
**When** I run resize
**Then** `growfs` uses `xfs_growfs` or `btrfs filesystem resize` respectively, selected from the same `filesystem` token field, never re-asked or sniffed

**Given** preflight for a create/resize targeting XFS or Btrfs
**When** it runs
**Then** it checks presence of that filesystem's toolchain (`xfsprogs` or `btrfs-progs`) — only the toolchain the requested operation actually needs, not always both

**Given** an ext4 volume (today's default)
**When** create or resize runs
**Then** behavior is unchanged from before this story

### Story 6.5: Concurrent-Invocation Guard

As a user,
I want the tool to prevent two simultaneous invocations from racing against the same volume,
So that I can never accidentally corrupt state or bypass a safety guard like the last-keyslot protection.

**Acceptance Criteria:**

**Given** a volume with exactly two valid keyslots
**When** I run two revoke invocations concurrently, each targeting a different key
**Then** only one succeeds — the second either fails fast with a clear "another operation is already in progress" error, or serializes cleanly behind the first, never both succeeding

**Given** any mutating workflow (create, enroll, revoke, close, resize)
**When** it runs
**Then** it acquires a non-blocking flock-based lock on the target as its second step, immediately after preflight passes

**Given** close-all or slam processing several open volumes
**When** each mapping is processed
**Then** a separate lock is acquired and released per mapping, never one lock held for the whole batch — one mapping's contention is only that mapping's own failure

**Given** info or unlock (including read-only unlock)
**When** they run
**Then** they acquire no lock at all — these are excluded from the guard by design

**Given** the tool crashes or exits mid-operation while holding the lock
**When** a later invocation runs against the same target
**Then** it proceeds normally — the kernel releases the lock automatically on process exit, with no stale-lock state to detect or clean up

### Story 6.6: Proactive FIDO2 PIN-Status Guidance

As a user,
I want the tool to warn me upfront when a FIDO2 device has a PIN configured, and give me plain-language guidance if I enter it wrong,
So that a PIN-required device never surprises me mid-touch-prompt or leaves me confused by a generic failure.

**Acceptance Criteria:**

**Given** a FIDO2 device with a PIN configured is involved in enroll (a specific device already selected)
**When** enroll runs
**Then** a warning naming that device and stating PIN entry will be required is shown before the touch/PIN prompt

**Given** an unlock where cryptsetup will match the stored token to whichever device answers (no device pre-selected)
**When** any currently-enumerated device has a PIN configured
**Then** a blanket warning listing every such device is shown before the blocking prompt

**Given** a wrong PIN is entered during a retry
**When** the authenticator reports it
**Then** the tool shows a plain-language warning naming that retries are limited and what happens if they run out — never a generic "authentication failed"

**Given** the subprocess call handling secret PIN entry
**When** stderr is captured for this diagnostic text
**Then** stdin/stdout stay strictly passthrough for the actual PIN entry, and stderr is read concurrently (not only after the child exits) to avoid a pipe-buffer deadlock on a long touch/PIN-blocking call

### Story 6.7: One-Letter CLI Flag Shorthand

As a user,
I want a one-letter shorthand for each subcommand's primary flags,
So that I can type common commands faster without giving up the long forms.

**Acceptance Criteria:**

**Given** any subcommand's `--help` output
**When** I view it
**Then** every documented flag shows a short alias alongside its long form

**Given** a short alias
**When** I use it instead of the long form
**Then** it behaves identically to the long form, for every flag across every subcommand

**Given** a same-subcommand collision on a flag's first letter
**When** aliases are assigned
**Then** the colliding flag falls back to the next-most-mnemonic distinguishing letter instead

**Given** `-h` and `-V`
**When** aliases are assigned across the CLI
**Then** neither is ever reassigned to a different flag

### Story 6.8: Repository Hygiene Badges

As a contributor or evaluator landing on the README,
I want to see build status, license, latest release, and MSRV badges at a glance,
So that I can judge the project's health without digging through CI or config files.

**Acceptance Criteria:**

**Given** the README header
**When** I view it
**Then** it displays build-status, license (GPL-3.0-or-later), latest-release, and MSRV badges, each linking to the resource it reflects (Actions run, LICENSE file, GitHub Releases)

**Given** CI does not yet run coverage instrumentation
**When** this story lands
**Then** `cargo-llvm-cov` is added to CI, reporting to Codecov, and a coverage badge is added linking to it

**Given** CI does not yet run a security audit
**When** this story lands
**Then** `cargo-audit` is added as a gating CI job (a known RustSec advisory fails the build, not just the badge), and a security-audit badge is added

**Given** all six badges
**When** they're added
**Then** each is a live, working link — not a placeholder image

## Epic 7: FIDO2 Unlocking-Behavior Flags & Interactive Menu

Users can enroll a FIDO2 key using any of systemd-cryptenroll's remaining unlock-behavior flags — presence-only (no PIN/UV) via `--client-pin`, or the weakest touch-free mode via `--user-presence` — alongside the existing `--user-verification` flag, giving four selectable unlocking modes in total (UV, PIN+UP, UP, NO-UP). When enrolling without specifying any of the three FIDO2 flags, the tool presents an interactive menu so regular users can choose a mode without memorizing flags, and newcomers can discover modes they didn't know existed — defaulting to UV when the connected token supports it, annotating (not hiding) any mode it can't offer, and warning clearly wherever a weaker mode is chosen. This closes out the "expose the full FIDO2 flag set" item previously carried as a non-goal/backlog item. No new port or architectural layer — slots onto Epic 4's existing FIDO2 enrollment surface (`Fido2Backend::enroll_fido2_key`, `fido2_verification_args`).

### Story 7.1: Presence-Only Enrollment (UP-only mode)

As a user,
I want to enroll a FIDO2 key with the PIN requirement dropped, keeping only the touch/presence check,
So that I can unlock with just a tap, without a PIN, when I don't need biometric-grade verification.

**Acceptance Criteria:**

**Given** I run enroll or create's bootstrap step with `--client-pin=false`
**When** enrollment completes
**Then** `--fido2-with-client-pin=false` is passed to systemd-cryptenroll for that credential, and future unlock requires only touch — no PIN

**Given** I run with `--client-pin=false` and `--user-verification` is not set (or explicitly false)
**When** enrollment completes
**Then** user-verification itself stays off — this flag governs PIN only, not UV, so a token with a fingerprint sensor still isn't asked for a fingerprint

**Given** `--client-pin` is not passed at all
**When** enrollment runs
**Then** behavior is unchanged from before this story — today's PIN+UP default

### Story 7.2: Touchless Enrollment & Flag Precedence (NO-UP mode)

As a user,
I want to enroll a FIDO2 key with the presence check itself disabled,
So that I can unlock with zero interaction, on tokens configured to allow it.

**Acceptance Criteria:**

**Given** I run enroll or create's bootstrap step with `--user-presence=false`
**When** enrollment completes
**Then** `--fido2-with-user-presence=false` is passed to systemd-cryptenroll, and future unlock requires no touch

**Given** `--user-presence=false` is requested without an explicit `--client-pin`
**When** enrollment completes
**Then** client-pin is also forced to false, producing the fully touch-and-PIN-free NO-UP mode — mirroring AD-16's existing precedent of UV forcing client-pin off

**Given** `--user-verification=true` is passed together with `--user-presence=false`
**When** enrollment is attempted
**Then** UV's existing precedence wins outright — user-presence is overridden (with a plain-language notice, not a silent drop), and the credential is enrolled as full UV, extending AD-16's existing "UV forces client-pin=false" precedent to also override user-presence

**Given** the connected token isn't itself configured to allow disabling UP
**When** systemd-cryptenroll/libfido2 rejects the enrollment for that reason
**Then** the tool surfaces the rejection in plain language, naming that the token needs UP disabled on itself first — not a generic failure

**Given** UP-only or NO-UP mode is being enrolled
**When** enrollment proceeds
**Then** an explicit security warning naming the weaker guarantee is shown first (NFR22), matching the existing enroll-time PIN-warning convention

### Story 7.3: UV Capability Detection

As a user,
I want the tool to know whether my connected FIDO2 token actually supports built-in user verification before offering it,
So that I'm never offered or defaulted into a mode my hardware can't deliver.

**Acceptance Criteria:**

**Given** a connected token whose `fido2-token -I` output reports the CTAP2 `uv` option as true (bare `uv` token, mirroring how `clientPin` is parsed today)
**When** the capability check runs during enrollment
**Then** it's reported as UV-capable

**Given** a token whose output reports `uv` as false or omits it entirely
**When** the capability check runs
**Then** it's reported as not UV-capable, with no error

**Given** the `fido2-token -I` call itself fails (device unplugged mid-check, communication error)
**When** the capability check runs
**Then** it's reported as a check-error, distinct from "not capable" — surfaced by Story 7.4's menu, not by this story's own UI

**Given** this capability check
**When** it runs
**Then** it's a plain internal query only — no CLI subcommand or user-facing message of its own, foundation for Story 7.4

### Story 7.4: Interactive Unlocking-Mode Menu

As a user,
I want to be walked through choosing an unlocking mode when I don't specify one via flags,
So that I can pick without memorizing three separate flags, and discover modes I didn't know existed.

**Acceptance Criteria:**

**Given** I run enroll or create's bootstrap step with none of `--user-verification`, `--client-pin`, or `--user-presence` passed
**When** enrollment reaches the point of resolving unlocking behavior
**Then** an interactive menu is shown listing, in order: UV, PIN+UP, UP, NO-UP

**Given** Story 7.3 reports the connected token as UV-capable
**When** the menu is shown
**Then** UV is pre-selected as the default

**Given** Story 7.3 reports the token as not UV-capable, or the capability check itself errored
**When** the menu is shown
**Then** the UV row is still shown, annotated inline with the reason it can't be selected (e.g. "unavailable — this token has no built-in verification" / "could not check: <error>"), and the pre-selected default falls to PIN+UP instead

**Given** UP or NO-UP is selected from the menu
**When** the choice is confirmed
**Then** the same security warning as Story 7.2/NFR22 is shown before enrollment proceeds

**Given** at least one of the three flags was passed explicitly
**When** enroll or create's bootstrap step runs
**Then** the menu is skipped entirely — flags always take precedence over the interactive prompt

**Given** the menu
**When** it's shown
**Then** it uses the same hand-rolled println/stdin prompt convention as device selection (`resolve_interactive_selection`) — no new menu library/dependency

## Backlog — Unscoped Candidate Ideas (Not Yet an Epic)

> Captured 2026-08-02 alongside the Hypogaol rename (Epic 5) but deliberately **not** part of it — unrelated in scope, and none of these have been through requirements elicitation yet (no FR numbers, no architecture decisions, no acceptance criteria). Listed here so they aren't lost, pending a future planning session to properly scope them into an epic.
>
> **Promoted to Epic 6 (2026-08-08), removed from this list:** custom key label at create (→ CAP-18/FR18), hook-template scaffolding (→ CAP-19/FR19), one-letter shorthand flags (→ CAP-20/FR20), repository hygiene badges (→ CAP-21/FR21), XFS/Btrfs support (→ CAP-22/FR22), crash-safe create resume (→ CAP-23/FR23), concurrent-invocation guard (→ CAP-24/FR24), FIDO2 PIN-required-device UX (→ CAP-25/FR25).
>
> **Partially promoted to Epic 7 (2026-09-10), removed from this list:** `--fido2-with-client-pin`/`--fido2-with-user-presence` support plus the interactive unlocking-mode menu (→ CAP-26/FR26, CAP-27/FR27, CAP-28/FR28). `--fido2-credential-algorithm`, `--fido2-salt-file`, and `--fido2-parameters-in-header` remain below, still undecided.

- **Expose remaining `systemd-cryptenroll` FIDO2 flags during enrollment** (`--fido2-credential-algorithm`, `--fido2-salt-file`, `--fido2-parameters-in-header`) — **open question, not yet decided:** is the added surface area worth it for advanced users, given the project's existing zero-fallback/zero-cognitive-overhead design posture (NFR3/NFR5)? Needs a deliberate design decision before this can become a real story — flagged here rather than assumed in scope.
- **Publish to crates.io — considered and declined (2026-08-06).** Would add ecosystem discoverability, a version badge, and a `cargo install hypogaol` path. Declined because: (1) it doesn't address the tool's actual dependency problem — hypogaol shells out to system binaries (`cryptsetup`, `systemd-cryptenroll`, `fido2-token`, `mkfs.ext4`/`resize2fs`) that `cargo install` can't provide, so a crates.io user still needs the same manual/Nix setup as building from source; (2) it would be a second release surface to keep in sync with the existing `cargo-dist` + `release-please` GitHub Releases pipeline, for no functional gain; (3) `AR-Dev4`'s scope fence already defers distro/packaging channels beyond GitHub Releases + `cargo build --release` for v1, and crates.io publishing falls inside that fence. Revisit only if there's an actual demand signal (someone asking for `cargo install`).
