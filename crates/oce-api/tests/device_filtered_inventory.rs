//! Device-filtered point lists are outside the supported profile: every filter refuses with the
//! existing `OcError::Load`, and neither the refusal nor the unfiltered inventory changes the run.
//! The engine has no equipment index or storage port to delegate a filter to.

use oce_api::{Engine, OcError, Value};

#[test]
fn device_filtered_inventory_refuses_without_engine_mutation() {
    let mut engine = Engine::new();
    engine
        .load_cxf(include_bytes!("fixtures/assertion_model.jsonld"))
        .unwrap();
    let prepared = engine
        .prepare_frame(2.0, &[("urn:assert#u", Value::Boolean(true))])
        .unwrap();
    engine.execute_frame(prepared).unwrap();
    let before = engine.state_snapshot().unwrap();
    let output = engine.get_output("urn:assert#invert.y").unwrap();
    for _ in 0..3 {
        for device in ["AHU-1", "", "unknown", "\0\n设备"] {
            assert!(
                matches!(engine.point_list(Some(device)), Err(OcError::Load { .. })),
                "{device:?}"
            );
        }
        let own = engine.point_list(None).unwrap();
        // One declared boundary input fans out to both sinks; it remains one host point.
        assert_eq!(
            own.iter().map(|p| p.path.as_str()).collect::<Vec<_>>(),
            ["urn:assert#u", "urn:assert#invert.y"]
        );
        assert_eq!(
            engine.state_snapshot().unwrap().as_bytes(),
            before.as_bytes()
        );
        assert!(
            engine
                .get_output("urn:assert#invert.y")
                .unwrap()
                .bit_eq(&output)
        );
    }
    assert!(
        matches!(engine.prepare_frame(1.0, &[]), Err(OcError::TimeRegression { now, prev })
        if now.to_bits() == 1.0_f64.to_bits() && prev.to_bits() == 2.0_f64.to_bits())
    );
}
