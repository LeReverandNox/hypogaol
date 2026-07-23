use std::path::PathBuf;
use std::process::Command;

use tomb_fido2::adapters::exec::ExecAdapter;
use tomb_fido2::domain::mapping_name;
use tomb_fido2::domain::types::{CreateTarget, Filesystem};
use tomb_fido2::domain::workflows::{create, unlock};

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

    let result = create::run(target, Filesystem::Ext4, &adapter, &adapter, &adapter);
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

    let result = create::run(target, Filesystem::Ext4, &adapter, &adapter, &adapter);
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

    let result = create::run(target, Filesystem::Ext4, &adapter, &adapter, &adapter);
    assert!(result.is_ok(), "create::run failed: {result:?}");

    let mountpoint = unlock::run(&path, &adapter, &adapter, &adapter).expect("unlock::run failed");

    let name = mapping_name::mapping_name(&path).expect("failed to derive mapping name");
    let device_node = PathBuf::from(format!("/dev/mapper/{name}"));
    let cleanup = UnlockCleanup::new(mountpoint.clone(), name);

    assert_actually_mounted(&device_node, &mountpoint);
    assert_readable_and_writable(&mountpoint);

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

    let result = create::run(target, Filesystem::Ext4, &adapter, &adapter, &adapter);
    assert!(result.is_ok(), "create::run failed: {result:?}");

    // Identical unlock::run call as the file-backed scenario above — no
    // different flags or behavior branch based on target type (AC #2).
    let mountpoint =
        unlock::run(&loop_device.path, &adapter, &adapter, &adapter).expect("unlock::run failed");

    let name =
        mapping_name::mapping_name(&loop_device.path).expect("failed to derive mapping name");
    let device_node = PathBuf::from(format!("/dev/mapper/{name}"));
    let cleanup = UnlockCleanup::new(mountpoint.clone(), name).with_loop_device(loop_device.path);

    assert_actually_mounted(&device_node, &mountpoint);
    assert_readable_and_writable(&mountpoint);

    cleanup.run();
}
