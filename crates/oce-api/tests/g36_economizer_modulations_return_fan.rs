//! Source-verified ASHRAE G36 Economizers.Subsequences.Modulations.ReturnFan through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, SimMetrics, SimSpec, Value};

const ECONOMIZER_MODULATIONS_RETURN_FAN: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_modulations_return_fan.jsonld"
);

const SUPPLY_TEMPERATURE_SIGNAL: &str = "conn#6";
const RETURN_DAMPER_MIN: &str = "conn#5";
const RETURN_DAMPER_MAX: &str = "conn#3";

const RETURN_DAMPER_COMMAND: &str = "conn#7";
const OUTDOOR_DAMPER_COMMAND: &str = "conn#8";

const EXPECTED_TIMES: [f64; 7] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
const SUPPLY_TEMPERATURE_SIGNAL_VALUES: [f64; 7] = [-0.5, -0.25, -0.125, 0.0, 0.125, 0.25, 0.5];
const EXPECTED_RETURN_DAMPER_COMMAND: [f64; 7] =
    [0.75, 0.75, 0.59375, 0.4375, 0.28125, 0.125, 0.125];
const EXPECTED_OUTDOOR_DAMPER_COMMAND: [f64; 7] = [1.0; 7];

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn row_index(t: f64) -> usize {
    EXPECTED_TIMES
        .iter()
        .position(|expected| expected.to_bits() == t.to_bits())
        .unwrap_or_else(|| panic!("unexpected test instant {t}"))
}

fn economizer_modulations_return_fan_inputs(t: f64) -> Vec<(String, Value)> {
    let row = row_index(t);
    vec![
        pair(
            SUPPLY_TEMPERATURE_SIGNAL,
            Value::Real(SUPPLY_TEMPERATURE_SIGNAL_VALUES[row]),
        ),
        pair(RETURN_DAMPER_MIN, Value::Real(0.125)),
        pair(RETURN_DAMPER_MAX, Value::Real(0.75)),
    ]
}

fn load_economizer_modulations_return_fan() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(ECONOMIZER_MODULATIONS_RETURN_FAN.as_bytes())
        .expect("source-verified G36 Economizers.Subsequences.Modulations.ReturnFan fixture loads");
    assert_eq!(report.block_count, 4);
    assert_eq!(report.stateful_blocks, 0);
    assert!(
        report.warnings.is_empty(),
        "fixture should not warn: {:?}",
        report.warnings
    );
    engine
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
            t_stop: 6.0,
            step: 1.0,
            inputs: InputSource::Closure(Box::new(economizer_modulations_return_fan_inputs)),
            collect: CollectSpec::Named {
                points: vec![
                    RETURN_DAMPER_COMMAND.to_string(),
                    OUTDOOR_DAMPER_COMMAND.to_string(),
                ],
                stride: 1,
            },
        })
        .expect("G36 Economizers.Subsequences.Modulations.ReturnFan simulates");
    assert_eq!(metrics.ticks, EXPECTED_TIMES.len() as u64);
    assert_eq!(
        metrics
            .trace
            .times()
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>(),
        EXPECTED_TIMES
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>()
    );
    (schedule, metrics)
}

fn real_column(metrics: &SimMetrics, path: &str) -> Vec<f64> {
    let index = metrics
        .trace
        .columns()
        .iter()
        .position(|column| column == path)
        .unwrap_or_else(|| panic!("missing trace column {path}"));
    let column = metrics.trace.column(index).expect("column index is valid");
    column
        .iter()
        .map(|value| match value {
            Value::Real(x) => *x,
            other => panic!("{path} must be Real, got {other:?}"),
        })
        .collect()
}

fn assert_real_bits(actual: &[f64], expected: &[f64], label: &str) {
    assert_eq!(
        actual.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        expected.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        "{label} diverged"
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
    for j in 0..left.trace.columns().len() {
        let l = left.trace.column(j).unwrap();
        let r = right.trace.column(j).unwrap();
        for (row, (lv, rv)) in l.iter().zip(r).enumerate() {
            assert!(
                lv.bit_eq(rv),
                "{} row {row} diverged: {lv:?} vs {rv:?}",
                left.trace.columns()[j]
            );
        }
    }
}

#[test]
fn multizone_vav_economizer_modulations_return_fan_loads_simulates_and_is_deterministic() {
    let engine = load_economizer_modulations_return_fan();
    let points = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in [
        SUPPLY_TEMPERATURE_SIGNAL,
        RETURN_DAMPER_MIN,
        RETURN_DAMPER_MAX,
    ] {
        assert!(
            points.contains(&input.to_string()),
            "missing facade input {input}"
        );
        assert_eq!(
            points.iter().filter(|path| path.as_str() == input).count(),
            1,
            "source input {input} should expose one logical facade point"
        );
    }
    for output in [RETURN_DAMPER_COMMAND, OUTDOOR_DAMPER_COMMAND] {
        assert!(
            points.contains(&output.to_string()),
            "missing runtime output {output}"
        );
    }

    let (schedule, metrics) = simulate(engine);
    assert_real_bits(
        &real_column(&metrics, RETURN_DAMPER_COMMAND),
        &EXPECTED_RETURN_DAMPER_COMMAND,
        "return damper command",
    );
    assert_real_bits(
        &real_column(&metrics, OUTDOOR_DAMPER_COMMAND),
        &EXPECTED_OUTDOOR_DAMPER_COMMAND,
        "outdoor damper command",
    );

    let (second_schedule, second_metrics) = simulate(load_economizer_modulations_return_fan());
    assert_eq!(schedule, second_schedule);
    assert_trace_bit_eq(&metrics, &second_metrics);
}
