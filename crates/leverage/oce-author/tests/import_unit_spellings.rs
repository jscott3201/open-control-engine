//! The CXF importer reads a unit and a display unit under the spellings the engine reads them
//! under, and an IRI-node unit through the engine's closed table (issue #631; unit-spellings note
//! `docs/design/unit-spellings.md`, decision 6; owner ruling of 2026-11-12). These are the twins
//! of the engine's `resolve_unit_spellings.rs` and `resolve_unit_iris.rs`, and each probe is also
//! put to the engine through the facade, so the two readers are held to one verdict.
//!
//! - **Boundary:** `S231:unit`, `S231:hasUnit` and `qudt:hasUnit` go to `SemanticPoint.unit`, and
//!   `S231:displayUnit` and `S231:hasDisplayUnit` to `display_unit`. A string is the unit; an IRI
//!   node, `wrong-shape` before, is read through the table.
//! - **Block port:** every spelling is `port-attribute` (importer decision 8).
//! - **Parameter:** every spelling is shape-checked, dropped and reported (decision 9).
//! - **Everywhere:** two spellings of one property on one node are `two-spellings`, and
//!   `qudt:hasQuantityKind` is `unknown-key`.
//!
//! Where the two readers differ, the engine is the more lenient or both refuse with different
//! codes; each difference is pinned here by name.

use oce_api::{DiagCode, Engine};
use oce_author::{
    DroppedProperty, EmitOptions, ImportError, ImportErrorKind, ImportOptions, Imported, emit,
    import_cxf,
};
use serde_json::{Value as Json, json};

use crate::support::cxf::{identity, load_clean};
use crate::support::fixture;

const S231: &str = "http://data.ashrae.org/S231#";
const QUDT_HASH: &str = "http://qudt.org/schema/qudt#";
const QUDT_SLASH: &str = "http://qudt.org/schema/qudt/";
const UNIT_HASH: &str = "http://qudt.org/vocab/unit#";
const UNIT_SLASH: &str = "http://qudt.org/vocab/unit/";

/// The roles of the every-element sequence: a Real root input and output that carry no unit, a
/// Real block port, a sequence parameter and a block parameter.
const ROOT_INPUT: &str = "urn:oce:author:every_element.TSupSet";
const ROOT_OUTPUT: &str = "urn:oce:author:every_element.yGain1";
const PORT: &str = "urn:oce:author:every_element.conPID.u_m";
const SEQUENCE_PARAMETER: &str = "urn:oce:author:every_element.kGain";
const BLOCK_PARAMETER: &str = "urn:oce:author:every_element.conPID.yMin";
const BOUNDARIES: [&str; 2] = [ROOT_INPUT, ROOT_OUTPUT];
const PARAMETERS: [&str; 2] = [SEQUENCE_PARAMETER, BLOCK_PARAMETER];

/// `(key, whether it spells the display unit)`: every spelling of the two properties.
const KEYS: [(&str, bool); 5] = [
    ("S231:unit", false),
    ("S231:hasUnit", false),
    ("qudt:hasUnit", false),
    ("S231:displayUnit", true),
    ("S231:hasDisplayUnit", true),
];

/// The `@context` modelica-json writes, less the prefixes the importer does not admit.
fn upstream_context() -> Json {
    json!({ "S231": S231, "qudt": QUDT_HASH, "unit": UNIT_HASH })
}

/// The every-element golden under `context`.
fn document(context: Json) -> Json {
    let text = std::fs::read_to_string(fixture("emit/every_element.cxf.json")).expect("golden");
    let mut doc: Json = serde_json::from_str(&text).expect("golden parses");
    doc["@context"] = context;
    doc
}

fn upstream() -> Json {
    document(upstream_context())
}

/// `doc` with `key`, as written, set to `value` on the node `id`.
fn with(mut doc: Json, id: &str, key: &str, value: Json) -> Json {
    let node = doc["@graph"]
        .as_array_mut()
        .expect("@graph")
        .iter_mut()
        .find(|node| node["@id"] == id)
        .unwrap_or_else(|| panic!("no node `{id}`"));
    node[key] = value;
    doc
}

fn bytes(doc: &Json) -> Vec<u8> {
    serde_json::to_vec(doc).expect("serialize")
}

