//! G36 Generic.TrimAndRespond `have_hol=false` facade simulation evidence.

use oce_api::{Engine, PointDirection, Value};
#[path = "support/frame_trace.rs"]
mod frame_trace;
use frame_trace::FrameRun;

const TRIM_AND_RESPOND: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/trim_and_respond_have_hol_false.jsonld");

const REQUEST_COUNT: &str =
    "http://example.org#g36.source.trim_and_respond_have_hol_false.numOfReq";
const DEVICE_STATUS: &str = "http://example.org#g36.source.trim_and_respond_have_hol_false.uDevSta";
const HOLD_INPUT: &str = "http://example.org#g36.source.trim_and_respond_have_hol_false.uHol";
const SETPOINT_PATH: &str = "http://example.org#g36.source.trim_and_respond_have_hol_false.swi.y";

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

fn load_trim_and_respond() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(TRIM_AND_RESPOND.as_bytes())
        .expect("G36 TrimAndRespond loads");
    assert_eq!(report.block_count, 44);
    assert_eq!(report.stateful_blocks, 5);
    assert!(
        report.warnings.is_empty(),
        "G36 TrimAndRespond should load cleanly, got {:?}",
        report.warnings
    );

    assert!(
        engine
            .io()
            .iter()
            .any(|point| point.path.as_str() == REQUEST_COUNT)
    );
    assert!(
        engine
            .io()
            .iter()
            .any(|point| point.path.as_str() == DEVICE_STATUS)
    );
    assert!(
        !engine
            .io()
            .iter()
            .any(|point| point.path.as_str() == HOLD_INPUT),
        "inactive have_hol=false input must be pruned from facade IO"
    );
    assert!(
        engine
            .io()
            .iter()
            .any(|point| point.direction == PointDirection::Out && point.path == SETPOINT_PATH),
        "top setpoint output should be exposed as {SETPOINT_PATH}"
    );

    engine
}

fn inputs(t: f64) -> Vec<(String, Value)> {
    let requests = if (720.0..840.0).contains(&t) {
        3
    } else if (840.0..1080.0).contains(&t) {
        6
    } else if (2160.0..2280.0).contains(&t) {
        5
    } else if (2520.0..2760.0).contains(&t) || (3720.0..3960.0).contains(&t) {
        6
    } else {
        0
    };
    // A trip holding the 1320 s sample instant, a short trip strictly between the 2760 s and
    // 2880 s instants, and a boundary trip whose single off tick is the 3960 s instant.
    let device_status = !((1260.0..1380.0).contains(&t)
        || (2820.0..2880.0).contains(&t)
        || (3960.0..4020.0).contains(&t));
    vec![
        (REQUEST_COUNT.to_string(), Value::Integer(requests)),
        (DEVICE_STATUS.to_string(), Value::Boolean(device_status)),
    ]
}

fn schedule_signature(engine: &Engine) -> ScheduleSignature {
    let schedule = engine.schedule();
    (
        schedule.order.iter().map(|id| id.0).collect(),
        schedule.connector_order.iter().map(|id| id.0).collect(),
        schedule.driver_of.iter().map(|id| id.0).collect(),
    )
}

fn simulate(mut engine: Engine) -> (ScheduleSignature, FrameRun) {
    let schedule = schedule_signature(&engine);
    let metrics = FrameRun::record(
        &mut engine,
        0.0,
        4920.0,
        60.0,
        inputs,
        vec![SETPOINT_PATH.to_string()],
    )
    .expect("G36 TrimAndRespond simulates");
    assert_eq!(metrics.ticks, 83);
    assert_eq!(metrics.trace.columns(), &[SETPOINT_PATH.to_string()]);
    assert_all_finite(&metrics);
    (schedule, metrics)
}

fn assert_all_finite(metrics: &FrameRun) {
    for (row, value) in column(metrics).iter().enumerate() {
        match value {
            Value::Real(x) => assert!(x.is_finite(), "setpoint[{row}] must stay finite, got {x}"),
            other => panic!("setpoint[{row}] must be Real, got {other:?}"),
        }
    }
}

fn column(metrics: &FrameRun) -> &[Value] {
    metrics
        .trace
        .column(0)
        .expect("setpoint trace column should exist")
}

fn real_at(metrics: &FrameRun, row: usize) -> f64 {
    match &column(metrics)[row] {
        Value::Real(x) => *x,
        other => panic!("setpoint[{row}] must be Real, got {other:?}"),
    }
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-12,
        "expected {actual} to be within 1e-12 of {expected}"
    );
}

fn assert_trace_bit_eq(left: &FrameRun, right: &FrameRun) {
    assert_eq!(left.trace.columns(), right.trace.columns());
    assert_eq!(
        left.trace
            .times()
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>(),
        right
            .trace
            .times()
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>()
    );
    for (row, (left, right)) in column(left).iter().zip(column(right)).enumerate() {
        assert!(
            left.bit_eq(right),
            "setpoint row {row}: {left:?} vs {right:?}"
        );
    }
}

