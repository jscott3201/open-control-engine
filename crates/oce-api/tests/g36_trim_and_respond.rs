//! G36 Generic.TrimAndRespond `have_hol=false` facade simulation evidence.

use oce_api::{CollectSpec, Engine, InputSource, PointDirection, SimMetrics, SimSpec, Value};

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
    let requests = if (840.0..1080.0).contains(&t) {
        6
    } else if (720.0..840.0).contains(&t) {
        3
    } else {
        0
    };
    let device_status = !(1080.0..1260.0).contains(&t);
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

fn simulate(mut engine: Engine) -> (ScheduleSignature, SimMetrics) {
    let schedule = schedule_signature(&engine);
    let metrics = engine
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop: 1320.0,
            step: 60.0,
            inputs: InputSource::Closure(Box::new(inputs)),
            collect: CollectSpec::Named {
                points: vec![SETPOINT_PATH.to_string()],
                stride: 1,
            },
        })
        .expect("G36 TrimAndRespond simulates");
    assert_eq!(metrics.ticks, 23);
    assert_eq!(metrics.trace.columns(), &[SETPOINT_PATH.to_string()]);
    assert_all_finite(&metrics);
    (schedule, metrics)
}

fn assert_all_finite(metrics: &SimMetrics) {
    for (row, value) in column(metrics).iter().enumerate() {
        match value {
            Value::Real(x) => assert!(x.is_finite(), "setpoint[{row}] must stay finite, got {x}"),
            other => panic!("setpoint[{row}] must be Real, got {other:?}"),
        }
    }
}

fn column(metrics: &SimMetrics) -> &[Value] {
    metrics
        .trace
        .column(0)
        .expect("setpoint trace column should exist")
}

fn real_at(metrics: &SimMetrics, row: usize) -> f64 {
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

fn assert_trace_bit_eq(left: &SimMetrics, right: &SimMetrics) {
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
    assert_close(real_at(&metrics_a, 0), 10.0);
    assert_close(real_at(&metrics_a, 12), 9.9);
    assert!(
        real_at(&metrics_a, 14) < real_at(&metrics_a, 12),
        "larger requests should apply the capped negative response"
    );
    assert_close(real_at(&metrics_a, 18), 10.0);
}
