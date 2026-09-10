---
baseline_commit: 1db093f44dc91fe17c779f34a57a3e346bf5cb63
---

# Story 7.4: UV Capability Detection

Status: review

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a user,
I want the tool to know whether my connected FIDO2 token actually supports built-in user verification before offering it,
so that I'm never offered or defaulted into a mode my hardware can't deliver.

## Acceptance Criteria

1. **Given** a connected token whose `fido2-token -I` output reports the CTAP2 `uv` option as true (bare `uv` token, mirroring how `clientPin` is parsed today), **when** the capability check runs during enrollment, **then** it's reported as UV-capable. [Source: epics.md#Story 7.4, lines 1046-1048]
2. **Given** a token whose output reports `uv` as false or omits it entirely, **when** the capability check runs, **then** it's reported as not UV-capable, with no error. [Source: epics.md#Story 7.4, lines 1050-1052]
3. **Given** the `fido2-token -I` call itself fails (device unplugged mid-check, communication error), **when** the capability check runs, **then** it's reported as a check-error, distinct from "not capable" — surfaced by Story 7.5's menu, not by this story's own UI. [Source: epics.md#Story 7.4, lines 1054-1056]
4. **Given** this capability check, **when** it runs, **then** it's a plain internal query only — no CLI subcommand or user-facing message of its own, foundation for Story 7.5. [Source: epics.md#Story 7.4, lines 1058-1060]

## Tasks / Subtasks

- [x] **Task 0: Read every file/section this story touches before changing anything** (AC: all)
  - `src/adapters/exec/mod.rs` lines 900-981: `Fido2Device` struct, `list_fido2_devices`, `fido2_token_has_pin`, and — the direct template for this story — the pure parser `parse_client_pin_configured` (lines 975-981). Note its exact technique: split the `options: ` line on `", "` and test for **exact token equality**, never substring — this is what already makes `noalwaysUv`/`pinUvAuthToken` safe non-matches for a `uv`-token parser and must be mirrored precisely.
  - `src/adapters/exec/mod.rs` lines 1012-1054 (`wait_for_enough_fido2_devices`): shows how `client_pin` is populated for every enumerated device and *collapsed* to `false` on a query failure via `.unwrap_or(false)` (line 1037) because it's advisory-only. **This story's new query must NOT be collapsed the same way** — AC #3 requires the check-error case to stay distinguishable from "not capable," so the new function returns its `Result` untouched; nothing in this story calls `.unwrap_or(false)` on it. (Wiring it into a call site that decides what to do with each outcome is Story 7.5's job, not this one's — see AC #4.)
  - `src/adapters/exec/mod.rs` lines 3506-3535 (existing `parse_client_pin_configured` unit tests + the `REAL_INFO_OUTPUT_WITH_PIN` fixture): this fixture is a **real captured `fido2-token -I` sample** (from Story 6.6) that already contains the bare `uv` token — reuse it directly for this story's "capable" test case instead of inventing a new fixture. There is no equivalently real captured sample for a *non*-UV-capable device; any negative fixture this story adds is a synthetic derivation of the real one (e.g. via `.replace(...)` on the bare `uv` token, the same technique the existing `noclientPin` test already uses) — comment it as synthetic, not real-captured, to avoid the project's recurring "unverified claim presented as fact" pattern (see epic 6/7 action items on self-reported claims).
  - `_bmad-output/planning-artifacts/epics.md` Epic 7 framing (lines 986-988) and Story 7.4 itself (lines 1038-1060) — the authoritative AC source, verbatim above.
  - `_bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md` AD-21 (lines 182-186) — the closest formalized precedent (proactive PIN-status detection riding the existing enumeration, no new port method). CAP-28 (this story feeds) has no AD of its own yet; the Requirements Inventory candidate note (epics.md line 110) already specifies the exact technique ("parse the bare `uv` token... same technique as `parse_client_pin_configured`... three outcomes: capable, not-capable, check-error") — implement per that note and AD-21's placement precedent (private `adapters::exec` function, no new `Fido2Backend` port method); flag in Completion Notes that no formal AD-22 was written since none was required to implement this story, for Winston to confirm or retcon later if he wants one on record.
  - Do not confuse this story's "UV capability" (a pre-enrollment device query) with the pre-existing, differently-named `uv_required`/`fido2-uv-required` field read from `cryptsetup luksDump` in `tests/hardware/main.rs` (~lines 2301-2459) — that's a post-enrollment LUKS2 token field describing what was enrolled, an unrelated concept that happens to share the "uv" abbreviation. This story does not touch that code.

