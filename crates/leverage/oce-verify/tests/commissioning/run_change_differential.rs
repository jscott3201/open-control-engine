//! A differential of the change-of-value comparison against the grid path (the review of
//! GitHub #790, item F): each seeded case runs with no `sampling` and again with a `sampling`
//! of 1/64 s, on which every event of the case lies, so the grid samples every stretch the
//! change-of-value path judges whole.
//!
//! The cases draw engine steps, trends logged on change with a delay from -2 s to +3 s,
//! spurious pulses, trends that stop logging, masks, uncovered stretches (a held input whose
//! hold runs out), `atolx` 0 to 4, `atoly` 0 to 0.5, `rtoly`, `ltolx` and both reference roles,
//! over the `plant` subject's two Real, one Boolean and one Integer output. The generator is
//! splitmix64 from a fixed seed, so every run draws the same cases. The funnel envelope only:
//! `aligned-grid` without a `sampling` is refused (owner ruling of 2026-11-22; issue #792).
//!
//! What must agree, and the documented ways the two may differ, are in [`check_output`].

use oce_verify::{CommissionOptions, CommissionRun, OutputReport, OutputStatus, SpecInputs};

use super::support;
use super::{body_of, commission, loaded};

/// A deterministic stream of 64-bit words (splitmix64).
struct Words(u64);

impl Words {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A draw below `n`.
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// One of `items`.
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[usize::try_from(self.below(items.len() as u64)).expect("fits")]
    }

    /// A half second strictly inside the window, 0.5 to 29.5 s, in halves.
    fn half_second(&mut self) -> i64 {
        1 + i64::try_from(self.below(59)).expect("fits")
    }
}

/// Halves of a second as a log writes them: `13` is `6.5`.
fn seconds(halves: i64) -> String {
    if halves % 2 == 0 {
        format!("{}", halves / 2)
    } else {
        format!("{}.5", halves / 2)
    }
}

/// The ideal device values at each input event: setpoint (degC), enabled, staged, limited (%).
#[derive(Clone, Copy, PartialEq)]
struct Values {
    setpoint: f64,
    enabled: bool,
    staged: i64,
    limited: f64,
}

/// One generated case: its files, its spec and a label for the failure message.
struct Case {
    spec: String,
    main: String,
    limit: String,
    outputs: String,
    fan: String,
    label: String,
}

fn sorted_unique(mut times: Vec<i64>) -> Vec<i64> {
    times.sort_unstable();
    times.dedup();
    times
}

/// The value held at `t` of a step function given at sorted `times`.
fn held<T: Copy>(times: &[i64], values: &[T], t: i64) -> T {
    let index = times.partition_point(|x| *x <= t).max(1) - 1;
    values[index]
}

