use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

use tomb_fido2::adapters::exec::ExecAdapter;
use tomb_fido2::domain::types::{CreateTarget, Filesystem};
use tomb_fido2::domain::workflows::create;

/// Attaches a genuine `/dev/loopN` block device backed by a disposable file —
/// `cryptsetup`/`blockdev` treat it identically to physical storage, so this
/// exercises the real Device code path without requiring (and risking data
/// loss on) a spare physical disk/partition. Detaches on drop regardless of
/// how the test exits, including on panic.
struct LoopDevice {
    path: PathBuf,
}

impl LoopDevice {
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

impl Drop for LoopDevice {
    fn drop(&mut self) {
        let _ = Command::new("sudo")
            .arg("losetup")
            .arg("-d")
            .arg(&self.path)
            .output();
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
         sudo cryptsetup open {} tomb-fido2-hardware-test\n  \
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
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
    let backing_file = dir.join("loop-backing.img");

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

    // AC #2's headroom claim: the LUKS2 payload segment itself must be
    // smaller than the full device capacity, confirming free space was left
    // for a later resize/grow.
    let json_dump = Command::new("cryptsetup")
        .arg("luksDump")
        .arg("--dump-json-metadata")
        .arg(&loop_device.path)
        .output()
        .expect("failed to run cryptsetup luksDump --dump-json-metadata");
    assert!(
        json_dump.status.success(),
        "cryptsetup luksDump --dump-json-metadata failed"
    );
    let metadata: Value =
        serde_json::from_slice(&json_dump.stdout).expect("failed to parse luksDump JSON");
    let segment_size: u64 = metadata["segments"]["0"]["size"]
        .as_str()
        .expect("segments.0.size missing or not a string in luksDump JSON")
        .parse()
        .expect(
            "segments.0.size was not a plain byte count (got \"dynamic\"? \
             the resize --device-size step may not have run/persisted)",
        );
    assert!(
        segment_size < capacity,
        "expected the LUKS2 payload ({segment_size} bytes) to be smaller than \
         the full loop device capacity ({capacity} bytes), leaving headroom \
         for a later resize"
    );

    // The rest of AC #1's break-glass clause (inherited from Story 1.5) —
    // actually running `cryptsetup open` (touch + PIN) and mounting the
    // filesystem — needs a live interactive prompt this test can't
    // automate; finish verifying it by hand, then record the result in the
    // story's Completion Notes:
    println!(
        "Device-backed tomb created at {} (loop device backed by {}).\n\
         LUKS2 payload: {segment_size} bytes of {capacity} bytes total.\n\
         To finish verifying by hand:\n  \
         sudo cryptsetup open {} tomb-fido2-hardware-test-device\n  \
         sudo mount /dev/mapper/tomb-fido2-hardware-test-device <mountpoint>\n  \
         ls <mountpoint>\n  \
         sudo umount <mountpoint> && sudo cryptsetup close tomb-fido2-hardware-test-device",
        loop_device.path.display(),
        backing_file.display(),
        loop_device.path.display(),
    );
}
