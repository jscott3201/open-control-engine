//! `@context` expansion at resolve: compact and expanded spellings key the same model, and an
//! identity token the document cannot canonicalize is a typed refusal, never a silent miskey.

mod render;

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value, json};

const ORIGINAL: &[u8] = include_bytes!("fixtures/minimal_loop.jsonld");
const COMPACT_TWIN: &[u8] = include_bytes!("fixtures/minimal_loop_compact.jsonld");
const MODELICA_JSON_STYLE: &[u8] = include_bytes!("fixtures/modelica_json_style.jsonld");

fn import_value(value: &Value) -> Result<(), Vec<Diagnostic>> {
    let bytes = serde_json::to_vec(value).expect("serialize test document");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Ok(_) => Ok(()),
        Err(CxfError::Validation(diags)) => Err(diags),
        Err(other) => panic!("expected validation diagnostics, got {other:?}"),
    }
}

#[test]
fn compact_twin_imports_bit_identical_to_the_expanded_original() {
    let (original, original_report) =
        import_cxf(ORIGINAL, &ResolveOptions::default()).expect("original loads");
    let (compact, compact_report) =
        import_cxf(COMPACT_TWIN, &ResolveOptions::default()).expect("compact twin loads");
    assert!(
        original_report.diagnostics.is_empty(),
        "{original_report:?}"
    );
    assert!(compact_report.diagnostics.is_empty(), "{compact_report:?}");
    // The twin's prefix bindings expand to byte-identical IRIs, so the whole imported model —
    // ids, wiring, parameters, §7.4.1 attrs — must render bit-identically.
    assert_eq!(render::render(&original), render::render(&compact));
    assert_eq!(
        compact_report.model_iri.as_deref(),
        Some("http://example.org#MinLoop"),
        "the compact root @id must resolve to the canonical absolute model IRI"
    );
    assert_eq!(original_report.model_iri, compact_report.model_iri);
}

#[test]
fn modelica_json_style_document_loads_clean_with_expanded_identities() {
    let (graph, report) =
        import_cxf(MODELICA_JSON_STYLE, &ResolveOptions::default()).expect("style fixture loads");
    assert!(report.diagnostics.is_empty(), "{report:?}");
    let prefix = "http://example.org#Buildings.Controls.OBC.Examples.SupplyGain";
    assert_eq!(
        report.model_iri.as_deref(),
        Some(prefix.to_owned()).as_deref()
    );
    let connector_iris: Vec<&str> = graph
        .connectors
        .iter()
        .map(|connector| connector.iri.as_deref().expect("authored connector iri"))
        .collect();
    // The boundary-driven child input surfaces under the boundary's expanded @id (AD-2); the
    // child output keeps its own expanded @id.
    assert_eq!(
        connector_iris,
        [format!("{prefix}.TSup"), format!("{prefix}.gai.y")]
    );
    assert_eq!(graph.blocks.len(), 1);
    assert_eq!(
        graph.blocks[0].instance_iri.as_deref(),
        Some(format!("{prefix}.gai")).as_deref()
    );
    assert_eq!(
        graph.blocks[0].class_iri.as_ref(),
        "CDL.Reals.MultiplyByParameter"
    );
    assert_eq!(graph.boundary_outputs.len(), 1);
    assert_eq!(
        graph.boundary_outputs[0].iri.as_ref(),
        format!("{prefix}.ySup")
    );
}

#[test]
fn compact_and_absolute_spellings_of_one_subject_collide_as_duplicate_id() {
    let document = json!({
        "@context": {
            "S231": "http://data.ashrae.org/S231P#",
            "ex": "http://example.org#"
        },
        "@graph": [
            { "@id": "ex:A" },
            { "@id": "http://example.org#A" }
        ]
    });
    let diags = import_value(&document).expect_err("cross-form duplicate must reject");
    assert!(
        diags.iter().any(|diag| {
            diag.code == DiagCode::DuplicateId
                && diag.subject.as_deref() == Some("http://example.org#A")
        }),
        "expected DuplicateId on the canonical form, got {diags:?}"
    );
}

