use std::path::PathBuf;
use std::process::Command;

use tomb_fido2::adapters::exec::ExecAdapter;
use tomb_fido2::domain::mapping_name;
use tomb_fido2::domain::types::{CreateTarget, Filesystem};
use tomb_fido2::domain::workflows::{create, enroll, unlock};
use tomb_fido2::ports::fido2_backend::Fido2DeviceSelection;
use tomb_fido2::ports::luks_backend::LuksBackend;

/// Attaches a genuine `/dev/loopN` block device backed by a disposable file —
/// `cryptsetup`/`blockdev` treat it identically to physical storage, so this
/// exercises the real Device code path without requiring (and risking data
/// loss on) a spare physical disk/partition.
///
/// Deliberately does *not* auto-detach on drop: the whole point of this
/// test's printed instructions is to let a human inspect the still-open
/// tomb by hand afterward (mount, `dumpe2fs`, etc.), and an unconditional
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
/// file-backed tomb via this tool's own `domain::workflows::create`, then
/// independently confirm the result is recognized by bare
/// `cryptsetup`/`fido2-token` (not this tool's own unlock, which doesn't
/// exist until Story 1.7).
///
/// Manual-only (AD-7, `make test-hardware`): requires root (LUKS2
/// `luksOpen`/dm-crypt mapping, mount) and a real FIDO2 security key
/// present, ready to be touched and to enter its PIN when prompted.
#[test]
#[ignore]
fn create_a_file_backed_tomb_is_independently_unlockable_via_bare_cryptsetup() {
    let dir = std::env::temp_dir().join("tomb-fido2-hardware-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("tomb.img");

    let adapter = ExecAdapter::default();
    let target = CreateTarget::File {
        path: path.clone(),
        size: 64 * 1024 * 1024,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
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
        "Tomb created at {}. To finish verifying AC #1 by hand:\n  \
         sudo cryptsetup open --token-only {} tomb-fido2-hardware-test\n  \
         sudo mount /dev/mapper/tomb-fido2-hardware-test <mountpoint>\n  \
         ls <mountpoint>\n  \
         sudo umount <mountpoint> && sudo cryptsetup close tomb-fido2-hardware-test",
        path.display(),
        path.display(),
    );
}

/// Device-backed create's headroom claim (Story 1.6, AC #2): create a real
/// tomb on a loop device using a size smaller than the loop device's own
/// capacity, then independently confirm (bare `cryptsetup`/`blockdev`) that
/// the LUKS2 payload is smaller than the full device, leaving free space for
/// a later resize/grow (Story 3.2).
///
/// Manual-only (AD-7, `make test-hardware`): requires root (LUKS2
/// `luksOpen`/dm-crypt mapping, `losetup`) and a real FIDO2 security key
/// present, ready to be touched and to enter its PIN when prompted.
#[test]
#[ignore]
fn create_a_device_backed_tomb_leaves_headroom_for_a_later_resize() {
    let dir = std::env::temp_dir().join("tomb-fido2-hardware-test-device");
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
        Fido2DeviceSelection::Interactive,
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
        "Device-backed tomb created at {} (loop device backed by {}).\n\
         Requested {requested_size} bytes of {capacity} bytes total capacity.\n\
         To finish verifying AC #2's headroom claim by hand:\n  \
         sudo cryptsetup open --token-only {} tomb-fido2-hardware-test-device\n  \
         sudo dumpe2fs -h /dev/mapper/tomb-fido2-hardware-test-device | grep -E 'Block count|Block size'\n  \
         # confirm block_count * block_size is close to {requested_size} bytes, not {capacity}\n  \
         sudo cryptsetup close tomb-fido2-hardware-test-device\n  \
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
    let marker = mountpoint.join("tomb-fido2-marker.txt");
    std::fs::write(&marker, b"tomb-fido2 hardware test").expect("failed to write marker file");
    let contents = std::fs::read_to_string(&marker).expect("failed to read marker file back");
    assert_eq!(contents, "tomb-fido2 hardware test");
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
/// file-backed tomb via this tool's own `create::run`, then unlock and mount
/// it via `unlock::run`, and confirm — independently of this tool's own
/// code, via `findmnt` — that the returned mount point is actually mounted
/// and its filesystem is readable/writable.
///
/// Manual-only (AD-7, `make test-hardware`): requires root and a real FIDO2
/// security key present, ready to be touched and to enter its PIN when
/// prompted (once for `create`'s enrollment, once more for `unlock`'s open).
#[test]
#[ignore]
fn unlock_mounts_a_file_backed_tomb_with_a_readable_writable_filesystem() {
    let dir = std::env::temp_dir().join("tomb-fido2-hardware-test-unlock");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("tomb.img");

    let adapter = ExecAdapter::default();
    let target = CreateTarget::File {
        path: path.clone(),
        size: 64 * 1024 * 1024,
    };

    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    let mountpoint = unlock::run(&path, &adapter, &adapter, &adapter).expect("unlock::run failed");

    let name = mapping_name::mapping_name(&path).expect("failed to derive mapping name");
    let device_node = PathBuf::from(format!("/dev/mapper/{name}"));
    let cleanup = UnlockCleanup::new(mountpoint.clone(), name);

    assert_actually_mounted(&device_node, &mountpoint);
    assert_readable_and_writable(&mountpoint);
    assert_owned_by_invoking_user(&mountpoint);
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
fn unlock_works_unmodified_against_a_device_backed_tomb() {
    let dir = std::env::temp_dir().join("tomb-fido2-hardware-test-unlock-device");
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
        Fido2DeviceSelection::Interactive,
        &adapter,
        &adapter,
        &adapter,
    );
    assert!(result.is_ok(), "create::run failed: {result:?}");

    // Identical unlock::run call as the file-backed scenario above — no
    // different flags or behavior branch based on target type (AC #2).
    let mountpoint =
        unlock::run(&loop_device.path, &adapter, &adapter, &adapter).expect("unlock::run failed");

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

/// Exercises the collision-suffix fallback (AC #2): two file-backed tombs
/// with the *same* basename (`collision.img`) in different scratch
/// directories derive the same `tomb_name`, so the second `unlock::run` must
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

    let dir_a = std::env::temp_dir().join("tomb-fido2-hardware-test-collision-a");
    let dir_b = std::env::temp_dir().join("tomb-fido2-hardware-test-collision-b");
    let _ = std::fs::remove_dir_all(&dir_a);
    let _ = std::fs::remove_dir_all(&dir_b);
    std::fs::create_dir_all(&dir_a).expect("failed to create scratch dir a");
    std::fs::create_dir_all(&dir_b).expect("failed to create scratch dir b");

    // Same basename in two different directories -> the same derived
    // tomb_name, forcing the fallback path.
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
            Fido2DeviceSelection::Interactive,
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

    let mountpoint_a =
        unlock::run(&path_a, &adapter, &adapter, &adapter).expect("unlock::run failed for a");
    let name_a = mapping_name::mapping_name(&path_a).expect("failed to derive mapping name a");
    let device_node_a = PathBuf::from(format!("/dev/mapper/{name_a}"));
    let cleanup_a = UnlockCleanup::new(mountpoint_a.clone(), name_a);

    let mountpoint_b =
        unlock::run(&path_b, &adapter, &adapter, &adapter).expect("unlock::run failed for b");
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
        "both tombs share the basename \"collision\" and must land at different mount points via the collision-suffix fallback"
    );
    assert_eq!(
        mountpoint_a.file_name().and_then(|n| n.to_str()),
        Some("collision"),
        "the first tomb to claim the basename should get the plain, unsuffixed name"
    );
    let basename_b = mountpoint_b
        .file_name()
        .expect("mountpoint_b has no basename")
        .to_string_lossy()
        .into_owned();
    assert!(
        basename_b.starts_with("collision-"),
        "expected the second tomb's mount point to fall back to a \"collision-<suffix>\" name, got {basename_b:?}"
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
/// PRIMARY key when `create::run` prompts. When `enroll::run` prompts:
/// first make sure *only* the PRIMARY key is plugged in and press Enter
/// (this identifies its hidraw device without touching it); then also plug
/// in the SECOND (new) key — keep the primary plugged in too — and press
/// Enter again; `systemd-cryptenroll` then runs with both devices attached,
/// prompting for the primary's touch/PIN to authorize, then the new key's
/// touch to complete enrollment.
#[test]
#[ignore]
fn enroll_adds_an_independent_second_key_without_corrupting_the_primary() {
    let dir = std::env::temp_dir().join("tomb-fido2-hardware-test-enroll");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let path = dir.join("tomb.img");

    let adapter = ExecAdapter::default();
    let target = CreateTarget::File {
        path: path.clone(),
        size: 64 * 1024 * 1024,
    };

    println!("Creating tomb — touch the PRIMARY key when prompted.");
    let result = create::run(
        target,
        Filesystem::Ext4,
        Fido2DeviceSelection::Interactive,
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

    // Both keys must independently unlock the tomb (AC #2). `unlock` relies
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
    let mountpoint = unlock::run(&path, &adapter, &adapter, &adapter)
        .expect("unlock::run with the primary key failed");
    let name = mapping_name::mapping_name(&path).expect("failed to derive mapping name");
    UnlockCleanup::new(mountpoint, name.clone()).run();

    pause("Now unplug the PRIMARY key and plug in ONLY the SECOND (backup) key.");
    println!("Unlocking with the SECOND (backup) key — touch it when prompted.");
    let mountpoint = unlock::run(&path, &adapter, &adapter, &adapter)
        .expect("unlock::run with the second key failed");
    UnlockCleanup::new(mountpoint, name).run();
}
