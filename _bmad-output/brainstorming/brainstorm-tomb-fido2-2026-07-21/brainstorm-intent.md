# tomb-fido2 - Intent

A compiled (Go/Rust) reimagining of dyne/tomb that unlocks LUKS-encrypted volumes exclusively via FIDO2 security keys, replacing GPG/keyfile-based key management.

## Problem / Motivation

- Core use case is cold storage for rarely-accessed, highly sensitive material (master GPG key, password manager backup) - like a bank safe opened once a year, in a crisis, that must unlock without hesitation.
- Design goal is not a wow-effect unlock experience - it's dead-simple, boring reliability: "I can trust it with my life."
- The job being hired: store sensitive information securely without relying on any third party or external service/tool.
- TrueCrypt's 2014 sudden shutdown is the cautionary reference: for critical use cases, rely on the simplest, lowest-level, most standard, well-tested tools (LUKS/dm-crypt) rather than proprietary or single-vendor-maintained solutions.
- The tool must be able to die and leave the volume fully recoverable: a thin wrapper over standard primitives (LUKS2 + FIDO2 hmac-secret), never a dependency the data's survival relies on.
- Core friction it solves: replacing the multi-tool memorization burden of cryptsetup + fido2-token + systemd-cryptenroll (each with its own flags) with one tool and simple commands - no hunting notes or searching the web during a crisis.

## Must-Have (v1)

- Unlock LUKS volumes with a FIDO2 key.
- Multi-key enrollment: add a keyslot for an additional FIDO2 key (LUKS2 native multi-keyslot support, up to 32 slots) as an alternate unlock method on the same volume.
- Key revocation: remove a single keyslot.
- One unified CLI replacing cryptsetup + fido2-token + systemd-cryptenroll.
- Plain-language terminal guidance assuming zero FIDO2 knowledge (e.g. "Please touch your security key button" instead of assuming familiarity).
- Pre-flight dependency check: verify all hard dependencies (LUKS2 FIDO2 support, required binaries/kernel features) are present; exit cleanly with a detailed error message if anything is missing.
- Support LUKS2 directly on raw physical block devices/partitions, in addition to file-backed loop devices - a core cryptsetup capability original Tomb never exposed.

## Should-Have (documented, not core)

- README-documented break-glass manual recovery procedure.
- README recommends a 3-2-1-style backup strategy for volumes/keys as a disclaimer/recommendation only. The tool itself stays entirely unaware of backups - no code or feature touches this.

## Out of Scope (Won't - this time)

- Remote/delegated unlock beyond cryptsetup's native token mode - unlock is scoped to exactly what cryptsetup's FIDO2 token mode natively offers, no more, no less.
- Post-quantum-readiness features - if classical asymmetric crypto breaks, the world has bigger problems than encrypted volumes; standard-tools recoverability is future-proof enough.

## Non-Negotiable Design Constraints

- Standard, low-level, well-tested tools only (LUKS/dm-crypt, FIDO2 hmac-secret) - no proprietary formats, no single-vendor-maintained crypto.
- Break-glass recoverability: README must document how to manually unlock the volume using only standard cryptsetup + fido2-token commands, with zero dependency on the tomb-fido2 binary surviving.
- Zero-FIDO2-knowledge UX: all prompts and errors in plain language, no assumed familiarity with FIDO2 concepts.
- Physical key presence at the moment of unlock is a fixed assumption, not a variable - the tool supports exactly what cryptsetup's FIDO2 token mode natively offers.
- No fallback auth paths (no GPG/keyfile fallback) - FIDO2 is the exclusive unlock mechanism.
- Real compiled binary (Go/Rust), not a shell script.
