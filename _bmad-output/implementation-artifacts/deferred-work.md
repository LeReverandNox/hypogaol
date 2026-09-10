# Deferred Work

## Deferred from: code review of 7-2-mount-point-creation-resilient-to-pre-existing-ownership (2026-09-10)

- The collision-retry loop's 3-attempt exhaustion arm is rewritten by this story (now driven by an unprivileged re-stat instead of `io::ErrorKind`) but still has zero test coverage before or after — only a single collision is ever exercised, never a full exhaustion. [src/adapters/exec/mod.rs:236-247]
- A narrow TOCTOU: if the leaf candidate is removed by an unrelated process between a failed privileged `mkdir` and the unprivileged `symlink_metadata` re-stat, a real collision can be misreported as a genuine, non-recoverable failure using the original mkdir's stderr text — impractical to fix without reintroducing the locale-dependent stderr parsing this story deliberately avoided. [src/adapters/exec/mod.rs:236-247]
- The privileged `chown identity:path` shape is now duplicated three times in this file (base-dir bootstrap, this story's new leaf chown, post-mount chown) with three independently-maintained error strings — worth extracting into a shared helper, but touches pre-existing call sites beyond this story's declared low-risk, no-scope-creep intent. [src/adapters/exec/mod.rs]

## Deferred from: code review of 7-1-presence-only-enrollment-up-only-mode (2026-09-10)

- `codecov.yml` addition contradicts the story's own "No new files" note and weakens patch-coverage enforcement by excluding `src/adapters/exec/mod.rs` and `src/cli/main.rs` — deferred: needed as a CI workaround, this being the first real development since codecov's introduction; already tracked as an open action item (owner: Winston). [codecov.yml]
- `print_enroll_pin_warning`'s UV-branch message doesn't name the ignored `--client-pin` flag when `--user-verification`+`--client-pin=true` are combined — pre-existing warning design (gated on device's own PIN state), cosmetic clarity gap only. [src/adapters/exec/mod.rs:1176-1182]
- Task 4's hardware spike only exercised the `create file` call path; the standalone `enroll --client-pin=false` branch (`--unlock-fido2-device`) was never itself run against real hardware — shares the same `fido2_verification_args` call, low risk. [src/adapters/exec/mod.rs enroll_fido2_key]
- `client_pin == Some(true)` has no real-hardware verification, only a unit test on the arg-string literal — self-admitted not required by any AC. [src/adapters/exec/mod.rs:3396-3403]
- Bare `-p`/`--client-pin` (no value) is behaviorally a no-op vs omitting the flag entirely, undocumented in `--help` — intentional pre-seeding for Story 7.2 per Dev Notes. [src/cli/main.rs:88-93]
- No hardware-in-the-loop automated regression test for `client_pin`'s `Some(true)`/`Some(false)` behavior; only a manual one-off hardware run recorded as prose — pre-existing project limitation (no CI-hooked hardware loop). [tests/hardware/main.rs]
- `client_pin: Option<bool>` threaded as another bare positional parameter through 6+ already-`too_many_arguments`-flagged functions with no structural mitigation — pre-existing pattern tracked via the epic-6 clippy-noise action item. [src/domain/workflows/create.rs, src/adapters/exec/mod.rs]

## Deferred from: code review of 6-6-proactive-fido2-pin-status-guidance (2026-08-11)

- `open_pty_pair`'s fixed 128-byte `ptsname_r` buffer has no `ERANGE` handling — pre-existing pattern (fixed-size stack buffers elsewhere in this file), effectively unreachable on Linux. [src/adapters/exec/mod.rs:472-484]
- The pty master fd `open_pty_pair` returns is never marked close-on-exec, leaking into every subprocess `run_with_stderr_watch` spawns afterward — low real-world impact for this short-lived, single-operation-per-invocation CLI. [src/adapters/exec/mod.rs:443-491]
- No test exercises `run_with_stderr_watch`'s full reader-thread accumulation loop end-to-end (e.g. a wrong-PIN marker split across a 4096-byte `read()` chunk boundary) — `Fido2StderrSignal::classify`/`wrong_pin_attempt_text` are each unit-tested in isolation but not through this integration point. [src/adapters/exec/mod.rs:604-663]
- `Fido2StderrSignal::classify` matches locale/vendor-specific English literal stderr text with no fallback — consistent with this file's pre-existing, project-wide convention of assuming English-locale subprocess output. [src/adapters/exec/mod.rs:364-373]
- `run_with_stderr_watch`'s `captured`/`scan_buffer` grow unbounded if the child ever emits stderr with no `\n`/`\r` boundary — low likelihood for `cryptsetup`/`systemd-cryptenroll`'s actual output shape. [src/adapters/exec/mod.rs:604-627]

## Deferred from: code review of 6-4-xfs-and-btrfs-filesystem-support (2026-08-09)

- `with_transient_mount` performs two independent, unguarded mount/unmount cycles per resize (`filesystem_size` then `growfs`) with no protection against filesystem state changing between them — Story 6.5 "concurrent-invocation-guard" is the planned fix for this exact class of issue. [src/adapters/exec/mod.rs]
- No real-hardware XFS resize scenario was exercised near the new 300MB create-time floor boundary — both hardware XFS scenarios use volumes hundreds of MiB above it. [tests/hardware/main.rs]
- `MIN_BTRFS_RESIZE_PAYLOAD_BYTES`'s margin above the real kernel floor (260 MiB vs. 256 MiB, ~1.5%) is thin and mostly earmarked for block-flooring elsewhere, not independently verified right at the boundary on real hardware. [src/domain/workflows/resize.rs:33]

## Deferred from: code review of 5-4-update-live-planning-automation-pointers (2026-08-03)

- Epic 5 completion (this is its last story) has no tracked action item for a retrospective or an `epic-5` status flip in sprint-status.yaml — out of this story's scope; a sprint-status/retro housekeeping step. [_bmad-output/implementation-artifacts/sprint-status.yaml]
- SPEC.md/ARCHITECTURE-SPINE.md addenda aren't themselves marked append-only, so a future edit could silently reword them instead of appending a new dated addendum. [_bmad-output/specs/spec-tomb-fido2/SPEC.md:105, _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md:121]
- Task 3's `tomllib.load` check only proves the new `[core]` table is syntactically valid TOML, not that it actually overrides the installer-managed `project_name` at runtime for any BMAD tool reading it. [_bmad/custom/config.toml:8-9]
- Both addenda cite `sprint-change-proposal-2026-08-02.md` as source of record with no version/commit pin, so a later edit to that proposal could make both citations silently stale. [_bmad-output/specs/spec-tomb-fido2/SPEC.md:105, _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md:121]
- `bmad-code-review.toml`'s `on_complete` step 1 (`gh pr list ... --state open`) has no explicit zero-match handling before step 2 uses `<number>` — pre-existing, the rename only touched the repo-path literal. [_bmad/custom/bmad-code-review.toml:9-10]
- `bmad-dev-story.toml`'s Issue lookup (`--state open`) has no zero-match handling before instructing "move it to In Progress". [_bmad/custom/bmad-dev-story.toml:4]
- `bmad-create-story.toml`'s Issue lookup uses `--state all`, so it can match and silently reuse an already-closed Issue with no reopen step. [_bmad/custom/bmad-create-story.toml:9]

## Deferred from: code review of 4-6-emergency-slam (2026-07-28)

- Second `fs.mount_point_of` call after a failed `umount` has no "not currently mounted" tolerance, unlike sibling calls — narrow TOCTOU window. [src/domain/workflows/slam.rs:76]
- PID reuse between `processes_using` and `signal_process` could target an unrelated process that recycled the PID. [src/domain/workflows/slam.rs:87-89]
- A `processes_using` `Err` mid-escalation (`?`) aborts remaining rounds rather than being treated as non-fatal — matches Task 2's "spawn failure only" design; a spawn failure would recur identically every round. [src/domain/workflows/slam.rs:80]
- Sequential batch processing gives zero incremental progress feedback, undercutting the "immediate" framing when multiple busy tombs each take up to 3s — inherited from `close_all`'s pre-existing batch shape. [src/domain/workflows/slam.rs:32-49]
- `run_slam` and `slam::run` each run their own `preflight::check` — pre-existing pattern already present in `run_close_all`. [src/cli/main.rs:611-621]
- Hung hook script blocks slam indefinitely — no timeout anywhere in the codebase; explicitly acknowledged as a pre-existing risk category in this story's own Dev Notes. [src/domain/workflows/slam.rs:62-66]
- `signal_process` failures are silently swallowed with no diagnostic surfaced to the user — matches this story's own "best-effort, ignore" Dev Notes resolution; surfacing would be an enhancement, not a fix. [src/domain/workflows/slam.rs:87-89]

## Deferred from: code review of 4-3-enroll-a-fido2-key-with-user-verification (2026-07-27)

- `run_create`/`create::run` now carry two untyped `bool` parameters (`user_verification`, `announce`) with no compiler-enforced distinction — pre-existing pattern (`announce: bool` predates this diff), not introduced by Story 4.3. [src/cli/main.rs:292]
- Task 9's "no new `AdapterFailure` string/`DomainError` variant" check is manual/inspection-only, with no automated grep/lint enforcing it — already tracked as an open, in-progress retro action item. [sprint-status.yaml#action_items, epic 2]
- `user_verification=false` relies on `systemd-cryptenroll`'s own undeclared clientPin default rather than pinning it explicitly — pre-existing Epic 2 behavior, unchanged by this story. [src/adapters/exec/mod.rs:672]

## Deferred from: code review of 1-1-project-scaffolding-nix-devshell (2026-07-22)

- `ARCHITECTURE-SPINE.md`'s Stack table has a stale combined `serde`+`serde_json` version figure (`1.0.229`) that caused Story 1.1's `serde_json` pin deviation and will mislead future stories reading the table. [ARCHITECTURE-SPINE.md#Stack]
- Dev Agent Record's claim that `nix develop -c cargo build` (not a bare system `cargo build`) was used for Story 1.1's Task 1 verification isn't independently verifiable from repo state, since a matching system Rust toolchain happens to be present. [1-1-project-scaffolding-nix-devshell.md:82]
- `Makefile` targets (`build`/`test`/`test-hardware`) don't pass `--locked` to cargo, so a `Cargo.lock`/`Cargo.toml` drift would silently re-resolve rather than fail fast. [Makefile:3-10]
- No `LICENSE` file despite the README referencing GitHub Releases distribution. [repo root]

## Deferred from: code review of 1-2-ci-runs-the-mocked-unit-test-suite (2026-07-22)

- Nix binary version isn't pinned by `cachix/install-nix-action@v31`, only nixpkgs is pinned via `flake.lock` — a future Nix release could silently change CI behavior without a diff to this repo. [.github/workflows/ci.yml:10]
- No branch protection rule requires the CI check to pass before merge, so a red `make test` run doesn't yet block merges — the AC is satisfied at the check-reporting level but not enforced at merge time. [.github/workflows/ci.yml]

## Deferred from: code review of 1-3-release-automation (2026-07-23)

- `release.yml`'s `pull_request:` trigger (no path filter) runs the `plan` job — including a curl-install of cargo-dist — on every PR, even docs-only ones. Standard `cargo dist generate-ci` output; Dev Notes forbid hand-authoring this file. [.github/workflows/release.yml:69-71]
- `release.yml` has no `concurrency:` guard, unlike `release-please.yml` — rapid tag pushes could race the `plan`/`host` jobs. Same generated-file constraint applies. [.github/workflows/release.yml]
- `release.yml` pins `ubuntu-22.04` runners (vs. `ubuntu-latest`) and has no dependency on Story 1.2's `ci.yml` passing before a tagged commit's binaries get built/published — the latter relies entirely on GitHub branch-protection settings outside this diff's scope. [.github/workflows/release.yml]
- cargo-dist is installed via `curl ... | sh` with no checksum/signature verification — standard cargo-dist-generated install step, same generated-file constraint. [.github/workflows/release.yml:94-95]
- `Cargo.toml`'s `[package]` section still has no `license`/`description` — relevant once this repo goes public, but out of this story's scope and would touch `[package]` further. [Cargo.toml]
- No live dry run of the full pipeline was completed (release-please `--dry-run` blocked by a CLI auth quirk per Completion Notes, not independently re-verified here); watch the first real merge-to-main closely.
- A pushed tag whose version doesn't match `Cargo.toml`'s version (no automated linkage enforced) surfaces as an opaque `dist plan` failure rather than a clear message.
- A manually pushed tag not created by release-please would fail outright at `gh release upload`/`edit` since `create-release = false` assumes the release already exists — inherent to the documented two-workflow chain; not a supported flow.
- release-please has no `bootstrap-sha`, so the very first release's changelog will include the entire commit history including internal story-process commits — acceptable for a project's first-ever release.

## Deferred from: code review of 1-5-create-a-file-backed-tomb (2026-07-23)

- `enroll_fido2_key` hardcodes `--fido2-device=auto`, with no way to target a specific device when multiple FIDO2 authenticators are attached — a known v1 scope limitation, not required by this story's ACs. [src/adapters/exec/mod.rs:434]

## Deferred from: code review of 1-6-create-a-device-backed-tomb (2026-07-23)

- `has_luks2_header`/`device_capacity`/`luksFormat` shell out unprivileged against real block devices that are typically `root:disk` mode `660`, making `create device` effectively require the whole CLI run under `sudo` — undocumented in `--help`/output. [src/adapters/exec/mod.rs:395-732]
- `has_luks2_header` only detects an existing LUKS2 header, not other filesystem/partition signatures (ext4, xfs, LVM PV, etc.) a device might already carry — matches AC #4's literal scope exactly; broader signature detection is a candidate for a future story. [src/adapters/exec/mod.rs:395-405]

## Deferred from: code review of 1-7-unlock-and-mount-a-tomb (2026-07-24)

- Resize guard (`raw_size > size`) fixes only the exact observed failure mode ("requested size equals full raw capacity"), not the general "not enough headroom for the LUKS2 header" constraint — a size just a few KB under full capacity could plausibly still fail with the same error. [src/adapters/exec/mod.rs:540]
- No automated (fake-backed unit) regression coverage for the `actual_raw_size`/resize-guard fix — it was only caught and confirmed via a manual hardware run, and a future refactor could silently reintroduce it with CI staying green. [tests/unit]
- Mount-directory name (`tomb-fido2-<mapper.name>-<suffix>`) embeds the same deterministic mapping-name hash used for the dm-crypt mapping, a minor local fingerprinting/correlation side channel for any user who can list a world-traversable `/tmp`. [src/adapters/exec/mod.rs:888]
- No plain-language wrapping of unlock failure paths (wrong/missing key, PIN mismatch, path isn't a LUKS2 header at all) — only cryptsetup's own raw stderr plus a generic `AdapterFailure` surfaces today; explicitly Story 1.8's scope per this story's own Dev Notes.
- No forward story currently closes the mount-exposure window (world-readable mount point, see the decision-needed finding on this story) until Story 3.1's `close` ships — process observation, not itself a code defect.

## Deferred from: code review of 1-9-mount-ux-and-ownership-hardening (2026-07-24)

- TOCTOU race on `base.exists()` between concurrent invocations, plus the pre-existing (Story 1.7-established) convention of swallowing compensating-cleanup failures (`umount`/`remove_dir` after a chown/chmod failure) so a partial failure can silently leave a root-owned filesystem mounted — pre-existing pattern reused per this story's own Dev Notes instruction, not a new deviation. [src/adapters/exec/mod.rs:981-1097]
- Once `close` (Story 3.1) exists, if it doesn't `rmdir` the plain-basename mount-point directory after unmounting, re-unlocking the same tomb will permanently fall back to a suffixed name — explicitly out of this story's scope per its own Dev Notes; flag for Story 3.1's scoping. [src/adapters/exec/mod.rs:157-184]

## Deferred from: code review of 2-1-enroll-an-additional-fido2-key (2026-07-25)

- `wait_for_enough_fido2_devices` blocks forever with no timeout, and can't distinguish "no device plugged in yet" from "a device is present but not enumerating due to a permissions/udev problem" — both print an identical, endlessly-repeating wait message with no escalation path short of killing the process. Deferred: blocking-forever is the explicitly-decided replacement for the old "fails immediately" behavior per the already-resolved architect consultation; the permission-vs-absence ambiguity is the same known device-permission gap class already flagged in Story 1.6 — not new, not blocking. [src/adapters/exec/mod.rs:442-461]

## Deferred from: code review of 2-2-revoke-a-fido2-key-guarded-against-last-keyslot-lockout (2026-07-26)

- Second `list_fido2_keyslots` read (inside `remove_keyslot_guarded`) re-checks the keyslot number is still live but never re-verifies the label still matches it — pre-existing multi-read pattern in a single-user local CLI, out of this story's scope. [src/domain/workflows/revoke.rs, src/domain/keyslot_guard.rs]
- Two live keyslots sharing the same `key_label` resolve silently to the first match (`find()`) — requires bypassing enroll's own uniqueness enforcement; pre-existing invariant gap. [src/domain/workflows/revoke.rs]
- Two `systemd-fido2` tokens referencing the same live keyslot number produce a non-deterministic reported label (HashMap iteration order) — pre-existing dedup logic, corrupted-state-only. [src/adapters/exec/mod.rs]
- `parse_label` doesn't trim whitespace; a trailing-space label becomes practically unrevocable since lookup is exact-match — pre-existing bug shared with `enroll`, fix would need to touch out-of-scope code. [src/cli/main.rs]
- Unsanitized `--label` value interpolated into terminal/error output (control-character risk) — pre-existing, low impact for a single-user local CLI. [src/cli/main.rs, src/cli/ux.rs]
- `run_revoke` calls `preflight::check` before `revoke::run` also calls it internally — double `cryptsetup luksDump` subprocess spawn per invocation — pre-existing pattern shared with `enroll`/`unlock`. [src/cli/main.rs, src/domain/workflows/revoke.rs]

## Deferred from: code review of 3-1-close-an-unlocked-tomb (2026-07-26)

- TOCTOU race between the `findmnt` check and the `umount` call — pre-existing risk pattern shared with `mount`'s own non-atomic multi-step subprocess sequence; low probability, no clean fix without a different unmount mechanism. [src/adapters/exec/mod.rs, `umount`]
- Silent `rmdir` failure after a successful `umount` is swallowed with no user-facing warning — mirrors the exact same `let _ = std::fs::remove_dir(...)` pattern `mount`'s own cleanup-on-error paths already use; established precedent, not a new inconsistency. [src/adapters/exec/mod.rs, `umount`]
- No unit test exercises the real `ExecAdapter::umount` subprocess wiring (`findmnt`/`umount`/`rmdir`) — covered only by manual-only `#[ignore]`d hardware tests, consistent with AD-7's established testing standard for every other adapter method in this codebase.
- Ambiguous CLI contract for `close`'s `path` argument — a user may reach for the mounted directory or `/dev/mapper/vault-*` node rather than the original backing path, but this mirrors the exact same convention already used by `unlock`/`enroll`/`revoke`, not a new ambiguity.
- AD-8's spec text says `FilesystemBackend`'s `mount`/`umount` take a `Filesystem` enum parameter, but neither does — pre-existing gap `mount` already had, harmless under v1's ext4-only scope. [src/ports/filesystem_backend.rs]
- `close` can't run at all if the backing path was deleted while the tomb is still open — `mapping_name`'s `std::fs::canonicalize` requiring the path to exist is shared by every workflow using this helper (`unlock`/`revoke`/`resize`), not introduced by this story. [src/domain/mapping_name.rs]

## Deferred from: code review of 3-2-grow-an-existing-tombs-capacity (2026-07-26)

- Error-path close failure is silently swallowed — `let _ = luks.close(&mapper);` on resize's error path matches the exact pattern already established in `create.rs:132` and `unlock.rs:30`, not introduced by this story. [src/domain/workflows/resize.rs:72]
- `filesystem_size`'s `dumpe2fs` parsing assumes an English locale — nothing forces `LC_ALL=C` on adapter subprocesses anywhere in this codebase (e.g. the `cryptsetup --help` plugin-path parsing has the same property), not specific to this story. [src/adapters/exec/mod.rs:1528-1546]
- No unit test exercises the real `ExecAdapter::is_block_device` against an actual block device — consistent with AD-7's established testing standard for every other hardware-dependent adapter method in this codebase; only reachable via manual `#[ignore]`d hardware tests. [src/adapters/exec/mod.rs]
- Device-backed headroom hardware scenario doesn't assert the filesystem's own size before/after — test-coverage improvement, not a functional defect; manual-only hardware test, not run in CI. [tests/hardware/main.rs]

## Deferred from: code review of story-3.3 (2026-07-27)

- `luks.close()`'s failure on the mount-failure rollback path is silently discarded (`let _ = luks.close(&mapper);`), so a `mount` failure followed by a `close` failure leaves a dangling mapper with no signal to the caller — same long-standing pattern already noted for `create.rs:132`/`unlock.rs:30` in the 3-2 review and `resize.rs:72`; now also covers the read-only path this story adds, still not introduced by this story. [src/domain/workflows/unlock.rs:31]

## Deferred from: code review of 4-4-per-tomb-bind-hooks-exec-hooks-automation (2026-07-28)

- TOCTOU gap between the exec-hooks guardrail check and execution — `hook_file_metadata` stats the file, then `run_hook` execs it by path with no fd-pinning in between; a local write-capable actor could swap the script in that window. A real fix needs fd-based exec (open once, fstat the fd, exec via the fd — e.g. `fexecve` via unsafe libc). Deferred: matches this codebase's existing local-single-user trust model — nothing else here defends against a co-resident attacker with write access either. [src/domain/workflows/unlock.rs:70,85; src/domain/workflows/close.rs:79,87; src/adapters/exec/mod.rs:1853-1895]
- Close-time bind-hooks teardown can leave a stale bind mount dangling under `$HOME` if `bind-hooks` is edited between `unlock`/`close` (removing an applied entry) or `invoking_home_dir` fails during teardown — pre-existing architectural constraint (AD-2 forbids a persisted mount registry, so `close` has no memory of what `unlock` actually applied). [src/domain/workflows/close.rs:115-137]
- Nothing verifies the invoking `tomb-fido2` process itself is unprivileged before running `exec-hooks` — if the whole CLI is launched under `sudo`, the hook script runs as root despite the guardrail's "never elevated" framing — pre-existing whole-tool privilege-model gap, not specific to this story's diff. [src/adapters/exec/mod.rs:1886-1895]

## Deferred from: code review of 4-5-close-every-open-tomb-close-all (2026-07-28)

- `list_open_mappings`'s `dmsetup ls` → `cryptsetup status` TOCTOU window hard-fails the *entire* discovery call if one mapping closes mid-scan, rather than skipping just that entry — a deliberate, documented Dev Notes decision (never intended as a per-entry-skip); the ARCHITECTURE-SPINE.md Deferred-section citation doesn't literally name this specific race, but the same "low-likelihood for a single-user cold-storage tool" reasoning it states for other concurrent-invocation races applies equally here. [src/adapters/exec/mod.rs:1199]
- No `LC_ALL=C` (or equivalent) is pinned on the `dmsetup ls`/`cryptsetup status` subprocess calls whose textual output is parsed for field values for the first time in this codebase — a non-English locale could in principle break the `loop:`/`device:` field-name match. Same underlying gap as the 3-2 review's `dumpe2fs`-locale item (nothing pins `LC_ALL=C` on adapter subprocesses anywhere in this codebase); deferred for the same reason. [src/adapters/exec/mod.rs:1164, 1191]

## Deferred from: code review of 5-1-product-identity-rename (2026-08-02)

- Internal test/temp-file fixtures still embed the old product name — not user-facing, out of this story's narrow scope (Cargo/config/flake/README/CHANGELOG/CI); natural pickup for Story 5.2 since it already touches these same files for the `tomb`→`volume` domain-noun rename. [src/adapters/exec/mod.rs:251,2251; tests/hardware/main.rs; tests/unit/*.rs]

## Deferred from: code review of 6-1-crash-safe-create-resume (2026-08-08)

- `luksFormat` and the marker-token write are non-atomic, sequential subprocess calls — inherent to the two-subprocess-call mechanism (no atomic cryptsetup primitive combines format + token-import); a crash in that narrow window loses the marker permanently, but the fallback is "refuses to resume, same as a genuine pre-existing volume" — the pre-CAP-23 baseline behavior, not new data loss. [src/adapters/exec/mod.rs:894-919]

## Deferred from: manual hardware verification of 6-1-crash-safe-create-resume (2026-08-08)

- **Stale-mapping gap found during manual crash-simulation — fixed directly in this story, not deferred.** See `6-1-crash-safe-create-resume.md`'s Review Findings/Completion Notes for the fix (`LuksBackend::close_stale_mapping`, called unconditionally at the top of `bootstrap_and_provision`).
- `create device` requires the whole CLI invocation to run as root (already tracked below, 1-6 review) — confirmed again live, plus a concrete UX direction from this session: elevate privileges automatically when needed (e.g. re-exec via `sudo`/`pkexec`) rather than requiring the operator to prefix the whole command with `sudo` themselves. Not fixed in this story — genuinely a different, broader concern (affects every privileged operation, not just `create`).

## Deferred from: code review of 5-3-top-level-branding-copy (2026-08-03)

- Task 3's tone-boundary grep omits `src/cli/main.rs` (where the real runtime `println!`/`eprintln!` strings live) and its flavor-vocabulary pattern doesn't cover all of brand-identity.md's mascot/palette terms (e.g. "gothic", "warden", "moss", "amber", "keyhole", "tracery", "blackletter") — re-running the check with `main.rs` and the expanded vocabulary included still returns zero matches today, so no live violation; flag for future stories that extend Epic 5's tone-boundary checks. [_bmad-output/implementation-artifacts/5-3-top-level-branding-copy.md, Task 3]
- No regression test guards the CLI `--help` banner's new description line (AC #2, dev-discretion addition) — a future `Cargo.toml` edit (e.g. someone writing a "real" crate description for publishing) could silently drop the tagline with nothing to catch it. Optional hardening beyond this copy-only story's stated acceptance bar (clean build + test). [Cargo.toml:8]

## Deferred from: code review of 6-2-custom-key-label-at-create (2026-08-09)

- `parse_label` accepts values that are unsafe as identifiers — untrimmed leading/trailing whitespace, invisible/zero-width-only strings pass the not-empty check, no length cap, and control characters (e.g. embedded newline) are not rejected. Pre-existing, shared by `enroll`/`revoke`, unchanged by this diff, and explicitly out of scope per this story's own Dev Notes ("reuse `parse_label` unchanged — do not write a second validator"). Consequences if exploited: whitespace padding can defeat the exact-match duplicate-label check (`src/adapters/exec/mod.rs:1413-1420`); an embedded newline can inject a fake extra line into `info`'s one-bullet-per-line output (`src/cli/main.rs:762`); an oversized label fails late inside `cryptsetup token import` instead of at CLI parse time. [src/cli/main.rs:213-219]

## Deferred from: code review of 6-5-concurrent-invocation-guard (2026-08-10)

- Non-UTF-8 path bytes are lossy-converted (`to_string_lossy()`) before hashing into the lock name, so two distinct paths could theoretically collapse to the same lock name — identical pattern already shipped, unchanged, in `mapping_name::mapping_name`; not introduced by this story. [src/adapters/exec/mod.rs:2572-2574, src/domain/mapping_name.rs:45]
- No automated regression test exists specific to the `systemd-cryptenroll` deadlock (only `cryptsetup`/`has_marker_token` is exercised in `tests/unit/lock_target.rs`) — the abstract-socket mechanism structurally can't touch the target file at all anymore, so the existing test already proves the general "our lock never conflicts with any subprocess call against the target" property; a dedicated test would be corroborating, not load-bearing, and needs real `systemd-cryptenroll`/hardware to be meaningful. [tests/unit/lock_target.rs]
