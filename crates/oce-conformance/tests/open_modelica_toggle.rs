//! Scoped stateful OpenModelica differential for `CDL.Logical.Toggle`.

mod block_harness;
#[path = "open_modelica_toggle/support.rs"]
mod scoped;

use oce_conformance::{ComparisonResult, DriveMode};

#[test]
fn external_toggle_matches_facade_and_independent_recurrence() {
    let reference = scoped::read_reference("toggle.canonical.csv").unwrap();
    let outcome = scoped::evaluate(&reference);
    let repeated = scoped::evaluate(&reference);
    assert_eq!(outcome.producer, "OpenModelica 1.25.1");
    assert_eq!(outcome.class, "CDL.Logical.Toggle");
    assert_eq!(outcome.scenario, scoped::SCENARIO);
    assert!(outcome.manifest.ends_with("logical_toggle/manifest.json"));
    assert_eq!(outcome.load_warning_count, 0);
    assert_eq!(outcome.engine.len(), scoped::TIME_BITS.len());
    assert_eq!(outcome.times, scoped::TIME_BITS);
    let DriveMode::EventAligned { instants } = &outcome.drive_mode else {
        panic!("Toggle must use explicit event-aligned facade execution");
    };
    assert_eq!(
        instants
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>(),
        scoped::TIME_BITS
    );
    let ComparisonResult::Exact(exact) = &outcome.comparison else {
        panic!("Toggle comparison must be exact");
    };
    assert!(exact.passed, "exact differential mismatch: {exact:?}");
    assert_eq!(exact.compared_points, scoped::TIME_BITS.len());
    assert_eq!(exact.first_mismatch, None);
    assert_eq!(outcome.times, repeated.times);
    assert_eq!(outcome.engine, repeated.engine);
    assert_eq!(outcome.drive_mode, repeated.drive_mode);
    assert_eq!(outcome.load_warning_count, repeated.load_warning_count);
    let discrepancies = scoped::recurrence_discrepancies(&reference, &outcome.engine).unwrap();
    assert!(discrepancies.is_empty(), "{discrepancies:?}");
}

#[test]
fn latch_class_substitution_first_fails_at_the_pinned_rise() {
    let reference = scoped::read_reference("latch.canonical.csv").unwrap();
    let outcome = scoped::evaluate(&reference);
    let ComparisonResult::Exact(exact) = outcome.comparison else {
        panic!("Latch control comparison must be exact");
    };
    assert!(!exact.passed);
    let mismatch = exact.first_mismatch.unwrap();
    assert_eq!(mismatch.index, 3);
    assert_eq!(mismatch.x.to_bits(), scoped::TIME_BITS[3]);
    assert_eq!(mismatch.expected, 1.0);
    assert_eq!(mismatch.actual, 0.0);
    let discrepancies = scoped::recurrence_discrepancies(&reference, &outcome.engine).unwrap();
    assert_eq!(
        discrepancies
            .iter()
            .find(|item| item.party == scoped::Party::OpenModelica)
            .map(|item| item.row),
        Some(3)
    );
}

#[test]
fn wrong_recurrences_fail_at_discriminating_rows() {
    let reference = scoped::read_reference("toggle.canonical.csv").unwrap();
    assert_eq!(
        scoped::initial_false_mismatch_rows(&reference).unwrap(),
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    );
    assert_eq!(
        scoped::every_input_change_mismatch_rows(&reference).unwrap(),
        [1, 2, 3, 4, 9, 10, 11, 14, 15, 17, 18, 19, 20, 21]
    );
    assert_eq!(
        scoped::ignore_all_clear_mismatch_rows(&reference).unwrap(),
        [11, 12, 13]
    );
    assert_eq!(
        scoped::ignore_clear_on_simultaneous_rise_mismatch_rows(&reference).unwrap(),
        [14]
    );
}

#[test]
fn canonical_reference_is_bounded_before_parse() {
    let at_limit = vec![b' '; scoped::MAX_REFERENCE_BYTES];
    assert!(
        !scoped::parse_reference_bytes(&at_limit)
            .unwrap_err()
            .contains("exceeds")
    );
    assert!(
        scoped::parse_reference_bytes(&[at_limit, vec![b' ']].concat())
            .unwrap_err()
            .contains("exceeds")
    );
}

#[cfg(unix)]
#[test]
fn canonical_reference_path_rejects_symlink_and_fifo_without_blocking() {
    use std::os::unix::fs::symlink;

    let directory = claim_temp_dir("oce-toggle-reference-path");
    let fixture = directory.join("fixture.csv");
    let link = directory.join("link.csv");
    let fifo = directory.join("input.fifo");
    std::fs::write(
        &fixture,
        b"#1\n# columns: time u clr y\ndouble x(1,4)\n0 1.0 0.0 1.0\n",
    )
    .unwrap();
    symlink(&fixture, &link).unwrap();
    assert!(
        scoped::read_reference_path(&link)
            .unwrap_err()
            .contains("non-symlink")
    );
    assert!(
        scoped::read_reference_path(&directory)
            .unwrap_err()
            .contains("regular")
    );
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        scoped::read_reference_path(&fifo)
            .unwrap_err()
            .contains("regular")
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
fn claim_temp_dir(prefix: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().canonicalize().unwrap();
    for nonce in 0_u32..1024 {
        let candidate = root.join(format!("{prefix}-{}-{nonce}", std::process::id()));
        match std::fs::create_dir(&candidate) {
            Ok(()) => return candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("cannot claim temporary directory: {error}"),
        }
    }
    panic!("cannot claim temporary directory")
}
