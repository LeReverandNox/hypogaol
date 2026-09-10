use std::path::Path;

use clap::Parser;
use hypogaol::cli::main::{
    confirms_revoke, confirms_wipe, device_create_confirmation, parse_size, unlock_intro_message,
    unlock_success_message, Cli,
};
use hypogaol::domain::workflows::create::MIN_VOLUME_SIZE_BYTES;

#[test]
fn rejects_empty_input() {
    assert!(parse_size("").is_err());
    assert!(parse_size("   ").is_err());
}

#[test]
fn rejects_sizes_below_the_minimum() {
    assert!(parse_size("0").is_err());
    assert!(parse_size("1024").is_err());
    assert!(parse_size(&(MIN_VOLUME_SIZE_BYTES - 1).to_string()).is_err());
}

#[test]
fn accepts_the_minimum_size_exactly() {
    assert_eq!(
        parse_size(&MIN_VOLUME_SIZE_BYTES.to_string()),
        Ok(MIN_VOLUME_SIZE_BYTES)
    );
}

#[test]
fn parses_binary_suffixes_case_insensitively() {
    assert_eq!(parse_size("64M"), Ok(64 * 1024 * 1024));
    assert_eq!(parse_size("64m"), Ok(64 * 1024 * 1024));
    assert_eq!(parse_size("1G"), Ok(1024 * 1024 * 1024));
    assert_eq!(parse_size("1T"), Ok(1024u64.pow(4)));
}

#[test]
fn rejects_a_non_numeric_input() {
    assert!(parse_size("abc").is_err());
    assert!(parse_size("16X").is_err());
}

#[test]
fn rejects_overflowing_sizes() {
    assert!(parse_size("99999999999999999999T").is_err());
}

#[test]
fn confirms_wipe_requires_exactly_yes() {
    assert!(confirms_wipe("yes"));
    assert!(confirms_wipe("yes\n"));
    assert!(confirms_wipe("  yes  "));
    assert!(!confirms_wipe("Yes"));
    assert!(!confirms_wipe("YES"));
    assert!(!confirms_wipe("y"));
    assert!(!confirms_wipe(""));
    assert!(!confirms_wipe("no"));
}

#[test]
fn device_create_confirmation_fresh_device_declined() {
    assert_eq!(device_create_confirmation(false, false), (false, false));
}

#[test]
fn device_create_confirmation_fresh_device_confirmed() {
    assert_eq!(device_create_confirmation(false, true), (true, true));
}

#[test]
fn device_create_confirmation_marker_verified_resume_skips_prompt_and_proceeds() {
    // The prompt is never asked on this path (`wipe_confirmed` is always
    // `false` at the real call site) — `confirmed` must stay `false` (no
    // interactive "yes" was obtained; `domain`'s own fresh marker check is
    // the real authority) while `announce` is still `true` (work is expected
    // to proceed). This is the regression guard for the confirmation-bypass
    // review finding (2026-08-08): an explicit decline must never be
    // silently overridden by marker state.
    assert_eq!(device_create_confirmation(true, false), (false, true));
}

#[test]
fn device_create_confirmation_marker_verified_resume_ignores_stale_wipe_confirmed() {
    // Defensive: even if a caller somehow passed `wipe_confirmed: true`
    // alongside `marker_verified_resume: true`, `confirmed` must still come
    // back `false` — resume is authorized by the marker, never by this flag.
    assert_eq!(device_create_confirmation(true, true), (false, true));
}

#[test]
fn confirms_revoke_requires_exactly_yes() {
    assert!(confirms_revoke("yes"));
    assert!(confirms_revoke("yes\n"));
    assert!(confirms_revoke("  yes  "));
    assert!(!confirms_revoke("Yes"));
    assert!(!confirms_revoke("YES"));
    assert!(!confirms_revoke("y"));
    assert!(!confirms_revoke(""));
    assert!(!confirms_revoke("no"));
}

