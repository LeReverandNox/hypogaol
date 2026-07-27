# Version/Fact Reality-Check — AD-14..19 and Stack "psmisc" row

Scope: verify the newly-added technical claims in ARCHITECTURE-SPINE.md are grounded in real,
current tool behavior rather than asserted from training data. Local tool versions match what the
Stack table cites (cryptsetup 2.8.6, systemd 261), so local execution/`man`/`strings` inspection was
used as a primary source alongside web search, per the environment's own devShell.

Date of check: 2026-07-27. No root/sudo available in this sandbox, so live LUKS2
create/enroll/unlock round-tripping could not be performed; verification relies on local man pages,
`--help` output, and `strings` on the installed binaries (which reflect the exact shipped
behavior/format strings of cryptsetup 2.8.6 / systemd 261 / psmisc 23.7 present in this devShell),
cross-checked against upstream docs.

---

## 1. AD-16 — `--fido2-with-user-verification` is enrollment-time-only; unlock reads it automatically

**Claim:** `--fido2-with-user-verification` is a `systemd-cryptenroll` flag, applied at enrollment
time only; `cryptsetup`'s `systemd-fido2` LUKS2 token plugin reads the UV requirement from the
stored credential automatically at unlock time, with no separate unlock-time flag needed.

**Verdict: CONFIRMED.**

- Local `man systemd-cryptenroll` (systemd 261, matches Stack table) shows the flag verbatim:
  ```
  --fido2-with-user-verification=BOOL
      When enrolling a FIDO2 security token, controls whether to require
      user verification when unlocking the volume (the FIDO2 "uv" feature).
      Defaults to "no" ... Added in version 249.
  ```
  This is explicitly an enrollment-time option ("When enrolling..."), confirming the "enrollment
  parameter" half of the claim.
- Local `cryptsetup --help` (cryptsetup 2.8.6, matches Stack table) has **no** `--fido2-*` flags at
  all on `open`/`luksOpen`. cryptsetup's own CLI carries no FIDO2-specific unlock parameters —
  consistent with unlock behavior being driven entirely by the external LUKS2 token plugin
  (`systemd-fido2`), which cryptsetup 2.8.6 confirms it loads (`LUKS2 external token plugin support
  is enabled` in `cryptsetup --help` output).
  - This directly supports "no separate unlock-time flag needed" — there is no flag to add even if
    one wanted to.
