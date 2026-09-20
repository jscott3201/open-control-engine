//! Full-state split-run continuation oracle.

use super::common::{advance, output_image};
use oce_model::Value;

use crate::{Engine, EngineStateSnapshot};

const MINIMAL_LOOP: &[u8] = include_bytes!("../../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

const INPUTS: [(&str, Value); 1] = [("http://example.org#MinLoop.uSet", Value::Real(0.0))];

fn assert_same_state(left: &Engine, right: &Engine) {
    assert_eq!(left.state.values.len(), right.state.values.len());
    assert!(
        left.state
            .values
            .iter()
            .zip(&right.state.values)
            .all(|(left, right)| left.bit_eq(right))
    );
    assert_eq!(left.state.words, right.state.words);
    assert_eq!(left.state.t.to_bits(), right.state.t.to_bits());
    assert_eq!(
        left.prev_t.map(f64::to_bits),
        right.prev_t.map(f64::to_bits)
    );
    let left_outputs = output_image(left);
    let right_outputs = output_image(right);
    assert_eq!(left_outputs.len(), right_outputs.len());
    assert!(left_outputs.iter().zip(&right_outputs).all(
        |((left_key, left), (right_key, right))| { left_key == right_key && left.bit_eq(right) }
    ));
}

#[test]
fn durable_split_run_matches_every_continuation_tick_bit_exactly() {
    let mut uninterrupted = Engine::in_memory();
    uninterrupted.load_cxf(MINIMAL_LOOP).unwrap();
    advance(&mut uninterrupted, 0.0, &INPUTS).unwrap();
    advance(&mut uninterrupted, 1.0, &INPUTS).unwrap();
    let snapshot = uninterrupted.state_snapshot().unwrap();
    let decoded = EngineStateSnapshot::from_bytes(snapshot.as_bytes()).unwrap();

    let mut restored = Engine::in_memory();
    restored.load_cxf(MINIMAL_LOOP).unwrap();
    restored.restore_state(&decoded).unwrap();
    assert_same_state(&uninterrupted, &restored);

    for t in [2.0, 2.5, 4.0, 8.0] {
        let left = advance(&mut uninterrupted, t, &INPUTS).unwrap();
        let right = advance(&mut restored, t, &INPUTS).unwrap();
        assert_same_state(&uninterrupted, &restored);
        let diagnostics = |frame: &crate::CompletedFrame| {
            frame
                .diagnostics()
                .iter()
                .map(|e| (e.block.clone(), e.message.clone(), e.t.to_bits(), e.level))
                .collect::<Vec<_>>()
        };
        assert_eq!(diagnostics(&left), diagnostics(&right));
    }
}

#[test]
fn tuned_model_exports_reloads_and_continues_bit_exactly() {
    let mut uninterrupted = Engine::in_memory();
    uninterrupted.load_cxf(MINIMAL_LOOP).unwrap();
    uninterrupted.halt().unwrap();
    uninterrupted
        .set_param("http://example.org#MinLoop.con.k", Value::Real(3.0))
        .unwrap();
    uninterrupted.resume().unwrap();
    advance(&mut uninterrupted, 0.0, &INPUTS).unwrap();
    advance(&mut uninterrupted, 1.0, &INPUTS).unwrap();
    let export = uninterrupted.export_cxf().unwrap();
    assert!(export.warnings.is_empty());
    let snapshot = uninterrupted.state_snapshot().unwrap();

    let mut restored = Engine::in_memory();
    restored.load_cxf(&export.bytes).unwrap();
    restored.restore_state(&snapshot).unwrap();
    assert_same_state(&uninterrupted, &restored);
    for t in [2.0, 3.0, 5.0] {
        advance(&mut uninterrupted, t, &INPUTS).unwrap();
        // Export mints a synthetic executable input identity; it is obtained from the schema.
        let definitions = restored.input_definitions().unwrap();
        assert_eq!(definitions.len(), 1);
        advance(
            &mut restored,
            t,
            &[(definitions[0].path.as_str(), Value::Real(0.0))],
        )
        .unwrap();
        assert_same_state(&uninterrupted, &restored);
    }
}
