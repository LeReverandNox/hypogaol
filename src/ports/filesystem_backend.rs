use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use crate::domain::errors::DomainError;
use crate::domain::types::{Filesystem, HookFileMeta, LockGuard, MapperHandle, Pid, Signal};

pub trait FilesystemBackend {
    /// `Err` carries one human-readable string per missing/unsupported dependency
    /// this port's real adapter needs; `Ok(())` means all are satisfied.
    /// `filesystem` narrows which mkfs/growfs toolchain to check: `None`
    /// means no such toolchain is needed for this operation (every
    /// non-create/resize workflow); `Some(fs)` means check exactly `fs`'s
    /// toolchain, not the others (`create` passes the requested type,
    /// `resize` the existing one).
    fn check_prerequisites(&self, filesystem: Option<Filesystem>) -> Result<(), Vec<String>>;

    /// True if `path` already exists (AD-9's create-mode refusal check).
    fn path_exists(&self, path: &Path) -> bool;

    /// True if `path` is a raw block device/partition; false if it's a
    /// regular file (Story 3.2, AC #1/#2's file-vs-device distinction) — a
    /// pure query, no mutation.
    fn is_block_device(&self, path: &Path) -> Result<bool, DomainError>;

    /// Byte capacity of the block device/partition at `path` (AD-9's
    /// device-mode default-to-full-capacity sizing) — a pure query, no
    /// mutation.
    fn device_capacity(&self, path: &Path) -> Result<u64, DomainError>;

    /// Creates the backing file at `path` sized to exactly `size` bytes; fails
    /// if a file (or symlink) already exists at `path` — `path_exists` narrows
    /// the check-then-create race but does not eliminate it, so this call
    /// itself must refuse to clobber anything already there (AC #2).
    fn set_backing_file_size(&self, path: &Path, size: u64) -> Result<(), DomainError>;

    /// Formats the opened mapping with `fs` (`Filesystem::Ext4`/`Xfs`/`Btrfs`,
    /// AD-8).
    fn mkfs(&self, mapper: &MapperHandle, fs: Filesystem) -> Result<(), DomainError>;

    /// Grows `fs` on `mapper`'s already-resized mapping to fill it
    /// (`Filesystem::Ext4`/`Xfs`/`Btrfs`, AD-8) — Story 3.2, AC #1/#4; Story
    /// 6.4 added Xfs/Btrfs. Called after `LuksBackend::resize`, so the
    /// mapping already reflects the new, larger size; no explicit target
    /// size is passed.
    fn growfs(&self, mapper: &MapperHandle, fs: Filesystem) -> Result<(), DomainError>;

    /// The `fs` filesystem's own current size on `mapper`'s active mapping
    /// (`Filesystem::Ext4`/`Xfs`/`Btrfs`) — a pure query, no mutation. This is
    /// deliberately distinct from `device_capacity(&mapper.device_node())`:
    /// confirmed empirically on real hardware that a LUKS2 mapping's dynamic
    /// segment always reflects the *full* backing storage on every reopen,
    /// even for a device-backed volume created with less than the raw
    /// device's full capacity (Story 1.6 headroom) — so the mapping's own
    /// size can never distinguish "this volume's filesystem currently uses
    /// less than the raw device" from "it uses all of it." Only the
    /// filesystem's own superblock (block count × block size) reports the
    /// volume's true current provisioned size.
    fn filesystem_size(&self, mapper: &MapperHandle, fs: Filesystem) -> Result<u64, DomainError>;

    /// Best-effort removal of a backing file this adapter created — used to
    /// clean up after a file-backed `create` fails partway through, so a
    /// retry at the same destination isn't permanently blocked.
    fn remove_backing_file(&self, path: &Path) -> Result<(), DomainError>;

    /// Mounts `mapper`'s decrypted device node at a fresh, uniquely-named
    /// mount point, letting the kernel auto-detect the filesystem type from
    /// the superblock. Returns the mount point. The mount point is never
    /// stored or derived from `mapper`'s path (AD-12) — rediscovering it
    /// later is `close`'s job (Story 3.1), via the kernel's own mount table.
    /// `read_only` maps to `mount -o ro` (AD-11).
    fn mount(&self, mapper: &MapperHandle, read_only: bool) -> Result<PathBuf, DomainError>;

    /// Unmounts `mapper`'s decrypted device node, resolving the live mount
    /// point itself via the kernel's mount table (AD-12) — takes the mapper,
    /// never a mountpoint, since none is ever stored. Also removes the
    /// now-empty mount-point directory `mount` created, so a later re-unlock
    /// of the same volume gets the plain basename back rather than permanently
    /// falling back to a collision-suffixed name.
    fn umount(&self, mapper: &MapperHandle) -> Result<(), DomainError>;

    /// Bind-mounts `source` onto `dest` (`mount --bind`) — privileged, one
    /// per valid `bind-hooks` entry (AC #1).
    fn bind_mount(&self, source: &Path, dest: &Path) -> Result<(), DomainError>;

    /// `stat`/`lstat` facts about a candidate `exec-hooks` file (AC #3) — an
    /// unprivileged query, no `mount`/`umount`-style privilege needed.
    fn hook_file_metadata(&self, path: &Path) -> Result<HookFileMeta, DomainError>;

