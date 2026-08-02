---
stepsCompleted: [step-01-validate-prerequisites, step-01-refresh-2026-07-22, step-02-design-epics, step-01-refresh-2026-07-22-b, step-03-epic-1-stories, step-03-epic-2-stories, step-03-epic-3-stories, step-03-create-stories, step-01-refresh-2026-07-27-epic4, step-02-design-epics-epic4, step-03-epic-4-stories]
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
- FIDO2 PIN-required-device UX specifics (deferred — implementation detail, not architectural fork).
- Concurrent invocations of the tool against the same device (deferred — TOCTOU risk against AD-5's live-count guard, low-likelihood for a single-user cold-storage tool).
- Resuming a partial/crashed `create` (deferred, AD-9 — not required by SPEC).
- Filesystem types beyond ext4 (deferred — AD-8's enum design makes this additive later).

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

## Backlog — Unscoped Candidate Ideas (Not Yet an Epic)

> Captured 2026-08-02 alongside the Hypogaol rename (Epic 5) but deliberately **not** part of it — unrelated in scope, and none of these have been through requirements elicitation yet (no FR numbers, no architecture decisions, no acceptance criteria). Listed here so they aren't lost, pending a future planning session to properly scope them into an epic.

- **Custom label for the first enrolled key at volume creation** — today's bootstrap enrollment (CAP-8) presumably assigns a default `key_label`; let the user supply one at `create` time, same as a standalone `enroll` presumably already allows.
- **Scaffold hooks template files on volume creation** — when `create` runs, optionally drop example/template `bind-hooks`/`exec-hooks` files into the new volume so users discover the hooks (CAP-16) format without consulting docs first.
- **One-letter shorthand flags for subcommand flags** — general CLI ergonomics pass across all subcommands.
- **Expose all `systemd-cryptenroll` FIDO2 flags during enrollment** (`--fido2-credential-algorithm`, `--fido2-salt-file`, `--fido2-parameters-in-header`, `--fido2-with-client-pin`, `--fido2-with-user-presence`) — **open question, not yet decided:** is the added surface area worth it for advanced users, given the project's existing zero-fallback/zero-cognitive-overhead design posture (NFR3/NFR5)? Needs a deliberate design decision before this can become a real story — flagged here rather than assumed in scope.
