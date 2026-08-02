use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::Command;

use hypogaol::adapters::exec::ExecAdapter;
use hypogaol::domain::mapping_name;
use hypogaol::domain::types::{CreateTarget, Filesystem};
use hypogaol::domain::workflows::{close, create, enroll, info, resize, revoke, slam, unlock};
use hypogaol::ports::fido2_backend::Fido2DeviceSelection;
use hypogaol::ports::luks_backend::LuksBackend;

/// No-op progress callback (separate test binary from `tests/unit`, so it
/// gets its own copy of this helper rather than sharing `tests/unit/fakes.rs`).
/// Generic over both `CreateStage` and `ResizeStage` via inference at each
/// call site.
fn no_progress<S>(_stage: S) {}

/// Attaches a genuine `/dev/loopN` block device backed by a disposable file —
/// `cryptsetup`/`blockdev` treat it identically to physical storage, so this
/// exercises the real Device code path without requiring (and risking data
/// loss on) a spare physical disk/partition.
///
/// Deliberately does *not* auto-detach on drop: the whole point of this
/// test's printed instructions is to let a human inspect the still-open
/// volume by hand afterward (mount, `dumpe2fs`, etc.), and an unconditional
/// `Drop`-based `losetup -d` would tear the loop device down the instant the
/// test function returns — before those instructions are ever followed.
/// Detaching is the last step of the printed manual sequence instead. Any
/// loop device left over from an interrupted previous run is cleaned up via
/// `detach_stale`, called by the test *before* it deletes/recreates the
/// backing file (see that function's own doc comment for why the ordering
/// matters) so repeated runs don't accumulate stale devices.
struct LoopDevice {
    path: PathBuf,
}

impl LoopDevice {
    /// Detaches any loop device still bound to `backing_file` from a
    /// previous, interrupted run. Must be called with the backing file in
    /// the state a prior run left it in — i.e. *before* the caller deletes
    /// and recreates it. `losetup -j` matches a backing file by its current
    /// dev/inode; once the file is recreated it gets a fresh inode, and a
    /// loop device still bound to the old (now-orphaned) inode can no
    /// longer be found this way.
    fn detach_stale(backing_file: &std::path::Path) {
        if let Ok(existing) = Command::new("sudo")
            .args(["losetup", "-j"])
            .arg(backing_file)
            .output()
        {
            for line in String::from_utf8_lossy(&existing.stdout).lines() {
                if let Some(stale_path) = line.split(':').next() {
                    let _ = Command::new("sudo")
                        .args(["losetup", "-d", stale_path])
                        .output();
                }
            }
        }
    }

    fn attach(backing_file: &std::path::Path) -> Self {
        let output = privileged_output("losetup", &["-f", "--show"], backing_file);
        assert!(
            output.status.success(),
            "losetup -f --show failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        Self { path }
    }
}

fn privileged_output(
    program: &str,
    args: &[&str],
    last_arg: &std::path::Path,
) -> std::process::Output {
    Command::new("sudo")
        .arg(program)
        .args(args)
        .arg(last_arg)
        .output()
        .unwrap_or_else(|e| panic!("failed to run sudo {program}: {e}"))
}

/// End-to-end break-glass verification (Story 1.5, AC #1): create a real
/// file-backed volume via this tool's own `domain::workflows::create`, then
/// independently confirm the result is recognized by bare
/// `cryptsetup`/`fido2-token` (not this tool's own unlock, which doesn't
/// exist until Story 1.7).
///
/// Manual-only (AD-7, `make test-hardware`): requires root (LUKS2
/// `luksOpen`/dm-crypt mapping, mount) and a real FIDO2 security key
/// present, ready to be touched and to enter its PIN when prompted.
#[test]
#[ignore]
fn create_a_file_backed_volume_is_independently_unlockable_via_bare_cryptsetup() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("volume.img");

    let adapter = ExecAdapter::default();
    let target = CreateTarget::File {
        path: path.clone(),
        size: 64 * 1024 * 1024,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    // Break-glass verification: bare cryptsetup, independent of this tool's
    // own code, must recognize the header and its systemd-fido2 token.
    let dump = Command::new("cryptsetup")
        .arg("luksDump")
        .arg(&path)
        .output()
        .expect("failed to run cryptsetup luksDump");
    assert!(dump.status.success(), "cryptsetup luksDump failed");

    let dump_text = String::from_utf8_lossy(&dump.stdout);
    assert!(
        dump_text.contains("systemd-fido2"),
        "expected a systemd-fido2 token in the header, got:\n{dump_text}"
    );

    // The rest of AC #1's break-glass clause — actually running `cryptsetup
    // open` (touch + PIN) and mounting the filesystem — needs a live
    // interactive prompt this test can't automate; finish verifying it by
    // hand, then record the result in the story's Completion Notes:
    println!(
        "Volume created at {}. To finish verifying AC #1 by hand:\n  \
         sudo cryptsetup open --token-only {} volume-fido2-hardware-test\n  \
         sudo mount /dev/mapper/volume-fido2-hardware-test <mountpoint>\n  \
         ls <mountpoint>\n  \
         sudo umount <mountpoint> && sudo cryptsetup close volume-fido2-hardware-test",
        path.display(),
        path.display(),
    );
}

/// Device-backed create's headroom claim (Story 1.6, AC #2): create a real
/// volume on a loop device using a size smaller than the loop device's own
/// capacity, then independently confirm (bare `cryptsetup`/`blockdev`) that
/// the LUKS2 payload is smaller than the full device, leaving free space for
/// a later resize/grow (Story 3.2).
///
/// Manual-only (AD-7, `make test-hardware`): requires root (LUKS2
/// `luksOpen`/dm-crypt mapping, `losetup`) and a real FIDO2 security key
/// present, ready to be touched and to enter its PIN when prompted.
#[test]
#[ignore]
fn create_a_device_backed_volume_leaves_headroom_for_a_later_resize() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-device");
    let backing_file = dir.join("loop-backing.img");

    // Must run before the backing file is deleted/recreated below — see
    // `detach_stale`'s doc comment.
    LoopDevice::detach_stale(&backing_file);

    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");

    let loop_capacity: u64 = 64 * 1024 * 1024;
    let requested_size: u64 = 32 * 1024 * 1024;

    {
        let file = std::fs::File::create(&backing_file).expect("failed to create backing file");
        file.set_len(loop_capacity)
            .expect("failed to size backing file");
    }

    let loop_device = LoopDevice::attach(&backing_file);

