//! Exact diagnostic goldens for the composite-subset contract rules.
//!
//! Every composite-subset CONTRACT rejection must name the offending composite node id as its
//! subject (where one exists) and carry the stable `composite/<rule-id>: ` message tag published
//! in `tools/reference-catalog/oce-cxf.composite-rules.json`, so an external CXF generator can
//! map a rejection back to its source graph and the violated rule. These tests pin the exact
//! (code, subject, message) triple per rule, the enumeration determinism of the multi-offender
//! messages, and the emitted-prefix tie to the checked-in catalog. The catalog byte golden itself
//! lives in-crate (`resolve::composite_rules_tests`).

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value as JsonValue, json};

/// The checked-in published rule catalog — the prefix-tie tests read THIS artifact (not the
/// in-crate table), so emission drifting from the published contract fails here even if the
/// table and emission moved together without a re-bless.
const CATALOG_JSON: &str =
    include_str!("../../../tools/reference-catalog/oce-cxf.composite-rules.json");

/// A minimal valid single-composite model (mirrors the `resolve_errors.rs` BASE): `c1 =
/// Constant(k=1.0)` driving `c2 = MultiplyByParameter(k=2.0)`. Resolves with zero diagnostics.
const BASE: &str = r#"{
  "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
  "@graph": [
    { "@id": "http://example.org#M", "@type": "S231:Block",
      "S231:containsBlock": [ { "@id": "http://example.org#M.c1" }, { "@id": "http://example.org#M.c2" } ] },

    { "@id": "http://example.org#M.c1",
      "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
      "S231:hasParameter": { "@id": "http://example.org#M.c1.k" },
      "S231:hasOutput": { "@id": "http://example.org#M.c1.y" } },
    { "@id": "http://example.org#M.c1.k",
      "S231:value": { "@value": "1.0", "@type": "http://www.w3.org/2001/XMLSchema#double" } },
    { "@id": "http://example.org#M.c1.y", "@type": "S231:RealOutput",
      "S231:isOfDataType": { "@id": "S231:Real" },
      "S231:isConnectedTo": { "@id": "http://example.org#M.c2.u" } },

    { "@id": "http://example.org#M.c2",
      "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
      "S231:hasParameter": { "@id": "http://example.org#M.c2.k" },
      "S231:hasInput": { "@id": "http://example.org#M.c2.u" },
      "S231:hasOutput": { "@id": "http://example.org#M.c2.y2" } },
    { "@id": "http://example.org#M.c2.k",
      "S231:value": { "@value": "2.0", "@type": "http://www.w3.org/2001/XMLSchema#double" } },
    { "@id": "http://example.org#M.c2.u", "@type": "S231:RealInput",
      "S231:isOfDataType": { "@id": "S231:Real" } },
    { "@id": "http://example.org#M.c2.y2", "@type": "S231:RealOutput",
      "S231:isOfDataType": { "@id": "S231:Real" } }
  ]
}"#;

fn base() -> JsonValue {
    serde_json::from_str(BASE).expect("BASE is valid JSON")
}

fn node_mut<'a>(doc: &'a mut JsonValue, suffix: &str) -> &'a mut JsonValue {
    doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .iter_mut()
        .find(|n| n["@id"].as_str().is_some_and(|s| s.ends_with(suffix)))
        .unwrap_or_else(|| panic!("no @graph node ending in {suffix:?}"))
}

/// Import `doc` and return the full, `finalize_diags`-sorted rejection vector.
fn reject(doc: &JsonValue) -> Vec<Diagnostic> {
    let bytes = serde_json::to_vec(doc).expect("serialize fixture");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => diags,
        other => panic!("expected a validation rejection, got {other:?}"),
    }
}

fn error_with_subject(code: DiagCode, subject: &str, message: &str) -> Diagnostic {
    Diagnostic::error(code, message).with_subject(subject.to_owned())
}

/// BASE plus a second unreferenced composite root `M2` → two root candidates.
fn multi_root_doc() -> JsonValue {
    let mut doc = base();
    doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .push(json!({
            "@id": "http://example.org#M2", "@type": "S231:Block",
            "S231:containsBlock": [ { "@id": "http://example.org#M.c1" } ]
        }));
    doc
}

