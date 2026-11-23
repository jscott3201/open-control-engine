//! The Building 33 re-enable that follows a one-frame disable, against OpenModelica (issue #800;
//! the resolved known divergence `DVG-000001`).
//!
//! The enable falls at 377 s and rises at 382 s, consecutive frames. Modelica's `Change` is true
//! at each instant, so the PI's `when trigger` fires at both and OpenModelica reinitialises the
//! integrator at 382 s: the valve is `lim(y_reset) = 0` there. Under HostTick v1 `Change` holds
//! its output for a frame, true at 377 s and at 382 s; under the settled-value rule (design note
//! `docs/design/pulse-event-inputs.md`) it is a primitive pulse, so `trigger` fires on both frames
//! and the engine resets at 382 s too. `CoolingCoilValveReenableProbe.mo` runs the same frames with
//! the supply air at 290 K from 382 s, so that the PI's error at the re-enable (5 K) differs from
//! the error at the disable (3 K) and a missing reset would show.
//!
//! Before the rule the engine did not reset at 382 s and stayed `k * (5 - 3) = 0.036` above
//! OpenModelica from 382 s on: `yCooVal_reenable_probe.engine.csv` keeps that signal as the
//! evidence of the resolved register entry.

use super::building_33_day::example_file;
use super::building_33_openmodelica_frames::{
    EXACT, Frame, openmodelica_signal, refined, resets, sha256,
};
use super::building_33_transcription::{OracleRow, ROOT, oracle_rows};

use oce_api::{Engine, Value};

const PROBE_FROM: f64 = 382.0;
const PROBE_SUPPLY: f64 = 290.0;
const REGISTER: &str =
    include_str!("../../../../oce-conformance/tests/fixtures/known_divergence/register.json");

/// The oracle's frames with the supply air at 290 K from 382 s, as `frames_wrapper.py probe`
/// writes them.
fn probe_rows() -> Vec<OracleRow> {
    let mut rows = oracle_rows();
    for row in &mut rows {
        if row.t >= PROBE_FROM {
            row.inputs[0] = PROBE_SUPPLY;
        }
    }
    rows
}

/// The engine's valve signal over `rows`, one frame per row.
fn engine_signal(rows: &[OracleRow]) -> Vec<f64> {
    let mut engine = Engine::new();
    engine
        .load_cxf(&example_file("cooling_coil_valve.cxf.json"))
        .expect("the CXF loads");
    let names =
        ["TSup", "TSupSet", "TOut", "uFanFee", "uFanSta", "yCooVal"].map(|n| format!("{ROOT}{n}"));
    rows.iter()
        .map(|row| {
            let mut entries: Vec<(&str, Value)> = row
                .inputs
                .iter()
                .zip(&names)
                .map(|(x, name)| (name.as_str(), Value::Real(*x)))
                .collect();
            entries.push((names[4].as_str(), Value::Boolean(row.fan_sta)));
            let frame = engine.prepare_frame(row.t, &entries).expect("prepares");
            let done = engine.execute_frame(frame).expect("executes");
            match done.outputs().iter().find(|(path, _)| *path == names[5]) {
                Some((_, Value::Real(y))) => *y,
                other => panic!("t {}: {other:?}", row.t),
            }
        })
        .collect()
}

/// `yCooVal_reenable_probe.engine.csv`: `x,y`, the engine's signal recorded as 16 hex digits of
/// bits, one row per frame.
fn recorded_engine() -> Vec<(f64, f64)> {
    String::from_utf8(example_file(
        "openmodelica/yCooVal_reenable_probe.engine.csv",
    ))
    .expect("UTF-8")
    .lines()
    .skip(1)
    .map(|line| {
        let (x, bits) = line.split_once(',').expect("two fields");
        (
            x.parse().expect("a time"),
            f64::from_bits(u64::from_str_radix(bits, 16).expect("hex bits")),
        )
    })
    .collect()
}

fn probe() -> Vec<Frame> {
    openmodelica_signal("yCooVal_reenable_probe.csv")
}

