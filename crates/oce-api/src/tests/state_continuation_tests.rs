//! Full-state split-run continuation oracle.

use std::cell::RefCell;

use oce_blocks::Diagnostics;
use oce_model::Value;

use crate::{Engine, EngineStateSnapshot};

const MINIMAL_LOOP: &[u8] = include_bytes!("../../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

#[derive(Default)]
struct CapturedDiagnostics(RefCell<Vec<(String, String, u64)>>);

impl Diagnostics for CapturedDiagnostics {
    fn warn(&self, source: &str, message: &str, t: f64) {
        self.0
            .borrow_mut()
            .push((source.to_owned(), message.to_owned(), t.to_bits()));
    }
}

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
    let left_outputs = left.outputs().to_map();
    let right_outputs = right.outputs().to_map();
    assert_eq!(left_outputs.len(), right_outputs.len());
    assert!(left_outputs.iter().zip(&right_outputs).all(
        |((left_key, left), (right_key, right))| { left_key == right_key && left.bit_eq(right) }
    ));
}

#[test]
fn durable_split_run_matches_every_continuation_tick_bit_exactly() {
    let mut uninterrupted = Engine::in_memory();
    uninterrupted.load_cxf(MINIMAL_LOOP).unwrap();
    uninterrupted.tick(0.0).unwrap();
    uninterrupted.tick(1.0).unwrap();
    let snapshot = uninterrupted.state_snapshot().unwrap();
    let decoded = EngineStateSnapshot::from_bytes(snapshot.as_bytes()).unwrap();

    let mut restored = Engine::in_memory();
    restored.load_cxf(MINIMAL_LOOP).unwrap();
    restored.restore_state(&decoded).unwrap();
    assert_same_state(&uninterrupted, &restored);

    for t in [2.0, 2.5, 4.0, 8.0] {
        let left_diagnostics = CapturedDiagnostics::default();
        let right_diagnostics = CapturedDiagnostics::default();
        uninterrupted.tick_with(t, &left_diagnostics).unwrap();
        restored.tick_with(t, &right_diagnostics).unwrap();
        assert_same_state(&uninterrupted, &restored);
        assert_eq!(*left_diagnostics.0.borrow(), *right_diagnostics.0.borrow());
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
    uninterrupted.tick(0.0).unwrap();
    uninterrupted.tick(1.0).unwrap();
    let export = uninterrupted.export_cxf().unwrap();
    assert!(export.warnings.is_empty());
    let snapshot = uninterrupted.state_snapshot().unwrap();

    let mut restored = Engine::in_memory();
    restored.load_cxf(&export.bytes).unwrap();
    restored.restore_state(&snapshot).unwrap();
    assert_same_state(&uninterrupted, &restored);
    for t in [2.0, 3.0, 5.0] {
        uninterrupted.tick(t).unwrap();
        restored.tick(t).unwrap();
        assert_same_state(&uninterrupted, &restored);
    }
}
