use oce_cxf::{CxfError, ResolveOptions, export, export_with_report, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{Attrs, BoundaryInput, NoAttrs, RealAttrs};
use serde_json::Value as JsonValue;

use super::document::{attr_input_document, node_mut};
use crate::{ATTR_KEYS, DECLARED_INPUT_ATTRS, import_ok, node_by_id, reference_ids};

const ENUM_DEFERRAL: &str = include_str!("../fixtures/enum_deferral_miniature.jsonld");

fn assert_import_diagnostic(document: JsonValue, expected: Diagnostic) {
    let bytes = serde_json::to_vec(&document).expect("document serializes");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Err(CxfError::Validation(diagnostics)) => assert_eq!(diagnostics, vec![expected]),
        other => panic!("expected one validation diagnostic, got {other:?}"),
    }
}

#[test]
fn represented_declarations_refuse_each_existing_attr_failure_family() {
    let subject = "http://example.org#AttrInput.uExt";
    assert_import_diagnostic(
        attr_input_document(
            "Real",
            "CDL.Reals.Add",
            serde_json::json!({ "S231:unit": 7 }),
        ),
        Diagnostic::error(
            DiagCode::MalformedDocument,
            "S231:unit is not a string, typed literal, or IRI node — malformed term",
        )
        .with_subject(subject.to_owned()),
    );
    assert_import_diagnostic(
        attr_input_document(
            "Real",
            "CDL.Reals.Add",
            serde_json::json!({ "S231:min": false }),
        ),
        Diagnostic::error(
            DiagCode::MalformedDocument,
            "S231:min on a Real connector did not ground to a number",
        )
        .with_subject(subject.to_owned()),
    );
    assert_import_diagnostic(
        attr_input_document(
            "Real",
            "CDL.Reals.Add",
            serde_json::json!({ "S231:min": "k" }),
        ),
        Diagnostic::error(
            DiagCode::GroundingFailed,
            "S231:min connector bound failed to ground: expression binding did not ground: \
             unknown identifier: k",
        )
        .with_subject(subject.to_owned()),
    );
    assert_import_diagnostic(
        attr_input_document(
            "Integer",
            "CDL.Integers.Add",
            serde_json::json!({ "S231:min": 1.5 }),
        ),
        Diagnostic::error(
            DiagCode::MalformedDocument,
            "S231:min on an Integer connector did not ground to an integer",
        )
        .with_subject(subject.to_owned()),
    );
    assert_import_diagnostic(
        attr_input_document(
            "Integer",
            "CDL.Integers.Add",
            serde_json::json!({ "S231:min": "k" }),
        ),
        Diagnostic::error(
            DiagCode::GroundingFailed,
            "S231:min connector bound failed to ground: expression binding did not ground: \
             unknown identifier: k",
        )
        .with_subject(subject.to_owned()),
    );
    assert_import_diagnostic(
        attr_input_document(
            "Boolean",
            "CDL.Logical.And",
            serde_json::json!({ "S231:unit": "K" }),
        ),
        Diagnostic::error(
            DiagCode::MalformedDocument,
            "§7.4.1 attribute (unit/quantity/displayUnit/min/max) declared on a Boolean connector, \
             which permits none",
        )
        .with_subject(subject.to_owned()),
    );
}

#[test]
fn distinct_boundary_sources_cannot_claim_one_child_input() {
    let mut document = attr_input_document("Real", "CDL.Reals.Add", serde_json::json!({}));
    node_mut(&mut document, "AttrInput")["S231:hasInput"]
        .as_array_mut()
        .expect("root hasInput array")
        .push(serde_json::json!({ "@id": "http://example.org#AttrInput.uConflict" }));
    document["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .push(serde_json::json!({
            "@id": "http://example.org#AttrInput.uConflict",
            "@type": "S231:RealInput",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:isConnectedTo": { "@id": "http://example.org#AttrInput.add.u1" }
        }));
    assert_import_diagnostic(
        document,
        Diagnostic::error(
            DiagCode::SingleAssignment,
            "input is driven by distinct boundary inputs",
        )
        .with_subject("http://example.org#AttrInput.add.u1".to_owned()),
    );
}

