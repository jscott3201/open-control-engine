//! The comparison of each output (issue #663; commissioning note, "Envelope" and "Report and
//! receipt"; decisions 8, 11, 14, 16, 32 and 40): Reals through the reference's envelope at the
//! test's instants, Booleans and Integers exactly, a non-finite engine sample failing its output
//! with no envelope, and unverified outputs changing no verdict.

use oce_verify::{
    CommissionOptions, CommissionRun, CompareLimits, ComparisonMode, Envelope, OutputStatus,
    OutputType, SpecInputs, Tolerances, VerifyError,
};

use super::support;
use super::{
    body_of, commission, commissioned, curves_named, loaded, output_named, run_file_edited,
    run_inputs, run_spec, run_spec_edited,
};

#[test]
fn each_envelope_is_named_with_its_version_in_the_header() {
    let run = commissioned(&run_spec(), &run_inputs(&[]));
    let comparison = run.receipt.header.comparison;
    assert_eq!(comparison.mode(), ComparisonMode::Tolerance);
    assert_eq!(comparison.envelope(), Some(Envelope::Funnel));
    assert_eq!(comparison.envelope_version(), Some("funnel@a12fa96"));

    let spec = run_spec_edited(&[(
        r#""model": "Plant","#,
        r#""model": "Plant", "envelope": "aligned-grid","#,
    )]);
    let run = commissioned(&spec, &run_inputs(&[]));
    let comparison = run.receipt.header.comparison;
    assert_eq!(comparison.envelope(), Some(Envelope::AlignedGrid));
    assert_eq!(comparison.envelope_version(), Some("aligned-grid@1"));
    let json = run.receipt.to_canonical_json();
    assert!(
        json.contains(r#""envelope_version": "aligned-grid@1""#),
        "{json}"
    );
    // Under `aligned-grid` with every `y` tolerance of an output zero, its half-height is zero.
    let n_spec = run_spec_edited(&[
        (
            r#""model": "Plant","#,
            r#""model": "Plant", "envelope": "aligned-grid","#,
        ),
        (
            r#""tolerances": { "atolx": 1, "atoly": 0.01 },"#,
            r#""tolerances": { "atolx": 1 },"#,
        ),
        (r#""outputs": { "plant.TSet": { "atoly": 0.05 } },"#, ""),
    ]);
    let run = commissioned(&n_spec, &run_inputs(&[]));
    let lim = output_named(body_of(&run.receipt), "limOut");
    let height = lim.half_height.expect("an envelope was built");
    assert_eq!((height.min, height.max), (0.0, 0.0));
}

#[test]
fn a_boolean_mismatch_is_an_exceedance_of_one_and_an_integer_is_never_widened() {
    let outputs = run_file_edited("outputs.csv", &[("10,22,true,2,50", "10,22,false,3,50")]);
    let spec = run_spec_edited(&[(
        r#""outputs": { "plant.TSet": { "atoly": 0.05 } }"#,
        r#""outputs": { "plant.TSet": { "atoly": 0.05 }, "plant.nOut": { "atoly": 10 } }"#,
    )]);
    let run = commissioned(&spec, &run_inputs(&[("outputs.csv", &outputs)]));
    let body = body_of(&run.receipt);
    for name in ["enOut", "nOut"] {
        let output = output_named(body, name);
        assert_eq!(output.status, OutputStatus::Fail, "{name}");
        let worst = output.worst.expect("failed");
        assert_eq!((worst.t, worst.exceedance), (10.0, 1.0), "{name}");
        assert_eq!(output.first_exceedance_t, Some(10.0));
        assert_eq!(
            output.exceedance_time_s, 10.0,
            "10 s to the next test instant"
        );
        assert!(output.tolerances.is_none() && output.funnel_reference.is_none());
        assert!(output.half_width.is_none());
        assert_eq!(
            curves_named(&run, name).errors.y(),
            [0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0]
        );
    }
    assert_eq!(output_named(body, "nOut").output_type, OutputType::Integer);
    assert_eq!(output_named(body, "enOut").output_type, OutputType::Boolean);
    assert_eq!(body.status, OutputStatus::Fail);
}

#[test]
fn the_funnel_reference_decides_the_verdict_on_an_asymmetric_case_and_is_recorded() {
    // The engine's setpoint spikes 2 K for one frame at 15 s; the trend is flat. With `rtoly`
    // 0.1 and no absolute `y` tolerance, the band is 0.1 of the reference's range: 0.2 K around
    // the engine's spike, which the flat trend leaves, and, around the flat trend, funnel's
    // fallback 0.1 of its magnitude (29 K), which holds the spike.
    let inputs = b"time,Supply Temp,Offset,Enable,Stage,Limit\n0,20,1,true,1,50\n15,22,1,true,1,50\n16,20,1,true,1,50\n30,20,1,true,1,50\n";
    let outputs = b"time,Setpoint,Enabled,Staged,Limited\n0,21,true,1,50\n";
    let tolerance = (
        r#""outputs": { "plant.TSet": { "atoly": 0.05 } }"#,
        r#""outputs": { "plant.TSet": { "atoly": 0, "rtoly": 0.1 } }"#,
    );
    let files: [(&str, &[u8]); 2] = [("inputs.csv", inputs), ("outputs.csv", outputs)];
    let run = commissioned(&run_spec_edited(&[tolerance]), &run_inputs(&files));
    let engine_role = output_named(body_of(&run.receipt), "TSet").clone();
    assert_eq!(
        engine_role.funnel_reference.map(|r| r.as_str()),
        Some("engine")
    );
    assert_eq!(engine_role.status, OutputStatus::Fail);
    assert_eq!(engine_role.worst.map(|w| w.t), Some(15.0));

    let spec = run_spec_edited(&[
        tolerance,
        (
            r#""model": "Plant","#,
            r#""model": "Plant", "funnelReference": "trend","#,
        ),
    ]);
    let run = commissioned(&spec, &run_inputs(&files));
    let trend_role = output_named(body_of(&run.receipt), "TSet");
    assert_eq!(
        trend_role.funnel_reference.map(|r| r.as_str()),
        Some("trend")
    );
    assert_eq!(trend_role.status, OutputStatus::Pass);
    assert_eq!((engine_role.compared, trend_role.compared), (7, 7));
}

#[test]
fn a_breakpoint_error_under_aligned_grid_that_no_test_instant_shares_changes_no_verdict() {
    // The trend is the engine's setpoint plus 0.04 K at every instant, inside the 0.05 K band
    // there. Where the engine's setpoint rises between two instants, at the rectangle edge 1 s
    // past the lower one the grid envelope's upper bound is the reference's own line, which the
    // trend's line passes: an error at a breakpoint that is no instant.
    let outputs = b"time,Setpoint,Enabled,Staged,Limited\n0,21.04,true,1,50\n10,22.04,true,2,50\n20,24.04,false,3,40\n";
    let spec = run_spec_edited(&[(
        r#""model": "Plant","#,
        r#""model": "Plant", "envelope": "aligned-grid","#,
    )]);
    let mut options = CommissionOptions::default();
    options.bounds = true;
    let run = commission(&spec, &run_inputs(&[("outputs.csv", outputs)]), &options).expect("runs");
    let set = output_named(body_of(&run.receipt), "TSet");
    assert_eq!((set.status, set.compared), (OutputStatus::Pass, 7));
    let curves = curves_named(&run, "TSet");
    assert!(curves.errors.y().iter().all(|e| *e == 0.0));
    // The same two curves compared directly: the grid envelope reports an error at a breakpoint.
    let bounds = curves.bounds.as_ref().expect("kept");
    let direct = oce_verify::aligned_grid(
        bounds.reference.as_series(),
        bounds.test.as_series(),
        &Tolerances {
            atolx: 1.0,
            atoly: 0.05,
            ..Tolerances::ZERO
        },
        &CompareLimits::default(),
    )
    .expect("compares");
    assert!(direct.max_error > 0.03, "{}", direct.max_error);
    let at = direct.max_error_x.expect("an error");
    assert!(
        !curves.errors.x().contains(&at),
        "{at} is a breakpoint, not an instant"
    );
    assert!(direct.errors_at_test().iter().all(|e| *e == 0.0));
}

#[test]
fn stop_off_the_grid_is_the_last_compared_instant() {
    // Device units in kelvin, so the setpoint is 280 K until the engine steps to 290 K at 22 s;
    // the trend is 280 K throughout. On the grid 0, 10, 20 s the two agree; at 25 s they do not.
    let mapping = run_file_edited(
        "points.json",
        &[
            (
                r#""name": "Supply Temp", "unit": "degC""#,
                r#""name": "Supply Temp", "unit": "K""#,
            ),
            (
                r#""name": "Setpoint", "unit": "degC""#,
                r#""name": "Setpoint", "unit": "K""#,
            ),
            (
                r#""name": "Offset", "unit": "degC_delta""#,
                r#""name": "Offset", "unit": "K""#,
            ),
        ],
    );
    let inputs =
        b"time,Supply Temp,Offset,Enable,Stage,Limit\n0,279,1,true,1,50\n22,289,1,true,1,50\n";
    let outputs = b"time,Setpoint,Enabled,Staged,Limited\n0,280,true,1,50\n";
    let spec = run_spec_edited(&[(
        r#""window": { "start": 0, "stop": 30 },"#,
        r#""window": { "start": 0, "stop": 25 }, "sampling": 10,"#,
    )]);
    let run = commissioned(
        &spec,
        &run_inputs(&[
            ("points.json", &mapping),
            ("inputs.csv", inputs),
            ("outputs.csv", outputs),
        ]),
    );
    let set = output_named(body_of(&run.receipt), "TSet");
    assert_eq!(set.status, OutputStatus::Fail);
    assert_eq!(set.compared, 4);
    assert_eq!(set.worst.map(|w| w.t), Some(25.0));
    assert_eq!(set.first_exceedance_t, Some(25.0));
    assert_eq!(set.exceedance_time_s, 0.0, "the last instant adds nothing");
}

#[test]
fn a_reference_with_no_sampling_runs_by_change_of_value_and_records_no_sampling() {
    let spec = run_spec_edited(&[(r#""sampling": 5,"#, "")]);
    let run = commissioned(&spec, &run_inputs(&[]));
    let body = body_of(&run.receipt);
    assert!(body.time.sampling.is_none());
    assert!(
        !run.receipt.to_canonical_json().contains("\"sampling\""),
        "a receipt with no sampling writes none"
    );
    // The instants are the series' own times, not a grid: the fixture's inputs and expected
    // outputs are logged every 10 s, and the fan indicator, at 0, 15 and 25, gates `limOut`
    // only.
    assert_eq!(
        curves_named(&run, "nOut").errors.x(),
        [0.0, 10.0, 20.0, 30.0]
    );
    let limited = curves_named(&run, "limOut").errors.x().to_vec();
    assert!(
        [15.0, 25.0].iter().all(|t| limited.contains(t)),
        "{limited:?}"
    );
    // The reference's own sampling serves as well as the spec's.
    let spec = run_spec_edited(&[
        (r#""sampling": 5,"#, ""),
        (
            r#""model": "Plant","#,
            r#""model": "Plant", "sampling": 10,"#,
        ),
    ]);
    let run = commissioned(&spec, &run_inputs(&[]));
    assert_eq!(
        body_of(&run.receipt)
            .time
            .sampling
            .as_ref()
            .map(|s| s.canonical()),
        Some("10".to_owned())
    );
}

#[test]
fn a_reference_with_no_expected_source_compares_nothing_and_never_passes() {
    // The keys still match the sequence's outputs; only the expected source is gone.
    let spec = run_spec_edited(&[(
        r#""expected": [
        { "path": "outputs.csv", "format": "csv", "timeColumn": "time",
          "columns": { "Setpoint": "Setpoint", "Enabled": "Enabled", "Staged": "Staged",
                       "Limited": "Limited" } }
      ],"#,
        "",
    )]);
    let mut inputs = run_inputs(&[]);
    inputs.files.retain(|(path, _)| path != "outputs.csv");
    let run = commissioned(&spec, &inputs);
    let body = body_of(&run.receipt);
    assert!(body.outputs.is_empty());
    assert_eq!(body.status, OutputStatus::NoData);
    assert!(!run.receipt.header.passed);
    assert_eq!(
        body.unverified_outputs.len(),
        4,
        "every boundary output is unverified"
    );
    assert!(run.curves.is_empty());
}

#[test]
fn a_series_with_no_sample_at_or_before_start_is_no_data() {
    let outputs = run_file_edited("outputs.csv", &[("0,21,true,1,50\n", "")]);
    let run = commissioned(&run_spec(), &run_inputs(&[("outputs.csv", &outputs)]));
    let body = body_of(&run.receipt);
    assert!(
        body.outputs
            .iter()
            .all(|o| o.status == OutputStatus::NoData)
    );
    assert!(
        body.outputs
            .iter()
            .all(|o| o.compared + o.masked + o.uncovered == 0)
    );
    assert_eq!(body.status, OutputStatus::NoData);
    assert!(curves_named(&run, "TSet").errors.is_empty());
}

#[test]
fn window_ends_between_and_outside_samples_hold_the_sample_before() {
    let spec = run_spec_edited(&[(
        r#""window": { "start": 0, "stop": 30 },"#,
        r#""window": { "start": 5, "stop": 42 }, "sampling": 10,"#,
    )]);
    let run = commissioned(&spec, &run_inputs(&[]));
    let set = output_named(body_of(&run.receipt), "TSet");
    assert_eq!(set.status, OutputStatus::Pass);
    // 5 s lies between samples and 42 s past the last; 42 s is the off-grid stop.
    assert_eq!(
        curves_named(&run, "TSet").errors.x(),
        [5.0, 15.0, 25.0, 35.0, 42.0]
    );
}

#[test]
fn a_sampling_that_does_not_divide_the_window_stops_short_and_an_equal_time_pair_gives_its_later_row()
 {
    // The trended stage switches at 10 s, written as two rows; the engine's frames hold it too.
    let inputs_csv = run_file_edited(
        "inputs.csv",
        &[("10,21,1,true,2,50", "10,21,1,true,1,50\n10,21,1,true,2,50")],
    );
    let outputs = run_file_edited(
        "outputs.csv",
        &[("10,22,true,2,50", "10,22,true,1,50\n10,22,true,2,50")],
    );
    let spec = run_spec_edited(&[(
        r#""window": { "start": 0, "stop": 30 },"#,
        r#""window": { "start": 0, "stop": 25 }, "sampling": 10,"#,
    )]);
    let run = commissioned(
        &spec,
        &run_inputs(&[("inputs.csv", &inputs_csv), ("outputs.csv", &outputs)]),
    );
    let n = output_named(body_of(&run.receipt), "nOut");
    assert_eq!((n.status, n.compared), (OutputStatus::Pass, 4));
    assert_eq!(
        curves_named(&run, "nOut").errors.x(),
        [0.0, 10.0, 20.0, 25.0]
    );
    let time = &body_of(&run.receipt).time;
    assert_eq!(
        time.sampling.as_ref().map(|s| s.canonical()),
        Some("10".to_owned())
    );
}

/// A reference of the `signals` subject comparing its `ratio` output (`zone.T / ahu.T`) only:
/// its other five outputs are unverified. `rows` are (time, ahu.T, zone.T, b) with `n` 1 and
/// `lim` 5, and the trended ratio is 1 throughout.
fn signals(rows: &[(u32, f64, f64, bool)]) -> Result<CommissionRun, VerifyError> {
    let point = |name: &str, kind: &str| {
        format!(
            r#"{{"cdl": {{"name": "{name}", "type": "{kind}"}}, "device": {{"name": "{name}", "type": "{kind}"}}}}"#
        )
    };
    let mapping = ["ahu.T", "zone.T", "lim", "ratio"]
        .iter()
        .map(|name| point(name, "float"))
        .chain([point("n", "int"), point("b", "bool")])
        .collect::<Vec<_>>()
        .join(", ");
    let spec = loaded(&format!(
        r#"{{"tolerances": {{"atolx": 0.5, "atoly": 0.01}},
            "references": [{{"model": "S", "sequence": "sig", "cxf": "s.json",
            "pointNameMapping": [{mapping}],
            "window": {{"start": 0, "stop": {}}}, "sampling": 1,
            "inputs": [{{"path": "in.csv", "format": "csv", "timeColumn": "time",
              "columns": {{"ahu.T": "ahu.T", "zone.T": "zone.T", "n": "n", "b": "b", "lim": "lim"}}}}],
            "expected": [{{"path": "out.csv", "format": "csv", "timeColumn": "time",
              "columns": {{"ratio": "ratio"}}}}]}}]}}"#,
        rows.last().map_or(0, |row| row.0)
    ));
    let mut input = String::from("time,ahu.T,zone.T,n,b,lim\n");
    let mut output = String::from("time,ratio\n");
    for (t, ahu, zone, b) in rows {
        input.push_str(&format!("{t},{ahu:?},{zone:?},1,{b},5\n"));
        output.push_str(&format!("{t},1\n"));
    }
    let inputs = SpecInputs {
        cxf: support::subject("signals"),
        files: vec![
            ("in.csv".to_owned(), input.into_bytes()),
            ("out.csv".to_owned(), output.into_bytes()),
        ],
    };
    oce_verify::run_commissioning(&spec, 0, &inputs, &CommissionOptions::default())
}

#[test]
fn outputs_no_expected_source_supplies_are_listed_as_unverified_and_change_no_verdict() {
    let run = signals(&[(0, 2.0, 2.0, true), (10, 3.0, 3.0, true)]).expect("runs");
    let body = body_of(&run.receipt);
    assert_eq!(body.status, OutputStatus::Pass);
    assert_eq!(body.outputs.len(), 1);
    assert_eq!(
        body.unverified_outputs,
        ["ahu.y", "bOut", "limOut", "nOut", "zone.y"]
            .map(|name| format!("{}{name}", support::SIGNALS))
    );
    assert_eq!(body.unverified_outputs_total, 5);
}

#[test]
fn a_non_finite_or_out_of_range_engine_sample_fails_its_output_with_no_envelope() {
    for (ahu, zone, what) in [
        (0.0, 1.0, "an infinity"),
        (0.0, 0.0, "a NaN"),
        (1.0, 2e301, "a value above 2^1000"),
    ] {
        let run = signals(&[
            (0, 1.0, 1.0, true),
            (10, ahu, zone, true),
            (20, 1.0, 1.0, true),
        ])
        .unwrap_or_else(|error| panic!("{what}: {} {error}", error.code()));
        let ratio = &body_of(&run.receipt).outputs[0];
        assert_eq!(ratio.status, OutputStatus::Fail, "{what}");
        let worst = ratio.worst.expect("failed");
        assert_eq!((worst.t, worst.exceedance), (10.0, f64::INFINITY), "{what}");
        assert!(
            ratio.half_width.is_none() && ratio.half_height.is_none(),
            "{what}"
        );
        // Every compared instant is judged infinitely outside (owner ruling of 2026-11-21).
        assert_eq!(ratio.first_exceedance_t, Some(0.0), "{what}");
        assert_eq!(ratio.exceedance_time_s, 20.0, "{what}");
        let json = run.receipt.to_canonical_json();
        assert!(
            json.contains(r#""exceedance": "0x7ff0000000000000""#),
            "{what}: {json}"
        );
    }
    // At 2^1000 exactly the value is in range, and only the envelope judges it.
    let run = signals(&[
        (0, 1.0, 1.0, true),
        (10, 1.0, 2.0_f64.powi(1000), true),
        (20, 1.0, 1.0, true),
    ])
    .expect("runs");
    let ratio = &body_of(&run.receipt).outputs[0];
    assert!(ratio.half_width.is_some());
}

#[test]
fn run_time_warnings_are_listed_in_row_order_and_capped_with_their_total() {
    let rows: Vec<(u32, f64, f64, bool)> = (0..17).map(|t| (t, 1.0, 1.0, false)).collect();
    let run = signals(&rows).expect("runs");
    let body = body_of(&run.receipt);
    assert_eq!(body.warnings_total, 17);
    assert_eq!(body.warnings.len(), oce_verify::MAX_RUN_WARNINGS);
    assert!(body.warnings.windows(2).all(|pair| pair[0].t < pair[1].t));
    assert_eq!(body.warnings[0].block, "CDL.Utilities.Assert");
    let run = signals(&rows[..16]).expect("runs");
    assert_eq!(body_of(&run.receipt).warnings.len(), 16);
    assert_eq!(body_of(&run.receipt).warnings_total, 16);
}

#[test]
fn an_envelope_that_gives_no_result_is_compare_refused_with_the_comparisons_code() {
    // A time tolerance wider than the frames' spacing makes the funnel remove loops, and no
    // work is allowed for it.
    let spec = run_spec_edited(&[(
        r#""outputs": { "plant.TSet": { "atoly": 0.05 } }"#,
        r#""outputs": { "plant.TSet": { "atoly": 0.05, "atolx": 15 } }"#,
    )]);
    let mut options = CommissionOptions::default();
    options.compare.max_loop_removal_work = 0;
    let error = commission(&spec, &run_inputs(&[]), &options).expect_err("no work allowed");
    assert_eq!(
        (error.code(), error.cause()),
        ("compare-refused", Some("work-limit-exceeded"))
    );
    assert_eq!(error.path(), Some("urn:verify:plant#top.TSet"));
    // With the default work bound the same spec compares.
    commission(&spec, &run_inputs(&[]), &CommissionOptions::default()).expect("compares");
}
