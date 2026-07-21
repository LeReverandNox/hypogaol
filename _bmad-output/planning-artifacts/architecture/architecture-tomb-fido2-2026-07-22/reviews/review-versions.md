---
review-of: ../ARCHITECTURE-SPINE.md
type: version-and-reality-check review
method: WebSearch/WebFetch spot-checks (crates.io API, official blogs/docs, ArchWiki, GitHub) + direct local verification (rustc, cargo, cryptsetup, systemd, fido2-token) on 2026-07-22
verdict: pass-with-minor-corrections
---

# Review — Versions & Reality Check

## Verdict

The core architectural bet is real and current, and every named tool exists and fits its stated
purpose. Nothing in the spine appears confabulated. There are, however, one factual
mis-attribution in AD-1, one untracked external dependency (libfido2/`fido2-token`), one
under-qualified version entry (rustc/cargo), and expected patch-level drift on four crates that
should be corrected or caveated before this spine is treated as frozen.

## Core architectural bet: systemd-cryptenroll + cryptsetup FIDO2 LUKS2 token plugin

**Confirmed real and current.** `systemd-cryptenroll` enrolls FIDO2 tokens (via the CTAP2
`hmac-secret` extension) into a LUKS2 header's token metadata slot; `systemd-cryptsetup`
(or vanilla `cryptsetup` with the plugin loaded) reads that metadata and unlocks the volume. This
is corroborated by the ArchWiki, Fedora Magazine, the `systemd-cryptenroll(1)` man page, and
Lennart Poettering's own introductory blog post for the feature (from systemd 248). It is not a
stale or deprecated mechanism — it's the standard, still-current way to do FIDO2-backed LUKS2 on
Linux in 2026.

Locally verified on this machine (matches the memlog's claims exactly):
- `cryptsetup 2.8.6` — and per web search, 2.8.6 is genuinely the latest **stable** cryptsetup
  release right now (a 2.8.7-rc2 release candidate exists but isn't released).
- `systemd 261 (261.1-1-arch)` — and per web search, systemd 261 was released 2026-06-19 and is
  the current stable systemd. `+FIDO2 +LIBCRYPTSETUP_PLUGINS` build flags are present.

So the "reality check" claim in the memlog is genuine, not asserted — it reproduces exactly what
`cryptsetup --version` / `systemctl --version` report on this box, and both happen to also be the
current upstream stable releases.

**One factual correction needed in AD-1's wording.** AD-1 says: "The actual hmac-secret exchange
and LUKS2 token handling stay inside cryptsetup's own token plugin." This attributes the plugin to
the wrong project. The FIDO2 LUKS2 token plugin (`libcryptsetup-token-systemd-fido2.so`) is
authored and maintained by the **systemd** project, not cryptsetup. cryptsetup only supplies the
generic LUKS2 token plugin-*loading* mechanism (the very `LIBCRYPTSETUP_PLUGINS` feature flag the
memlog itself cites as evidence — which actually undercuts the "cryptsetup's own" phrasing rather
than supporting it). This doesn't threaten the architectural bet at all — it's still fully
externally-maintained code that tomb-fido2 never reimplements — but AD-1 should say "systemd's
LUKS2 token plugin" (or "the systemd-fido2 cryptsetup plugin") rather than "cryptsetup's own token
plugin," so a future reader doesn't go looking for this logic in the wrong upstream project.

## Untracked dependency: `fido2-token` / libfido2

AD-1 and the Stack table name `fido2-token` as a subprocess tomb-fido2 shells out to (device
listing, CAP-6 pre-flight). `fido2-token` actually ships as part of **libfido2** (Yubico), a
project distinct from both cryptsetup and systemd. The Stack table folds it into the same row as
`systemd-cryptenroll` and versions it only as "systemd 261" — that version number does not apply
to `fido2-token`. Locally, `fido2-token -V` reports **1.17.0** (`libfido2 1.17.0-1`), but this was
never checked or recorded in the memlog despite `fido2-token` being an explicit, named external
dependency. Recommend adding a distinct `libfido2 (fido2-token)` row to the Stack table with its
own verified version, rather than implying it travels with systemd's version number.

## Rust/cargo version: correct but under-qualified

Spine states `Rust (rustc/cargo) | 1.90.0`. Direct local check confirms this is genuinely what's
installed (`rustc 1.90.0 (1159e78c4 2025-09-14)`, `cargo 1.90.0`), so it is not fabricated. However,
live-checking the Rust blog shows the actual current upstream stable is **1.97.0** (released
2026-07-09) — roughly 7 six-week release cycles ahead of what's on this dev machine. The Stack
table presents "1.90.0" flatly as though it were simply "the" current Rust version, with no note
that it's the locally-installed toolchain rather than a deliberate MSRV floor or "latest stable"
claim. Rust's backward-compatibility guarantees mean this is harmless either way (code targeting
1.90 semantics builds fine on 1.97), but the entry should be relabeled — e.g. "1.90.0 (locally
verified toolchain; treat as MSRV floor, not latest)" — so nobody mistakes it for a freshness claim.