    let adapter = ExecAdapter::default();
    let target = CreateTarget::Device {
        path: loop_device.path.clone(),
        size: Some(requested_size),
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    // Break-glass verification: bare cryptsetup, independent of this tool's
    // own code, must recognize the header and its systemd-fido2 token.
    let dump = Command::new("cryptsetup")
        .arg("luksDump")
        .arg(&loop_device.path)
        .output()
        .expect("failed to run cryptsetup luksDump");
    assert!(dump.status.success(), "cryptsetup luksDump failed");

    let dump_text = String::from_utf8_lossy(&dump.stdout);
    assert!(
        dump_text.contains("systemd-fido2"),
        "expected a systemd-fido2 token in the header, got:\n{dump_text}"
    );

    // The raw loop device's own capacity must be untouched (create must
    // never resize/repartition the underlying device or partition table,
    // AD-6/AD-10's scope fence).
    let capacity_output = Command::new("blockdev")
        .arg("--getsize64")
        .arg(&loop_device.path)
        .output()
        .expect("failed to run blockdev --getsize64");
    assert!(
        capacity_output.status.success(),
        "blockdev --getsize64 failed"
    );
    let capacity: u64 = String::from_utf8_lossy(&capacity_output.stdout)
        .trim()
        .parse()
        .expect("failed to parse blockdev --getsize64 output");
    assert_eq!(
        capacity, loop_capacity,
        "create must never touch the underlying device's own geometry"
    );

    // AC #2's headroom claim can't be confirmed from the closed header
    // alone: LUKS2 deliberately leaves `segments.0.size` as `"dynamic"`
    // (recompute from the real device size at every open) rather than
    // persisting a fixed smaller value — confirmed empirically via
    // `cryptsetup status` immediately after `resize --device-size` runs
    // inside `bootstrap_format_and_open`, which showed the *active* mapping
    // genuinely constrained to the requested size at the moment `mkfs` ran.
    // That's by design: it's what lets a later grow (Story 3.2) resize just
    // the ext4 filesystem, with no LUKS2-level resize ever needed. Proving
    // the ext4 filesystem itself was sized to `requested_size` (not the full
    // capacity) needs the mapping reopened, which needs a live FIDO2 touch —
    // the same live-interaction limit Story 1.5's break-glass clause hit, so
    // it's left as a manual step below rather than automated here.
    println!(
        "Device-backed volume created at {} (loop device backed by {}).\n\
         Requested {requested_size} bytes of {capacity} bytes total capacity.\n\
         To finish verifying AC #2's headroom claim by hand:\n  \
         sudo cryptsetup open --token-only {} volume-fido2-hardware-test-device\n  \
         sudo dumpe2fs -h /dev/mapper/volume-fido2-hardware-test-device | grep -E 'Block count|Block size'\n  \
         # confirm block_count * block_size is close to {requested_size} bytes, not {capacity}\n  \
         sudo cryptsetup close volume-fido2-hardware-test-device\n  \
         sudo losetup -d {}",
        loop_device.path.display(),
        backing_file.display(),
        loop_device.path.display(),
        loop_device.path.display(),
    );
}

/// Confirms `findmnt` recognizes `device_node` as actually mounted, and
/// specifically at `mountpoint` — the real, kernel-level check backing AC
/// #1's "the mounted filesystem becomes accessible at a discoverable mount
/// point (via the kernel's mount table)" claim. Checking `TARGET` (not just
/// that `device_node` is mounted *somewhere*) catches a bug that mounted the
/// right device at the wrong directory.
fn assert_actually_mounted(device_node: &std::path::Path, mountpoint: &std::path::Path) {
    let output = Command::new("findmnt")
        .args(["-n", "-o", "TARGET", "--source"])
        .arg(device_node)
        .output()
        .expect("failed to run findmnt");
    assert!(
        output.status.success(),
        "expected {} to be mounted, findmnt found nothing: {}",
        device_node.display(),
        String::from_utf8_lossy(&output.stderr)
    );

    let actual_target = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(
        actual_target,
        mountpoint.display().to_string(),
        "{} is mounted, but not at the mount point unlock::run returned",
        device_node.display()
    );
}

/// Writes a marker file at `mountpoint` and reads it back, proving the
/// filesystem `unlock::run` mounted is actually readable/writable, not just
/// present in the mount table.
fn assert_readable_and_writable(mountpoint: &std::path::Path) {
    let marker = mountpoint.join("volume-fido2-marker.txt");
    std::fs::write(&marker, b"volume-fido2 hardware test").expect("failed to write marker file");
    let contents = std::fs::read_to_string(&marker).expect("failed to read marker file back");
    assert_eq!(contents, "volume-fido2 hardware test");
}

/// Runs unprivileged `id <flag>`, independent of `ExecAdapter`'s own
/// `invoking_identity()` — this test process must itself run unprivileged
/// (see the References section on the hardware-run environment: individual
/// operations escalate via their own `sudo` call, the test binary never
/// runs as root as a whole), so this reports the same real invoking
/// identity `mount`'s `chown` is expected to have used.
fn id_output(flag: &str) -> String {
    let output = Command::new("id")
        .arg(flag)
        .output()
        .unwrap_or_else(|e| panic!("failed to run id {flag}: {e}"));
    assert!(output.status.success(), "id {flag} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Confirms `mountpoint`'s directory entry is owned by the invoking (real,
/// non-root) user and group, and restricted to `0700` — together, the full
/// ownership bug this story (AC #1) fixes: owned by the invoking user *and*
/// inaccessible to any other local user.
fn assert_owned_by_invoking_user(mountpoint: &std::path::Path) {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let metadata = std::fs::metadata(mountpoint)
        .unwrap_or_else(|e| panic!("failed to stat {}: {e}", mountpoint.display()));

    let expected_uid: u32 = id_output("-u")
        .parse()
        .expect("failed to parse id -u output");
    let expected_gid: u32 = id_output("-g")
        .parse()
        .expect("failed to parse id -g output");

    assert_eq!(
        metadata.uid(),
        expected_uid,
        "{} is owned by uid {}, expected the invoking user's uid {expected_uid} (not root)",
        mountpoint.display(),
        metadata.uid()
    );
    assert_eq!(
        metadata.gid(),
        expected_gid,
        "{} is owned by gid {}, expected the invoking user's gid {expected_gid}",
        mountpoint.display(),
        metadata.gid()
    );

    let mode = metadata.permissions().mode() & 0o777;
    assert_eq!(
        mode,
        0o700,
        "{} has mode {mode:o}, expected 0700 (inaccessible to any other local user)",
        mountpoint.display()
    );
}

/// Confirms `mkfs.ext4`'s auto-created `lost+found` was also handed to the
/// invoking user, not left root-owned — the mount point's own chown only
/// covers its own inode, not this pre-existing entry underneath it.
fn assert_lost_and_found_owned_by_invoking_user(mountpoint: &std::path::Path) {
    use std::os::unix::fs::MetadataExt;

    let lost_and_found = mountpoint.join("lost+found");
    let metadata = std::fs::metadata(&lost_and_found)
        .unwrap_or_else(|e| panic!("failed to stat {}: {e}", lost_and_found.display()));

    let expected_uid: u32 = id_output("-u")
        .parse()
        .expect("failed to parse id -u output");
    let expected_gid: u32 = id_output("-g")
        .parse()
        .expect("failed to parse id -g output");

    assert_eq!(
        metadata.uid(),
        expected_uid,
        "{} is owned by uid {}, expected the invoking user's uid {expected_uid} (not root)",
        lost_and_found.display(),
        metadata.uid()
    );
    assert_eq!(
        metadata.gid(),
        expected_gid,
        "{} is owned by gid {}, expected the invoking user's gid {expected_gid}",
        lost_and_found.display(),
        metadata.gid()
    );
}

/// Confirms `mountpoint` lives directly under `/run/media/<username>/` and
/// its basename matches `source_path`'s `file_stem()`, optionally followed
/// by a `-<suffix>` collision fallback (AC #2).
fn assert_mountpoint_under_run_media(mountpoint: &std::path::Path, source_path: &std::path::Path) {
    let username = id_output("-un");
    let expected_parent = PathBuf::from(format!("/run/media/{username}"));
    assert_eq!(
        mountpoint.parent(),
        Some(expected_parent.as_path()),
        "{} is not directly under {}",
        mountpoint.display(),
        expected_parent.display()
    );

    let expected_stem = source_path
        .file_stem()
        .unwrap_or(source_path.as_os_str())
        .to_string_lossy()
        .into_owned();
    let actual_basename = mountpoint
        .file_name()
        .expect("mountpoint has no basename")
        .to_string_lossy()
        .into_owned();
    assert!(
        actual_basename == expected_stem
            || actual_basename.starts_with(&format!("{expected_stem}-")),
        "{} basename {actual_basename:?} doesn't match source stem {expected_stem:?} (with optional collision suffix)",
        mountpoint.display()
    );
}

/// Manual-close cleanup: `close` (Story 3.1) doesn't exist yet, so unmount
/// and close the mapping directly via bare `cryptsetup`, mirroring the
/// break-glass pattern already used elsewhere in this file.
fn unmount_and_close(mountpoint: &std::path::Path, mapping_name: &str) {
    let umount = Command::new("sudo")
        .arg("umount")
        .arg(mountpoint)
        .output()
        .expect("failed to run umount");
    assert!(
        umount.status.success(),
        "umount failed: {}",
        String::from_utf8_lossy(&umount.stderr)
    );
    let _ = std::fs::remove_dir(mountpoint);

    let close = Command::new("sudo")
        .args(["cryptsetup", "close", mapping_name])
        .output()
        .expect("failed to run cryptsetup close");
    assert!(
        close.status.success(),
        "cryptsetup close failed: {}",
        String::from_utf8_lossy(&close.stderr)
    );
}

/// Guards a hardware unlock scenario's cleanup (unmount, close, optional
/// loop-device detach) so a panicking assertion mid-test — e.g. in
/// `assert_actually_mounted`/`assert_readable_and_writable` — doesn't leak
/// mount/mapping/loop-device state, mirroring `RealFixtureFile`'s Drop-based
/// cleanup in `tests/unit/unlock.rs`. Call `.run()` explicitly at the normal
/// end of a test for the full asserted cleanup (via `unmount_and_close`); if
/// a panic happens first, `Drop` runs a best-effort fallback instead —
/// asserting again while already unwinding would abort the process rather
/// than report the original test failure.
struct UnlockCleanup {
    mountpoint: PathBuf,
    mapping_name: String,
    loop_device_path: Option<PathBuf>,
    done: bool,
}

impl UnlockCleanup {
    fn new(mountpoint: PathBuf, mapping_name: String) -> Self {
        Self {
            mountpoint,
            mapping_name,
            loop_device_path: None,
            done: false,
        }
    }

    fn with_loop_device(mut self, loop_device_path: PathBuf) -> Self {
        self.loop_device_path = Some(loop_device_path);
        self
    }

    fn run(mut self) {
        unmount_and_close(&self.mountpoint, &self.mapping_name);
        if let Some(loop_path) = &self.loop_device_path {
            let detach = Command::new("sudo")
                .args(["losetup", "-d"])
                .arg(loop_path)
                .output()
                .expect("failed to run losetup -d");
            assert!(
                detach.status.success(),
                "losetup -d failed: {}",
                String::from_utf8_lossy(&detach.stderr)
            );
        }
        self.done = true;
    }
}

impl Drop for UnlockCleanup {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        let _ = Command::new("sudo")
            .arg("umount")
            .arg(&self.mountpoint)
            .output();
        let _ = std::fs::remove_dir(&self.mountpoint);
        let _ = Command::new("sudo")
            .args(["cryptsetup", "close", &self.mapping_name])
            .output();
        if let Some(loop_path) = &self.loop_device_path {
            let _ = Command::new("sudo")
                .args(["losetup", "-d"])
                .arg(loop_path)
                .output();
        }
    }
}

/// End-to-end unlock verification (Story 1.7, AC #1): create a real
/// file-backed volume via this tool's own `create::run`, then unlock and mount
/// it via `unlock::run`, and confirm — independently of this tool's own
/// code, via `findmnt` — that the returned mount point is actually mounted
/// and its filesystem is readable/writable.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a real FIDO2
/// security key present, ready to be touched and to enter its PIN when
/// prompted (once for `create`'s enrollment, once more for `unlock`'s open).
#[test]
#[ignore]
fn unlock_mounts_a_file_backed_volume_with_a_readable_writable_filesystem() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-unlock");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("volume.img");

    let adapter = ExecAdapter::default();
    let target = CreateTarget::File {
        path: path.clone(),
        size: 64 * 1024 * 1024,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    let mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("unlock::run failed");

    let name = mapping_name::mapping_name(&path).expect("failed to derive mapping name");
    let device_node = PathBuf::from(format!("/dev/mapper/{name}"));
    let cleanup = UnlockCleanup::new(mountpoint.clone(), name);

    assert_actually_mounted(&device_node, &mountpoint);
    assert_readable_and_writable(&mountpoint);
    assert_owned_by_invoking_user(&mountpoint);
    assert_lost_and_found_owned_by_invoking_user(&mountpoint);
    assert_mountpoint_under_run_media(&mountpoint, &path);

    cleanup.run();
}

/// Covers AC #2 (identical `unlock::run` call, no branching on target type)
/// against a real device-backed target instead of a plain file, using the
/// same `LoopDevice` helper as the device-backed create hardware test above.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a real FIDO2
/// security key present, ready to be touched and to enter its PIN when
/// prompted (once for `create`'s enrollment, once more for `unlock`'s open).
#[test]
#[ignore]
fn unlock_works_unmodified_against_a_device_backed_volume() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-unlock-device");
    let backing_file = dir.join("loop-backing.img");

    // Must run before the backing file is deleted/recreated below — see
    // `detach_stale`'s doc comment.
    LoopDevice::detach_stale(&backing_file);

    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");

    let loop_capacity: u64 = 64 * 1024 * 1024;
    {
        let file = std::fs::File::create(&backing_file).expect("failed to create backing file");
        file.set_len(loop_capacity)
            .expect("failed to size backing file");
    }

    let loop_device = LoopDevice::attach(&backing_file);

    let adapter = ExecAdapter::default();
    let target = CreateTarget::Device {
        path: loop_device.path.clone(),
        size: None,
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    // Identical unlock::run call as the file-backed scenario above — no
    // different flags or behavior branch based on target type (AC #2).
    let mountpoint = unlock::run(
        &loop_device.path,
        false,
        false,
        &|_| {},
        &adapter,
        &adapter,
        &adapter,
    )
    .expect("unlock::run failed");

    let name =
        mapping_name::mapping_name(&loop_device.path).expect("failed to derive mapping name");
    let device_node = PathBuf::from(format!("/dev/mapper/{name}"));
    let loop_device_path = loop_device.path.clone();
    let cleanup = UnlockCleanup::new(mountpoint.clone(), name).with_loop_device(loop_device.path);

    assert_actually_mounted(&device_node, &mountpoint);
    assert_readable_and_writable(&mountpoint);
    assert_owned_by_invoking_user(&mountpoint);
    // `MapperHandle::source_path` is the device path unlock::run was called
    // with (`loop_device_path`, e.g. `/dev/loop0`), not the loop-backing
    // file — `file_stem()` on an extensionless device path is the whole
    // basename (`/dev/sdb1` -> `sdb1`), matching AC #2 directly.
    assert_mountpoint_under_run_media(&mountpoint, &loop_device_path);

    cleanup.run();
}

/// Covers AC #1/#2/#3 of Story 3.3 (read-only unlock): unlock once
/// read-write to establish real chown/chmod ownership on the volume's root
/// inode (so the read-only assertions below aren't confounded by the
/// "never-writably-mounted" edge case, per Task 2's design note), write a
/// marker file, close, then re-unlock the same volume read-only and confirm
/// writes are rejected at both the filesystem level (a write attempt fails)
/// and the underlying dm-crypt mapping level (`mount -o remount,rw` also
/// fails, per AC #2's explicit "including a later remount attempt").
/// AC #3 (rollback on a read-only `luksOpen`-succeeds-but-mount-fails) is
/// covered at the unit level instead (see `tests/unit/unlock.rs`'s
/// `read_only_mount_failure_still_closes_the_just_opened_mapping`) — a
/// real, freshly-formatted ext4 filesystem essentially never fails a
/// read-only mount, so there's no organic way to force this on real
/// hardware without a fragile contrivance.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a real FIDO2
/// security key present, ready to be touched and to enter its PIN when
/// prompted (once for `create`'s enrollment, once for the writable unlock,
/// once for the read-only unlock).
#[test]
#[ignore]
fn unlock_read_only_rejects_writes_at_both_layers_including_remount() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-unlock-read-only");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("volume.img");

    let adapter = ExecAdapter::default();
    let target = CreateTarget::File {
        path: path.clone(),
        size: 64 * 1024 * 1024,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    let name = mapping_name::mapping_name(&path).expect("failed to derive mapping name");
    let device_node = PathBuf::from(format!("/dev/mapper/{name}"));

    // Writable unlock first, to establish real chown/chmod ownership on the
    // volume's own root inode before the read-only assertions below.
    let mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("writable unlock::run failed");
    assert_readable_and_writable(&mountpoint);
    let marker = mountpoint.join("volume-fido2-marker.txt");
    let marker_contents = std::fs::read_to_string(&marker)
        .expect("failed to read back marker written by the writable unlock");

    let result = close::run(&path, false, &|_| {}, &adapter, &adapter, &adapter);
    assert!(result.is_ok(), "close::run failed: {result:?}");

    // Now the read-only unlock under test.
    let mountpoint = unlock::run(&path, true, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("read-only unlock::run failed");
    let cleanup = UnlockCleanup::new(mountpoint.clone(), name);

    assert_actually_mounted(&device_node, &mountpoint);
    assert_owned_by_invoking_user(&mountpoint);

    let reread_contents = std::fs::read_to_string(&marker)
        .expect("marker written by the earlier writable unlock should still be readable");
    assert_eq!(
        reread_contents, marker_contents,
        "marker contents must be unchanged across the read-only re-unlock"
    );

    let write_result = std::fs::write(mountpoint.join("volume-fido2-write-attempt.txt"), b"nope");
    assert!(
        write_result.is_err(),
        "a write attempt inside a read-only mount must fail, got {write_result:?}"
    );

    // Filesystem-level `-o ro` alone would not prove the underlying dm-crypt
    // mapping itself refuses to become writable (NFR11) — attempting a
    // remount to rw must also fail, proving the block-device-level
    // `--readonly` is load-bearing too.
    let remount = Command::new("mount")
        .args(["-o", "remount,rw"])
        .arg(&mountpoint)
        .output()
        .expect("failed to run mount -o remount,rw");
    assert!(
        !remount.status.success(),
        "mount -o remount,rw succeeded against a --readonly dm-crypt mapping; expected it to fail"
    );

    cleanup.run();
}

/// Covers AC #5 of Story 3.3 (read-only unlock makes no branching decision
/// based on target type): the identical read-only scenario above, against a
/// real device-backed target instead of a plain file, using the same
/// `LoopDevice` helper as the other device-backed hardware tests.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a real FIDO2
/// security key present, ready to be touched and to enter its PIN when
/// prompted (once for `create`'s enrollment, once for the writable unlock,
/// once for the read-only unlock).
#[test]
#[ignore]
fn unlock_read_only_rejects_writes_at_both_layers_against_a_device_backed_volume() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-unlock-read-only-device");
    let backing_file = dir.join("loop-backing.img");

    LoopDevice::detach_stale(&backing_file);

    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");

    let loop_capacity: u64 = 64 * 1024 * 1024;
    {
        let file = std::fs::File::create(&backing_file).expect("failed to create backing file");
        file.set_len(loop_capacity)
            .expect("failed to size backing file");
    }

    let loop_device = LoopDevice::attach(&backing_file);

    let adapter = ExecAdapter::default();
    let target = CreateTarget::Device {
        path: loop_device.path.clone(),
        size: None,
        confirmed: true,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    let name =
        mapping_name::mapping_name(&loop_device.path).expect("failed to derive mapping name");
    let device_node = PathBuf::from(format!("/dev/mapper/{name}"));

    let mountpoint = unlock::run(
        &loop_device.path,
        false,
        false,
        &|_| {},
        &adapter,
        &adapter,
        &adapter,
    )
    .expect("writable unlock::run failed");
    assert_readable_and_writable(&mountpoint);

    let result = close::run(
        &loop_device.path,
        false,
        &|_| {},
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "close::run failed: {result:?}");

    // Identical unlock::run call as the file-backed read-only scenario
    // above — no different flags or behavior branch based on target type.
    let mountpoint = unlock::run(
        &loop_device.path,
        true,
        false,
        &|_| {},
        &adapter,
        &adapter,
        &adapter,
    )
    .expect("read-only unlock::run failed");
    let cleanup =
        UnlockCleanup::new(mountpoint.clone(), name).with_loop_device(loop_device.path.clone());

    assert_actually_mounted(&device_node, &mountpoint);
    assert_owned_by_invoking_user(&mountpoint);

    let write_result = std::fs::write(mountpoint.join("volume-fido2-write-attempt.txt"), b"nope");
    assert!(
        write_result.is_err(),
        "a write attempt inside a read-only mount must fail, got {write_result:?}"
    );

    let remount = Command::new("mount")
        .args(["-o", "remount,rw"])
        .arg(&mountpoint)
        .output()
        .expect("failed to run mount -o remount,rw");
    assert!(
        !remount.status.success(),
        "mount -o remount,rw succeeded against a --readonly dm-crypt mapping; expected it to fail"
    );

    cleanup.run();
}

/// Exercises the collision-suffix fallback (AC #2): two file-backed volumes
/// with the *same* basename (`collision.img`) in different scratch
/// directories derive the same `volume_name`, so the second `unlock::run` must
/// land at a distinct, suffixed mount point rather than failing or
/// colliding with the first.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a real FIDO2
/// security key present, ready to be touched and to enter its PIN when
/// prompted (once per `create`/`unlock` call — 4 touches total).
#[test]
#[ignore]
fn unlock_falls_back_to_a_suffixed_mount_point_on_a_basename_collision() {
    // Guard against flakiness from a prior interrupted run: `UnlockCleanup`'s
    // Drop-based fallback doesn't fire on a hard-killed test process, so a
    // stale `collision`/`collision-<suffix>` mount point could otherwise
    // pre-claim the plain name this test asserts on. Best-effort unmount +
    // remove any such leftovers before starting.
    let username = id_output("-un");
    let media_base = PathBuf::from(format!("/run/media/{username}"));
    if let Ok(entries) = std::fs::read_dir(&media_base) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == "collision" || name.starts_with("collision-") {
                let path = entry.path();
                let _ = Command::new("sudo").arg("umount").arg(&path).output();
                let _ = std::fs::remove_dir(&path);
            }
        }
    }

    let dir_a = std::env::temp_dir().join("volume-fido2-hardware-test-collision-a");
    let dir_b = std::env::temp_dir().join("volume-fido2-hardware-test-collision-b");
    let _ = std::fs::remove_dir_all(&dir_a);
    let _ = std::fs::remove_dir_all(&dir_b);
    std::fs::create_dir_all(&dir_a).expect("failed to create scratch dir a");
    std::fs::create_dir_all(&dir_b).expect("failed to create scratch dir b");

    // Same basename in two different directories -> the same derived
    // volume_name, forcing the fallback path.
    let path_a = dir_a.join("collision.img");
    let path_b = dir_b.join("collision.img");

    let adapter = ExecAdapter::default();

    for path in [&path_a, &path_b] {
        let target = CreateTarget::File {
            path: path.clone(),
            size: 64 * 1024 * 1024,
        };
        let result = create::run(
            target,
            Filesystem::Ext4,
            false,
            Fido2DeviceSelection::Interactive,
            &no_progress,
            &adapter,
            &adapter,
            &adapter,
        );
        assert!(
            result.is_ok(),
            "create::run failed for {}: {result:?}",
            path.display()
        );
    }

    let mountpoint_a = unlock::run(&path_a, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("unlock::run failed for a");
    let name_a = mapping_name::mapping_name(&path_a).expect("failed to derive mapping name a");
    let device_node_a = PathBuf::from(format!("/dev/mapper/{name_a}"));
    let cleanup_a = UnlockCleanup::new(mountpoint_a.clone(), name_a);

    let mountpoint_b = unlock::run(&path_b, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("unlock::run failed for b");
    let name_b = mapping_name::mapping_name(&path_b).expect("failed to derive mapping name b");
    let device_node_b = PathBuf::from(format!("/dev/mapper/{name_b}"));
    let cleanup_b = UnlockCleanup::new(mountpoint_b.clone(), name_b);

    assert_actually_mounted(&device_node_a, &mountpoint_a);
    assert_actually_mounted(&device_node_b, &mountpoint_b);

    assert_eq!(
        mountpoint_a.parent(),
        mountpoint_b.parent(),
        "both mount points should share the same /run/media/<username> base directory"
    );
    assert_ne!(
        mountpoint_a, mountpoint_b,
        "both volumes share the basename \"collision\" and must land at different mount points via the collision-suffix fallback"
    );
    assert_eq!(
        mountpoint_a.file_name().and_then(|n| n.to_str()),
        Some("collision"),
        "the first volume to claim the basename should get the plain, unsuffixed name"
    );
    let basename_b = mountpoint_b
        .file_name()
        .expect("mountpoint_b has no basename")
        .to_string_lossy()
        .into_owned();
    assert!(
        basename_b.starts_with("collision-"),
        "expected the second volume's mount point to fall back to a \"collision-<suffix>\" name, got {basename_b:?}"
    );

    cleanup_b.run();
    cleanup_a.run();
}

/// Blocks on stdin until the tester presses Enter, after printing `prompt` —
/// used only to pace a manual physical-key swap; never reads or echoes
/// anything secret (AD-3 governs PIN/touch material, not this plain
/// orchestration step).
fn pause(prompt: &str) {
    println!("{prompt}");
    println!("Press Enter once ready.");
    let mut input = String::new();
    let _ = std::io::stdin().read_line(&mut input);
}

/// End-to-end verification that `enroll::run` adds a genuinely independent
/// second key rather than corrupting the primary's own metadata (Story 2.1,
/// AC #1/#2/#3) — the concrete regression test for Task 1's before/after
/// token-diffing fix: a naive "pick the first `systemd-fido2` token"
/// implementation would silently overwrite the primary key's
/// `key_label`/`created_at` instead of writing the new token.
///
/// AC #5 (raw device vs. loop-backed file parity) needs no separate
/// device-backed variant here: `enroll::run`'s own code (confirmed by
/// inspection — see `domain::workflows::enroll`) has no target-type branch
/// to have gotten wrong in the first place, the same reasoning already
/// covered by the device-backed `unlock` scenarios above.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and TWO distinct
/// physical FIDO2 security keys, each pluggable independently. Touch the
/// PRIMARY key when `create::run` prompts. When `enroll::run` prompts: plug
/// in *both* keys simultaneously and keep them plugged in — the interactive
/// device-selection flow waits until both are enumerated, then lists them by
/// index and asks "Which is your EXISTING key?" followed by "Which is your
/// NEW key?"; answer with the primary's and the backup's numbers
/// respectively. `systemd-cryptenroll` then runs with both devices attached,
/// prompting for the primary's touch/PIN to authorize, then the new key's
/// touch to complete enrollment.
#[test]
#[ignore]
fn enroll_adds_an_independent_second_key_without_corrupting_the_primary() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-enroll");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("volume.img");

    let adapter = ExecAdapter::default();
    let target = CreateTarget::File {
        path: path.clone(),
        size: 64 * 1024 * 1024,
    };

    println!("Creating volume — touch the PRIMARY key when prompted.");
    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    println!(
        "Enrolling a second key — touch the PRIMARY key first to authorize, \
         then touch the NEW (second) key."
    );
    let result = enroll::run(
        &path,
        "backup".to_string(),
        Fido2DeviceSelection::Interactive,
        false,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "enroll::run failed: {result:?}");

    // AD-5's ground truth: exactly two live keyslots now exist, each backed
    // by a systemd-fido2 token.
    let keyslots = adapter
        .list_fido2_keyslots(&path)
        .expect("list_fido2_keyslots failed");
    assert_eq!(
        keyslots.len(),
        2,
        "expected exactly two live FIDO2 keyslots after enrolling a second key, got {keyslots:?}"
    );

    // Confirm the primary's original label survived untouched and the newly
    // enrolled key's label is present — not swapped or overwritten — the
    // concrete regression check for Task 1's before/after token-diffing fix.
    // Deliberately `--dump-json-metadata`, not plain `luksDump`: cryptsetup's
    // human-readable dump only renders its own known fields for external
    // token types and never surfaces our custom `key_label`/`filesystem`/
    // `created_at` additions, so a plain-dump substring check can never find
    // them regardless of whether the write actually succeeded.
    let dump = Command::new("cryptsetup")
        .arg("luksDump")
        .arg("--dump-json-metadata")
        .arg(&path)
        .output()
        .expect("failed to run cryptsetup luksDump");
    assert!(dump.status.success(), "cryptsetup luksDump failed");
    let dump_text = String::from_utf8_lossy(&dump.stdout);
    assert!(
        dump_text.contains("primary"),
        "expected the primary key's original \"primary\" label to survive enrollment, got:\n{dump_text}"
    );
    assert!(
        dump_text.contains("backup"),
        "expected the newly enrolled key's \"backup\" label to be present, got:\n{dump_text}"
    );

    // Both keys must independently unlock the volume (AC #2). `unlock` relies
    // entirely on cryptsetup's own automatic FIDO2 token-matching (no
    // explicit device flag, by design — see the story's "Architect
    // consultation resolved" note) — it tries every enrolled token against
    // whatever's currently plugged in and succeeds on the first match. With
    // *both* physical keys left plugged in, that means the second attempt
    // below would silently re-prove the same key as the first and never
    // actually exercise the other one. Forcing only one candidate to be
    // physically present per attempt is the only way to prove independence,
    // hence the pauses instructing the tester to swap keys by hand.
    pause("Unplug the SECOND (backup) key now, leaving only the PRIMARY key plugged in.");
    println!("Unlocking with the PRIMARY key — touch it when prompted.");
    let mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("unlock::run with the primary key failed");
    let name = mapping_name::mapping_name(&path).expect("failed to derive mapping name");
    UnlockCleanup::new(mountpoint, name.clone()).run();

    pause("Now unplug the PRIMARY key and plug in ONLY the SECOND (backup) key.");
    println!("Unlocking with the SECOND (backup) key — touch it when prompted.");
    let mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("unlock::run with the second key failed");
    UnlockCleanup::new(mountpoint, name).run();
}

/// End-to-end verification that `revoke::run` removes exactly the targeted
/// key's keyslot without disturbing any other enrolled key (Story 2.2, AC
/// #1) — the concrete regression check for Task 1/2's label-to-keyslot
/// resolution: a bug there could revoke the wrong keyslot instead of the one
/// named by `--label`.
///
/// No separate device-backed variant needed (AC #5) — `revoke::run` has no
/// target-type branch to test around, same reasoning as `enroll`'s own AC #5
/// (confirmed by inspection of `domain::workflows::revoke`).
///
/// Manual-only (AD-7, `make test-hardware`): requires root and TWO distinct
/// physical FIDO2 security keys. Touch the PRIMARY key when `create::run`
/// prompts, then both keys simultaneously when `enroll::run` prompts (same
/// device-selection flow `enroll_adds_an_independent_second_key_without_corrupting_the_primary`
/// documents above).
#[test]
#[ignore]
fn revoke_removes_a_key_without_affecting_others() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-revoke");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("volume.img");

    let adapter = ExecAdapter::default();
    let target = CreateTarget::File {
        path: path.clone(),
        size: 64 * 1024 * 1024,
    };

    println!("Creating volume — touch the PRIMARY key when prompted.");
    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    println!(
        "Enrolling a second key — touch the PRIMARY key first to authorize, \
         then touch the NEW (second) key."
    );
    let result = enroll::run(
        &path,
        "backup".to_string(),
        Fido2DeviceSelection::Interactive,
        false,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "enroll::run failed: {result:?}");

    println!("Revoking the PRIMARY key by label.");
    let result = revoke::run(&path, "primary", &adapter, &adapter, &adapter);
    assert!(result.is_ok(), "revoke::run failed: {result:?}");

    // AD-5's ground truth: exactly one live keyslot remains, and it belongs
    // to the surviving "backup" key — not a relabeled/shadowed "primary".
    // Asserting on the parsed `key_label` field (rather than a raw luksDump
    // text search) proves label-to-keyslot resolution through the same
    // parsing path revoke::run itself relies on.
    let keyslots = adapter
        .list_fido2_keyslots(&path)
        .expect("list_fido2_keyslots failed");
    assert_eq!(
        keyslots.len(),
        1,
        "expected exactly one live FIDO2 keyslot after revoking the primary, got {keyslots:?}"
    );
    assert_eq!(
        keyslots[0].key_label, "backup",
        "expected the surviving keyslot to be labeled \"backup\", got {keyslots:?}"
    );

    // The surviving backup key must still unlock the volume (AC #1's "other
    // enrolled keys still do").
    println!("Unlocking with the surviving BACKUP key — touch it when prompted.");
    let mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("unlock::run with the surviving backup key failed");
    let name = mapping_name::mapping_name(&path).expect("failed to derive mapping name");
    UnlockCleanup::new(mountpoint, name).run();
}

/// End-to-end verification that `revoke::run` refuses to remove the last
/// remaining FIDO2 key rather than locking the volume out permanently (Story
/// 2.2, AC #2).
///
/// Manual-only (AD-7, `make test-hardware`): requires root and one physical
/// FIDO2 security key.
#[test]
#[ignore]
fn revoke_aborts_on_the_last_remaining_key() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-revoke-last-key");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("volume.img");