#[test]
fn at_the_re_enable_after_a_one_frame_disable_the_engine_resets_as_openmodelica_does() {
    let rows = probe_rows();
    let openmodelica = probe();
    let engine = engine_signal(&rows);
    let (modelica, hosttick) = resets(&rows);
    assert_eq!(modelica.last(), Some(&382.0));
    assert_eq!(
        hosttick, modelica,
        "the engine resets at every change, 382 s included"
    );
    // Identical to the unprobed run before the probe: the same frames, the same signal.
    let frames = openmodelica_signal("yCooVal_frames.csv");
    for (a, b) in openmodelica.iter().zip(&frames) {
        if a.t < PROBE_FROM {
            assert_eq!(
                (a.y.to_bits(), a.reset),
                (b.y.to_bits(), b.reset),
                "t {} s",
                a.t
            );
        }
    }
    // OpenModelica, from 382 s: reset to 0, then one 5 s step of `k/Ti * 5 K` per frame.
    let tail: Vec<(f64, f64, bool)> = openmodelica
        .iter()
        .filter(|f| f.t >= PROBE_FROM)
        .map(|f| (f.t, f.y, f.reset))
        .collect();
    assert_eq!(tail.len(), 3);
    assert_eq!((tail[0].0, tail[0].1, tail[0].2), (382.0, 0.0, true));
    for ((t, y, _), want) in tail.iter().zip([0.0, 0.015, 0.030]) {
        assert!((y - want).abs() < 1e-9, "OpenModelica at {t} s: {y}");
    }
    // The engine, from 382 s: the reset, then the one frame of integral action every engine
    // reset trails by (the reset frame takes no Euler step): 0, 0, 0.015.
    let from = rows.len() - tail.len();
    assert!(
        (engine[from] - tail[0].1).abs() <= EXACT,
        "the reset at 382 s"
    );
    let lead = 0.018 / 30.0 * 5.0 * 5.0;
    for (k, (t, y, _)) in tail.iter().enumerate().skip(1) {
        let e = engine[from + k];
        assert!(
            (y - e - lead).abs() < 1e-9,
            "t {t} s: engine {e}, OpenModelica {y}"
        );
    }
    // Before 382 s the probe changes nothing for the engine either.
    let unprobed = engine_signal(&oracle_rows());
    for ((row, a), b) in rows.iter().zip(&engine).zip(&unprobed) {
        if row.t < PROBE_FROM {
            assert_eq!(a.to_bits(), b.to_bits(), "t {} s", row.t);
        }
    }
    // The engine before the rule (the resolved divergence's evidence) stayed 0.036 above
    // OpenModelica from 382 s on; it equals this run before 382 s.
    let recorded = recorded_engine();
    assert_eq!(recorded.len(), rows.len());
    assert!((recorded[from].1 - 0.036).abs() < 1e-12);
    for ((t, y), (row, e)) in recorded.iter().zip(rows.iter().zip(&engine)) {
        assert_eq!(t.to_bits(), row.t.to_bits());
        if row.t < 377.0 {
            assert_eq!(y.to_bits(), e.to_bits(), "t {t} s");
        }
    }
}

#[test]
fn on_finer_frames_the_engine_resets_at_382_s_too_and_meets_openmodelica_there() {
    // At 1/64 s frames the disable and the re-enable are 320 frames apart, so the engine resets
    // at both under any rule: the divergence the settled-value rule resolved was the projection
    // of two events onto consecutive frames.
    let rows = probe_rows();
    let openmodelica = probe();
    let engine = refined(&rows, 1.0 / 64.0);
    let at = rows
        .iter()
        .position(|row| row.t == PROBE_FROM)
        .expect("382 s");
    assert!((engine[at] - openmodelica[at].y).abs() <= EXACT);
    for (frame, y) in openmodelica.iter().zip(&engine).skip(at) {
        assert!((y - frame.y).abs() < 1e-3, "t {} s", frame.t);
    }
}

#[test]
fn the_probe_run_is_recorded_and_the_resolved_divergence_keeps_its_evidence() {
    let record: serde_json::Value =
        serde_json::from_slice(&example_file("openmodelica/provenance_frames.json"))
            .expect("provenance JSON");
    let field = |key: &str| record[key].as_str().expect(key).to_owned();
    let wrapper = example_file("openmodelica/CoolingCoilValveReenableProbe.mo");
    assert_eq!(field("probe_wrapper_sha256"), sha256(&wrapper));
    let signal = example_file("openmodelica/yCooVal_reenable_probe.csv");
    assert_eq!(field("probe_signal_sha256"), sha256(&signal));
    assert_eq!(
        field("probe_raw_sha256"),
        sha256(&example_file(
            "openmodelica/CoolingCoilValveReenableProbe.raw.csv"
        ))
    );
    // The probe's table is the oracle's with the supply air at 290 K from 382 s.
    let text = String::from_utf8(wrapper).expect("UTF-8");
    for row in probe_rows() {
        let [t_sup, t_sup_set, t_out, fan_fee] = row.inputs;
        let fan = if row.fan_sta { 1 } else { 0 };
        let line = format!(
            "      {:?}, {t_sup:?}, {t_sup_set:?}, {t_out:?}, {fan_fee:?}, {fan}",
            row.t
        );
        assert!(text.contains(&line), "{line}");
    }
    let engine = example_file("openmodelica/yCooVal_reenable_probe.engine.csv");
    assert!(REGISTER.contains(r#""id": "DVG-000001""#));
    let entry = &REGISTER[REGISTER.find(r#""id": "DVG-000001""#).expect("entry")..];
    let entry = &entry[..entry.find(r#""conformance_effect""#).expect("end")];
    assert!(
        entry.contains(r#""state": "resolved""#),
        "DVG-000001 is resolved"
    );
    for digest in [sha256(&engine), sha256(&signal)] {
        assert!(REGISTER.contains(&format!(r#""sha256": "{digest}""#)));
    }
    assert!(REGISTER.contains("https://github.com/jscott3201/open-control-engine/issues/800"));
}