- Web corroboration (man7.org, Debian/Arch/Ubuntu manpages, Lennart Poettering's systemd blog on
  TPM2/FIDO2/PKCS#11 unlocking) is consistent: enrollment "embeds all necessary information ... in
  the LUKS2 volume header," and unlock via `/etc/crypttab`'s `fido2-device=` (or interactively) uses
  whatever was embedded at enroll time with no additional UV flag surfaced to the user at unlock.
- One nuance not stated in AD-16 but worth flagging for context (not a correctness issue for the
  claim as written): systemd GitHub issues (#36235, #19208) describe real-world friction where
  UV-enrolled tokens can't always be pre-identified during automatic multi-token discovery, causing
  systemd-cryptsetup to iterate tokens one-by-one. This doesn't contradict AD-16 (UV is still read
  automatically, no flag needed) but is a known rough edge in multi-key setups the spine doesn't
  need to address at this level.

Sources:
- `man systemd-cryptenroll` (local, systemd 261)
- `cryptsetup --help` (local, cryptsetup 2.8.6)
- https://www.man7.org/linux/man-pages/man1/systemd-cryptenroll.1.html
- https://man.archlinux.org/man/systemd-cryptenroll.1.en
- https://0pointer.net/blog/unlocking-luks2-volumes-with-tpm2-fido2-pkcs11-security-hardware-on-systemd-248.html
- https://github.com/systemd/systemd/issues/36235
- https://github.com/systemd/systemd/issues/19208

---

## 2. AD-18 / Stack "psmisc" row — `fuser -m`, `kill -s <SIGNAL>`, psmisc still real/maintained

**Claim:** `fuser` (from `psmisc`) can list processes using a mountpoint via `fuser -m <mountpoint>`;
`kill -s SIGTERM/SIGHUP/SIGKILL <pid>` is the right tool for those three signals; psmisc is a real,
maintained, commonly-available Linux package.

**Verdict: CONFIRMED**, with one small imprecision in the Stack row's phrasing (see note).

- Local `fuser --help` (fuser 23.7, "PSmisc", installed at `/usr/bin/fuser`) shows:
  ```
  -m,--mount  show all processes using the named filesystems or block device
  ```
  matching `fuser -m <mountpoint>` exactly as AD-18/adapters describe.
- Local `/usr/bin/kill --version` reports `kill from util-linux 2.42.2`. `kill -l` lists all
  standard signal names including `HUP`, `TERM`, and `KILL`; `kill -s TERM <pid>` was exercised
  locally and correctly delivered the signal (process exited with the expected signal-terminated
  status). `kill -s SIGTERM/SIGHUP/SIGKILL <pid>` is valid syntax for both the util-linux `kill` and
  the shell-builtin `kill` (bash/zsh builtins accept the same `-s SIGNAME` form) — this is the
  standard, correct tool for the job.
- **Minor inaccuracy to flag:** the Stack table row says `psmisc — 'fuser', and the coreutils/util-linux
  'kill'`. In this environment (and generally), `kill` is shipped by **util-linux** (confirmed:
  `/usr/bin/kill` → `kill from util-linux 2.42.2`), not GNU coreutils — coreutils does not ship a
  `kill` binary. The "coreutils/" qualifier is technically wrong (or at best an unnecessary hedge for
  procps-ng-based distros, where `kill` comes from procps-ng, still not coreutils). Low severity:
  doesn't affect correctness of AD-18's mechanism, only the Stack table's provenance note for `kill`.
  Suggest changing to "util-linux/procps `kill`" or just "the system `kill`".
- psmisc is actively maintained: GitLab `psmisc/psmisc` shows commits as recent as October 2025
  (Craig Small, current maintainer), latest release **23.7** (which is exactly the version installed
  locally), and it remains packaged in all mainstream distros (Debian, Fedora, Arch, Ubuntu, etc.).
  Package is not deprecated or abandoned.

Sources:
- `fuser --help` / `fuser -V` (local, psmisc 23.7)
- `/usr/bin/kill --version` (local, util-linux 2.42.2)
- https://gitlab.com/psmisc/psmisc/-/commits/master
- https://gitlab.com/psmisc/psmisc/-/tags
- https://packages.fedoraproject.org/pkgs/psmisc/psmisc/index.html
- https://man7.org/linux/man-pages/man1/fuser.1.html

---

## 3. AD-17 — `dmsetup ls` / `cryptsetup status <name>` enumerate mappings and recover device/file path

**Claim:** `dmsetup ls` and/or `cryptsetup status <name>` can enumerate live dm-crypt mappings and
`cryptsetup status <name>` reports the underlying device/file path back via a "device:" field;
sanity-check against cryptsetup 2.8.6 (the version the spine cites elsewhere as verified-local).

**Verdict: CONFIRMED**, and directly verified against the exact locally-installed cryptsetup 2.8.6
binary (strongest evidence class available without root).

- `strings` on the locally installed `cryptsetup` binary (2.8.6, path
  `/nix/store/.../cryptsetup-2.8.6-bin/bin/cryptsetup`) contains the literal status-output format
  strings:
  ```
  %s/%s is active%s.
    type:    %s
    cipher:  %s-%s
    device:  %s
  ```
  This is the exact `device:` field the spine claims `cryptsetup status <name>` reports, extracted
  directly from the shipped 2.8.6 binary's compiled string table — i.e., confirmed against the same
  binary/version cited elsewhere in the Stack table as "verified locally," not just docs.
- The cryptsetup(8) man page (man7.org, and local `man cryptsetup`) documents `status <name>` as
  "Reports the status for the mapping `<name>`," and explicitly notes (loopback section): "When
  device mapping is active, you can see the loop backing file in the status command output" —
  confirming that for file-backed (loopback) tombs specifically, `status` surfaces the backing file
  path, not just a raw block device node.
- `dmsetup ls` (from `lvm2`/device-mapper tooling, present locally at
  `/nix/store/.../lvm2-2.03.41-bin/bin/dmsetup`) supports `dmsetup ls --target crypt` to filter and
  enumerate only active dm-crypt mappings by name, per Red Hat LVM Administration docs, Arch
  `dmsetup(8)` man page, and community write-ups (e.g., sleeplessbeastie's notes: "list all open
  cryptsetup devices using `sudo dmsetup ls --target crypt`"). This matches AD-17's
  "`dmsetup ls` filtered by prefix, cross-checked with `cryptsetup status <name>`" mechanism exactly:
  `dmsetup ls` gives names, `cryptsetup status <name>` (or the `device:` line specifically) recovers
  the source path per name.
- Could not perform a live end-to-end `luksFormat` → `open` → `dmsetup ls` → `cryptsetup status`
  round trip in this sandbox (no root, `Cannot initialize device-mapper, running as non-root user`
  when attempting local test), so the "no other side effects/edge cases" question (e.g. exact
  formatting when there are multiple keyslots, or behavior under `--disable-locks`) is not
  independently re-derived — only the field's existence and the man-page-documented mechanism are
  confirmed. This is a residual gap, not a contradiction; recommend a real hardware/loopback smoke
  test during Epic 4 implementation (this project already has a `make test-hardware` path per
  AD-7/README) to close it before AD-17's `list_open_mappings` ships.

Sources:
- `strings $(which cryptsetup)` (local, cryptsetup 2.8.6)
- `man cryptsetup` (local) / https://man7.org/linux/man-pages/man8/cryptsetup.8.html
- https://man7.org/linux/man-pages/man8/cryptsetup-status.8.html
- https://man.archlinux.org/man/dmsetup.8.en / https://man7.org/linux/man-pages/man8/dmsetup.8.html
- https://docs.redhat.com/en/documentation/red_hat_enterprise_linux/6/html/logical_volume_manager_administration/dmsetup-ls
- https://sleeplessbeastie.eu/2021/07/07/how-to-display-current-mappings-for-encrypted-devices/

---

## 4. Rest of the Stack table

Per the task scope, only the new `psmisc` row and the newly-claimed CLI behaviors above were
re-verified. Spot-checked local versions for the rows touched by this review (`cryptsetup`,
`systemd-cryptenroll`) both matched the Stack table's previously-recorded verified-local versions
(cryptsetup 2.8.6, systemd 261 `+FIDO2 +LIBCRYPTSETUP_PLUGINS`) — no drift found. No other rows were
re-examined, consistent with the instruction that everything else was already verified 2026-07-22.

---

## Summary Table

| # | Claim | Verdict | Confidence |
| --- | --- | --- | --- |
| 1 | AD-16: UV flag is enrollment-time-only; unlock auto-reads it, no unlock flag | Confirmed | High (local man page + local `cryptsetup --help` showing no fido2 unlock flags + web docs) |
| 2 | AD-18/Stack: `fuser -m <mountpoint>`, `kill -s SIGTERM/SIGHUP/SIGKILL`, psmisc real & maintained | Confirmed (minor wording nit: "coreutils" mischaracterizes `kill`'s provenance — it's util-linux/procps, not coreutils) | High |
| 3 | AD-17: `dmsetup ls` + `cryptsetup status <name>` enumerate mappings, `device:` field recovers path | Confirmed (device: field verified directly from local cryptsetup 2.8.6 binary strings; full live round-trip not possible without root in this sandbox) | High, with one residual gap (no live round-trip test) |