    let adapter = ExecAdapter::default();
    let target = CreateTarget::File {
        path: path.clone(),
        size: 64 * 1024 * 1024,
    };

    println!("Creating volume — touch the key when prompted.");
    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    println!("Attempting to revoke the only enrolled key — expecting a refusal.");
    let result = revoke::run(&path, "primary", &adapter, &adapter, &adapter);
    assert!(
        matches!(
            result,
            Err(hypogaol::domain::errors::DomainError::LastKeyslotGuard)
        ),
        "expected DomainError::LastKeyslotGuard, got {result:?}"
    );

    // The volume must remain unlockable (AC #2's explicit "volume remains
    // unlockable") — the refused revoke must not have touched anything.
    println!("Confirming the volume is still unlockable — touch the key when prompted.");
    let mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("unlock::run failed after a refused revoke — the guard must be a no-op on abort");
    let name = mapping_name::mapping_name(&path).expect("failed to derive mapping name");
    UnlockCleanup::new(mountpoint, name).run();
}

/// End-to-end close verification (Story 3.1, AC #1/#3): create a real
/// file-backed volume, unlock it, then close it via this tool's own
/// `close::run`, and confirm — independently of this tool's own code — both
/// halves of AC #3: the mount point is gone (not merely unmounted, so a
/// repeat unlock reclaims the plain basename rather than falling back to a
/// collision-suffixed name — the Epic 2 retro action item Task 1's `rmdir`
/// resolves) and the dm-crypt mapping device node is gone (the volume
/// requires the FIDO2 key again to unlock). Finishes with a second
/// `unlock::run` on the same path to prove the volume is genuinely
/// re-lockable/re-unlockable, not just superficially torn down.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a real FIDO2
/// security key present, ready to be touched and to enter its PIN when
/// prompted (once for `create`, once for each of the two `unlock::run` calls).
#[test]
#[ignore]
fn close_unmounts_and_relocks_a_file_backed_volume_allowing_a_clean_repeat_unlock() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-close");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("volume.img");

    let adapter = ExecAdapter::default();
    let target = CreateTarget::File {
        path: path.clone(),
        size: 64 * 1024 * 1024,
    };

    println!("Creating volume — touch the key when prompted.");
    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    println!("Unlocking — touch the key when prompted.");
    let mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("first unlock::run failed");
    let name = mapping_name::mapping_name(&path).expect("failed to derive mapping name");
    let device_node = PathBuf::from(format!("/dev/mapper/{name}"));

    assert_actually_mounted(&device_node, &mountpoint);
    assert_eq!(
        mountpoint.file_name().and_then(|n| n.to_str()),
        Some("volume"),
        "the first unlock should claim the plain, unsuffixed basename"
    );

    println!("Closing the volume via close::run.");
    let result = close::run(&path, false, &|_| {}, &adapter, &adapter, &adapter);
    assert!(result.is_ok(), "close::run failed: {result:?}");

    assert!(
        std::fs::metadata(&mountpoint).is_err(),
        "expected the mount-point directory {} to be removed after close, not merely unmounted",
        mountpoint.display()
    );
    assert!(
        !device_node.exists(),
        "expected the dm-crypt mapping {} to be gone after close",
        device_node.display()
    );

    println!(
        "Unlocking again with the same key to confirm close left the volume re-lockable — touch the key when prompted."
    );
    let second_mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("second unlock::run failed");
    assert_eq!(
        second_mountpoint, mountpoint,
        "a repeat unlock after close should reclaim the plain basename freed by close's rmdir, not fall back to a suffixed name"
    );
    assert_actually_mounted(&device_node, &second_mountpoint);

    UnlockCleanup::new(second_mountpoint, name).run();
}

