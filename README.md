# tomb-fido2

> Working title. `tomb-fido2` is a placeholder pending a permanent name — don't read anything into it.

A FIDO2-native reimagining of [dyne/tomb](https://github.com/dyne/tomb)'s core insight: a thin, disposable wrapper over standard, boring primitives, built to survive its own death.

tomb-fido2 exists to do one job well: create, unlock, and maintain a LUKS2 volume holding rarely-touched, highly sensitive material — a master GPG key, a password-manager backup — using a FIDO2 security key, as reliably as a bank safe opened once a year in a crisis. It replaces the multi-tool memorization burden of `cryptsetup` + `fido2-token` + `systemd-cryptenroll` + `mkfs` + `mount` + `umount` with one CLI, without ever hiding what those tools are actually doing.

TrueCrypt's abrupt 2014 shutdown is the cautionary reference here. tomb-fido2 bets everything on LUKS/dm-crypt and FIDO2's `hmac-secret` extension precisely because they're low-level, standard, and multi-vendor — never on anything only tomb-fido2 itself understands. If this binary disappears tomorrow, your volume stays unlockable. See [Break-glass recovery](#break-glass-recovery-no-tomb-fido2-required) below.

## What it does

| Capability | |
| --- | --- |
| **Create** | Format a new tomb — LUKS2 header, a filesystem (ext4 in v1), and your first FIDO2 key enrolled — all as one operation, in either of two modes: give it a destination path and a size, and it allocates the backing file itself (no manual `dd`/`fallocate`/`truncate` first), or point it at an existing raw device/partition with an optional `--size` (defaults to the device's full capacity; can be set smaller to leave room for a later resize). |
| **Unlock** | Unlock a LUKS2 volume with a FIDO2 security key, mounted and ready in the same operation. No prior FIDO2 knowledge required — tomb-fido2 tells you exactly what to do ("Please touch your security key," never "Awaiting UP"). |
| **Read-only unlock** | Unlock and mount a tomb so both the LUKS2 mapping and the filesystem refuse writes — stronger than a plain read-only mount over a writable volume. |
| **Close** | Unmount and re-lock an open tomb — the symmetric counterpart to unlock. |
| **Enroll** | Add another FIDO2 key as an alternate unlock method on an already-unlocked volume (LUKS2 supports up to 32 keyslots). Useful for a backup key stored elsewhere. Optionally require user-verification (fingerprint/PIN, not just touch) on that key — whether it's this enrollment or a tomb's first key at creation. |
| **Revoke** | Remove a single FIDO2 key's ability to unlock the volume. tomb-fido2 refuses to remove your last remaining valid key — raw `cryptsetup` will happily let you lock yourself out; tomb-fido2 won't. |
| **Resize** | Grow an existing tomb's volume and filesystem in place, no re-enrollment needed. Grow-only — shrinking isn't supported. |
| **Info** | See a tomb's enrolled FIDO2 keys and their labels without unlocking it. |
| **Close all** | Close every tomb-fido2-managed tomb currently open, in one command. |
| **Slam** | The panic button: close everything, and for any tomb whose mount is stuck behind a busy process, escalate through `SIGTERM` → `SIGHUP` → `SIGKILL` against it automatically. Fires instantly, no confirmation prompt. |
| **Hooks** | Per-tomb bind-mounts and an open/close script, run automatically on unlock and close — e.g. auto-mounting `~/.gnupg` from inside the tomb, or firing your own automation. Skippable per invocation. |
| **Pre-flight check** | Before any operation, verifies your system actually supports what's about to happen (LUKS2 + FIDO2 support, required binaries, kernel features) and fails cleanly with an actionable message — never mid-operation. |

Create and resize both report each real stage as it happens (allocating, formatting, enrolling the key, and so on) rather than a single "please wait."

Works identically against a raw partition or a loop-mounted file, for every operation above — a capability the original Tomb never had. Resize is the one exception for raw partitions: tomb-fido2 grows the LUKS2 volume and filesystem into existing free space, but doesn't repartition — you still need to grow the partition itself first with your tool of choice.

Create refuses outright rather than risking your data: it won't touch a file-backed destination that already exists, or a device/partition that already carries a LUKS2 header. Because picking the wrong device is a real, higher-stakes mistake than a typo'd file path, creating on a raw device/partition always shows an explicit wipe warning and requires your confirmation before formatting anything — even when no existing LUKS2 header was found.

### Hooks (optional)

If a tomb has a `bind-hooks` file in its root (a two-column list: a path relative to the tomb, and where under your `$HOME` it should appear) and/or an executable `exec-hooks` file, tomb-fido2 runs them automatically on unlock and close. On unlock, it bind-mounts each valid entry, then invokes `exec-hooks open <mountpoint>`. On close, it reverses both: un-bind-mounting first, then invoking `exec-hooks close <mountpoint> <tomb-name> <loopback-device> <mapper-device>`. Pass `--skip-hooks` to skip both for one invocation.

Adapted from [dyne/tomb](https://dyne.org/docs/tomb/manpage/#hooks)'s hook model, with stricter guardrails: a `bind-hooks` entry that tries to escape the tomb or your home directory is skipped with a warning rather than applied, and `exec-hooks` only runs if it's a regular, non-world-writable file owned by you or root with the executable bit set — anything else is refused outright. This is the one place tomb-fido2 ever runs code it didn't write itself, so it's checked accordingly.

## What it deliberately does *not* do

- **No fallback authentication.** FIDO2 is the only way tomb-fido2 unlocks a volume. There is no built-in recovery passphrase or keyfile escape hatch — that's a deliberate constraint, not an oversight. (The one narrow exception, used internally and only while creating a brand-new tomb, is explained in [Security model](#security-model).)
- **No backup logic.** tomb-fido2 has no idea what backup strategy you use for your volumes or keys, and never will. (You should have one — see the disclaimer below.)
- **No configurable key-presence timing.** Unlock requires the physical key present and touched at the exact moment cryptsetup asks for it. This is exactly what cryptsetup's FIDO2 token mode offers — no more, no less.
- **No remote/delegated unlock**, no post-quantum anything. Out of scope by design.

## Installation

Prebuilt binaries are published on [GitHub Releases](../../releases) for each tagged version.

To build locally instead:

```sh
make build   # wraps: cargo build --release
```

Contributing? `nix develop` drops you into a shell with the exact Rust toolchain and runtime tools (`cryptsetup`, `systemd-cryptenroll`, `fido2-token`) this project uses, without installing anything system-wide.

## Security model

tomb-fido2 is an orchestrator, not a crypto library. It never implements the FIDO2 `hmac-secret` protocol or LUKS2 keyslot cryptography itself — that work happens inside `cryptsetup` and systemd's own `systemd-fido2` LUKS2 token plugin, both mature, independently maintained, and already installed on most modern Linux systems. Filesystem work (creating, growing, mounting, unmounting) is handled the same way, by shelling out to standard tools (`mkfs.ext4`, `resize2fs`, `mount`, `umount`) rather than any bespoke filesystem code. tomb-fido2's job is to sequence those tools correctly, translate their output into plain language, and enforce the guardrails they don't provide on their own: refusing to let you revoke your last working key, and refusing to shrink a tomb.

Anything tomb-fido2 needs to remember (like a friendly label for each enrolled key, or which filesystem a tomb was created with) is stored *inside the LUKS2 header itself*, via cryptsetup's token metadata — never in a separate config file or database. That's what makes the break-glass recovery below actually work.

Creating a brand-new tomb has one narrow, unavoidable exception to "FIDO2 only": LUKS2 needs an initial passphrase-protected keyslot to exist before anything else can be enrolled into it. tomb-fido2 generates that passphrase itself, uses it once to set up the header and filesystem, then removes it the moment your real FIDO2 key is enrolled — it's never shown to you, never asked of you, and never written anywhere outside that one internal step.

Wherever a secret (an existing unlock passphrase, a FIDO2 PIN) might need to be typed, tomb-fido2 hands the terminal directly to the underlying tool rather than reading it itself — that secret never passes through tomb-fido2's own memory.

User-verification enrollment (requiring your key's own fingerprint/PIN check, not just a touch) is configured once, at enroll time — the requirement lives on the FIDO2 credential itself, so unlock needs no separate flag to honor it.

tomb-fido2 never runs entirely as root; only the specific steps that need it (opening/closing the LUKS2 mapping, mounting) individually elevate via `sudo`, prompted right when they're reached. That also means `exec-hooks` (see [Hooks](#hooks-optional) above) always runs as you, not as root, even mid-operation — it was never elevated to begin with.

Close-all and slam find every currently open tomb by asking the kernel directly (which dm-crypt mappings exist right now) — never a stored list of "known tombs." Slam in particular is the one command that skips every confirmation prompt on purpose: it's the emergency button, and stopping to ask defeats the point.

### Break-glass recovery (no tomb-fido2 required)

If this binary is lost, corrupted, or you simply don't trust it anymore, your volume is still yours. Everything tomb-fido2 does day-to-day — unlocking, mounting, closing — can be done with stock `cryptsetup`, `mount`/`umount`, and `fido2-token` alone:

```sh
# Unlock, using an already-enrolled FIDO2 token:
cryptsetup open /path/to/device-or-file my-volume

# Mount it:
mount /dev/mapper/my-volume /path/to/mountpoint

# ...use it, then when you're done, reverse the order:
umount /path/to/mountpoint
cryptsetup close my-volume

# Read-only, instead of the two commands above:
cryptsetup open --readonly /path/to/device-or-file my-volume
mount -o ro /dev/mapper/my-volume /path/to/mountpoint

# List enrolled FIDO2 tokens/keyslots on the header:
cryptsetup luksDump /path/to/device-or-file

# List connected FIDO2 devices (for reference during manual recovery):
fido2-token -L

# List every currently open tomb-fido2 mapping (what close-all/slam do internally):
dmsetup ls | grep '^vault-'
```

`cryptsetup open` automatically detects and uses any `systemd-fido2` token stored in the LUKS2 header — it will prompt you to touch your key the same way tomb-fido2 does, because it's the same underlying mechanism. No tomb-fido2 binary, no external notes, no dependency on this project surviving. (Creating a new tomb and resizing one are setup/maintenance operations, not crisis-day operations — they aren't covered by break-glass recovery — you'd only run them while tomb-fido2 itself is available.)

## Backing up your volume

tomb-fido2 doesn't touch this, on purpose — but you should have a plan. A basic [3-2-1 backup strategy](https://en.wikipedia.org/wiki/Backup#3-2-1_rule) (3 copies, 2 different media, 1 offsite) applies to both the encrypted volume itself and the FIDO2 keys used to unlock it. Losing every enrolled key or the only copy of the volume is unrecoverable by design — that's the nature of strong encryption, not a tomb-fido2 limitation.

## Status

Early-stage. Architecture is settled (see `_bmad-output/planning-artifacts/architecture/` for the full design rationale); implementation is in progress.
