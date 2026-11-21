//! What replay makes of the merged inputs: a hold at exactly `maxAge` and one ulp beyond, equal
//! times as their own frames, and no hold leaving the run-level refusal in place.

use oce_replay::{FrameStatus, TraceColumn};

use super::{point, read_trends, replay_signals, trend_spec_with};

/// The `signals` subject's five inputs, mapped one to one with no units.
fn mapping() -> String {
    format!(
        "[{}, {}, {}, {}, {}]",
        point(("ahu.T", None, "float"), ("A", None, "float")),
        point(("zone.T", None, "float"), ("Z", None, "float")),
        point(("n", None, "int"), ("N", None, "int")),
        point(("b", None, "bool"), ("B", None, "bool")),
        point(("lim", None, "float"), ("L", None, "float")),
    )
}

/// Two input sources: `main.csv` with four inputs, `lim.csv` with `lim` under `lim_hold`.
fn statuses(main: &str, lim: &str, lim_hold: &str) -> (Vec<f64>, Vec<FrameStatus>, Vec<f64>) {
    let spec = trend_spec_with(
        None,
        &mapping(),
        &format!(
            r#""inputs": [
              {{"path": "main.csv", "format": "csv", "timeColumn": "time",
                "columns": {{"A": "A", "Z": "Z", "N": "N", "B": "B"}}}},
              {{"path": "lim.csv", "format": "csv", "timeColumn": "time",
                "columns": {{"L": "L"}}{lim_hold}}}]"#
        ),
    );
    let read = read_trends(
        &spec,
        &[("main.csv", main.as_bytes()), ("lim.csv", lim.as_bytes())],
    )
    .expect("reads");
    let times = read.inputs.times().to_vec();
    let trace = replay_signals(read.inputs);
    let status = trace
        .status()
        .iter()
        .map(|byte| FrameStatus::from_byte(*byte).expect("a status"))
        .collect();
    let inputs = trace.inputs().expect("retained");
    let lim = inputs
        .iter()
        .find(|(path, _)| path.ends_with(".lim"))
        .map(|(_, column)| match column {
            TraceColumn::Real(values) => values.clone(),
            other => panic!("{other:?}"),
        })
        .expect("lim");
    (times, status, lim)
}

const MAIN: &str = "time,A,Z,N,B\n0,1,2,3,true\n10,1,2,3,true\n10.000000000000002,1,2,3,true\n";

#[test]
fn a_hold_at_exactly_max_age_substitutes_and_one_ulp_beyond_refuses() {
    let (times, status, lim) = statuses(MAIN, "time,L\n0,4\n", r#", "hold": {"maxAge": 10}"#);
    assert_eq!(times, [0.0, 10.0, 10.0_f64.next_up()]);
    assert_eq!(
        status,
        [
            FrameStatus::Executed,
            FrameStatus::Substituted,
            FrameStatus::RefusedMissing
        ]
    );
    assert_eq!(lim[1], 4.0, "held from t = 0 at age exactly 10");
    // The same at a hold one ulp longer: all three run.
    let longer = format!(r#", "hold": {{"maxAge": {}}}"#, 10.0_f64.next_up());
    let (_, status, _) = statuses(MAIN, "time,L\n0,4\n", &longer);
    assert_eq!(status[2], FrameStatus::Substituted);
}

#[test]
fn with_no_hold_a_missing_sample_refuses_its_frame() {
    let (_, status, _) = statuses(MAIN, "time,L\n0,4\n", "");
    assert_eq!(
        status,
        [
            FrameStatus::Executed,
            FrameStatus::RefusedMissing,
            FrameStatus::RefusedMissing
        ]
    );
}

#[test]
fn equal_times_are_two_frames_and_the_second_holds_the_first_rows_sample() {
    let main = "time,A,Z,N,B\n0,1,2,3,true\n5,1,2,3,true\n5,1,2,3,false\n";
    let (times, status, lim) = statuses(main, "time,L\n0,4\n5,6\n", r#", "hold": {"maxAge": 0}"#);
    assert_eq!(times, [0.0, 5.0, 5.0]);
    // At the second frame at 5, `lim.csv` has no second row: replay holds its row at 5, of age
    // zero, which `maxAge: 0` admits.
    assert_eq!(
        status,
        [
            FrameStatus::Executed,
            FrameStatus::Executed,
            FrameStatus::Substituted
        ]
    );
    assert_eq!(lim, [4.0, 6.0, 6.0]);
    // Two files with one row each at an instant give one frame.
    let (times, status, _) = statuses("time,A,Z,N,B\n5,1,2,3,true\n", "time,L\n5,6\n", "");
    assert_eq!((times, status), (vec![5.0], vec![FrameStatus::Executed]));
}
