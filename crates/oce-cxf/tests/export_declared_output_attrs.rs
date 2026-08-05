//! Document-to-document oracle for declared boundary-output §7.4.1 attribute fidelity (#233).
//!
//! Every model-level guard in this workspace compares the engine to itself: the RT-2 fixpoints
//! are symmetric across the round trip, and the shared render never prints `boundary_outputs`.
//! A fix that carried the DRIVER's attributes instead of the DECLARED node's would therefore
//! round-trip perfectly with every one of them green. This file is the asymmetric check: it
//! compares the AUTHORED engine-G36-corpus document to the EXPORTED document key by key, so the
//! attribute values answer to the source bytes, never to the engine.
//!
//! Three pinned layers (the third lives in `export_g36_roundtrip.rs`):
//! 1. Population, counted from what the comparator ACTUALLY COMPARED — the counters increment
//!    only after a node's per-key comparison has executed, so a loop that skips nodes cannot
//!    keep them green (a counter at the top of the loop would count enumeration, not
//!    comparison).
//! 2. Per-node key-by-key comparison, authored vs exported, with exactly one declared
//!    normalization: `min`/`max` are authored as typed literals and exported as bare scalars by
//!    documented design (`docs/cxf-round-trip.md`), so both sides parse to `f64` and compare by
//!    `to_bits` — never a string compare across that pair, never an epsilon.
//! 3. Absence attribution — `assert_authored_boundary_output_contract` in
//!    `export_g36_roundtrip.rs` already attributes every non-surviving declared output to a
//!    deferral owner by set equality; this file deliberately does not duplicate it.
//!
//! `@type` and the `isConditionalComponent`/`conditionalExpression` pair are EXPECTED deltas
//! (owner ruling 2026-08-05): `isOfDataType` stays the type carrier, and the conditional pair
//! is elaborated-away polymorphism metadata on a post-specialization document.
//!
//! The directory-reading and JSON helpers are duplicated from `export_g36_roundtrip.rs` under
//! that file's own licence for helper duplication across test binaries; the directory/list
//! bijection stays that file's job.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use oce_cxf::{CxfError, ResolveOptions, export, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{
    Attrs, BlockId, BlockInstance, BoundaryOutput, Connector, ConnectorId, Dir, ModelGraph,
    ParamTable, RealAttrs, ValueType,
};
use serde_json::Value as JsonValue;

/// The five §7.4.1 attribute keys in scope (owner ruling 2026-08-05). `@type`,
/// `label`/`accessSpecifier`, and the conditional pair are out.
const ATTR_KEYS: [&str; 5] = [
    "S231:unit",
    "S231:quantity",
    "S231:displayUnit",
    "S231:min",
    "S231:max",
];

/// The new top-level fixture: one elided and one pass-through declared Real output, both
/// carrying authored §7.4.1 attributes; pinned separately from the corpus counts.
const DECLARED_OUTPUT_ATTRS: &str = include_str!("fixtures/declared_output_attrs.jsonld");
/// A declared Boolean output carrying `S231:unit` — newly reachable rejection arm 1.
const BOOLEAN_OUTPUT_ATTR: &str = include_str!("fixtures/declared_output_attr_on_boolean.jsonld");
/// A declared Real output whose `S231:min` is an expression — newly reachable rejection arm 2.
const EXPRESSION_BOUND: &str = include_str!("fixtures/declared_output_expression_bound.jsonld");

fn g36_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/g36")
}

/// Sorted `*.jsonld` paths of the engine G36 corpus (46 fixtures; the count is pinned by the
/// comparator's own per-fixture counter, not by this listing).
fn sorted_fixture_paths() -> Vec<PathBuf> {
    let dir = g36_dir();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("G36 fixture dir {} must exist: {e}", dir.display()))
        .map(|entry| entry.expect("readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonld"))
        .collect();
    paths.sort();
    paths
}

