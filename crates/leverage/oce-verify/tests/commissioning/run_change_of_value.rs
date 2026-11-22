//! The change-of-value comparison of a reference with no `sampling` (issue #746, ruled by the
//! owner on 2026-11-22; commissioning note, "As landed: the change-of-value comparison"):
//!
//! - **Oracle**: `derive_run.py <run fixture> change-of-value`, which shares no code with the
//!   crate, derives every figure, every instant and every exceedance of the cases under
//!   `tests/fixtures/commissioning/run/change_of_value/` into `change_of_value/oracle.txt`; the
//!   library's run of each case, printed in the same lines, equals it byte for byte, every
//!   number as the hex of its bits.
//! - **Determinism**: one receipt for two runs, replay chunks of 1, 7 and the whole run, and
//!   every order of the files.
//! - **The bounds before the replay** at their edge, and the owner's and orchestrator's readings
//!   of 2026-11-22 each pinned (`run_change_readings.rs` holds the reproductions and the trends
//!   that pass).

use oce_verify::{CommissionOptions, CommissionRun, CompareLimits, OutputReport};

use super::support;
use super::{body_of, commission, loaded};

/// The cases of the change-of-value fixture, in name order.
pub(super) const CASES: [&str; 19] = [
    "boolean_switch_on_a_frame",
    "deadband_analogue",
    "exact_steps",
    "late_step",
    "ltolx_late_step",
    "masked_and_uncovered",
    "never_changes",
    "no_sample_at_start",
    "one_sample_before_start",
    "rtoly_late_step",
    "sparse_mask",
    "step_beyond_tolerance",
    "step_one_tolerance_late",
    "step_within_tolerance",
    "trend_as_reference",
    "trend_ends_before_stop",
    "unlogged_pulse",
    "unlogged_toggle",
    "value_at_stop_only",
];

/// The files of a case, in the order the spec names them, and the mapping beside them.
const FILES: [&str; 4] = ["main.csv", "limit.csv", "outputs.csv", "fan.csv"];

fn case_file(case: &str, name: &str) -> Vec<u8> {
    support::fixture_bytes(&format!("commissioning/run/change_of_value/{case}/{name}"))
}

/// The spec text of `case` with each `(old, new)` of `edits` replaced.
pub(super) fn case_spec_text(case: &str, edits: &[(&str, &str)]) -> String {
    String::from_utf8(support::edited(&case_file(case, "spec.json"), edits)).expect("UTF-8")
}

/// The files of `case`, `replaced` (path, bytes) in place of the case's own, in `order`.
pub(super) fn case_inputs(case: &str, replaced: &[(&str, &[u8])]) -> oce_verify::SpecInputs {
    let mut files: Vec<(String, Vec<u8>)> = vec![(
        "points.json".to_owned(),
        support::fixture_bytes("commissioning/run/points.json"),
    )];
    for name in FILES {
        let bytes = replaced
            .iter()
            .find(|(path, _)| *path == name)
            .map_or_else(|| case_file(case, name), |(_, bytes)| bytes.to_vec());
        files.push((name.to_owned(), bytes));
    }
    oce_verify::SpecInputs {
        cxf: support::subject("plant"),
        files,
    }
}

/// Run `case` with its own files under `options`.
pub(super) fn run_case_with(case: &str, options: &CommissionOptions) -> CommissionRun {
    let spec = loaded(&case_spec_text(case, &[]));
    commission(&spec, &case_inputs(case, &[]), options)
        .unwrap_or_else(|error| panic!("{case} runs: {} {error}", error.code()))
}

/// Run `case` with its own files.
pub(super) fn run_case(case: &str) -> CommissionRun {
    run_case_with(case, &CommissionOptions::default())
}

/// The report of the output whose canonical path ends with `.{name}` in `run`.
pub(super) fn report<'r>(run: &'r CommissionRun, name: &str) -> &'r OutputReport {
    super::output_named(body_of(&run.receipt), name)
}

