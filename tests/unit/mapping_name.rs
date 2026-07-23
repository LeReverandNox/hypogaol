use tomb_fido2::domain::mapping_name::mapping_name;

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
    let missing = std::env::temp_dir().join("tomb-fido2-mapping-name-does-not-exist");

    let result = mapping_name(&missing);

    assert!(
        result.is_err(),
        "expected an error for a path that cannot be canonicalized, got {result:?}"
    );
}
