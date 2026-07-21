# tomb-fido2

A FIDO2-native reimagining of [dyne/tomb](https://github.com/dyne/tomb)'s core insight: a thin, disposable wrapper over standard, boring primitives, built to survive its own death.

tomb-fido2 exists to do one job well: unlock a LUKS2 volume holding rarely-touched, highly sensitive material — a master GPG key, a password-manager backup — using a FIDO2 security key, as reliably as a bank safe opened once a year in a crisis. It replaces the multi-tool memorization burden of `cryptsetup` + `fido2-token` + `systemd-cryptenroll` with one CLI, without ever hiding what those tools are actually doing.

TrueCrypt's abrupt 2014 shutdown is the cautionary reference here. tomb-fido2 bets everything on LUKS/dm-crypt and FIDO2's `hmac-secret` extension precisely because they're low-level, standard, and multi-vendor — never on anything only tomb-fido2 itself understands. If this binary disappears tomorrow, your volume stays unlockable. See [Break-glass recovery](#break-glass-recovery-no-tomb-fido2-required) below.

## What it does

| Capability | |
| --- | --- |
| **Unlock** | Unlock a LUKS2 volume with a FIDO2 security key. No prior FIDO2 knowledge required — tomb-fido2 tells you exactly what to do ("Please touch your security key," never "Awaiting UP"). |
| **Enroll** | Add another FIDO2 key as an alternate unlock method on an already-unlocked volume (LUKS2 supports up to 32 keyslots). Useful for a backup key stored elsewhere. |
| **Revoke** | Remove a single FIDO2 key's ability to unlock the volume. tomb-fido2 refuses to remove your last remaining valid key — raw `cryptsetup` will happily let you lock yourself out; tomb-fido2 won't. |
| **Pre-flight check** | Before any operation, verifies your system actually supports what's about to happen (LUKS2 + FIDO2 support, required binaries, kernel features) and fails cleanly with an actionable message — never mid-operation. |

Works identically against a raw partition or a loop-mounted file — a capability the original Tomb never had.

## What it deliberately does *not* do

- **No fallback authentication.** FIDO2 is the only way tomb-fido2 unlocks a volume. There is no built-in recovery passphrase or keyfile escape hatch — that's a deliberate constraint, not an oversight.
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

tomb-fido2 is an orchestrator, not a crypto library. It never implements the FIDO2 `hmac-secret` protocol or LUKS2 keyslot cryptography itself — that work happens inside `cryptsetup` and systemd's own `systemd-fido2` LUKS2 token plugin, both mature, independently maintained, and already installed on most modern Linux systems. tomb-fido2's job is to sequence those tools correctly, translate their output into plain language, and enforce the one guardrail they don't provide: refusing to let you revoke your last working key.

Anything tomb-fido2 needs to remember (like a friendly label for each enrolled key) is stored *inside the LUKS2 header itself*, via cryptsetup's token metadata — never in a separate config file or database. That's what makes the break-glass recovery below actually work.

Wherever a secret (an existing unlock passphrase, a FIDO2 PIN) might need to be typed, tomb-fido2 hands the terminal directly to the underlying tool rather than reading it itself — that secret never passes through tomb-fido2's own memory.

### Break-glass recovery (no tomb-fido2 required)

If this binary is lost, corrupted, or you simply don't trust it anymore, your volume is still yours. Everything tomb-fido2 does can be done with stock `cryptsetup` and `fido2-token` alone:

```sh
# Unlock, using an already-enrolled FIDO2 token:
cryptsetup open /path/to/device-or-file my-volume

# List enrolled FIDO2 tokens/keyslots on the header:
cryptsetup luksDump /path/to/device-or-file

# List connected FIDO2 devices (for reference during manual recovery):
fido2-token -L
```

`cryptsetup open` automatically detects and uses any `systemd-fido2` token stored in the LUKS2 header — it will prompt you to touch your key the same way tomb-fido2 does, because it's the same underlying mechanism. No tomb-fido2 binary, no external notes, no dependency on this project surviving.

## Backing up your volume

tomb-fido2 doesn't touch this, on purpose — but you should have a plan. A basic [3-2-1 backup strategy](https://en.wikipedia.org/wiki/Backup#3-2-1_rule) (3 copies, 2 different media, 1 offsite) applies to both the encrypted volume itself and the FIDO2 keys used to unlock it. Losing every enrolled key or the only copy of the volume is unrecoverable by design — that's the nature of strong encryption, not a tomb-fido2 limitation.

## Status

Early-stage. Architecture is settled (see `_bmad-output/planning-artifacts/architecture/` for the full design rationale); implementation is in progress.
