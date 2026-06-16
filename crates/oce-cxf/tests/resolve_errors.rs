//! Edge-case / never-panic integration tests for the §7.1 resolver (M1-PR-5, `TESTING.md` pillar 1).
//!
//! Each test mutates a minimal valid 2-block composite ([`BASE`]) to trigger exactly one failure,
//! then asserts a **specific** [`DiagCode`] (or [`CxfError::Json`]). The resolver is a *total*
//! function over arbitrary JSON-LD: reaching the assertion at all proves it returned a typed error
//! rather than panicking (M1 exit #6). The type-domain cases (i64 overflow, garbage float) are the
//! C1-lesson tripwires — the bug a happy-path test never writes.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value, json};

/// A minimal valid single-composite model: `c1 = Constant(k=1.0)` whose output drives
/// `c2 = MultiplyByParameter(k=2.0)`'s input. No boundary ports. Resolves with zero diagnostics.
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

fn base() -> Value {
    serde_json::from_str(BASE).expect("BASE is valid JSON")
}

/// A mutable reference to the `@graph` node whose `@id` ends with `suffix`.
fn node_mut<'a>(doc: &'a mut Value, suffix: &str) -> &'a mut Value {
    doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .iter_mut()
        .find(|n| n["@id"].as_str().is_some_and(|s| s.ends_with(suffix)))
        .unwrap_or_else(|| panic!("no @graph node ending in {suffix:?}"))
}

fn import(doc: &Value) -> Result<(oce_model::ModelGraph, oce_cxf::ValidationReport), CxfError> {
    let bytes = serde_json::to_vec(doc).expect("serialize doc");
    import_cxf(&bytes, &ResolveOptions::default())
}

/// Assert the import fails with at least one **error** diagnostic of `code`. Reaching here proves
/// the resolver did not panic on the malformed input.
#[track_caller]
fn assert_error_code(doc: &Value, code: DiagCode) -> Vec<Diagnostic> {
    match import(doc) {
        Err(CxfError::Validation(diags)) => {
            assert!(
                diags.iter().any(|d| d.code == code && d.is_error()),
                "expected an error diagnostic {code:?}, got {diags:#?}"
            );
            diags
        }
        Ok((_g, r)) => panic!("expected Validation({code:?}), but import succeeded: {r:?}"),
        Err(other) => panic!("expected Validation({code:?}), got {other:?}"),
    }
}

#[test]
fn base_resolves_without_error() {
    let (g, report) = import(&base()).expect("BASE must resolve");
    assert!(
        report.is_empty(),
        "BASE should emit no diagnostics: {:?}",
        report.diagnostics
    );
    assert_eq!(g.blocks.len(), 2);
    assert_eq!(g.connections.len(), 1); // c1.y -> c2.u
}

#[test]
fn duplicate_id_is_rejected() {
    let mut doc = base();
    // Append a second node colliding with c1.k's @id.
    doc["@graph"].as_array_mut().unwrap().push(json!({
        "@id": "http://example.org#M.c1.k",
        "S231:value": { "@value": "9.0", "@type": "http://www.w3.org/2001/XMLSchema#double" }
    }));
    assert_error_code(&doc, DiagCode::DuplicateId);
}

#[test]
fn unresolved_connection_endpoint_is_rejected() {
    let mut doc = base();
    node_mut(&mut doc, "M.c1.y")["S231:isConnectedTo"] =
        json!({ "@id": "http://example.org#M.nope" });
    assert_error_code(&doc, DiagCode::UnresolvedReference);
}

#[test]
fn unresolved_datatype_is_rejected() {
    let mut doc = base();
    node_mut(&mut doc, "M.c2.u")["S231:isOfDataType"] = json!({ "@id": "S231:Frobnicate" });
    assert_error_code(&doc, DiagCode::UnresolvedReference);
}

