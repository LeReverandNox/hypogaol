use hypogaol::domain::mapping_name::{lock_target_path, mapping_name};

#[test]
fn same_path_produces_the_same_name_every_time() {
    let dir = std::env::temp_dir();

    let first = mapping_name(&dir).expect("canonicalize should succeed for an existing dir");
    let second = mapping_name(&dir).expect("canonicalize should succeed for an existing dir");

    assert_eq!(first, second);
    assert!(first.starts_with("vault-"));
}

#[test]
fn nonexistent_path_returns_an_error_instead_of_silently_falling_back() {
    let missing = std::env::temp_dir().join("volume-fido2-mapping-name-does-not-exist");

    let result = mapping_name(&missing);

    assert!(
        result.is_err(),
        "expected an error for a path that cannot be canonicalized, got {result:?}"
    );
}

#[test]
fn lock_target_path_of_an_existing_file_returns_that_files_canonical_path() {
    let dir = std::env::temp_dir();
    let path = dir.join("hypogaol-unit-test-lock-target-path-existing-file");
    std::fs::write(&path, []).expect("failed to create test fixture file");

    let result = lock_target_path(&path);

    std::fs::remove_file(&path).ok();

    let expected = std::fs::canonicalize(&dir)
        .expect("temp dir should canonicalize")
        .join("hypogaol-unit-test-lock-target-path-existing-file");
    assert_eq!(result.unwrap(), expected);
}

#[test]
fn lock_target_path_of_a_nonexistent_path_returns_its_parents_canonical_path() {
    let dir = std::env::temp_dir();
    let missing = dir.join("hypogaol-unit-test-lock-target-path-does-not-exist");

    let result = lock_target_path(&missing);

    let expected_parent = std::fs::canonicalize(&dir).expect("temp dir should canonicalize");
    assert_eq!(result.unwrap(), expected_parent);
}

// Regression test for a bug found post-review (2026-08-10): a real
// `hypogaol create file --size 64M volume.img` invocation failed outright
// with a misleading "couldn't find volume.img" error, even though the path
// was perfectly valid. Root cause: `Path::parent()` on a bare relative
// filename (no directory component at all) returns `Some("")` — the empty
// path — not `None`, so a naive `path.parent().unwrap_or_else(|| Path::new("."))`
// never reaches its own "." fallback, and `canonicalize("")` fails with
// ENOENT. Every other test in this file uses an absolute path (via
// `std::env::temp_dir()`), which never exercises this branch.
#[test]
fn lock_target_path_of_a_bare_relative_filename_falls_back_to_the_current_directory() {
    let missing = std::path::PathBuf::from(
        "hypogaol-unit-test-lock-target-path-bare-relative-filename-does-not-exist",
    );

    let result = lock_target_path(&missing);

    let expected_parent =
        std::fs::canonicalize(".").expect("current directory should canonicalize");
    assert_eq!(result.unwrap(), expected_parent);
}

#[test]
fn lock_target_path_of_a_path_whose_parent_also_does_not_exist_returns_an_adapter_failure() {
    let missing = std::env::temp_dir()
        .join("hypogaol-unit-test-lock-target-path-missing-parent-dir-does-not-exist")
        .join("also-does-not-exist");

    let result = lock_target_path(&missing);

    assert!(
        result.is_err(),
        "expected an error when neither the path nor its parent exist, got {result:?}"
    );
}
