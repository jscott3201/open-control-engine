//! Owned topology snapshot contracts.

use oce_api::Engine;

const RETURN_FAN: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_airflow_tracking.jsonld"
);

#[test]
fn independent_loads_produce_equal_reconstructable_topologies() {
    let mut first = Engine::in_memory();
    first
        .load_cxf(RETURN_FAN.as_bytes())
        .expect("fixture loads");
    let mut second = Engine::in_memory();
    second
        .load_cxf(RETURN_FAN.as_bytes())
        .expect("fixture loads independently");
    let left = first.topology();
    let right = second.topology();
    assert_eq!(left, right);
    assert!(
        left.blocks
            .iter()
            .all(|block| !block.class_iri.starts_with("urn:oce:lowering#"))
    );
    assert!(!left.pass_through.is_empty());
    assert!(
        left.pass_through
            .iter()
            .any(|pair| { pair.input.ends_with("u1SupFan") && pair.output.ends_with("y1RetFan") })
    );
    let input = left
        .pass_through
        .iter()
        .find(|pair| pair.input.ends_with("u1SupFan"))
        .unwrap();
    assert!(left.external_inputs.contains(&input.input));
    assert!(
        left.blocks
            .iter()
            .any(|block| block.inputs.contains(&input.input))
    );
    for edge in &left.connections {
        assert!(
            left.blocks
                .iter()
                .any(|block| block.outputs.contains(&edge.from))
        );
        assert!(
            left.blocks
                .iter()
                .any(|block| block.inputs.contains(&edge.to))
        );
    }
}

#[test]
fn unloaded_engine_has_an_empty_non_panicking_topology() {
    let topology = Engine::in_memory().topology();
    assert!(topology.blocks.is_empty());
    assert!(topology.connections.is_empty());
    assert!(topology.external_inputs.is_empty());
    assert!(topology.pass_through.is_empty());
}