#[test]
fn class_iri_without_hash_fragment_is_class_not_found_not_panic() {
    let mut doc = base();
    // A @type with no '#': the bridge must not panic; the (whole-string) candidate fails lookup.
    node_mut(&mut doc, "M.c1")["@type"] = json!("NotAnIri");
    assert_error_code(&doc, DiagCode::ClassNotFound);
}

#[test]
fn non_subset_class_is_class_not_found() {
    let mut doc = base();
    // CDL.Reals.PID strips cleanly but is NOT registered → ClassNotFound (exit #2 non-subset).
    node_mut(&mut doc, "M.c1")["@type"] =
        json!("http://example.org#Buildings.Controls.OBC.CDL.Reals.PID");
    assert_error_code(&doc, DiagCode::ClassNotFound);
}

#[test]
fn undriven_non_external_input_is_single_assignment() {
    let mut doc = base();
    // Drop the only edge → c2.u has in-degree 0 and is NOT an external boundary input.
    node_mut(&mut doc, "M.c1.y")
        .as_object_mut()
        .unwrap()
        .remove("S231:isConnectedTo");
    assert_error_code(&doc, DiagCode::SingleAssignment);
}

#[test]
fn doubly_driven_input_is_single_assignment() {
    let mut doc = base();
    // c1.y drives c2.u twice (an isConnectedTo array with a duplicate target) → in-degree 2.
    node_mut(&mut doc, "M.c1.y")["S231:isConnectedTo"] = json!([
        { "@id": "http://example.org#M.c2.u" },
        { "@id": "http://example.org#M.c2.u" }
    ]);
    assert_error_code(&doc, DiagCode::SingleAssignment);
}

#[test]
fn input_driving_output_is_direction_mismatch() {
    let mut doc = base();
    // Make the input c2.u a connection SOURCE to the output c2.y2 → In→Out, a direction violation.
    node_mut(&mut doc, "M.c2.u")["S231:isConnectedTo"] =
        json!({ "@id": "http://example.org#M.c2.y2" });
    assert_error_code(&doc, DiagCode::DirectionMismatch);
}

#[test]
fn self_loop_output_to_itself_is_direction_mismatch_no_hang() {
    let mut doc = base();
    // An output connected to itself: to-endpoint is an Out → DirectionMismatch, and must not hang.
    node_mut(&mut doc, "M.c2.y2")["S231:isConnectedTo"] =
        json!({ "@id": "http://example.org#M.c2.y2" });
    assert_error_code(&doc, DiagCode::DirectionMismatch);
}

#[test]
fn type_mismatched_connection_is_rejected() {
    let mut doc = base();
    // Retype c2.u as Boolean; the Real output c1.y → Boolean input c2.u is a type mismatch.
    let u = node_mut(&mut doc, "M.c2.u");
    u["@type"] = json!("S231:BooleanInput");
    u["S231:isOfDataType"] = json!({ "@id": "S231:Boolean" });
    assert_error_code(&doc, DiagCode::TypeMismatch);
}

#[test]
fn expr_referencing_unbound_param_is_grounding_failure() {
    let mut doc = base();
    // A bare-string S231:value is parsed as CxfValue::Expr; an unbound symbol fails to ground.
    node_mut(&mut doc, "M.c1.k")["S231:value"] = json!("undefined_param + 1");
    assert_error_code(&doc, DiagCode::GroundingFailed);
}

#[test]
fn integer_literal_overflow_is_grounding_failure_not_panic() {
    let mut doc = base();
    // The C1-lesson type-domain boundary: a lexical past i64 must be a typed diagnostic, not a panic.
    node_mut(&mut doc, "M.c1.k")["S231:value"] = json!({ "@value": "99999999999999999999999", "@type": "http://www.w3.org/2001/XMLSchema#integer" });
    assert_error_code(&doc, DiagCode::GroundingFailed);
}

#[test]
fn garbage_double_literal_is_grounding_failure() {
    let mut doc = base();
    node_mut(&mut doc, "M.c1.k")["S231:value"] =
        json!({ "@value": "not-a-number", "@type": "http://www.w3.org/2001/XMLSchema#double" });
    assert_error_code(&doc, DiagCode::GroundingFailed);
}