#[test]
fn trim_and_respond_have_hol_false_loads_simulates_and_is_deterministic() {
    let (schedule_a, metrics_a) = simulate(load_trim_and_respond());
    let (schedule_b, metrics_b) = simulate(load_trim_and_respond());

    assert_eq!(
        schedule_a, schedule_b,
        "TrimAndRespond schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    for (row, value) in column(&metrics_a).iter().enumerate() {
        let Value::Real(x) = value else {
            panic!("setpoint[{row}] must be Real, got {value:?}");
        };
        assert!(
            (0.0..=20.0).contains(x),
            "setpoint[{row}]={x} outside [0, 20]"
        );
    }
    assert_restart_reinitializes_the_reset(&metrics_a);
}

/// Pin the setpoint (row = t / 60 s) across three restarts.
///
/// `iniSet = 10`, `triAmo = 0.1`, `resAmo = -0.2`, `maxRes = -0.6`, `numIgnReq = 2`, and the
/// TrueDelay waits `delTim + samplePeriod = 720 s`. The chain reaches 9.1 by 1200 s. The device is
/// off for 1260..1379 s: `swi` publishes `iniSet`, and at the 1320 s instant `swi2` stages `iniSet`
/// into the UnitDelay while its output still holds 9.1. The restart at 1380 s falls between
/// instants, so `swi` must publish `iniSet` although the chain alone would read 9.1; the re-armed
/// TrueDelay releases at 2100 s and the reset resumes from `iniSet`.
///
/// The next two trips pin both sides of the reseed boundary in `TrimAndRespond.mo`'s graph
/// (GitHub issue #354): `iniSet` reaches the UnitDelay only at a sample instant, whatever the
/// block documentation says about the device being off.
///
/// - Short trip, 2820..2879 s, strictly between the 2760 s and 2880 s instants: `swi2` never
///   reaches the UnitDelay, which keeps the pre-trip 8.8. `swi` publishes `iniSet` through the
///   re-armed delay, and at the 3600 s release the reset resumes from the stale 8.8 (8.9), not from
///   `iniSet` (10.1).
/// - Boundary trip, 3960..4019 s, whose single off tick is the 3960 s instant: `swi2` stages
///   `iniSet` there, so at the 4740 s release the reset resumes from `iniSet` (10.1), not from the
///   stale 7.9 (8.0).
fn assert_restart_reinitializes_the_reset(metrics: &FrameRun) {
    // (first row, last row, setpoint) of each hold interval.
    let holds: [(usize, usize, f64); 22] = [
        (0, 11, 10.0),  // TrueDelay not yet elapsed: iniSet
        (12, 13, 9.9),  // 3 requests: 0.1 - 0.2
        (14, 15, 9.4),  // 6 requests: 0.1 - 0.6 (maxRes cap)
        (16, 17, 8.9),  // 6 requests again
        (18, 19, 9.0),  // no requests: trim
        (20, 20, 9.1),  // no requests: trim
        (21, 22, 10.0), // device off: iniSet; the 1320 s instant stages iniSet
        (23, 34, 10.0), // restart re-arms the TrueDelay: iniSet although 9.1 is held at 1380 s
        (35, 35, 10.1), // released at 2100 s: iniSet + trim
        (36, 37, 9.5),  // 5 requests at the 2160 s instant: iniSet + 0.1 - 0.6
        (38, 39, 9.6),  // no requests: trim
        (40, 41, 9.7),  // no requests: trim
        (42, 43, 9.2),  // 6 requests: 0.1 - 0.6
        (44, 45, 8.7),  // 6 requests again
        (46, 46, 8.8),  // no requests: trim
        (47, 59, 10.0), // short trip at 2820 s, no instant inside: iniSet through the re-armed delay
        (60, 61, 8.9),  // released at 3600 s from the stale 8.8, not iniSet: 8.8 + trim
        (62, 63, 8.4),  // 6 requests: 0.1 - 0.6
        (64, 65, 7.9),  // 6 requests again
        (66, 78, 10.0), // boundary trip on the 3960 s instant stages iniSet; re-armed delay
        (79, 81, 10.1), // released at 4740 s from iniSet, not the stale 7.9: iniSet + trim
        (82, 82, 10.2), // no requests: trim
    ];
    for (first, last, expected) in holds {
        for row in first..=last {
            assert_close(real_at(metrics, row), expected);
        }
    }
    // Reseed discriminators: each release is one trim from its seed, and at least one full
    // response away from the value the other seeding rule would give.
    let short_trip_release = real_at(metrics, 60);
    assert!(
        (short_trip_release - (10.0 + 0.1)).abs() > 1.0,
        "a short trip with no sample instant must not reseed iniSet, got {short_trip_release}"
    );
    let boundary_trip_release = real_at(metrics, 79);
    assert!(
        (boundary_trip_release - (7.9 + 0.1)).abs() > 1.0,
        "a trip holding a sample instant must reseed iniSet, got {boundary_trip_release}"
    );
}
