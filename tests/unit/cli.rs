use tomb_fido2::cli::main::{parse_size, MIN_TOMB_SIZE_BYTES};

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
