# Hypogaol — Brand Identity Reference

Reference spec for whoever designs the logo/mascot artwork and writes top-level branding copy (README header, `--help` banner). This document covers visual and verbal identity only — not engineering scope or the rename effort (see the sibling doc `brainstorm-intent.md` for that).

## 1. Name

- **Hypogaol** — the new project name, replacing the `tomb-fido2` placeholder.
- **Origin**: a blend of "hypogean/hypogeum" (a real Greek archaeological term for an underground chamber or tomb) and "gaol" (the archaic British spelling of "jail"), directly referencing Bloodborne's "Hypogean Gaol" location.
- Works on two independent levels at once: a genuine archaeological term, and a specific gaming Easter egg — it lands even for readers who miss one of the two references.
- **Pronunciation note required in the README**: "gaol" reads like "jail." This is archaic spelling, not a typo, and readers unfamiliar with it will trip on it without a note. Precedent for this kind of callout: k3sup does something similar for its own name.
- Confirmed zero OSS/GitHub collision.

## 2. Mascot — "Hypo"

- A stone gargoyle warden.
- **Costume/attributes**:
  - Victorian-era warden's coat with brass buttons
  - A ring of ornate keys at the belt
  - Carries a lantern
- **Tone**: deadpan, grounded gothic — not twee, not silly, not cartoonish.
- Name is a short, friendly nickname callback to the "hypo-" root, distinct from the formal project name "Hypogaol."

## 3. Mark / Logo Concept

- A gothic rose-window tracery circle.
- At its center: a stylized keyhole shape, into which a FIDO2 security-key silhouette fits.
- **Rationale**: rose windows and gargoyles are both genuine, real Gothic-cathedral architectural elements — so the mark and the mascot cohere as one consistent architectural world (a Gothic cathedral/prison), rather than mixed or arbitrary references.

## 4. Color Palette

- Base tones: stone grey and moss green.
- One accent color, reserved specifically for the "unlock/success" state: warm lantern-amber.
- The amber accent echoes Bloodborne's lamp checkpoints — it should not be used decoratively elsewhere, only to mark successful unlock.

## 5. Typography Direction

- A gothic/blackletter touch, reserved strictly for the wordmark and top-level headers.
- Contrasted everywhere functional (CLI output, code) with clean monospace.
- This deliberately stages an "ancient stone meets modern hardware" tension — do not blend the two registers; keep them as a contrast.

## 6. Tagline

> Sealed until touched.

## 7. Tone Boundary (Critical)

The gothic/playful flavor is strictly confined to top-level presentation: logo, mascot, README header/banner, and tagline.

**Explicitly out of bounds for this flavor:**
- CLI subcommand names (e.g. unlock, lock, enroll, revoke, list, init, doctor)
- Error messages
- README body text

All of the above must stay plain, literal, and human-friendly — zero flavor. This is a crisis-use tool; the design principle is zero-cognitive-overhead clarity everywhere the user depends on it under stress. The playful identity belongs only at the name/mascot/mark/tagline level, never in the functional surface.

## 8. Distinctiveness Note

Hypogaol's visual identity deliberately does **not** echo the user's other project, **runed**, which uses a Lovecraftian theme (ominous stone monolith, eldritch tentacles, dark greenish glowing Nordic-inspired runes).

Hypogaol's Victorian-gothic/gargoyle register is intentionally more "grounded" by contrast, so the two projects — despite both being dark-toned — read as distinct authorial choices rather than blurring into a repeated formula. Concretely, this ruled out a rune-circle mark for Hypogaol (already owned by runed's visual language) in favor of the rose-window tracery circle.
