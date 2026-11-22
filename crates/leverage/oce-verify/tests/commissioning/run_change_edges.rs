//! The edges of the change-of-value comparison found in the review of GitHub #790: a step
//! exactly one `x` tolerance from the other series' (the corner snap), a value at exactly
//! `stop`, an engine sample out of range, a mask that outlasts the window, a logger that falls
//! silent, and the `aligned-grid` envelope, refused without a `sampling` (owner ruling of
//! 2026-11-22; issue #792).

use oce_verify::{CommissionOptions, CommissionRun, OutputStatus, SpecInputs, VerifyError};

use super::run_change_of_value::{case_inputs, case_spec_text, report, run_case};
use super::{commission, curves_named, loaded};

/// Run `case` with `edits` to its spec and `replaced` files.
fn run_edited(
    case: &str,
    edits: &[(&str, &str)],
    replaced: &[(&str, &[u8])],
    options: &CommissionOptions,
) -> CommissionRun {
    let spec = loaded(&case_spec_text(case, edits));
    commission(&spec, &case_inputs(case, replaced), options)
        .unwrap_or_else(|error| panic!("{case} runs: {} {error}", error.code()))
}

/// Hundredths of a second as the decimal a log writes: `650` is `6.5`.
fn decimal(hundredths: i64) -> String {
    format!("{}.{:02}", hundredths / 100, hundredths % 100)
}

/// The engine's setpoint steps from 21 °C to 22 °C at `step`; the trend logs the step at
/// `logged`; both in hundredths of a second, the window 0 to 30 s.
fn step_files(step: i64, logged: &str) -> (String, String, &'static str) {
    let main = format!(
        "time,Supply Temp,Offset,Enable,Stage\n0,20,1,true,1\n{},21,1,true,1\n30,21,1,true,1\n",
        decimal(step)
    );
    let outputs =
        format!("time,Setpoint,Enabled,Staged,Limited\n0,21,true,1,50\n{logged},22,true,1,50\n");
    (main, outputs, "time,Limit\n0,50\n30,50\n")
}