fn bits(x: f64) -> String {
    if x.is_nan() {
        "nan".to_owned()
    } else {
        format!("{:016x}", x.to_bits())
    }
}

/// The run of `case` in `derive_run.py`'s lines.
fn oracle_lines(case: &str, run: &CommissionRun) -> Vec<String> {
    let mut lines = vec![format!("case\t{case}")];
    for name in ["TSet", "enOut", "limOut", "nOut"] {
        let output = report(run, name);
        let curves = super::curves_named(run, name);
        let kind = if matches!(name, "TSet" | "limOut") {
            "real"
        } else {
            "discrete"
        };
        lines.push(format!("output\t{name}"));
        lines.push(format!("type\t{kind}"));
        lines.push(format!(
            "counts\t{}\t{}\t{}",
            output.compared, output.masked, output.uncovered
        ));
        lines.push(format!("status\t{}", output.status.as_str()));
        if let Some(worst) = output.worst {
            lines.push(format!(
                "worst\t{}\t{}",
                bits(worst.t),
                bits(worst.exceedance)
            ));
        }
        if let Some(first) = output.first_exceedance_t {
            lines.push(format!("first\t{}", bits(first)));
        }
        lines.push(format!("time\t{}", bits(output.exceedance_time_s)));
        if let Some(last) = output.last_sample_t {
            lines.push(format!("last_sample\t{}", bits(last)));
        }
        for interval in &output.masked_intervals {
            lines.push(format!(
                "masked_interval\t{}\t{}",
                bits(interval.from),
                bits(interval.to)
            ));
        }
        if let (Some(width), Some(height)) = (output.half_width, output.half_height) {
            lines.push(format!(
                "half_width\t{}\t{}",
                bits(width.min),
                bits(width.max)
            ));
            lines.push(format!(
                "half_height\t{}\t{}",
                bits(height.min),
                bits(height.max)
            ));
        }
        for masked in &output.masked_exceedance {
            lines.push(format!(
                "masked_exceedance\t{}\t{}",
                bits(masked.t),
                bits(masked.exceedance)
            ));
        }
        for (t, error) in curves.errors.points() {
            lines.push(format!("error\t{}\t{}", bits(t), bits(error)));
        }
    }
    lines
}

/// The cases whose receipts are golden, `change_of_value/<case>.receipt.json`.
const GOLDEN_CASES: [&str; 3] = ["exact_steps", "late_step", "unlogged_toggle"];

#[test]
fn every_case_matches_the_independent_derivation_bit_for_bit() {
    let mut text = String::from("# derive_run.py over the run fixture's change-of-value cases\n");
    for case in CASES {
        text.push_str(&oracle_lines(case, &run_case(case)).join("\n"));
        text.push('\n');
    }
    // The script also recomputes each golden receipt's id from its canonical text.
    for case in GOLDEN_CASES {
        text.push_str(&format!(
            "receipt\t{case}.receipt.json\t{}\n",
            run_case(case).receipt.header.receipt_id
        ));
    }
    let oracle = String::from_utf8(support::fixture_bytes(
        "commissioning/run/change_of_value/oracle.txt",
    ))
    .expect("UTF-8");
    assert_eq!(text, oracle);
}

#[test]
fn three_receipts_with_no_sampling_match_their_goldens_byte_for_byte() {
    for case in GOLDEN_CASES {
        let run = run_case(case);
        let receipt = run.receipt.to_canonical_json();
        assert!(!receipt.contains("\"sampling\""), "{case}");
        support::compare_or_bless(
            &support::fixture(&format!(
                "commissioning/run/change_of_value/{case}.receipt.json"
            )),
            &receipt,
        );
    }
}

