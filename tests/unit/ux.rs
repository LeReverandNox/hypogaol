use std::path::PathBuf;

use tomb_fido2::cli::ux::translate;
use tomb_fido2::domain::errors::DomainError;

const JARGON_MARKERS: [&str; 4] = [
    "cryptsetup",
    "AdapterFailure",
    "luksFormat",
    "systemd-cryptenroll",
];

fn assert_no_jargon(message: &str) {
    assert!(!message.is_empty());
    for marker in JARGON_MARKERS {
        assert!(
            !message.contains(marker),
            "message {message:?} unexpectedly contains jargon marker {marker:?}"
        );
    }
}

#[test]
fn translates_destination_exists() {
    let err = DomainError::DestinationExists(PathBuf::from("/tmp/my-tomb"));
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(message.contains("/tmp/my-tomb"));
}

#[test]
fn translates_device_already_formatted() {
    let err = DomainError::DeviceAlreadyFormatted(PathBuf::from("/dev/sdb1"));
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(message.contains("/dev/sdb1"));
}

#[test]
fn translates_device_confirmation_required() {
    let err = DomainError::DeviceConfirmationRequired;
    let message = translate(&err);
    assert_no_jargon(&message);
}

#[test]
fn translates_device_size_exceeds_capacity() {
    let err = DomainError::DeviceSizeExceedsCapacity {
        path: PathBuf::from("/dev/sdb1"),
        requested: 2_000_000_000,
        capacity: 1_000_000_000,
    };
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(message.contains("2000000000"));
    assert!(message.contains("1000000000"));
}

#[test]
fn translates_device_too_small() {
    let err = DomainError::DeviceTooSmall {
        path: PathBuf::from("/dev/sdb1"),
        size: 1024,
    };
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(message.contains("1024"));
    assert!(message.contains("/dev/sdb1"));
    // `size` is the device's own capacity (not necessarily a user-typed
    // value) whenever `--size` was omitted on a device target — the wording
    // must not imply the user requested this exact number.
    assert!(!message.contains("requested"));
}

#[test]
fn translates_preflight_failed() {
    let err = DomainError::PreflightFailed(vec![
        "cryptsetup binary not found on PATH".to_string(),
        "kernel hidraw support not found (/sys/class/hidraw missing)".to_string(),
    ]);
    let message = translate(&err);
    assert!(!message.is_empty());
    // PreflightFailed's per-item strings are passed through as-is (they
    // already name real, installable/checkable things) — only the framing
    // sentence is plain language, so the jargon-marker check doesn't apply
    // to the whole message here.
    assert!(message.contains("cryptsetup binary not found on PATH"));
    assert!(message.contains("kernel hidraw support not found (/sys/class/hidraw missing)"));
}

#[test]
fn translates_last_keyslot_guard() {
    let err = DomainError::LastKeyslotGuard;
    let message = translate(&err);
    assert_no_jargon(&message);
}

#[test]
fn translates_adapter_failure_cmd_debug_dump() {
    let err = DomainError::AdapterFailure(
        r#""cryptsetup" "luksFormat" "--type" "luks2" "--batch-mode" "--key-file" "-" "/tmp/foo" failed: some stderr"#
            .to_string(),
    );
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("--batch-mode"));
    assert!(!message.contains("--key-file"));
}

#[test]
fn translates_adapter_failure_cryptsetup_open_failure() {
    let err = DomainError::AdapterFailure(
        "cryptsetup open --token-only failed for /tmp/foo as vault-abc123".to_string(),
    );
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("--token-only"));
    assert!(!message.contains("vault-abc123"));
}

#[test]
fn translates_adapter_failure_enrollment_failure() {
    let err =
        DomainError::AdapterFailure("systemd-cryptenroll --fido2-device=auto failed".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
}

#[test]
fn translates_adapter_failure_token_export_failure_as_enrollment_not_cryptsetup() {
    // This message contains the literal substring "cryptsetup" but must be
    // classified as an enrollment failure, not a cryptsetup-subprocess
    // failure — proves the enrollment-marker check runs before the broader
    // "cryptsetup" substring check.
    let err =
        DomainError::AdapterFailure("cryptsetup token export failed: some stderr".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(message.contains("Enrolling your security key"));
}

#[test]
fn translates_adapter_failure_token_import_debug_dump_as_enrollment() {
    // The real `write_fido2_token_metadata` token-import call goes through
    // `run_piping_stdin`'s `"{cmd:?} failed: ..."` format, whose Debug-quoted
    // output renders as `"token" "import"` (separate quoted tokens, never
    // the literal substring "token import") — this must still classify as
    // enrollment, not fall through to the generic cryptsetup bucket.
    let err = DomainError::AdapterFailure(
        r#""cryptsetup" "token" "import" "--token-id" "5" "--token-replace" "/tmp/foo" failed: some stderr"#
            .to_string(),
    );
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(message.contains("Enrolling your security key"));
}

#[test]
fn translates_adapter_failure_temp_key_file_creation_as_enrollment() {
    // `TempKeyFile::create`'s failure happens inside `enroll_fido2_key`'s own
    // body — it must not be misdiagnosed as a device/file sizing problem just
    // because its text contains "failed to create".
    let err = DomainError::AdapterFailure(
        "failed to create temporary key file /dev/shm/.tomb-fido2-bootstrap-abc123: Permission denied (os error 13)"
            .to_string(),
    );
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(message.contains("Enrolling your security key"));
}

#[test]
fn translates_adapter_failure_missing_transient_passphrase_as_enrollment() {
    let err = DomainError::AdapterFailure(
        "no transient bootstrap passphrase available to authenticate FIDO2 enrollment".to_string(),
    );
    let message = translate(&err);
    assert!(!message.is_empty());
    assert!(message.contains("Enrolling your security key"));
    assert!(!message.contains("FIDO2"));
}

#[test]
fn translates_adapter_failure_mount_failure() {
    let err = DomainError::AdapterFailure("mount failed: some real stderr".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(message.contains("mount"));
}

#[test]
fn translates_adapter_failure_sizing_failure() {
    let err = DomainError::AdapterFailure("blockdev --getsize64 failed: some stderr".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
}

#[test]
fn translates_adapter_failure_mapping_name_canonicalization_failure() {
    let err = DomainError::AdapterFailure(
        "failed to canonicalize /no/such/path: No such file or directory (os error 2)".to_string(),
    );
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(message.contains("/no/such/path"));
}

#[test]
fn translates_adapter_failure_canonicalization_failure_path_containing_colon_space() {
    // A path containing ": " (legal on Unix) must not be truncated at that
    // embedded colon — the split must anchor on the LAST ": ", not the
    // first, since the io::Error text itself is appended after the path.
    let err = DomainError::AdapterFailure(
        "failed to canonicalize /tmp/weird: name/foo: No such file or directory (os error 2)"
            .to_string(),
    );
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(message.contains("/tmp/weird: name/foo"));
}

#[test]
fn translates_adapter_failure_unknown_category_falls_back_gracefully() {
    let err = DomainError::AdapterFailure("some completely novel failure string".to_string());
    let message = translate(&err);
    assert!(!message.is_empty());
    assert!(message.contains("some completely novel failure string"));
}