/// Covers AC #5 (identical `close::run` call, no branching on target type)
/// against a real device-backed target instead of a plain file, using the
/// same `LoopDevice` helper as the device-backed create/unlock hardware
/// tests above.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a real FIDO2
/// security key present, ready to be touched and to enter its PIN when
/// prompted (once for `create`, once for `unlock`).
#[test]
#[ignore]
fn close_works_unmodified_against_a_device_backed_volume() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-close-device");
    let backing_file = dir.join("loop-backing.img");

    // Must run before the backing file is deleted/recreated below — see
    // `detach_stale`'s doc comment.
    LoopDevice::detach_stale(&backing_file);

    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");

    let loop_capacity: u64 = 64 * 1024 * 1024;
    {
        let file = std::fs::File::create(&backing_file).expect("failed to create backing file");
        file.set_len(loop_capacity)
            .expect("failed to size backing file");
    }

    let loop_device = LoopDevice::attach(&backing_file);

    let adapter = ExecAdapter::default();
    let target = CreateTarget::Device {
        path: loop_device.path.clone(),
        size: None,
        confirmed: true,
    };

    println!("Creating volume — touch the key when prompted.");
    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    println!("Unlocking — touch the key when prompted.");
    let mountpoint = unlock::run(
        &loop_device.path,
        false,
        false,
        &|_| {},
        &adapter,
        &adapter,
        &adapter,
    )
    .expect("unlock::run failed");
    let name =
        mapping_name::mapping_name(&loop_device.path).expect("failed to derive mapping name");
    let device_node = PathBuf::from(format!("/dev/mapper/{name}"));

    assert_actually_mounted(&device_node, &mountpoint);

    // Identical close::run call as the file-backed scenario above — no
    // different flags or behavior branch based on target type (AC #5).
    println!("Closing the volume via close::run.");
    let result = close::run(
        &loop_device.path,
        false,
        &|_| {},
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "close::run failed: {result:?}");

    assert!(
        std::fs::metadata(&mountpoint).is_err(),
        "expected the mount-point directory {} to be removed after close",
        mountpoint.display()
    );
    assert!(
        !device_node.exists(),
        "expected the dm-crypt mapping {} to be gone after close",
        device_node.display()
    );

    let detach = Command::new("sudo")
        .args(["losetup", "-d"])
        .arg(&loop_device.path)
        .output()
        .expect("failed to run losetup -d");
    assert!(
        detach.status.success(),
        "losetup -d failed: {}",
        String::from_utf8_lossy(&detach.stderr)
    );
}