fn import(doc: &Json) -> Result<Imported, ImportError> {
    import_cxf(&bytes(doc), &ImportOptions::new())
}

fn imported(doc: &Json) -> Imported {
    import(doc).unwrap_or_else(|error| panic!("expected an import: {error}"))
}

/// The importer's refusal of `doc`: its subject and kind.
fn refusal(doc: &Json) -> (String, ImportErrorKind) {
    match import(doc) {
        Err(error) => (error.subject, error.kind),
        Ok(_) => panic!("expected a refusal, but it imported"),
    }
}

fn wrong_shape(subject: &str, key: &str) -> (String, ImportErrorKind) {
    (
        subject.to_owned(),
        ImportErrorKind::WrongShape {
            key: key.to_owned(),
        },
    )
}

fn unknown_key(subject: &str, key: &str) -> (String, ImportErrorKind) {
    (
        subject.to_owned(),
        ImportErrorKind::UnknownKey {
            key: key.to_owned(),
        },
    )
}

/// The engine's verdict on `doc`: `Ok` when it loads, else the `(code, subject)` of every error.
fn engine(doc: &Json) -> Result<Engine, Vec<(DiagCode, Option<String>)>> {
    let mut engine = Engine::new();
    match engine.load_cxf(&bytes(doc)) {
        Ok(_) => Ok(engine),
        Err(error) => Err(error
            .all_diagnostics()
            .filter(|d| d.is_error())
            .map(|d| (d.code, d.subject.as_deref().map(str::to_owned)))
            .collect()),
    }
}

/// The engine's one refusal of `doc`.
fn engine_refusal(doc: &Json) -> (DiagCode, Option<String>) {
    match engine(doc) {
        Err(mut errors) if errors.len() == 1 => errors.remove(0),
        Err(errors) => panic!("the engine refuses with {errors:?}"),
        Ok(_) => panic!("the engine loads it"),
    }
}

/// The `(unit, display unit)` the import of `doc` gives the boundary `id`.
fn boundary_units(doc: &Json, id: &str) -> (Option<String>, Option<String>) {
    let sequence = imported(doc).sequence;
    let local = id.rsplit('.').next().expect("a local name");
    let boundary = sequence
        .inputs
        .iter()
        .chain(&sequence.outputs)
        .find(|boundary| boundary.name.as_str() == local)
        .unwrap_or_else(|| panic!("no boundary `{local}`"));
    (
        boundary.semantic.unit.clone(),
        boundary.display_unit.clone(),
    )
}

fn reads(display: bool, unit: &str) -> (Option<String>, Option<String>) {
    if display {
        (None, Some(unit.to_owned()))
    } else {
        (Some(unit.to_owned()), None)
    }
}

/// The engine's own key for the property `key` spells.
fn own_key(display: bool) -> &'static str {
    if display {
        "S231:displayUnit"
    } else {
        "S231:unit"
    }
}

/// Both readers take `doc` for the document `same` in the engine's own spelling: the importer
/// emits the same bytes for both, and the engine gives both one content id, which is also the
/// content id of what the importer emits.
fn assert_one_reading(doc: &Json, same: &Json, label: &str) {
    let emitted = emit(&imported(doc).sequence, &EmitOptions::default()).expect("emits");
    let emitted_same = emit(&imported(same).sequence, &EmitOptions::default()).expect("emits");
    assert_eq!(emitted, emitted_same, "{label}: the importer's reading");
    let id = identity(&engine(doc).unwrap_or_else(|e| panic!("{label}: engine {e:?}")));
    let id_same = identity(&engine(same).unwrap_or_else(|e| panic!("{label}: engine {e:?}")));
    assert_eq!(id, id_same, "{label}: the engine's reading");
    assert_eq!(
        identity(&load_clean(&emitted)),
        id,
        "{label}: across readers"
    );
}

#[test]
fn the_golden_under_the_upstream_context_imports_and_loads_as_before() {
    let golden = document(json!({ "S231": S231 }));
    let before = imported(&golden);
    assert_eq!(imported(&upstream()).sequence, before.sequence);
    assert_eq!(
        identity(&engine(&upstream()).expect("the engine loads it")),
        identity(&engine(&golden).expect("the engine loads it"))
    );
    assert_eq!(boundary_units(&upstream(), ROOT_INPUT), (None, None));
    assert_eq!(boundary_units(&upstream(), ROOT_OUTPUT), (None, None));
}