fn help_text(args: &[&str]) -> String {
    match Cli::try_parse_from(args) {
        Ok(_) => panic!("expected --help to short-circuit parsing with a clap::Error"),
        Err(err) => err.to_string(),
    }
}

#[test]
fn top_level_help_lists_all_subcommands() {
    let help = help_text(&["hypogaol", "--help"]);
    assert!(help.contains("create"));
    assert!(help.contains("unlock"));
    assert!(help.contains("enroll"));
    assert!(help.contains("revoke"));
    assert!(help.contains("info"));
}

#[test]
fn create_help_lists_file_and_device_modes() {
    let help = help_text(&["hypogaol", "create", "--help"]);
    assert!(help.contains("file"));
    assert!(help.contains("device"));
}

#[test]
fn unlock_help_lists_path_as_positional() {
    let help = help_text(&["hypogaol", "unlock", "--help"]);
    assert!(help.contains("<PATH>"));
    assert!(!help.contains("--path"));
}

#[test]
fn unlock_help_lists_read_only_flag() {
    let help = help_text(&["hypogaol", "unlock", "--help"]);
    assert!(help.contains("--read-only"));
}

#[test]
fn unlock_intro_message_mentions_read_only_when_set() {
    assert!(unlock_intro_message(true).contains("Unlocking read-only — no changes will be saved."));
    assert!(!unlock_intro_message(false).contains("read-only"));
}

#[test]
fn unlock_success_message_mentions_read_only_when_set() {
    let mountpoint = Path::new("/run/media/user/vault");
    assert!(unlock_success_message(true, mountpoint).contains("(read-only)"));
    assert!(!unlock_success_message(false, mountpoint).contains("(read-only)"));
}

#[test]
fn create_file_help_lists_path_as_positional() {
    let help = help_text(&["hypogaol", "create", "file", "--help"]);
    assert!(help.contains("<PATH>"));
    assert!(!help.contains("--path"));
}

#[test]
fn create_device_help_lists_path_as_positional() {
    let help = help_text(&["hypogaol", "create", "device", "--help"]);
    assert!(help.contains("<PATH>"));
    assert!(!help.contains("--path"));
}

#[test]
fn enroll_help_lists_path_as_positional_and_label_as_a_flag() {
    let help = help_text(&["hypogaol", "enroll", "--help"]);
    assert!(help.contains("<PATH>"));
    assert!(!help.contains("--path"));
    assert!(help.contains("--label"));
}

#[test]
fn revoke_help_lists_path_as_positional_and_label_as_a_flag() {
    let help = help_text(&["hypogaol", "revoke", "--help"]);
    assert!(help.contains("<PATH>"));
    assert!(!help.contains("--path"));
    assert!(help.contains("--label"));
}

#[test]
fn info_help_lists_path_as_positional() {
    let help = help_text(&["hypogaol", "info", "--help"]);
    assert!(help.contains("<PATH>"));
    assert!(!help.contains("--path"));
}

#[test]
fn enroll_help_lists_the_explicit_device_selection_flags() {
    let help = help_text(&["hypogaol", "enroll", "--help"]);
    assert!(help.contains("--fido2-device"));
    assert!(help.contains("--unlock-fido2-device"));
}

#[test]
fn create_file_help_lists_the_fido2_device_flag() {
    let help = help_text(&["hypogaol", "create", "file", "--help"]);
    assert!(help.contains("--fido2-device"));
}

#[test]
fn create_device_help_lists_the_fido2_device_flag() {
    let help = help_text(&["hypogaol", "create", "device", "--help"]);
    assert!(help.contains("--fido2-device"));
}

#[test]
fn create_file_help_lists_xfs_and_btrfs_as_filesystem_values() {
    let help = help_text(&["hypogaol", "create", "file", "--help"]);
    assert!(help.contains("xfs"));
    assert!(help.contains("btrfs"));
}

