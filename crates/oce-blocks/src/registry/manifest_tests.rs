//! Block-registry manifest guard tests: byte golden, coverage oracle, and variadic-width pins.

use std::collections::BTreeSet;
use std::sync::Arc;

use oce_model::{ParamTable, Value};
use serde_json::Value as JsonValue;

use super::manifest::{MANIFEST_JSON, MANIFEST_PATH, render_manifest};
use super::{CATALOG, lookup};
use crate::{ParamRule, PortKind};

/// Entry count pinned so registry growth is a conscious re-bless, not a silent drift.
const PINNED_ENTRY_COUNT: usize = 133;

#[test]
fn checked_in_manifest_matches_regenerated_bytes() {
    let generated = render_manifest();
    assert_eq!(
        generated,
        render_manifest(),
        "repeated generation must be byte-identical"
    );

    if std::env::var("UPDATE_EXPECT").is_ok_and(|value| !value.trim().is_empty()) {
        std::fs::write(MANIFEST_PATH, &generated).expect("write blessed manifest artifact");
        return;
    }

    if generated != MANIFEST_JSON {
        let first_diff = generated
            .lines()
            .zip(MANIFEST_JSON.lines())
            .position(|(fresh, checked_in)| fresh != checked_in);
        panic!(
            "tools/reference-catalog/oce-blocks.registry-manifest.json is stale \
             (generated {} lines, checked in {} lines, first differing line index: {first_diff:?}).\n\
             Re-bless deliberately with `UPDATE_EXPECT=1 cargo nextest run -p oce-blocks \
             checked_in_manifest_matches_regenerated_bytes` and review the diff.",
            generated.lines().count(),
            MANIFEST_JSON.lines().count(),
        );
    }
}

#[test]
fn manifest_lists_every_catalog_class_path_in_slice_order() {
    let listed = manifest_entries()
        .iter()
        .map(|entry| entry["class_path"].as_str().expect("class_path").to_owned())
        .collect::<Vec<_>>();
    let live = CATALOG
        .iter()
        .flat_map(|entries| entries.iter())
        .map(|entry| entry.class_path.to_owned())
        .collect::<Vec<_>>();

    assert_eq!(
        listed, live,
        "manifest class paths must equal the live CATALOG, alias entries included, in slice order"
    );
    let unique = listed.iter().collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), listed.len(), "class paths must be unique");
    assert_eq!(
        live.len(),
        PINNED_ENTRY_COUNT,
        "registry growth must re-bless the manifest and this pin together"
    );
}

#[test]
fn every_registry_constructor_builds_at_default_parameters() {
    let mut constructed = 0usize;
    for entry in CATALOG.iter().flat_map(|entries| entries.iter()) {
        let block = (entry.make)(&ParamTable::default());
        let signature = block.resolved_signature();
        assert!(
            !signature.class_path.is_empty(),
            "{}: resolved signature must carry a class path",
            entry.class_path
        );
        constructed += 1;
    }
    assert_eq!(constructed, PINNED_ENTRY_COUNT);
}

#[test]
fn variadic_multi_logical_entries_record_empty_default_inputs_with_width_flag() {
    for class_path in ["CDL.Logical.MultiAnd", "CDL.Logical.MultiOr"] {
        let entry = lookup(class_path).expect("registered class");
        let block = (entry.make)(&ParamTable::default());
        let signature = block.resolved_signature();
        assert!(
            signature.inputs.is_empty(),
            "{class_path}: `nin` defaults to 0, so default resolved inputs must be EMPTY"
        );
        assert_eq!(signature.outputs.as_ref(), &[PortKind::Boolean]);
        assert!(
            entry
                .param_rules()
                .iter()
                .any(|rule| matches!(rule, ParamRule::Structural { name: "nin" })),
            "{class_path}: variadic width must be flagged by Structural {{ nin }}"
        );

        let manifest = manifest_entry(class_path);
        assert_eq!(manifest["inputs"], JsonValue::Array(Vec::new()));
        assert_eq!(manifest["width_driven"], JsonValue::Bool(true));
    }
}