    /// Runs `path` with `args` as the already-unprivileged invoking process —
    /// deliberately **not** privileged (AC #3's "never elevated"), no
    /// explicit privilege-drop needed since this process never escalated to
    /// begin with. `Err` only if the process fails to spawn at all; a
    /// nonzero exit from the hook script itself is not an `Err` — `domain`
    /// inspects the returned `ExitStatus` and reports a non-fatal
    /// `HookWarning::ExecHookNonZeroExit`, never aborting the workflow.
    fn run_hook(&self, path: &Path, args: &[&str]) -> Result<ExitStatus, DomainError>;

    /// The invoking user's home directory — reads `$HOME`, falling back to a
    /// passwd lookup by uid if unset.
    fn invoking_home_dir(&self) -> Result<PathBuf, DomainError>;

    /// `mapper`'s live mount point, via the kernel's own mount table (AD-12)
    /// — a pure query, wrapping the exact `findmnt` logic `umount` already
    /// uses internally. Lets `close::run` learn the mountpoint up front to
    /// build hook file paths and `run_hook`'s `close` argument, without
    /// `AD-12` ever storing one.
    fn mount_point_of(&self, mapper: &MapperHandle) -> Result<PathBuf, DomainError>;

    /// Unmounts a single `bind-hooks` destination directly (`umount <dest>`,
    /// no `findmnt` resolution — the caller already knows `dest` *is* the
    /// mountpoint, straight out of the `bind-hooks` file) and does not
    /// remove it afterward (unlike `umount`'s own mount-point directory: a
    /// bind-hook destination is user-owned and pre-existing under `$HOME`,
    /// never created by this tool). Used only by `close::run`'s bind-hooks
    /// teardown step, once per parsed entry; individual failures are the
    /// caller's to ignore (AC #5).
    fn unmount_bind_hook_destination(&self, dest: &Path) -> Result<(), DomainError>;

    /// Every process ID currently holding `mountpoint` open (`fuser -m`),
    /// used by slam's busy-mount escalation (AD-18) to know who to signal.
    /// `Ok(vec![])` means nothing holds it open — not an error — since the
    /// escalation loop uses an empty result as its own "no holders remain,
    /// stop escalating" exit condition.
    fn processes_using(&self, mountpoint: &Path) -> Result<Vec<Pid>, DomainError>;

    /// Sends `signal` to `pid` (`kill -s <signal> <pid>`). Best-effort from
    /// the caller's perspective — slam's escalation loop ignores a single
    /// failed signal (e.g. the process already exited between
    /// `processes_using` and this call) rather than treating it as fatal.
    fn signal_process(&self, pid: Pid, signal: Signal) -> Result<(), DomainError>;

    /// Writes `domain::hooks::BIND_HOOKS_TEMPLATE`/`EXEC_HOOKS_TEMPLATE`
    /// (CAP-19, AD-9) into `mountpoint` as `bind-hooks` and
    /// `exec-hooks.example` respectively — an unprivileged write, needing no
    /// FIDO2 selection: by the time `create` calls this, `mount` has already
    /// chowned `mountpoint` to the invoking user.
    fn scaffold_hook_templates(&self, mountpoint: &Path) -> Result<(), DomainError>;

    /// Acquires a non-blocking, exclusive, process-scoped lock keyed by
    /// `path` (AD-20, CAP-24) — the second statement of every mutating
    /// workflow (`create`/`enroll`/`revoke`/`close`/`resize`), immediately
    /// after `preflight` passes. Returns `DomainError::LockContention`
    /// immediately (never blocks) if another invocation already holds it.
    /// The real implementation is a Linux abstract-namespace socket — a
    /// kernel-only resource, never written to any filesystem — so it can
    /// never collide with `cryptsetup`/`systemd-cryptenroll`'s own internal
    /// LUKS2 metadata locking on the target itself. This assumes every
    /// concurrent invocation runs in the same network namespace — an
    /// abstract socket name is scoped per network namespace, so two
    /// invocations under different namespaces would each bind their own
    /// copy of the same name and never contend (accepted, documented
    /// limitation, 2026-08-10 — see `lock_mapping` for why `close_all`/
    /// `slam` use a separate method). The returned `LockGuard` releases the
    /// lock when dropped — hold it for the remainder of the caller's work;
    /// dropping it early re-opens the race window this method exists to
    /// close. `info` and `unlock` (including read-only unlock) never call
    /// this — excluded from the guard by design (AC #4).
    fn lock_target(&self, path: &Path) -> Result<LockGuard, DomainError>;

    /// Same lock as `lock_target`, keyed directly by an already-known
    /// dm-crypt mapping name (`MapperHandle::name`) rather than re-derived
    /// from a filesystem path — used by `close_all`/`slam`, once per
    /// mapping inside their existing discovery loops, since a discovered
    /// mapping's backing storage may no longer exist on disk (`lock_target`
    /// would otherwise hard-fail there instead of contending correctly,
    /// review finding 2026-08-10). The real implementation derives an
    /// identical final lock name to `lock_target`'s for the same volume, so
    /// a concurrent path-keyed `lock_target` call against that volume still
    /// contends with this one. `display_path` is used only for the
    /// contention error's human-facing text, not the lock derivation.
    fn lock_mapping(&self, name: &str, display_path: &Path) -> Result<LockGuard, DomainError>;
}
