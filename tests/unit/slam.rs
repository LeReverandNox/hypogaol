use std::path::PathBuf;

use hypogaol::domain::errors::DomainError;
use hypogaol::domain::types::{MapperHandle, Pid, Signal};
use hypogaol::domain::workflows::slam;

use crate::fakes::{new_call_log, FakeFido2Backend, FakeFilesystemBackend, FakeLuksBackend};

fn mapper(name: &str, source_path: &str) -> MapperHandle {
    MapperHandle {
        name: name.to_string(),
        source_path: PathBuf::from(source_path),
    }
}

#[test]
fn escalates_through_sigterm_sighup_sigkill_until_umount_succeeds() {
    let log = new_call_log();
    let mapper_a = mapper("vault-aaaa", "/volume/a.img");

    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_open_mappings(vec![mapper_a.clone()]);
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_umount_fail_times(2)
        .with_processes_using(vec![Pid(111)]);

    let results = slam::run(&|_| {}, &luks, &fido2, &fs).expect("expected Ok");

    assert_eq!(results.len(), 1);
    assert!(
        results[0].1.is_ok(),
        "expected mapping to eventually close, got {:?}",
        results[0].1
    );

    assert_eq!(
        fs.signal_calls(),
        vec![(Pid(111), Signal::Sigterm), (Pid(111), Signal::Sighup)],
        "expected escalation to stop as soon as it clears, never reaching SIGKILL"
    );

    // mount_point_of runs once for the hooks step and once, separately, for
    // escalation prep — never once per round.
    assert_eq!(
        log.borrow()
            .iter()
            .filter(|c| *c == "mount_point_of")
            .count(),
        2,
        "expected exactly two mount_point_of calls, log: {:?}",
        log.borrow()
    );
}

#[test]
fn no_holders_remaining_stops_escalation_and_reports_that_mappings_failure() {
    let log = new_call_log();
    let mapper_a = mapper("vault-aaaa", "/volume/a.img");

    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_open_mappings(vec![mapper_a.clone()]);
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_umount_fail_times(u32::MAX);

    let results = slam::run(&|_| {}, &luks, &fido2, &fs).expect("expected Ok");

    assert_eq!(results.len(), 1);
    assert!(
        results[0].1.is_err(),
        "expected the mapping to be reported as failed, got {:?}",
        results[0].1
    );
    assert!(
        fs.signal_calls().is_empty(),
        "expected zero signal_process calls when no holders are ever reported, got {:?}",
        fs.signal_calls()
    );
}

#[test]
fn hooks_step_runs_exactly_once_never_repeated_across_escalation_rounds() {
    let log = new_call_log();
    let mapper_a = mapper("vault-aaaa", "/volume/a.img");

    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_open_mappings(vec![mapper_a.clone()]);
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_umount_fail_times(1)
        .with_processes_using(vec![Pid(222)]);

    let results = slam::run(&|_| {}, &luks, &fido2, &fs).expect("expected Ok");

    assert!(results[0].1.is_ok(), "got {:?}", results[0].1);

    // The hooks step's own call signature (mount_point_of + 2x path_exists,
    // both hooks files absent) appears exactly once, not duplicated per
    // escalation round.
    assert_eq!(
        log.borrow()
            .iter()
            .filter(|c| *c == "mount_point_of")
            .count(),
        2,
        "expected hooks-step mount_point_of + escalation-prep mount_point_of, log: {:?}",
        log.borrow()
    );
    assert_eq!(
        log.borrow().iter().filter(|c| *c == "path_exists").count(),
        2,
        "expected exactly 2 path_exists calls (exec-hooks, bind-hooks), not repeated per round, log: {:?}",
        log.borrow()
    );
}

