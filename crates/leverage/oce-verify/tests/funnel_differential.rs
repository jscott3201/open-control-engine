//! The funnel port against the pinned pyfunnel (commissioning note, decision 13): the oracle
//! pillar of the envelope.
//!
//! For every case of the fixture (`tests/fixtures/funnel/`: funnel's own 17 test directories,
//! the hostile set and the recorded Building 33 day), the port's five curves, printed with this crate's `%.16g` writer, equal
//! the five files `pyfunnel==2.1.1` wrote **byte for byte**. Where pyfunnel refuses a case, the
//! port refuses it for the same reason at the same end.
//!
//! The claim is textual and no stronger: `%.16g` keeps sixteen significant digits and a double
//! needs seventeen, so two curves that print the same can differ in their last bit. pyfunnel
//! cannot emit more, which is why the bar is text equality and why there is no numeric tolerance
//! anywhere in this file: a tolerance would hide a porting error. The port's exact bits are
//! pinned separately, as goldens (`tests/funnel_goldens.rs`).
//!
//! Nothing here runs Python or reaches the network: `expected/` is checked in, written once by
//! `tests/fixtures/funnel/capture/capture.sh` on x86_64 Linux.

mod support;

use oce_verify::{CompareError, CompareLimits, Series, funnel};

use support::funnel_cases::{
    CURVE_FILES, Case, Expected, HOSTILE_CASES, RECORDED_CASES, UPSTREAM_CASES, cases, curves,
    expected, recorded_cases,
};

/// The port's outcome for a case, under the default limits.
fn port(case: &Case) -> Result<oce_verify::FunnelResult, CompareError> {
    funnel(
        Series::new(&case.reference.0, &case.reference.1),
        Series::new(&case.test.0, &case.test.1),
        &case.tolerances,
        &CompareLimits::default(),
    )
}

/// The first line at which two texts differ, for a failure message.
fn first_difference(ours: &str, theirs: &str) -> String {
    let mut ours_lines = ours.split('\n');
    let mut theirs_lines = theirs.split('\n');
    let mut line = 1;
    loop {
        match (ours_lines.next(), theirs_lines.next()) {
            (Some(a), Some(b)) if a == b => line += 1,
            (None, None) => return "no differing line".to_owned(),
            (a, b) => return format!("line {line}: port {a:?}, pyfunnel {b:?}"),
        }
    }
}

#[test]
fn every_case_prints_the_five_files_pyfunnel_wrote_or_is_refused_as_pyfunnel_refuses_it() {
    let mut mismatches = Vec::new();
    let (mut compared, mut refused) = (0, 0);
    for case in cases() {
        match (expected(&case.name), port(&case)) {
            (Expected::Curves(files), Ok(result)) => {
                compared += 1;
                for ((curve, file), theirs) in curves(&result).iter().zip(CURVE_FILES).zip(&files) {
                    let ours = curve.to_csv();
                    if ours != *theirs {
                        mismatches.push(format!(
                            "{}/{}/{file}: {}",
                            case.set,
                            case.name,
                            first_difference(&ours, theirs)
                        ));
                    }
                }
            }
            (Expected::Refused(end), Err(CompareError::MismatchedEndpoints { end: ours, .. }))
                if ours == end =>
            {
                refused += 1;
            }
            (Expected::Curves(_), Err(error)) => mismatches.push(format!(
                "{}/{}: pyfunnel wrote curves, the port refused with {}",
                case.set,
                case.name,
                error.code()
            )),
            (Expected::Refused(end), outcome) => mismatches.push(format!(
                "{}/{}: pyfunnel refused at the {} x, the port gave {:?}",
                case.set,
                case.name,
                end.as_str(),
                outcome.map(|result| result.compared)
            )),
        }
    }
    assert!(
        mismatches.is_empty(),
        "the port differs from pyfunnel:\n{}",
        mismatches.join("\n")
    );
    // The comparison is not vacuous: every case took one of the two arms.
    assert_eq!(compared + refused, UPSTREAM_CASES.len() + HOSTILE_CASES);
    assert_eq!((compared, refused), (60, 6));
}

