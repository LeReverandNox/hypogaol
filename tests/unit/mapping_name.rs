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