#[test]
fn one_mappings_never_clearing_does_not_stop_the_batch() {
    let log = new_call_log();
    let mapper_a = mapper("vault-aaaa", "/volume/a.img");
    let mapper_b = mapper("vault-bbbb", "/volume/b.img");

    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_open_mappings(vec![mapper_a.clone(), mapper_b.clone()]);
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    // mapper_a's umount always fails and no holders are ever reported, so its
    // escalation breaks immediately every time (mirrors the previous test's
    // "never clears" scenario) — keyed to mapper_a only, so mapper_b's own
    // umount call is entirely unaffected (Story 4.5's per-mapping selective
    // failure mechanism; `with_umount_fail_times` is deliberately global, not
    // per-mapping, so it can't isolate the failure to mapper_a alone in a
    // multi-mapping batch).
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_umount_failure_for(&mapper_a.name);

    let results = slam::run(&|_| {}, &luks, &fido2, &fs).expect("expected Ok");

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

    // mapper_b's full sequence (hooks, umount, close) still ran.
    assert_eq!(
        log.borrow().iter().filter(|c| *c == "close").count(),
        1,
        "expected only mapper_b's close to run, log: {:?}",
        log.borrow()
    );
}

#[test]
fn zero_open_mappings_yields_ok_empty_vec() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing().with_log(log.clone());
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let result = slam::run(&|_| {}, &luks, &fido2, &fs);

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

    let result = slam::run(&|_| {}, &luks, &fido2, &fs);

    assert!(matches!(result, Err(DomainError::PreflightFailed(_))));
    assert_eq!(
        *log.borrow(),
        vec!["check_prerequisites".to_string()],
        "no port call beyond preflight's own check_prerequisites should run before preflight fails"
    );
}

#[test]
fn list_open_mappings_failure_propagates_as_slams_own_err() {
    let log = new_call_log();
    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_failure_at("list_open_mappings");
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let result = slam::run(&|_| {}, &luks, &fido2, &fs);

    assert!(result.is_err(), "expected Err, got {result:?}");
    assert_eq!(
        *log.borrow(),
        vec![
            "check_prerequisites".to_string(),
            "list_open_mappings".to_string()
        ]
    );
}

#[test]
fn lock_contention_on_every_mapping_reports_each_mappings_own_failure_without_stopping_the_batch() {
    let log = new_call_log();
    let mapper_a = mapper("vault-aaaa", "/volume/a.img");
    let mapper_b = mapper("vault-bbbb", "/volume/b.img");

    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_open_mappings(vec![mapper_a.clone(), mapper_b.clone()]);
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing()
        .with_log(log.clone())
        .with_lock_contention();

    let results = slam::run(&|_| {}, &luks, &fido2, &fs).expect("expected Ok");

    assert_eq!(results.len(), 2);
    assert!(
        results
            .iter()
            .all(|(_, r)| matches!(r, Err(DomainError::LockContention(_)))),
        "expected every mapping to report its own LockContention, got {results:?}"
    );
    assert_eq!(
        log.borrow()
            .iter()
            .filter(|c| *c == "list_open_mappings")
            .count(),
        1,
        "discovery itself must run exactly once, unaffected by per-mapping lock contention"
    );
}

#[test]
fn acquires_one_lock_per_mapping_using_each_mappings_own_source_path() {
    let log = new_call_log();
    let mapper_a = mapper("vault-aaaa", "/volume/a.img");
    let mapper_b = mapper("vault-bbbb", "/volume/b.img");

    let luks = FakeLuksBackend::passing()
        .with_log(log.clone())
        .with_open_mappings(vec![mapper_a.clone(), mapper_b.clone()]);
    let fido2 = FakeFido2Backend::passing().with_log(log.clone());
    let fs = FakeFilesystemBackend::passing().with_log(log.clone());

    let results = slam::run(&|_| {}, &luks, &fido2, &fs).expect("expected Ok");

    assert!(results.iter().all(|(_, r)| r.is_ok()));
    assert_eq!(
        fs.lock_target_calls(),
        vec![mapper_a.source_path.clone(), mapper_b.source_path.clone()],
        "expected one lock per mapping, using that mapping's own source_path, not one shared lock for the batch"
    );
}
