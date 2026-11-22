//! The commissioning receipt (issue #663; commissioning note, "Report and receipt", decisions
//! 7, 20, 30, 31 and 42) and the run that issues it (issue #662):
//!
//! - **Goldens**: `pass`, `fail` and `no_data` receipts of the run fixture
//!   (`tests/fixtures/commissioning/run/`), byte for byte, and the exceedance column of every
//!   output of each as hex bits; blessed only with `OCE_BLESS_VERIFY=1`.
//! - **Oracle**: `derive_run.py`, which shares no code with the crate, derives the verdict of a
//!   four-output case under `aligned-grid` (masking, zeroing, the envelope at the test's
//!   instants, the exact comparisons, every figure of the report) into `oracle.txt`, and the
//!   golden receipts' ids; the funnel's figures for one output are derived by hand below from the
//!   note's formulas; and the engine series is cross-checked against a plain frame loop over the
//!   facade.
//! - **Determinism**: one receipt for replay chunks of 1, 7 and the whole run, for two runs, for
//!   every order of the files handed in, and whatever the caller's timings.

use oce_api::Value;
use oce_replay::TraceColumn;
use oce_verify::{CommissionOptions, CommissionRun, OutputStatus, ReceiptKind, Timings, TrendData};

use super::support;
use super::{
    body_of, commission, commissioned, curves_named, output_named, run_file, run_inputs, run_spec,
    run_spec_edited,
};

/// A golden: its name, the spec edits and the files replaced.
struct Golden {
    name: &'static str,
    edits: &'static [(&'static str, &'static str)],
    files: Vec<(&'static str, Vec<u8>)>,
}

fn goldens() -> Vec<Golden> {
    vec![
        Golden {
            name: "pass",
            edits: &[],
            files: Vec::new(),
        },
        Golden {
            name: "fail",
            edits: &[],
            files: vec![("outputs.csv", run_file("oracle_outputs.csv"))],
        },
        Golden {
            name: "no_data",
            edits: &[],
            files: vec![("fan.csv", b"time,Fan Status\n0,false\n".to_vec())],
        },
    ]
}

fn run_golden(golden: &Golden, options: &CommissionOptions) -> CommissionRun {
    let spec = run_spec_edited(golden.edits);
    let files: Vec<(&str, &[u8])> = golden
        .files
        .iter()
        .map(|(path, bytes)| (*path, bytes.as_slice()))
        .collect();
    commission(&spec, &run_inputs(&files), options).expect("the golden runs")
}

fn hex(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}

/// Every output's exceedance column, as hex bits.
fn curves_text(run: &CommissionRun) -> String {
    let mut out = String::from("# the exceedance column of every compared output, as hex bits\n");
    for curves in &run.curves {
        out.push_str(&format!("output\t{}\n", curves.output));
        for (t, e) in curves.errors.points() {
            out.push_str(&format!("{}\t{}\n", hex(t), hex(e)));
        }
    }
    out
}

#[test]
fn the_pass_fail_and_no_data_receipts_and_their_columns_match_their_goldens_byte_for_byte() {
    for golden in goldens() {
        let run = run_golden(&golden, &CommissionOptions::default());
        let status = body_of(&run.receipt).status;
        assert_eq!(status.as_str(), golden.name);
        support::compare_or_bless(
            &support::fixture(&format!(
                "commissioning/run/receipts/{}.receipt.json",
                golden.name
            )),
            &run.receipt.to_canonical_json(),
        );
        support::compare_or_bless(
            &support::fixture(&format!(
                "commissioning/run/receipts/{}.curves.txt",
                golden.name
            )),
            &curves_text(&run),
        );
    }
}

/// The oracle's rows for one output of one case, each split on tabs.
fn oracle_rows(case: &str, output: &str) -> Vec<Vec<String>> {
    let text = String::from_utf8(run_file("oracle.txt")).expect("UTF-8");
    let mut rows = Vec::new();
    let (mut in_case, mut inside) = (false, false);
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let cells: Vec<String> = line.split('\t').map(str::to_owned).collect();
        match cells[0].as_str() {
            "case" => in_case = cells[1] == case,
            "output" => inside = in_case && cells[1] == output,
            "receipt" => inside = false,
            _ if inside => rows.push(cells),
            _ => {}
        }
    }
    rows
}