fn import_ok(fixture: &str, bytes: &[u8]) -> ModelGraph {
    let (g, report) = import_cxf(bytes, &ResolveOptions::default())
        .unwrap_or_else(|e| panic!("`{fixture}` must resolve without error: {e:?}"));
    assert!(
        report.is_empty(),
        "`{fixture}` expected zero diagnostics, got: {:?}",
        report.diagnostics
    );
    g
}

fn top_composite(doc: &JsonValue) -> &JsonValue {
    doc["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node.get("S231:containsBlock").is_some())
        .expect("top composite")
}

fn reference_ids(value: Option<&JsonValue>) -> BTreeSet<String> {
    let Some(value) = value else {
        return BTreeSet::new();
    };
    let values = value
        .as_array()
        .map_or_else(|| vec![value], |items| items.iter().collect());
    values
        .into_iter()
        .map(|item| item["@id"].as_str().expect("reference @id").to_owned())
        .collect()
}

fn node_by_id<'doc>(doc: &'doc JsonValue, id: &str) -> Option<&'doc JsonValue> {
    doc["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"].as_str() == Some(id))
}

/// A term attribute (`unit`/`quantity`/`displayUnit`) as its bare-string form. The engine
/// corpus authors these as bare strings only; a different shape is corpus drift, not a delta
/// this comparator may normalize away.
fn term_string(fixture: &str, id: &str, key: &str, value: &JsonValue) -> String {
    value
        .as_str()
        .unwrap_or_else(|| {
            panic!("`{fixture}` {id} {key}: expected a bare string, got {value} — corpus drift")
        })
        .to_owned()
}

/// A `min`/`max` value as `f64` bits from either side's shape: the authored typed literal's
/// `@value` lexical form, or the exported bare scalar.
fn bound_bits(fixture: &str, id: &str, key: &str, value: &JsonValue) -> u64 {
    if let Some(object) = value.as_object() {
        let lexical = object
            .get("@value")
            .and_then(JsonValue::as_str)
            .unwrap_or_else(|| panic!("`{fixture}` {id} {key}: typed literal without @value"));
        lexical
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("`{fixture}` {id} {key}: unparseable @value: {e}"))
            .to_bits()
    } else {
        value
            .as_f64()
            .unwrap_or_else(|| {
                panic!("`{fixture}` {id} {key}: expected a bare number, got {value}")
            })
            .to_bits()
    }
}

/// Compare one surviving declared output's authored node against its exported node key by key,
/// appending every disagreement (either direction: a lost authored value or an export-only
/// extra) to `mismatches`. Also asserts the two EXPECTED deltas, so their absence is a checked
/// fact rather than a silent skip.
fn compare_declared_node(
    fixture: &str,
    id: &str,
    authored: &JsonValue,
    exported: &JsonValue,
    mismatches: &mut Vec<String>,
) {
    for key in ["S231:unit", "S231:quantity", "S231:displayUnit"] {
        let a = authored.get(key).map(|v| term_string(fixture, id, key, v));
        let e = exported.get(key).map(|v| term_string(fixture, id, key, v));
        if a != e {
            mismatches.push(format!(
                "`{fixture}` {id} {key}: authored {a:?} exported {e:?}"
            ));
        }
    }
    for key in ["S231:min", "S231:max"] {
        let a = authored.get(key).map(|v| bound_bits(fixture, id, key, v));
        let e = exported.get(key).map(|v| bound_bits(fixture, id, key, v));
        if a != e {
            mismatches.push(format!(
                "`{fixture}` {id} {key}: authored bits {a:0>16x?} exported bits {e:0>16x?}"
            ));
        }
    }
    assert!(
        exported.get("@type").is_none(),
        "`{fixture}` {id}: exported declared output grew an @type — isOfDataType is the type \
         carrier by owner ruling (2026-08-05); @type is an expected delta, not part of the \
         five-attribute scope"
    );
    assert!(
        exported.get("S231:isConditionalComponent").is_none()
            && exported.get("S231:conditionalExpression").is_none(),
        "`{fixture}` {id}: exported declared output carries conditional-polymorphism triples — \
         the exported document is post-specialization, so their absence is an expected delta \
         (owner ruling 2026-08-05)"
    );
}

