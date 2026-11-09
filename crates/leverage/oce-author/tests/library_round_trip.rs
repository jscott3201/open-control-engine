//! Gate item 1 (authoring note; issue #464): every G36 corpus fixture the flat authoring dialect
//! covers imports into the model, emits, loads warning-free and has the same identity as the
//! committed fixture loaded directly; every other fixture is refused by the importer with a typed
//! error, listed here with its reason.
//!
//! **Namespace.** The corpus names its nodes under `http://example.org#g36.source.` (or
//! `http://example.org#g36.`), which ends in `.`; an emitter namespace must end in `#`, `/` or
//! `:` (the content-id note, GitHub #470/#471), so the emitter cannot spell the corpus IRIs. The
//! comparison therefore re-stamps the committed fixture's root IRI `{namespace}{sequence}` to
//! `urn:oce:library:{sequence}` — a textual substitution of the root prefix only — and the test
//! also proves that the re-stamp is the only difference: the committed fixture's own export,
//! with the same substitution, equals the emitted document's export byte for byte.
//!
//! **Node order (GitHub #458).** Today's id is order-bearing. One flat fixture's `@graph` block
//! order differs from its `containsBlock` order (`multizone_vav_outdoor_airflow_sumzone`), which
//! the emitter's layout cannot reproduce, and four fixtures are nested composites the flat
//! layout has no order for; all five are refused by the importer before order matters (see
//! [`REFUSED`]), and `node_order_alone_moves_the_sumzone_identity` pins the mismatch itself.

mod support;

use oce_author::{EmitOptions, ImportError, emit, import_cxf};

use support::cxf::{corpus_fixture, export_text, identity, load_clean};

#[path = "../../../oce-api/tests/support/blob_corpus.rs"]
mod blob_corpus;

/// The namespace the round trip emits under.
const LIBRARY_NAMESPACE: &str = "urn:oce:library:";

/// The fixtures the importer reads, all of which must round-trip.
const IMPORTED: &[&str] = &[
    "cooling_only_system_requests",
    "generic_time_suppression",
    "multizone_vav_relief_fan_group",
    "reheat_overrides",
];