#[test]
fn relative_node_id_is_refused_with_a_typed_diagnostic() {
    let document = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [ { "@id": "MinLoop", "@type": "S231:Block" } ]
    });
    let diags = import_value(&document).expect_err("relative @id must reject");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::RelativeIri);
    assert_eq!(diags[0].subject.as_deref(), Some("MinLoop"));
    assert!(
        diags[0].message.contains("`MinLoop`") && diags[0].message.contains("@base"),
        "{}",
        diags[0].message
    );
}

#[test]
fn relative_structural_reference_is_refused_naming_slot_owner_and_token() {
    let document = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [ {
            "@id": "http://example.org#M",
            "@type": "S231:Block",
            "S231:hasInput": { "@id": "uSet" }
        } ]
    });
    let diags = import_value(&document).expect_err("relative reference must reject");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::RelativeIri);
    assert_eq!(diags[0].subject.as_deref(), Some("http://example.org#M"));
    assert!(
        diags[0].message.contains("S231:hasInput")
            && diags[0].message.contains("`uSet`")
            && diags[0].message.contains("http://example.org#M"),
        "{}",
        diags[0].message
    );
}

#[test]
fn non_string_context_term_is_refused_as_malformed_document() {
    let document = json!({
        "@context": {
            "S231": { "@id": "http://data.ashrae.org/S231P#" }
        },
        "@graph": [ { "@id": "http://example.org#M" } ]
    });
    let diags = import_value(&document).expect_err("non-string context term must reject");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::MalformedDocument);
    assert_eq!(diags[0].subject.as_deref(), Some("S231"));
}

/// The G36 enum parameter document used by the closed-world safety pin, with the declared type
/// spelled as a CURIE (`base:Buildings.…G36.Types.VentilationStandard`) the way a compact
/// emitter writes it, and `literal` as the parameter's bound value.
fn compact_enum_document(literal: &str) -> Value {
    json!({
        "@context": {
            "S231": "http://data.ashrae.org/S231P#",
            "base": "http://example.org#"
        },
        "@graph": [
            {
                "@id": "base:M",
                "@type": "S231:Block",
                "S231:containsBlock": { "@id": "base:M.con" },
                "S231:hasOutput": { "@id": "base:M.y" }
            },
            {
                "@id": "base:M.con",
                "@type": "base:Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                "S231:hasParameter": [
                    { "@id": "base:M.con.k" },
                    { "@id": "base:M.con.venStd" }
                ],
                "S231:hasOutput": { "@id": "base:M.con.y" }
            },
            { "@id": "base:M.con.k",
              "S231:value": { "@value": "2.0", "@type": "http://www.w3.org/2001/XMLSchema#double" } },
            { "@id": "base:M.con.venStd",
              "@type": "S231:Parameter",
              "S231:isOfDataType": {
                  "@id": "base:Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard"
              },
              "S231:value": literal },
            { "@id": "base:M.con.y",
              "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" },
              "S231:isConnectedTo": { "@id": "base:M.y" } },
            { "@id": "base:M.y",
              "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    })
}

/// The safety regression R4(e) pins: at base, a compact `isOfDataType` silently DISABLED G36
/// closed-world enum checking — `canonical_g36_type` splits on `#`/`/` only, so the CURIE made
/// `g36_enum_class_id` return `None` and the wrong-class literal below loaded with zero
/// diagnostics. Expansion canonicalizes the declared type first, so the closed world sees it.
#[test]
fn compact_g36_datatype_no_longer_disables_closed_world_enum_checking() {
    let wrong_class =
        compact_enum_document("Buildings.Controls.OBC.ASHRAE.G36.Types.CoolingCoil.WaterBased");
    let diags = import_value(&wrong_class).expect_err("wrong-class enum literal must reject");
    assert!(
        diags.iter().any(|diag| {
            diag.code == DiagCode::TypeMismatch
                && diag.message.contains("does not match declared type")
        }),
        "expected a closed-world TypeMismatch, got {diags:?}"
    );
    // Control: the identical document with a literal OF the declared class loads clean, so the
    // rejection above is the literal's class, not some other defect of the fixture.
    let matching_class = compact_enum_document(
        "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard.ASHRAE62_1",
    );
    import_value(&matching_class).expect("matching-class enum literal loads");
}
