use std::process::Command;

use tomb_fido2::adapters::exec::ExecAdapter;
use tomb_fido2::domain::types::{CreateTarget, Filesystem};
use tomb_fido2::domain::workflows::create;

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
