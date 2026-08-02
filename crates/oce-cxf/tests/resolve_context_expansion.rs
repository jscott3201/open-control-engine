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
    // The render comparator does not cover `boundary_outputs` (extending it would move the
    // checked-in resolver goldens), so that field is compared directly.
    let boundary_outputs = |graph: &oce_model::ModelGraph| -> Vec<(String, u32)> {
        graph
            .boundary_outputs
            .iter()
            .map(|output| (output.iri.to_string(), output.source.0))
            .collect()
    };
    let original_outputs = boundary_outputs(&original);
    assert!(
        !original_outputs.is_empty(),
        "twin boundary-output comparison must not be vacuous"
    );
    assert_eq!(original_outputs, boundary_outputs(&compact));
    assert_eq!(
        compact_report.model_iri.as_deref(),
        Some("http://example.org#MinLoop"),
        "the compact root @id must resolve to the canonical absolute model IRI"
    );
    assert_eq!(original_report.model_iri, compact_report.model_iri);
}

/// The kill for the list-shaped-context hole: the SAME bindings as the compact twin's map
/// context, spelled as a list of maps, must produce the identical model — before this held,
/// a list context yielded an empty prefix table and the compact tokens loaded VERBATIM as
/// durable identities with zero diagnostics.
#[test]
fn list_shaped_context_with_the_same_bindings_loads_identically_to_the_map_shape() {
    let mut document: Value = serde_json::from_slice(COMPACT_TWIN).expect("twin parses");
    let map = document["@context"]
        .as_object()
        .expect("map context")
        .clone();
    document["@context"] = Value::Array(
        map.into_iter()
            .map(|(term, iri)| {
                let mut entry = serde_json::Map::new();
                entry.insert(term, iri);
                Value::Object(entry)
            })
            .collect(),
    );
    let list_bytes = serde_json::to_vec(&document).expect("serialize list-context twin");
    let (map_shaped, _) =
        import_cxf(COMPACT_TWIN, &ResolveOptions::default()).expect("map-shaped twin loads");
    let (list_shaped, list_report) =
        import_cxf(&list_bytes, &ResolveOptions::default()).expect("list-shaped twin loads");
    assert!(list_report.diagnostics.is_empty(), "{list_report:?}");
    assert_eq!(render::render(&map_shaped), render::render(&list_shaped));
    assert_eq!(
        list_report.model_iri.as_deref(),
        Some("http://example.org#MinLoop")
    );
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
    // The OUT-list positive pin: the fixture declares `unit` as a prefix AND uses it in a
    // `S231:unit` IRI-node decoration on gai.y. The lexical term must survive VERBATIM —
    // `TermAttr::Iri` is excluded from expansion even when its prefix is declared.
    match &graph.connectors[1].attrs {
        oce_model::Attrs::Real(attrs) => assert_eq!(
            attrs.unit.as_deref(),
            Some("unit:K"),
            "the unit term must stay the authored CURIE, never expand"
        ),
        other => panic!("gai.y must carry Real attrs, got {other:?}"),
    }
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

#[test]
fn remote_context_reference_is_refused_as_non_subset() {
    // The whole context a remote reference, and a remote reference inside a list: both are
    // refused — an embedded deterministic engine dereferences nothing at load. Before this
    // held, both shapes yielded an empty prefix table and compact tokens loaded VERBATIM as
    // durable identities with zero diagnostics.
    for context in [
        json!("http://example.org/context.jsonld"),
        json!([{ "ex": "http://example.org#" }, "http://example.org/context.jsonld"]),
    ] {
        let document = json!({
            "@context": context,
            "@graph": [ { "@id": "http://example.org#M" } ]
        });
        let diags = import_value(&document).expect_err("remote context must reject");
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct);
        assert_eq!(
            diags[0].subject.as_deref(),
            Some("http://example.org/context.jsonld")
        );
    }
}

#[test]
fn context_base_and_vocab_declarations_are_refused_as_non_subset() {
    // The review's @vocab probe: at the previous head this document survived context handling
    // (the keyword skipped as if it were @version) and died much later in composite root
    // classification with a confusing refusal. Both keywords now refuse up front, named.
    for keyword in ["@base", "@vocab"] {
        let mut context = serde_json::Map::new();
        context.insert(keyword.to_owned(), json!("http://example.org/"));
        context.insert("ex".to_owned(), json!("http://example.org#"));
        let document = json!({
            "@context": context,
            "@graph": [ { "@id": "ex:M" } ]
        });
        let diags = import_value(&document).expect_err("identity-changing keyword must reject");
        assert_eq!(diags.len(), 1, "{keyword}: {diags:?}");
        assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct);
        assert_eq!(diags[0].subject.as_deref(), Some(keyword));
        assert!(diags[0].message.contains(keyword), "{}", diags[0].message);
    }
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
