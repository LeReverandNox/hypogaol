---
stepsCompleted: [step-01-validate-prerequisites, step-01-refresh-2026-07-22, step-02-design-epics, step-01-refresh-2026-07-22-b, step-03-epic-1-stories, step-03-epic-2-stories, step-03-epic-3-stories, step-03-create-stories]
inputDocuments:
  - _bmad-output/specs/spec-tomb-fido2/SPEC.md
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
- Data/error conventions: per-key label + metadata stored as JSON in the LUKS2 token slot (`key_label`, `credential_id`, `created_at`, `filesystem`); domain errors are a typed enum (`thiserror`), translated to plain-language text only at the `cli` boundary; no persistent logging/telemetry (stderr-only, ephemeral); no config file.
- Stack: Rust 1.90.0, clap 4.6.4, serde/serde_json 1.0.229, thiserror 2.0.19, anyhow 1.0.104, zeroize 1.9.0 (AD-3/AD-9 bootstrap-passphrase wipe only); external: cryptsetup 2.8.6, systemd 261 (+FIDO2 +LIBCRYPTSETUP_PLUGINS), fido2-token/libfido2 1.17.0, e2fsprogs (`mkfs.ext4`/`resize2fs`, AD-8 v1 ext4-only), util-linux `blockdev` (AD-9 `device_capacity`); Linux only.
- Structural seed: `src/domain/{workflows/{create,unlock,enroll,revoke,close,resize}.rs, preflight.rs, errors.rs}`, `src/ports/{luks_backend,fido2_backend,filesystem_backend}.rs`, `src/adapters/exec/`, `src/cli/{main,ux}.rs`, `tests/{unit,hardware}/`, `Makefile`, `flake.nix`/`flake.lock`, `README.md`. No starter template — greenfield project.

**Tooling / DevOps:**

- AR-Dev1 (Nix devShell): `flake.nix`/`flake.lock` provides a reproducible dev environment (nixpkgs-unstable + flake-utils) bundling the Rust toolchain plus `cryptsetup`/`systemd`/`libfido2`, so contributors never install these system-wide. Epic 1 setup work.
- AR-Dev2 (CI workflow): A GitHub Actions workflow runs `make test` (mocked unit suite, per AD-7) on every push/PR. `make test-hardware` is explicitly excluded from this workflow — manual/local only.
- AR-Dev3 (Release automation): Two release-time GitHub Actions workflows — `release-please` (versioning/changelog from conventional commits) and `cargo-dist` (~0.32.x, builds and publishes release binaries to GitHub Releases across target platforms).
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

## Epic List

### Epic 1: Create & Open a Tomb (Foundation)
Users can create a brand-new tomb from scratch — file-backed (the tool allocates the backing file itself) or device-backed (an existing raw device/partition, with mandatory wipe confirmation) — formatting it as LUKS2, creating the chosen filesystem inside it, and bootstrap-enrolling the first FIDO2 key — then unlock it with the filesystem mounted and ready to use, all through the tool's own CLI with zero FIDO2 knowledge required. This epic also stands up the project foundation (Nix devShell, CI) and the shared infrastructure every later epic depends on: the `LuksBackend`/`Fido2Backend`/`FilesystemBackend` ports, the `domain::preflight` gate, deterministic mapping-name/mountpoint discovery (AD-12), and the CLI/UX translation boundary.
**FRs covered:** FR8, FR1, FR4, FR5, FR6, FR7 (established)

### Epic 2: Manage Tomb Access (Key Lifecycle)
Users can enroll an additional FIDO2 key as a backup unlock method on an already-created tomb, and revoke a single key's access when it's lost or compromised — with a hard guarantee that they can never revoke their way into a locked-out volume.
**FRs covered:** FR2, FR3

### Epic 3: Tomb Lifecycle & Advanced Access
Users can close an unlocked tomb (unmount + re-lock) as the clean counterpart to Epic 1's unlock, grow an existing tomb's capacity without recreating it or re-enrolling keys, and unlock a tomb read-only when they only need to inspect its contents safely. This epic also finalizes release packaging (`cargo-dist`/`release-please`) as the last capability epic before v1 ships.
**FRs covered:** FR9, FR10, FR11

## Epic 1: Create & Open a Tomb (Foundation)

Users can create a brand-new tomb from scratch — file-backed (the tool allocates the backing file itself) or device-backed (an existing raw device/partition, with mandatory wipe confirmation) — formatting it as LUKS2, creating the chosen filesystem inside it, and bootstrap-enrolling the first FIDO2 key — then unlock it with the filesystem mounted and ready to use, all through the tool's own CLI with zero FIDO2 knowledge required. This epic also stands up the project foundation (Nix devShell, CI) and the shared infrastructure every later epic depends on: the `LuksBackend`/`Fido2Backend`/`FilesystemBackend` ports, the `domain::preflight` gate, deterministic mapping-name/mountpoint discovery (AD-12), and the CLI/UX translation boundary.

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

### Story 1.3: Dependency Preflight Check

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

### Story 1.4: Create a File-Backed Tomb

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

### Story 1.5: Create a Device-Backed Tomb

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

### Story 1.6: Unlock and Mount a Tomb

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

### Story 1.7: Unified CLI Dispatch & Plain-Language Errors

As a user,
I want to drive create and unlock through one CLI with prompts/errors in plain language,
So that I never need to fall back to cryptsetup/fido2-token flags directly, even when something goes wrong.

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

## Epic 3: Tomb Lifecycle & Advanced Access

Users can close an unlocked tomb (unmount + re-lock) as the clean counterpart to Epic 1's unlock, grow an existing tomb's capacity without recreating it or re-enrolling keys, and unlock a tomb read-only when they only need to inspect its contents safely. This epic also finalizes release packaging (`cargo-dist`/`release-please`) as the last capability epic before v1 ships.

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

### Story 3.4: Release Automation

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
