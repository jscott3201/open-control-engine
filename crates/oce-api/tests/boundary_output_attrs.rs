//! End-to-end projection of declared boundary-output attributes onto their driver.

use oce_api::oce_store::ModelStore;
use oce_api::{ContentIdError, Engine, IoSummary, PointDirection};
use serde_json::{Map, Value};

const FIXTURE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_limits_common.jsonld");
const DECLARED: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.yRetDamPhy_max";
const DRIVER: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.retDamPhyPosMaxSig.y";
const ATTR_KEYS: [&str; 5] = [
    "S231:unit",
    "S231:quantity",
    "S231:displayUnit",
    "S231:min",
    "S231:max",
];

fn node<'a>(document: &'a Value, id: &str) -> &'a Map<String, Value> {
    document["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"].as_str() == Some(id))
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("node {id} exists"))
}

fn assert_exported_attrs(node: &Map<String, Value>) {
    let present = ATTR_KEYS
        .into_iter()
        .filter(|key| node.contains_key(*key))
        .collect::<Vec<_>>();
    assert_eq!(present, vec!["S231:unit", "S231:min", "S231:max"]);
    assert_eq!(node["S231:unit"].as_str(), Some("1"));
    assert_eq!(
        node["S231:min"].as_f64().map(f64::to_bits),
        Some(0.0_f64.to_bits())
    );
    assert_eq!(
        node["S231:max"].as_f64().map(f64::to_bits),
        Some(1.0_f64.to_bits())
    );
}

#[test]
fn declared_attrs_reach_driver_export_and_host_projections() {
    let authored: Value = serde_json::from_str(FIXTURE).expect("fixture JSON");
    assert!(
        ATTR_KEYS
            .iter()
            .all(|key| !node(&authored, DRIVER).contains_key(*key))
    );
    let declared = node(&authored, DECLARED);
    assert_eq!(declared["S231:unit"].as_str(), Some("1"));
    assert_eq!(declared["S231:min"]["@value"].as_str(), Some("0"));
    assert_eq!(declared["S231:max"]["@value"].as_str(), Some("1"));

    let mut engine = Engine::in_memory();
    let report = engine.load_cxf(FIXTURE.as_bytes()).expect("fixture loads");
    assert_eq!(
        report.io,
        IoSummary {
            analog_inputs: 18,
            analog_outputs: 13,
            digital_inputs: 5,
            digital_outputs: 3,
            network: 0,
            total: 39,
        }
    );

    let driver = engine
        .io()
        .iter()
        .find(|point| point.path == DRIVER)
        .expect("driver inventory row");
    assert_eq!(driver.direction, PointDirection::Out);
    assert_eq!(driver.unit.as_deref(), Some("1"));
    assert_eq!(driver.quantity, None);
    assert_eq!(driver.display_unit, None);
    assert_eq!(driver.min.map(f64::to_bits), Some(0.0_f64.to_bits()));
    assert_eq!(driver.max.map(f64::to_bits), Some(1.0_f64.to_bits()));
    let point_list_driver = engine
        .point_list(None)
        .expect("point list")
        .into_iter()
        .find(|point| point.path == DRIVER)
        .expect("driver point-list row");
    assert_eq!(point_list_driver, driver);

    let durable = engine
        .store()
        .load_model(&report.model_id)
        .expect("durable model")
        .points
        .into_iter()
        .find(|point| point.key.as_str() == DRIVER)
        .expect("durable driver row");
    assert_eq!(durable.unit.as_deref(), Some("1"));
    assert_eq!(durable.quantity, None);
    assert_eq!(durable.display_unit, None);

    let export = engine.export_cxf().expect("fixture exports");
    assert!(matches!(
        export.content_id_complete(),
        Err(ContentIdError::Incomplete {
            warning_count: 3,
            ..
        })
    ));
    let exported: Value = serde_json::from_slice(&export.bytes).expect("exported JSON");
    assert_exported_attrs(node(&exported, DRIVER));
    assert_exported_attrs(node(&exported, DECLARED));
}