/// A PURE composite `containsBlock` cycle A→B→C→A with no root at all: every composite is
/// referenced, so root classification finds ZERO candidates and the cycle detector never runs.
fn pure_cycle_doc() -> JsonValue {
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [
            { "@id": "http://example.org#A", "@type": "S231:Block",
              "S231:containsBlock": { "@id": "http://example.org#B" } },
            { "@id": "http://example.org#B", "@type": "S231:Block",
              "S231:containsBlock": { "@id": "http://example.org#C" } },
            { "@id": "http://example.org#C", "@type": "S231:Block",
              "S231:containsBlock": { "@id": "http://example.org#A" } }
        ]
    })
}

/// A valid single root R OUTSIDE the cycle reaching A→B→C→A via `containsBlock` — the only shape
/// that reaches the cycle detector. Three distinct cycle nodes keep the pinned participant list
/// non-palindromic, so a reversed reconstruction cannot pass by accident.
fn reachable_cycle_doc() -> JsonValue {
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [
            { "@id": "http://example.org#R", "@type": "S231:Block",
              "S231:containsBlock": { "@id": "http://example.org#A" } },
            { "@id": "http://example.org#A", "@type": "S231:Block",
              "S231:containsBlock": { "@id": "http://example.org#B" } },
            { "@id": "http://example.org#B", "@type": "S231:Block",
              "S231:containsBlock": { "@id": "http://example.org#C" } },
            { "@id": "http://example.org#C", "@type": "S231:Block",
              "S231:containsBlock": { "@id": "http://example.org#A" } }
        ]
    })
}

/// BASE with `isReplaceable=true` on the `c2` leaf component.
fn replaceable_doc() -> JsonValue {
    let mut doc = base();
    node_mut(&mut doc, ".c2")["S231:isReplaceable"] = json!(true);
    doc
}

/// BASE with a banned Modelica construct key (any spelling) surviving on `c2`.
fn banned_key_doc(key: &str) -> JsonValue {
    let mut doc = base();
    node_mut(&mut doc, ".c2")[key] = json!("http://example.org#SomeBase");
    doc
}

/// BASE with an array-valued parameter `M.p` on the top composite.
fn array_parameter_doc() -> JsonValue {
    let mut doc = base();
    node_mut(&mut doc, "#M")["S231:hasParameter"] = json!({ "@id": "http://example.org#M.p" });
    doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .push(json!({
            "@id": "http://example.org#M.p",
            "S231:isArray": true,
            "S231:value": { "@value": "1.0", "@type": "http://www.w3.org/2001/XMLSchema#double" }
        }));
    doc
}

#[test]
fn multi_root_rejection_enumerates_candidates_in_document_order_with_first_as_subject() {
    assert_eq!(
        reject(&multi_root_doc()),
        vec![error_with_subject(
            DiagCode::MalformedDocument,
            "http://example.org#M",
            "composite/root-count: expected exactly one top composite root after nested \
             classification, found 2 candidate roots: http://example.org#M, \
             http://example.org#M2",
        )],
        "the multi-root rejection must enumerate every candidate in @graph order and subject \
         the first"
    );
}

#[test]
fn pure_containsblock_cycle_classifies_as_zero_roots_never_reaching_the_cycle_detector() {
    let diags = reject(&pure_cycle_doc());
    assert_eq!(
        diags,
        vec![Diagnostic::error(
            DiagCode::MalformedDocument,
            "composite/root-count: expected exactly one top composite root after nested \
             classification, found zero candidate roots",
        )],
        "a pure cycle makes every composite referenced, so it must reject as root-count found \
         zero — with NO subject, because there is no candidate to name"
    );
    assert!(
        diags
            .iter()
            .all(|d| !d.message.contains("composite/contains-cycle")),
        "the cycle detector must be unreachable for a rootless pure cycle: {diags:#?}"
    );
}

#[test]
fn reachable_containsblock_cycle_names_every_participant_in_path_order() {
    assert_eq!(
        reject(&reachable_cycle_doc()),
        vec![error_with_subject(
            DiagCode::MalformedDocument,
            "http://example.org#A",
            "composite/contains-cycle: cycle in nested composite containsBlock graph: \
             http://example.org#A -> http://example.org#B -> http://example.org#C -> \
             http://example.org#A",
        )],
        "the cycle rejection must name all participants in traversal path order, closing with \
         the re-entered id, which stays the subject"
    );
}