#[test]
fn every_spelling_reads_a_boundarys_unit_as_a_string_or_an_iri_node() {
    for boundary in BOUNDARIES {
        for (key, display) in KEYS {
            let own = with(upstream(), boundary, own_key(display), json!("K"));
            for value in [json!("K"), json!({ "@id": "unit:K" })] {
                let doc = with(upstream(), boundary, key, value.clone());
                assert_eq!(
                    boundary_units(&doc, boundary),
                    reads(display, "K"),
                    "{boundary} {key} {value}"
                );
                assert_one_reading(&doc, &own, &format!("{boundary} {key} {value}"));
            }
        }
    }
}

#[test]
fn the_qudt_key_and_a_unit_iri_are_read_in_both_namespaces() {
    for schema in [QUDT_HASH, QUDT_SLASH] {
        for units in [UNIT_HASH, UNIT_SLASH] {
            let context = json!({ "S231": S231, "qudt": schema, "unit": units });
            let own = with(
                document(context.clone()),
                ROOT_INPUT,
                "S231:unit",
                json!("degC"),
            );
            for value in [
                json!({ "@id": "unit:DEG_C" }),
                json!({ "@id": format!("{UNIT_HASH}DEG_C") }),
                json!({ "@id": format!("{UNIT_SLASH}DEG_C") }),
            ] {
                let doc = with(document(context.clone()), ROOT_INPUT, "qudt:hasUnit", value);
                assert_eq!(
                    boundary_units(&doc, ROOT_INPUT),
                    reads(false, "degC"),
                    "{schema} {units}"
                );
                assert_one_reading(&doc, &own, &format!("{schema} {units}"));
            }
        }
    }
    // A full unit IRI needs no `unit` binding, and the S231 spellings no `qudt` one.
    let bare = json!({ "S231": S231 });
    let doc = with(
        document(bare.clone()),
        ROOT_OUTPUT,
        "S231:hasDisplayUnit",
        json!({ "@id": format!("{UNIT_SLASH}PA") }),
    );
    assert_eq!(boundary_units(&doc, ROOT_OUTPUT), reads(true, "Pa"));
    let own = with(document(bare), ROOT_OUTPUT, "S231:displayUnit", json!("Pa"));
    assert_one_reading(&doc, &own, "no unit binding");
}

#[test]
fn the_s231_fallback_reads_the_unit_string_after_the_namespace() {
    // `1` is a unit the emitter writes too, so the whole round trip is compared.
    for written in ["S231:1", "http://data.ashrae.org/S231#1"] {
        let doc = with(
            upstream(),
            ROOT_INPUT,
            "qudt:hasUnit",
            json!({ "@id": written }),
        );
        assert_eq!(
            boundary_units(&doc, ROOT_INPUT),
            reads(false, "1"),
            "{written}"
        );
        let own = with(upstream(), ROOT_INPUT, "S231:unit", json!("1"));
        assert_one_reading(&doc, &own, written);
    }
    // The fallbacks of the vendored corpus: the importer carries each as the engine reads it.
    for (written, unit) in [
        ("http://data.ashrae.org/S231#s/K", "s/K"),
        ("http://data.ashrae.org/S231#1/K", "1/K"),
        ("S231:1/s", "1/s"),
    ] {
        for (key, display) in KEYS {
            let doc = with(upstream(), ROOT_INPUT, key, json!({ "@id": written }));
            assert_eq!(
                boundary_units(&doc, ROOT_INPUT),
                reads(display, unit),
                "{written} {key}"
            );
            let own = with(upstream(), ROOT_INPUT, own_key(display), json!(unit));
            assert_eq!(
                identity(&engine(&doc).expect("the engine loads it")),
                identity(&engine(&own).expect("the engine loads it")),
                "{written} {key}"
            );
        }
    }
}