- [x] **Task 1: Add a pure, unit-testable `uv`-capability parser** (AC #1, #2)
  - Add `fn parse_uv_capable(fido2_token_info_output: &str) -> bool` directly below (or near) `parse_client_pin_configured`, structurally identical to it: find the `options: ` line, split on `", "`, return `true` iff exactly the token `"uv"` is present. No new logic shape — same one-liner chain as its template.
  - Doc-comment it the same way: note that `fido2-token` renders a `false` boolean CTAP2 option with a `no` prefix (`nouv`, not omitted) in this codebase's captured samples where the capability is unsupported, or with the capability simply absent from the line — both must parse as `false`, exactly mirroring `clientPin`'s documented behavior.

- [x] **Task 2: Add the I/O wrapper, `Result`-preserving (not `bool`-collapsing)** (AC #1, #2, #3)
  - Add `fn fido2_token_supports_uv(path: &str) -> Result<bool, DomainError>`, structurally identical to `fido2_token_has_pin` (runs `fido2-token -I <path>`, no `-c`, never prompts): on subprocess spawn failure or non-zero exit, return `Err(DomainError::AdapterFailure(...))` with the same message shape (`"fido2-token -I failed for {path}: {stderr}"`); on success, `Ok(parse_uv_capable(&stdout))`.
  - **This is the one place this story diverges from its template on purpose:** `fido2_token_has_pin`'s only caller (`wait_for_enough_fido2_devices`) collapses its `Result` with `.unwrap_or(false)` because PIN-status is advisory. This function's `Result<bool, DomainError>` return type is itself how AC #3's three-way distinction is represented (`Ok(true)` = capable, `Ok(false)` = not capable, `Err` = check-error) — do not add a `bool`-collapsing wrapper or default-on-error fallback anywhere in this story; that decision belongs to whichever call site Story 7.5 adds.
  - Per AC #4, this story adds no call site for `fido2_token_supports_uv` — it will be dead code from the compiler's point of view until Story 7.5 wires it in. Mark it `#[allow(dead_code)]` with a one-line comment noting it's called starting in Story 7.5, so `cargo build`/`cargo clippy` stay clean without inventing a premature caller. Remove the attribute in Story 7.5 once a real call site exists.

- [x] **Task 3: Unit tests for the parser** (AC #1, #2 — testing-strategy compliance, AD-7)
  - Add tests mirroring `parse_client_pin_configured`'s four cases, reusing `REAL_INFO_OUTPUT_WITH_PIN` (already contains bare `uv`) for the positive case:
    - `parse_uv_capable_true_for_real_captured_output_with_uv_option` — asserts `parse_uv_capable(REAL_INFO_OUTPUT_WITH_PIN)`.
    - `parse_uv_capable_false_when_uv_option_present_but_disabled` — derive a fixture by replacing the bare `uv` token (not the `noalwaysUv`/`pinUvAuthToken` substrings it's embedded near) with `nouv`; assert `false`. Comment that this fixture is synthetically derived, not independently real-captured.
    - `parse_uv_capable_false_when_token_absent_entirely` — an options line with no `uv`/`nouv` token at all (e.g. adapt the existing `"options: rk, up, noplat, noalwaysUv\npin retries: 8\n"` shape minus the bare `uv`); assert `false`.
    - `parse_uv_capable_false_for_empty_or_malformed_input` — mirror the existing empty-string and garbage-input cases.
  - No unit test is added for `fido2_token_supports_uv` itself — same as its template `fido2_token_has_pin`, which has none: `adapters::exec`'s direct `Command` invocations aren't mocked at this layer (AD-7 only fakes the `domain`-facing ports), so the wrapper's `Command`-handling is exercised only by real `fido2-token`, outside this story's scope (AC #4: no call site yet to hang a hardware-gated scenario off either).

- [x] **Task 4: Full regression pass**
  - `cargo build` succeeds with no new warnings (confirms Task 2's `#[allow(dead_code)]` does its job).
  - `make test` (`cargo test --lib --test unit`) passes with all prior tests green plus this story's new ones. **Verified live in this session at baseline_commit (`1db093f`): 39 lib + 289 unit = 328 total, 0 failed.** Re-verify from scratch rather than reusing this number if any time/other work has passed, per the standing project convention (epic-5/6/7 self-reported-count watch items).
  - `cargo fmt --check` and `cargo clippy --all-targets` both clean of new warnings. **Baseline, verified live at `1db093f`: 6 warnings, all pre-existing `too_many_arguments`** (identical to Story 7.2's documented baseline — nothing has changed it since). This story adds two new, narrow-signature free functions; it should not newly cross the `too_many_arguments` threshold anywhere, but confirm the post-change count explicitly rather than assuming.
  - State explicitly in Completion Notes whether the parser's behavior was additionally eyeballed against a real `fido2-token -I` invocation against actual hardware (not required for the unit-test suite to pass, but this project's convention is to note what real-hardware confirmation was or wasn't available in a non-interactive session — see Story 7.1/7.2's own Completion Notes for the pattern).

## Dev Notes

- **Scope is deliberately narrow and additive** — two new private free functions (a pure parser, an I/O wrapper) in `src/adapters/exec/mod.rs`, plus their unit tests. No new `Fido2Backend` port method (mirrors AD-21's "reuse over new method" precedent exactly), no new `DomainError` variant (reuses `AdapterFailure`, same as `fido2_token_has_pin`), no CLI change, no change to `Fido2Device`/enumeration/enrollment call sites. Per AC #4, this story does **not** wire `fido2_token_supports_uv` into any menu, enrollment flow, or user-facing message — that integration, plus the actual UV/PIN+UP/UP menu, is Story 7.5's job entirely. Resist the temptation to "finish the feature" by starting the menu here; it isn't this story's AC and would blur the two stories' review boundaries.
- **Why the `Result` must not be collapsed here:** the existing, structurally near-identical `client_pin` detection (AD-21/CAP-25) treats a query failure as "assume false, no warning" because it's a nice-to-have proactive hint. Story 7.4's AC #3 is explicit that a check-error must stay a *third*, distinguishable outcome — Story 7.5 needs to render "unavailable — no built-in verification" differently from "could not check: `<error>`" (per epics.md's Story 7.5 AC, line 1080). If this story collapses the error here, Story 7.5 has nothing left to distinguish. Keep `fido2_token_supports_uv`'s signature as `Result<bool, DomainError>`, full stop.
- **No architecture-spine amendment authored by this story.** CAP-28's AD is still listed as "pending Architect formalization" in epics.md's Requirements Inventory (line 110), but that same note already fully specifies this story's technique and this story's scope doesn't require a port/layer decision Winston would need to arbitrate (same "no new port" shape AD-21 already established) — implement per the note directly. Say so plainly in Completion Notes so it's visible that no AD-22 exists yet, in case Winston wants one formally recorded before Story 7.5 (which will need real architectural decisions: menu wiring, default-selection precedence with AD-16).
- **Real-hardware grounding already exists:** `REAL_INFO_OUTPUT_WITH_PIN` (captured 2026-08-10 from a real TOKEN2 FIDO2 Security Key, per its own comment) already contains the bare `uv` token, confirming `fido2-token -I` really does emit CTAP2 options this way on real hardware — this story's positive test case isn't speculative.

### Project Structure Notes

- Files touched (production): `src/adapters/exec/mod.rs` only — two new private functions near `parse_client_pin_configured`/`fido2_token_has_pin` (same file region, ~lines 950-1010).
- Files touched (tests): inline `#[cfg(test)]` unit tests in the same file, near the existing `parse_client_pin_configured` tests (~lines 3506-3535).
- No new files, no `Cargo.toml` changes, no changes to `src/domain/*`, `src/cli/*`, `src/ports/*`, `tests/unit/*`, or `tests/hardware/*`.
- Alignment with unified project structure: same-file, same-region addition as the existing PIN-detection precedent (AD-21) — no new module, no new port, no naming departure from the Consistency Conventions table.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 7.4: UV Capability Detection, lines 1038-1060] — acceptance criteria origin, verbatim.
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 7: FIDO2 Unlocking-Behavior Flags & Interactive Menu, lines 986-988] — epic-level framing.
- [Source: _bmad-output/planning-artifacts/epics.md, Requirements Inventory, line 110 (CAP-28 candidate note)] — the specific implementation technique this story follows.
- [Source: _bmad-output/planning-artifacts/epics.md#Story 7.5: Interactive Unlocking-Mode Menu, lines 1062-1094] — the consumer of this story's output; clarifies why the check-error/not-capable distinction (AC #3) matters and how the annotated-row UX will use it.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-21, lines 182-186] — the closest formalized precedent (reuse over new port method); this story follows the same placement rule.
- [Source: src/adapters/exec/mod.rs, `parse_client_pin_configured`, lines 975-981] — the exact pure-parser template this story mirrors.
- [Source: src/adapters/exec/mod.rs, `fido2_token_has_pin`, lines 952-966] — the exact I/O-wrapper template this story mirrors (with the one deliberate divergence noted in Dev Notes).
- [Source: src/adapters/exec/mod.rs, `wait_for_enough_fido2_devices`, lines 1012-1054, esp. line 1037] — shows the `.unwrap_or(false)` collapse pattern this story's new function must NOT replicate.
- [Source: src/adapters/exec/mod.rs, `REAL_INFO_OUTPUT_WITH_PIN` fixture and its tests, lines 3506-3535] — real captured sample and test-naming convention this story's new tests reuse/mirror.
- Verified live in this session against `baseline_commit` (`1db093f`): `cargo build` clean; `cargo test --lib --test unit` → 39 lib + 289 unit = 328 passed, 0 failed; `cargo clippy --all-targets` → 6 warnings, all pre-existing `too_many_arguments` (unchanged from Story 7.2's own documented baseline).

## Dev Agent Record

### Agent Model Used

Amelia (claude-sonnet-5)

### Debug Log References

### Completion Notes List

- Tasks 1-2: added `parse_uv_capable` (pure parser, exact-token split on `", "` for `"uv"`, mirroring `parse_client_pin_configured` precisely) and `fido2_token_supports_uv` (I/O wrapper, structurally identical to `fido2_token_has_pin`) to `src/adapters/exec/mod.rs`, placed directly beside their respective templates. `fido2_token_supports_uv`'s `Result<bool, DomainError>` is never collapsed anywhere in this story — per AC #3/Dev Notes, that's how the three-way capable/not-capable/check-error distinction is represented, left for Story 7.5's call site to interpret. No call site added (AC #4); `fido2_token_supports_uv` is marked `#[allow(dead_code)]` until Story 7.5 wires it in.
- Task 3: added 4 unit tests for `parse_uv_capable`, mirroring `parse_client_pin_configured`'s test shape exactly. The positive case reuses the existing `REAL_INFO_OUTPUT_WITH_PIN` fixture (already contains a real-captured bare `uv` token, per Story 6.6's 2026-08-10 capture) — no new fixture needed. The "disabled" negative case derives a synthetic fixture via `.replace(", uv,", ", nouv,")` on that same real fixture (commented as synthetic, not real-captured, per project convention on unverified-claim hygiene).
- Task 4: full regression pass verified live this session at `baseline_commit` (`1db093f`) plus this story's commits: `cargo build` clean, no warnings. `cargo test --lib --test unit` → 43 lib (+4 from this story) + 289 unit = 332 passed, 0 failed — consistent with the documented baseline (39+289=328) plus the 4 new tests. `cargo fmt --check` clean. `cargo clippy --all-targets` → 6 warnings, all pre-existing `too_many_arguments`, identical to the story's own documented baseline — the two new free functions (2 and 1 arguments respectively) did not newly cross the threshold anywhere.
- No real-hardware eyeballing of `fido2_token_supports_uv` against an actual `fido2-token -I` invocation was performed in this non-interactive dev-agent session (no interactive sudo/physical-touch channel available, same environment gap noted in prior Epic 7 stories' Completion Notes). This story's positive-case grounding instead rests on `REAL_INFO_OUTPUT_WITH_PIN` itself being a real 2026-08-10 capture (per Story 6.6) that already contains the bare `uv` token — confirmed by inspection, not a fresh live run. Per AC #4 there is no call site yet to hardware-gate a scenario off, so this doesn't block the story; worth a manual eyeball once Story 7.5 adds the first real caller.
- No AD-22 architecture-spine amendment authored, matching Dev Notes' guidance — the Requirements Inventory candidate note (epics.md line 110) already fully specified this story's technique and scope, and no new port/layer decision was required (same "no new port method" shape as AD-21). Flagging for Winston in case he wants one formally recorded before Story 7.5.

### File List

- src/adapters/exec/mod.rs
