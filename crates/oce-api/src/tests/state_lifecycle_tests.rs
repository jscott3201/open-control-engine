//! Additional durable fresh-target lifecycle transitions.

use crate::{Engine, EngineStateError, OcError};

const MINIMAL_LOOP: &[u8] = include_bytes!("../../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

fn snapshot() -> crate::EngineStateSnapshot {
    let mut source = Engine::in_memory();
    source.load_cxf(MINIMAL_LOOP).unwrap();
    source.state_snapshot().unwrap()
}

#[test]
fn successful_reload_opens_a_new_restore_window() {
    let snapshot = snapshot();
    let mut target = Engine::in_memory();
    target.load_cxf(MINIMAL_LOOP).unwrap();
    target.tick(0.0).unwrap();
    target.load_cxf(MINIMAL_LOOP).unwrap();
    target.restore_state(&snapshot).unwrap();
}

#[test]
fn successful_durable_restore_consumes_the_restore_window() {
    let snapshot = snapshot();
    let mut target = Engine::in_memory();
    target.load_cxf(MINIMAL_LOOP).unwrap();
    target.restore_state(&snapshot).unwrap();
    assert!(matches!(
        target.restore_state(&snapshot),
        Err(OcError::State(EngineStateError::DurableTargetAdvanced))
    ));
}