/// End-to-end emergency-slam verification (Story 4.6, AC #1): create a real
/// file-backed volume, unlock it, then hold its mountpoint busy with a real
/// process that ignores SIGTERM/SIGHUP (`exec`'d after `trap '' TERM HUP`,
/// so only SIGKILL can end it) — forcing `slam::run` through the full
/// three-round escalation instead of clearing on the first signal.
///
/// This is the exact scenario the 2026-07-28 code review found broken: the
/// original `processes_using` silently dropped every real `fuser -m` PID
/// (psmisc appends access-mode letters directly onto each PID with no
/// separating whitespace, e.g. `1234c`, which `u32::parse` rejected
/// outright), so escalation could never fire against a live process. This
/// test only passes if `fuser -m`'s real output is parsed correctly *and*
/// the holder is genuinely killed — not just coincidentally gone.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a real FIDO2
/// security key present, ready to be touched when prompted.
#[test]
#[ignore]
fn slam_escalates_through_signals_to_close_a_volume_with_a_process_holding_it_open() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-slam");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("volume.img");

    let adapter = ExecAdapter::default();
    let target = CreateTarget::File {
        path: path.clone(),
        size: 64 * 1024 * 1024,
    };

    println!("Creating volume — touch the key when prompted.");
    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    println!("Unlocking — touch the key when prompted.");
    let mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("unlock::run failed");
    let name = mapping_name::mapping_name(&path).expect("failed to derive mapping name");
    let device_node = PathBuf::from(format!("/dev/mapper/{name}"));

    assert_actually_mounted(&device_node, &mountpoint);

    // `exec` replaces the shell's own process image with `sleep`, so there is
    // exactly one process (no fork/child ambiguity) and its ignored TERM/HUP
    // disposition (set by `trap`) survives the `exec` — POSIX guarantees
    // SIG_IGN is preserved across exec. Its `cwd` inside the mountpoint is
    // exactly what `fuser -m` reports as access-mode `c`.
    let mut holder = Command::new("sh")
        .args(["-c", "trap '' TERM HUP; exec sleep 30"])
        .current_dir(&mountpoint)
        .spawn()
        .expect("failed to spawn a process to hold the mount open");
    let holder_pid = holder.id();

    // Give the shell a moment to actually exec and chdir before slam looks
    // for holders.
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        matches!(holder.try_wait(), Ok(None)),
        "the holder process (PID {holder_pid}) exited before slam even ran"
    );

    println!(
        "Running slam — should escalate SIGTERM -> SIGHUP -> SIGKILL to clear the busy mount."
    );
    let started = std::time::Instant::now();
    let results = slam::run(&|_| {}, &adapter, &adapter, &adapter).expect("slam::run failed");
    let elapsed = started.elapsed();

    assert_eq!(
        results.len(),
        1,
        "expected exactly one open volume, got {results:?}"
    );
    let (mapper, outcome) = &results[0];
    assert_eq!(mapper.source_path, path);
    assert!(
        outcome.is_ok(),
        "expected slam to close the volume, got {outcome:?}"
    );

    // SIGTERM's round and SIGHUP's round each pause `ESCALATION_PAUSE` (1s)
    // before retrying `umount` — both are ignored by the holder, so at least
    // two full pauses must have elapsed before SIGKILL's round could clear
    // it. A shorter elapsed time means escalation didn't actually happen
    // (e.g. `processes_using` silently returned zero holders again and the
    // mapping was wrongly reported as an immediate failure — except this
    // assertion only runs once `outcome.is_ok()` above already held).
    assert!(
        elapsed >= std::time::Duration::from_secs(2),
        "expected slam to pause through at least two escalation rounds before SIGKILL cleared it, only took {elapsed:?}"
    );

    let exit_status = holder.wait().expect("failed to reap the holder process");
    assert!(
        !exit_status.success(),
        "expected the holder process (PID {holder_pid}) to be killed, not exit cleanly"
    );
    assert_eq!(
        exit_status.signal(),
        Some(9),
        "expected the holder (PID {holder_pid}) to die by SIGKILL specifically, since SIGTERM/SIGHUP were trapped — got {exit_status:?}"
    );

    assert!(
        std::fs::metadata(&mountpoint).is_err(),
        "expected the mount-point directory {} to be removed after slam",
        mountpoint.display()
    );
    assert!(
        !device_node.exists(),
        "expected the dm-crypt mapping {} to be gone after slam",
        device_node.display()
    );
}

