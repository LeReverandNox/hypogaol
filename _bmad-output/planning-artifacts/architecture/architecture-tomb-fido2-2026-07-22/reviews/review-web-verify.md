# Web-Verification Review — Epic 6 additions to ARCHITECTURE-SPINE.md

Reviewed: AD-9 amendments (CAP-19/CAP-23), AD-20, AD-21, and the Stack table's new rows
(xfsprogs, btrfs-progs, cargo-llvm-cov, cargo-audit, Codecov).

Scope note: this reviews *technical claims presented as fact*, not the architectural
reasoning built on top of them.

---

## 1. `mkfs.btrfs --mixed` — data+metadata block-group sharing, size floor

**Spine claim (Stack table, row `btrfs-progs`, and Structural Seed):** `--mixed` merges
data+metadata into one block-group type, unconditionally for every Btrfs volume this tool
creates, dropping the minimum viable size from standard mode's "~114 MiB floor" to
mixed-mode's "~18 MiB".

**Verified — mechanism is accurate, exact floor numbers are off:**
- `--mixed` mode is real, current, not deprecated. Mechanism as described: "the mixed mode
  will remove the isolation and store both types in the same block group type" — accurate.
  Source: [mkfs.btrfs(8), man7.org](https://www.man7.org/linux/man-pages/man8/mkfs.btrfs.8.html)
- btrfs-progs' own docs give the minimum-size figures as **~109 MiB without mixed-bg**, and
  **~16 MiB with mixed-bg** — not 114/18 as the spine states. Source:
  [BTRFS Filesystem limits, btrfs.readthedocs.io](https://btrfs.readthedocs.io/en/latest/ch-fs-limits.html)
  (confirmed via search snippet; direct fetch was rate-limited, HTTP 429, during this
  review — treat as needing a follow-up direct read before finalizing the spine's numbers).
- The spine's own hedge ("roughly ~114 MiB" / "~18 MiB") absorbs some of this gap, but the
  actual documented figures (109/16) don't match either rounded number closely enough to be
  called "roughly" the same — worth a one-line correction, low severity.
- Additional real-world caveat the spine omits: **mixed mode is only recommended by upstream
  for filesystems under ~1 GiB (soft ceiling ~5 GiB)**, and "may lead to degraded performance
  on larger filesystems." The spine describes `--mixed` as *this tool's one Btrfs mode,
  unconditionally, no size-based threshold* — i.e., it will also be used on a multi-GiB or
  multi-TB tomb, which is exactly the regime upstream calls out as a fragmentation/performance
  risk, not merely a small-volume optimization. This isn't "wrong" (the tool's own tradeoff
  choice is legitimate for a "simple, one code path" design), but the spine states the
  unconditional choice without acknowledging the tradeoff upstream documents. Worth a line
  in AD-8/CAP-22 rationale, not just the Stack table footnote.
- Historical note found but not load-bearing: btrfs-progs ≤4.2.x used to *force* mixed mode
  under 1 GiB; that auto-forcing was removed in 4.3+ (now opt-in via `--mixed` only) — doesn't
  affect the spine's claim, included for completeness.

Sources:
- [mkfs.btrfs(8) — man7.org](https://www.man7.org/linux/man-pages/man8/mkfs.btrfs.8.html)
- [mkfs.btrfs(8) — btrfs.readthedocs.io](https://btrfs.readthedocs.io/en/latest/mkfs.btrfs.html)
- [BTRFS Filesystem limits — btrfs.readthedocs.io](https://btrfs.readthedocs.io/en/latest/ch-fs-limits.html)

---

## 2. `flock(2)` `LOCK_EX | LOCK_NB` — auto-release, no stale-lock cleanup

**Spine claim (AD-20):** kernel releases the lock automatically on process exit/crash, "no
stale-lock state to detect or clean up."

**Verified — core claim is correct; two real gotchas the spine doesn't mention:**
- Confirmed: locks are released "either by an explicit `LOCK_UN` operation ... or when all
  such file descriptors have been closed" — closing happens automatically at process exit,
  including a crash/SIGKILL (the kernel closes all fds on process termination). No stale-lock
  file is left because there is no lock *file* at all — `flock` state lives in kernel memory
  tied to the open file description, not on disk. This part of AD-20's claim is accurate.
  Source: [flock(2) — man7.org](https://www.man7.org/linux/man-pages/man2/flock.2.html)
- **Gotcha 1 — fork/exec inheritance (not addressed in the spine):** "Locks created by
  `flock()` are preserved across an `execve(2)`." Since a Rust CLI shells out to
  `cryptsetup`/`mkfs.*`/etc. via fork+exec, if the lock fd is not marked `close-on-exec`, it
  leaks into every spawned subprocess, and the lock is only fully released once *all* holders
  of that open file description (parent **and** every still-running child that inherited the
  fd) have closed it. In practice this is low-risk here because Rust's `std::fs::File` opens
  with `O_CLOEXEC` by default on Unix, so as long as `lock_target`'s implementation opens the
  fd via the standard library (not a raw libc `open()` without `O_CLOEXEC`), the fd will not
  leak into `Command`-spawned children. This is worth stating explicitly as an implementation
  requirement rather than leaving it as an unstated assumption — it's the kind of detail that
  silently breaks if someone reaches for `nix::fcntl::open` without the flag.
- **Gotcha 2 — same-process independent opens don't share a lock, but do still contend
  correctly:** flock is scoped to the *open file description*, not the inode or the process.
  Two independent `open()` calls on the same path (e.g., two separate CLI invocations, which
  is exactly AD-20's target scenario) do **not** share a lock automatically, but they *do*
  correctly contend/block against each other — this is the intended mechanism and matches the
  spine's design (each invocation opens the path fresh and calls `flock`). No issue here, just
  confirming the mechanism actually delivers cross-process mutual exclusion as claimed.
- **Block device vs. regular file:** no documented behavioral difference for `flock` — it
  operates at the VFS/open-file-description level regardless of the underlying file type, so
  locking a raw block device node (`/dev/sdX`, for a device-backed tomb) works the same as
  locking a regular file (for a file-backed tomb). Not explicitly documented as "verified for
  block devices" anywhere I found, but no counter-evidence either, and this is a widely-used
  pattern in other Linux storage tooling (e.g., `blkid`, `wipefs` flock the device node to
  avoid udev races).
- **NFS caveat:** not relevant here (local block devices/files only), but noted for
  completeness — `flock` only became NFS-safe from Linux 2.6.12 onward via fcntl byte-range
  emulation; irrelevant to this tool's Linux-only, local-storage scope.

Sources:
- [flock(2) — man7.org](https://www.man7.org/linux/man-pages/man2/flock.2.html)

---

## 3. `fido2-token -I <device>` — clientPin status reporting

**Spine claim (AD-21):** `Fido2Device` gains a `client_pin: bool` field populated via one
additional `fido2-token -I <path>` call per device; this is read as non-secret,
authentication-free enumeration data.

**Verified — accurate, and confirmed at the source-code level (not just docs):**
- `fido2-token -I` retrieves `authenticatorGetInfo` data, which per CTAP2 requires no PIN and
  no user presence/touch — it's a pure read, consistent with AD-21's framing as "no new
  blocking touch/PIN prompt before the enumeration itself."
- Checked the actual print logic in libfido2's `tools/token.c` (`print_opt_array`), which
  renders each CTAP2 `options` map entry as the option name, prefixed with `"no"` when the
  value is `false`, and omitted entirely if the authenticator doesn't advertise that option key
  at all:
  ```c
  printf("%s%s%s", i > 0 ? ", " : "", value[i] ? "" : "no", name[i]);
  ```
  So real output looks like `options: rk, up, uv, noplat, noalwaysUv, ..., clientPin, ...` —
  `clientPin` bare means a PIN **is set**, `noclientPin` means the authenticator supports PIN
  but none is set yet, and its total absence means the authenticator doesn't support
  `clientPin` at all. A `bool` correctly captures "PIN entry will be required" (true only in
  the bare-`clientPin` case) — collapsing "unsupported" and "supported-but-unset" into the same
  `false` is fine for this tool's purpose (both mean no PIN prompt happens). AD-21's claim
  holds up, including at a level of detail (source code) beyond what documentation alone
  would confirm.

Sources:
- [fido2-token(1) — Arch manual pages](https://man.archlinux.org/man/fido2-token.1.en)
- [Yubico/libfido2, tools/token.c (`print_opt_array`), github.com](https://github.com/Yubico/libfido2/blob/main/tools/token.c)
- [FIDO2 security key management via commandline — blog.tinned-software.net](https://blog.tinned-software.net/fido2-security-key-management-via-commandline/) (example real-world `-I` output)

---

## 4. cargo-llvm-cov / cargo-audit / Codecov — still current standard choices?

**Verified — all three remain reasonable/standard choices, one ownership change worth
flagging:**
- **cargo-llvm-cov** is the current recommended choice over `cargo-tarpaulin` for new Rust
  projects: more accurate (LLVM source-based coverage vs. tarpaulin's ptrace-based
  instrumentation), broader platform support (tarpaulin is Linux x86_64-only), and simpler to
  run with optimizations enabled. Tarpaulin remains viable/maintained but is the
  previous-generation choice. No newer/better-established alternative surfaced.
- **cargo-audit** remains the standard lightweight RustSec advisory scanner, actively
  maintained under the rustsec org. The more current *ecosystem trend*, however, is to pair it
  with **cargo-deny** (which subsumes advisory scanning plus license/source/duplicate-crate
  policy) rather than run cargo-audit alone — several 2026-era sources describe "use both" as
  the emerging standard practice for anything beyond a minimal check. The spine's choice of
  cargo-audit alone is not wrong, just narrower than what's increasingly recommended; worth a
  one-line note (not a correction) that cargo-deny is the adjacent tool to reconsider if the
  gate ever needs to grow beyond advisories.
- **Codecov**: still active and still free for open source, but changed ownership mid-2026 —
  acquired by Harness from Sentry (announced ~June 2026). Service continuity looks fine so
  far, but this is exactly the kind of dependency the spine's own hedge ("hosted service, no
  version to pin") anticipates as a re-resolve-at-use-time risk. Not a correction, just a
  reason to re-check at CI-setup time rather than treat "Codecov" as a fixed, stable choice.

Sources:
- [cargo-llvm-cov — github.com/taiki-e](https://github.com/taiki-e/cargo-llvm-cov)
- [`cargo-tarpaulin`: code coverage for Rust — c4dt.epfl.ch](https://c4dt.epfl.ch/article/cargo-tarpaulin-code-coverage-for-rust)
- [Rust and Cargo Supply Chain Security: cargo-audit, cargo-deny — systemshardening.com](https://www.systemshardening.com/articles/cicd/rust-cargo-supply-chain-security/)
- [About RustSec — rustsec.org](https://rustsec.org/)
- [Harness Acquires Codecov from Sentry — harness.io](https://www.harness.io/press-and-news/harness-acquires-codecov)

---

## 5. Other Epic 6 claims that read as assumption rather than checked fact

- **AD-2's "Open item" is still genuinely open, and AD-9's "Realized" language slightly
  overstates it.** AD-2 explicitly flags as unresolved whether cryptsetup's `systemd-fido2`
  token plugin tolerates *extra JSON fields on its own token type* — that question is not
  addressed by anything checked in this review and is still marked open in the doc itself
  (good — it's honestly flagged). Separately, AD-9's CAP-23 paragraph says the "fallback
  mechanism" (a second, sibling custom-type LUKS2 token) "is now used for real" for the crash
  marker. This *is* independently verifiable and checks out: cryptsetup's LUKS2 token schema
  requires only `type` + a `keyslots` array, an **empty `keyslots` array is valid**, and
  **tokens of an unrecognized type are ignored by cryptsetup during automatic activation**
  rather than rejected/erroring. So the marker-token mechanism itself is sound. But this is a
  different question from AD-2's still-open item about extra fields on the *systemd-fido2*
  token — the spine's phrasing ("that fallback mechanism is now used for real") could be
  misread as resolving AD-2's open item; it doesn't. Worth a clarifying phrase so a reader
  doesn't conflate "the second-custom-token pattern works" (verified) with "extra fields on
  the systemd-fido2 token are tolerated" (still unverified).
  Sources: [cryptsetup API: LUKS2 token wrapper access](https://mbroz.fedorapeople.org/libcryptsetup_API/group__crypt-tokens.html),
  [cryptsetup-token(8) — man7.org](https://man7.org/linux/man-pages/man8/cryptsetup-token.8.html)

- **`--mixed`'s unconditional use on arbitrarily large volumes** (see §1) is presented as a
  settled implementation detail in the Stack table/Structural Seed with no caveat, when
  upstream btrfs-progs documentation explicitly scopes mixed mode's *recommendation* to
  sub-1–5 GiB filesystems and calls out degraded performance beyond that. This is the most
  "assumption dressed as fact" item found: the spine states the unconditional choice
  matter-of-factly without noting that it's a documented-tradeoff decision, not a
  no-downside optimization.

- **flock-on-a-raw-block-device-node** (see §2) works by mechanism (flock is
  filetype-agnostic at the VFS level) but I found no source explicitly confirming it for LUKS2
  block devices specifically, only analogous usage by other Linux storage tools (`blkid`,
  `wipefs`). Low risk, but technically "inferred from adjacent evidence," not directly
  confirmed for this exact use case.

- **cargo-audit alone vs. cargo-audit+cargo-deny** (see §4) — not wrong, but the spine
  presents "cargo-audit, gating CI" as the settled choice without acknowledging that current
  Rust-ecosystem practice increasingly treats cargo-deny (which includes advisory scanning) as
  the more complete default.
