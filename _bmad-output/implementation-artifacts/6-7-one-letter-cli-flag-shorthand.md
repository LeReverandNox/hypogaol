---
baseline_commit: 437b726d1d41a51933ac6087655133aac606302a
---

# Story 6.7: One-Letter CLI Flag Shorthand

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a user,
I want a one-letter shorthand for each subcommand's primary flags,
So that I can type common commands faster without giving up the long forms.

## Acceptance Criteria

1. **Given** any subcommand's `--help` output, **when** I view it, **then** every documented flag shows a short alias alongside its long form. [Source: epics.md#Story 6.7, lines 928-932]
2. **Given** a short alias, **when** I use it instead of the long form, **then** it behaves identically to the long form, for every flag across every subcommand. [Source: epics.md#Story 6.7, lines 934-936]
3. **Given** a same-subcommand collision on a flag's first letter, **when** aliases are assigned, **then** the colliding flag falls back to the next-most-mnemonic distinguishing letter instead. [Source: epics.md#Story 6.7, lines 938-940]
4. **Given** `-h` and `-V`, **when** aliases are assigned across the CLI, **then** neither is ever reassigned to a different flag. [Source: epics.md#Story 6.7, lines 942-944]

## Tasks / Subtasks

- [x] **Task 0: Read every file this story touches before changing anything** (AC: all)
  - Read in full: `src/cli/main.rs` (880 lines, whole file is in scope — every `#[arg(...)]` attribute across `Commands` and `CreateMode` needs a `short` added). Read `tests/unit/cli.rs` in full (341 lines) — it already exercises `Cli::try_parse_from`/a `help_text(&[...]) -> String` helper (built on `Cli::try_parse_from(...).unwrap_err().to_string()`, since `--help` short-circuits clap's parser with `Err(clap::Error)`) for exactly this kind of flag-presence/parse-behavior assertion; this story's tests extend that existing pattern, they don't invent a new one.
  - Confirm `short` is not used anywhere yet in `src/cli/main.rs` (verified during story creation via `grep -n short src/cli/main.rs` — zero matches) — this is a from-scratch addition across every flag, not a partial one needing audit of existing aliases.
  - Note: `Cli`/`Commands`/`CreateMode` currently derive only `Parser`/`Subcommand` (no `Debug`) and their fields (`command`, `mode`, and every subcommand's fields) are **not** `pub` — `tests/unit/cli.rs` is an external-crate integration test (`hypogaol::cli::main::Cli`), so it cannot inspect parsed field values directly today, only `Cli::try_parse_from(...).is_ok()`/`.is_err()` and `--help` text content. See Task 6 for why this story adds `#[derive(Debug)]` (not `pub` fields) to close that gap cheaply for AC #2's "behaves identically" testing.

- [x] **Task 1: Design and record the full flag → short-alias table before touching any code** (AC: #1, #2, #3, #4)
  - This is the one piece of design judgment AD-9/AD-19/etc.-style architecture text doesn't pre-resolve for this story (the Consistency Conventions row states the *rule*, not a worked table) — do this once, up front, and implement exactly it, rather than deriving ad hoc aliases flag-by-flag while editing (which is how two colliding flags could silently end up with the same letter). Aliases are assigned **per subcommand** (collisions are scoped to "same-subcommand" per AC #3 and the Consistency Conventions row) — the same long flag name may legitimately get a different short letter in two different subcommands if their collision environments differ; that is expected, not a bug, given the per-subcommand scoping the epic/architecture text both use.
  - **Proposed table (apply exactly, or record and justify any deviation in Completion Notes):**

    | Subcommand | Flag (declaration order) | Short | Rationale |
    | --- | --- | --- | --- |
    | `create file` / `create device` (identical flag set, apply the same table to both — `size` is `u64` on `file`, `Option<u64>` on `device`, doesn't affect the alias) | `--size` | `-s` | First declared, first letter, no collision at assignment time |
    | | `--filesystem` | `-f` | First letter, no collision at assignment time |
    | | `--scaffold-hooks` | `-c` | Collides with `--size`'s `-s` (declared later); falls back to the next letter within its own name that's still mnemonic — 2nd letter of "s**c**affold" |
    | | `--label` | `-l` | First letter, no collision |
    | | `--fido2-device` | `-d` | Collides with `--filesystem`'s `-f` (declared later); falls back to "fido2-**d**evice" — the operative noun, distinguishing letter |
    | | `--user-verification` | `-u` | First letter, no collision within this subcommand (no other `-u` flag on `create file`/`create device`) |
    | `unlock` | `--read-only` | `-r` | First letter, no collision |
    | | `--skip-hooks` | `-s` | First letter, no collision |
    | `enroll` | `--label` | `-l` | First letter, no collision |
    | | `--fido2-device` | `-f` | First letter, no collision within this subcommand (no `--filesystem` here, unlike `create`) |
    | | `--unlock-fido2-device` | `-u` | First letter, first declared among `-u`-starting flags |
    | | `--user-verification` | `-v` | Collides with `--unlock-fido2-device`'s `-u` (declared later); falls back to "verification"'s `-v` — distinct from `-V` (version, case-sensitive, untouched per AC #4) |
    | `revoke` | `--label` | `-l` | First letter, only flag, no collision |
    | `close` | `--skip-hooks` | `-s` | First letter, only flag, no collision |
    | `close-all` | `--skip-hooks` | `-s` | First letter, only flag, no collision |
    | `resize` | `--size` | `-s` | First letter, only flag, no collision |
    | `info` | *(none)* | — | Read-only, single positional `path`, no flags to alias |
    | `slam` | *(none)* | — | No flags to alias |

  - **Positional `path` arguments are out of scope for this story.** AC #1/#2/#3 all say "flag" — clap positionals (`path` on `unlock`/`enroll`/`revoke`/`close`/`resize`/`info`/`create file`/`create device`) are not flags and have no `--long`/`-short` form to alias in the first place (confirmed by the existing `tests/unit/cli.rs::*_help_lists_path_as_positional` tests, which assert `<PATH>` appears and `--path` does **not**). Do not add a `-p`/`--path` alias — that would be a new, unrequested flag form, not a short alias for an existing one.
  - **`-h`/`-V` (help/version) are clap-automatic and already excluded from every collision above** — none of the proposed short letters is `h` or `V` (note: `-v` lowercase, used for `enroll`'s `--user-verification`, is a distinct, case-sensitive short flag from `-V` uppercase version — clap treats short flags case-sensitively, confirmed no conflict). Do not pass `short = 'h'`/`short = 'V'` to any `#[arg(...)]`.

- [x] **Task 2: Add `short` to every flag in `Commands::Unlock`/`Enroll`/`Revoke`/`Close`/`CloseAll`/`Resize`** (AC: #1, #2, #3, #4)
  - `src/cli/main.rs` lines 42-135 (`Commands` enum). For each `#[arg(long, ...)]` (and the bare `#[arg(long)]` boolean flags), add `short = '<letter>'` per Task 1's table, keeping existing `value_parser`/`requires`/`default_value` attributes unchanged, e.g. `#[arg(short = 'r', long)]` for `Unlock::read_only`, `#[arg(short = 'l', long, value_parser = parse_label)]` for `Revoke::label`.
  - `Enroll::fido2_device`/`Enroll::unlock_fido2_device` keep their existing `requires = "..."` attributes unchanged alongside the new `short`.

- [x] **Task 3: Add `short` to every flag in `CreateMode::File`/`CreateMode::Device`** (AC: #1, #2, #3, #4)
  - `src/cli/main.rs` lines 141-219. Apply Task 1's `create file`/`create device` row to both variants identically — `size`, `filesystem`, `scaffold_hooks`, `label`, `fido2_device`, `user_verification` each gain the same `short` in both `File { ... }` and `Device { ... }` (the two variants declare the same six flags in the same order; `Device::size` is `Option<u64>` vs `File::size`'s `u64`, which doesn't change its short letter). Keep `value_enum`/`default_value`/`value_parser` attributes unchanged.

- [x] **Task 4: Verify no accidental clap conflict at compile/parse time** (AC: #1, #2, #3, #4)
  - `cargo build` — clap validates `short`/`long` uniqueness per-command at derive-macro-expansion time for some conflict classes, but duplicate `short` letters *within the same variant* may only surface as a runtime panic the first time that command is parsed (clap's derive doesn't catch every case at compile time). Manually cross-check Task 1's table against the actual `#[arg(...)]` attributes once written: no two flags in the same `Commands`/`CreateMode` variant share a `short`.
  - Run `cargo run -- --help`, `cargo run -- create file --help`, `cargo run -- create device --help`, `cargo run -- unlock --help`, `cargo run -- enroll --help`, `cargo run -- revoke --help`, `cargo run -- close --help`, `cargo run -- close-all --help`, `cargo run -- resize --help`, `cargo run -- info --help`, `cargo run -- slam --help` and visually confirm every flag's help line shows `-X, --long-form` (clap's default rendering) with the letter from Task 1's table, and that no subcommand panics on startup.

- [x] **Task 5: Regression-check every existing flag combination still works via its long form** (AC: #2)
  - `tests/unit/cli.rs` already has flag-`requires`/rejection tests (`enroll_rejects_fido2_device_flag_given_without_its_unlock_pair`, `enroll_rejects_unlock_fido2_device_flag_given_without_its_pair`, `enroll_accepts_both_explicit_device_flags_together`, `create_file_rejects_an_empty_label`, `create_device_rejects_a_whitespace_only_label`) built entirely on long-form flags — adding `short` must not change any of their outcomes (clap's `requires`/`value_parser` semantics apply identically regardless of which form invoked the flag). Re-run `make test` after Tasks 2-3 and confirm all pre-existing `tests/unit/cli.rs` tests still pass unmodified — this is a regression signal, not new coverage.

- [x] **Task 6: Add `#[derive(Debug)]` to `Cli`/`Commands`/`CreateMode` so short-vs-long equivalence is directly testable** (AC: #2)
  - `src/cli/main.rs` lines 26-31 (`Cli`), 33-136 (`Commands`), 141-219 (`CreateMode`): add `Debug` to each existing `#[derive(...)]` (`Parser`/`Subcommand`). This is the cheapest way to make AC #2 ("behaves identically to the long form") mechanically checkable from `tests/unit/cli.rs`'s external-crate position, since none of these types' fields are `pub` and adding `pub` to every field would be a much larger, unrequested surface-area change. `Debug` output is never shown to a user (it's not used anywhere in `run()`'s printed/error text — confirmed by reading `run()`, lines 788-880, which only ever destructures fields, never formats the enum itself) — this is purely a test-seam addition, not a UX change.
  - Do **not** add `PartialEq`/`Clone` — not needed for the equivalence tests below (string-comparing two `Debug` outputs is sufficient and simpler than deriving equality across types containing `PathBuf`/`Option<String>`/nested enums).

- [x] **Task 7: Add short-vs-long equivalence tests to `tests/unit/cli.rs`** (AC: #2, #3)
  - For a representative flag in each subcommand (at minimum one per subcommand that has flags — no need to test every single flag exhaustively if the parsing mechanism is proven generic, but cover at least one boolean flag and one value-taking flag per subcommand, plus every flag directly involved in a Task 1 collision), assert that parsing the same command line with the short form and with the long form produces identical `format!("{cli:?}")` output. Pattern:
    ```rust
    #[test]
    fn unlock_read_only_short_and_long_forms_are_equivalent() {
        let short = Cli::try_parse_from(["hypogaol", "unlock", "/tmp/v", "-r"]).unwrap();
        let long = Cli::try_parse_from(["hypogaol", "unlock", "/tmp/v", "--read-only"]).unwrap();
        assert_eq!(format!("{short:?}"), format!("{long:?}"));
    }
    ```
  - **Explicit collision-pair coverage required** (these are the tests that actually prove AC #3, not just AC #2): for `create file`/`create device`, a test asserting `-s` maps to `--size` and *not* `--scaffold-hooks` (e.g. parse `create file <path> -s 64M` and confirm it's equivalent to `--size 64M`, while `create file <path> -c` behaves like `--scaffold-hooks`, not `--size`); similarly `-f` → `--filesystem` (not `--fido2-device`) and `-d` → `--fido2-device` (not `--filesystem`) on both `create` variants; for `enroll`, `-u` → `--unlock-fido2-device` (not `--user-verification`) and `-v` → `--user-verification` (not `--unlock-fido2-device`).
  - **Help-text short-alias presence tests** (AC #1), extending the existing `help_text(&[...])` helper pattern already in this file: for each subcommand, assert its `--help` output contains the expected `-X` token alongside the existing long-form assertions already present (e.g. extend `unlock_help_lists_read_only_flag` or add a sibling test asserting `help.contains("-r, --read-only")` — check clap 4.6.4's actual rendered format first via Task 4's manual `--help` runs before hardcoding the exact separator/spacing, since clap's help formatting is not this story's to redesign).
  - **`-h`/`-V` untouched test** (AC #4): a test asserting `Cli::try_parse_from(["hypogaol", "-V"])` still errors with clap's version short-circuit (mirroring the existing `help_text` helper's `--help` pattern — `-V`/`--version` behave the same way, both return `Err(clap::Error)` with `ErrorKind::DisplayVersion` and short-circuit before subcommand dispatch) and that no subcommand's `--help` text shows `-h`/`-V` bound to anything other than help/version.

- [ ] **Task 8: Full regression pass**
  - `cargo build` succeeds, no panics on any subcommand's `--help`/actual invocation shape.
  - `make test` passes with all prior tests green (verified current baseline **279 total: 33 lib + 246 tests/unit**, independently re-run against this story's own `baseline_commit` during story creation — not taken from a prior story's self-reported claim) plus this story's new tests. This story is CLI-attribute-only — no `domain`/`ports`/`adapters` change expected; if any turns out to be needed, that is a signal of scope drift, stop and re-check against Task 1's design first.
  - `cargo fmt --check` and `cargo clippy --all-targets` both clean — no new warnings (baseline: 7 pre-existing `too_many_arguments` warnings per Story 6.6's own verified count, unrelated to this story — confirm the same count, not a new one, after this story's changes).
  - State explicitly in Completion Notes the actual final test count (lib + tests/unit), verified against real `cargo test` output, not a memory/estimate — matches this project's own recurring-pattern watchlist (self-reported counts drifting from reality hit Stories 6.4/6.5/6.6 already, each caught only in review).

## Dev Notes

- **No new port, no new architectural layer, no `domain`/`ports`/`adapters` change at all — narrower in scope than every other Epic 6 story so far, and narrower even than Story 6.6.** CAP-20's row in the Capability → Architecture Map states `cli/` is the only location, governed only by "Consistency Conventions" — there is no AD dedicated to CAP-20 the way AD-20/AD-21 exist for CAP-24/CAP-25; the Consistency Conventions table row **is** the complete architectural spec for this story: *"Each flag's short alias is its long name's first letter where unambiguous; a same-subcommand collision falls back to the next-most-mnemonic distinguishing letter; `-h`/`-V` are never reassigned."* This story's entire surface area is `src/cli/main.rs` (production) and `tests/unit/cli.rs` (tests). [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md, Consistency Conventions table, line 195; Capability → Architecture Map, line 296]

- **Task 1's alias table is this story's own design decision, not a literal architecture-text lookup** — the same posture Story 6.6 took when it had to design collision resolution the spike didn't hand it pre-resolved. Implement Task 1's table as specified; if a genuinely more mnemonic letter is found while implementing (e.g. a reviewer/user preference surfaces post-review, mirroring Story 6.6's own multiple post-review wording iterations), that's an expected kind of follow-up patch, not a sign this story's initial table was wrong to commit to.

- **Why `Debug`-string comparison (Task 6/7) instead of exposing fields as `pub` or adding `PartialEq`.** `Cli`'s/`Commands`'s/`CreateMode`'s fields are deliberately private today (`tests/unit/cli.rs` only ever calls `Cli::try_parse_from(...).is_ok()`/`.is_err()` or inspects `--help` text, never a field) — this story is the first to need to assert two *different* parses produced the *same* value, which neither existing pattern supports. Adding `Debug` is the minimal-surface-area seam: no runtime behavior change (never printed to a user, confirmed by reading all of `run()`), no new pub API, and it composes with the existing `try_parse_from` test style rather than replacing it.

- **Per-subcommand alias scoping is deliberate, not an oversight, when the same flag name gets different letters in different subcommands.** `--fido2-device` is `-f` in `enroll` (no `--filesystem` there to collide with) but `-d` in `create file`/`create device` (where `--filesystem` claims `-f` first). AC #3 and the Consistency Conventions row both scope collision resolution to "same-subcommand" — there is no cross-subcommand consistency requirement in either the AC text or the architecture text, so this divergence is compliant, not a gap to "fix" by forcing one global mapping.

- **Recurring review-pattern watchlist from Epic 4/5/6 retros — apply proactively:**
  - New pure parsing/logic functions shipping without a direct unit test: not applicable here in the usual sense (this story adds no new parsing *logic*, only clap attribute metadata), but the *collision-resolution table itself* is exactly the kind of "logic with no test" risk in spirit — Task 7's explicit collision-pair tests exist specifically to close that gap, don't skip them as "just attributes."
  - Self-reported completion-note claims (test counts, grep counts) not matching actual output — hit in Stories 5.1/5.2/5.3/6.4/6.5/6.6, each caught only in review. This story's baseline (279 total: 33 lib + 246 tests/unit) was independently re-verified via a real `cargo test` run against `baseline_commit` during story creation, not copied from Story 6.6's own final Completion Notes number — confirm the same discipline for this story's own final count in Task 8.

### Project Structure Notes

- Files touched (production): `src/cli/main.rs` only — every `#[arg(...)]` attribute in `Commands` (lines 42-135) and `CreateMode` (lines 141-219) gains a `short`; `Cli`/`Commands`/`CreateMode` each gain `Debug` in their existing `#[derive(...)]` list.
- Files touched (tests): `tests/unit/cli.rs` only — new short-vs-long equivalence tests and help-text short-alias assertions, extending the existing `help_text`/`Cli::try_parse_from` patterns already in this file. No new test file needed.
- No changes to `src/domain/`, `src/ports/`, `src/adapters/`, `tests/unit/fakes.rs`, or `README.md` (confirmed during story creation: README has no per-flag long-form reference table that would need a short-form update — its CLI mentions are prose-level, e.g. `--skip-hooks`/`--size` referenced narratively, not a flag-by-flag doc block).

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 6.7: One-Letter CLI Flag Shorthand, lines 922-944] — acceptance criteria origin, verbatim.
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 6: Volume Resilience, Filesystem Choice & Everyday Polish, lines 766-768, 178-180] — epic-level framing; confirms no new port/architectural layer for any Epic 6 story.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md, Consistency Conventions table, line 195] — the complete mechanism spec for CAP-20: first-letter default, same-subcommand collision fallback rule, `-h`/`-V` untouched.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md, Capability → Architecture Map, line 296] — confirms CAP-20 lives entirely in `cli/`, governed only by Consistency Conventions (no dedicated AD).
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md, FR Coverage Map, line 149] — FR20: Epic 6 - One-letter shorthand aliases across the CLI.
- [Source: src/cli/main.rs, lines 1-880] — full current CLI implementation, read in full during story creation; the complete flag inventory (Task 1's table) is derived directly from this file, not inferred.
- [Source: tests/unit/cli.rs, lines 1-341] — full current CLI test file, read in full during story creation; the existing `help_text`/`Cli::try_parse_from` helper patterns this story's new tests extend.
- [Source: _bmad-output/implementation-artifacts/6-6-proactive-fido2-pin-status-guidance.md] — previous story in this epic; confirms the verified baseline this story starts from (279 total, independently re-confirmed via a real `cargo test` run at this story's own `baseline_commit` during story creation) and the project's recurring self-reported-count-drift watchlist this story's Task 8 explicitly guards against.
- [Source: _bmad-output/implementation-artifacts/sprint-status.yaml] — confirms this is the seventh story of Epic 6 (epic already `in-progress` since Story 6.1); no epic-6 action item currently references CLI flag shorthand.
- Verified locally (2026-08-11, story creation): `grep -n short src/cli/main.rs` → zero matches, confirming no existing short aliases to audit/preserve; `cargo test --lib` → 33 passed; `cargo test --lib --test unit` → 246 passed (baseline 279 total, matching Story 6.6's own final self-reported count exactly this time — first Epic 6 story where the independently re-verified number matches the prior story's claim with no drift).

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

- Tasks 0-6: implemented Task 1's alias table exactly as proposed (validated against the actual `src/cli/main.rs` before coding — declaration order, collisions, and rationale all matched with no deviation needed). Added `short = '<letter>'` to every `#[arg(...)]` in `Commands` and both `CreateMode::File`/`Device` variants. Added `Debug` to `Cli`, `Commands`, `CreateMode`, and (required transitively, not explicitly called out in Task 6 but needed for the derive to compile since it's a field type) `CliFilesystem`. `cargo build` clean; manually ran `--help` for all 11 subcommand forms, confirmed every short letter renders exactly per Task 1's table and no subcommand panics. `make test` re-run after these changes: 279/279 pre-existing tests pass unmodified (33 lib + 246 tests/unit) — no regressions from the `short` additions.
- Task 7: added 28 new tests to `tests/unit/cli.rs` — short-vs-long equivalence for a representative flag per subcommand, explicit collision-pair coverage for every Task 1 collision (`create file`/`device`'s `-s`/`-c`/`-f`/`-d`, `enroll`'s `-u`/`-v`), help-text short-alias presence per subcommand, and `-h`/`-V` untouched tests (both still short-circuit correctly, and top-level `--help` still lists them). `cargo build --tests` clean, `cargo fmt` applied (one line needed rewrapping), `make test` green: 307/307 (33 lib + 274 tests/unit, baseline 279 + 28 new).
- Task 8: `cargo clippy --all-targets` — 5 pre-existing `too_many_arguments` warnings, none new or removed by this story's changes. Note: independently re-verified against this story's own `baseline_commit` (437b726) by stashing, checking out that commit's files, and re-running clippy there: the real baseline is **5**, not the **7** this story's own Dev Notes/Task 8 text stated (itself sourced from "Story 6.6's own verified count"). This is exactly the self-reported-count-drift pattern this story's own watchlist (Dev Notes) warns about — flagging it rather than silently using the wrong number. What matters for this story's regression gate is unaffected either way: 5 before this story's changes, 5 after, zero drift introduced by this story.

### File List

- `src/cli/main.rs` (modified)
- `tests/unit/cli.rs` (modified)