/// End-to-end resize verification (Story 3.2, AC #1/#4): create a small
/// file-backed volume, write data and close it, resize it larger via this
/// tool's own `resize::run`, then unlock again and confirm the pre-resize
/// data survived untouched, the previously enrolled key still works, and
/// the grown capacity is actually usable — a write comfortably larger than
/// the original capacity but within the grown one must succeed, proving
/// `growfs` (not just the LUKS mapping) actually grew.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a real FIDO2
/// security key present, ready to be touched and to enter its PIN when
/// prompted (once for `create`, once for `resize`'s own re-authenticating
/// `luks.open`+`luks.resize`, once each for the two `unlock::run` calls).
#[test]
#[ignore]
fn resize_grows_a_file_backed_volume_preserving_data_and_keys() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-resize");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("volume.img");

    let adapter = ExecAdapter::default();
    let initial_size: u64 = 32 * 1024 * 1024;
    let grown_size: u64 = 96 * 1024 * 1024;

    let target = CreateTarget::File {
        path: path.clone(),
        size: initial_size,
    };

    println!("Creating volume — touch the key when prompted.");
    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    println!("Unlocking to write a marker file — touch the key when prompted.");
    let mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("first unlock::run failed");
    assert_readable_and_writable(&mountpoint);
    let name = mapping_name::mapping_name(&path).expect("failed to derive mapping name");

    // A write close to the pre-grow capacity must still be readable back
    // after the resize below — proves growth doesn't corrupt or lose
    // pre-existing data (AC #4).
    let before_contents = vec![0xABu8; 8 * 1024 * 1024];
    std::fs::write(mountpoint.join("before-resize.bin"), &before_contents)
        .expect("failed to write pre-resize file");

    println!("Closing the volume via close::run before resizing.");
    let result = close::run(&path, false, &|_| {}, &adapter, &adapter, &adapter);
    assert!(result.is_ok(), "close::run failed: {result:?}");

    println!("Resizing the volume — touch the key when prompted (re-authenticates the grow).");
    let result = resize::run(
        &path,
        grown_size,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "resize::run failed: {result:?}");

    let backing_len = std::fs::metadata(&path)
        .expect("failed to stat backing file")
        .len();
    assert_eq!(
        backing_len, grown_size,
        "expected the backing file to be grown to {grown_size} bytes, got {backing_len}"
    );

    println!(
        "Unlocking again to confirm data, key, and new capacity — touch the same key when prompted."
    );
    let mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("unlock::run after resize failed — the previously enrolled key must still work");
    let device_node = PathBuf::from(format!("/dev/mapper/{name}"));
    assert_actually_mounted(&device_node, &mountpoint);

    let recovered = std::fs::read(mountpoint.join("before-resize.bin"))
        .expect("failed to read back the pre-resize marker file after growing");
    assert_eq!(
        recovered, before_contents,
        "pre-resize data must survive the grow untouched (AC #4)"
    );

    // A write comfortably larger than the ORIGINAL 32M capacity, but well
    // within the grown 96M one, must now succeed — proves the filesystem
    // itself was actually grown (growfs), not just the LUKS mapping.
    let after_contents = vec![0xCDu8; 48 * 1024 * 1024];
    std::fs::write(mountpoint.join("after-resize.bin"), &after_contents).expect(
        "writing a file larger than the pre-resize capacity failed — filesystem growth didn't take effect",
    );

    UnlockCleanup::new(mountpoint, name).run();
}

/// Covers growing a device-backed volume into headroom left free at create
/// time (Story 1.6's `size` < device capacity feature, combined with this
/// story's resize) — the scenario the Dev Notes' "Open Design Question"
/// two-tier grow-only check exists specifically to support: the raw loop
/// device's own geometry must never change, but the volume's provisioned size
/// must grow from `requested_size` toward (not exceeding) the loop device's
/// own `loop_capacity`.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a real FIDO2
/// security key present, ready to be touched and to enter its PIN when
/// prompted (once for `create`, once for `resize`, once for the final
/// `unlock::run`).
#[test]
#[ignore]
fn resize_grows_a_device_backed_volume_into_its_own_headroom() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-resize-device");
    let backing_file = dir.join("loop-backing.img");

    LoopDevice::detach_stale(&backing_file);

    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");

    let loop_capacity: u64 = 96 * 1024 * 1024;
    let requested_size: u64 = 32 * 1024 * 1024;
    let grown_size: u64 = 64 * 1024 * 1024;

    {
        let file = std::fs::File::create(&backing_file).expect("failed to create backing file");
        file.set_len(loop_capacity)
            .expect("failed to size backing file");
    }

    let loop_device = LoopDevice::attach(&backing_file);

    let adapter = ExecAdapter::default();
    let target = CreateTarget::Device {
        path: loop_device.path.clone(),
        size: Some(requested_size),
        confirmed: true,
    };

    println!("Creating volume with headroom — touch the key when prompted.");
    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    println!("Resizing into the device's headroom — touch the key when prompted.");
    let result = resize::run(
        &loop_device.path,
        grown_size,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "resize::run failed: {result:?}");

    // The raw loop device's own geometry must never change (resize never
    // touches the partition table, AC #2).
    let capacity_output = Command::new("blockdev")
        .arg("--getsize64")
        .arg(&loop_device.path)
        .output()
        .expect("failed to run blockdev --getsize64");
    assert!(
        capacity_output.status.success(),
        "blockdev --getsize64 failed"
    );
    let capacity: u64 = String::from_utf8_lossy(&capacity_output.stdout)
        .trim()
        .parse()
        .expect("failed to parse blockdev --getsize64 output");
    assert_eq!(
        capacity, loop_capacity,
        "resize must never touch the underlying device's own geometry"
    );

    println!("Unlocking to confirm the grown capacity is usable — touch the key when prompted.");
    let mountpoint = unlock::run(
        &loop_device.path,
        false,
        false,
        &|_| {},
        &adapter,
        &adapter,
        &adapter,
    )
    .expect("unlock::run after resize failed");
    let name =
        mapping_name::mapping_name(&loop_device.path).expect("failed to derive mapping name");
    let device_node = PathBuf::from(format!("/dev/mapper/{name}"));
    assert_actually_mounted(&device_node, &mountpoint);

    // A write comfortably larger than the original 32M provisioned size,
    // but within the grown 64M, must succeed.
    let contents = vec![0xEFu8; 48 * 1024 * 1024];
    std::fs::write(mountpoint.join("after-resize.bin"), &contents)
        .expect("writing a file larger than the pre-resize provisioned size failed");

    UnlockCleanup::new(mountpoint, name)
        .with_loop_device(loop_device.path)
        .run();
}

