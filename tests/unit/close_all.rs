use std::path::PathBuf;

use hypogaol::domain::errors::DomainError;
use hypogaol::domain::types::MapperHandle;
use hypogaol::domain::workflows::close_all;

use crate::fakes::{new_call_log, FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend};

fn mapper(name: &str, source_path: &str) -> MapperHandle {
    MapperHandle {
        name: name.to_string(),
        source_path: PathBuf::from(source_path),
    }
}

#[test]
fn closes_every_discovered_mapping_using_the_full_close_sequence() {
    let log = new_call_log();
    let mapper_a = mapper("vault-aaaa", "/volume/a.img");
    let mapper_b = mapper("vault-bbbb", "/volume/b.img");

    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_open_mappings(vec![mapper_a.clone(), mapper_b.clone()]);
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let result = close_all::run(false, &|_| {}, &luks, &fido2, &fs);

    let results = result.expect("expected Ok");
    assert_eq!(results.len(), 2);
    assert!(
        results.iter().all(|(_, r)| r.is_ok()),
        "expected every mapping to close successfully, got {results:?}"
    );
    assert_eq!(results[0].0.name, mapper_a.name);
    assert_eq!(results[1].0.name, mapper_b.name);

    // Hooks step (mount_point_of + 2x path_exists, both hooks files absent)
    // then umount, then close — once per discovered mapping, in order.
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "list_open_mappings".to_string(),
            "mount_point_of".to_string(),
            "path_exists".to_string(),
            "path_exists".to_string(),
            "umount".to_string(),
            "close".to_string(),
            "mount_point_of".to_string(),
            "path_exists".to_string(),
            "path_exists".to_string(),
            "umount".to_string(),
            "close".to_string(),
        ]
    );

    // Each port call received the exact mapper `list_open_mappings` handed
    // back — identity, not a re-derived name.
    assert_eq!(luks.last_close().unwrap().name, mapper_b.name);
    assert_eq!(fs.last_umount().unwrap().name, mapper_b.name);
}

#[test]
fn one_mappings_close_failure_does_not_stop_the_batch() {
    let log = new_call_log();
    let mapper_a = mapper("vault-aaaa", "/volume/a.img");
    let mapper_b = mapper("vault-bbbb", "/volume/b.img");

    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_open_mappings(vec![mapper_a.clone(), mapper_b.clone()])
        .with_close_failure_for(&mapper_a.name);
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let results = close_all::run(false, &|_| {}, &luks, &fido2, &fs).expect("expected Ok");

    assert_eq!(results.len(), 2);
    assert!(
        results[0].1.is_err(),
        "expected mapper_a to fail, got {:?}",
        results[0].1
    );
    assert!(
        results[1].1.is_ok(),
        "expected mapper_b to still succeed, got {:?}",
        results[1].1
    );

    // Both mappings' `close` calls must have actually run — the failure on
    // mapper_a's close must not prevent mapper_b's from being attempted.
    assert_eq!(
        log.borrow().iter().filter(|c| *c == "close").count(),
        2,
        "expected close to be attempted for both mappings, log: {:?}",
        log.borrow()
    );
    assert_eq!(
        log.borrow().iter().filter(|c| *c == "umount").count(),
        2,
        "expected umount to be attempted for both mappings, log: {:?}",
        log.borrow()
    );
}

#[test]
fn one_mappings_umount_failure_does_not_stop_the_batch() {
    let log = new_call_log();
    let mapper_a = mapper("vault-aaaa", "/volume/a.img");
    let mapper_b = mapper("vault-bbbb", "/volume/b.img");

    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_open_mappings(vec![mapper_a.clone(), mapper_b.clone()]);
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_umount_failure_for(&mapper_a.name);

    let results = close_all::run(false, &|_| {}, &luks, &fido2, &fs).expect("expected Ok");

    assert_eq!(results.len(), 2);
    assert!(
        results[0].1.is_err(),
        "expected mapper_a to fail, got {:?}",
        results[0].1
    );
    assert!(
        results[1].1.is_ok(),
        "expected mapper_b to still succeed, got {:?}",
        results[1].1
    );

    // mapper_a's umount failure must stop its own luks.close (same ordering
    // close::run enforces) but must not stop mapper_b's full sequence.
    assert_eq!(
        log.borrow().iter().filter(|c| *c == "umount").count(),
        2,
        "expected umount to be attempted for both mappings, log: {:?}",
        log.borrow()
    );
    assert_eq!(
        log.borrow().iter().filter(|c| *c == "close").count(),
        1,
        "expected only mapper_b's close to run, log: {:?}",
        log.borrow()
    );
}

#[test]
fn zero_open_mappings_yields_ok_empty_vec_with_no_further_port_calls() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let result = close_all::run(false, &|_| {}, &luks, &fido2, &fs);

    assert!(
        matches!(result, Ok(ref v) if v.is_empty()),
        "expected Ok(vec![]), got {result:?}"
    );
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "list_open_mappings".to_string()
        ]
    );
}

#[test]
fn preflight_failure_short_circuits_before_list_open_mappings_is_called() {
    let log = new_call_log();
    let luks = FakeLuksBackend::failing(&["cryptsetup"]).with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let result = close_all::run(false, &|_| {}, &luks, &fido2, &fs);

    assert!(matches!(result, Err(DomainError::PreflightFailed(_))));
    assert_eq!(
        *log.borrow(),
        vec!["check_prerequisites".to_string()],
        "no port call beyond preflight's own check_prerequisites should run before preflight fails"
    );
}

#[test]
fn skip_hooks_true_skips_hooks_for_every_mapping_in_the_batch() {
    let log = new_call_log();
    let mapper_a = mapper("vault-aaaa", "/volume/a.img");
    let mapper_b = mapper("vault-bbbb", "/volume/b.img");

    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_open_mappings(vec![mapper_a.clone(), mapper_b.clone()]);
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_path_exists(true);

    let results = close_all::run(true, &|_| {}, &luks, &fido2, &fs).expect("expected Ok");

    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|(_, r)| r.is_ok()));

    // No `mount_point_of`/`path_exists` at all — hooks step never runs for
    // either mapping when `skip_hooks` is true.
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "list_open_mappings".to_string(),
            "umount".to_string(),
            "close".to_string(),
            "umount".to_string(),
            "close".to_string(),
        ]
    );
}

#[test]
fn list_open_mappings_failure_propagates_as_close_alls_own_err_distinct_from_a_per_mapping_err() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_failure_at("list_open_mappings");
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let result = close_all::run(false, &|_| {}, &luks, &fido2, &fs);

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "list_open_mappings".to_string()
        ]
    );
}
