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
fn scheme_invalid_node_id_is_refused_like_a_colon_free_relative() {
    // The old verbatim arm accepted ANY colon-containing @id: both spellings below loaded
    // clean as durable point identities with zero diagnostics. The guard is now symmetric
    // with the @context binding check, so ingest refuses — an error, not a warning — with
    // the same diagnostic shape as `MinLoop` above.
    for id in ["2024:MinLoop.con.y", ":MinLoop.con.y"] {
        let document = json!({
            "@context": { "S231": "http://data.ashrae.org/S231P#" },
            "@graph": [ { "@id": id, "@type": "S231:Block" } ]
        });
        let diags = import_value(&document).expect_err("non-absolute @id must reject");
        assert_eq!(diags.len(), 1, "{id}: {diags:?}");
        assert_eq!(diags[0].code, DiagCode::RelativeIri);
        assert_eq!(diags[0].subject.as_deref(), Some(id));
        assert!(
            diags[0].message.contains(&format!("`{id}`")) && diags[0].message.contains("@base"),
            "{}",
            diags[0].message
        );
    }
}

#[test]
fn scheme_invalid_structural_reference_is_refused_naming_slot_owner_and_token() {
    let document = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [ {
            "@id": "http://example.org#M",
            "@type": "S231:Block",
            "S231:hasInput": { "@id": "2024:M.u" }
        } ]
    });
    let diags = import_value(&document).expect_err("non-absolute reference must reject");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::RelativeIri);
    assert_eq!(diags[0].subject.as_deref(), Some("http://example.org#M"));
    assert!(
        diags[0].message.contains("S231:hasInput")
            && diags[0].message.contains("`2024:M.u`")
            && diags[0].message.contains("http://example.org#M"),
        "{}",
        diags[0].message
    );
}

#[test]
fn scheme_invalid_class_token_is_class_not_found_not_relative_iri() {
    // Typing stays best-effort under the symmetric identity guard: the spelling that refuses
    // as an @id (`2024:` is not an RFC 3986 scheme) passes through verbatim in a @type slot
    // and fails class lookup downstream instead.
    let document = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [
            { "@id": "http://example.org#M", "@type": "S231:Block",
              "S231:containsBlock": { "@id": "http://example.org#M.c1" } },
            { "@id": "http://example.org#M.c1", "@type": "2024:NotAnIri",
              "S231:hasOutput": { "@id": "http://example.org#M.c1.y" } },
            { "@id": "http://example.org#M.c1.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    });
    let diags = import_value(&document).expect_err("junk class token must reject downstream");
    assert!(
        diags
            .iter()
            .any(|diag| diag.code == DiagCode::ClassNotFound),
        "expected ClassNotFound, got {diags:?}"
    );
    assert!(
        diags.iter().all(|diag| diag.code != DiagCode::RelativeIri),
        "typing must never take the identity refusal: {diags:?}"
    );
}

#[test]
fn double_slash_node_id_without_a_valid_scheme_is_refused_like_a_colon_free_relative() {
    // Each spelling below is refused as a `@context` binding value yet loaded clean as a
    // durable identity while the `//` arm went scheme-unchecked. The arm now runs the same
    // absolute-form guard as the undeclared-prefix arm, and an expansion refusal returns
    // alone before the resolver runs, so a one-node document yields exactly one diagnostic
    // with the same shape as the colon-free `MinLoop` refusal above.
    for id in [
        "1st://x",
        "://x",
        "2024://x",
        "éttp://x",
        " http://x",
        "http ://x",
    ] {
        let document = json!({
            "@context": { "S231": "http://data.ashrae.org/S231P#" },
            "@graph": [ { "@id": id, "@type": "S231:Block" } ]
        });
        let diags = import_value(&document).expect_err("scheme-invalid `//` @id must reject");
        assert_eq!(diags.len(), 1, "{id:?}: {diags:?}");
        assert_eq!(diags[0].code, DiagCode::RelativeIri);
        assert_eq!(diags[0].subject.as_deref(), Some(id));
        assert!(
            diags[0].message.contains(&format!("`{id}`")) && diags[0].message.contains("@base"),
            "{}",
            diags[0].message
        );
    }
}

#[test]
fn double_slash_structural_reference_without_a_valid_scheme_names_slot_owner_and_token() {
    let document = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [ {
            "@id": "http://example.org#M",
            "@type": "S231:Block",
            "S231:hasInput": { "@id": "2024://M.u" }
        } ]
    });
    let diags = import_value(&document).expect_err("scheme-invalid `//` reference must reject");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::RelativeIri);
    assert_eq!(diags[0].subject.as_deref(), Some("http://example.org#M"));
    assert!(
        diags[0].message.contains("S231:hasInput")
            && diags[0].message.contains("`2024://M.u`")
            && diags[0].message.contains("http://example.org#M"),
        "{}",
        diags[0].message
    );
}