#[test]
fn the_fixture_holds_exactly_the_listed_cases() {
    let directory = support::fixture("commissioning/run/change_of_value");
    let mut found: Vec<String> = std::fs::read_dir(&directory)
        .expect("read the cases")
        .map(|entry| entry.expect("an entry"))
        .filter(|entry| entry.file_type().expect("a type").is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    assert_eq!(found, CASES);
}

#[test]
fn two_runs_every_chunk_size_and_every_file_order_give_one_receipt() {
    for case in [
        "masked_and_uncovered",
        "deadband_analogue",
        "unlogged_toggle",
    ] {
        let first = run_case(case).receipt.to_canonical_json();
        assert_eq!(run_case(case).receipt.to_canonical_json(), first, "{case}");
        for chunk in [1, 7, usize::MAX] {
            let mut options = CommissionOptions::default();
            options.chunk_frames = chunk;
            assert_eq!(
                run_case_with(case, &options).receipt.to_canonical_json(),
                first,
                "{case} in chunks of {chunk}"
            );
        }
        let spec = loaded(&case_spec_text(case, &[]));
        let mut inputs = case_inputs(case, &[]);
        inputs.files.reverse();
        let reversed = commission(&spec, &inputs, &CommissionOptions::default())
            .expect("the reversed files run");
        assert_eq!(
            reversed.receipt.to_canonical_json(),
            first,
            "{case} reversed"
        );
    }
}

/// `never_changes` over `rows` seconds: the main input and the trended outputs both logged at
/// every second, the limit at the window's two ends and held between them.
fn rows_case(rows: u32) -> (String, oce_verify::SpecInputs) {
    let stop = f64::from(rows - 1);
    let mut main = String::from("time,Supply Temp,Offset,Enable,Stage\n");
    let mut outputs = String::from("time,Setpoint,Enabled,Staged,Limited\n");
    for k in 0..rows {
        main.push_str(&format!("{k},20,1,true,1\n"));
        outputs.push_str(&format!("{k},21,true,1,50\n"));
    }
    let limit = format!("time,Limit\n0,50\n{stop},50\n");
    let spec = case_spec_text(
        "never_changes",
        &[
            (r#""stop": 30"#, &format!(r#""stop": {stop}"#)),
            (r#""maxAge": 5"#, r#""maxAge": 1000000000"#),
        ],
    );
    let inputs = case_inputs(
        "never_changes",
        &[
            ("main.csv", main.as_bytes()),
            ("limit.csv", limit.as_bytes()),
            ("outputs.csv", outputs.as_bytes()),
        ],
    );
    (spec, inputs)
}

#[test]
fn the_points_bound_before_the_replay_is_held_at_its_edge_and_one_past() {
    // `rows` rows of main input and trend (the limit's two rows fall on main rows), so the
    // window holds `rows` frames and `rows` trend samples, and `limOut` one fan sample more. Its
    // series instants are at most `2 rows + 3` and the envelope is handed at most
    // `2 (2 rows + 3) - 1` points per series. A cap of exactly that runs; one less is refused
    // before the replay, as the run's own `too-many-points`, never the envelope's.
    let rows = 50;
    let (spec, inputs) = rows_case(rows);
    let spec = loaded(&spec);
    let real_points = 2 * (2 * u64::from(rows) + 3) - 1;
    let at_cap = |cap: u64| {
        let mut options = CommissionOptions::default();
        options.compare = CompareLimits {
            max_funnel_points: usize::try_from(cap).expect("fits"),
            ..CompareLimits::default()
        };
        commission(&spec, &inputs, &options)
    };
    assert!(at_cap(real_points).is_ok(), "exactly at the cap runs");
    let refused = at_cap(real_points - 1).expect_err("one past the cap is refused");
    assert_eq!(refused.code(), "too-many-points");
    assert!(
        refused
            .to_string()
            .contains("by change of value on up to 205 points"),
        "{refused}"
    );
    assert!(
        refused.cause().is_none(),
        "never the envelope's own refusal"
    );
}