#[test]
fn boundary_input_identity_cannot_also_name_an_instance_connector() {
    let mut document = attr_input_document("Real", "CDL.Reals.Add", serde_json::json!({}));
    node_mut(&mut document, "AttrInput")["S231:hasInput"][0]["@id"] =
        serde_json::json!("http://example.org#AttrInput.add.y");
    node_mut(&mut document, "uExt")
        .as_object_mut()
        .expect("boundary node object")
        .remove("S231:isConnectedTo");
    node_mut(&mut document, "add.y")["S231:isConnectedTo"] =
        serde_json::json!({ "@id": "http://example.org#AttrInput.add.u1" });

    assert_import_diagnostic(
        document,
        Diagnostic::error(
            DiagCode::MalformedDocument,
            "boundary input shadows an instance port connector",
        )
        .with_subject("http://example.org#AttrInput.add.y".to_owned()),
    );
}

#[test]
fn refused_connector_alias_parses_malformed_attrs_once() {
    let mut document = attr_input_document("Real", "CDL.Reals.Add", serde_json::json!({}));
    node_mut(&mut document, "AttrInput")["S231:hasInput"][0]["@id"] =
        serde_json::json!("http://example.org#AttrInput.add.y");
    node_mut(&mut document, "uExt")
        .as_object_mut()
        .expect("boundary node object")
        .remove("S231:isConnectedTo");
    let alias = node_mut(&mut document, "add.y");
    alias["S231:isConnectedTo"] =
        serde_json::json!({ "@id": "http://example.org#AttrInput.add.u1" });
    alias["S231:unit"] = serde_json::json!(7);

    let bytes = serde_json::to_vec(&document).expect("document serializes");
    let diagnostics = match import_cxf(&bytes, &ResolveOptions::default()) {
        Err(CxfError::Validation(diagnostics)) => diagnostics,
        other => panic!("expected validation diagnostics, got {other:?}"),
    };
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(
                DiagCode::MalformedDocument,
                "S231:unit is not a string, typed literal, or IRI node — malformed term",
            )
            .with_subject("http://example.org#AttrInput.add.y".to_owned()),
            Diagnostic::error(
                DiagCode::MalformedDocument,
                "boundary input shadows an instance port connector",
            )
            .with_subject("http://example.org#AttrInput.add.y".to_owned()),
        ]
    );
}

#[test]
fn boundary_input_identity_cannot_also_name_a_contained_block() {
    let mut document = attr_input_document("Real", "CDL.Reals.Add", serde_json::json!({}));
    node_mut(&mut document, "AttrInput")["S231:hasInput"][0]["@id"] =
        serde_json::json!("http://example.org#AttrInput.add");
    node_mut(&mut document, "uExt")
        .as_object_mut()
        .expect("boundary node object")
        .remove("S231:isConnectedTo");
    let block = node_mut(&mut document, "AttrInput.add")
        .as_object_mut()
        .expect("block node object");
    block.insert(
        "S231:isOfDataType".to_owned(),
        serde_json::json!({ "@id": "S231:Real" }),
    );
    block.insert(
        "S231:isConnectedTo".to_owned(),
        serde_json::json!({ "@id": "http://example.org#AttrInput.add.u1" }),
    );

    assert_import_diagnostic(
        document,
        Diagnostic::error(
            DiagCode::MalformedDocument,
            "boundary input shadows a contained block",
        )
        .with_subject("http://example.org#AttrInput.add".to_owned()),
    );
}

#[test]
fn boundary_input_identity_cannot_also_name_an_instance_parameter() {
    let mut document = attr_input_document("Real", "CDL.Reals.Add", serde_json::json!({}));
    node_mut(&mut document, "AttrInput")["S231:hasInput"][0]["@id"] =
        serde_json::json!("http://example.org#AttrInput.add.k");
    node_mut(&mut document, "uExt")
        .as_object_mut()
        .expect("boundary node object")
        .remove("S231:isConnectedTo");
    node_mut(&mut document, "AttrInput.add")["S231:hasParameter"] =
        serde_json::json!({ "@id": "http://example.org#AttrInput.add.k" });
    document["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .push(serde_json::json!({
            "@id": "http://example.org#AttrInput.add.k",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:value": 1.0,
            "S231:isConnectedTo": { "@id": "http://example.org#AttrInput.add.u1" }
        }));

    assert_import_diagnostic(
        document,
        Diagnostic::error(
            DiagCode::MalformedDocument,
            "boundary input shadows an instance member",
        )
        .with_subject("http://example.org#AttrInput.add.k".to_owned()),
    );
}