#[allow(clippy::too_many_lines)]
fn generate(seed: u64) -> Case {
    let mut words = Words(seed);
    // Inputs, in halves of a second over 0 to 60 (0 to 30 s).
    let mut main_times = vec![0, 60];
    for _ in 0..1 + words.below(5) {
        main_times.push(words.half_second());
    }
    let main_times = sorted_unique(main_times);
    let temps = [20.0, 20.5, 21.0, 22.0, 23.0];
    let main_rows: Vec<(f64, bool, i64)> = main_times
        .iter()
        .map(|_| {
            (
                *words.pick(&temps),
                words.below(2) == 0,
                1 + i64::try_from(words.below(3)).expect("fits"),
            )
        })
        .collect();
    let mut limit_times = vec![0, 60];
    for _ in 0..words.below(4) {
        limit_times.push(words.half_second());
    }
    let limit_times = sorted_unique(limit_times);
    let limits: Vec<f64> = limit_times
        .iter()
        .map(|_| *words.pick(&[40.0, 50.0, 60.0]))
        .collect();
    let max_age = *words.pick(&["100", "100", "2.5"]);

    // The trend: the ideal values at every input event, logged `delay` halves late (early when
    // negative), the value at 0 the ideal value at the delay's start.
    let events = sorted_unique(main_times.iter().chain(&limit_times).copied().collect());
    let ideal = |t: i64| {
        let t = t.clamp(0, 60);
        let (temp, enabled, staged) = held(&main_times, &main_rows, t);
        Values {
            setpoint: temp + 1.0,
            enabled,
            staged,
            limited: held(&limit_times, &limits, t),
        }
    };
    let delay = *words.pick(&[-4_i64, -2, -1, 0, 0, 1, 2, 4, 6]);
    let mut logged: Vec<(i64, Values)> = vec![(0, ideal(-delay))];
    for event in &events {
        let t = event + delay;
        if 0 < t && t < 60 {
            logged.push((t, ideal(*event)));
        }
    }
    let mut label = format!("seed {seed}: delay {delay} halves");
    // A spurious pulse in one output, cut short by the next logged row.
    if words.below(4) == 0 {
        let at = words.half_second();
        let which = words.below(3);
        let mut pulse = ideal(at - delay);
        match which {
            0 => pulse.setpoint += 1.0,
            1 => pulse.enabled = !pulse.enabled,
            _ => pulse.limited += 10.0,
        }
        logged.push((at, pulse));
        let back = at + 1 + i64::try_from(words.below(6)).expect("fits");
        if back < 60 {
            logged.push((back, ideal(back - delay)));
        }
        label.push_str(&format!(", pulse {which} at {at}"));
    }
    logged.sort_by_key(|(t, _)| *t);
    // A logger that stops: nothing after `silent`.
    if words.below(4) == 0 {
        let silent = words.half_second();
        logged.retain(|(t, _)| *t <= silent);
        label.push_str(&format!(", silent after {silent}"));
    }
    // Rows at one time collapse to the last as both paths read them; keep them as written.
    let mut outputs = String::from("time,Setpoint,Enabled,Staged,Limited\n");
    for (t, v) in &logged {
        outputs.push_str(&format!(
            "{},{},{},{},{}\n",
            seconds(*t),
            v.setpoint,
            v.enabled,
            v.staged,
            v.limited
        ));
    }
    let mut main = String::from("time,Supply Temp,Offset,Enable,Stage\n");
    for (t, (temp, enabled, staged)) in main_times.iter().zip(&main_rows) {
        main.push_str(&format!("{},{temp},1,{enabled},{staged}\n", seconds(*t)));
    }
    let mut limit = String::from("time,Limit\n");
    for (t, value) in limit_times.iter().zip(&limits) {
        limit.push_str(&format!("{},{value}\n", seconds(*t)));
    }
    let mut fan = String::from("time,Fan Status\n0,true\n");
    if words.below(3) == 0 {
        let off = words.half_second();
        let on = off + 1 + i64::try_from(words.below(20)).expect("fits");
        fan.push_str(&format!("{},false\n", seconds(off)));
        if on < 60 {
            fan.push_str(&format!("{},true\n", seconds(on)));
        }
        label.push_str(&format!(", mask {off}..{on}"));
    }
    let atolx = *words.pick(&["0", "0.5", "1", "2", "4"]);
    let atoly = *words.pick(&["0", "0.05", "0.2", "0.5"]);
    let mut tolerances = format!(r#""atolx": {atolx}, "atoly": {atoly}"#);
    if words.below(4) == 0 {
        tolerances.push_str(r#", "rtoly": 0.01"#);
    }
    if words.below(4) == 0 {
        tolerances.push_str(r#", "ltolx": 0.02"#);
    }
    let role = *words.pick(&["engine", "trend"]);
    label.push_str(&format!(", {tolerances}, maxAge {max_age}, {role}"));
    let spec = format!(
        r#"{{"schema": "oce/commissioning-spec/v1", "tolerances": {{{tolerances}}},
  "references": [{{"model": "Plant", "sequence": "plant", "cxf": "plant.cxf.json",
    "pointNameMapping": "points.json", "window": {{"start": 0, "stop": 30}},
    "funnelReference": "{role}",
    "inputs": [
      {{"path": "main.csv", "format": "csv", "timeColumn": "time",
        "columns": {{"Supply Temp": "Supply Temp", "Offset": "Offset", "Enable": "Enable", "Stage": "Stage"}}}},
      {{"path": "limit.csv", "format": "csv", "timeColumn": "time",
        "columns": {{"Limit": "Limit"}}, "hold": {{"maxAge": {max_age}}}}}],
    "expected": [{{"path": "outputs.csv", "format": "csv", "timeColumn": "time",
      "columns": {{"Setpoint": "Setpoint", "Enabled": "Enabled", "Staged": "Staged", "Limited": "Limited"}}}}],
    "indicatorSources": [{{"path": "fan.csv", "format": "csv", "timeColumn": "time",
      "columns": {{"Fan Status": "Fan Status"}}}}],
    "indicators": {{"plant.lim*": ["fanSta.y"]}}}}]}}"#
    );
    Case {
        spec,
        main,
        limit,
        outputs,
        fan,
        label,
    }
}

/// Run `case`, with no sampling or with `sampling` (spelled as the spec writes it).
fn run(case: &Case, sampling: Option<&str>) -> CommissionRun {
    let spec = match sampling {
        Some(step) => case.spec.replace(
            r#""funnelReference""#,
            &format!(r#""sampling": {step}, "funnelReference""#),
        ),
        None => case.spec.clone(),
    };
    let spec = loaded(&spec);
    let inputs = SpecInputs {
        cxf: support::subject("plant"),
        files: vec![
            (
                "points.json".to_owned(),
                support::fixture_bytes("commissioning/run/points.json"),
            ),
            ("main.csv".to_owned(), case.main.clone().into_bytes()),
            ("limit.csv".to_owned(), case.limit.clone().into_bytes()),
            ("outputs.csv".to_owned(), case.outputs.clone().into_bytes()),
            ("fan.csv".to_owned(), case.fan.clone().into_bytes()),
        ],
    };
    commission(&spec, &inputs, &CommissionOptions::default())
        .unwrap_or_else(|error| panic!("{}: {} {error}", case.label, error.code()))
}

/// What the two paths of one output must agree on.
///
/// - A Boolean or Integer output: the same status and the same time outside, to the bit.
///   Every event is on the 1/64 s grid and both series are constant between events, so the
///   grid's sum of 1/64 s steps is the change-of-value path's exact time.
/// - A Real output: the same status, and the change-of-value time outside not below the grid's
///   by more than [`grid_quantisation`] allows.
///
/// The documented differences: a time, [`grid_quantisation`], and a status,
/// [`grid_ramp_at_stop`]. No other status may differ. (The grid path draws every reference step
/// as a ramp one grid step wide, which could part the verdicts at a band corner inside the
/// window too; with every event on the grid and the corner snap, no drawn case does, so no
/// exception is allowed for it.)
fn check_output(
    label: &str,
    change: &OutputReport,
    grid: &OutputReport,
    grid_runs: usize,
) -> Result<(), String> {
    let discrete = change.funnel_reference.is_none();
    let fail = |what: &str| {
        Err(format!(
            "{label}: {} {what}: change of value {:?} {} s, grid {:?} {} s",
            change.output,
            change.status,
            change.exceedance_time_s,
            grid.status,
            grid.exceedance_time_s
        ))
    };
    if discrete {
        if change.status != grid.status || change.exceedance_time_s != grid.exceedance_time_s {
            return fail("discrete figures differ");
        }
        return Ok(());
    }
    if change.status != grid.status && !grid_ramp_at_stop(change, grid) {
        return fail("status differs");
    }
    if change.exceedance_time_s < grid.exceedance_time_s - grid_quantisation(grid_runs) {
        return fail("the change-of-value time outside is below the grid's by more than its edges");
    }
    Ok(())
}

const GRID_STEP: f64 = 1.0 / 64.0;

/// The window's stop in these cases, seconds.
const STOP: f64 = 30.0;

/// A reference that steps at exactly `stop`. The grid draws the step as a ramp over the last
/// grid step, so the grid's band at `stop` is the new value's alone and a test still at the old
/// value there fails at that one instant (a time outside of zero). The change-of-value path
/// draws a step: its band at `stop` spans the step's vertical, which holds the old value, so
/// the test is inside and the output passes. The step lasts no time in the window, and a
/// vertical counts no time (the orchestrator's reading 5). So: the change-of-value output
/// passes, the grid's fails, and its only excess is at `stop`.
fn grid_ramp_at_stop(change: &OutputReport, grid: &OutputReport) -> bool {
    change.status == OutputStatus::Pass
        && grid.status == OutputStatus::Fail
        && grid.exceedance_time_s == 0.0
        && grid.first_exceedance_t == Some(STOP)
}

/// The grid counts a whole grid step for each instant outside, so at each end of a run of
/// such instants it can count up to one step the test was not outside: a band corner a
/// fallback half-width of `1e-10` s past an instant, or a sloped band's crossing, falls inside
/// a grid step. The change-of-value path reads those ends where they are, so its time can be
/// below the grid's by up to two grid steps per run of the grid's instants outside, and never
/// by more.
fn grid_quantisation(runs: usize) -> f64 {
    2.0 * GRID_STEP * f64::from(u32::try_from(runs).expect("fits"))
}

/// The runs of instants with an error above zero in an exceedance column (a masked instant's
/// error counted too, so the count is an upper bound).
fn runs_outside(errors: &[f64]) -> usize {
    let mut runs = 0;
    let mut inside = true;
    for error in errors {
        let outside = *error > 0.0;
        if outside && inside {
            runs += 1;
        }
        inside = !outside;
    }
    runs
}

#[test]
fn the_change_of_value_path_agrees_with_a_fine_grid_on_seeded_cases() {
    let mut failures = Vec::new();
    let mut compared = 0;
    let mut failing = 0;
    for seed in 0..400 {
        let case = generate(0x7461_6e67_6c65_0000 + seed);
        let change = run(&case, None);
        let grid = run(&case, Some("0.015625"));
        for ((c, g), curves) in body_of(&change.receipt)
            .outputs
            .iter()
            .zip(&body_of(&grid.receipt).outputs)
            .zip(&grid.curves)
        {
            assert_eq!(c.output, g.output);
            assert_eq!(curves.output, g.output);
            compared += 1;
            failing += usize::from(c.status == OutputStatus::Fail);
            let runs = runs_outside(curves.errors.y());
            if let Err(message) = check_output(&case.label, c, g, runs) {
                failures.push(message);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {compared}:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // The cases are not all one verdict: both pass and fail are drawn.
    assert!(
        failing > compared / 10 && failing < compared * 9 / 10,
        "{failing} of {compared}"
    );
}