#[test]
fn an_unmapped_unit_iri_is_refused_by_both_readers() {
    let long = format!("{UNIT_HASH}{}", "K".repeat(1 << 20));
    let unmapped = [
        // A relative reference, an unbound prefix, a name the table lacks, another case.
        "degC",
        "K",
        "u:K",
        "unit:FOO",
        "unit:k",
        "unit:",
        "http://qudt.org/vocab/unit#KELVIN",
        "https://qudt.org/vocab/unit#K",
        "http://qudt.org/vocab/quantitykind#Pressure",
        "http://example.org/units#K",
        // The two entries left out of the table, in both namespaces.
        "unit:M3-PER-MIN",
        "unit:IN_H2O_60DEG_F",
        "http://qudt.org/vocab/unit/M3-PER-MIN",
        "http://qudt.org/vocab/unit/IN_H2O_60DEG_F",
        // A fallback with no unit, with whitespace, or that is no compact IRI.
        "S231:",
        "S231",
        "http://data.ashrae.org/S231#",
        "http://data.ashrae.org/S231# K",
        "S231:s K",
        "S231://K",
        "http://data.ashrae.org/S231P#s/K",
        long.as_str(),
    ];
    for id in unmapped {
        let quoted = &id[..id.len().min(40)];
        for (key, _) in KEYS {
            for node in BOUNDARIES.into_iter().chain(PARAMETERS) {
                let doc = with(upstream(), node, key, json!({ "@id": id }));
                let error = import(&doc).map(|_| ()).expect_err("the importer refuses");
                assert_eq!(
                    (error.subject.clone(), error.kind.clone()),
                    wrong_shape(node, key),
                    "{quoted} {key} {node}"
                );
                // The refusal names the key, never the megabyte.
                assert!(error.to_string().len() < 200, "{}", error.to_string().len());
                assert_eq!(
                    engine_refusal(&doc),
                    (DiagCode::NonSubsetConstruct, Some(node.to_owned())),
                    "{quoted} {key} {node}"
                );
            }
        }
    }
}

#[test]
fn base_is_inert_so_no_binding_of_it_turns_an_identifier_into_a_unit() {
    // `base` may be any string here, an IRI or not. Nothing is expanded through it, as a prefix
    // or as a whole token, so the importer refuses each of these.
    let cases = [
        // Bound to a string that ends in a zero-width space: the engine expands it, finds no
        // unit (the table reads no fallback carrying a character no reader can see) and refuses
        // the identifier for the character.
        (
            "http://data.ashrae.org/S231#K\u{200B}",
            "base",
            Some(DiagCode::MalformedDocument),
        ),
        (
            "http://data.ashrae.org/S231#",
            "base:K\u{200B}",
            Some(DiagCode::MalformedDocument),
        ),
        // Bound to a fragment that only a join makes an IRI: the engine refuses the binding.
        (
            "h",
            "base:ttp://qudt.org/vocab/unit#K",
            Some(DiagCode::NonSubsetConstruct),
        ),
        // Bound to a real unit namespace or a unit IRI: a standing difference, the engine the
        // more lenient. It reads any prefix; this reader takes `S231:`, `qudt:` and `unit:`.
        (UNIT_HASH, "base:K", None),
        ("http://qudt.org/vocab/unit#K", "base", None),
        ("http://data.ashrae.org/S231#", "base:1/s", None),
    ];
    for (base, id, engine_code) in cases {
        let mut context = upstream_context();
        context["base"] = json!(base);
        for (key, _) in KEYS {
            for node in BOUNDARIES.into_iter().chain(PARAMETERS) {
                let doc = with(document(context.clone()), node, key, json!({ "@id": id }));
                assert_eq!(
                    refusal(&doc),
                    wrong_shape(node, key),
                    "{base:?} {id:?} {key} {node}"
                );
                match engine_code {
                    Some(code) => {
                        assert_eq!(engine_refusal(&doc).0, code, "{base:?} {id:?} {key} {node}")
                    }
                    None => {
                        engine(&doc).unwrap_or_else(|e| panic!("{base:?} {id:?} {key}: {e:?}"));
                    }
                }
            }
        }
    }
}

#[test]
fn no_imported_unit_is_blank_or_carries_whitespace_or_an_invisible_character() {
    // Every identifier the importer maps, it maps to a string the engine's term rule accepts.
    let ids = [
        "unit:K",
        "S231:1/s",
        "http://data.ashrae.org/S231#s/K",
        "http://qudt.org/vocab/unit/M3-PER-SEC",
        "S231:µS/cm",
    ];
    for id in ids {
        let doc = with(upstream(), ROOT_INPUT, "qudt:hasUnit", json!({ "@id": id }));
        let unit = boundary_units(&doc, ROOT_INPUT).0.expect("a unit");
        assert!(!unit.trim().is_empty() && unit.trim() == unit, "{unit:?}");
        assert!(
            !unit
                .chars()
                .any(|c| c.is_whitespace() || oce_api::is_format_or_control(c)),
            "{unit:?}"
        );
    }
}