#[test]
fn boundary_input_identity_cannot_collide_with_a_minted_child_port() {
    let mut document = attr_input_document("Real", "CDL.Reals.Add", serde_json::json!({}));
    node_mut(&mut document, "AttrInput")["S231:hasInput"][0]["@id"] =
        serde_json::json!("http://example.org#AttrInput.add.in0");
    node_mut(&mut document, "uExt")["@id"] =
        serde_json::json!("http://example.org#AttrInput.add.in0");

    assert_import_diagnostic(
        document,
        Diagnostic::error(
            DiagCode::MalformedDocument,
            "boundary input collides with a canonical export node identity",
        )
        .with_subject("http://example.org#AttrInput.add.in0".to_owned()),
    );
}

#[test]
fn boundary_input_identity_cannot_collide_with_a_minted_parameter_node() {
    let mut document = attr_input_document("Real", "CDL.Reals.Add", serde_json::json!({}));
    node_mut(&mut document, "AttrInput")["S231:hasInput"][0]["@id"] =
        serde_json::json!("http://example.org#AttrInput.add.k");
    node_mut(&mut document, "uExt")["@id"] =
        serde_json::json!("http://example.org#AttrInput.add.k");
    node_mut(&mut document, "AttrInput.add")["S231:hasParameter"] =
        serde_json::json!({ "@id": "http://example.org#Other.k" });
    document["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .push(serde_json::json!({
            "@id": "http://example.org#Other.k",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:value": 1.0
        }));

    assert_import_diagnostic(
        document,
        Diagnostic::error(
            DiagCode::MalformedDocument,
            "boundary input collides with a canonical export node identity",
        )
        .with_subject("http://example.org#AttrInput.add.k".to_owned()),
    );
}

#[test]
fn boundary_input_identity_cannot_collide_with_the_canonical_export_root() {
    let mut document = attr_input_document("Real", "CDL.Reals.Add", serde_json::json!({}));
    node_mut(&mut document, "AttrInput")["S231:hasInput"][0]["@id"] =
        serde_json::json!("urn:open-control:cxf-export:root");
    node_mut(&mut document, "uExt")["@id"] = serde_json::json!("urn:open-control:cxf-export:root");

    assert_import_diagnostic(
        document,
        Diagnostic::error(
            DiagCode::MalformedDocument,
            "boundary input collides with a canonical export node identity",
        )
        .with_subject("urn:open-control:cxf-export:root".to_owned()),
    );
}

fn enum_deferral_with_boundary_alias(alias: &str, rename_parameter: bool) -> JsonValue {
    let mut document: JsonValue =
        serde_json::from_str(ENUM_DEFERRAL).expect("enum deferral fixture parses");
    if rename_parameter {
        node_mut(&mut document, "Mini.con")["S231:hasParameter"][1]["@id"] =
            serde_json::json!("http://example.org#Other.k");
        node_mut(&mut document, "Mini.con.k")["@id"] =
            serde_json::json!("http://example.org#Other.k");
    }
    node_mut(&mut document, "Mini")["S231:hasInput"]["@id"] = serde_json::json!(alias);
    node_mut(&mut document, "Mini.bIn")["@id"] = serde_json::json!(alias);
    document
}

#[test]
fn deferred_boundary_aliases_do_not_collide_with_unemitted_nodes() {
    for (alias, rename_parameter) in [
        ("http://example.org#Mini.con.in0", false),
        ("http://example.org#Mini.con.k", true),
        ("urn:open-control:cxf-export:root", false),
    ] {
        let document = enum_deferral_with_boundary_alias(alias, rename_parameter);
        let bytes = serde_json::to_vec(&document).expect("document serializes");
        let graph = import_ok("deferred boundary alias", &bytes);
        assert_eq!(graph.boundary_inputs[0].iri.as_ref(), alias);

        let report = export_with_report(&graph).expect("deferred alias does not reach the wire");
        assert!(!report.warnings.is_empty(), "the target owner is deferred");
        let exported: JsonValue =
            serde_json::from_slice(&report.bytes).expect("exported JSON parses");
        let graph_nodes = exported["@graph"].as_array().expect("@graph array");
        let expected_nodes = usize::from(alias == "urn:open-control:cxf-export:root");
        assert_eq!(
            graph_nodes
                .iter()
                .filter(|node| node["@id"].as_str() == Some(alias))
                .count(),
            expected_nodes,
            "deferred boundary `{alias}` does not add an emitted node"
        );
        let root = node_by_id(&exported, "urn:open-control:cxf-export:root");
        assert!(
            !reference_ids(root.get("S231:hasInput")).contains(alias),
            "deferred boundary `{alias}` is absent from the emitted root interface"
        );
    }
}

