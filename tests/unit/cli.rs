use clap::Parser;
use tomb_fido2::cli::main::{confirms_wipe, parse_size, Cli};
use tomb_fido2::domain::workflows::create::MIN_TOMB_SIZE_BYTES;

#[test]
fn rejects_empty_input() {
    assert!(parse_size("").is_err());
    assert!(parse_size("   ").is_err());
}

#[test]
fn rejects_sizes_below_the_minimum() {
    assert!(parse_size("0").is_err());
    assert!(parse_size("1024").is_err());
    assert!(parse_size(&(MIN_TOMB_SIZE_BYTES - 1).to_string()).is_err());
}

#[test]
fn accepts_the_minimum_size_exactly() {
    assert_eq!(
        parse_size(&MIN_TOMB_SIZE_BYTES.to_string()),
        Ok(MIN_TOMB_SIZE_BYTES)
    );
}

#[test]
fn parses_binary_suffixes_case_insensitively() {
    assert_eq!(parse_size("16M"), Ok(16 * 1024 * 1024));
    assert_eq!(parse_size("16m"), Ok(16 * 1024 * 1024));
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

fn help_text(args: &[&str]) -> String {
    match Cli::try_parse_from(args) {
        Ok(_) => panic!("expected --help to short-circuit parsing with a clap::Error"),
        Err(err) => err.to_string(),
    }
}

#[test]
fn top_level_help_lists_both_subcommands() {
    let help = help_text(&["tomb-fido2", "--help"]);
    assert!(help.contains("create"));
    assert!(help.contains("unlock"));
}

#[test]
fn create_help_lists_file_and_device_modes() {
    let help = help_text(&["tomb-fido2", "create", "--help"]);
    assert!(help.contains("file"));
    assert!(help.contains("device"));
}

#[test]
fn unlock_help_lists_path_as_positional() {
    let help = help_text(&["tomb-fido2", "unlock", "--help"]);
    assert!(help.contains("<PATH>"));
    assert!(!help.contains("--path"));
}

#[test]
fn create_file_help_lists_path_as_positional() {
    let help = help_text(&["tomb-fido2", "create", "file", "--help"]);
    assert!(help.contains("<PATH>"));
    assert!(!help.contains("--path"));
}

#[test]
fn create_device_help_lists_path_as_positional() {
    let help = help_text(&["tomb-fido2", "create", "device", "--help"]);
    assert!(help.contains("<PATH>"));
    assert!(!help.contains("--path"));
}
