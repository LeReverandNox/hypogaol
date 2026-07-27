# Architecture Spine Rubric Review — v4 (2026-07-27 update)

Scope: `ARCHITECTURE-SPINE.md` as amended for Epic 4 (CAP-12..17), judged against SPEC.md, hooks.md, and the real brownfield source in `src/`. This review covers the whole document but weights AD-14..19 (new) and the amended AD-1/2/4/6/7/8/13 most heavily, per the request.

## Verdict

The v4 update is substantively sound — every new AD (14-19) states an enforceable rule that plausibly prevents its named divergence, all CAP-1..17 are covered with no regression from the pre-Epic-4 sections, and every checked piece of named tech (psmisc/fuser, util-linux `kill`, `cryptsetup token export/import`, `systemd-cryptenroll --fido2-with-user-verification`, cargo-dist, all pinned crate versions) is real, current, and web/locally verified. The update's actual defect is a **documentation/brownfield-sync gap, not a design gap**: it edited the Structural Seed's description of `flake.nix` to claim psmisc coverage that was never added to the real `flake.nix`, and it left a pre-existing stale port-method reference (`add_key`) uncorrected in the same line it was editing.

## Findings

### 1. (Medium) v4 claims a devShell capability that doesn't exist — `flake.nix` was never updated

The Structural Seed line was edited in this pass:

```
- flake.nix / flake.lock # Nix devShell: Rust toolchain + cryptsetup/systemd/libfido2/e2fsprogs, no system-wide install
+ flake.nix / flake.lock # Nix devShell: Rust toolchain + cryptsetup/systemd/libfido2/e2fsprogs/psmisc, no system-wide install
```

But the real `/home/rlaidet/src/perso/LeReverandNox/tomb-fido2/flake.nix` `devShells.default.packages` list is only `rustc cargo clippy rustfmt rust-analyzer cryptsetup systemd libfido2` — it contains **neither `e2fsprogs` nor `psmisc`**, before or after this edit. AD-18 (CAP-15, slam) newly makes `fuser`/`kill` hard preflight dependencies (AD-4 says preflight "checks presence" of "the hooks/process tooling AD-14/AD-18 need"). A contributor who enters only the Nix devShell (the isolation the Stack table advertises: "kept out of the contributor's system profile") will have neither tool and `preflight::check` will correctly fail — but the spine asserts this is already covered. This is a real implementation blocker for anyone following the architecture literally, and it's a documentation regression introduced by this exact edit (it added a false claim rather than just failing to add a true one). `e2fsprogs`'s absence predates v4 (Epic 1-3 already needed `mkfs.ext4`/`resize2fs`) and was already inaccurate; v4 compounded it.
**Action:** either add `e2fsprogs`/`psmisc` to `flake.nix`'s `devShells.default.packages`, or correct the Structural Seed line to stop claiming coverage it doesn't have.

### 2. (Low) Stale `add_key` reference in the `LuksBackend` Structural Seed line, uncorrected while editing the same line

```
luks_backend.rs # trait: bootstrap_format_and_open(...)/has_luks2_header(...)/open(...)/close/resize/add_key/remove_key(...)/list_fido2_keyslots/list_open_mappings(AD-17)
```

The real `src/ports/luks_backend.rs` trait has no `add_key` method — FIDO2 enrollment goes entirely through `Fido2Backend::enroll_fido2_key`, never a `LuksBackend` method. This is a pre-existing inaccuracy (unchanged text carried over from the pre-Epic-4 version), but v4 touched this exact line to append `list_open_mappings(AD-17)` and didn't fix the neighboring stale token. Low severity (Structural Seed is illustrative, not the binding Rule text) but it's exactly the kind of "spine ratifies vs. contradicts the brownfield" drift the review was asked to catch, and it was a free fix given the line was already open for editing.

### 3. (Low) Nix channel name mismatch, pre-existing, not introduced by v4

Stack table: "Nix flake devShell (`flake.nix`, nixpkgs-unstable + flake-utils)". Real `flake.nix`: `nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable"`. "nixpkgs-unstable" and "nixos-unstable" are two different real nixpkgs channel branches — the spine names the wrong one. This line is unchanged by the v4 diff, so it's not a new defect, but it remains live in the reviewed document.

