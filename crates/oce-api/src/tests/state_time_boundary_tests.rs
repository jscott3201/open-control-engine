//! Sample-clock acceptance and refusal at the exact signed-index boundaries.

use super::state_tests::sampled_model;
use crate::{Engine, OcError};

const UPPER_ACCEPTED: f64 = 9_223_372_036_854_773_760.0;
const UPPER_REFUSED: f64 = 9_223_372_036_854_775_808.0;
const LOWER_ACCEPTED: f64 = -9_223_372_036_854_775_808.0;
const LOWER_REFUSED: f64 = -9_223_372_036_854_777_856.0;

fn assert_refusal_is_unchanged(engine: &mut Engine, t_now: f64, class_path: &str) {
    let words = engine.state.words.clone();
    let values = engine.state.values.clone();
    let state_t = engine.state.t.to_bits();
    let prev_t = engine.prev_t.map(f64::to_bits);
    let ready = engine.durable_restore_ready;
    assert!(
        matches!(engine.tick(t_now), Err(OcError::ModelTimeUnrepresentable { now }) if now == t_now),
        "{class_path}"
    );
    assert_eq!(engine.state.words, words, "{class_path}");
    assert!(
        engine
            .state
            .values
            .iter()
            .zip(&values)
            .all(|(left, right)| left.bit_eq(right)),
        "{class_path}"
    );
    assert_eq!(engine.state.t.to_bits(), state_t, "{class_path}");
    assert_eq!(engine.prev_t.map(f64::to_bits), prev_t, "{class_path}");
    assert_eq!(engine.durable_restore_ready, ready, "{class_path}");
}

#[test]
fn initialized_clocks_accept_the_last_upper_index_then_refuse_two_to_the_sixty_three() {
    for class_path in [
        "CDL.Discrete.Sampler",
        "CDL.Discrete.ZeroOrderHold",
        "CDL.Discrete.FirstOrderHold",
        "CDL.Discrete.UnitDelay",
        "CDL.Logical.Sources.SampleTrigger",
    ] {
        let mut engine = Engine::in_memory();
        engine
            .build_model_in_memory(sampled_model(class_path, 1.0), None)
            .unwrap();
        engine.tick(0.0).unwrap();
        engine
            .tick(UPPER_ACCEPTED)
            .unwrap_or_else(|error| panic!("{class_path}: {error}"));
        assert_refusal_is_unchanged(&mut engine, UPPER_REFUSED, class_path);
    }
}

#[test]
fn sample_trigger_accepts_the_lower_bound_and_refuses_the_adjacent_float_below_it() {
    let class_path = "CDL.Logical.Sources.SampleTrigger";
    let mut accepted = Engine::in_memory();
    accepted
        .build_model_in_memory(sampled_model(class_path, 1.0), None)
        .unwrap();
    accepted.tick(LOWER_ACCEPTED).unwrap();
    assert_eq!(accepted.state.words[0].cast_signed(), -1);

    let mut refused = Engine::in_memory();
    refused
        .build_model_in_memory(sampled_model(class_path, 1.0), None)
        .unwrap();
    assert_refusal_is_unchanged(&mut refused, LOWER_REFUSED, class_path);
}
