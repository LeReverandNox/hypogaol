---
baseline_commit: f867364ad071992bdd0a48efcff2fd5eb79e7e98
---

# Story 6.6: Proactive FIDO2 PIN-Status Guidance

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a user,
I want the tool to warn me upfront when a FIDO2 device has a PIN configured, and give me plain-language guidance if I enter it wrong,
so that a PIN-required device never surprises me mid-touch-prompt or leaves me confused by a generic failure.

## Acceptance Criteria

1. **Given** a FIDO2 device with a PIN configured is involved in enroll (a specific device already selected), **when** enroll runs, **then** a warning naming that device and stating PIN entry will be required is shown before the touch/PIN prompt. [Source: epics.md#Story 6.6, lines 906-908]
2. **Given** an unlock where cryptsetup will match the stored token to whichever device answers (no device pre-selected), **when** any currently-enumerated device has a PIN configured, **then** a blanket warning listing every such device is shown before the blocking prompt. [Source: epics.md#Story 6.6, lines 910-912]
3. **Given** a wrong PIN is entered during a retry, **when** the authenticator reports it, **then** the tool shows a plain-language warning naming that retries are limited and what happens if they run out — never a generic "authentication failed". [Source: epics.md#Story 6.6, lines 914-916]
4. **Given** the subprocess call handling secret PIN entry, **when** stderr is captured for this diagnostic text, **then** stdin/stdout stay strictly passthrough for the actual PIN entry, and stderr is read concurrently (not only after the child exits) to avoid a pipe-buffer deadlock on a long touch/PIN-blocking call. [Source: epics.md#Story 6.6, lines 918-920]

## Tasks / Subtasks

- [ ] **Task 0: Read every file this story touches before changing anything** (AC: all)
  - Read in full: `src/adapters/exec/mod.rs` lines 485-736 (`Fido2Device` struct, `list_fido2_devices`, `wait_for_enough_fido2_devices`, `print_numbered_fido2_devices`, `prompt_for_device_index`, `resolve_interactive_selection`, `validate_device_enumerated`, `resolve_explicit_selection`, `resolve_device_selection`, `fido2_verification_args` — every device-selection/enumeration function this story extends), lines 1222-1258 (`open`'s presence-wait + `cryptsetup open --token-only` call), lines 1260-1290 (`resize`'s `cryptsetup resize --token-only` call — same touch/PIN-blocking shape as `open`, relevant to Task 5's scope decision below), lines 1444-1600ish (`enroll_fido2_key`'s full impl: device-selection resolution, the two `systemd-cryptenroll` branches with/without a transient passphrase), lines 55-59 (`privileged()` — wraps a `Command` in `sudo`, used by `open`/`resize` but **not** by `enroll_fido2_key`'s `systemd-cryptenroll` call, which runs directly since it operates on the LUKS2 header file and needs no elevation), lines 292-330 (`run_piping_stdin` — the existing concurrent-thread pattern for a pipe that could otherwise deadlock; `enroll_fido2_key`'s `--unlock-key-file` path doesn't use this today because the passphrase travels via a temp file, not stdin, but this function is the direct precedent for Task 5's concurrent-stderr-read thread). Also skim `src/ports/fido2_backend.rs` (confirm the trait is genuinely untouched by this story — see Dev Notes) and `tests/unit/fakes.rs`'s `FakeFido2Backend` (confirm it too is genuinely untouched).
  - **No spike needed for the PIN-status detection mechanism** — Task 0's own research (see Dev Notes' "Verified `fido2-token -I` output format") already confirms the exact field and parsing rule against a real device in this dev environment. **A spike is required for Task 5** (the reactive wrong-PIN-retry warning) — the exact stderr text `cryptsetup`/`systemd-cryptenroll` emit on a wrong PIN attempt is not documented anywhere web-verifiable (checked 2026-08-10 — see References) and must be captured from a real deliberate-wrong-PIN attempt against the physical FIDO2 device already present in this environment (`fido2-token -L` reports `/dev/hidraw5`, PIN already configured — confirmed live, see Dev Notes) before any detection logic is written against guessed text.

- [ ] **Task 1: Add `client_pin: bool` to `Fido2Device` and a PIN-status query function** (AC: #1, #2)
  - `src/adapters/exec/mod.rs`: add a `client_pin: bool` field to the `Fido2Device` struct (~line 490).
  - Add a new pure-parsing function, e.g.:
    ```rust
    /// Runs `fido2-token -I <path>` (no `-c` — never prompts for a PIN or
    /// touch, safe to call during plain enumeration) and parses whether the
    /// device currently has a PIN configured, per AD-21/CAP-25.
    fn fido2_token_has_pin(path: &str) -> Result<bool, DomainError> {
        let output = Command::new("fido2-token")
            .args(["-I", path])
            .output()
            .map_err(|e| DomainError::AdapterFailure(format!("failed to run fido2-token -I: {e}")))?;
        if !output.status.success() {
            return Err(DomainError::AdapterFailure(format!(
                "fido2-token -I failed for {path}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(parse_client_pin_configured(&String::from_utf8_lossy(&output.stdout)))
    }

    /// Pure parser, unit-testable without a real device: `true` iff the
    /// `options:` line contains the bare token `clientPin` (CTAP2
    /// authenticatorGetInfo semantics: `clientPin` option `true` = a PIN is
    /// currently set). `fido2-token` renders a `false` boolean option with a
    /// `no` prefix instead of omitting it (e.g. `noplat`, `noalwaysUv`), so
    /// `noclientPin` (PIN capability present but not set) and no `clientPin`
    /// token at all (capability unsupported) both correctly parse as `false`.
    fn parse_client_pin_configured(fido2_token_info_output: &str) -> bool {
        fido2_token_info_output
            .lines()
            .find_map(|line| line.strip_prefix("options: "))
            .map(|options| options.split(", ").any(|opt| opt == "clientPin"))
            .unwrap_or(false)
    }
    ```
  - Add unit tests (in this file's existing `#[cfg(test)]` module, alongside `fido2_verification_args_*`) covering: the real captured sample line from Dev Notes with `clientPin` present → `true`; the same line with `clientPin` replaced by `noclientPin` → `false`; the token absent entirely → `false`; an empty/malformed string → `false` (no panic).

- [ ] **Task 2: Populate `client_pin` once per settled device list, never inside the poll loop** (AC: #1, #2 — see Dev Notes' "Avoid poll-loop spam" for why this task exists as its own step)
  - `wait_for_enough_fido2_devices` (~line 533): its polling loop must keep calling the existing `list_fido2_devices()` unchanged (path+description only, no `-I` call) every 500ms — do **not** add a `client_pin` lookup inside the loop body. Once the loop is about to return `Ok(devices)` (the `devices.len() >= needed` branch), enrich that final `Vec<Fido2Device>` with one `fido2_token_has_pin` call per device before returning it. This is the only place `client_pin` gets populated for `resolve_interactive_selection`'s (enroll) and `open`'s (unlock) callers.
  - `resolve_device_selection`'s `Fido2DeviceSelection::Explicit` branch (~line 710) calls `list_fido2_devices()` directly, bypassing the poll loop above — after resolving `resolve_explicit_selection`'s `(new_path, existing_path)`, look up `client_pin` for just those 1-2 resolved paths directly (via `fido2_token_has_pin`), not by enriching the whole enumerated list — cheaper and matches AD-21's framing that the resolvers "know the specific device" once resolved.

- [ ] **Task 3: Print the device-specific warning from enroll's resolvers** (AC: #1)
  - In `resolve_interactive_selection` and `resolve_explicit_selection` (or a shared point right after `resolve_device_selection` returns `(new_device, existing_device)` in `enroll_fido2_key`, whichever avoids duplicating the print call in two places — prefer centralizing in `resolve_device_selection` itself, printing for whichever of `new_device`/`existing_device` has `client_pin == true` once both are known), print a warning naming the specific device before returning to `enroll_fido2_key`, which then proceeds straight to the blocking `systemd-cryptenroll` call. Example wording (adjust to match this file's existing tone, e.g. `wait_for_enough_fido2_devices`'s own messages): `"Heads up: {device} has a PIN configured — you'll be asked to enter it."`
  - This applies to **both** create's bootstrap-enroll call and a standalone `enroll` — both go through this same `enroll_fido2_key`/`resolve_device_selection` path, so no separate wiring is needed for `create.rs`.

- [ ] **Task 4: Print the blanket warning from `open`'s presence-wait** (AC: #2)
  - `open()` (~line 1222): after `wait_for_enough_fido2_devices(1)` returns (now carrying populated `client_pin` per Task 2), filter for `client_pin == true` and, if the resulting list is non-empty, print a blanket warning listing every such device **before** the `cryptsetup open --token-only` call. Example wording: `"Heads up: the following currently-plugged-in security keys have a PIN configured — you may be asked to enter one: {list}."` This must fire whether or not `read_only` is set (no scoping by read-only here — read-only unlock still touches a real device).

- [ ] **Task 5: Reactive wrong-PIN-retry warning via concurrent stderr capture** (AC: #3, #4)
  - **First, spike against real hardware** (per Task 0): deliberately trigger a wrong-PIN retry against the real device in this environment — run a real `unlock` (or `enroll`) against a test volume with a PIN-protected key enrolled, and when prompted, enter an incorrect PIN once. Capture the literal stderr `cryptsetup`/`systemd-cryptenroll` produces for that attempt (temporarily redirect/tee stderr, or run the raw `cryptsetup open --token-only`/`systemd-cryptenroll` command by hand outside the tool first). Record the exact captured text verbatim in Completion Notes — do not guess or assume wording from unconfirmed web sources (see References — GitHub issues confirm prompt strings like "Please enter security token PIN" exist but do **not** confirm the wrong-PIN-retry stderr wording itself).
  - Once the real text is known, change the blocking calls from `.status()` (fully inherited stdio) to a spawn where **only stderr is piped**, stdin/stdout stay inherited (Rust's `Command` default is inherit unless overridden, so simply adding `.stderr(Stdio::piped())` before `.spawn()` is sufficient — no explicit `.stdin(Stdio::inherit())`/`.stdout(Stdio::inherit())` needed). Read the piped stderr **concurrently on a separate thread** while the main thread calls `child.wait()` — mirroring `run_piping_stdin`'s existing `std::thread::spawn` pattern (line 313), which exists for exactly this deadlock reason (AD-3's Epic 6 amendment states it explicitly: piping stderr while stdin/stdout stay inherited risks a pipe-buffer deadlock on a long touch/PIN-blocking call if the child writes enough stderr before it's drained). The reader thread should scan each line as it arrives for the wrong-PIN-retry signal identified by the spike and print the plain-language warning as soon as it's seen (not batched until the child exits) — and forward/print any other stderr line unchanged if this file's existing convention already surfaces `cryptsetup`/`systemd-cryptenroll` stderr on failure (check `AdapterFailure`'s existing construction at the end of `open`/`enroll_fido2_key` — those currently only report a generic message without the real subprocess's own stderr; decide whether to also capture the full stderr text here for that error path, consistent with this story's spirit, and note the decision either way in Completion Notes).
  - Apply this to: `open()`'s `cryptsetup open --token-only` call (unlock, AC #2/#3 both apply here), and both branches of `enroll_fido2_key`'s `systemd-cryptenroll` call (enroll and create's bootstrap step, AC #1/#3).
  - **Scope decision needed and must be documented**: `resize`'s `cryptsetup resize --token-only` call (line 1274) is architecturally identical to `open`'s — same `--token-only` re-authentication against the enrolled FIDO2 token, same touch/PIN-blocking shape — but epics.md's ACs for this story only name enroll/unlock literally, and AD-21 only describes `enroll_fido2_key`'s resolvers and `LuksBackend::open`'s presence-wait loop, never mentioning `resize`. AD-3's amended Rule text, however, is written per-subprocess-call generically ("any subprocess invocation that may involve secret entry... this same subprocess call... on a long touch/PIN-blocking call"), not scoped to two named workflows. Recommendation: apply the same concurrent-stderr-capture + wrong-PIN warning to `resize`'s call too, for consistency and because a `resize` invocation is just as likely to hit a wrong-PIN retry as `open`/`enroll` — but this is a judgment call, not a literal AC requirement; if deferred, note it explicitly as a documented gap (matching this project's established convention for logged, not silently dropped, scope decisions).

- [ ] **Task 6: Regression-proof the concurrent-stderr-read pattern without needing real hardware** (AC: #4)
  - Add a hardware-independent test (in `src/adapters/exec/mod.rs`'s own `#[cfg(test)]` module, or a small helper extracted so it's testable without a real `fido2-token`/`cryptsetup` binary) proving the concurrent-read shape itself doesn't deadlock: spawn a real child process (e.g. `sh -c` writing well past a pipe buffer's worth of bytes to stderr — 128KB+ — while doing nothing on stdin) through the same piped-stderr-plus-inherited-stdio-plus-background-reader-thread shape Task 5 implements, and assert it completes within a short timeout rather than hanging. This mirrors Story 6.5's Task 12 precedent (a real, non-fake regression test for an OS-level concurrency property that needs no FIDO2 hardware) — the goal is to catch a regression to reading stderr only after `child.wait()` (which would reintroduce the exact deadlock AD-3's amendment describes) the same way Task 12 caught a regression to the wildcard lock-drop bug.

- [ ] **Task 7: Full regression pass**
  - `cargo build` succeeds. `make test` passes with all prior tests green (baseline **258 total: 17 lib + 241 tests/unit**, per Story 6.5's own verified Completion Notes) plus this story's new tests. This story adds **no new `domain`/`ports` code and no new `FakeFido2Backend`/`FakeFilesystemBackend` fields** (see Dev Notes) — if any change turns out to be needed in `domain`, `ports`, or `tests/unit/fakes.rs`, treat that as a signal the design has drifted from AD-21's explicit "no new port method" framing and re-check before proceeding, don't just patch around it.
  - `cargo fmt --check` and `cargo clippy --all-targets` both clean — no new warnings.
  - State explicitly in Completion Notes whether real hardware was available and what was/wasn't verified on it (this project's standing convention, e.g. Stories 4.3, 5.1, 5.2, 6.1-6.5) — this story is unusual in that its *primary* correctness mechanisms (the `-I` output parser, Task 1; the concurrent-stderr-read shape, Task 6) need no hardware at all, but Task 5's exact wrong-PIN stderr text is only discoverable via the real device already present in this environment, so state clearly whether that spike was actually run and what text was captured, or flag it as a retrospective action item for `LeReverandNox` if it could not be (matching Story 6.5's own disclosed-gap convention in `sprint-status.yaml`'s `action_items`).

## Dev Notes

- **No new port, no new architectural layer, and — narrower than every other Epic 6 story so far — no `domain` or `ports` change at all.** AD-21 states explicitly: "no new `Fido2Backend` port method, per the reuse-over-new-method philosophy AD-14/AD-15 already established," and the existing device-enumeration machinery (`Fido2Device`, `list_fido2_devices`, the selection resolvers) already lives entirely inside `adapters::exec`, never in `domain`/`cli` — "per the existing, pre-Epic-6 precedent that FIDO2 device-selection UX lives inside `adapters::exec`, not `domain`/`cli`." This story's entire surface area is `src/adapters/exec/mod.rs`. `src/ports/fido2_backend.rs`, `src/domain/workflows/{enroll,unlock,create}.rs`, `src/domain/errors.rs`, `src/cli/ux.rs`, and `tests/unit/fakes.rs` should all be **untouched** by a correct implementation — if a change seems needed in any of them, stop and re-read AD-21 before proceeding, since that's a strong signal of scope drift. [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-21, lines 182-186]

- **All warnings print directly via `println!`/`eprintln!` from inside `adapters::exec`, never through `cli::ux::translate`.** This isn't a gap — it's the existing, established pattern for exactly this kind of advisory (non-error) text: `wait_for_enough_fido2_devices` already prints its own "Found N FIDO2 security keys..." message directly (line 542-547), with no `cli`/`ux` involvement at all. `cli::ux::translate` exists for `DomainError -> plain text`, and `translate_stage`/`translate_hook_warning` exist for `domain`-owned typed enums `cli` receives back — neither shape fits here, since these warnings originate and terminate entirely inside `adapters::exec` with no `domain` layer in between. [Source: src/adapters/exec/mod.rs:527-552 — existing direct-print precedent]

- **Verified `fido2-token -I` output format (2026-08-10, against the real device present in this dev environment, `/dev/hidraw5`, TOKEN2 FIDO2 Security Key, PIN already configured on it):**
  ```
  options: rk, up, uv, noplat, noalwaysUv, credMgmt, authnrCfg, bioEnroll, clientPin, largeBlobs, pinUvAuthToken, setMinPINLength, makeCredUvNotRqd, credentialMgmtPreview, userVerificationMgmtPreview
  ...
  pin retries: 8
  pin change required: false
  ```
  `fido2-token -I <device>` (no `-c` flag) never prompts for a PIN or touch — confirmed by running it live with zero prompt. Every boolean CTAP2 option on the `options:` line is rendered either bare (`true`) or with a `no` prefix (`false`) — e.g. `noplat`, `noalwaysUv` sit alongside bare `uv`, `clientPin` in the same real output above, confirming the convention applies uniformly, not just to `clientPin`. Per CTAP2's `authenticatorGetInfo` semantics, the `clientPin` option is `true` when a PIN is currently set, `false` when the capability exists but no PIN is set yet, and absent when the authenticator doesn't support a PIN at all — `parse_client_pin_configured` (Task 1) treats both the `false` and absent cases identically (`false`, i.e. no warning), which is correct: only a device that currently *has* a PIN set should trigger CAP-25's warning. The `pin retries: 8` line is also present in this same output — tempting to reuse for AC #3's retry-count warning, but **do not**: AD-21 is explicit that the reactive wrong-PIN warning is sourced from the failed subprocess call's own stderr (Task 5), not from this proactive `-I` query, and `-I`'s retry count reflects the state *before* the failed attempt, not after — using it here would give a stale or misleading number.

- **Avoid poll-loop spam (a real gap in AD-21's literal wording, not addressed by the architecture text as written).** AD-21 says `Fido2Device` "gains a `client_pin: bool` field, populated via one additional `fido2-token -I` call per enumerated device" without specifying *when*. `wait_for_enough_fido2_devices` calls `list_fido2_devices()` in a loop every 500ms while waiting for the user to plug in a key (line 533-552) — naively populating `client_pin` inside `list_fido2_devices()` itself would mean firing an extra `fido2-token -I` subprocess call per already-connected device on *every* poll tick, which is wasteful and adds latency to a loop whose whole point is being cheap enough to poll frequently. Task 2 resolves this by populating `client_pin` exactly once, on the settled list right before `wait_for_enough_fido2_devices` returns — not inside the loop body, and not inside `list_fido2_devices()` itself (which stays exactly as it is today, still used unmodified by the poll loop).

- **Why `enroll_fido2_key`'s `systemd-cryptenroll` calls run unprivileged while `open`'s/`resize`'s `cryptsetup` calls run under `privileged()`/`sudo`.** Not this story's concern to change, but relevant to Task 5's stdio surgery: `privileged()` (line 55) wraps a `Command` as `sudo <program> ...` — `open`/`resize` need this since they touch device-mapper. `enroll_fido2_key`'s `Command::new("systemd-cryptenroll")` (line 1513/1545) does not, since it operates on the LUKS2 header file directly. Piping stderr only (Task 5) works identically either way — `Command::stderr(Stdio::piped())` composes fine whether or not the command is wrapped under `sudo`, since `sudo` itself just execs the target program with the same fd table.

- **Recurring review-pattern watchlist from Epic 4/5/6 retros — apply proactively:**
  - New pure parsing/logic functions shipping without a direct unit test — `parse_client_pin_configured` (Task 1) and any stderr-line-matching logic (Task 5) are exactly this shape and reachable directly from `#[cfg(test)]` with no real device needed; both must get direct unit tests against captured real output/text, not deferred to the hardware suite.
  - Self-reported completion-note claims not matching actual output — verify Task 5's captured stderr text and Task 7's test-count claims against real command/`cargo test` output, not memory.

### Project Structure Notes

- Files touched (production): `src/adapters/exec/mod.rs` only — `Fido2Device` gains `client_pin: bool`; new `fido2_token_has_pin`/`parse_client_pin_configured` functions; `wait_for_enough_fido2_devices` enriches its final return value; `resolve_device_selection`'s `Explicit` branch gains a targeted lookup; `resolve_interactive_selection`/`resolve_explicit_selection` (or a shared point in `enroll_fido2_key`) print the device-specific warning; `open` prints the blanket warning; `open`'s/`enroll_fido2_key`'s (and, per Task 5's scope decision, possibly `resize`'s) blocking subprocess calls change from `.status()` to a piped-stderr-plus-background-reader-thread shape.
- No new files under `src/domain/`, `src/ports/`, or `src/cli/`. No changes to `tests/unit/fakes.rs` (no new fake fields/methods — confirms this story's narrower-than-usual blast radius).
- Files touched (tests): new unit tests live in `src/adapters/exec/mod.rs`'s own existing `#[cfg(test)]` module (same module `fido2_verification_args_true_disables_client_pin`/`fido2_verification_args_false_leaves_client_pin_at_its_default` already live in, ~line 2681 onward) — no new `tests/unit/*.rs` file is needed since none of this story's logic is reachable through `domain`'s fake-port test harness.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 6.6: Proactive FIDO2 PIN-Status Guidance, lines 898-920] — acceptance criteria origin, verbatim.
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 6: Volume Resilience, Filesystem Choice & Everyday Polish, lines 766-768] — epic-level framing; explicitly names "a FIDO2 device with a PIN configured warns the user before the touch prompt, not after a confusing failure" as this story's contribution, and confirms no new port/architectural layer for any Epic 6 story.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-21 — Proactive FIDO2 PIN-status detection, lines 182-186] — the complete mechanism spec: `client_pin: bool` on the existing `Fido2Device` struct, populated via `fido2-token -I`, no new port method; device-specific warning from the enroll resolvers vs. blanket warning from `open`'s presence-wait; wrong-PIN-retry warning sourced from stderr, never stdin/stdout.
- [Source: architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-3 — Secret material never enters the process, Epic 6 amendment, lines 58] — stdin/stdout stay strictly inherited for the actual secret exchange; stderr on that same call may now be piped/parsed for non-secret diagnostic text only; concurrent (not post-exit) stderr reading is required to avoid a pipe-buffer deadlock on a long touch/PIN-blocking call.
- [Source: src/adapters/exec/mod.rs, lines 485-736] — full current `Fido2Device`/enumeration/selection-resolver implementation, read in full during story creation; every insertion point above is derived directly from this code, not inferred.
- [Source: src/adapters/exec/mod.rs, lines 1222-1258 (`open`), 1260-1290 (`resize`), 1444-1600ish (`enroll_fido2_key`)] — the three touch/PIN-blocking subprocess call sites, read in full during story creation.
- [Source: src/adapters/exec/mod.rs, lines 292-330 (`run_piping_stdin`)] — the existing concurrent-thread pattern for a pipe that could otherwise deadlock; the direct precedent Task 5's stderr-reader thread follows.
- [Source: src/adapters/exec/mod.rs, lines 55-59 (`privileged`)] — confirms `open`/`resize` run under `sudo` while `enroll_fido2_key`'s `systemd-cryptenroll` call does not; both compose fine with Task 5's stdio change.
- Live verification (2026-08-10, this dev environment): `fido2-token -L` → `/dev/hidraw5: vendor=0x349e, product=0x0204 (TOKEN2 FIDO2 Security Key(0204))`; `fido2-token -I /dev/hidraw5` → full output captured in Dev Notes, confirming the `options:` line's `clientPin`/`no`-prefix convention and that `-I` alone never prompts.
- Web search (2026-08-10, inconclusive on the exact point that matters): confirms real systemd FIDO2-unlock prompt strings exist (e.g. "Please enter security token PIN", "Please enter LUKS2 token PIN" — systemd/systemd#35393, #26034) and that a PIN-retry-counter display is a known open concern in the systemd project itself, but does **not** confirm the literal stderr text emitted on a wrong-PIN retry attempt — Task 5's spike against real hardware in this environment is required, not optional, before writing the detection logic.
- [Source: _bmad-output/implementation-artifacts/6-5-concurrent-invocation-guard.md] — previous story in this epic; Task 12's real-non-fake-regression-test pattern (no hardware needed to test a real OS-level concurrency property) is the direct precedent for this story's Task 6; Completion Notes' verified baseline (258 total: 17 lib + 241 tests/unit) is this story's starting point.
- [Source: _bmad-output/implementation-artifacts/sprint-status.yaml] — confirms this is the sixth story of Epic 6 (epic already `in-progress` since Story 6.1); no epic-6 action item currently references PIN status.

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

### File List
