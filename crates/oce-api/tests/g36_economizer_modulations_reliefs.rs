//! Source-verified ASHRAE G36 Economizers.Subsequences.Modulations.Reliefs through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, SimMetrics, SimSpec, Value};

const ECONOMIZER_MODULATIONS_RELIEFS: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_modulations_reliefs.jsonld"
);

const SUPPLY_TEMPERATURE_SIGNAL: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.uTSup";
const OUTDOOR_DAMPER_MIN: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.uOutDam_min";
const OUTDOOR_DAMPER_MAX: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.uOutDam_max";
const RETURN_DAMPER_MIN: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.uRetDam_min";
const RETURN_DAMPER_MAX: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.uRetDam_max";

const OUTDOOR_DAMPER_COMMAND: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.min.y";
const RETURN_DAMPER_COMMAND: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.max.y";

const EXPECTED_TIMES: [f64; 7] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
const SUPPLY_TEMPERATURE_SIGNAL_VALUES: [f64; 7] = [-0.5, -0.25, -0.125, 0.0, 0.125, 0.25, 0.5];
const EXPECTED_OUTDOOR_DAMPER_COMMAND: [f64; 7] = [0.25, 0.25, 0.5625, 0.875, 0.875, 0.875, 0.875];
const EXPECTED_RETURN_DAMPER_COMMAND: [f64; 7] = [0.75, 0.75, 0.75, 0.75, 0.4375, 0.125, 0.125];

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

fn economizer_modulations_reliefs_inputs(t: f64) -> Vec<(String, Value)> {
    let row = row_index(t);
    vec![
        pair(
            SUPPLY_TEMPERATURE_SIGNAL,
            Value::Real(SUPPLY_TEMPERATURE_SIGNAL_VALUES[row]),
        ),
        pair(OUTDOOR_DAMPER_MIN, Value::Real(0.25)),
        pair(OUTDOOR_DAMPER_MAX, Value::Real(0.875)),
        pair(RETURN_DAMPER_MIN, Value::Real(0.125)),
        pair(RETURN_DAMPER_MAX, Value::Real(0.75)),
    ]
}

fn load_economizer_modulations_reliefs() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(ECONOMIZER_MODULATIONS_RELIEFS.as_bytes())
        .expect("source-verified G36 Economizers.Subsequences.Modulations.Reliefs fixture loads");
    assert_eq!(report.block_count, 8);
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
            inputs: InputSource::Closure(Box::new(economizer_modulations_reliefs_inputs)),
            collect: CollectSpec::Named {
                points: vec![
                    OUTDOOR_DAMPER_COMMAND.to_string(),
                    RETURN_DAMPER_COMMAND.to_string(),
                ],
                stride: 1,
            },
        })
        .expect("G36 Economizers.Subsequences.Modulations.Reliefs simulates");
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
fn multizone_vav_economizer_modulations_reliefs_loads_simulates_and_is_deterministic() {
    let engine = load_economizer_modulations_reliefs();
    let points = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in [
        SUPPLY_TEMPERATURE_SIGNAL,
        OUTDOOR_DAMPER_MIN,
        OUTDOOR_DAMPER_MAX,
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
    for output in [OUTDOOR_DAMPER_COMMAND, RETURN_DAMPER_COMMAND] {
        assert!(
            points.contains(&output.to_string()),
            "missing runtime output {output}"
        );
    }

    let (schedule, metrics) = simulate(engine);
    assert_real_bits(
        &real_column(&metrics, OUTDOOR_DAMPER_COMMAND),
        &EXPECTED_OUTDOOR_DAMPER_COMMAND,
        "outdoor damper command",
    );
    assert_real_bits(
        &real_column(&metrics, RETURN_DAMPER_COMMAND),
        &EXPECTED_RETURN_DAMPER_COMMAND,
        "return damper command",
    );

    let (second_schedule, second_metrics) = simulate(load_economizer_modulations_reliefs());
    assert_eq!(schedule, second_schedule);
    assert_trace_bit_eq(&metrics, &second_metrics);
}
