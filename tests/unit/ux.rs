use std::path::PathBuf;

use hypogaol::cli::ux::{
    translate, translate_create_stage, translate_hook_warning, translate_resize_stage,
};
use hypogaol::domain::errors::DomainError;
use hypogaol::domain::hooks::{BindHookSkipReason, HookRejectionReason, HookWarning};
use hypogaol::domain::progress::{CreateStage, ResizeStage};

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
    let err = DomainError::DestinationExists(PathBuf::from("/tmp/my-volume"));
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(message.contains("/tmp/my-volume"));
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
fn translates_adapter_failure_too_small_for_activation_not_as_a_touch_pin_failure() {
    // This message contains the literal substring "cryptsetup" (via the
    // `{cmd:?}` argv dump) but must be classified as a sizing problem, not
    // the generic touch/PIN-timing bucket — proves the too-small-for-
    // activation marker check runs before the broader "cryptsetup" check.
    let err = DomainError::AdapterFailure(
        r#""cryptsetup" "luksOpen" "--key-file" "-" "/tmp/foo" "vault-abc123" failed: Device /tmp/foo is too small for activation, there is no remaining space for data."#
            .to_string(),
    );
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("PIN"));
    assert!(message.to_lowercase().contains("size") || message.to_lowercase().contains("larger"));
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
        "failed to create temporary key file /dev/shm/.volume-fido2-bootstrap-abc123: Permission denied (os error 13)"
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
fn translates_adapter_failure_fido2_token_enumeration_as_workflow_neutral() {
    // `list_fido2_devices`/`wait_for_enough_fido2_devices` are shared by
    // create's bootstrap enroll, `enroll`'s own device-selection, AND
    // `unlock`'s presence-wait — this message must not claim "Enrolling..."
    // since the failure could just as easily come from a plain `unlock`.
    let err = DomainError::AdapterFailure("fido2-token -L failed: some stderr".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("Enrolling"));
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

// `"umount".contains("mount")` is `true` as a plain substring, so a close
// failure could silently fall into the unlock-flavored "Your volume unlocked,
// but Hypogaol couldn't mount its filesystem" message unless it's matched
// ahead of that generic bucket (the "marker bleed" bug class the Epic 2
// retro flagged).
#[test]
fn translates_adapter_failure_umount_failure_is_not_swallowed_by_the_unlock_mount_message() {
    let err = DomainError::AdapterFailure("umount failed: target is busy".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(
        !message.contains("Your volume unlocked"),
        "close's own umount failure was misclassified as unlock's mount-failure message: {message:?}"
    );
}

#[test]
fn translates_adapter_failure_findmnt_failure_is_not_swallowed_by_the_unlock_mount_message() {
    let err = DomainError::AdapterFailure("failed to run findmnt: some io error".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("Your volume unlocked"));
}

#[test]
fn translates_adapter_failure_not_currently_mounted_gets_its_own_distinct_message() {
    let err = DomainError::AdapterFailure(
        "/dev/mapper/vault-deadbeef is not currently mounted".to_string(),
    );
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("Your volume unlocked"));

    let generic_umount_err =
        DomainError::AdapterFailure("umount failed: target is busy".to_string());
    let generic_message = translate(&generic_umount_err);
    assert_ne!(
        message, generic_message,
        "the not-currently-mounted case should read distinctly from a generic umount failure"
    );
}

// `"cryptsetup close failed"` contains `"cryptsetup"`, so a `close`-time
// `luks.close` failure could silently fall into the generic cryptsetup
// bucket's touch/PIN-entry framing unless matched ahead of it — the same
// "marker bleed" class as the umount/findmnt guard above (review finding,
// 2026-07-26).
#[test]
fn translates_adapter_failure_luks_close_failure_is_not_swallowed_by_the_generic_cryptsetup_message(
) {
    let err = DomainError::AdapterFailure("cryptsetup close failed: device is busy".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(
        !message.contains("security key"),
        "close's own luks.close failure was misclassified as the generic cryptsetup message: {message:?}"
    );
}

#[test]
fn translates_adapter_failure_no_active_mapping_gets_its_own_distinct_message() {
    let err =
        DomainError::AdapterFailure("/dev/mapper/vault-deadbeef has no active mapping".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("Your volume unlocked"));

    let not_currently_mounted_err = DomainError::AdapterFailure(
        "/dev/mapper/vault-deadbeef is not currently mounted".to_string(),
    );
    let not_currently_mounted_message = translate(&not_currently_mounted_err);
    assert_ne!(
        message, not_currently_mounted_message,
        "never-unlocked should read distinctly from unmounted-but-still-open"
    );
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
fn translates_resize_must_grow() {
    let err = DomainError::ResizeMustGrow {
        path: PathBuf::from("/tmp/my-volume.img"),
        requested: 1_000,
        current_size: 2_000,
    };
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(message.contains("1000"));
    assert!(message.contains("2000"));
    assert!(message.contains("/tmp/my-volume.img"));
}

// "cryptsetup resize --token-only failed for ..." and "failed to run
// cryptsetup resize: ..." both contain the literal substring "cryptsetup",
// so a resize failure could silently fall into the generic cryptsetup
// bucket's create/unlock-flavored framing unless matched ahead of it — the
// same "marker bleed" class the Epic 2 retro flagged (3 real bugs from this
// pattern so far).
#[test]
fn translates_adapter_failure_cryptsetup_resize_failure_gets_its_own_message() {
    let err = DomainError::AdapterFailure(
        "cryptsetup resize --token-only failed for vault-abc123".to_string(),
    );
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("vault-abc123"));

    let err =
        DomainError::AdapterFailure("failed to run cryptsetup resize: some io error".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
}

// "resize2fs failed: ..." contains neither "mount" nor "mkfs", so without a
// dedicated branch it would fall all the way through to the unhelpful
// generic fallback (which leaks the raw technical string) instead of a
// plain-language message.
#[test]
fn translates_adapter_failure_resize2fs_failure_gets_its_own_message() {
    let err = DomainError::AdapterFailure("resize2fs failed: some stderr".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("some stderr"));

    let err = DomainError::AdapterFailure("failed to run resize2fs: some io error".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("some io error"));
}

// Neither "e2fsck -f failed: ..." nor "failed to run e2fsck: ..." contains
// "resize2fs", "mount", or "mkfs", so without a dedicated branch these would
// fall all the way through to the unhelpful generic fallback (marker-bleed
// guard, review finding 2026-07-26).
#[test]
fn translates_adapter_failure_e2fsck_failure_gets_its_own_message() {
    let err = DomainError::AdapterFailure("e2fsck -f failed: some stderr".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("some stderr"));

    let err = DomainError::AdapterFailure("failed to run e2fsck: some io error".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("some io error"));
}

// `resize`'s pre-grow stat/symlink-refusal failures (`is_block_device`'s own
// stat, and `set_backing_file_size`'s grow-branch checks) previously had no
// dedicated branch and leaked raw technical detail via the generic fallback
// (review finding, 2026-07-26).
#[test]
fn translates_adapter_failure_resize_pre_grow_checks_as_sizing_failure() {
    let err =
        DomainError::AdapterFailure("failed to stat /tmp/my-volume.img: some io error".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("some io error"));

    let err = DomainError::AdapterFailure(
        "/tmp/my-volume.img is not a regular file — refusing to grow it".to_string(),
    );
    let message = translate(&err);
    assert_no_jargon(&message);

    let err = DomainError::AdapterFailure(
        "failed to open /tmp/my-volume.img for growing: some io error".to_string(),
    );
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("some io error"));
}

// A `luks.close` failure that happens *after* a successful grow must read
// distinctly from both a plain resize failure and a plain `close` failure —
// otherwise the user is told resize failed even though their volume's capacity
// was already safely increased (review finding, 2026-07-26).
#[test]
fn translates_adapter_failure_post_grow_close_failure_is_distinct_from_generic_close_failure() {
    let err = DomainError::AdapterFailure(
        "volume grown to 8192 bytes, but failed to re-lock afterward: cryptsetup close failed: device is busy"
            .to_string(),
    );
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(
        message.contains("grew this volume successfully"),
        "message should acknowledge the grow succeeded: {message:?}"
    );

    let generic_close_err =
        DomainError::AdapterFailure("cryptsetup close failed: device is busy".to_string());
    let generic_message = translate(&generic_close_err);
    assert_ne!(
        message, generic_message,
        "a post-grow close failure should read distinctly from a plain close failure"
    );
}

#[test]
fn translates_key_not_found() {
    let err = DomainError::KeyNotFound("nonexistent".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(message.contains("nonexistent"));
}

#[test]
fn translates_adapter_failure_luks_dump_read_failure_as_workflow_neutral() {
    // Shared by `enroll`'s label-uniqueness check and `revoke`'s
    // `list_fido2_keyslots` call — must not claim "Enrolling..." since the
    // failure could just as easily come from a plain `revoke`.
    let err = DomainError::AdapterFailure(
        "cryptsetup luksDump --dump-json-metadata failed: some stderr".to_string(),
    );
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("Enrolling"));
}

#[test]
fn translates_adapter_failure_revoke_removal_failure_as_revoke_specific() {
    // `remove_key`'s own failures (`luksKillSlot`/`token remove`) are a
    // non-interactive header edit — must not fall through to the generic
    // touch/PIN-entry cryptsetup message, which would be meaningless here.
    let err =
        DomainError::AdapterFailure("cryptsetup luksKillSlot failed: some stderr".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("PIN"));
    assert!(message.contains("Revoking"));

    let err =
        DomainError::AdapterFailure("cryptsetup token remove failed: some stderr".to_string());
    let message = translate(&err);
    assert_no_jargon(&message);
    assert!(!message.contains("PIN"));
    assert!(message.contains("Revoking"));
}

#[test]
fn translates_adapter_failure_unknown_category_falls_back_gracefully() {
    let err = DomainError::AdapterFailure("some completely novel failure string".to_string());
    let message = translate(&err);
    assert!(!message.is_empty());
    assert!(message.contains("some completely novel failure string"));
}

#[test]
fn translate_create_stage_covers_every_variant_with_a_distinct_no_jargon_message() {
    let stages = [
        CreateStage::AllocatingBackingFile,
        CreateStage::FormattingLuks2,
        CreateStage::EnrollingFido2Key,
        CreateStage::CreatingFilesystem,
    ];
    let messages: Vec<&str> = stages.iter().map(translate_create_stage).collect();

    for message in messages.iter().copied() {
        assert_no_jargon(message);
    }

    let mut unique = messages.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        messages.len(),
        "expected every CreateStage variant to translate to a distinct message, got {messages:?}"
    );
}

#[test]
fn translate_resize_stage_covers_every_variant_with_a_distinct_no_jargon_message() {
    let stages = [
        ResizeStage::GrowingBackingFile,
        ResizeStage::ResizingLuks2Mapping,
        ResizeStage::GrowingFilesystem,
    ];
    let messages: Vec<&str> = stages.iter().map(translate_resize_stage).collect();

    for message in messages.iter().copied() {
        assert_no_jargon(message);
    }

    let mut unique = messages.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        messages.len(),
        "expected every ResizeStage variant to translate to a distinct message, got {messages:?}"
    );
}