#[test]
fn surviving_boundary_can_reuse_identity_minted_only_by_a_deferred_owner() {
    for (alias, rename_parameter) in [
        ("http://example.org#Mini.con.in0", false),
        ("http://example.org#Mini.con.k", true),
    ] {
        let mut document = enum_deferral_with_boundary_alias(alias, rename_parameter);
        node_mut(&mut document, "Mini.src.y")["S231:isConnectedTo"]
            .as_array_mut()
            .expect("source target array")
            .retain(|target| target["@id"].as_str() != Some("http://example.org#Mini.gain.u"));
        node_mut(&mut document, alias)["S231:isConnectedTo"] = serde_json::json!([
            { "@id": "http://example.org#Mini.con.u_s" },
            { "@id": "http://example.org#Mini.gain.u" }
        ]);

        let bytes = serde_json::to_vec(&document).expect("document serializes");
        let graph = import_ok("mixed survivor boundary alias", &bytes);
        let report =
            export_with_report(&graph).expect("only the live boundary identity is emitted");
        assert!(!report.warnings.is_empty(), "one target owner is deferred");
        let exported: JsonValue =
            serde_json::from_slice(&report.bytes).expect("exported JSON parses");
        node_by_id(&exported, alias);
        let root = node_by_id(&exported, "urn:open-control:cxf-export:root");
        assert!(
            reference_ids(root.get("S231:hasInput")).contains(alias),
            "surviving boundary `{alias}` remains on the emitted root interface"
        );
    }
}

fn export_diagnostics(graph: &oce_model::ModelGraph) -> Vec<Diagnostic> {
    match export(graph) {
        Err(CxfError::Validation(diagnostics)) => diagnostics,
        other => panic!("expected export validation diagnostics, got {other:?}"),
    }
}

fn declaration_mut<'graph>(
    graph: &'graph mut oce_model::ModelGraph,
    suffix: &str,
) -> &'graph mut BoundaryInput {
    graph
        .boundary_inputs
        .iter_mut()
        .find(|input| input.iri.ends_with(suffix))
        .unwrap_or_else(|| panic!("boundary declaration ending in `{suffix}`"))
}

#[test]
fn host_sidecar_structure_refuses_instead_of_dropping_metadata() {
    let mut duplicate = import_ok(
        "declared_input_attrs.jsonld",
        DECLARED_INPUT_ATTRS.as_bytes(),
    );
    duplicate
        .boundary_inputs
        .push(duplicate.boundary_inputs[0].clone());
    assert_eq!(
        export_diagnostics(&duplicate),
        vec![
            Diagnostic::error(
                DiagCode::ExportUnsupported,
                "export subset: duplicate boundary-input declaration metadata",
            )
            .with_subject("http://example.org#DeclaredInputAttrs.uPass".to_owned()),
        ]
    );

    let mut orphan = import_ok(
        "declared_input_attrs.jsonld",
        DECLARED_INPUT_ATTRS.as_bytes(),
    );
    orphan.boundary_inputs.push(BoundaryInput {
        iri: "http://example.org#DeclaredInputAttrs.orphan".into(),
        attrs: Attrs::Real(RealAttrs::default()),
    });
    assert_eq!(
        export_diagnostics(&orphan),
        vec![
            Diagnostic::error(
                DiagCode::ExportUnsupported,
                "export subset: boundary-input declaration metadata has no external target",
            )
            .with_subject("http://example.org#DeclaredInputAttrs.orphan".to_owned()),
        ]
    );

    let mut tag_mismatch = import_ok(
        "declared_input_attrs.jsonld",
        DECLARED_INPUT_ATTRS.as_bytes(),
    );
    declaration_mut(&mut tag_mismatch, "uExt").attrs = Attrs::Boolean(NoAttrs);
    assert_eq!(
        export_diagnostics(&tag_mismatch),
        vec![
            Diagnostic::error(
                DiagCode::ExportUnsupported,
                "export subset: block/connector wiring is structurally inconsistent",
            )
            .with_subject("http://example.org#DeclaredInputAttrs.uExt".to_owned()),
        ]
    );
}