#[test]
fn create_device_help_lists_xfs_and_btrfs_as_filesystem_values() {
    let help = help_text(&["hypogaol", "create", "device", "--help"]);
    assert!(help.contains("xfs"));
    assert!(help.contains("btrfs"));
}

#[test]
fn create_file_help_lists_label_as_a_flag() {
    let help = help_text(&["hypogaol", "create", "file", "--help"]);
    assert!(help.contains("--label"));
}

#[test]
fn create_device_help_lists_label_as_a_flag() {
    let help = help_text(&["hypogaol", "create", "device", "--help"]);
    assert!(help.contains("--label"));
}

#[test]
fn create_file_help_lists_scaffold_hooks_as_a_flag() {
    let help = help_text(&["hypogaol", "create", "file", "--help"]);
    assert!(help.contains("--scaffold-hooks"));
}

#[test]
fn create_device_help_lists_scaffold_hooks_as_a_flag() {
    let help = help_text(&["hypogaol", "create", "device", "--help"]);
    assert!(help.contains("--scaffold-hooks"));
}

#[test]
fn create_file_rejects_an_empty_label() {
    let result = Cli::try_parse_from([
        "hypogaol",
        "create",
        "file",
        "/tmp/some-volume",
        "--size",
        "64M",
        "--label",
        "",
    ]);
    assert!(
        result.is_err(),
        "--label \"\" must be a parse error, not a silently-accepted empty label"
    );
}

#[test]
fn create_device_rejects_a_whitespace_only_label() {
    let result = Cli::try_parse_from([
        "hypogaol",
        "create",
        "device",
        "/tmp/some-device",
        "--label",
        "   ",
    ]);
    assert!(
        result.is_err(),
        "--label \"   \" must be a parse error, not a silently-accepted blank label"
    );
}

#[test]
fn resize_help_lists_path_as_positional_and_size_as_a_flag() {
    let help = help_text(&["hypogaol", "resize", "--help"]);
    assert!(help.contains("<PATH>"));
    assert!(!help.contains("--path"));
    assert!(help.contains("--size"));
}

#[test]
fn enroll_rejects_fido2_device_flag_given_without_its_unlock_pair() {
    let result = Cli::try_parse_from([
        "hypogaol",
        "enroll",
        "/tmp/some-volume",
        "--label",
        "backup",
        "--fido2-device",
        "/dev/hidraw1",
    ]);
    assert!(
        result.is_err(),
        "--fido2-device without --unlock-fido2-device must be a parse error, not a partial fallback"
    );
}

#[test]
fn enroll_rejects_unlock_fido2_device_flag_given_without_its_pair() {
    let result = Cli::try_parse_from([
        "hypogaol",
        "enroll",
        "/tmp/some-volume",
        "--label",
        "backup",
        "--unlock-fido2-device",
        "/dev/hidraw0",
    ]);
    assert!(
        result.is_err(),
        "--unlock-fido2-device without --fido2-device must be a parse error, not a partial fallback"
    );
}

#[test]
fn enroll_accepts_both_explicit_device_flags_together() {
    let result = Cli::try_parse_from([
        "hypogaol",
        "enroll",
        "/tmp/some-volume",
        "--label",
        "backup",
        "--fido2-device",
        "/dev/hidraw1",
        "--unlock-fido2-device",
        "/dev/hidraw0",
    ]);
    assert!(
        result.is_ok(),
        "expected --fido2-device and --unlock-fido2-device together to parse successfully"
    );
}

// --- Story 6.7: short-vs-long equivalence tests ---