#[test]
fn a_malformed_unit_value_is_refused_by_both_readers_under_every_spelling() {
    let shapes = [
        json!(5),
        json!(true),
        json!([]),
        json!(["K"]),
        json!({}),
        json!(""),
        json!(" "),
        json!({ "@id": "" }),
        json!({ "@id": 5 }),
        json!({ "@id": "unit:K", "qudt:symbol": "K" }),
        json!({ "@id": "unit:K", "@value": "K" }),
        json!({ "@id": "unit:K\u{200B}" }),
        json!("K\u{200B}"),
    ];
    for value in &shapes {
        for (key, _) in KEYS {
            for node in BOUNDARIES.into_iter().chain(PARAMETERS) {
                let doc = with(upstream(), node, key, value.clone());
                assert_eq!(
                    refusal(&doc),
                    wrong_shape(node, key),
                    "{value} {key} {node}"
                );
                assert_eq!(
                    engine_refusal(&doc),
                    (DiagCode::MalformedDocument, Some(node.to_owned())),
                    "{value} {key} {node}"
                );
            }
        }
    }
}

#[test]
fn a_typed_literal_unit_is_the_one_shape_the_engine_reads_and_a_boundary_here_does_not() {
    // A standing difference, the engine the more lenient: a boundary's unit here is a string or
    // a unit IRI node. A parameter's may be a typed literal in both readers.
    let typed = json!({ "@value": "K", "@type": "http://www.w3.org/2001/XMLSchema#string" });
    for (key, _) in KEYS {
        for boundary in BOUNDARIES {
            let doc = with(upstream(), boundary, key, typed.clone());
            assert_eq!(refusal(&doc), wrong_shape(boundary, key), "{key}");
            engine(&doc).unwrap_or_else(|e| panic!("{key} {boundary}: {e:?}"));
        }
        for parameter in PARAMETERS {
            let doc = with(upstream(), parameter, key, typed.clone());
            imported(&doc);
            engine(&doc).unwrap_or_else(|e| panic!("{key} {parameter}: {e:?}"));
        }
    }
}

#[test]
fn every_spelling_on_a_parameter_is_checked_dropped_and_reported_as_written() {
    let original = imported(&upstream());
    for parameter in PARAMETERS {
        for (key, display) in KEYS {
            let property = if display {
                DroppedProperty::DisplayUnit
            } else {
                DroppedProperty::Unit
            };
            for value in [
                json!("K"),
                json!({ "@id": "unit:K" }),
                json!({ "@id": "S231:1/s" }),
                json!({ "@id": format!("{UNIT_SLASH}M3-PER-SEC") }),
            ] {
                let doc = with(upstream(), parameter, key, value.clone());
                let read = imported(&doc);
                assert_eq!(
                    read.sequence, original.sequence,
                    "{parameter} {key} {value}"
                );
                let written = match &value {
                    Json::String(text) => text.clone(),
                    other => other.to_string(),
                };
                let dropped: Vec<_> = read
                    .report
                    .dropped
                    .iter()
                    .filter(|dropped| dropped.subject == parameter)
                    .map(|dropped| (dropped.property, dropped.value.clone()))
                    .collect();
                assert_eq!(dropped, [(property, written)], "{parameter} {key} {value}");
                // The engine reads the same node and uses nothing of it.
                assert_eq!(
                    identity(&engine(&doc).expect("the engine loads it")),
                    identity(&engine(&upstream()).expect("the engine loads it")),
                    "{parameter} {key} {value}"
                );
            }
        }
    }
}