#[test]
fn host_sidecar_attrs_obey_the_existing_canonical_export_subset() {
    let mut nonfinite = import_ok(
        "declared_input_attrs.jsonld",
        DECLARED_INPUT_ATTRS.as_bytes(),
    );
    let Attrs::Real(attrs) = &mut declaration_mut(&mut nonfinite, "uExt").attrs else {
        panic!("uExt has Real attrs");
    };
    attrs.min = Some(f64::NAN);
    assert_eq!(
        export_diagnostics(&nonfinite),
        vec![
            Diagnostic::error(
                DiagCode::ExportUnsupported,
                "export subset: connector carries a non-finite §7.4.1 min/max bound, which is \
                 outside the canonical (bare-scalar) export subset",
            )
            .with_subject("http://example.org#DeclaredInputAttrs.uExt".to_owned()),
        ]
    );

    let mut unsupported = import_ok(
        "declared_input_attrs.jsonld",
        DECLARED_INPUT_ATTRS.as_bytes(),
    );
    let Attrs::Real(attrs) = &mut declaration_mut(&mut unsupported, "uExt").attrs else {
        panic!("uExt has Real attrs");
    };
    attrs.nominal = Some(1.0);
    attrs.unbounded = Some(true);
    assert_eq!(
        export_diagnostics(&unsupported),
        vec![
            Diagnostic::error(
                DiagCode::ExportUnsupported,
                "export subset: connector carries a non-default §7.4.1 nominal attribute, which \
                 is outside the canonical (bare-scalar) export subset",
            )
            .with_subject("http://example.org#DeclaredInputAttrs.uExt".to_owned()),
            Diagnostic::error(
                DiagCode::ExportUnsupported,
                "export subset: connector carries a non-default §7.4.1 unbounded attribute, which \
                 is outside the canonical (bare-scalar) export subset",
            )
            .with_subject("http://example.org#DeclaredInputAttrs.uExt".to_owned()),
        ]
    );
}

#[test]
fn absent_sidecars_preserve_legacy_attribute_free_boundary_export() {
    let mut graph = import_ok(
        "declared_input_attrs.jsonld",
        DECLARED_INPUT_ATTRS.as_bytes(),
    );
    graph.boundary_inputs.clear();
    let bytes = export(&graph).expect("missing sidecars retain legacy export support");
    let document: JsonValue = serde_json::from_slice(&bytes).expect("exported JSON");
    for suffix in ["uExt", "uPass"] {
        let node = node_by_id(
            &document,
            &format!("http://example.org#DeclaredInputAttrs.{suffix}"),
        );
        assert!(
            ATTR_KEYS.iter().all(|key| node.get(*key).is_none()),
            "{suffix} emits no inferred child attrs without a declaration sidecar"
        );
    }
}

#[test]
fn sidecar_checks_do_not_turn_an_omitted_owner_into_an_export_error() {
    let mut graph = import_ok("enum_deferral_miniature.jsonld", ENUM_DEFERRAL.as_bytes());
    assert_eq!(
        graph.boundary_inputs.len(),
        1,
        "one represented boundary input"
    );
    let Attrs::Real(attrs) = &mut graph.boundary_inputs[0].attrs else {
        panic!("bIn has Real attrs");
    };
    attrs.min = Some(f64::NAN);
    attrs.nominal = Some(1.0);

    let report =
        export_with_report(&graph).expect("the invalid attrs belong only to a deferred owner");
    assert!(
        !report.warnings.is_empty(),
        "the owner is actually deferred"
    );
    let document: JsonValue = serde_json::from_slice(&report.bytes).expect("exported JSON");
    assert!(
        document["@graph"]
            .as_array()
            .expect("@graph array")
            .iter()
            .all(|node| node["@id"].as_str() != Some("http://example.org#Mini.bIn")),
        "the deferred declaration is omitted rather than emitted without attrs"
    );
}

#[test]
fn duplicate_sidecars_refuse_before_owner_deferral() {
    let mut graph = import_ok("enum_deferral_miniature.jsonld", ENUM_DEFERRAL.as_bytes());
    graph.boundary_inputs.push(graph.boundary_inputs[0].clone());

    let errors = export_diagnostics(&graph)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == oce_diag::Severity::Error)
        .collect::<Vec<_>>();
    assert_eq!(
        errors,
        vec![
            Diagnostic::error(
                DiagCode::ExportUnsupported,
                "export subset: duplicate boundary-input declaration metadata",
            )
            .with_subject("http://example.org#Mini.bIn".to_owned()),
        ]
    );
}
