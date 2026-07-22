# Reviewer Gate v3 — Reconcile spine vs. SPEC.md (create semantics + placeholder-name diff)

**Verdict:** the spine fully and correctly reflects every SPEC.md requirement, including both areas under special scrutiny — no substantive gaps found, only two cosmetic nitpicks (addressed at Finalize).

## A) CAP-8 create semantics (AD-9)

Complete: explicit file-vs-device target mode (never path-sniffed); file-backed refuse-if-exists then `set_backing_file_size` (no user `dd`/`fallocate`/`truncate`); device-backed refuse-if-`has_luks2_header`; size resolution (user size rejected if it exceeds `device_capacity`, else defaults to full capacity); and the mandatory `confirmed: bool` gate stated as domain-enforced, not a CLI courtesy — matching SPEC's "even when no existing LUKS2 header was detected."

- Nitpick (fixed at Finalize): the original wording read as if `confirmed` applied function-wide rather than device-backed only. Reworked AD-9 to split `CreateTarget` into `File { path, size }` / `Device { path, size, confirmed }` variants so the field only exists on the device branch.

## B) Placeholder-name constraint (AD-13)

Verified via diff against the pre-update version. The only real leftover, `tomb_fido2_label` in the old AD-2/token-schema, was fixed to generic `key_label`. All other `tomb-fido2`/`tomb_fido2` occurrences remaining in the file are legitimate doc-continuity references (title, prose, the `tomb-fido2/` structural-seed root mirroring the single-sourced Cargo package name) — carved out explicitly by AD-13 itself.

## General reconcile

`binds: [CAP-1..11]` matches the capability map 1:1; every Constraints/Non-goals/Success-signal bullet in SPEC.md traces to at least one AD or map row.

Minor pre-existing inconsistency (not introduced by this update, not fixed): AD-6 isn't listed in CAP-8's "Governed by" map column despite AD-9 invoking AD-6's no-fallback-auth reasoning — same gap exists on other CAP rows, left as-is.