#[test]
fn empty_graph_is_malformed_document() {
    let doc = json!({ "@context": { "S231": "http://data.ashrae.org/S231P#" }, "@graph": [] });
    assert_error_code(&doc, DiagCode::MalformedDocument);
}

#[test]
fn invalid_json_is_a_json_error_not_a_panic() {
    let err = import_cxf(b"{ this is not json", &ResolveOptions::default()).unwrap_err();
    assert!(
        matches!(err, CxfError::Json(_)),
        "expected Json error, got {err:?}"
    );
}

#[test]
fn multi_type_array_classifies_on_first_without_panic() {
    let mut doc = base();
    // A @type that is a 2-element array: classification must use `.first()` and not panic.
    // First element is the valid Constant class, so this still resolves.
    node_mut(&mut doc, "M.c1")["@type"] = json!([
        "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
        "S231:DecorativeMarker"
    ]);
    let (g, _r) = import(&doc).expect("multi-@type with a valid first class resolves");
    assert_eq!(g.blocks[0].class_iri.as_ref(), "CDL.Reals.Sources.Constant");
}

#[test]
fn diagnostics_are_deterministically_ordered_across_imports() {
    // Two errors on different connectors + one type issue; the returned, sorted diagnostic vector
    // must be byte-identical across two imports (determinism rule §2.3, exit #4).
    let mut doc = base();
    // Undrive c2.u (SingleAssignment on C1) and add a second undriven Boolean input on a new block?
    // Simpler: drop the edge (c2.u undriven) AND retype c2.u Boolean is moot without an edge — use
    // two undriven inputs by adding a second input port to c2.
    node_mut(&mut doc, "M.c1.y")
        .as_object_mut()
        .unwrap()
        .remove("S231:isConnectedTo");
    node_mut(&mut doc, "M.c2")["S231:hasInput"] = json!([
        { "@id": "http://example.org#M.c2.u" },
        { "@id": "http://example.org#M.c2.u2" }
    ]);
    doc["@graph"].as_array_mut().unwrap().push(json!({
        "@id": "http://example.org#M.c2.u2", "@type": "S231:RealInput",
        "S231:isOfDataType": { "@id": "S231:Real" }
    }));

    let CxfError::Validation(d1) = import(&doc).unwrap_err() else {
        panic!("expected Validation");
    };
    let CxfError::Validation(d2) = import(&doc).unwrap_err() else {
        panic!("expected Validation");
    };
    assert_eq!(
        d1, d2,
        "diagnostic vectors must be byte-identical across imports (sorted order)"
    );
    // Both undriven inputs surface as SingleAssignment, ordered by ascending ConnectorId.
    let codes: Vec<DiagCode> = d1.iter().map(|d| d.code).collect();
    assert!(
        codes
            .iter()
            .filter(|c| **c == DiagCode::SingleAssignment)
            .count()
            >= 2
    );
}

#[test]
fn connector_shared_by_two_instances_is_malformed() {
    // C-1 regression: a connector IRI referenced by two instance ports would be silently wired into
    // two blocks. Add a third instance c3 that also claims c2.u as its input → MalformedDocument.
    let mut doc = base();
    node_mut(&mut doc, "#M")["S231:containsBlock"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "@id": "http://example.org#M.c3" }));
    let graph = doc["@graph"].as_array_mut().unwrap();
    graph.push(json!({
        "@id": "http://example.org#M.c3",
        "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
        "S231:hasInput": { "@id": "http://example.org#M.c2.u" },
        "S231:hasOutput": { "@id": "http://example.org#M.c3.y" }
    }));
    graph.push(json!({
        "@id": "http://example.org#M.c3.y", "@type": "S231:RealOutput",
        "S231:isOfDataType": { "@id": "S231:Real" }
    }));
    assert_error_code(&doc, DiagCode::MalformedDocument);
}