/// Every other fixture, with the construct outside the authoring subset that refuses it (a
/// substring of the `ImportError` display). The first refusal found is the one listed.
const REFUSED: &[(&str, &str)] = &[
    ("ahu_economizer", "`S231:max`"),
    ("ahu_supply_air_temp_reset", "`S231:max`"),
    ("cooling_only_active_air_flow", "a library enumeration"),
    ("cooling_only_alarms", "a library enumeration"),
    ("cooling_only_controller", "a nested or second composite"),
    ("cooling_only_dampers", "not Real, Integer or Boolean"),
    (
        "generic_air_economizer_high_limits_ashrae_differential",
        "not Real, Integer or Boolean",
    ),
    (
        "generic_air_economizer_high_limits_ashrae_fixed_18",
        "not Real, Integer or Boolean",
    ),
    (
        "generic_air_economizer_high_limits_ashrae_fixed_21",
        "not Real, Integer or Boolean",
    ),
    (
        "generic_air_economizer_high_limits_ashrae_fixed_24",
        "not Real, Integer or Boolean",
    ),
    (
        "generic_air_economizer_high_limits_title24_differential_offset_0",
        "not Real, Integer or Boolean",
    ),
    (
        "generic_air_economizer_high_limits_title24_differential_offset_1",
        "not Real, Integer or Boolean",
    ),
    (
        "generic_air_economizer_high_limits_title24_differential_offset_2",
        "not Real, Integer or Boolean",
    ),
    (
        "generic_air_economizer_high_limits_title24_differential_offset_3",
        "not Real, Integer or Boolean",
    ),
    (
        "generic_air_economizer_high_limits_title24_fixed_21",
        "not Real, Integer or Boolean",
    ),
    (
        "generic_air_economizer_high_limits_title24_fixed_22",
        "not Real, Integer or Boolean",
    ),
    (
        "generic_air_economizer_high_limits_title24_fixed_23",
        "not Real, Integer or Boolean",
    ),
    (
        "generic_air_economizer_high_limits_title24_fixed_24",
        "not Real, Integer or Boolean",
    ),
    (
        "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21",
        "a nested or second composite",
    ),
    ("multizone_vav_economizer_enable", "`S231:max`"),
    (
        "multizone_vav_economizer_limits_common",
        "not Real, Integer or Boolean",
    ),
    ("multizone_vav_economizer_modulations_reliefs", "`S231:max`"),
    (
        "multizone_vav_economizer_modulations_return_fan_relief_damper",
        "`S231:max`",
    ),
    (
        "multizone_vav_economizer_modulations_return_fan",
        "`S231:max`",
    ),
    (
        "multizone_vav_freeze_protection",
        "not Real, Integer or Boolean",
    ),
    (
        "multizone_vav_outdoor_airflow_ahu",
        "not Real, Integer or Boolean",
    ),
    (
        "multizone_vav_outdoor_airflow_sumzone",
        "a library enumeration",
    ),
    (
        "multizone_vav_outdoor_airflow_title24_ahu",
        "not Real, Integer or Boolean",
    ),
    (
        "multizone_vav_outdoor_airflow_title24_sumzone",
        "`S231:conditionalExpression`",
    ),
    (
        "multizone_vav_plant_requests",
        "not Real, Integer or Boolean",
    ),
    ("multizone_vav_relief_damper", "`S231:max`"),
    ("multizone_vav_relief_fan", "`S231:max`"),
    (
        "multizone_vav_return_fan_airflow_tracking",
        "not Real, Integer or Boolean",
    ),
    (
        "multizone_vav_return_fan_direct_pressure",
        "not Real, Integer or Boolean",
    ),
    ("multizone_vav_supply_fan", "a nested or second composite"),
    (
        "multizone_vav_supply_signals",
        "not Real, Integer or Boolean",
    ),
    (
        "multizone_vav_supply_temperature",
        "a nested or second composite",
    ),
    ("thermal_zones_control_loops", "`S231:max`"),
    ("thermal_zones_zone_states", "a library enumeration"),
    (
        "trim_and_respond_have_hol_false",
        "`S231:conditionalExpression`",
    ),
    ("vav_single_zone", "`S231:max`"),
    (
        "ventilation_zones_ashrae62_1_setpoints",
        "a library enumeration",
    ),
];

