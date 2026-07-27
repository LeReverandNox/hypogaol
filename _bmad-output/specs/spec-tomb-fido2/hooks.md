# Hooks (CAP-16)

Mechanism detail for bind-hooks and exec-hooks. Adapted from dyne/tomb's hooks model (`https://dyne.org/docs/tomb/manpage/#hooks`; signal/guardrail behavior cross-checked against dyne/tomb's actual implementation at `tomb#L2824-2910`, `#L3391-3406`) — similar in spirit, not necessarily an exact port. Both mechanisms run only at the `open` and `close` lifecycle points (not create, resize, or read-only unlock).

## bind-hooks

A file named `bind-hooks` in the tomb's own root: a two-column, whitespace-separated list, one mapping per line — first column a path relative to the tomb root, second column a path relative to `$HOME`.

```
mail          mail
.gnupg        .gnupg
.mozilla      .mozilla
```

On `open`, for each line, the tool bind-mounts (`mount -o bind`) the tomb-relative path onto the `$HOME`-relative path.

**Guardrails (stricter than dyne/tomb's stock model):**
- Both the resolved source and destination paths must exist before mounting; a missing path skips that mapping with a warning, not a hard failure of the whole open.
- The resolved source path must stay within the tomb root, and the resolved destination path must stay within `$HOME` — an entry using `..` or an absolute path to escape either root is rejected (skipped with a warning), not applied. dyne/tomb itself performs no such containment check.

## exec-hooks

A file named `exec-hooks` in the tomb's own root, run as the invoking user (never with elevated privilege, regardless of what privilege the lifecycle step itself needed):

- On `open`: invoked with arguments `open <mountpoint>`.
- On `close`: invoked with arguments `close <mountpoint> <tomb-name> <loopback-device> <mapper-device>`.

**Guardrails (stricter than dyne/tomb's stock model):**
- Must be a regular file (not a symlink or other non-regular file) with the executable bit set. dyne/tomb only checks the executable bit, which a symlink also satisfies.
- Must be owned by the invoking user or by root, and must not be world-writable. dyne/tomb performs no ownership or permission check, so any tomb whose backing file/hooks weren't authored by the current user (inherited, restored from a shared backup, downloaded) would otherwise execute arbitrary code silently on open/close.

## Disabling hooks

A per-invocation flag (mirroring dyne/tomb's `-n`) skips both bind-hooks and exec-hooks processing entirely for that command.