/// The setpoint's report when the engine steps at `step` and the trend at `logged` under
/// `atolx` (spelled as the spec writes it) and `envelope`.
fn setpoint(step: i64, logged: &str, atolx: &str, envelope: &str) -> (OutputStatus, f64) {
    let (main, outputs, limit) = step_files(step, logged);
    let tolerance = format!(r#""atolx": {atolx},"#);
    let model = format!(r#""model": "Plant", "envelope": "{envelope}","#);
    let run = run_edited(
        "late_step",
        &[
            (r#""atolx": 1,"#, &tolerance),
            (r#""model": "Plant","#, &model),
            (r#""maxAge": 5"#, r#""maxAge": 100"#),
        ],
        &[
            ("main.csv", main.as_bytes()),
            ("outputs.csv", outputs.as_bytes()),
            ("limit.csv", limit.as_bytes()),
        ],
        &CommissionOptions::default(),
    );
    let set = report(&run, "TSet");
    (set.status, set.exceedance_time_s)
}

#[test]
fn a_step_exactly_one_tolerance_late_or_early_passes_wherever_it_falls() {
    // The review's sweep: a step at every half second, the trend exactly one `x` tolerance late
    // and early, nine tolerances. Before the corner snap, 132 of the review's cases failed
    // under `funnel`, each on a sliver of about 1e-15 s. (The review also swept `aligned-grid`,
    // which is now refused without a `sampling`.)
    let tolerances: [(&str, i64); 9] = [
        ("0.1", 10),
        ("0.25", 25),
        ("0.3", 30),
        ("0.5", 50),
        ("0.7", 70),
        ("1", 100),
        ("1.5", 150),
        ("2", 200),
        ("3", 300),
    ];
    let mut cases = 0;
    let mut failed = Vec::new();
    for envelope in ["funnel"] {
        for (atolx, width) in tolerances {
            for step in (50..=2850).step_by(50) {
                for logged in [step + width, step - width] {
                    if logged <= 0 || logged >= 3000 {
                        continue;
                    }
                    cases += 1;
                    let (status, time) = setpoint(step, &decimal(logged), atolx, envelope);
                    if status != OutputStatus::Pass {
                        failed.push((envelope, atolx, step, logged, time));
                    }
                }
            }
        }
    }
    // Every pair whose trend step lies strictly inside the window. (The review counted 1,950
    // over both envelopes with its own bounds on the window's ends.)
    assert_eq!(cases, 1_002);
    assert!(failed.is_empty(), "{} failed: {failed:?}", failed.len());
}

#[test]
fn a_step_one_tolerance_and_one_unit_past_the_snap_radius_late_fails() {
    // The corner the funnel computes for a step at 5 s under a 1 s tolerance, read from its
    // bound curves; the snap radius is 4 eps (m + w), m 30 s, w 1 s.
    let mut options = CommissionOptions::default();
    options.bounds = true;
    let (main, outputs, limit) = step_files(500, "6");
    let run = run_edited(
        "late_step",
        &[(r#""maxAge": 5"#, r#""maxAge": 100"#)],
        &[
            ("main.csv", main.as_bytes()),
            ("outputs.csv", outputs.as_bytes()),
            ("limit.csv", limit.as_bytes()),
        ],
        &options,
    );
    assert_eq!(report(&run, "TSet").status, OutputStatus::Pass);
    let bounds = curves_named(&run, "TSet").bounds.as_ref().expect("kept");
    let corner = bounds
        .lower
        .x()
        .iter()
        .copied()
        .find(|x| (x - 6.0).abs() < 1e-12)
        .expect("a lower corner at about 6 s");
    let radius = 4.0 * f64::EPSILON * (30.0 + 1.0);
    let mut late = corner + radius;
    while late - corner <= radius {
        late = late.next_up();
    }
    let (main, outputs, limit) = step_files(500, &format!("{late:?}"));
    let run = run_edited(
        "late_step",
        &[(r#""maxAge": 5"#, r#""maxAge": 100"#)],
        &[
            ("main.csv", main.as_bytes()),
            ("outputs.csv", outputs.as_bytes()),
            ("limit.csv", limit.as_bytes()),
        ],
        &CommissionOptions::default(),
    );
    let set = report(&run, "TSet");
    assert_eq!(set.status, OutputStatus::Fail, "the step at {late:?} s");
    assert_eq!(set.first_exceedance_t, Some(corner));
    assert_eq!(set.exceedance_time_s, late - corner);
}

#[test]
fn a_value_at_exactly_stop_is_judged_and_adds_no_time() {
    // The trend logs a wrong setpoint and a wrong enable at 30 s, the stop, and nowhere else.
    let run = run_case("value_at_stop_only");
    for name in ["TSet", "enOut"] {
        let output = report(&run, name);
        assert_eq!(output.status, OutputStatus::Fail, "{name}");
        assert_eq!(output.first_exceedance_t, Some(30.0), "{name}");
        assert_eq!(output.worst.map(|w| w.t), Some(30.0), "{name}");
        assert_eq!(output.exceedance_time_s, 0.0, "{name}");
    }
    assert_eq!(report(&run, "enOut").worst.map(|w| w.exceedance), Some(1.0));
}

#[test]
fn a_reference_step_at_exactly_stop_asserts_nothing() {
    // The same case with the trend as the reference: the setpoint's step at 30 s is now the
    // reference's, a vertical whose band spans both levels, so the engine's value there is
    // inside it and the output passes (the differential's `grid_ramp_at_stop`). A discrete
    // output has no role and still fails.
    let run = run_edited(
        "value_at_stop_only",
        &[(
            r#""model": "Plant","#,
            r#""model": "Plant", "funnelReference": "trend","#,
        )],
        &[],
        &CommissionOptions::default(),
    );
    let set = report(&run, "TSet");
    assert_eq!(set.status, OutputStatus::Pass);
    assert_eq!(set.exceedance_time_s, 0.0);
    assert_eq!(report(&run, "enOut").status, OutputStatus::Fail);
}

#[test]
fn an_engine_sample_out_of_range_fails_the_output_with_no_envelope() {
    // 1e302 degC is above 2^1000 K: the engine's setpoint is out of range from 10 s on.
    let main = "time,Supply Temp,Offset,Enable,Stage\n0,20,1,true,1\n10,1e302,1,true,1\n20,20,1,true,1\n30,20,1,true,1\n";
    let run = run_edited(
        "never_changes",
        &[],
        &[("main.csv", main.as_bytes())],
        &CommissionOptions::default(),
    );
    let set = report(&run, "TSet");
    assert_eq!(set.status, OutputStatus::Fail);
    assert_eq!(
        set.worst.map(|w| (w.t, w.exceedance)),
        Some((10.0, f64::INFINITY))
    );
    assert_eq!(set.first_exceedance_t, Some(0.0));
    assert!(set.half_width.is_none() && set.half_height.is_none());
    let errors = curves_named(&run, "TSet").errors.y().to_vec();
    assert!(errors.iter().all(|e| *e == f64::INFINITY), "{errors:?}");
    // The other outputs are judged as usual.
    assert_eq!(report(&run, "nOut").status, OutputStatus::Pass);
}

#[test]
fn a_mask_that_outlasts_the_window_is_one_stretch_to_stop() {
    // The fan is off from 5 s until 1000 s, past the stop: one masked stretch, 5 s to 30 s,
    // however few instants lie inside it.
    let run = run_case("sparse_mask");
    let limited = report(&run, "limOut");
    assert_eq!(limited.masked_intervals.len(), 1);
    let interval = limited.masked_intervals[0];
    assert_eq!((interval.from, interval.to), (5.0, 30.0));
    assert_eq!(limited.status, OutputStatus::Pass);
}

/// The `unlogged_pulse` case under `aligned-grid` and `atolx`, with `extra` spec members.
fn pulse_under_the_grid_envelope(atolx: &str, extra: &str) -> Result<CommissionRun, VerifyError> {
    let tolerance = format!(r#""atolx": {atolx},"#);
    let model = format!(r#""model": "Plant", "envelope": "aligned-grid",{extra}"#);
    let spec = loaded(&case_spec_text(
        "unlogged_pulse",
        &[
            (r#""atolx": 1,"#, &tolerance),
            (r#""model": "Plant","#, &model),
        ],
    ));
    // The refusal reads no file: no CXF and no trend is handed in.
    let inputs = if extra.is_empty() {
        SpecInputs::default()
    } else {
        case_inputs("unlogged_pulse", &[])
    };
    commission(&spec, &inputs, &CommissionOptions::default())
}

#[test]
fn the_grid_envelope_with_no_sampling_is_refused_before_any_file_is_read() {
    // Its band is a straight line between grid points, so without a grid it passed a pulse the
    // trend never logged at every x tolerance the review tried. Refused, at each.
    for atolx in ["0", "1", "4"] {
        let refused = pulse_under_the_grid_envelope(atolx, "").expect_err("refused");
        assert_eq!(
            (refused.code(), refused.subject()),
            ("sampling-required", "references[0].sampling"),
            "atolx {atolx}"
        );
        assert!(refused.to_string().contains("issue #792"), "{refused}");
    }
}

#[test]
fn the_grid_envelope_with_a_sampling_runs_as_before() {
    // With a grid of 1/64 s the same pulse fails under `aligned-grid`, as it does under the
    // funnel by change of value. (Every golden of a run with a sampling is held byte-identical
    // by run_receipts.rs.)
    let run = pulse_under_the_grid_envelope("1", r#" "sampling": 0.015625,"#).expect("runs");
    let set = report(&run, "TSet");
    assert_eq!(set.status, OutputStatus::Fail);
    assert!(
        set.last_sample_t.is_none(),
        "written without a sampling only"
    );
    assert_eq!(
        run.receipt.header.comparison.envelope_version(),
        Some("aligned-grid@1")
    );
}

#[test]
fn a_logger_that_falls_silent_is_held_and_says_when_it_last_logged() {
    // An hour's window; the trend logs at 0 s and 100 s and then nothing. Held, it matches the
    // engine to the stop, so it passes, and `last_sample_t` says the last value is from 100 s.
    let outputs = "time,Setpoint,Enabled,Staged,Limited\n0,21,true,1,50\n100,21,true,1,50\n";
    let run = run_edited(
        "never_changes",
        &[(r#""stop": 30"#, r#""stop": 3600"#)],
        &[("outputs.csv", outputs.as_bytes())],
        &CommissionOptions::default(),
    );
    for name in ["TSet", "enOut", "limOut", "nOut"] {
        let output = report(&run, name);
        assert_eq!(output.status, OutputStatus::Pass, "{name}");
        assert_eq!(output.last_sample_t, Some(100.0), "{name}");
    }
    assert!(
        run.receipt
            .to_canonical_json()
            .contains("\"last_sample_t\": 100.0"),
        "the receipt writes it"
    );
}

#[test]
fn the_last_sample_of_a_trend_held_from_before_the_start_is_its_own_time() {
    // The window starts at 5 s; the trend's one sample is at 0 s.
    let run = run_case("one_sample_before_start");
    assert_eq!(report(&run, "TSet").last_sample_t, Some(0.0));
    // With no sample at or before the start there is no instant and no last sample.
    let run = run_case("no_sample_at_start");
    assert_eq!(report(&run, "TSet").last_sample_t, None);
}