#[test]
fn banned_modelica_key_rejections_are_tagged_and_name_the_authored_key_in_any_spelling() {
    // All six banned terms, covering bare, `S231:`-prefixed, and absolute-IRI spellings.
    for key in [
        "redeclare",
        "modelicaSource",
        "S231:constrainedby",
        "S231:extendsFrom",
        "http://data.ashrae.org/S231P#extends",
        "http://data.ashrae.org/S231P#moSource",
    ] {
        assert_eq!(
            reject(&banned_key_doc(key)),
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.c2",
                &format!(
                    "composite/banned-modelica-key: unsupported Modelica construct `{key}` \
                     survived CXF lowering"
                ),
            )],
            "the {key} rejection must carry the tag, the owning node subject, and the key as \
             authored"
        );
    }
}

#[test]
fn replaceable_component_rejection_is_tagged_with_the_component_subject() {
    assert_eq!(
        reject(&replaceable_doc()),
        vec![error_with_subject(
            DiagCode::UnresolvedPolymorphism,
            "http://example.org#M.c2",
            "composite/replaceable: replaceable CXF components must be resolved before import",
        )]
    );
}

#[test]
fn array_valued_composite_parameter_rejection_is_tagged_with_the_parameter_subject() {
    assert_eq!(
        reject(&array_parameter_doc()),
        vec![error_with_subject(
            DiagCode::NonSubsetConstruct,
            "http://example.org#M.p",
            "composite/array-parameter: array-valued composite parameters are not supported by \
             this CXF lowering subset",
        )]
    );
}

#[test]
fn contract_rejection_enumerations_are_byte_identical_across_repeated_imports() {
    // The two multi-offender enumerations (candidate roots, cycle participants) must not leak
    // any map-iteration order: repeated imports yield identical full messages.
    for doc in [multi_root_doc(), reachable_cycle_doc()] {
        assert_eq!(
            reject(&doc),
            reject(&doc),
            "enumerated rejection messages must be byte-identical across imports"
        );
    }
}

#[test]
fn every_contract_rejection_starts_with_its_published_catalog_prefix() {
    let catalog: JsonValue = serde_json::from_str(CATALOG_JSON).expect("catalog parses as JSON");
    let catalog = catalog.as_object().expect("catalog top-level object");
    let fixtures: [(&str, JsonValue); 5] = [
        ("root-count", multi_root_doc()),
        ("contains-cycle", reachable_cycle_doc()),
        ("replaceable", replaceable_doc()),
        ("banned-modelica-key", banned_key_doc("redeclare")),
        ("array-parameter", array_parameter_doc()),
    ];
    assert_eq!(
        catalog.len(),
        fixtures.len(),
        "published catalog and fixture coverage must stay one-to-one"
    );
    for (rule_id, doc) in fixtures {
        let entry = catalog
            .get(rule_id)
            .unwrap_or_else(|| panic!("catalog entry missing for rule {rule_id:?}"));
        let prefix = entry["message_prefix"]
            .as_str()
            .expect("message_prefix string");
        let diag_code = entry["diag_code"].as_str().expect("diag_code string");
        let tagged: Vec<Diagnostic> = reject(&doc)
            .into_iter()
            .filter(|d| d.message.starts_with("composite/"))
            .collect();
        assert!(
            !tagged.is_empty(),
            "{rule_id}: fixture must emit a tagged contract rejection"
        );
        for diag in tagged {
            assert!(
                diag.message.starts_with(prefix),
                "{rule_id}: emitted message must start with the published prefix {prefix:?}, \
                 got {:?}",
                diag.message
            );
            assert_eq!(
                diag.code.as_str(),
                diag_code,
                "{rule_id}: emitted DiagCode must match the published diag_code"
            );
        }
    }
}

#[test]
fn co_located_contract_rejections_keep_the_pinned_post_sort_order() {
    // Two contract rules violated on the SAME node: `finalize_diags` sorts non-connector
    // subjects by (subject, code, message), so the NonSubsetConstruct banned-key rejection
    // precedes the UnresolvedPolymorphism replaceable rejection deterministically.
    let mut doc = base();
    let c2 = node_mut(&mut doc, ".c2");
    c2["S231:isReplaceable"] = json!(true);
    c2["redeclare"] = json!("http://example.org#SomeBase");
    assert_eq!(
        reject(&doc),
        vec![
            error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.c2",
                "composite/banned-modelica-key: unsupported Modelica construct `redeclare` \
                 survived CXF lowering",
            ),
            error_with_subject(
                DiagCode::UnresolvedPolymorphism,
                "http://example.org#M.c2",
                "composite/replaceable: replaceable CXF components must be resolved before \
                 import",
            ),
        ],
        "co-located contract rejections must keep the finalize_diags (subject, code, message) \
         order"
    );
}