### 4. (Low/nit) AD-14 ↔ AD-8's "Rule of Three" cross-reference is circular and slightly imprecise
AD-8 defers its rationale for growing `FilesystemBackend` with hook/escalation methods to "AD-14's Rule-of-Three rationale," while AD-14 in turn justifies itself as "the same call AD-9 already made for CAP-8's query methods." AD-9's Rule *does* in fact add query methods (`path_exists`, `has_luks2_header`, `device_capacity`) to the existing ports rather than inventing new ones, so the substance holds up — but neither AD-8, AD-9, nor AD-14 is the original source of the term "Rule of Three," and the two ADs point at each other rather than at a single authoritative statement. Purely a traceability nit; doesn't change the enforceability of either rule.

### 5. (Worth flagging, not a defect) AD-14 doesn't say who resolves `$HOME` for bind-hooks
AD-14's Rule requires bind-hooks destinations to resolve within `$HOME`, and hooks.md repeats this guardrail, but neither says which layer reads the `$HOME` environment variable — `domain` (which the Design Paradigm says "depends on nothing but three trait-defined ports") or `cli` (which would then have to thread it into `domain::workflows::unlock`/`::close` as a new parameter, unlike every other AD-14 detail which lives entirely behind `FilesystemBackend`). Real precedent exists for `domain` making a direct, non-port std call for a deterministic query (`mapping_name.rs` calls `std::fs::canonicalize` directly, `resize.rs` calls `std::fs::symlink_metadata`/`metadata` directly), so this is resolvable by analogy, but AD-14 doesn't make the call explicitly the way AD-11's `read_only: bool` or AD-19's `Stage` callback do. Two independent implementers could plausibly choose differently (domain reads env var vs. cli passes it in), which is exactly the class of drift AD-14 is otherwise careful to close off elsewhere in the same rule.

## What checked out clean

- **CAP coverage:** all of CAP-1..17 present in the Capability → Architecture Map with no regression in the CAP-1..11 rows.
- **AD-14..19 enforceability:** each states a concrete, type-level or ordering-level rule (skip flags, hard-error vs. skip-with-warning distinctions, exact signal/pause sequence for AD-18, callback-not-I/O for AD-19) that a fake-port unit test could actually verify per AD-7.
- **Brownfield consistency spot-checks:** `LuksBackend::open(path, name: &str, read_only: bool)`, `remove_key` (token-then-keyslot order), `FilesystemBackend::mount/umount` ordering in `close.rs`/`unlock.rs`, `CreateTarget` enum shape, `mapping_name.rs`'s FNV-1a + fixed prefix, and the AD-4 preflight-first pattern in every workflow all match the real code exactly.
- **Named tech, web/locally verified 2026-07-27:** `systemd-cryptenroll --fido2-with-user-verification=yes|no` (real flag; UV requirement is stored in the FIDO2 credential/LUKS2 token and read automatically by the plugin at open — confirms AD-16's "Resolved" claim); `cryptsetup token export`/`import` (real, AD-2's spike plan is sound); `fuser -m` and util-linux `kill -s` (both real, both present as standalone binaries independent of shell builtins — confirmed `/usr/bin/kill` from util-linux 2.42.2, `fuser` from psmisc 23.7 locally); `fido2-token` 1.17.0 and `systemd` 261 match local toolchain. All pinned crate versions (`clap` 4.6.4, `serde` 1.0.229, `thiserror` 2.0.19, `anyhow` 1.0.104, `zeroize` 1.9.0) match `Cargo.toml`/`Cargo.lock` exactly; `rust-toolchain.toml` pins 1.90.0 as claimed.
- **Both SPEC "intentionally unresolved" assumptions are actually resolved:** AD-16 answers whether `open`/`resize` need a UV parameter (no); AD-17 answers the close-all/slam discovery mechanism (live `dmsetup ls` + `cryptsetup status`, prefix-filtered, no registry).
- **Deferred section:** none of the seven items would let two independently-built units diverge in a way that matters — each is either a genuinely low-likelihood/single-user-scoped risk explicitly named as such (concurrency/TOCTOU, slam's race window), or a fixed, non-configurable behavior both implementers would build identically (1s escalation pause, no hook timeout, ext4-only, no crash-resume).