#[test]
fn double_slash_class_token_without_a_valid_scheme_is_class_not_found_not_relative_iri() {
    // Typing stays best-effort under the guarded `//` arm: the spelling that refuses as an
    // @id passes through verbatim in a @type slot and fails class lookup downstream.
    let document = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": [
            { "@id": "http://example.org#M", "@type": "S231:Block",
              "S231:containsBlock": { "@id": "http://example.org#M.c1" } },
            { "@id": "http://example.org#M.c1", "@type": "2024://NotAnIri",
              "S231:hasOutput": { "@id": "http://example.org#M.c1.y" } },
            { "@id": "http://example.org#M.c1.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    });
    let diags = import_value(&document).expect_err("junk class token must reject downstream");
    assert!(
        diags
            .iter()
            .any(|diag| diag.code == DiagCode::ClassNotFound),
        "expected ClassNotFound, got {diags:?}"
    );
    assert!(
        diags.iter().all(|diag| diag.code != DiagCode::RelativeIri),
        "typing must never take the identity refusal: {diags:?}"
    );
}

#[test]
fn a_declared_prefix_never_rescues_a_double_slash_node_id() {
    // Per JSON-LD 1.1 §4.1.5 (Compact IRIs) a value whose suffix begins with `//` is
    // interpreted as an IRI instead of being CURIE-expanded, so `2024://x` is not a
    // CURIE even with `2024` declared: it must refuse, never expand to
    // `http://example.org#//x` through the prefix lookup. (Contrast
    // `declaring_the_prefix_rescues_a_scheme_invalid_compact_token` in the unit suite:
    // the same declaration DOES rescue `2024:x`, whose suffix is CURIE-legal.)
    let document = json!({
        "@context": { "2024": "http://example.org#" },
        "@graph": [ { "@id": "2024://x" } ]
    });
    let diags = import_value(&document).expect_err("`//`-suffix token must never prefix-expand");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::RelativeIri);
    assert_eq!(diags[0].subject.as_deref(), Some("2024://x"));
    assert!(
        diags[0].message.contains("`2024://x`") && diags[0].message.contains("@base"),
        "{}",
        diags[0].message
    );
}

#[test]
fn bare_scheme_node_id_is_refused_even_though_its_scheme_is_rfc_valid() {
    // The remainder tightening at ingest: `ab` is an RFC-valid scheme, so the refusal is
    // about what follows the colon — nothing, or whitespace — never the scheme itself.
    for id in ["ab:", "ab:c d", "ab:c\nd"] {
        let document = json!({
            "@context": { "S231": "http://data.ashrae.org/S231P#" },
            "@graph": [ { "@id": id, "@type": "S231:Block" } ]
        });
        let diags = import_value(&document).expect_err("empty/whitespace remainder must reject");
        assert_eq!(diags.len(), 1, "{id:?}: {diags:?}");
        assert_eq!(diags[0].code, DiagCode::RelativeIri);
        assert_eq!(diags[0].subject.as_deref(), Some(id));
    }
}