#[test]
fn boundary_input_driving_an_output_is_direction_mismatch() {
    // C-2 regression: a boundary INPUT may only drive a child INPUT; wiring it to an output (c1.y)
    // must NOT silently land an Out connector in external_inputs — it is a direction error.
    let mut doc = base();
    node_mut(&mut doc, "#M")["S231:hasInput"] = json!({ "@id": "http://example.org#M.uX" });
    doc["@graph"].as_array_mut().unwrap().push(json!({
        "@id": "http://example.org#M.uX", "@type": "S231:RealInput",
        "S231:isOfDataType": { "@id": "S231:Real" },
        "S231:isConnectedTo": { "@id": "http://example.org#M.c1.y" }
    }));
    assert_error_code(&doc, DiagCode::DirectionMismatch);
}

#[test]
fn analog_connector_coerces_to_real_with_warning() {
    // C-5: the resolver's only Warning path. An Analog connector (no isOfDataType) coerces to Real
    // and the load SUCCEEDS with an AnalogCoercedToReal warning recorded.
    let mut doc = base();
    let y = node_mut(&mut doc, "M.c1.y");
    y["@type"] = json!("S231:AnalogOutput");
    y.as_object_mut().unwrap().remove("S231:isOfDataType");
    let (g, report) = import(&doc).expect("Analog coercion is a warning, not an error");
    assert_eq!(g.connectors[0].value_type, oce_model::ValueType::Real);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == DiagCode::AnalogCoercedToReal && !d.is_error()),
        "expected an AnalogCoercedToReal warning, got {:?}",
        report.diagnostics
    );
}

#[test]
fn deny_warnings_turns_the_analog_warning_into_a_failure() {
    // C-5: deny_warnings (a documented but otherwise untested public option) fails the load on any
    // warning. (ResolveOptions is #[non_exhaustive], so set the field on a Default instance.)
    let mut doc = base();
    let y = node_mut(&mut doc, "M.c1.y");
    y["@type"] = json!("S231:AnalogOutput");
    y.as_object_mut().unwrap().remove("S231:isOfDataType");
    let mut opts = ResolveOptions::default();
    opts.deny_warnings = true;
    let err = import_cxf(&serde_json::to_vec(&doc).unwrap(), &opts).unwrap_err();
    assert!(
        matches!(err, CxfError::Validation(_)),
        "deny_warnings must fail on a warning: {err:?}"
    );
}

#[test]
fn block_interface_arity_mismatch_is_malformed() {
    // PR-6 review [CRITICAL]: a block declaring fewer/more ports than its class signature requires
    // must be rejected at RESOLVE — otherwise it loads and the engine's emit-by-port-index panics
    // on the first tick (index out of bounds). Remove c2's output (MultiplyByParameter requires
    // exactly one output).
    let mut doc = base();
    node_mut(&mut doc, "M.c2")
        .as_object_mut()
        .unwrap()
        .remove("S231:hasOutput");
    assert_error_code(&doc, DiagCode::MalformedDocument);
}

#[test]
fn multiple_top_composites_is_malformed_document() {
    // C-6: the resolver's only defense against two containsBlock roots. A regression that picked
    // composites[0] would silently flatten the wrong sub-model.
    let mut doc = base();
    doc["@graph"].as_array_mut().unwrap().push(json!({
        "@id": "http://example.org#M2", "@type": "S231:Block",
        "S231:containsBlock": [ { "@id": "http://example.org#M.c1" } ]
    }));
    assert_error_code(&doc, DiagCode::MalformedDocument);
}