#[test]
fn of_the_upstream_cases_funnel_itself_refuses_exactly_fail2_and_fail6() {
    // Upstream's own test list expects "maximum x values are different" from these two
    // (`CMakeLists.txt`); the other fifteen produce curves.
    let refused: Vec<&str> = UPSTREAM_CASES
        .into_iter()
        .filter(|name| matches!(expected(name), Expected::Refused(_)))
        .collect();
    assert_eq!(refused, ["fail2", "fail6"]);
    for name in refused {
        assert!(matches!(
            expected(name),
            Expected::Refused(oce_verify::SeriesEnd::Last)
        ));
    }
}

#[test]
fn the_verdict_figures_agree_with_the_error_file_pyfunnel_wrote() {
    // Read from pyfunnel's `errors.csv` alone: an instant is outside the band where its error
    // is not printed as `0`. The port's figures are read from its own error curve.
    let mut leaving = Vec::new();
    for case in cases() {
        let Expected::Curves(files) = expected(&case.name) else {
            continue;
        };
        let result = port(&case).expect("pyfunnel wrote curves, so the port compares");
        let rows: Vec<(&str, &str)> = files[4]
            .lines()
            .skip(1)
            .map(|line| line.split_once(',').expect("an `x,y` row"))
            .collect();
        let first_error = rows.iter().find(|(_, error)| *error != "0");

        assert_eq!(result.compared, rows.len(), "{}", case.name);
        assert_eq!(result.compared, case.test.0.len(), "{}", case.name);
        assert_eq!(
            result.within_envelope(),
            first_error.is_none(),
            "{}",
            case.name
        );
        assert_eq!(
            result.first_error_x.map(oce_verify::format_g16).as_deref(),
            first_error.map(|(x, _)| *x),
            "{}",
            case.name
        );
        assert_eq!(
            result.max_error_x.is_some(),
            first_error.is_some(),
            "{}",
            case.name
        );
        if case.set == "upstream" && first_error.is_some() {
            leaving.push(case.name.clone());
        }
    }
    // Of upstream's fifteen compared cases these six leave the band. `fail4` does not: it fails
    // upstream by an inaccessible output folder, and its numbers are `success5`'s. `success6`
    // and `success8` do: upstream's "success" is that the tool reproduces its stored numbers.
    assert_eq!(
        leaving,
        [
            "fail1",
            "fail3",
            "fail5",
            "should_succeed1",
            "success6",
            "success8"
        ]
    );
}

#[test]
fn the_recorded_building_33_day_prints_the_five_files_pyfunnel_wrote() {
    // One day of the Building 33 worked example at 5 s (issue #755): the engine series of the
    // transcribed `CoolingCoilValve` as the reference and the trended valve as the test, under
    // `atolx` 1 s and `rtoly` 0.01. 17,281 points a series, and a band that the trend leaves:
    // the case exercises the corners and the loop removal on recorded data.
    let recorded = recorded_cases();
    assert_eq!(recorded.len(), RECORDED_CASES.len());
    for case in recorded {
        assert_eq!(case.reference.0.len(), 17_281, "{}", case.name);
        let Expected::Curves(files) = expected(&case.name) else {
            panic!("{}: pyfunnel wrote curves", case.name);
        };
        let result = port(&case).unwrap_or_else(|error| panic!("{}: {}", case.name, error.code()));
        for ((curve, file), theirs) in curves(&result).iter().zip(CURVE_FILES).zip(&files) {
            let ours = curve.to_csv();
            assert!(
                ours == *theirs,
                "recorded/{}/{file}: {}",
                case.name,
                first_difference(&ours, theirs)
            );
        }
        assert!(
            result.errors_at_test().iter().any(|error| *error > 0.0),
            "{}: the trend leaves the band somewhere",
            case.name
        );
    }
}