#[test]
fn every_spelling_on_a_block_port_is_a_port_attribute() {
    for (key, _) in KEYS {
        for value in [
            json!("K"),
            json!({ "@id": "unit:K" }),
            json!({ "@id": "unit:FOO" }),
        ] {
            let doc = with(upstream(), PORT, key, value.clone());
            assert_eq!(
                refusal(&doc),
                (
                    PORT.to_owned(),
                    ImportErrorKind::PortAttribute {
                        attribute: key.to_owned()
                    }
                ),
                "{key} {value}"
            );
        }
        // A standing difference (decision 8): the engine reads a unit on a block port.
        engine(&with(upstream(), PORT, key, json!({ "@id": "unit:K" })))
            .unwrap_or_else(|e| panic!("{key}: {e:?}"));
    }
    // Among several, the first in the fixed order is named, the QUDT spelling last.
    let both = with(
        with(upstream(), PORT, "qudt:hasUnit", json!("K")),
        PORT,
        "S231:hasDisplayUnit",
        json!("degC"),
    );
    assert_eq!(
        refusal(&both).1,
        ImportErrorKind::PortAttribute {
            attribute: "S231:hasDisplayUnit".to_owned()
        }
    );
}

#[test]
fn two_spellings_of_one_property_are_refused_by_both_readers_even_when_they_agree() {
    // `(property, first, second)`, in the order the importer reports them.
    let pairs = [
        ("S231:unit", "S231:unit", "S231:hasUnit"),
        ("S231:unit", "S231:unit", "qudt:hasUnit"),
        ("S231:unit", "S231:hasUnit", "qudt:hasUnit"),
        (
            "S231:displayUnit",
            "S231:displayUnit",
            "S231:hasDisplayUnit",
        ),
    ];
    let values = [
        (json!("K"), json!("K")),
        (json!("K"), json!({ "@id": "unit:K" })),
        (json!({ "@id": "unit:K" }), json!({ "@id": "unit:K" })),
        (json!("K"), json!("degC")),
    ];
    for (property, first, second) in pairs {
        for (one, other) in &values {
            for node in BOUNDARIES.into_iter().chain(PARAMETERS) {
                let doc = with(
                    with(upstream(), node, first, one.clone()),
                    node,
                    second,
                    other.clone(),
                );
                assert_eq!(
                    refusal(&doc),
                    (
                        node.to_owned(),
                        ImportErrorKind::TwoSpellings {
                            property,
                            first: first.to_owned(),
                            second: second.to_owned(),
                        }
                    ),
                    "{node} {first} {second}"
                );
                assert_eq!(
                    engine_refusal(&doc),
                    (DiagCode::MalformedDocument, Some(node.to_owned())),
                    "{node} {first} {second}"
                );
            }
        }
    }
    let error = import(&with(
        with(upstream(), ROOT_INPUT, "S231:unit", json!("K")),
        ROOT_INPUT,
        "qudt:hasUnit",
        json!({ "@id": "unit:K" }),
    ))
    .map(|_| ())
    .expect_err("refused");
    assert_eq!(error.code(), "two-spellings");
    assert_eq!(
        error.to_string(),
        format!(
            "CXF import refused `{ROOT_INPUT}` (two-spellings): `S231:unit` is written twice, as \
             `S231:unit` and as `qudt:hasUnit`; a property has exactly one spelling on a node"
        )
    );
    // A unit beside a display unit is two properties, the form the vendored translations write.
    let both = with(
        with(
            upstream(),
            ROOT_INPUT,
            "qudt:hasUnit",
            json!({ "@id": "unit:K" }),
        ),
        ROOT_INPUT,
        "S231:hasDisplayUnit",
        json!({ "@id": "unit:DEG_C" }),
    );
    assert_eq!(
        boundary_units(&both, ROOT_INPUT),
        (Some("K".to_owned()), Some("degC".to_owned()))
    );
}

#[test]
fn qudt_has_quantity_kind_is_an_unknown_key_everywhere() {
    let key = "qudt:hasQuantityKind";
    for context in [upstream_context(), json!({ "S231": S231 })] {
        for node in BOUNDARIES.into_iter().chain(PARAMETERS).chain([PORT]) {
            for value in [json!({ "@id": "q:Pressure" }), json!("Pressure")] {
                let doc = with(document(context.clone()), node, key, value);
                assert_eq!(refusal(&doc), unknown_key(node, key), "{node}");
                // A standing difference: the engine leaves the key unread and loads.
                engine(&doc).unwrap_or_else(|e| panic!("{node}: {e:?}"));
            }
        }
    }
}

