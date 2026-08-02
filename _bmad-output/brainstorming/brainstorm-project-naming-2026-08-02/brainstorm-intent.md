# Intent: Rename tomb-fido2 → Hypogaol

## 1. Decision

- New project name: **Hypogaol** (replaces the `tomb-fido2` placeholder).
- Rationale: zero OSS/GitHub collision; playful, strong parti-pris via a Bloodborne/Soulsborne ("Hypogean Gaol") reference; stays in the underground/buried register without literally naming "tomb" or "fido2".

## 2. Renaming scope (project name)

Everything that currently surfaces the placeholder name `tomb-fido2` / `tomb` as a *product name* must become `Hypogaol` / `hypogaol`:

- `Cargo.toml` package name
- Compiled binary name
- Repository name (GitHub)
- GitHub org/repo references (URLs, badges, CI config, links)
- README title and badges
- CLI `--help` banner / top-level program name
- Any other user-facing string currently saying "tomb-fido2" or "tomb" as the product name

### Distinct decision: domain term rename ("tomb" → "volume")

Separately from the product rename, the word **"tomb"** is used throughout the source code as the domain term for the encrypted container/volume concept. This must be replaced everywhere with the generic term **"volume"** (function/type/variable names, comments, log/error strings, docs, help text referring to the container concept). This term choice is deliberately theme-independent so it does not need to change again if branding evolves. Do not conflate this with the project name change above — they are two separate renames touching overlapping files.

## 3. CLI surface decisions

Subcommands stay plain and literal — no thematic/warden vocabulary:

- `unlock`
- `lock`
- `enroll`
- `revoke`
- `list`
- `init`
- `doctor`

Themed alternatives (parole, remand, deputize, discharge, roster, found, inspect, etc.) were explicitly considered and rejected as too abstract to memorize under crisis-use conditions (zero-cognitive-overhead requirement carried over from Epic 1).

## 4. Tone constraints

- README body and all error/status messages: straight-to-the-point, clear, human-friendly, **no stylistic flavor**.
- Branding flavor (name, tagline, mascot, visual mark) is confined strictly to top-level presentation: e.g. top of README, `--help` banner, logo/mark assets.
- Flavor must **not** leak into functional CLI output, documentation body, or error messages.

## 5. Tagline

**"Sealed until touched."** — for use in top-level branding contexts only (README header, `--help` banner). Not to appear in functional CLI/error copy.

## 6. Non-goals / explicit exclusions

- Do not use "tomb" or "fido2" literally in the new brand name.
- Do not theme error messages or subcommands with warden/gaol vocabulary.
- Do not reuse or echo the visual language of the user's other project **"runed"** (Lovecraftian/eldritch runes, glowing stone monolith, tentacles). Hypogaol's identity is a deliberately distinct, more "grounded" Victorian-gothic register (gargoyle warden, rose-window tracery, stone grey/moss green + lantern amber), not a shared visual universe with `runed`.

## 7. Reference

Full mascot/mark/palette/visual-identity details live in the sibling doc `brand-identity.md` (same directory). Those visual assets (logo, README-header treatment, mascot art) are out of scope for the code-renaming epic itself, but are useful context for anyone touching logo/README-header assets alongside this rename.
