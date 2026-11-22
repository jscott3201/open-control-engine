//! `oce verify --commission` on a reference with no `sampling`, compared by change of value
//! (issue #746, ruled by the owner on 2026-11-22; commissioning note, "As landed: the
//! change-of-value comparison"). Before it, such a reference was refused `sampling-required`.
//!
//! The receipt on stdout is `oce-verify`'s golden for the same case, byte for byte, and `--out
//! --bounds` writes the step series and the funnel's bounds, the repeated `x` of each step
//! included, as the library keeps them. Regenerate the transcripts with `OCE_BLESS_CLI=1`.

mod support;

use std::fs;

use oce_verify::{CommissionOptions, CommissioningSpec, SpecInputs, SpecLimits};
use support::commission::{change_of_value_file, change_of_value_layout, run_file, subject};
use support::{case_in, oce_in};

const SPEC: &str = "spec.json";

#[test]
fn a_reference_with_no_sampling_prints_its_verdict_and_the_librarys_receipt() {
    for (case, exit) in [("late_step", 1), ("unlogged_toggle", 1), ("exact_steps", 0)] {
        let dir = change_of_value_layout(&format!("commission_change_{case}"), case);
        case_in(
            &dir,
            &format!("verify_commission_change_of_value_{case}"),
            exit,
            &["verify", "--commission", SPEC],
        );
        let run = oce_in(&dir, &["verify", "--commission", SPEC], b"");
        assert!(
            run.stdout == run_file(&format!("change_of_value/{case}.receipt.json")),
            "{case}: stdout is not the library's golden receipt\n{}",
            run.stdout_text()
        );
        assert!(run.stderr.is_empty(), "{case}: {}", run.transcript());
    }
}

#[test]
fn bounds_writes_the_step_series_and_the_band_as_the_library_keeps_them() {
    let case = "late_step";
    let dir = change_of_value_layout("commission_change_bounds", case);
    fs::create_dir(dir.join("out")).expect("out");
    let run = oce_in(
        &dir,
        &["verify", "--commission", SPEC, "--out", "out", "--bounds"],
        b"",
    );
    assert_eq!(run.code, 1, "{}", run.transcript());
    let spec = CommissioningSpec::from_json_bytes(
        &change_of_value_file(case, "spec.json"),
        &SpecLimits::default(),
    )
    .expect("the spec loads");
    let mut files = vec![("points.json".to_owned(), run_file("points.json"))];
    for name in ["main.csv", "limit.csv", "outputs.csv", "fan.csv"] {
        files.push((name.to_owned(), change_of_value_file(case, name)));
    }
    let mut options = CommissionOptions::default();
    options.bounds = true;
    let library = oce_verify::run_commissioning(
        &spec,
        0,
        &SpecInputs {
            cxf: subject("plant"),
            files,
        },
        &options,
    )
    .expect("the library runs");
    for (k, curves) in library.curves.iter().enumerate() {
        let errors =
            fs::read_to_string(dir.join(format!("out/outputs/{k}/errors.csv"))).expect("errors");
        assert_eq!(errors, curves.errors.to_csv(), "output {k}");
        let Some(bounds) = &curves.bounds else {
            continue;
        };
        for (name, curve) in ["reference", "test", "lowerBound", "upperBound"]
            .iter()
            .zip([
                &bounds.reference,
                &bounds.test,
                &bounds.lower,
                &bounds.upper,
            ])
        {
            let written = fs::read_to_string(dir.join(format!("out/outputs/{k}/{name}.csv")))
                .expect("a bound file");
            assert_eq!(written, curve.to_csv(), "output {k} {name}");
        }
    }
    // The setpoint (output 0) steps at 10 s in the engine, the reference: the held value is
    // carried to 10 s, then the new value starts there.
    let reference = fs::read_to_string(dir.join("out/outputs/0/reference.csv")).expect("csv");
    assert!(
        reference.contains("\n10,294.15\n10,295.15\n"),
        "{reference}"
    );
    let test = fs::read_to_string(dir.join("out/outputs/0/test.csv")).expect("csv");
    assert!(test.contains("\n13,294.15\n13,295.15\n"), "{test}");
}