#[test]
fn the_qudt_key_without_a_qudt_binding_is_an_unknown_key_on_every_node() {
    // Unbound, `qudt:hasUnit` is not the QUDT property: no unit here, and on a port no port
    // attribute. The engine leaves the key unread and loads the document without the unit.
    let context = json!({ "S231": S231, "unit": UNIT_HASH });
    for node in BOUNDARIES.into_iter().chain(PARAMETERS).chain([PORT]) {
        let doc = with(
            document(context.clone()),
            node,
            "qudt:hasUnit",
            json!({ "@id": "unit:K" }),
        );
        assert_eq!(refusal(&doc), unknown_key(node, "qudt:hasUnit"), "{node}");
        assert_eq!(
            identity(&engine(&doc).expect("the engine loads it")),
            identity(&engine(&document(context.clone())).expect("the engine loads it")),
            "{node}"
        );
    }
}

#[test]
fn a_qudt_or_unit_prefix_bound_to_another_iri_is_refused_with_the_document() {
    for (term, iri) in [
        ("unit", "http://example.org/unit#"),
        ("unit", "https://qudt.org/vocab/unit#"),
        ("unit", QUDT_HASH),
        ("qudt", "http://example.org/qudt#"),
        ("qudt", "http://qudt.org/schema/qudt"),
        ("qudt", UNIT_HASH),
    ] {
        let mut context = upstream_context();
        context[term] = json!(iri);
        let doc = with(
            document(context),
            ROOT_INPUT,
            "qudt:hasUnit",
            json!({ "@id": "unit:K" }),
        );
        assert_eq!(refusal(&doc), unknown_key("document", term), "{term} {iri}");
        // The engine does not read the binding as QUDT either: bound elsewhere, the `unit:`
        // identifier is unmapped and refused, and the `qudt:` key is some other property, left
        // unread.
        match term {
            "unit" => assert_eq!(
                engine_refusal(&doc),
                (DiagCode::NonSubsetConstruct, Some(ROOT_INPUT.to_owned())),
                "{iri}"
            ),
            _ => assert_eq!(
                identity(&engine(&doc).expect("the engine loads it")),
                identity(&engine(&upstream()).expect("the engine loads it")),
                "{iri}"
            ),
        }
    }
}

#[test]
fn a_near_miss_of_a_spelling_is_refused_by_both_readers() {
    for key in [
        "qudt:hasUnit ",
        " qudt:hasUnit",
        "S231:hasUnit\t",
        " S231:hasDisplayUnit",
    ] {
        for node in BOUNDARIES.into_iter().chain(PARAMETERS) {
            let doc = with(upstream(), node, key, json!("K"));
            assert_eq!(refusal(&doc), unknown_key(node, key), "{key:?} {node}");
            assert_eq!(
                engine_refusal(&doc),
                (DiagCode::MalformedDocument, Some(node.to_owned())),
                "{key:?} {node}"
            );
        }
    }
    // A full-IRI key stays outside the importer's subset: compact keys only.
    for key in [
        format!("{QUDT_HASH}hasUnit"),
        format!("{S231}hasUnit"),
        format!("{S231}hasDisplayUnit"),
    ] {
        let doc = with(upstream(), ROOT_INPUT, &key, json!("K"));
        assert_eq!(refusal(&doc), unknown_key(ROOT_INPUT, &key), "{key}");
        engine(&doc).unwrap_or_else(|e| panic!("{key}: {e:?}"));
    }
}

#[test]
fn a_units_quantity_stays_a_string_on_a_boundary_and_any_term_on_a_parameter() {
    // `S231:quantity` is read as before: no table.
    let node = json!({ "@id": "unit:K" });
    for boundary in BOUNDARIES {
        assert_eq!(
            refusal(&with(upstream(), boundary, "S231:quantity", node.clone())),
            wrong_shape(boundary, "S231:quantity")
        );
    }
    for parameter in PARAMETERS {
        imported(&with(upstream(), parameter, "S231:quantity", node.clone()));
    }
}

#[test]
fn an_import_in_the_upstream_spellings_is_the_same_every_time() {
    let doc = with(
        with(
            upstream(),
            ROOT_INPUT,
            "qudt:hasUnit",
            json!({ "@id": "unit:K" }),
        ),
        ROOT_OUTPUT,
        "S231:hasDisplayUnit",
        json!({ "@id": "unit:DEG_C" }),
    );
    let first = imported(&doc);
    for _ in 0..8 {
        assert_eq!(imported(&doc), first);
    }
}
