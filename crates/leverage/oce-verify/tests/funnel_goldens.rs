//! The funnel port's own bits, pinned, and its determinism: the golden and determinism pillars
//! of the envelope.
//!
//! `tests/funnel_differential.rs` holds the port equal to pyfunnel as text, and sixteen digits
//! do not identify a double. Here every case's five curves and three figures are compared with
//! a checked-in golden **bit for bit** (`tests/fixtures/funnel/golden/<case>.txt`, hex bits),
//! so a change in the last bit of any corner, on any target, fails. The goldens are written
//! only by the crate's bless switch (`OCE_BLESS_VERIFY=1 cargo nextest run -p oce-verify`).
//! They are the port's output, so they prove nothing about funnel by themselves; the
//! differential test does that.

mod support;

use std::collections::BTreeSet;

use oce_verify::{CompareError, CompareLimits, FunnelResult, Series, Tolerances, funnel};

use support::funnel_cases::{
    Case, HOSTILE_CASES, UPSTREAM_CASES, cases, compare_or_bless_bits, curve_bits, optional_bits,
    root,
};

fn port(case: &Case) -> Result<FunnelResult, CompareError> {
    funnel(
        Series::new(&case.reference.0, &case.reference.1),
        Series::new(&case.test.0, &case.test.1),
        &case.tolerances,
        &CompareLimits::default(),
    )
}

/// An outcome as hex bits: the five curves and the figures of a result, or a refusal's code and
/// what it carries.
fn bits(outcome: &Result<FunnelResult, CompareError>) -> String {
    match outcome {
        Ok(result) => {
            let mut text = String::new();
            for (name, curve) in [
                ("reference", &result.reference),
                ("test", &result.test),
                ("lower_bound", &result.lower_bound),
                ("upper_bound", &result.upper_bound),
                ("errors", &result.errors),
            ] {
                text.push_str(&curve_bits(name, curve));
            }
            text.push_str(&format!(
                "[figures]\nenvelope {}\ncompared {}\nmax_error {:016x}\nmax_error_x {}\nfirst_error_x {}\n",
                result.envelope,
                result.compared,
                result.max_error.to_bits(),
                optional_bits(result.max_error_x),
                optional_bits(result.first_error_x),
            ));
            text
        }
        Err(CompareError::MismatchedEndpoints {
            end,
            reference,
            test,
        }) => format!(
            "[refused]\ncode mismatched-endpoints\nend {}\nreference {:016x}\ntest {:016x}\n",
            end.as_str(),
            reference.to_bits(),
            test.to_bits()
        ),
        Err(other) => format!("[refused]\ncode {}\n", other.code()),
    }
}

fn golden(case: &Case, outcome: &Result<FunnelResult, CompareError>) -> String {
    format!(
        "# oce-verify funnel golden: {}/{}\n# Hex IEEE-754 bits, `x y` per point. Written by \
         OCE_BLESS_VERIFY, never by hand.\n{}",
        case.set,
        case.name,
        bits(outcome)
    )
}

#[test]
fn every_case_reproduces_its_golden_bit_for_bit() {
    let directory = root().join("golden");
    let mut written = BTreeSet::new();
    for case in cases() {
        let file = format!("{}.txt", case.name);
        compare_or_bless_bits(&directory.join(&file), &golden(&case, &port(&case)));
        written.insert(file);
    }
    assert_eq!(written.len(), UPSTREAM_CASES.len() + HOSTILE_CASES);

    // No golden without a case: a stale file would look like coverage.
    let present: BTreeSet<String> = std::fs::read_dir(&directory)
        .expect("the golden directory")
        .map(|entry| {
            entry
                .expect("a directory entry")
                .file_name()
                .into_string()
                .expect("a UTF-8 file name")
        })
        .collect();
    assert_eq!(present, written);
}

#[test]
fn a_golden_that_differs_in_one_bit_fails_the_comparison() {
    // Under the bless switch the comparison writes instead; the control would overwrite a
    // golden with an edited copy, so it does not run then.
    if oce_bless::enabled(support::BLESS_SWITCH) {
        return;
    }
    let case = support::funnel_cases::case("success1");
    let text = golden(&case, &port(&case));
    let path = root().join("golden/success1.txt");
    compare_or_bless_bits(&path, &text);

    // Flip the last bit of the last error.
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let line = lines
        .iter()
        .rposition(|line| line.len() == 33 && !line.starts_with('['))
        .expect("a point line");
    let last = lines[line].pop().expect("a hex digit");
    lines[line].push(if last == '0' { '1' } else { '0' });
    let edited = lines.join("\n") + "\n";
    assert_ne!(edited, text);
    let outcome = std::panic::catch_unwind(|| compare_or_bless_bits(&path, &edited));
    let message = *outcome
        .expect_err("a one-bit difference is a failure")
        .downcast::<String>()
        .expect("a formatted panic");
    assert!(
        message.contains(&format!("at line {}", line + 1)),
        "{message}"
    );
}

