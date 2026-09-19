//! Structural allocation budget plus non-gating latency observations, on one synchronous thread.
//! Time is measured only for reporting; neither execution nor assertions depend on elapsed time.
//! These synthetic constant inputs are benchmark data, not a host default/quality policy.
#![allow(clippy::print_stdout)]

use allocation_counter::measure;
use oce_api::{AssertEvent, ConnectorId, Engine, Value};
use std::hint::black_box;
use std::time::Instant;

const CASES: &[(&str, &[u8], usize, usize)] = &[
    (
        "arithmetic",
        include_bytes!("fixtures/legacy_frame_add.jsonld"),
        1,
        0,
    ),
    ("delay", include_bytes!("fixtures/frame_delay.jsonld"), 1, 0),
    (
        "feedback",
        include_bytes!("fixtures/frame_pre.jsonld"),
        2,
        0,
    ),
    (
        "controller",
        include_bytes!("../../oce-cxf/tests/fixtures/g36/cooling_only_controller.jsonld"),
        10,
        0,
    ),
    (
        "assertion",
        include_bytes!("fixtures/assertion_model.jsonld"),
        0,
        1,
    ),
];

fn setup(bytes: &[u8]) -> (Engine, Vec<(String, Value)>) {
    let mut engine = Engine::in_memory();
    engine.load_cxf(bytes).unwrap();
    let observations = engine
        .input_definitions()
        .unwrap()
        .into_iter()
        .map(|d| {
            let value = d.min.unwrap_or_else(|| d.value_type.zero_value());
            (d.path, value)
        })
        .collect();
    (engine, observations)
}

#[test]
fn allocation_cost_is_only_prepared_targets_boundary_results_and_emitted_warnings() {
    let positive = measure(|| {
        black_box(vec![0_u8; 1024]);
    });
    assert_eq!(positive.count_total, 1);
    assert_eq!(positive.bytes_total, 1024);
    for &(name, bytes, output_count, warning_count) in CASES {
        let (mut engine, observations) = setup(bytes);
        let pairs: Vec<_> = observations
            .iter()
            .map(|(p, v)| (p.as_str(), v.clone()))
            .collect();
        let inputs = pairs.len();
        let targets = engine.topology().external_inputs.len();
        let plan = engine.prepare_frame(0.0, &pairs).unwrap();
        let sample = engine.execute_frame(plan).unwrap();
        assert_eq!(sample.outputs().len(), output_count);
        assert_eq!(sample.diagnostics().len(), warning_count);
        let output_bytes = output_count * size_of::<(String, Value)>()
            + sample.outputs().iter().map(|(p, _)| p.len()).sum::<usize>();
        // Current fixtures emit at most one warning: Vec's initial non-ZST capacity is four.
        let diagnostic_bytes = if warning_count == 0 {
            0
        } else {
            4 * size_of::<AssertEvent>() + "CDL.Utilities.Assert".len() + "freezestat tripped".len()
        };
        let preparation_count = if inputs == 0 { 0 } else { inputs + 2 };
        let preparation_bytes = inputs
            * (size_of::<Option<&Value>>() + size_of::<(Vec<ConnectorId>, Value)>())
            + targets * size_of::<ConnectorId>();
        let commit_count = output_count + usize::from(output_count != 0) + 3 * warning_count;
        let repetitions = 128;
        let census = measure(|| {
            for _ in 0..repetitions {
                let plan = engine.prepare_frame(0.0, &pairs).unwrap();
                black_box(engine.execute_frame(plan).unwrap());
            }
        });
        assert_eq!(
            census.count_total,
            (preparation_count + commit_count) as u64 * repetitions,
            "{name}"
        );
        assert_eq!(
            census.bytes_total,
            (preparation_bytes + output_bytes + diagnostic_bytes) as u64 * repetitions,
            "{name}"
        );
        assert_eq!(
            census.count_current, 0,
            "{name}: no retained temporary allocation"
        );
        assert_eq!(
            census.bytes_current, 0,
            "{name}: no retained temporary bytes"
        );
        println!(
            "frame allocation {name}: N={inputs} T={targets} B={output_count} D={warning_count} prepare={preparation_count}/{preparation_bytes}B commit={commit_count}/{}B repetitions={repetitions}",
            output_bytes + diagnostic_bytes
        );
    }
}

#[test]
fn report_latency_against_legacy_tick_without_a_flaky_ratio_threshold() {
    const BATCH: usize = 2048;
    const SAMPLES: usize = 5;
    for &(name, bytes, _, _) in CASES {
        let (mut native, observations) = setup(bytes);
        let (mut legacy, _) = setup(bytes);
        let pairs: Vec<_> = observations
            .iter()
            .map(|(p, v)| (p.as_str(), v.clone()))
            .collect();
        for (path, value) in &observations {
            legacy.set_input(path, value.clone()).unwrap();
        }
        for _ in 0..64 {
            let frame = native.prepare_frame(0.0, &pairs).unwrap();
            black_box(native.execute_frame(frame).unwrap());
            black_box(legacy.tick(0.0).unwrap());
        }
        let mut commit_samples = Vec::new();
        let mut tick_samples = Vec::new();
        let mut complete_samples = Vec::new();
        for sample in 0..SAMPLES {
            // Preparation and outer batch allocation excluded from commit-only; moved plans and
            // result destruction included. Legacy holds identical inputs and reads MemStore's
            // empty snapshots; it uses a no-op warning sink and retains no immutable result.
            let plans: Vec<_> = (0..BATCH)
                .map(|_| native.prepare_frame(0.0, &pairs).unwrap())
                .collect();
            let commit = |native: &mut Engine| {
                let start = Instant::now();
                for plan in plans {
                    black_box(native.execute_frame(plan).unwrap());
                }
                start.elapsed().as_nanos() / BATCH as u128
            };
            let tick = |legacy: &mut Engine| {
                let start = Instant::now();
                for _ in 0..BATCH {
                    black_box(legacy.tick(0.0).unwrap());
                }
                start.elapsed().as_nanos() / BATCH as u128
            };
            if sample % 2 == 0 {
                commit_samples.push(commit(&mut native));
                tick_samples.push(tick(&mut legacy));
            } else {
                tick_samples.push(tick(&mut legacy));
                commit_samples.push(commit(&mut native));
            }
            let start = Instant::now();
            for _ in 0..BATCH {
                let plan = native.prepare_frame(0.0, &pairs).unwrap();
                black_box(native.execute_frame(plan).unwrap());
            }
            complete_samples.push(start.elapsed().as_nanos() / BATCH as u128);
        }
        println!(
            "frame latency {name} {}/{} debug_assertions={} warmup=64 batch={BATCH} samples={SAMPLES} ns/op tick={tick_samples:?} commit={commit_samples:?} prepare+commit={complete_samples:?}",
            std::env::consts::ARCH,
            std::env::consts::OS,
            cfg!(debug_assertions)
        );
    }
}
