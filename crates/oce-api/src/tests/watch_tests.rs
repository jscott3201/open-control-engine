//! Key-selected output read behavior, determinism, and independent oracle checks.

use std::collections::HashSet;

use super::common::*;

const AHU_SAT_RESET: &str =
    include_str!("../../../oce-cxf/tests/fixtures/g36/ahu_supply_air_temp_reset.jsonld");

#[test]
fn selected_g36_outputs_match_the_independent_outputs_snapshot_path() {
    let mut engine = Engine::in_memory();
    engine
        .load_cxf(AHU_SAT_RESET.as_bytes())
        .expect("G36 fixture loads");
    let internal_ids: HashSet<ConnectorId> = engine
        .model
        .connections
        .iter()
        .map(|connection| connection.from)
        .collect();
    let paths = crate::engine::out_connector_paths(&engine.model);
    let selected: Vec<&str> = engine
        .model
        .connectors
        .iter()
        .filter(|connector| connector.dir == Dir::Out && internal_ids.contains(&connector.id))
        .zip(paths.iter())
        .take(3)
        .map(|(_, path)| path.as_str())
        .collect();
    assert_eq!(
        selected.len(),
        3,
        "fixture must expose three internal outputs"
    );

    for step in 0..=2 {
        engine.tick(step as f64).expect("G36 fixture ticks");
        let all_outputs = engine.outputs().to_map();
        let distinct_paths: HashSet<&str> =
            all_outputs.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(
            distinct_paths.len(),
            all_outputs.len(),
            "cross-check fixture must not contain duplicated output paths"
        );

        let watched = engine
            .watch(&selected)
            .expect("valid output keys are watchable");
        for ((watched_path, watched_value), selected_path) in watched.iter().zip(&selected) {
            assert_eq!(watched_path, selected_path);
            let snapshot_value = all_outputs
                .iter()
                .find(|(path, _)| path == selected_path)
                .map(|(_, value)| value)
                .expect("selected path exists in all-output snapshot");
            assert!(watched_value.bit_eq(snapshot_value));
        }
    }
}

fn chain_model() -> (ModelGraph, [ConnectorId; 3]) {
    let mut mb = Mb::new();
    let (_, _, constant) = mb.block(
        "CDL.Reals.Sources.Constant",
        &[],
        &[ValueType::Real],
        vec![rp("k", 2.0)],
    );
    let (_, first_inputs, first) = mb.block(
        "CDL.Reals.MultiplyByParameter",
        &[ValueType::Real],
        &[ValueType::Real],
        vec![rp("k", 3.0)],
    );
    let (_, second_inputs, second) = mb.block(
        "CDL.Reals.MultiplyByParameter",
        &[ValueType::Real],
        &[ValueType::Real],
        vec![rp("k", 5.0)],
    );
    mb.connect(constant[0], first_inputs[0]);
    mb.connect(first[0], second_inputs[0]);
    (mb.finish(), [constant[0], first[0], second[0]])
}

fn loaded_chain() -> Engine<MemStore> {
    let (model, _) = chain_model();
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(model, None)
        .expect("three-block chain builds");
    engine
}

#[test]
fn chained_outputs_match_hand_computed_exact_literals() {
    let (model, output_ids) = chain_model();
    assert_eq!(output_ids, [ConnectorId(0), ConnectorId(2), ConnectorId(4)]);
    let driven: HashSet<ConnectorId> = model
        .connections
        .iter()
        .map(|connection| connection.from)
        .collect();
    assert!(driven.contains(&output_ids[0]));
    assert!(driven.contains(&output_ids[1]));
    assert!(!driven.contains(&output_ids[2]));

    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(model, None)
        .expect("three-block chain builds");
    for step in 0..=2 {
        engine.tick(step as f64).expect("chain ticks");
        let watched = engine
            .watch(&["conn#0", "conn#2", "conn#4"])
            .expect("chain outputs resolve");
        assert_eq!(
            watched
                .iter()
                .map(|(path, _)| path.as_str())
                .collect::<Vec<_>>(),
            ["conn#0", "conn#2", "conn#4"]
        );
        for ((_, actual), expected) in
            watched
                .iter()
                .zip([Value::Real(2.0), Value::Real(6.0), Value::Real(30.0)])
        {
            assert!(actual.bit_eq(&expected), "{actual:?} != {expected:?}");
        }
    }
}

#[test]
fn caller_order_duplicates_and_first_error_are_preserved() {
    let mut engine = loaded_chain();
    engine.tick(0.0).expect("chain ticks");

    let duplicated = engine
        .watch(&["conn#4", "conn#0", "conn#4"])
        .expect("duplicate valid keys resolve");
    assert_eq!(
        duplicated
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>(),
        ["conn#4", "conn#0", "conn#4"]
    );
    assert!(duplicated[0].1.bit_eq(&duplicated[2].1));

    assert!(matches!(
        engine.watch(&["conn#0", "bogus", "conn#4"]),
        Err(OcError::UnknownPoint(path)) if path == "bogus"
    ));
    assert!(matches!(
        engine.watch(&["bogus-a", "bogus-b"]),
        Err(OcError::UnknownPoint(path)) if path == "bogus-a"
    ));
    assert!(matches!(
        engine.watch(&["conn#1"]),
        Err(OcError::UnknownPoint(path)) if path == "conn#1"
    ));
}

#[test]
fn named_simulation_trace_matches_manual_watch_loop() {
    let mut simulated = loaded_chain();
    let metrics = simulated
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop: 3.0,
            step: 1.0,
            inputs: InputSource::None,
            collect: CollectSpec::Named {
                points: vec!["conn#2".to_owned()],
                stride: 1,
            },
        })
        .expect("chain simulates");
    assert_eq!(metrics.trace.columns(), ["conn#2"]);
    assert_eq!(metrics.trace.times(), [0.0, 1.0, 2.0, 3.0]);

    let mut manual = loaded_chain();
    let mut watched = Vec::new();
    for step in 0..=3 {
        manual.tick(step as f64).expect("manual chain ticks");
        watched.push(
            manual.watch(&["conn#2"]).expect("internal output resolves")[0]
                .1
                .clone(),
        );
    }
    let recorded = metrics.trace.column(0).expect("named trace has one column");
    assert_eq!(recorded.len(), watched.len());
    assert!(
        recorded
            .iter()
            .zip(&watched)
            .all(|(left, right)| left.bit_eq(right))
    );

    let post_simulate = simulated
        .watch(&["conn#2"])
        .expect("watch remains available after simulate");
    assert!(post_simulate[0].1.bit_eq(&Value::Real(6.0)));
}

#[test]
fn identical_engines_produce_bit_identical_selected_snapshots() {
    let mut left = loaded_chain();
    let mut right = loaded_chain();
    let points = ["conn#4", "conn#0", "conn#2"];
    for step in 0..=4 {
        left.tick(step as f64).expect("left engine ticks");
        right.tick(step as f64).expect("right engine ticks");
        let left_values = left.watch(&points).expect("left snapshot resolves");
        let right_values = right.watch(&points).expect("right snapshot resolves");
        assert!(left_values.iter().zip(&right_values).all(
            |((left_path, left_value), (right_path, right_value))| {
                left_path == right_path && left_value.bit_eq(right_value)
            }
        ));
    }
}

#[test]
fn selected_reads_are_available_after_realtime_step() {
    let mut engine = loaded_chain();
    engine
        .step_realtime(0.0)
        .expect("realtime chain step succeeds");
    let watched = engine
        .watch(&["conn#4"])
        .expect("watch remains available after realtime step");
    assert!(watched[0].1.bit_eq(&Value::Real(30.0)));
}