#[test]
fn unlock_read_only_short_and_long_forms_are_equivalent() {
    let short = Cli::try_parse_from(["hypogaol", "unlock", "/tmp/v", "-r"]).unwrap();
    let long = Cli::try_parse_from(["hypogaol", "unlock", "/tmp/v", "--read-only"]).unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn unlock_skip_hooks_short_and_long_forms_are_equivalent() {
    let short = Cli::try_parse_from(["hypogaol", "unlock", "/tmp/v", "-s"]).unwrap();
    let long = Cli::try_parse_from(["hypogaol", "unlock", "/tmp/v", "--skip-hooks"]).unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn enroll_label_short_and_long_forms_are_equivalent() {
    let short = Cli::try_parse_from(["hypogaol", "enroll", "/tmp/v", "-l", "backup"]).unwrap();
    let long = Cli::try_parse_from(["hypogaol", "enroll", "/tmp/v", "--label", "backup"]).unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn revoke_label_short_and_long_forms_are_equivalent() {
    let short = Cli::try_parse_from(["hypogaol", "revoke", "/tmp/v", "-l", "primary"]).unwrap();
    let long = Cli::try_parse_from(["hypogaol", "revoke", "/tmp/v", "--label", "primary"]).unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn close_skip_hooks_short_and_long_forms_are_equivalent() {
    let short = Cli::try_parse_from(["hypogaol", "close", "/tmp/v", "-s"]).unwrap();
    let long = Cli::try_parse_from(["hypogaol", "close", "/tmp/v", "--skip-hooks"]).unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn close_all_skip_hooks_short_and_long_forms_are_equivalent() {
    let short = Cli::try_parse_from(["hypogaol", "close-all", "-s"]).unwrap();
    let long = Cli::try_parse_from(["hypogaol", "close-all", "--skip-hooks"]).unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn resize_size_short_and_long_forms_are_equivalent() {
    let short = Cli::try_parse_from(["hypogaol", "resize", "/tmp/v", "-s", "20G"]).unwrap();
    let long = Cli::try_parse_from(["hypogaol", "resize", "/tmp/v", "--size", "20G"]).unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

// --- Story 6.7: explicit collision-pair coverage (proves AC #3) ---

#[test]
fn create_file_short_s_is_size_not_scaffold_hooks() {
    let short = Cli::try_parse_from(["hypogaol", "create", "file", "/tmp/v", "-s", "64M"]).unwrap();
    let long =
        Cli::try_parse_from(["hypogaol", "create", "file", "/tmp/v", "--size", "64M"]).unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn create_file_short_c_is_scaffold_hooks_not_size() {
    let short = Cli::try_parse_from([
        "hypogaol", "create", "file", "/tmp/v", "--size", "64M", "-c",
    ])
    .unwrap();
    let long = Cli::try_parse_from([
        "hypogaol",
        "create",
        "file",
        "/tmp/v",
        "--size",
        "64M",
        "--scaffold-hooks",
    ])
    .unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn create_file_short_f_is_filesystem_not_fido2_device() {
    let short = Cli::try_parse_from([
        "hypogaol", "create", "file", "/tmp/v", "--size", "64M", "-f", "xfs",
    ])
    .unwrap();
    let long = Cli::try_parse_from([
        "hypogaol",
        "create",
        "file",
        "/tmp/v",
        "--size",
        "64M",
        "--filesystem",
        "xfs",
    ])
    .unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn create_file_short_d_is_fido2_device_not_filesystem() {
    let short = Cli::try_parse_from([
        "hypogaol",
        "create",
        "file",
        "/tmp/v",
        "--size",
        "64M",
        "-d",
        "/dev/hidraw1",
    ])
    .unwrap();
    let long = Cli::try_parse_from([
        "hypogaol",
        "create",
        "file",
        "/tmp/v",
        "--size",
        "64M",
        "--fido2-device",
        "/dev/hidraw1",
    ])
    .unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn create_device_short_s_is_size_not_scaffold_hooks() {
    let short =
        Cli::try_parse_from(["hypogaol", "create", "device", "/tmp/v", "-s", "64M"]).unwrap();
    let long =
        Cli::try_parse_from(["hypogaol", "create", "device", "/tmp/v", "--size", "64M"]).unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn create_device_short_c_is_scaffold_hooks_not_size() {
    let short = Cli::try_parse_from(["hypogaol", "create", "device", "/tmp/v", "-c"]).unwrap();
    let long = Cli::try_parse_from(["hypogaol", "create", "device", "/tmp/v", "--scaffold-hooks"])
        .unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn create_device_short_f_is_filesystem_not_fido2_device() {
    let short =
        Cli::try_parse_from(["hypogaol", "create", "device", "/tmp/v", "-f", "btrfs"]).unwrap();
    let long = Cli::try_parse_from([
        "hypogaol",
        "create",
        "device",
        "/tmp/v",
        "--filesystem",
        "btrfs",
    ])
    .unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn create_device_short_d_is_fido2_device_not_filesystem() {
    let short = Cli::try_parse_from([
        "hypogaol",
        "create",
        "device",
        "/tmp/v",
        "-d",
        "/dev/hidraw1",
    ])
    .unwrap();
    let long = Cli::try_parse_from([
        "hypogaol",
        "create",
        "device",
        "/tmp/v",
        "--fido2-device",
        "/dev/hidraw1",
    ])
    .unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn enroll_short_n_is_unlock_fido2_device_not_user_verification() {
    let short = Cli::try_parse_from([
        "hypogaol",
        "enroll",
        "/tmp/v",
        "--label",
        "backup",
        "--fido2-device",
        "/dev/hidraw1",
        "-n",
        "/dev/hidraw0",
    ])
    .unwrap();
    let long = Cli::try_parse_from([
        "hypogaol",
        "enroll",
        "/tmp/v",
        "--label",
        "backup",
        "--fido2-device",
        "/dev/hidraw1",
        "--unlock-fido2-device",
        "/dev/hidraw0",
    ])
    .unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn enroll_short_u_is_user_verification_not_unlock_fido2_device() {
    let short =
        Cli::try_parse_from(["hypogaol", "enroll", "/tmp/v", "--label", "backup", "-u"]).unwrap();
    let long = Cli::try_parse_from([
        "hypogaol",
        "enroll",
        "/tmp/v",
        "--label",
        "backup",
        "--user-verification",
    ])
    .unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn create_file_label_short_and_long_forms_are_equivalent() {
    let short = Cli::try_parse_from([
        "hypogaol", "create", "file", "/tmp/v", "--size", "64M", "-l", "backup",
    ])
    .unwrap();
    let long = Cli::try_parse_from([
        "hypogaol", "create", "file", "/tmp/v", "--size", "64M", "--label", "backup",
    ])
    .unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn create_file_user_verification_short_and_long_forms_are_equivalent() {
    let short = Cli::try_parse_from([
        "hypogaol", "create", "file", "/tmp/v", "--size", "64M", "-u",
    ])
    .unwrap();
    let long = Cli::try_parse_from([
        "hypogaol",
        "create",
        "file",
        "/tmp/v",
        "--size",
        "64M",
        "--user-verification",
    ])
    .unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn create_device_label_short_and_long_forms_are_equivalent() {
    let short =
        Cli::try_parse_from(["hypogaol", "create", "device", "/tmp/v", "-l", "backup"]).unwrap();
    let long = Cli::try_parse_from([
        "hypogaol", "create", "device", "/tmp/v", "--label", "backup",
    ])
    .unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn create_device_user_verification_short_and_long_forms_are_equivalent() {
    let short = Cli::try_parse_from(["hypogaol", "create", "device", "/tmp/v", "-u"]).unwrap();
    let long = Cli::try_parse_from([
        "hypogaol",
        "create",
        "device",
        "/tmp/v",
        "--user-verification",
    ])
    .unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

// --- Story 7.1: -p/--client-pin short-vs-long equivalence tests ---

#[test]
fn enroll_client_pin_short_and_long_forms_are_equivalent() {
    let short =
        Cli::try_parse_from(["hypogaol", "enroll", "/tmp/v", "--label", "backup", "-p"]).unwrap();
    let long = Cli::try_parse_from([
        "hypogaol",
        "enroll",
        "/tmp/v",
        "--label",
        "backup",
        "--client-pin",
    ])
    .unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn create_file_client_pin_short_and_long_forms_are_equivalent() {
    let short = Cli::try_parse_from([
        "hypogaol", "create", "file", "/tmp/v", "--size", "64M", "-p",
    ])
    .unwrap();
    let long = Cli::try_parse_from([
        "hypogaol",
        "create",
        "file",
        "/tmp/v",
        "--size",
        "64M",
        "--client-pin",
    ])
    .unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

#[test]
fn create_device_client_pin_short_and_long_forms_are_equivalent() {
    let short = Cli::try_parse_from(["hypogaol", "create", "device", "/tmp/v", "-p"]).unwrap();
    let long =
        Cli::try_parse_from(["hypogaol", "create", "device", "/tmp/v", "--client-pin"]).unwrap();
    assert_eq!(format!("{short:?}"), format!("{long:?}"));
}

// --- Story 6.7: help-text short-alias presence tests (AC #1) ---

#[test]
fn unlock_help_shows_short_aliases() {
    let help = help_text(&["hypogaol", "unlock", "--help"]);
    assert!(help.contains("-r, --read-only"));
    assert!(help.contains("-s, --skip-hooks"));
}

#[test]
fn enroll_help_shows_short_aliases() {
    let help = help_text(&["hypogaol", "enroll", "--help"]);
    assert!(help.contains("-l, --label"));
    assert!(help.contains("-f, --fido2-device"));
    assert!(help.contains("-n, --unlock-fido2-device"));
    assert!(help.contains("-u, --user-verification"));
    assert!(help.contains("-p, --client-pin"));
}

#[test]
fn revoke_help_shows_short_alias() {
    let help = help_text(&["hypogaol", "revoke", "--help"]);
    assert!(help.contains("-l, --label"));
}

#[test]
fn close_help_shows_short_alias() {
    let help = help_text(&["hypogaol", "close", "--help"]);
    assert!(help.contains("-s, --skip-hooks"));
}

#[test]
fn close_all_help_shows_short_alias() {
    let help = help_text(&["hypogaol", "close-all", "--help"]);
    assert!(help.contains("-s, --skip-hooks"));
}

#[test]
fn resize_help_shows_short_alias() {
    let help = help_text(&["hypogaol", "resize", "--help"]);
    assert!(help.contains("-s, --size"));
}

#[test]
fn create_file_help_shows_short_aliases() {
    let help = help_text(&["hypogaol", "create", "file", "--help"]);
    assert!(help.contains("-s, --size"));
    assert!(help.contains("-f, --filesystem"));
    assert!(help.contains("-c, --scaffold-hooks"));
    assert!(help.contains("-l, --label"));
    assert!(help.contains("-d, --fido2-device"));
    assert!(help.contains("-u, --user-verification"));
    assert!(help.contains("-p, --client-pin"));
}

#[test]
fn create_device_help_shows_short_aliases() {
    let help = help_text(&["hypogaol", "create", "device", "--help"]);
    assert!(help.contains("-s, --size"));
    assert!(help.contains("-f, --filesystem"));
    assert!(help.contains("-c, --scaffold-hooks"));
    assert!(help.contains("-l, --label"));
    assert!(help.contains("-d, --fido2-device"));
    assert!(help.contains("-u, --user-verification"));
    assert!(help.contains("-p, --client-pin"));
}

// --- Story 6.7: -h/-V untouched (AC #4) ---

#[test]
fn short_h_flag_still_short_circuits_to_help() {
    let err = Cli::try_parse_from(["hypogaol", "-h"]).unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
}

#[test]
fn short_uppercase_v_flag_still_short_circuits_to_version() {
    let err = Cli::try_parse_from(["hypogaol", "-V"]).unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
}

#[test]
fn top_level_help_still_lists_the_clap_automatic_h_and_uppercase_v_flags() {
    let help = help_text(&["hypogaol", "--help"]);
    assert!(help.contains("-h, --help"));
    assert!(help.contains("-V, --version"));
}