## Crate versions: real, correctly attributed, minor patch drift since verification

All four crates checked directly against the crates.io API (not just search snippets):

| Crate | Spine says | Actual latest (checked live) | Note |
| --- | --- | --- | --- |
| clap | 4.6.2 | 4.6.4 | 2 patch releases behind |
| serde | 1.0.228 | 1.0.229 | 1 patch release behind |
| thiserror | 2.0.18 | 2.0.19 | 1 patch release behind |
| anyhow | 1.0.103 | 1.0.104 | 1 patch release behind |
| cargo-dist | ~0.32.x | 0.32.0 | exact match — the spine's own "~0.32.x" hedge was the right call |

None of this is confabulation — these are real crates, real version numbers that were accurate at
the moment the architect checked, and the drift is normal patch-level churn (non-breaking, no
newer major/minor line exists for any of them; clap/serde/thiserror/anyhow are all still on the
same major generation the spine assumed). The only action item: don't treat the Stack table's
exact patch versions as frozen — re-resolve to whatever's current when `Cargo.toml` is actually
written, which the spine's own phrasing ("2.0.18", not "^2.0") doesn't currently signal.

## Distribution tooling: cargo-dist and release-please

- **cargo-dist**: real, actively maintained (axodotdev/cargo-dist), purpose-built for exactly this
  use case (packaging Rust binaries for GitHub Releases). 0.32.0 confirmed current and matches the
  spine's "~0.32.x."
- **release-please**: real, googleapis-maintained, confirmed to support Rust via `Cargo.toml`
  (single-crate "rust" release-type) and a separate `cargo-workspace` plugin for workspaces. The
  spine's structural seed is a single non-workspace crate at repo root, so the one open
  cargo-workspace edge case found in research (Cargo.toml outside repo root, GH issue #2589) does
  not apply here. The spine's characterization ("language-agnostic, Rust manifest support") is
  accurate.

## What was NOT independently re-verified

- The exact FIDO2 CTAP2 `hmac-secret` extension requirement details (PIN vs. user-presence-only
  flows) — spine correctly defers device-specific UX details rather than asserting specifics, so
  no claim needed checking there.
- Whether any Linux distro besides Arch (the local box) currently ships cryptsetup 2.8.6 / systemd
  261 as default — irrelevant, since the spine already scopes "Platform: Linux only" without
  claiming distro-specific packaging currency, and distro packaging is explicitly Deferred.

## Summary of required corrections

1. AD-1: attribute the FIDO2 LUKS2 token plugin to **systemd**, not cryptsetup.
2. Stack table: add a distinct, independently-versioned row for `fido2-token`/libfido2 (locally:
   1.17.0) instead of implying it shares systemd's version.
3. Stack table: qualify the Rust entry as a locally-verified toolchain/MSRV floor, not a freshness
   claim (current upstream stable is 1.97.0 as of this review).
4. Treat the crate version column (clap/serde/thiserror/anyhow) as a snapshot to re-resolve at
   `Cargo.toml`-authoring time, not a pin — all four have already ticked forward by one or two
   patches.

None of these change any invariant, AD, or the capability map — they're corrections to the Stack
table and one sentence in AD-1, not to the architecture itself.
