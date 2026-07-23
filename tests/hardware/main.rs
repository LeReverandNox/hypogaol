use std::path::PathBuf;
use std::process::Command;

use tomb_fido2::adapters::exec::ExecAdapter;
use tomb_fido2::domain::types::{CreateTarget, Filesystem};
use tomb_fido2::domain::workflows::create;

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
/// loop device left over from an interrupted previous run is cleaned up at
/// the top of `attach` so repeated runs don't accumulate stale devices.
struct LoopDevice {
    path: PathBuf,
}

impl LoopDevice {
    fn attach(backing_file: &std::path::Path) -> Self {
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
         sudo cryptsetup open {} tomb-fido2-hardware-test-device\n  \
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