/// LAYERS 1 + 2: the corpus sweep. Population is pinned from the comparator loop's OWN
/// counters, incremented only AFTER each node's comparison executed — under a node-level skip
/// of attr-carriers, `surviving` reds at 36-vs-97 and `attr_carrying` at 0-vs-61, while the
/// per-fixture counter alone never could.
#[test]
fn corpus_declared_outputs_export_their_authored_attrs_key_by_key() {
    let mut fixtures = 0usize;
    let mut surviving = 0usize;
    let mut attr_carrying = 0usize;
    let mut conditional_pair_compared = 0usize;
    let mut mismatches: Vec<String> = Vec::new();

    for path in sorted_fixture_paths() {
        let fixture = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("UTF-8 fixture name")
            .to_owned();
        let bytes =
            std::fs::read(&path).unwrap_or_else(|e| panic!("`{fixture}` must be readable: {e}"));
        let graph = import_ok(&fixture, &bytes);
        let exported = export(&graph)
            .unwrap_or_else(|e| panic!("`{fixture}` must be inside the export subset: {e:?}"));
        let source_doc: JsonValue = serde_json::from_slice(&bytes).expect("source JSON");
        let export_doc: JsonValue = serde_json::from_slice(&exported).expect("export JSON");
        let exported_outputs = reference_ids(top_composite(&export_doc).get("S231:hasOutput"));
        fixtures += 1;

        for id in reference_ids(top_composite(&source_doc).get("S231:hasOutput")) {
            if !exported_outputs.contains(&id) {
                // Non-surviving: attributed to a resolved deferral owner by the set equality in
                // `export_g36_roundtrip.rs` (layer 3); nothing to compare here.
                continue;
            }
            let authored = node_by_id(&source_doc, &id)
                .unwrap_or_else(|| panic!("`{fixture}` authored node {id} missing"));
            let exported_node = node_by_id(&export_doc, &id)
                .unwrap_or_else(|| panic!("`{fixture}` exported node {id} missing"));
            compare_declared_node(&fixture, &id, authored, exported_node, &mut mismatches);
            // Counter placement is the teeth: incremented AFTER the comparison, so these count
            // what was COMPARED. A `continue` above this line un-counts the node it skips.
            surviving += 1;
            if ATTR_KEYS.iter().any(|key| authored.get(*key).is_some()) {
                attr_carrying += 1;
            }
            if authored.get("S231:isConditionalComponent").is_some()
                || authored.get("S231:conditionalExpression").is_some()
            {
                conditional_pair_compared += 1;
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "declared boundary-output §7.4.1 attribute mismatches over the engine G36 corpus \
         ({} total):\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    assert_eq!(fixtures, 46, "engine G36 corpus size moved");
    // One assertion carrying BOTH counters: written as two assert_eq! calls, the first panic
    // would hide the second, and mutation control 7 needs either red observable.
    assert_eq!(
        (surviving, attr_carrying),
        (97, 61),
        "compared-population pins moved (surviving, attr-carrying), counted from what the \
         comparator actually compared over the engine G36 corpus"
    );
    assert!(
        conditional_pair_compared > 0,
        "the conditional-pair expected-delta assertion went vacuous: no compared declared \
         output authored the pair"
    );
}

// ---- the new fixture: both drop sites witnessed ------------------------------------------------

/// Site A (elided) and site B (pass-through) at the MODEL level: import parses the declared
/// node's own attributes, not the driver's and not the type's empty defaults.
#[test]
fn import_parses_declared_output_attrs_on_both_drop_sites() {
    let graph = import_ok(
        "declared_output_attrs.jsonld",
        DECLARED_OUTPUT_ATTRS.as_bytes(),
    );

    let elided = graph
        .boundary_outputs
        .iter()
        .find(|output| output.iri.ends_with("yElided"))
        .expect("elided declared output is materialized");
    match &elided.attrs {
        Attrs::Real(a) => {
            assert_eq!(
                (
                    a.unit.as_deref(),
                    a.quantity.as_deref(),
                    a.display_unit.as_deref(),
                    a.min.map(f64::to_bits),
                    a.max.map(f64::to_bits),
                ),
                (
                    Some("K"),
                    Some("ThermodynamicTemperature"),
                    Some("degC"),
                    Some(200.0f64.to_bits()),
                    Some(330.15f64.to_bits()),
                ),
                "elided declared output carries its authored five attributes"
            );
        }
        other => panic!("elided declared Real output parsed non-Real attrs: {other:?}"),
    }

    let pass_through = graph
        .connectors
        .iter()
        .find(|c| c.iri.as_deref().is_some_and(|iri| iri.ends_with("yPass")))
        .expect("pass-through declared output is a real connector");
    match &pass_through.attrs {
        Attrs::Real(a) => {
            assert_eq!(
                (
                    a.unit.as_deref(),
                    a.quantity.as_deref(),
                    a.display_unit.as_deref(),
                    a.min.map(f64::to_bits),
                    a.max.map(f64::to_bits),
                ),
                (
                    Some("Pa"),
                    Some("PressureDifference"),
                    None,
                    Some((-50.0f64).to_bits()),
                    Some(50.0f64.to_bits()),
                ),
                "pass-through declared output carries its authored attributes, not \
                 `Attrs::default_for` (site B)"
            );
        }
        other => panic!("pass-through declared Real output parsed non-Real attrs: {other:?}"),
    }
}

/// The same two sites at the BYTE level: the exported declared nodes carry the declared node's
/// authored attributes — and the driving connector keeps its own, different, attribute set, so
/// a driver-attrs mix-up cannot pass.
#[test]
fn export_emits_declared_output_attrs_on_both_fill_sites() {
    let graph = import_ok(
        "declared_output_attrs.jsonld",
        DECLARED_OUTPUT_ATTRS.as_bytes(),
    );
    let bytes = export(&graph).expect("fixture is inside the export subset");
    let doc: JsonValue = serde_json::from_slice(&bytes).expect("export JSON");

    let elided = node_by_id(&doc, "http://example.org#DeclaredOutputAttrs.yElided")
        .expect("exported elided declared output node");
    assert_eq!(
        (
            elided["S231:unit"].as_str(),
            elided["S231:quantity"].as_str(),
            elided["S231:displayUnit"].as_str(),
            elided["S231:min"].as_f64().map(f64::to_bits),
            elided["S231:max"].as_f64().map(f64::to_bits),
        ),
        (
            Some("K"),
            Some("ThermodynamicTemperature"),
            Some("degC"),
            Some(200.0f64.to_bits()),
            Some(330.15f64.to_bits()),
        ),
        "elided fill site emits the declared node's five authored attributes"
    );

    let pass_through = node_by_id(&doc, "http://example.org#DeclaredOutputAttrs.yPass")
        .expect("exported pass-through declared output node");
    assert_eq!(
        (
            pass_through["S231:unit"].as_str(),
            pass_through["S231:quantity"].as_str(),
            pass_through.get("S231:displayUnit"),
            pass_through["S231:min"].as_f64().map(f64::to_bits),
            pass_through["S231:max"].as_f64().map(f64::to_bits),
        ),
        (
            Some("Pa"),
            Some("PressureDifference"),
            None,
            Some((-50.0f64).to_bits()),
            Some(50.0f64.to_bits()),
        ),
        "pass-through fill site emits the declared node's authored attributes (site B)"
    );

    let driver = node_by_id(&doc, "http://example.org#DeclaredOutputAttrs.con.y")
        .expect("exported driving connector node");
    assert_eq!(
        (
            driver["S231:unit"].as_str(),
            driver.get("S231:quantity"),
            driver.get("S231:min"),
        ),
        (Some("K"), None, None),
        "the driver keeps its own attribute set — declared attrs are the declared node's"
    );
}

// ---- the two newly reachable rejection arms ----------------------------------------------------

/// Newly reachable arm 1 (`resolve/attrs.rs` non-Real/Integer arm): a §7.4.1 attribute on a
/// declared Boolean output was silently ACCEPTED before #233 and is REFUSED now, with the same
/// diagnostic an instance-port Boolean connector already gets. 0 engine-G36-corpus occurrences.
#[test]
fn declared_attr_on_a_boolean_output_is_refused() {
    match import_cxf(BOOLEAN_OUTPUT_ATTR.as_bytes(), &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => assert_eq!(
            diags,
            vec![
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    "§7.4.1 attribute (unit/quantity/displayUnit/min/max) declared on a Boolean \
                     connector, which permits none",
                )
                .with_subject("http://example.org#BooleanOutputAttr.yBool".to_owned()),
            ],
            "exactly the Boolean-attr refusal, subject = the declared node"
        ),
        other => panic!("expected a validation rejection, got {other:?}"),
    }
}

/// Newly reachable arm 2 (`resolve/attrs.rs` bound-grounding arm): an expression bound on a
/// declared Real output imported with zero diagnostics before #233 and is REFUSED now — the
/// same refusal an instance-port connector's expression bound already gets (bounds ground
/// against the empty scope), so this is consistency, not new policy. 0 corpus occurrences (all
/// 36 corpus bounds are typed literals).
#[test]
fn expression_bound_on_a_declared_real_output_is_refused() {
    match import_cxf(EXPRESSION_BOUND.as_bytes(), &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => assert_eq!(
            diags,
            vec![
                Diagnostic::error(
                    DiagCode::GroundingFailed,
                    "S231:min connector bound failed to ground: expression binding did not \
                     ground: unknown identifier: k",
                )
                .with_subject("http://example.org#ExpressionBoundOutput.yExpr".to_owned()),
            ],
            "exactly the expression-bound refusal, subject = the declared node"
        ),
        other => panic!("expected a validation rejection, got {other:?}"),
    }
}

// ---- the host-constructed tag mismatch: refused at export, never emitted or dropped ------------

/// `BoundaryOutput.attrs` is a plain field, so a host can construct Real-tagged attrs on a
/// Boolean output through the public API. Export must REFUSE (error severity, no document) —
/// emitting would produce bytes the fixed import rejects, and dropping with a warning would
/// silently discard a host's attributes.
#[test]
fn host_constructed_attr_tag_mismatch_is_refused_at_export() {
    let block_id = BlockId(0);
    let output_id = ConnectorId(0);
    let mut output = Connector::new(output_id, block_id, Dir::Out, ValueType::Boolean, 0);
    output.iri = Some(Arc::from("http://example.org#Host.con.y"));
    let graph = ModelGraph {
        blocks: vec![BlockInstance {
            id: block_id,
            class_iri: Arc::from("CDL.Logical.Sources.Constant"),
            inputs: vec![],
            outputs: vec![output_id],
            params: ParamTable::default(),
            decl_order: 0,
            instance_iri: Some(Arc::from("http://example.org#Host.con")),
        }],
        connectors: vec![output],
        connections: vec![],
        external_inputs: vec![],
        boundary_outputs: vec![BoundaryOutput {
            iri: Arc::from("http://example.org#Host.y"),
            source: output_id,
            attrs: Attrs::Real(RealAttrs {
                unit: Some(Arc::from("K")),
                ..RealAttrs::default()
            }),
        }],
    };
    match export(&graph) {
        Err(CxfError::Validation(diags)) => {
            assert!(
                diags.iter().any(|d| {
                    d.code == DiagCode::ExportUnsupported
                        && d.message
                            == "export subset: block/connector wiring is structurally inconsistent"
                        && d.subject.as_deref() == Some("http://example.org#Host.y")
                }),
                "the structure refusal names the declared node a host must fix: {diags:?}"
            );
        }
        other => panic!("a tag-mismatched declared output must refuse export, got {other:?}"),
    }
}
