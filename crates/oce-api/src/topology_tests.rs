//! Unit tests for the cases of [`Engine::topology`] no document reaches: a declared output
//! whose driving connector is out of range, which the load gate refuses.

use std::sync::Arc;

use oce_model::ConnectorId;

use crate::Engine;

fn loaded(bytes: &[u8]) -> Engine {
    let mut engine = Engine::new();
    engine.load_cxf(bytes).unwrap();
    engine
}

#[test]
fn a_declared_output_whose_driver_is_out_of_range_is_absent_and_nothing_panics() {
    let mut engine = loaded(include_bytes!(
        "../tests/fixtures/content_id/pass_through.jsonld"
    ));
    let before = engine.topology();
    assert_eq!(before.boundary_outputs.len(), 2);
    let model = Arc::make_mut(&mut engine.model);
    let past_the_end = ConnectorId(model.connectors.len() as u32);
    let declared = model.boundary_outputs[0].iri.to_string();
    model.boundary_outputs[0].source = past_the_end;

    let after = engine.topology();
    // The declaration has no connector to take a type from, so it reads as undriven; the
    // pass-through output and everything else are as they were.
    assert!(after.boundary_outputs.iter().all(|o| o.path != declared));
    assert_eq!(after.boundary_outputs.len(), 1);
    assert_eq!(after.boundary_outputs[0], before.boundary_outputs[1]);
    assert_eq!(after.boundary_inputs, before.boundary_inputs);
    assert_eq!(after.blocks, before.blocks);
}

#[test]
fn a_pass_through_whose_output_connector_is_out_of_range_keeps_its_pair_and_has_no_output() {
    let mut engine = loaded(include_bytes!(
        "../tests/fixtures/content_id/pass_through.jsonld"
    ));
    let before = engine.topology();
    let model = Arc::make_mut(&mut engine.model);
    let past_the_end = ConnectorId(model.connectors.len() as u32);
    let lowered = model
        .blocks
        .iter_mut()
        .find(|block| block.class_iri.starts_with("urn:oce:lowering#"))
        .unwrap();
    lowered.outputs[0] = past_the_end;

    let after = engine.topology();
    assert_eq!(after.pass_through.len(), 1);
    assert_eq!(after.pass_through[0].input, before.pass_through[0].input);
    assert_eq!(
        after.pass_through[0].output,
        format!("conn#{}", past_the_end.0)
    );
    assert_eq!(after.boundary_outputs.len(), 1);
    assert_eq!(after.boundary_outputs[0], before.boundary_outputs[0]);
}