#[test]
fn whitespace_suffix_compact_token_and_its_expanded_twin_refuse_identically() {
    // The declared-prefix arm's joined-expansion check, through real ingest. At the
    // previous head the compact spelling below LOADED and minted the durable key
    // `http://example.org#MinLoop.con x` while the expanded spelling of the same subject
    // refused — compact/expanded twins of one subject diverged, the exact
    // spelling-changes-identity hazard this pass exists to close. Both spellings now
    // refuse with the same code, the authored token as subject, and the same message
    // shape.
    for id in ["ex:MinLoop.con x", "http://example.org#MinLoop.con x"] {
        let document = json!({
            "@context": { "ex": "http://example.org#" },
            "@graph": [ { "@id": id } ]
        });
        let diags = import_value(&document).expect_err("whitespace-bearing @id must reject");
        assert_eq!(diags.len(), 1, "{id:?}: {diags:?}");
        assert_eq!(diags[0].code, DiagCode::RelativeIri);
        assert_eq!(diags[0].subject.as_deref(), Some(id));
        assert!(
            diags[0].message.contains(&format!("`{id}`")) && diags[0].message.contains("@base"),
            "{}",
            diags[0].message
        );
    }
}

#[test]
fn a_whitespace_bearing_identity_stops_at_import_before_it_can_reach_export() {
    // The executed roundtrip break at the previous head: the compact twin with a
    // whitespace-bearing root @id imported clean, the exporter emitted
    // `http://example.org#MinLoop x` verbatim, and re-importing the engine's OWN export
    // refused with RelativeIri. Import→export→re-import agreement for this shape is now
    // vacuously restored — nothing with this shape survives the import step — so the pin
    // lives here, on the exact diagnostic the import refusal produces.
    let mut document: Value = serde_json::from_slice(COMPACT_TWIN).expect("twin parses");
    document["@graph"][0]["@id"] = json!("ex:MinLoop x");
    let diags = import_value(&document).expect_err("whitespace-bearing identity must reject");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::RelativeIri);
    assert_eq!(diags[0].subject.as_deref(), Some("ex:MinLoop x"));
    assert_eq!(
        diags[0].message,
        "@id `ex:MinLoop x` is a relative IRI reference and the document declares no @base \
         to resolve it against"
    );
}

#[test]
fn bare_scheme_binding_value_is_refused_as_non_subset_at_ingest() {
    // The same predicate guards binding values, so the tightening narrows acceptance on
    // that side too: `{"ex": "urn:"}` is legal JSON-LD, refused here by policy before any
    // slot is read.
    let document = json!({
        "@context": { "ex": "urn:" },
        "@graph": [ { "@id": "http://example.org#M" } ]
    });
    let diags = import_value(&document).expect_err("bare-scheme binding must reject");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct);
    assert_eq!(diags[0].subject.as_deref(), Some("ex"));
    assert!(
        diags[0].message.contains("absolute IRI"),
        "{}",
        diags[0].message
    );
}

#[test]
fn valid_scheme_double_slash_and_urn_root_ids_load_clean_and_verbatim() {
    // Acceptance side of the guard through real ingest on a structurally valid document:
    // the fixture's root @id is rewritten to the token under test while everything else
    // keeps its canonical spelling, so an accepted form loads with five blocks and zero
    // diagnostics and surfaces verbatim as the model IRI.
    for id in [
        "http://x",
        "https://x/path",
        "a2+b-c.d://x",
        "urn:open-control:model:root",
    ] {
        let mut document: Value = serde_json::from_slice(ORIGINAL).expect("fixture parses");
        document["@graph"][0]["@id"] = json!(id);
        let bytes = serde_json::to_vec(&document).expect("serialize mutated fixture");
        let (graph, report) =
            import_cxf(&bytes, &ResolveOptions::default()).expect("accepted form loads");
        assert!(report.diagnostics.is_empty(), "{id}: {report:?}");
        assert_eq!(report.model_iri.as_deref(), Some(id));
        assert_eq!(graph.blocks.len(), 5, "{id}");
    }
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
fn non_absolute_prefix_binding_refuses_up_front_with_one_diagnostic() {
    // Reviewer probe J: the compact twin with `ex` bound to the empty string previously loaded
    // with ZERO diagnostics and identities like `MinLoop.con.y` — a relative identity routed
    // past the per-token guard by concatenation. The binding itself now refuses, alone and
    // before any slot is read.
    let mut document: Value = serde_json::from_slice(COMPACT_TWIN).expect("twin parses");
    document["@context"]["ex"] = json!("");
    let diags = import_value(&document).expect_err("empty prefix binding must reject");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct);
    assert_eq!(diags[0].subject.as_deref(), Some("ex"));
    assert!(
        diags[0].message.contains("absolute IRI"),
        "{}",
        diags[0].message
    );
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