#[test]
fn two_runs_give_identical_bits_for_every_case() {
    for case in cases() {
        let first = bits(&port(&case));
        for _ in 0..3 {
            assert_eq!(bits(&port(&case)), first, "{}", case.name);
        }
    }
}

#[test]
fn the_result_does_not_depend_on_where_or_how_the_caller_holds_the_series() {
    for case in cases() {
        let expected = bits(&port(&case));
        let run = |reference: (&[f64], &[f64]), test: (&[f64], &[f64])| {
            bits(&funnel(
                Series::new(reference.0, reference.1),
                Series::new(test.0, test.1),
                &case.tolerances,
                &CompareLimits::default(),
            ))
        };

        // The same samples as windows into larger buffers, at several offsets, surrounded by
        // values no comparison may read: nothing outside the slices reaches the result.
        for offset in [0_usize, 1, 3, 8] {
            let window = |values: &[f64], filler: f64| -> Vec<f64> {
                let mut buffer = vec![filler; offset];
                buffer.extend_from_slice(values);
                buffer.extend(std::iter::repeat_n(filler, 5));
                buffer
            };
            let buffers = [
                window(&case.reference.0, f64::NAN),
                window(&case.reference.1, f64::INFINITY),
                window(&case.test.0, -1e300),
                window(&case.test.1, f64::NAN),
            ];
            let slice = |index: usize, len: usize| &buffers[index][offset..offset + len];
            let (reference_len, test_len) = (case.reference.0.len(), case.test.0.len());
            assert_eq!(
                run(
                    (slice(0, reference_len), slice(1, reference_len)),
                    (slice(2, test_len), slice(3, test_len))
                ),
                expected,
                "{} at offset {offset}",
                case.name
            );
        }

        // The same samples gathered from chunks of several sizes, as a caller reading a trend
        // in blocks would hold them.
        for chunk in [1_usize, 2, 7, 4096] {
            let gathered = |values: &[f64]| -> Vec<f64> {
                let mut whole = Vec::new();
                for block in values.chunks(chunk) {
                    whole.extend_from_slice(block);
                }
                whole
            };
            let (rx, ry) = (gathered(&case.reference.0), gathered(&case.reference.1));
            let (tx, ty) = (gathered(&case.test.0), gathered(&case.test.1));
            assert_eq!(
                run((&rx, &ry), (&tx, &ty)),
                expected,
                "{} in chunks of {chunk}",
                case.name
            );
        }
    }
}

#[test]
fn the_reference_and_test_curves_of_a_result_are_the_callers_series_bit_for_bit() {
    for case in cases() {
        let Ok(result) = port(&case) else {
            continue;
        };
        let same = |ours: &[f64], theirs: &[f64]| {
            ours.len() == theirs.len()
                && ours
                    .iter()
                    .zip(theirs)
                    .all(|(a, b)| a.to_bits() == b.to_bits())
        };
        assert!(
            same(result.reference.x(), &case.reference.0),
            "{}",
            case.name
        );
        assert!(
            same(result.reference.y(), &case.reference.1),
            "{}",
            case.name
        );
        assert!(same(result.test.x(), &case.test.0), "{}", case.name);
        assert!(same(result.test.y(), &case.test.1), "{}", case.name);
        assert!(same(result.errors.x(), &case.test.0), "{}", case.name);
    }
}

#[test]
fn lowering_a_limit_that_is_not_reached_does_not_change_a_bit() {
    let case = support::funnel_cases::case("noisy_wide_rectangles");
    let expected = bits(&port(&case));
    // Its two series have 240 points and its loop removal needs 2,687 units.
    let limits = CompareLimits {
        max_funnel_points: 240,
        max_aligned_grid_points: 0,
        max_loop_removal_work: 2_687,
    };
    let lowered = funnel(
        Series::new(&case.reference.0, &case.reference.1),
        Series::new(&case.test.0, &case.test.1),
        &case.tolerances,
        &limits,
    );
    assert_eq!(bits(&lowered), expected);
    // Tolerances are the caller's and are not changed by a comparison.
    assert_eq!(
        case.tolerances,
        Tolerances {
            rtolx: 0.01,
            rtoly: 0.01,
            ..Tolerances::ZERO
        }
    );
}