#[test]
fn translate_covers_every_hook_rejection_reason_with_a_distinct_no_jargon_message() {
    let reasons = [
        HookRejectionReason::NotARegularFile,
        HookRejectionReason::NotExecutable,
        HookRejectionReason::WrongOwner,
        HookRejectionReason::WorldWritable,
    ];
    let messages: Vec<String> = reasons
        .iter()
        .map(|reason| {
            translate(&DomainError::HookRejected {
                path: PathBuf::from("/tmp/my-volume/exec-hooks"),
                reason: *reason,
            })
        })
        .collect();

    for message in &messages {
        assert_no_jargon(message);
        assert!(message.contains("/tmp/my-volume/exec-hooks"));
    }

    let mut unique = messages.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        messages.len(),
        "expected every HookRejectionReason variant to translate to a distinct message, got {messages:?}"
    );
}

#[test]
fn translate_hook_warning_covers_every_bind_hook_skip_reason_with_a_distinct_no_jargon_message() {
    let reasons = [
        BindHookSkipReason::SourceMissing,
        BindHookSkipReason::DestMissing,
        BindHookSkipReason::SourceEscapesVolumeRoot,
        BindHookSkipReason::DestEscapesHome,
        BindHookSkipReason::BindMountFailed,
    ];
    let messages: Vec<String> = reasons
        .iter()
        .map(|reason| {
            translate_hook_warning(&HookWarning::BindHookSkipped {
                source: ".gnupg".to_string(),
                dest: ".gnupg".to_string(),
                reason: *reason,
            })
        })
        .collect();

    for message in &messages {
        assert_no_jargon(message);
        assert!(message.contains(".gnupg"));
    }

    let mut unique = messages.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        messages.len(),
        "expected every BindHookSkipReason variant to translate to a distinct message, got {messages:?}"
    );
}

#[test]
fn translate_hook_warning_exec_hook_non_zero_exit_with_a_code() {
    let message = translate_hook_warning(&HookWarning::ExecHookNonZeroExit {
        path: PathBuf::from("/tmp/my-volume/exec-hooks"),
        exit_code: Some(3),
    });
    assert_no_jargon(&message);
    assert!(message.contains("/tmp/my-volume/exec-hooks"));
    assert!(message.contains('3'));
}

#[test]
fn translate_hook_warning_exec_hook_non_zero_exit_without_a_code() {
    let message = translate_hook_warning(&HookWarning::ExecHookNonZeroExit {
        path: PathBuf::from("/tmp/my-volume/exec-hooks"),
        exit_code: None,
    });
    assert_no_jargon(&message);
    assert!(message.contains("/tmp/my-volume/exec-hooks"));
}