/// The too-small-partition error path (Story 3.2, AC #2): requesting a
/// resize larger than a device-backed volume's raw underlying capacity must
/// be refused clearly, before ever touching the FIDO2 key — this is a
/// zero-`luks.open` tier-1 rejection, so no touch/PIN prompt should appear
/// at all for the `resize::run` call itself.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a real FIDO2
/// security key present, ready to be touched only for `create::run`'s own
/// enrollment.
#[test]
#[ignore]
fn resize_rejects_a_request_exceeding_the_raw_devices_capacity() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-resize-too-small");
    let backing_file = dir.join("loop-backing.img");

    LoopDevice::detach_stale(&backing_file);

    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");

    let loop_capacity: u64 = 64 * 1024 * 1024;
    {
        let file = std::fs::File::create(&backing_file).expect("failed to create backing file");
        file.set_len(loop_capacity)
            .expect("failed to size backing file");
    }

    let loop_device = LoopDevice::attach(&backing_file);

    let adapter = ExecAdapter::default();
    let target = CreateTarget::Device {
        path: loop_device.path.clone(),
        size: None,
        confirmed: true,
    };

    println!("Creating volume — touch the key when prompted.");
    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    println!(
        "Requesting a resize larger than the raw device's capacity — expecting a clean refusal, no key touch needed."
    );
    let result = resize::run(
        &loop_device.path,
        loop_capacity + 32 * 1024 * 1024,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(
        matches!(
            result,
            Err(hypogaol::domain::errors::DomainError::DeviceSizeExceedsCapacity { .. })
        ),
        "expected DomainError::DeviceSizeExceedsCapacity, got {result:?}"
    );

    let detach = Command::new("sudo")
        .args(["losetup", "-d"])
        .arg(&loop_device.path)
        .output()
        .expect("failed to run losetup -d");
    assert!(
        detach.status.success(),
        "losetup -d failed: {}",
        String::from_utf8_lossy(&detach.stderr)
    );
}

/// The grow-only rejection path (Story 3.2, AC #3): requesting a size no
/// larger than the volume's current size must be refused before touching
/// anything, leaving the volume exactly as it was — same backing file size,
/// still unlockable with the same key.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a real FIDO2
/// security key present, ready to be touched for `create::run`'s own
/// enrollment and for the final confirming `unlock::run` (the rejected
/// `resize::run` call itself needs no touch — a zero-`luks.open` tier-1
/// rejection).
#[test]
#[ignore]
fn resize_rejects_a_shrink_request_and_leaves_the_volume_untouched() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-resize-grow-only");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("volume.img");

    let adapter = ExecAdapter::default();
    let initial_size: u64 = 32 * 1024 * 1024;
    let target = CreateTarget::File {
        path: path.clone(),
        size: initial_size,
    };

    println!("Creating volume — touch the key when prompted.");
    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    println!(
        "Requesting a same-size resize (grow-only rejection) — expecting a clean refusal, no key touch needed."
    );
    let result = resize::run(
        &path,
        initial_size,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(
        matches!(
            result,
            Err(hypogaol::domain::errors::DomainError::ResizeMustGrow { .. })
        ),
        "expected DomainError::ResizeMustGrow, got {result:?}"
    );

    let backing_len = std::fs::metadata(&path)
        .expect("failed to stat backing file")
        .len();
    assert_eq!(
        backing_len, initial_size,
        "a rejected resize must not have touched the backing file's size"
    );

    println!(
        "Confirming the volume is still unlockable with the original key — touch it when prompted."
    );
    let mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("unlock::run failed after a refused resize — the rejection must be a no-op");
    let name = mapping_name::mapping_name(&path).expect("failed to derive mapping name");
    UnlockCleanup::new(mountpoint, name).run();
}

/// Concrete proof of Story 4.1, AC #1's "without performing any unlock/open
/// call": create a file-backed volume, then call `info::run` with no preceding
/// `unlock::run`/mount anywhere in the test. If `info::run` accidentally
/// required an open mapping, this would hang waiting for a touch prompt that
/// never comes, or fail outright since nothing was ever mounted.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a real FIDO2
/// security key present, ready to be touched only once, for `create::run`'s
/// own enrollment.
#[test]
#[ignore]
fn info_lists_enrolled_keys_without_unlocking() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-info");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("volume.img");

    let adapter = ExecAdapter::default();
    let target = CreateTarget::File {
        path: path.clone(),
        size: 64 * 1024 * 1024,
    };

    println!("Creating volume — touch the key when prompted.");
    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    println!("Running info — no key touch expected, no unlock/open call made.");
    let result = info::run(&path, &adapter, &adapter, &adapter);
    let keyslots = result.expect("info::run failed");
    assert_eq!(
        keyslots.len(),
        1,
        "expected exactly one enrolled FIDO2 keyslot, got {keyslots:?}"
    );
    assert_eq!(
        keyslots[0].key_label, "primary",
        "expected the enrolled keyslot to be labeled \"primary\", got {keyslots:?}"
    );
}

/// Proves Story 4.1, AC #3's "identical command works unmodified" against a
/// raw device/partition instead of a loop-backed file — same shape as
/// `info_lists_enrolled_keys_without_unlocking` above, just via the
/// `LoopDevice` helper. No branch to test around: `info::run` takes one
/// `path` for both target types, same as `revoke`/`close`.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a real FIDO2
/// security key present, ready to be touched only once, for `create::run`'s
/// own enrollment.
#[test]
#[ignore]
fn info_works_unmodified_against_a_device_backed_volume() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-info-device");
    let backing_file = dir.join("loop-backing.img");

    // Must run before the backing file is deleted/recreated below — see
    // `detach_stale`'s doc comment.
    LoopDevice::detach_stale(&backing_file);

    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");

    let loop_capacity: u64 = 64 * 1024 * 1024;
    {
        let file = std::fs::File::create(&backing_file).expect("failed to create backing file");
        file.set_len(loop_capacity)
            .expect("failed to size backing file");
    }

    let loop_device = LoopDevice::attach(&backing_file);

    let adapter = ExecAdapter::default();
    let target = CreateTarget::Device {
        path: loop_device.path.clone(),
        size: None,
        confirmed: true,
    };

    println!("Creating volume — touch the key when prompted.");
    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    println!("Running info — no key touch expected, no unlock/open call made.");
    let result = info::run(&loop_device.path, &adapter, &adapter, &adapter);
    let keyslots = result.expect("info::run failed");
    assert_eq!(
        keyslots.len(),
        1,
        "expected exactly one enrolled FIDO2 keyslot, got {keyslots:?}"
    );
    assert_eq!(
        keyslots[0].key_label, "primary",
        "expected the enrolled keyslot to be labeled \"primary\", got {keyslots:?}"
    );

    let detach = Command::new("sudo")
        .args(["losetup", "-d"])
        .arg(&loop_device.path)
        .output()
        .expect("failed to run losetup -d");
    assert!(
        detach.status.success(),
        "losetup -d failed: {}",
        String::from_utf8_lossy(&detach.stderr)
    );
}

/// Reads a security token's `key_label`/`fido2-uv-required`/
/// `fido2-clientPin-required` fields directly off the LUKS2 JSON token area —
/// same `--dump-json-metadata` approach `enroll_adds_an_independent_second_key_without_corrupting_the_primary`
/// uses for `key_label`, extended to the two FIDO2 UV fields
/// `systemd-cryptenroll`/`cryptsetup`'s own `libcryptsetup-token-systemd-fido2`
/// plugin writes into every `systemd-fido2` token (confirmed by inspecting
/// that plugin's shared object: it stores exactly these two boolean fields,
/// `fido2-uv-required` and `fido2-clientPin-required`, alongside
/// `fido2-up-required`). Reading these back is a genuine end-to-end
/// assertion of Story 4.3's post-review fix — not a proxy — because they're
/// the real, on-disk credential fields `systemd-fido2`'s unlock-time PAM/
/// cryptsetup token plugin reads to decide whether to prompt for a PIN at
/// all, independent of whatever this process observed at enroll time.
fn dumped_uv_fields_for_label(path: &std::path::Path, key_label: &str) -> (bool, bool) {
    let dump = Command::new("cryptsetup")
        .arg("luksDump")
        .arg("--dump-json-metadata")
        .arg(path)
        .output()
        .expect("failed to run cryptsetup luksDump");
    assert!(dump.status.success(), "cryptsetup luksDump failed");

    let metadata: serde_json::Value =
        serde_json::from_slice(&dump.stdout).expect("failed to parse luksDump JSON output");
    let tokens = metadata
        .get("tokens")
        .and_then(|t| t.as_object())
        .expect("no \"tokens\" object in luksDump JSON output");

    let token = tokens
        .values()
        .find(|token| token.get("key_label").and_then(|v| v.as_str()) == Some(key_label))
        .unwrap_or_else(|| panic!("no token found with key_label {key_label:?} in {tokens:?}"));

    let uv_required = token
        .get("fido2-uv-required")
        .and_then(|v| v.as_bool())
        .expect("token has no boolean \"fido2-uv-required\" field");
    let client_pin_required = token
        .get("fido2-clientPin-required")
        .and_then(|v| v.as_bool())
        .expect("token has no boolean \"fido2-clientPin-required\" field");
    (uv_required, client_pin_required)
}