fn oracle_value(rows: &[Vec<String>], kind: &str) -> Option<Vec<String>> {
    rows.iter()
        .find(|row| row[0] == kind)
        .map(|row| row[1..].to_vec())
}

fn nan_or_hex(x: f64) -> String {
    if x.is_nan() { "nan".to_owned() } else { hex(x) }
}

/// The oracle case (`oracle_spec.json`: two input sources, one frame refused for a missing
/// sample, an off-grid stop) run under `envelope`.
fn oracle_run(envelope: &str) -> CommissionRun {
    let text = String::from_utf8(run_file("oracle_spec.json"))
        .expect("UTF-8")
        .replace(
            r#""model": "Plant","#,
            &format!(r#""model": "Plant", "envelope": "{envelope}","#),
        );
    let spec = super::loaded(&text);
    let files = [
        "points.json",
        "oracle_main.csv",
        "oracle_limit.csv",
        "oracle_outputs.csv",
        "fan.csv",
    ]
    .iter()
    .map(|name| ((*name).to_owned(), run_file(name)))
    .collect();
    let inputs = oce_verify::SpecInputs {
        cxf: support::subject("plant"),
        files,
    };
    commission(&spec, &inputs, &CommissionOptions::default()).expect("the oracle case runs")
}

#[test]
fn four_outputs_under_either_envelope_match_the_independent_derivation() {
    for envelope in ["funnel", "aligned-grid"] {
        check_oracle_case(envelope);
    }
}

fn check_oracle_case(case: &str) {
    let run = oracle_run(case);
    let body = body_of(&run.receipt);
    assert_eq!(body.frames.refused, 1, "{case}");
    for output in ["TSet", "enOut", "limOut", "nOut"] {
        let name = format!("{case} {output}");
        let name = name.as_str();
        let rows = oracle_rows(case, output);
        assert!(!rows.is_empty(), "{name}");
        let report = output_named(body, output);
        let counts = oracle_value(&rows, "counts").expect("counts");
        assert_eq!(
            [report.compared, report.masked, report.uncovered].map(|n| n.to_string()),
            [counts[0].clone(), counts[1].clone(), counts[2].clone()],
            "{name}"
        );
        assert_eq!(
            report.status.as_str(),
            oracle_value(&rows, "status").expect("status")[0],
            "{name}"
        );
        match oracle_value(&rows, "worst") {
            Some(worst) => {
                let mine = report.worst.expect("a worst");
                assert_eq!(
                    [hex(mine.t), hex(mine.exceedance)],
                    [worst[0].clone(), worst[1].clone()],
                    "{name}"
                );
                let first = oracle_value(&rows, "first").expect("first");
                assert_eq!(
                    hex(report.first_exceedance_t.expect("first")),
                    first[0],
                    "{name}"
                );
            }
            None => assert!(
                report.worst.is_none() && report.first_exceedance_t.is_none(),
                "{name}"
            ),
        }
        assert_eq!(
            hex(report.exceedance_time_s),
            oracle_value(&rows, "time").expect("time")[0],
            "{name}"
        );
        let intervals: Vec<[String; 2]> = rows
            .iter()
            .filter(|row| row[0] == "masked_interval")
            .map(|row| [row[1].clone(), row[2].clone()])
            .collect();
        let mine: Vec<[String; 2]> = report
            .masked_intervals
            .iter()
            .map(|i| [hex(i.from), hex(i.to)])
            .collect();
        assert_eq!(mine, intervals, "{name}");
        let masked: Vec<[String; 2]> = rows
            .iter()
            .filter(|row| row[0] == "masked_exceedance")
            .map(|row| [row[1].clone(), row[2].clone()])
            .collect();
        let mine: Vec<[String; 2]> = report
            .masked_exceedance
            .iter()
            .map(|e| [hex(e.t), hex(e.exceedance)])
            .collect();
        assert_eq!(mine, masked, "{name}");
        for (kind, mine) in [
            ("half_width", report.half_width),
            ("half_height", report.half_height),
        ] {
            match oracle_value(&rows, kind) {
                Some(oracle) => {
                    let size = mine.expect("an envelope");
                    assert_eq!(
                        [hex(size.min), hex(size.max)],
                        [oracle[0].clone(), oracle[1].clone()],
                        "{name} {kind}"
                    );
                }
                None => assert!(mine.is_none(), "{name} {kind}"),
            }
        }
        let errors: Vec<[String; 2]> = rows
            .iter()
            .filter(|row| row[0] == "error")
            .map(|row| [row[1].clone(), row[2].clone()])
            .collect();
        let mine: Vec<[String; 2]> = curves_named(&run, output)
            .errors
            .points()
            .map(|(t, e)| [hex(t), nan_or_hex(e)])
            .collect();
        assert_eq!(mine, errors, "{name}");
    }
}

#[test]
fn the_funnels_worst_exceedance_and_half_sizes_follow_the_notes_formulas() {
    // The fail golden's setpoint, under the funnel, atolx 1 s and atoly 0.05 K. The engine's
    // series is the reference: 20 + 1, 21 + 1, 22 + 2, 22 + 2 degC read in kelvin at 0, 10, 20
    // and 30 s. Step 1: every half-width is max(1, 0, 0) = 1 and every half-height
    // max(0.05, 0, 0) = 0.05, no fallback. At 30 s the reference is flat from 20 s, so the upper
    // bound there is the flat corners' y + 0.05 and the error is the trended 24.3 degC above it
    // (step 5: max(0, y - y_up) - min(0, y - y_low)).
    let run = run_golden(&goldens()[1], &CommissionOptions::default());
    let set = output_named(body_of(&run.receipt), "TSet");
    let (width, height) = (
        set.half_width.expect("built"),
        set.half_height.expect("built"),
    );
    assert_eq!((width.min, width.max), (1.0, 1.0));
    assert_eq!((height.min, height.max), (0.05, 0.05));
    let reference_at_30 = (22.0_f64 + 273.15) + 2.0;
    let test_at_30 = 24.3_f64 + 273.15;
    let upper = reference_at_30 + 0.05;
    let lower = reference_at_30 - 0.05;
    let by_hand = (test_at_30 - upper).max(0.0) - (test_at_30 - lower).min(0.0);
    let curve = curves_named(&run, "TSet");
    let at_30 = curve.errors.y()[curve.errors.len() - 1];
    assert_eq!(at_30.to_bits(), by_hand.to_bits());
    assert!((by_hand - 0.25).abs() < 1e-9);
    // At 15 s the trended 23 degC is above the band. The reference, held on the 5 s grid, is
    // 294.15 K at 0 and 5 s, 295.15 K at 10 and 15 s and 297.15 K from 20 s. Step 3 at 15 s
    // (flat before, rising after) and at 20 s (rising before, flat after) takes the top-left
    // corners (14 s, 295.2 K) and (19 s, 297.2 K), so the upper bound at 15 s is 295.6 K and the
    // error 0.55 K: the worst, since 0.25 K at 30 s is less. (The port builds the corners on `x`
    // normalised by its magnitude, 30 s, so the bits are held only to a nanokelvin.)
    let corner_at_14 = ((21.0_f64 + 273.15) + 1.0) + 0.05;
    let corner_at_19 = ((22.0_f64 + 273.15) + 2.0) + 0.05;
    let upper_at_15 = corner_at_14 + (15.0 - 14.0) / (19.0 - 14.0) * (corner_at_19 - corner_at_14);
    let at_15 = (23.0_f64 + 273.15) - upper_at_15;
    assert!(
        (curve.errors.y()[3] - at_15).abs() < 1e-9,
        "{}",
        curve.errors.y()[3]
    );
    assert!((at_15 - 0.55).abs() < 1e-9);
    // 22.2 degC at 10 s: the corner (9 s, 295.2 K) before and (14 s, 295.2 K) after hold the
    // upper bound at 295.2 K there, and 295.35 K is above it by 0.15 K.
    assert!(
        (curve.errors.y()[2] - 0.15).abs() < 1e-9,
        "{}",
        curve.errors.y()[2]
    );
    let worst = set.worst.expect("failed");
    assert_eq!(worst.t, 15.0);
    assert_eq!(worst.exceedance, curve.errors.y()[3]);
    assert_eq!(set.first_exceedance_t, Some(10.0));
}

#[test]
fn the_engine_series_is_what_a_plain_frame_loop_over_the_facade_gives() {
    let run = commissioned(&run_spec(), &run_inputs(&[]));
    let mut engine = oce_api::Engine::new();
    engine.load_cxf(&support::subject("plant")).expect("loads");
    let rows = [
        (0.0, 20.0, 1.0, true, 1, 50.0),
        (10.0, 21.0, 1.0, true, 2, 50.0),
        (20.0, 22.0, 2.0, false, 3, 40.0),
        (30.0, 22.0, 2.0, false, 3, 40.0),
    ];
    let p = |name: &str| format!("urn:verify:plant#top.{name}");
    let names = ["T", "dT", "en", "n", "lim"].map(p);
    let trace = &run.trace;
    for (row, (t, temp, offset, en, n, lim)) in rows.into_iter().enumerate() {
        let entries = [
            (names[0].as_str(), Value::Real(temp + 273.15)),
            (names[1].as_str(), Value::Real(offset)),
            (names[2].as_str(), Value::Boolean(en)),
            (names[3].as_str(), Value::Integer(n)),
            (names[4].as_str(), Value::Real(lim / 100.0)),
        ];
        let frame = engine.prepare_frame(t, &entries).expect("prepares");
        let done = engine.execute_frame(frame).expect("executes");
        for (path, value) in done.outputs() {
            let (_, column) = trace
                .outputs()
                .iter()
                .find(|(name, _)| name == path)
                .expect("the trace has the output");
            let same = match (column, value) {
                (TraceColumn::Real(v), Value::Real(x)) => v[row].to_bits() == x.to_bits(),
                (TraceColumn::Integer(v), Value::Integer(x)) => v[row] == *x,
                (TraceColumn::Boolean(v), Value::Boolean(x)) => (v[row] == 1) == *x,
                _ => false,
            };
            assert!(same, "{path} at row {row}: {column:?} against {value:?}");
        }
    }
    assert_eq!(trace.rows(), 4);
    assert!(trace.inputs().is_some(), "the trace retains its inputs");
}

#[test]
fn every_golden_receipt_id_matches_the_independent_derivation_and_its_own_recomputation() {
    let text = String::from_utf8(run_file("oracle.txt")).expect("UTF-8");
    for golden in goldens() {
        let run = run_golden(&golden, &CommissionOptions::default());
        let header = &run.receipt.header;
        assert_eq!(header.receipt_id, run.receipt.computed_receipt_id());
        let row = format!(
            "receipt\t{}.receipt.json\t{}",
            golden.name, header.receipt_id
        );
        assert!(
            text.lines().any(|line| line == row),
            "oracle.txt holds `{row}`"
        );
        assert_eq!(header.kind, ReceiptKind::Commissioning);
        assert_eq!(header.engine_version, "0.6.0");
        assert_eq!(header.passed, golden.name == "pass");
    }
    // The spec's id, as tests/fixtures/commissioning/derive_canonical.py printed it.
    assert_eq!(run_spec().spec_id(), RUN_SPEC_ID);
}

/// The run fixture's `spec_id`, derived outside the crate.
const RUN_SPEC_ID: &str = "spec:fnv1a128:268d862461b0b585c0bcfa9e9ef5d847";

#[test]
fn one_receipt_for_every_chunk_size_every_run_every_file_order_and_any_timings() {
    for golden in goldens() {
        let reference = run_golden(&golden, &CommissionOptions::default());
        let bytes = reference.receipt.to_canonical_json();
        let curves = curves_text(&reference);
        for chunk in [1, 7, usize::MAX] {
            let mut options = CommissionOptions::default();
            options.chunk_frames = chunk;
            let run = run_golden(&golden, &options);
            assert_eq!(
                run.receipt.to_canonical_json(),
                bytes,
                "{} chunk {chunk}",
                golden.name
            );
            assert_eq!(curves_text(&run), curves);
            assert_eq!(run.trace.rows(), reference.trace.rows());
        }
        let mut options = CommissionOptions::default();
        options.timings = Some(Timings {
            load_ms: 3,
            run_ms: 5,
            compare_ms: 7,
        });
        let timed = run_golden(&golden, &options);
        assert_eq!(
            timed.receipt.header.receipt_id,
            reference.receipt.header.receipt_id
        );
        assert_eq!(timed.receipt.header.timings.run_ms, 5);
        assert_ne!(
            timed.receipt.to_canonical_json(),
            bytes,
            "the timings are written"
        );
    }
    // The files handed in, in every rotation.
    let reference = commissioned(&run_spec(), &run_inputs(&[]))
        .receipt
        .to_canonical_json();
    for turn in 1..4 {
        let mut inputs = run_inputs(&[]);
        inputs.files.rotate_left(turn);
        assert_eq!(
            commissioned(&run_spec(), &inputs)
                .receipt
                .to_canonical_json(),
            reference
        );
    }
}

#[test]
fn the_data_label_is_the_callers_and_inside_the_receipt_id() {
    let trended = commissioned(&run_spec(), &run_inputs(&[]));
    let options = CommissionOptions::default().with_data(TrendData::Synthetic);
    let synthetic = commission(&run_spec(), &run_inputs(&[]), &options).expect("runs");
    assert_eq!(body_of(&trended.receipt).data, TrendData::Trended);
    assert_eq!(body_of(&synthetic.receipt).data, TrendData::Synthetic);
    assert!(
        synthetic
            .receipt
            .to_canonical_json()
            .contains(r#""data": "synthetic""#)
    );
    assert_ne!(
        trended.receipt.header.receipt_id,
        synthetic.receipt.header.receipt_id
    );
    assert_eq!(body_of(&synthetic.receipt).status, OutputStatus::Pass);
}

#[test]
fn the_body_records_the_files_units_time_and_findings_the_note_lists() {
    let spec = run_spec_edited(&[(
        r#""model": "Plant","#,
        r#""model": "Plant", "runController": true,"#,
    )]);
    let run = commissioned(&spec, &run_inputs(&[]));
    let body = body_of(&run.receipt);
    assert_eq!(
        (body.model.as_str(), body.sequence.as_str()),
        ("Plant", "plant")
    );
    let mapping = body.mapping.as_ref().expect("a mapping file");
    assert_eq!(mapping.path, "points.json");
    let roles: Vec<(&str, &str, u64)> = body
        .sources
        .iter()
        .map(|s| (s.role.as_str(), s.path.as_str(), s.rows))
        .collect();
    assert_eq!(
        roles,
        [
            ("inputs", "inputs.csv", 4),
            ("expected", "outputs.csv", 4),
            ("indicators", "fan.csv", 3)
        ]
    );
    assert_eq!(body.unit_columns_total, 5);
    assert_eq!(body.findings_total, 1);
    assert_eq!(body.findings[0].at, "references[0].runController");
    assert_eq!(
        body.time.origin.as_ref().map(|o| o.as_str()),
        Some("2026-11-21T00:00:00Z")
    );
    assert_eq!(body.time.window_stop.canonical(), "30");
    assert_eq!(
        body.time.sampling.as_ref().map(|s| s.canonical()),
        Some("5".to_owned())
    );
    assert!(body.load_warnings.is_empty());
    assert_eq!(body.frames.executed, 4);
}
