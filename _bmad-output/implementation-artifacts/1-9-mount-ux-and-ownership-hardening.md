---
baseline_commit: 09b9d1d
---

# Story 1.9: Mount UX & Ownership Hardening

Status: in-progress

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a user unlocking my tomb,
I want the unlocked filesystem owned by me (not root), mounted somewhere visible in my file manager, and referenced by a plain positional path argument,
so that the tool actually gives *me* access to my own decrypted data and feels native to use, matching the `dyne/tomb` experience this project is modeled on.

> **Origin note:** this story is not in `epics.md` — it was raised in the Epic 1 retrospective (`_bmad-output/implementation-artifacts/epic-1-retro-2026-07-24.md`) after the user tested the compiled binary post-epic and found a real bug plus two UX gaps. Agreed as a hardening patch to complete before Epic 2 starts. The dm-crypt mapping name (`vault-<16-hex-hash>`) is explicitly **not** changing — the user confirmed keeping it as-is; only the mount point and CLI ergonomics change.

## Acceptance Criteria

1. **Given** a tomb is unlocked, **when** the mount completes, **then** the mount point is owned by the invoking (real, non-root) user — not root — so the user can actually read/write their own files, while still being inaccessible to any other local user. [Source: epic-1-retro-2026-07-24.md — Post-Epic Findings #1]
2. **Given** a tomb is unlocked, **when** the mount completes, **then** it lands at `/run/media/<username>/<tomb-name>` (not `/tmp`), where `<tomb-name>` is derived from the source path's basename with its extension stripped (e.g. `vault.img` → `vault`; `/dev/sdb1` → `sdb1`), visible to standard desktop file managers. **And** if that exact directory name is already in use, the tool falls back to a short disambiguating suffix rather than failing outright. [Source: epic-1-retro-2026-07-24.md — Post-Epic Findings #2; matches `dyne/tomb`'s `/run/media/<username>/<tomb_name>` convention per the user's own reference]
3. **Given** the CLI, **when** the user runs `create file`, `create device`, or `unlock`, **then** the target path is a positional argument (e.g. `tomb-fido2 unlock /path/to/tomb`), not a `--path` flag. [Source: epic-1-retro-2026-07-24.md — Post-Epic Findings #3]
4. **Given** these changes, **when** any of them ships, **then** the dm-crypt mapping name (`vault-<16-hex-hash>`, `src/domain/mapping_name.rs`) is completely unchanged — no truncation, no basename influence, no format change. [Source: epic-1-retro-2026-07-24.md — user explicitly confirmed keeping it as-is]

## Tasks / Subtasks

- [x] Task 1: Add an "invoking identity" lookup to `adapters::exec` — needed by both Task 2 (chown) and Task 3 (mount base path) (AC: #1, #2)
  - [x] Add a small internal helper, e.g. `fn invoking_identity() -> Result<(String uid, String gid, String username), DomainError>` (or three separate small helpers — dev's call), shelling out to `id -u`, `id -g`, `id -un` — three quick, unprivileged, non-secret queries, the same class as the existing `blockdev --getsize64`/`cryptsetup isLuks` pure-query calls (Story 1.6's Dev Notes explicitly draws this "no secret material, no mutation" distinction; reuse it here)
  - [x] **Do not add a new Cargo dependency** (`libc`/`nix`) for this and **do not use `unsafe`** — this codebase has zero `unsafe` blocks and gets every OS-level fact (sizes, header state, PATH lookups) via subprocess calls parsed as plain strings; shelling out to `id` matches that established pattern exactly. `tomb_fido2` itself always runs unprivileged as the invoking user — only specific calls escalate via the existing `privileged()` helper — so `id -u`/`id -g`/`id -un` invoked directly (no `privileged()` wrapper) already report the real invoking identity, not root's
  - [x] Add `"id"` to `FilesystemBackend::check_prerequisites`'s required-binaries list (`src/adapters/exec/mod.rs:801`, currently `["mkfs.ext4", "resize2fs", "blockdev", "mount"]`) — AD-4's preflight gate must cover every hard dependency this story introduces, same discipline Story 1.5 used for `blockdev` and Story 1.7 used for `mount`
- [ ] Task 2: Fix mount-point ownership — the actual bug (AC: #1)
  - [ ] In `FilesystemBackend::mount`'s real implementation (`src/adapters/exec/mod.rs:896-955`), after the `mount` call succeeds and **before** the existing `chmod 0700` call, add `privileged("chown").arg(format!("{uid}:{gid}", ...)).arg(&mountpoint)` (both need root, since the mounted directory's underlying inode — created by a privileged `mkfs.ext4` at `create` time — is currently root-owned; a bare unprivileged `chown` would fail with EPERM)
  - [ ] Keep the existing `chmod 0700` call afterward, unchanged — the combination (owned by the invoking user, `0700`) is what actually achieves "only the invoking user can access this," which the current `chmod`-only fix (root-owned, `0700`) does not
  - [ ] On `chown` failure, follow the exact same cleanup pattern the existing `chmod` failure branch already uses (unmount, remove the directory, return `AdapterFailure`) — don't invent a different cleanup shape for the new failure branch
- [ ] Task 3: Relocate the mount point from `/tmp` to `/run/media/<username>/<tomb-name>` (AC: #2)
  - [ ] Derive `tomb_name` from `mapper.source_path.file_stem()` (`MapperHandle.source_path: PathBuf` already exists, `src/domain/types.rs:32` — no signature change needed, `mount(&self, mapper: &MapperHandle)`'s existing parameter already carries what this needs). `file_stem()` strips the extension for a file path (`vault.img` → `vault`) and returns the whole name for an extensionless device path (`/dev/sdb1` → `sdb1`), matching AC #2's examples directly
  - [ ] Base directory: `/run/media/<username>` (from Task 1's identity lookup). If it doesn't exist yet, create it via `privileged("mkdir").args(["-p"]).arg(&base)` then `privileged("chown")` to the invoking uid/gid (mode `0755` — conventional for a per-user base directory under `/run/media`, matching udisks2's own convention) — do this once per mount call, cheap to check-and-skip if it already exists (e.g. `base.exists()` first, avoiding an unnecessary `sudo` prompt on every unlock)
  - [ ] Mount point: `base.join(tomb_name)`. Replace the current `std::env::temp_dir().join(format!("tomb-fido2-{}-{suffix}", mapper.name))` (`:898`) and its random-suffix generation — the suffix (via `random_hex_suffix()`, `:111`) is no longer needed for the primary path, only as a *fallback* (next bullet)
  - [ ] Collision handling: if `std::fs::create_dir(&mountpoint)` fails with `ErrorKind::AlreadyExists`, retry once with `base.join(format!("{tomb_name}-{suffix}"))` using the existing `random_hex_suffix()` helper (shortened, e.g. 4 hex chars is plenty for a rare collision) — a bounded number of attempts (e.g. 3), returning a clear `AdapterFailure` if all are exhausted. Any other `create_dir` error (permission denied, etc.) still propagates immediately as today, unchanged
  - [ ] This does **not** violate AD-12's "mount point is not stored/remembered" rule — the path is still recomputed fresh from `mapper.source_path` on every call, exactly like today; it's just a different (and now non-random, so collision-checked) formula. `close` (Story 3.1) still discovers the live mountpoint via the kernel's own mount table (`findmnt`), never by reconstructing this path — don't build a shortcut that skips that discovery
- [ ] Task 4: Positional path arguments (AC: #3)
  - [ ] `src/cli/main.rs`: remove `#[arg(long)]` from the `path` field on `CreateMode::File` (`:46-47`), `CreateMode::Device` (`:63-64`), and `Commands::Unlock` (`:33-34`) — in clap's derive API, a field with no `long`/`short` attribute is positional by default, so this is the entire mechanical change (no other attribute needed)
  - [ ] No change to `run_create`/`run_unlock`/`domain::workflows::create::run`/`domain::workflows::unlock::run` — they already take `path`/`target` as plain values; only how clap *parses* it from argv changes
- [ ] Task 5: Update fakes, unit tests, and hardware tests (AC: #1, #2, #3)
  - [ ] `tests/unit/cli.rs`: `unlock_help_lists_path_flag` (`:78-82`) currently asserts the rendered help contains `"--path"` — after Task 4, clap's help output shows a positional placeholder (e.g. `<PATH>`) instead. Rename/update this test to assert the positional form instead (check clap's actual rendered output empirically rather than guessing the exact placeholder text)
  - [ ] `tests/unit/fakes.rs`'s `FakeFilesystemBackend::mount` currently returns a deterministic `/tmp/fake-mount-<mapper.name>` path (Story 1.7) — this is fine to leave as-is for existing tests that only care *that* mount was called, but if any test asserts on the exact path shape, update it to reflect that path derivation is now basename-based, not purely `mapper.name`-based (the fake doesn't need to replicate the real adapter's `/run/media` logic — it's a domain-level fake, and `mount`'s real path-construction logic lives entirely in `adapters::exec`, untested by `domain`-level fakes either way)
  - [ ] No `domain`-level unit test can exercise the real `chown`/`/run/media` logic (it lives entirely in `adapters::exec`, same as every other real-subprocess behavior in this codebase) — this story's correctness proof is the hardware-gated test below, not a fake-backed unit test
  - [ ] Extend `tests/hardware/main.rs`'s existing unlock scenarios (Story 1.7) to additionally assert: the returned mount point is under `/run/media/<username>/`, its directory entry is owned by the invoking user (not root) via `std::fs::metadata`/`MetadataExt::uid()`, and its basename matches the source file's `file_stem()`. Add (or extend) a scenario that mounts two tombs with the same basename (e.g. two disposable files both named the same in different directories) to exercise the collision-suffix fallback
  - [ ] Confirm `cargo test --test unit` / `make test`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --check` all stay green, then run `make test-hardware` by hand (root required, same environment caveats as every prior story's hardware run — see References) before considering this story done

## Dev Notes

- **Scope is `adapters::exec` + `cli` only.** No `domain`/`ports` signature changes are needed anywhere — `FilesystemBackend::mount(&self, mapper: &MapperHandle)`'s existing signature already carries `mapper.source_path`, which is all Task 3 needs. Do not widen the port signature "just in case"; this story doesn't require it, matching the discipline every prior story (esp. 1.6/1.7) followed of not speculatively widening a signature ahead of an actual need.
- **This is the first story to shell out to `id`.** No existing pattern for parsing its output exists yet in this codebase — follow the same conventions `blockdev --getsize64`'s output-parsing already established (`.output()`, check `status.success()`, parse stdout as a plain string/number, wrap any failure as `AdapterFailure`).
- **Why `chown` needs `privileged()` even though reading the uid/gid does not:** `id -u`/`id -g`/`id -un` are pure, non-mutating reads of the *current process's own* identity — no privilege needed, and definitely do not wrap them in `privileged()` (that would ask `sudo` for the invoking user's own already-known identity, an unnecessary and confusing prompt). `chown`/`mkdir -p` on `/run/media/<username>` and the mount point itself, in contrast, mutate root-owned paths and do need it, exactly like the existing `mount`/`chmod` calls in the same function.
- **AD-12 compliance:** the mount point is still derived fresh from `mapper.source_path` on every call — never stored, never a registry. Only the *formula* changes (basename-derived + collision-checked, instead of `mapper.name`-derived + random-suffixed). `close` (Story 3.1, not yet built) will still rediscover the live mountpoint via `findmnt`, unaffected by this story.
- **AD-13 compliance:** `/run/media` and the `id`/`mkdir`/`chown` calls introduce no new product-name string anywhere — nothing here touches placeholder-name isolation.
- **Previous story pattern (1.7, 1.8):** both kept their diffs tightly scoped to exactly what their ACs needed and explicitly called out what they were *not* touching. Follow the same discipline — this story does not touch `create`'s own logic, `enroll`/`revoke`/`close`/`resize` (still `todo!()` stubs, out of scope), or the mapping-name helper (`src/domain/mapping_name.rs` — AC #4 exists specifically to make this explicit and testable).
- **Testing (AD-7):** as with every prior adapter-level behavior in this codebase (privilege escalation, real subprocess calls), the real `chown`/`/run/media` logic cannot be exercised by a `domain`-level fake-backed unit test — it's proven only by the hardware-gated scenario (Task 5), consistent with how Story 1.5/1.6/1.7 each had unit tests for domain orchestration and a separate hardware test for real-world correctness.
- **Known edge case, not required to solve here:** if a user runs the whole command under `sudo` themselves (e.g. `sudo tomb-fido2 unlock ...`), `id -u`/`id -g` report root, not the "real" invoker — the same class of bug this story fixes would reappear, just self-inflicted. The documented/expected usage (per `privileged()`'s own doc comment, Story 1.5) is to run the binary unprivileged and let it prompt for `sudo` only for the specific calls that need it — this story doesn't need to detect/recover `$SUDO_UID`/`$SUDO_GID` for the self-`sudo`'d case; note it as a known limitation if it comes up, don't expand scope to handle it.
- **Git intelligence:** baseline is `09b9d1d` ("unified CLI dispatch & plain-language errors", Story 1.8's merge, current `main` HEAD as of this story's creation) — no other commits since. The mount function, CLI flag definitions, and check_prerequisites lists quoted throughout this story are accurate as of that commit.

### Project Structure Notes

- Modified: `src/adapters/exec/mod.rs` (new `id`-shelling identity helper, `mount`'s real implementation changed: chown added, path construction changed from `/tmp` + random suffix to `/run/media/<username>/<tomb-name>` + collision fallback, `"id"` added to `check_prerequisites`), `src/cli/main.rs` (`path` fields become positional on 3 commands), `tests/unit/cli.rs` (help-text assertion updated), `tests/hardware/main.rs` (extended assertions + a collision scenario).
- Do **not** touch `src/domain/mapping_name.rs` (AC #4 — mapping name unchanged), `src/domain/workflows/{create,unlock}.rs` (their logic and signatures are unchanged; only how `unlock`'s CLI wrapper and the adapter's `mount` behave underneath change), `src/ports/*` (no port signature changes needed), `src/cli/ux.rs` (no new `DomainError` variant needed — this story's failure modes fold into the existing `AdapterFailure` catch-all, same category as `mount`'s existing failure messages, per Story 1.8's `translate_adapter_failure` classifier), or `src/domain/workflows/{enroll,revoke,close,resize}.rs` (later epics, untouched).
- Consistent with `ARCHITECTURE-SPINE.md`'s Structural Seed: `adapters/exec/` gains its first `id`-based subprocess call; no new module, port, or domain type needed.

### References

- [Source: _bmad-output/implementation-artifacts/epic-1-retro-2026-07-24.md] (origin of this story — the 3 agreed post-epic findings and the mapping-name-stays-as-is decision)
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-12 — Deterministic mapping name and mountpoint discovery, no registry]
- [Source: _bmad-output/planning-artifacts/architecture/architecture-tomb-fido2-2026-07-22/ARCHITECTURE-SPINE.md#AD-13 — Placeholder-name isolation]
- [Source: src/adapters/exec/mod.rs:29-46] (`binary_on_path`, `privileged()` helpers to reuse unchanged)
- [Source: src/adapters/exec/mod.rs:111] (`random_hex_suffix()` — reused for the collision-fallback suffix only, no longer for the primary path)
- [Source: src/adapters/exec/mod.rs:798-812] (`FilesystemBackend::check_prerequisites`'s current required-binaries list, to extend with `"id"`)
- [Source: src/adapters/exec/mod.rs:896-955] (`FilesystemBackend::mount`'s current real implementation — the exact code this story changes)
- [Source: src/domain/types.rs:29-39] (`MapperHandle`, already carrying `source_path` — no signature change needed)
- [Source: src/cli/main.rs:22-75] (`Commands`/`CreateMode` — the 3 `path` field definitions to make positional)
- [Source: tests/unit/cli.rs:78-82] (`unlock_help_lists_path_flag` — needs updating for the positional form)
- [Source: _bmad-output/implementation-artifacts/1-7-unlock-and-mount-a-tomb.md] (previous story to touch `mount` — established the `chmod 0700` fix this story builds on top of, and the hardware-run environment caveats: build `cargo test --test hardware --no-run` as the normal user, then run under `sudo` outside the Nix devShell)
- [Source: _bmad-output/implementation-artifacts/1-8-unified-cli-dispatch-plain-language-errors.md] (previous story — most recent CLI-layer work, established `cli::ux`'s `AdapterFailure` classification this story's new failure messages will fall into unchanged)

## Dev Agent Record

### Agent Model Used

### Debug Log References

### Completion Notes List

- Task 1: Added `invoking_identity()` (+ `InvokingIdentity` struct) to `src/adapters/exec/mod.rs`, shelling out to `id -u`/`id -g`/`id -un` unprivileged, following the same `.output()`/`status.success()`/plain-string-parse pattern as `device_capacity`'s `blockdev --getsize64` call. Added `"id"` to `check_prerequisites`'s required-binaries list. No new Cargo dependency, no `unsafe`. `cargo build --lib` compiles (helper currently unused, consumed by Task 2/3).

### File List

- `src/adapters/exec/mod.rs`