#[test]
fn structural_diagnostics_sort_by_ascending_connector_id() {
    // C-4 regression: with ≥11 connectors, IRI-less structural diagnostics must sort by NUMERIC
    // ConnectorId, not lexicographically (where "connector#10" would precede "connector#2"). Six
    // MultiplyByParameter blocks each with an UNDRIVEN input → SingleAssignment on the even
    // connector ids C0,C2,C4,C6,C8,C10. A lexical sort would emit them as 0,10,2,4,6,8.
    let mut graph = vec![json!({
        "@id": "http://example.org#M", "@type": "S231:Block",
        "S231:containsBlock": (0..6)
            .map(|k| json!({ "@id": format!("http://example.org#M.b{k}") }))
            .collect::<Vec<_>>()
    })];
    for k in 0..6 {
        graph.push(json!({
            "@id": format!("http://example.org#M.b{k}"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
            "S231:hasInput": { "@id": format!("http://example.org#M.b{k}.u") },
            "S231:hasOutput": { "@id": format!("http://example.org#M.b{k}.y") }
        }));
        graph.push(json!({
            "@id": format!("http://example.org#M.b{k}.u"), "@type": "S231:RealInput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }));
        graph.push(json!({
            "@id": format!("http://example.org#M.b{k}.y"), "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }));
    }
    let doc = json!({ "@context": { "S231": "http://data.ashrae.org/S231P#" }, "@graph": graph });
    let CxfError::Validation(diags) = import(&doc).unwrap_err() else {
        panic!("expected Validation");
    };
    let ids: Vec<u32> = diags
        .iter()
        .filter(|d| d.code == DiagCode::SingleAssignment)
        .filter_map(|d| {
            d.subject
                .as_deref()?
                .strip_prefix("connector#")?
                .parse::<u32>()
                .ok()
        })
        .collect();
    assert_eq!(
        ids,
        vec![0, 2, 4, 6, 8, 10],
        "IRI-less structural diagnostics must sort by ascending ConnectorId, not lexically"
    );
}

#[test]
fn malformed_connector_bound_is_grounding_failure_not_silent_drop() {
    // M1-PR-11 (NO-SILENT-FAILURE): a PRESENT S231:min/max that fails to ground must surface a
    // diagnostic, never be silently dropped to "unset" (which would mask an R13.1 BoundMismatch the
    // §7.10 gate exists to catch). A garbage typed-literal double bound fails to ground.
    let mut doc = base();
    node_mut(&mut doc, "M.c1.y")["S231:min"] =
        json!({ "@value": "not-a-number", "@type": "http://www.w3.org/2001/XMLSchema#double" });
    let diags = assert_error_code(&doc, DiagCode::GroundingFailed);
    assert!(
        diags.iter().any(|d| d.code == DiagCode::GroundingFailed
            && d.message
                .contains("S231:min connector bound failed to ground")),
        "the malformed bound must surface its own GroundingFailed, got {diags:#?}"
    );
}

#[test]
fn symbolic_connector_bound_is_grounding_failure_not_silent_drop() {
    // A symbolic (expression) connector bound grounds against an EMPTY scope in M1 and so fails to
    // ground — it must report grounding-failed (symbolic connector bounds are M2), never vanish.
    let mut doc = base();
    node_mut(&mut doc, "M.c1.y")["S231:max"] = json!("someUnboundParam + 1");
    assert_error_code(&doc, DiagCode::GroundingFailed);
}

#[test]
fn attribute_on_non_numeric_connector_is_malformed_not_silent_drop() {
    // M1-PR-11 (NO-SILENT-FAILURE): a §7.4.1 attribute on a Boolean/String/Enum connector — which
    // CDL §7.4.1.3–.5 forbid — must surface, not be silently dropped as the default-attrs path did.
    // Retype c1.y Boolean and declare a unit on it; assert the specific "permits none" diagnostic
    // (other errors such as the resulting Boolean→Real TypeMismatch may also fire — we assert on the
    // exact diagnostic the non-numeric-attr path is responsible for).
    let mut doc = base();
    let y = node_mut(&mut doc, "M.c1.y");
    y["@type"] = json!("S231:BooleanOutput");
    y["S231:isOfDataType"] = json!({ "@id": "S231:Boolean" });
    y["S231:unit"] = json!("K");
    let diags = assert_error_code(&doc, DiagCode::MalformedDocument);
    assert!(
        diags
            .iter()
            .any(|d| d.code == DiagCode::MalformedDocument && d.message.contains("permits none")),
        "a unit on a Boolean connector must surface its own MalformedDocument, got {diags:#?}"
    );
}

#[test]
fn malformed_unit_shape_is_targeted_diagnostic_not_a_document_sink() {
    // M1-PR-11: a unit/quantity/displayUnit in a non-string/non-IRI shape (here a bare number) is
    // malformed per S231P, but must NOT sink the WHOLE document with a coarse CxfError::Json — it
    // deserializes into TermAttr::Other and the resolver surfaces a TARGETED MalformedDocument
    // carrying the connector IRI. Reaching `Validation` (not `Json`) is the proof it did not sink.
    let mut doc = base();
    node_mut(&mut doc, "M.c1.y")["S231:unit"] = json!(5);
    let diags = assert_error_code(&doc, DiagCode::MalformedDocument);
    assert!(
        diags.iter().any(|d| d.code == DiagCode::MalformedDocument
            && d.message.contains("malformed term")
            && d.subject.as_deref().is_some_and(|s| s.ends_with("c1.y"))),
        "a malformed unit shape must surface a per-connector MalformedDocument, got {diags:#?}"
    );
}

#[test]
fn partial_typed_literal_unit_is_targeted_diagnostic_not_a_document_sink() {
    // The other TermAttr::Other case: a partial object `{"@value":"K"}` missing `@type` matches
    // neither Typed nor Iri, so it must fall to Other and surface a targeted MalformedDocument
    // rather than a whole-document Json error.
    let mut doc = base();
    node_mut(&mut doc, "M.c1.y")["S231:displayUnit"] = json!({ "@value": "K" });
    let diags = assert_error_code(&doc, DiagCode::MalformedDocument);
    assert!(
        diags
            .iter()
            .any(|d| d.code == DiagCode::MalformedDocument && d.message.contains("malformed term")),
        "a partial typed-literal must surface a targeted MalformedDocument, got {diags:#?}"
    );
}

#[test]
fn real_bound_on_integer_connector_is_malformed() {
    // The Integer connector-bound path (`int_connector_bound`): a Real-valued bound on an Integer
    // connector is malformed. No M1 *block* has Integer ports, but a connector typed `S231:Integer`
    // still classifies as `ValueType::Integer`, so the path is reachable. Retype the model output
    // c2.y2 Integer and give it a Real (1.5) min — assert the Integer-bound diagnostic fires.
    let mut doc = base();
    let y = node_mut(&mut doc, "M.c2.y2");
    y["@type"] = json!("S231:IntegerOutput");
    y["S231:isOfDataType"] = json!({ "@id": "S231:Integer" });
    y["S231:min"] = json!(1.5);
    let diags = assert_error_code(&doc, DiagCode::MalformedDocument);
    assert!(
        diags.iter().any(|d| d.code == DiagCode::MalformedDocument
            && d.message.contains("did not ground to an integer")),
        "a Real bound on an Integer connector must surface its own MalformedDocument, got {diags:#?}"
    );
}

// ---- file-based rejection corpus (crates/oce-cxf/tests/fixtures/invalid/) -------------------

/// The `double_driven.jsonld` fixture (two distinct constant outputs both driving `add.u1`) is
/// rejected at ingest with a single-assignment error. This is the **file-based** complement to
/// `doubly_driven_input_is_single_assignment` above (which duplicates one edge); here two separate
/// drivers exercise the in-degree gate through the real `include_str!` byte path. The sibling
/// §7.10 fixtures (`unit_mismatch` / `one_sided_unit` / `display_unit_divergence` / `bound_mismatch`)
/// are now driven **end-to-end** through the full pipeline in `oce-api`'s `conformance.rs` (M1-PR-11
/// wired the resolver's §7.4.1 attribute extraction) — see `fixtures/invalid/README.md`.
#[test]
fn double_driven_fixture_is_rejected() {
    let doc: Value = serde_json::from_str(include_str!("fixtures/invalid/double_driven.jsonld"))
        .expect("fixture is valid JSON");
    assert_error_code(&doc, DiagCode::SingleAssignment);
}
