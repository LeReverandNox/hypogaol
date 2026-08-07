# Architecture Spine Review — Epic 6 (CAP-18..25) amendment

**Reviewed:** `ARCHITECTURE-SPINE.md` (updated 2026-08-08), against the good-spine checklist,
with spot-checks against `src/ports/*.rs`, `src/domain/workflows/*.rs`, and
`src/adapters/exec/mod.rs`.

Note: this supersedes an earlier review in this same file, written against the original
7-capability v1 spine. That review's findings (undecided token write-path, AD-5 "valid
keyslot" ambiguity, no CI/testing strategy, AD-4 enforcement mechanism) have all since
been resolved in the current spine (AD-2's token schema, AD-5's FIDO2-token-specific
definition, AD-7's testing strategy, AD-4's "first statement inside the workflow
function itself" wording) and are not re-raised here.

## Verdict

Solid, well-reasoned amendment overall — the pre-Epic-6 ADs spot-checked (AD-3, AD-5,
AD-8, AD-9's pre-CAP-23 shape, AD-14, AD-17, AD-18, and AD-21's device-enumeration reuse
claim) match the real code exactly, including subtle details (token-then-keyslot removal
order, `.status()` with inherited stdio in `enroll_fido2_key`, `close`/`slam`/`close_all`
sequencing, `privileged()`/`invoking_identity()` reuse). But one new invariant — AD-20 —
has a Rule that does not actually work for its own primary target scenario, and a second
— AD-21 — has an unaddressed precision gap at unlock. Both are exactly the kind of
divergence risk this checklist exists to catch, so this is not a clean pass as-is.

## Findings

### 1. [CRITICAL] AD-20's `lock_target` cannot be acquired before AD-9's own ordering requires it, for the single most common `create` case

**Where:** AD-20 (lines 167–171), cross-referenced by AD-9 (line 95: "acquires AD-20's
per-invocation lock, then takes one `CreateTarget` enum argument").

AD-20's Rule mandates `lock_target(path)` take `flock(2)` on "an open fd to the path,
canonicalized via the same helper AD-12's mapping-name derivation already uses." That
helper is `domain::mapping_name::mapping_name()`, confirmed in
`src/domain/mapping_name.rs:38` to call `std::fs::canonicalize(path)`, which **errors if
the path does not exist**.

AD-9's Rule mandates the lock be acquired *before* `CreateTarget` is even matched — i.e.
before the File-branch's `path_exists` check and long before `set_backing_file_size`
allocates the file. Confirmed in `src/domain/workflows/create.rs:50-67`: today, `preflight`
runs first, then the `match target`, then `fs.path_exists`, then allocation — all *before*
any `mapping_name` call, which today happens only later inside `bootstrap_and_provision`
(line 162), after the file already exists.

For `CreateTarget::File` targeting a brand-new path — the ordinary, most common `create`
invocation — canonicalizing that path before it exists is impossible with the mandated
helper. The spine gives no resolution for this, so two independent implementers will
diverge exactly as AD-20 exists to prevent:
- One might canonicalize the parent directory instead and lock that — silently breaking
  the "same helper AD-12 already uses" requirement and changing lock granularity to
  "one file per directory" rather than "one file per path."
- One might open the path with `O_CREAT` to get a lockable fd — which spuriously makes
  `path_exists(path)` return `true` on the very first invocation, sending a brand-new
  `create` down AD-9's "does this path already have a marker token" branch and refusing
  or mishandling what should be a clean new-file create.
- One might reorder locking to after allocation — silently contradicting AD-9's own
  explicit "then acquires AD-20's ... lock, then takes one `CreateTarget`" ordering, and
  reopening exactly the TOCTOU race AD-20 was written to close for two concurrent
  `create`s at a brand-new path.

This needs an explicit rule (e.g., lock derived from the *parent directory's* canonical
path + file basename, decoupled from AD-12's mapping-name helper, with that decoupling
stated explicitly) before it's implementable without divergence.

### 2. [MEDIUM-HIGH] AD-21's PIN warning can't reliably identify "the device about to be used" at unlock

**Where:** AD-21 (lines 173–177).

AD-21's Rule says the PIN warning fires "for any device about to be used, whenever
`client_pin` is `true`," reusing the existing point-in-time `Fido2Device` enumeration
already shared by `LuksBackend::open`'s presence-wait loop. That reuse claim is accurate
against the real code (`wait_for_enough_fido2_devices(1)` at
`src/adapters/exec/mod.rs:1075`, immediately before `cryptsetup open`).

But at `enroll`/`create` time "the device about to be used" is unambiguous — it's
whichever device `Fido2DeviceSelection` resolved to. At **unlock**, tomb-fido2 does no
device selection at all: `wait_for_enough_fido2_devices(1)` only confirms *at least one*
device is present, then `cryptsetup luksOpen` internally matches the LUKS2 token against
whichever plugged-in device actually holds the matching credential — invisible to
`domain`/`adapters::exec`. If more than one FIDO2 device is plugged in at unlock time (a
normal case for anyone using per-key labels, CAP-18's own motivating scenario), AD-21's
rule as written would warn about every `client_pin: true` device in the enumeration
indiscriminately, including devices unrelated to this tomb — a false-positive warning
that undercuts AD-21's own "Prevents" clause ("a PIN-required device surprising the
user... instead of before it") by training users to expect warnings that don't
correspond to the device actually used. This gap is unaddressed by the Rule and should
be scoped explicitly (e.g., state that the unlock-time warning is "any PIN-required
device present," not "the device that will actually be used" — a real, and currently
unstated, difference from the enroll/create case).

### 3. [LOW-MEDIUM] Undefined cross-reference: "AR-Dev5" is cited four times but defined nowhere

**Where:** Stack table rows for cargo-llvm-cov/cargo-audit/Codecov (lines 211-213),
Capability Map row for CAP-21 (line 288).

AR-Dev1 through AR-Dev4 are real, defined entries in `epics.md`'s "Additional
Requirements — Tooling / DevOps" section, and are cited that way consistently elsewhere
in this repo's docs (e.g. `implementation-artifacts/1-2-*.md`, `1-3-*.md`). AR-Dev5 is
not defined anywhere — `grep -rn "AR-Dev5" epics.md` returns nothing. Its only other
occurrence in the repo is a same-day decision note in `.memlog.md` ("captured as a new
AR-Dev5 item... not a spine invariant") — i.e. the spine was written assuming a
companion update to `epics.md` that doesn't appear to have landed. A reader following
the spine's own citation convention to look up what AR-Dev5 actually requires will find
nothing. Low functional risk (CAP-21's own SPEC.md entry is self-contained), but it's a
dangling reference in a document whose whole purpose is to be an authoritative,
closed-loop contract.

### 4. [LOW] Stack table: "serde + serde_json" row overstates precision — the two crates are pinned to different versions

**Where:** Stack table, line 196: `serde + serde_json (token JSON schema) | 1.0.229`.

`Cargo.toml` pins `serde = "1.0.229"` and `serde_json = "1.0.151"` — two different
versions collapsed into one table row under one version number. Minor, but this table's
whole premise is "Snapshot verified 2026-07-22... re-resolve against Cargo.lock at build
time" — a reader taking the table at face value would misreport `serde_json`'s version.
Doesn't affect any AD's enforceability, but undercuts the "verified-current" claim the
checklist asked to spot-check.

### 5. [LOW] AD-3's stderr-piping amendment doesn't flag the pipe-buffer deadlock hazard it introduces

**Where:** AD-3 (line 58, "Amended (Epic 6, CAP-25)").

Today's `enroll_fido2_key` call (`src/adapters/exec/mod.rs:1341-1362`) uses `.status()`
with all three streams fully inherited — confirmed by its own comment ("stdin/stdout/
stderr all stay inherited in both branches"). AD-3's amendment for CAP-25 changes this to
inherited stdin/stdout but *piped* stderr on the same call, for a subprocess that can
block for an arbitrarily long human touch/PIN wait. Piping one stream while inheriting
others requires actively draining the piped stderr on a separate thread while the child
is running (`Command::spawn` + manual stream handling) — using `Stdio::piped()` and only
reading it after the process exits (or via `.output()`, which is also disallowed here
since it would capture stdout too) risks the OS pipe buffer filling during a long wait
and deadlocking the child. The Rule states the *what* (pipe stderr, not stdin/stdout)
but not the *how*, leaving a correctness pitfall for whoever implements it. Worth a
one-line implementation note.

## Minor / non-blocking

- AD-9's prose signature for `bootstrap_format_and_open(path, size, filesystem) ->
  MapperHandle` omits the `name` parameter present in the real trait
  (`src/ports/luks_backend.rs:20-26`). Cosmetic — the Structural Seed and the port file
  itself carry the correct signature — but worth tightening since AD-9 is the passage a
  new implementer is most likely to read first.
- Deferred section re-read critically: none of the six entries leave an ambiguous
  "how does X behave" gap that could cause two units to diverge — each is a clean
  "not implemented" cutline (fixed defaults, out-of-scope operations, additive-later
  filesystem types), not an underspecified behavior. No finding here.
- SPEC.md CAP-18..25 all have a corresponding row in the Capability → Architecture Map,
  and their AD text tracks the SPEC intent/success text closely (checked CAP-18, 19, 22,
  23, 24, 25 side by side). No obviously missing capability.
- Stack table's "no pinned version, preflight checks presence" annotations
  (e2fsprogs/util-linux/psmisc/xfsprogs/btrfs-progs) and "latest stable, re-resolve"
  annotations (cargo-llvm-cov/cargo-audit) are consistently and explicitly flagged, not a
  silent gap — this part of the checklist item passes.
- No whole dimension the initiative altitude owns was found left silent; all Consistency
  Conventions rows and the Deferred section together cover naming, data/format,
  state/logging/config, and CLI-alias conventions for the new capabilities.