fn committed(name: &str) -> String {
    let path = corpus_fixture(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The root `@id` of a committed fixture: the node that carries `containsBlock`.
fn root_iri(text: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(text).expect("JSON");
    value["@graph"]
        .as_array()
        .and_then(|g| g.iter().find(|n| n.get("S231:containsBlock").is_some()))
        .and_then(|n| n["@id"].as_str())
        .expect("a root")
        .to_owned()
}

#[test]
fn the_tables_cover_the_corpus_exactly_once() {
    let mut listed: Vec<&str> = IMPORTED
        .iter()
        .copied()
        .chain(REFUSED.iter().map(|(n, _)| *n))
        .collect();
    listed.sort_unstable();
    let mut corpus = blob_corpus::FIXTURES.to_vec();
    corpus.sort_unstable();
    assert_eq!(listed, corpus);
}

#[test]
fn every_covered_fixture_round_trips_to_the_committed_identity() {
    for name in IMPORTED {
        let text = committed(name);
        let sequence = import_cxf(text.as_bytes()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let root = root_iri(&text);
        let restamped_root = format!("{LIBRARY_NAMESPACE}{}", sequence.name);
        let emitted = emit(
            &sequence,
            &EmitOptions::new().with_namespace(LIBRARY_NAMESPACE),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let from_model = load_clean(&emitted);
        let reference = load_clean(text.replace(&root, &restamped_root).as_bytes());
        assert_eq!(identity(&from_model), identity(&reference), "{name}");
        // The re-stamp is the only difference from the committed fixture loaded directly.
        let direct = load_clean(text.as_bytes());
        assert_eq!(
            export_text(&direct).replace(&root, &restamped_root),
            export_text(&from_model),
            "{name}"
        );
    }
}

#[test]
fn every_other_fixture_is_refused_with_its_reason() {
    for (name, reason) in REFUSED {
        match import_cxf(committed(name).as_bytes()) {
            Err(error @ ImportError::Unsupported { .. }) => {
                assert!(error.to_string().contains(reason), "{name}: {error}");
            }
            other => panic!("{name}: expected an unsupported construct, got {other:?}"),
        }
    }
}

#[test]
fn re_emitting_an_imported_document_is_byte_stable() {
    for name in IMPORTED {
        let sequence = import_cxf(committed(name).as_bytes()).expect("import");
        let options = EmitOptions::new();
        let once = emit(&sequence, &options).expect("emit");
        let again = import_cxf(&once).expect("re-import");
        assert_eq!(emit(&again, &options).expect("emit"), once, "{name}");
        // The model reads back whole; only connections regroup, by source node in the
        // emitter's layout (each source's targets keep their order).
        assert_eq!(
            (&again.params, &again.inputs, &again.outputs, &again.blocks),
            (
                &sequence.params,
                &sequence.inputs,
                &sequence.outputs,
                &sequence.blocks
            ),
            "{name}"
        );
        let spelled = |s: &oce_author::Sequence| {
            let mut all: Vec<String> = s.connections.iter().map(|c| c.spelling()).collect();
            all.sort_unstable();
            all
        };
        assert_eq!(spelled(&again), spelled(&sequence), "{name}");
    }
}

/// Reorder a fixture's `@graph` so block nodes follow `containsBlock` (keeping every other node
/// where it is relative to its block), the order the emitter's layout would write.
fn blocks_in_contains_order(text: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(text).expect("JSON");
    let graph = value["@graph"].as_array().expect("graph").clone();
    let root = graph
        .iter()
        .find(|n| n.get("S231:containsBlock").is_some())
        .expect("root")
        .clone();
    let blocks: Vec<String> = root["S231:containsBlock"]
        .as_array()
        .expect("blocks")
        .iter()
        .map(|r| r["@id"].as_str().expect("id").to_owned())
        .collect();
    let owner = |id: &str| -> Option<usize> {
        blocks
            .iter()
            .position(|b| id == b || id.starts_with(&format!("{b}.")))
    };
    let mut head: Vec<serde_json::Value> = Vec::new();
    let mut per_block: Vec<Vec<serde_json::Value>> = vec![Vec::new(); blocks.len()];
    let mut tail: Vec<serde_json::Value> = Vec::new();
    for node in graph {
        let id = node["@id"].as_str().expect("id").to_owned();
        match owner(&id) {
            Some(b) => per_block[b].push(node),
            None if per_block.iter().all(Vec::is_empty) => head.push(node),
            None => tail.push(node),
        }
    }
    let reordered: Vec<serde_json::Value> = head
        .into_iter()
        .chain(per_block.into_iter().flatten())
        .chain(tail)
        .collect();
    value["@graph"] = serde_json::Value::Array(reordered);
    serde_json::to_string(&value).expect("write")
}

#[test]
fn node_order_alone_moves_the_sumzone_identity() {
    // GitHub #458: `multizone_vav_outdoor_airflow_sumzone` lists its blocks in `@graph` as
    // groFlo, groFlo1, groFlo2, groFlo3, booToRea_1, … while `containsBlock` reads groFlo,
    // groFlo1, booToRea_1, booToRea_2, mul_1, … (first divergence at the third block). Moving
    // only the block nodes into `containsBlock` order — the emitter's layout — moves the id.
    let text = committed("multizone_vav_outdoor_airflow_sumzone");
    let reordered = blocks_in_contains_order(&text);
    assert_ne!(reordered, text);
    let direct = identity(&support::cxf::load(text.as_bytes()).expect("load").0);
    let moved = identity(&support::cxf::load(reordered.as_bytes()).expect("load").0);
    assert_ne!(direct, moved);
    // The same reordering leaves a fixture whose order already agrees unchanged.
    let agreeing = committed("reheat_overrides");
    let same = identity(&support::cxf::load(agreeing.as_bytes()).expect("load").0);
    let again = identity(
        &support::cxf::load(blocks_in_contains_order(&agreeing).as_bytes())
            .expect("load")
            .0,
    );
    assert_eq!(same, again);
}