/// End-to-end verification of Story 4.3's post-review fix (LeReverandNox,
/// 2026-07-27 dogfooding): enrolling with `--user-verification` must not
/// merely ask `systemd-cryptenroll` for UV, it must also disable `clientPin`
/// (`fido2_verification_args` in `src/adapters/exec/mod.rs`) so a UV-capable
/// token proves verification via its own fingerprint sensor rather than a
/// host-typed PIN. Reads both booleans directly off the enrolled token's
/// LUKS2 JSON metadata (`dumped_uv_fields_for_label`) rather than relying on
/// a human watching for a PIN prompt — a real, deterministic, automatable
/// assertion of the actual on-disk credential, not an observation proxy.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a FIDO2
/// security key with a genuine on-device user-verification method (e.g. a
/// fingerprint sensor — a YubiKey Bio or similar). Touch/verify when
/// `create::run` prompts. If your key has no such method, use
/// `enroll_with_user_verification_on_a_non_uv_capable_key_fails_cleanly`
/// below instead — this scenario cannot pass on a PIN-only token, since
/// disabling `clientPin` removes the only UV method such a token has.
#[test]
#[ignore]
fn enroll_with_user_verification_on_a_uv_capable_key_disables_client_pin() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-uv-capable");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("volume.img");

    let adapter = ExecAdapter::default();
    let target = CreateTarget::File {
        path: path.clone(),
        size: 32 * 1024 * 1024,
    };

    println!(
        "Creating volume with --user-verification — verify on-device (fingerprint) when prompted, \
         not a typed PIN."
    );
    let result = create::run(
        target,
        Filesystem::Ext4,
        true,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(
        result.is_ok(),
        "create::run failed: {result:?} — if your key has no on-device UV method (no fingerprint \
         sensor), this failure is expected; use the non-UV-capable test instead"
    );

    let (uv_required, client_pin_required) = dumped_uv_fields_for_label(&path, "primary");
    assert!(
        uv_required,
        "expected fido2-uv-required=true on the enrolled token after --user-verification"
    );
    assert!(
        !client_pin_required,
        "expected fido2-clientPin-required=false — clientPin must be disabled when UV is \
         requested, so verification happens on-device (fingerprint) instead of via a typed PIN"
    );

    println!("Confirming the volume still unlocks — verify on-device (fingerprint) when prompted.");
    let mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("unlock::run failed after UV-required enrollment");
    let name = mapping_name::mapping_name(&path).expect("failed to derive mapping name");
    UnlockCleanup::new(mountpoint, name).run();
}

/// Companion to `enroll_with_user_verification_on_a_uv_capable_key_disables_client_pin`:
/// that test only exercises a single-key bootstrap enrollment. This one
/// proves the second-key path — enrolling a UV-required key while
/// authenticated by an already-enrolled, non-UV key — actually succeeds
/// end-to-end, and that doing so leaves the *existing* key's own
/// `fido2-uv-required`/`fido2-clientPin-required` fields untouched (no
/// cross-contamination between the two tokens' credential metadata).
///
/// Manual-only (AD-7, `make test-hardware`): requires root and TWO distinct
/// physical FIDO2 security keys — the PRIMARY (any key, no UV method
/// required) and the BACKUP (must have a genuine on-device user-verification
/// method, e.g. a fingerprint sensor). Touch the PRIMARY key when
/// `create::run` prompts, touch the PRIMARY key again to authorize the
/// second enrollment, then verify (fingerprint) on the BACKUP key. Follow
/// the swap prompts to confirm both keys independently unlock afterward.
#[test]
#[ignore]
fn enroll_with_user_verification_authenticated_by_an_existing_key_succeeds_and_leaves_it_unchanged()
{
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-uv-mixed-auth");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("volume.img");

    let adapter = ExecAdapter::default();
    let target = CreateTarget::File {
        path: path.clone(),
        size: 32 * 1024 * 1024,
    };

    println!("Creating volume WITHOUT --user-verification — touch the PRIMARY key when prompted.");
    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    println!(
        "Enrolling a BACKUP key WITH --user-verification — touch the PRIMARY key first to \
         authorize, then verify (fingerprint) on the BACKUP key. If the BACKUP key has no \
         on-device UV method, use the non-UV-capable failure test instead."
    );
    let result = enroll::run(
        &path,
        "backup".to_string(),
        Fido2DeviceSelection::Interactive,
        true,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "enroll::run failed: {result:?}");

    let keyslots = adapter
        .list_fido2_keyslots(&path)
        .expect("list_fido2_keyslots failed");
    assert_eq!(
        keyslots.len(),
        2,
        "expected exactly two live FIDO2 keyslots after enrolling the backup key, got {keyslots:?}"
    );

    let (backup_uv_required, backup_client_pin_required) =
        dumped_uv_fields_for_label(&path, "backup");
    assert!(
        backup_uv_required,
        "expected fido2-uv-required=true on the newly enrolled backup token"
    );
    assert!(
        !backup_client_pin_required,
        "expected fido2-clientPin-required=false on the newly enrolled backup token"
    );

    let (primary_uv_required, _) = dumped_uv_fields_for_label(&path, "primary");
    assert!(
        !primary_uv_required,
        "expected the PRIMARY key's own fido2-uv-required to remain false — enrolling the \
         backup key with --user-verification must not alter the primary token's credential \
         metadata"
    );

    pause("Unplug the BACKUP key now, leaving only the PRIMARY key plugged in.");
    println!("Unlocking with the PRIMARY key — touch it when prompted.");
    let mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("unlock::run with the primary key failed");
    let name = mapping_name::mapping_name(&path).expect("failed to derive mapping name");
    UnlockCleanup::new(mountpoint, name.clone()).run();

    pause("Now unplug the PRIMARY key and plug in ONLY the BACKUP key.");
    println!("Unlocking with the BACKUP key — verify on-device (fingerprint) when prompted.");
    let mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("unlock::run with the backup key failed");
    UnlockCleanup::new(mountpoint, name).run();
}

/// The deliberate-failure counterpart to
/// `enroll_with_user_verification_on_a_uv_capable_key_disables_client_pin`:
/// a token with no on-device UV method (a standard, non-biometric security
/// key — the common case) asked to enroll with `--user-verification` must
/// fail enrollment outright, not silently fall back to PIN-based
/// verification. That silent fallback is exactly the bug the post-review fix
/// closed (disabling `clientPin` removes the only UV method such a token
/// has, so `systemd-cryptenroll` has no way left to satisfy "uv" and must
/// refuse). A clean, explicit failure here — leaving the volume exactly as it
/// was before the attempt — is the correct, intended outcome, not a defect.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a FIDO2
/// security key with NO on-device UV method (no fingerprint sensor — most
/// standard security keys qualify). Touch the key for the first (successful,
/// non-UV) enrollment; the second (UV) enrollment attempt is expected to
/// fail before or without a touch prompt completing successfully.
#[test]
#[ignore]
fn enroll_with_user_verification_on_a_non_uv_capable_key_fails_cleanly() {
    let dir = std::env::temp_dir().join("volume-fido2-hardware-test-uv-incapable");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("volume.img");

    let adapter = ExecAdapter::default();
    let target = CreateTarget::File {
        path: path.clone(),
        size: 32 * 1024 * 1024,
    };

    println!("Creating volume WITHOUT --user-verification — touch the key when prompted.");
    let result = create::run(
        target,
        Filesystem::Ext4,
        false,
        Fido2DeviceSelection::Interactive,
        &no_progress,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    println!(
        "Attempting to enroll a second key WITH --user-verification on a non-UV-capable key — \
         touch the PRIMARY key to authorize if prompted; this enrollment is expected to fail."
    );
    let result = enroll::run(
        &path,
        "backup".to_string(),
        Fido2DeviceSelection::Interactive,
        true,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(
        result.is_err(),
        "expected enroll::run to fail requesting --user-verification on a non-UV-capable key, got \
         Ok(()) — if your key DOES have an on-device UV method (fingerprint), use the UV-capable \
         test instead"
    );

    // The failed attempt must leave the volume exactly as it was — no partial
    // keyslot, same rollback discipline `enroll_fido2_key`'s own failure path
    // (rolling back a metadata-write failure) already guarantees.
    let keyslots = adapter
        .list_fido2_keyslots(&path)
        .expect("list_fido2_keyslots failed");
    assert_eq!(
        keyslots.len(),
        1,
        "a failed UV enrollment must not leave a partial keyslot behind, got {keyslots:?}"
    );
    assert_eq!(
        keyslots[0].key_label, "primary",
        "the original primary key's label must survive an untouched, got {keyslots:?}"
    );

    println!("Confirming the primary key still unlocks the volume — touch it when prompted.");
    let mountpoint = unlock::run(&path, false, false, &|_| {}, &adapter, &adapter, &adapter)
        .expect("unlock::run with the primary key failed after a rejected UV enrollment");
    let name = mapping_name::mapping_name(&path).expect("failed to derive mapping name");
    UnlockCleanup::new(mountpoint, name).run();
}