#[test]
fn vector_filter_entries_record_empty_default_ports_on_both_sides() {
    for class_path in [
        "CDL.Routing.BooleanVectorFilter",
        "CDL.Routing.IntegerVectorFilter",
        "CDL.Routing.RealVectorFilter",
    ] {
        let entry = lookup(class_path).expect("registered class");
        let block = (entry.make)(&ParamTable::default());
        let signature = block.resolved_signature();
        assert!(
            signature.inputs.is_empty() && signature.outputs.is_empty(),
            "{class_path}: `nin`/`nout` default to 0, so BOTH default port lists must be EMPTY"
        );
        let rules = entry.param_rules();
        assert!(
            rules
                .iter()
                .any(|rule| matches!(rule, ParamRule::Structural { name: "nin" }))
                && rules
                    .iter()
                    .any(|rule| matches!(rule, ParamRule::Structural { name: "nout" })),
            "{class_path}: variadic widths must be flagged by Structural {{ nin }} and {{ nout }}"
        );

        let manifest = manifest_entry(class_path);
        assert_eq!(manifest["inputs"], JsonValue::Array(Vec::new()));
        assert_eq!(manifest["outputs"], JsonValue::Array(Vec::new()));
        assert_eq!(manifest["width_driven"], JsonValue::Bool(true));
    }
}

#[test]
fn default_width_routing_entries_record_nonempty_default_ports() {
    for class_path in [
        "CDL.Routing.BooleanExtractSignal",
        "CDL.Routing.IntegerExtractSignal",
        "CDL.Routing.RealExtractSignal",
        "CDL.Routing.BooleanExtractor",
        "CDL.Routing.IntegerExtractor",
        "CDL.Routing.RealExtractor",
        "CDL.Routing.BooleanScalarReplicator",
        "CDL.Routing.IntegerScalarReplicator",
        "CDL.Routing.RealScalarReplicator",
        "CDL.Routing.BooleanVectorReplicator",
        "CDL.Routing.IntegerVectorReplicator",
        "CDL.Routing.RealVectorReplicator",
    ] {
        let entry = lookup(class_path).expect("registered class");
        let block = (entry.make)(&ParamTable::default());
        let signature = block.resolved_signature();
        assert!(
            !signature.inputs.is_empty() && !signature.outputs.is_empty(),
            "{class_path}: width parameters default to 1, so default ports must be NON-empty"
        );

        let manifest = manifest_entry(class_path);
        assert!(!manifest["inputs"].as_array().expect("inputs").is_empty());
        assert!(!manifest["outputs"].as_array().expect("outputs").is_empty());
        assert_eq!(manifest["width_driven"], JsonValue::Bool(true));
    }
}

#[test]
fn resolved_signature_widens_variadic_inputs_that_static_signature_omits() {
    let entry = lookup("CDL.Logical.MultiAnd").expect("registered class");
    let params = ParamTable {
        values: vec![(Arc::from("nin"), Value::Integer(3))],
    };
    let block = (entry.make)(&params);
    assert_eq!(
        block.resolved_signature().inputs.as_ref(),
        &[PortKind::Boolean; 3],
        "resolved_signature() must widen `u[nin]` to nin scalar Boolean ports"
    );
    assert!(
        block.signature().inputs.is_empty(),
        "static signature() stays at the empty default width — which is WHY the manifest \
         records resolved_signature(), never the static class descriptor"
    );
}

#[test]
fn float_guard_fields_serialize_as_shortest_round_trip_decimals() {
    for pinned_rule_line in [
        r#"{ "rule": "RealGreaterOrEqual", "name": "deltaU", "min": 0.001 }"#,
        r#"{ "rule": "RealLessOrEqualConstant", "name": "deltaU", "max": 0.5 }"#,
        r#"{ "rule": "RealGreaterOrEqualScaledWarning", "left": "period", "right": "minTruFalHol", "factor": 2.0 }"#,
        r#"{ "rule": "RealLessOrEqualConstant", "name": "fallingSlewRate", "max": -1e-37 }"#,
        r#"{ "rule": "RealGreaterOrEqual", "name": "Td", "min": 1e-15 }"#,
    ] {
        assert!(
            MANIFEST_JSON.contains(pinned_rule_line),
            "checked-in manifest must pin the exact serde_json f64 decimal: {pinned_rule_line}"
        );
    }
}

/// Parse the checked-in manifest's top-level entry array.
fn manifest_entries() -> Vec<JsonValue> {
    let manifest: JsonValue = serde_json::from_str(MANIFEST_JSON).expect("manifest JSON parses");
    manifest.as_array().expect("top-level JSON array").clone()
}

/// Find one entry object by class path in the checked-in manifest.
fn manifest_entry(class_path: &str) -> JsonValue {
    manifest_entries()
        .into_iter()
        .find(|entry| entry["class_path"] == JsonValue::String(class_path.to_owned()))
        .unwrap_or_else(|| panic!("manifest entry missing for {class_path}"))
}
