# Reviewer Gate v3 — Version verification (new Stack row: util-linux/blockdev)

**Verdict:** confirmed — `blockdev --getsize64 <device>` is the correct, current, idiomatic util-linux command for querying byte capacity; "near-universal" is a fair claim.

- `blockdev --getsize64` (man7.org util-linux man page) wraps the `BLKGETSIZE64` ioctl and prints size in bytes. Long-standing (BLKGETSIZE64 dates to the 2.4 kernel era), not deprecated — only the older 32-bit-sector `--getsize` is superseded (by `--getsz`), not `--getsize64`.
- vs. `lsblk -b -n -o SIZE`: both work, but `blockdev --getsize64` is the more direct single-purpose tool (clean scalar byte output, no parsing) — a reasonable idiomatic choice.
- util-linux is a pre-installed essential base package on virtually every mainstream distro (Debian, Ubuntu, RHEL, Arch, Alpine) — "near-universal" confirmed accurate.

Spot-check of the rest of the Stack table: no staleness found. "Snapshot verified 2026-07-22" header matches today's date; systemd 261, cryptsetup 2.8.6 cross-checked as still current. No inconsistencies in surrounding rows/formatting. No fix required.
