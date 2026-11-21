//! A `unit` or `displayUnit` written as an IRI node is read through the closed unit table (issue
//! #631; design note `docs/design/unit-spellings.md`, decisions 2 and 8; owner ruling of
//! 2026-11-12). A term's shape decides how it is read, not its key: a string or typed literal is
//! the unit string, verbatim; an IRI node, under any key that names the property and the engine's
//! own `S231:unit` included, is expanded against the document `@context` and looked up. An IRI
//! the table does not map is refused as `non-subset-construct`, subject the node, the IRI quoted
//! in at most 64 bytes.
//!
//! Before, an IRI node's `@id` was the term, verbatim: `{"@id": "unit:K"}` was the unit `unit:K`,
//! which equals no unit string, and `{"@id": "degC"}` loaded as the unit `degC`.
//!
//! Every refusal is pinned as the complete diagnostic list, stated by hand. Which keys name the
//! property is `resolve_unit_spellings.rs`'s.

use oce_cxf::{QUDT_UNITS, export, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value as Json, json};

use crate::unit_documents::{
    CONNECTOR_ROLES, INTEGER_OUTPUT, NO_TERMS, ORPHAN, PARAMETER_ROLES, PORT, ROLES, ROOT_INPUT,
    ROOT_OUTPUT, S231, UNIT_HASH, UNIT_SLASH, display_only, document, loaded, refusal, rendered,
    terms, unit_only, upstream, upstream_context, with,
};

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// `(key, the modeled term it names)`: every spelling of the two properties read through the
/// table, under [`upstream_context`].
const KEYS: [(&str, &str); 5] = [
    ("S231:unit", "unit"),
    ("S231:hasUnit", "unit"),
    ("qudt:hasUnit", "unit"),
    ("S231:displayUnit", "displayUnit"),
    ("S231:hasDisplayUnit", "displayUnit"),
];

/// What the role `id` declares when `term` alone reads `unit`.
fn declares(term: &str, unit: &str) -> (Option<String>, Option<String>, Option<String>) {
    if term == "unit" {
        unit_only(unit)
    } else {
        display_only(unit)
    }
}

/// The refusal of the unmapped IRI `iri` (as the message quotes it) for `term` on `node`.
fn unmapped(node: &str, term: &str, iri: &str) -> Diagnostic {
    let on = if PARAMETER_ROLES.contains(&node) {
        " on a parameter"
    } else {
        ""
    };
    Diagnostic::error(
        DiagCode::NonSubsetConstruct,
        format!(
            "S231:{term}{on} is the IRI node `{iri}`, which the closed unit table does not map; a \
             unit written as an IRI node is read only as a QUDT unit the table lists \
             (`http://qudt.org/vocab/unit#K`) or as an S231-namespace unit string, so write one \
             of those or the unit as a string (`\"K\"`)"
        ),
    )
    .with_subject(node)
}

#[test]
fn every_table_entry_reads_as_its_unit_string_in_both_unit_namespaces() {
    assert_eq!(QUDT_UNITS.len(), 25);
    for (local, unit) in QUDT_UNITS {
        for namespace in [UNIT_HASH, UNIT_SLASH] {
            let value = json!({ "@id": format!("{namespace}{local}") });
            for (key, term) in KEYS {
                let graph = loaded(&with(upstream(), PORT, key, value.clone()));
                assert_eq!(
                    terms(&graph, PORT),
                    declares(term, unit),
                    "{namespace}{local} {key}"
                );
            }
        }
    }
}

#[test]
fn an_iri_node_reads_through_the_table_on_every_role_under_every_key() {
    let unset = rendered(&upstream());
    for (key, term) in KEYS {
        for value in [
            json!({ "@id": "unit:M3-PER-SEC" }),
            json!({ "@id": format!("{UNIT_HASH}M3-PER-SEC") }),
            json!({ "@id": format!("{UNIT_SLASH}M3-PER-SEC") }),
        ] {
            for role in CONNECTOR_ROLES {
                let graph = loaded(&with(upstream(), role, key, value.clone()));
                assert_eq!(
                    terms(&graph, role),
                    declares(term, "m3/s"),
                    "{role} {key} {value}"
                );
            }
            for role in PARAMETER_ROLES {
                assert_eq!(
                    rendered(&with(upstream(), role, key, value.clone())),
                    unset,
                    "{role} {key} {value}"
                );
            }
        }
    }
}

#[test]
fn a_compact_iri_reads_through_whatever_prefix_binds_a_unit_namespace() {
    for namespace in [UNIT_HASH, UNIT_SLASH] {
        for prefix in ["unit", "u", "qudtunit"] {
            let mut context = upstream_context();
            context.as_object_mut().expect("context").remove("unit");
            context[prefix] = json!(namespace);
            for role in CONNECTOR_ROLES {
                let doc = with(
                    document(context.clone()),
                    role,
                    "qudt:hasUnit",
                    json!({ "@id": format!("{prefix}:DEG_C") }),
                );
                assert_eq!(
                    terms(&loaded(&doc), role),
                    unit_only("degC"),
                    "{namespace} {prefix} {role}"
                );
            }
        }
    }
    // A whole-token `@context` term is an IRI too, as for every expanded token.
    let mut context = upstream_context();
    context["kelvin"] = json!(format!("{UNIT_SLASH}K"));
    let doc = with(
        document(context),
        PORT,
        "S231:unit",
        json!({ "@id": "kelvin" }),
    );
    assert_eq!(terms(&loaded(&doc), PORT), unit_only("K"));
}

#[test]
fn a_unit_prefix_bound_to_another_iri_is_not_read_as_qudt() {
    for elsewhere in [
        "http://example.org/unit#",
        "https://qudt.org/vocab/unit#",
        "http://qudt.org/vocab/unit",
        "http://qudt.org/vocab/quantitykind#",
        "http://qudt.org/schema/qudt#",
    ] {
        let mut context = upstream_context();
        context["unit"] = json!(elsewhere);
        for (key, term) in KEYS {
            for role in ROLES {
                let doc = with(
                    document(context.clone()),
                    role,
                    key,
                    json!({ "@id": "unit:K" }),
                );
                assert_eq!(
                    refusal(&doc),
                    vec![unmapped(role, term, &format!("{elsewhere}K"))],
                    "{elsewhere} {key} {role}"
                );
            }
        }
    }
}

#[test]
fn an_unmapped_iri_is_refused_on_every_role_under_every_key() {
    // `(the @id as written, the IRI the message quotes)`: the expanded IRI where the token
    // expands, the token itself where it does not.
    let unit_foo = format!("{UNIT_HASH}FOO");
    let cases = [
        // A relative reference: what `connector_attrs.jsonld` wrote before this change.
        ("degC", "degC"),
        ("K", "K"),
        // A local name the table lacks, a different case, and a longer or shorter name.
        ("unit:FOO", unit_foo.as_str()),
        (
            "http://qudt.org/vocab/unit#k",
            "http://qudt.org/vocab/unit#k",
        ),
        (
            "http://qudt.org/vocab/unit/Pa",
            "http://qudt.org/vocab/unit/Pa",
        ),
        (
            "http://qudt.org/vocab/unit#KELVIN",
            "http://qudt.org/vocab/unit#KELVIN",
        ),
        ("http://qudt.org/vocab/unit#", "http://qudt.org/vocab/unit#"),
        // Another namespace, one character from a read one among them.
        (
            "https://qudt.org/vocab/unit#K",
            "https://qudt.org/vocab/unit#K",
        ),
        (
            "http://qudt.org/vocab/unit%23K",
            "http://qudt.org/vocab/unit%23K",
        ),
        (
            "http://qudt.org/vocab/quantitykind#Pressure",
            "http://qudt.org/vocab/quantitykind#Pressure",
        ),
        ("http://example.org/units#K", "http://example.org/units#K"),
        ("urn:unit:K", "urn:unit:K"),
        // A prefix the document does not bind.
        ("u:K", "u:K"),
        ("q:Pressure", "q:Pressure"),
    ];
    for (written, quoted) in cases {
        for (key, term) in KEYS {
            for role in ROLES {
                assert_eq!(
                    refusal(&with(upstream(), role, key, json!({ "@id": written }))),
                    vec![unmapped(role, term, quoted)],
                    "{written} {key} {role}"
                );
            }
        }
    }
}

#[test]
fn an_unbound_unit_prefix_is_not_read_as_qudt() {
    let context = json!({ "S231": S231 });
    for (key, term) in [
        ("S231:unit", "unit"),
        ("S231:hasDisplayUnit", "displayUnit"),
    ] {
        for role in ROLES {
            assert_eq!(
                refusal(&with(
                    document(context.clone()),
                    role,
                    key,
                    json!({ "@id": "unit:K" })
                )),
                vec![unmapped(role, term, "unit:K")],
                "{key} {role}"
            );
        }
    }
}

#[test]
fn the_two_entries_left_out_of_the_table_are_refused() {
    // modelica-json writes `unit:M3-PER-MIN` for `cfm` and `unit:IN_H2O_60DEG_F` for `inH2O`;
    // each QUDT identifier names another unit, so neither is read back.
    for local in ["M3-PER-MIN", "IN_H2O_60DEG_F"] {
        for namespace in [UNIT_HASH, UNIT_SLASH] {
            let iri = format!("{namespace}{local}");
            for (key, term) in KEYS {
                for role in ROLES {
                    assert_eq!(
                        refusal(&with(upstream(), role, key, json!({ "@id": iri }))),
                        vec![unmapped(role, term, &iri)],
                        "{iri} {key} {role}"
                    );
                }
            }
        }
        let compact = json!({ "@id": format!("unit:{local}") });
        assert_eq!(
            refusal(&with(upstream(), PORT, "qudt:hasUnit", compact)),
            vec![unmapped(PORT, "unit", &format!("{UNIT_HASH}{local}"))]
        );
    }
}

#[test]
fn a_megabyte_long_iri_is_quoted_in_at_most_64_bytes() {
    let long = format!("{UNIT_HASH}{}", "K".repeat(1 << 20));
    let quoted = format!("{}…", &long[..64]);
    for (key, term) in KEYS {
        for role in ROLES {
            let diags = refusal(&with(upstream(), role, key, json!({ "@id": long })));
            assert_eq!(diags, vec![unmapped(role, term, &quoted)], "{key} {role}");
            assert!(diags[0].message.len() < 400, "{}", diags[0].message.len());
        }
    }
    // A multi-byte character straddling the limit is left out whole, never split.
    let straddling = format!("{UNIT_HASH}{}é", "K".repeat(63 - UNIT_HASH.len()));
    let diags = refusal(&with(
        upstream(),
        PORT,
        "S231:unit",
        json!({ "@id": format!("{straddling}{}", "K".repeat(100)) }),
    ));
    let cut = format!("{UNIT_HASH}{}…", "K".repeat(63 - UNIT_HASH.len()));
    assert_eq!(diags, vec![unmapped(PORT, "unit", &cut)]);
}

#[test]
fn the_s231_fallback_reads_the_unit_string_after_the_namespace() {
    // The three fallbacks of the vendored corpus (`s/K`, `1/K`, `1/s`), a compact one, and a
    // non-ASCII one.
    for (written, unit) in [
        ("http://data.ashrae.org/S231#s/K", "s/K"),
        ("http://data.ashrae.org/S231#1/K", "1/K"),
        ("http://data.ashrae.org/S231#1/s", "1/s"),
        ("S231:1/s", "1/s"),
        ("S231:ppm", "ppm"),
        ("http://data.ashrae.org/S231#µS/cm", "µS/cm"),
    ] {
        for (key, term) in KEYS {
            for role in CONNECTOR_ROLES {
                let graph = loaded(&with(upstream(), role, key, json!({ "@id": written })));
                assert_eq!(
                    terms(&graph, role),
                    declares(term, unit),
                    "{written} {key} {role}"
                );
            }
            for role in PARAMETER_ROLES {
                loaded(&with(upstream(), role, key, json!({ "@id": written })));
            }
        }
    }
}

#[test]
fn a_fallback_with_no_unit_or_with_whitespace_is_refused() {
    for written in [
        "http://data.ashrae.org/S231#",
        "S231",
        "http://data.ashrae.org/S231# K",
        "http://data.ashrae.org/S231#s /K",
        "http://data.ashrae.org/S231#K\u{a0}",
        "S231:s K",
        // A fallback in a namespace the engine does not read the vocabulary in.
        "http://data.ashrae.org/S231P#s/K",
        "https://data.ashrae.org/S231#s/K",
        // `//` after the first colon is never a compact IRI.
        "S231://K",
    ] {
        // The whole-token term `S231` expands to the namespace itself.
        let quoted = if written == "S231" { S231 } else { written };
        for (key, term) in KEYS {
            for role in ROLES {
                assert_eq!(
                    refusal(&with(upstream(), role, key, json!({ "@id": written }))),
                    vec![unmapped(role, term, quoted)],
                    "{written:?} {key} {role}"
                );
            }
        }
    }
    // With `S231` unbound, a compact fallback has no IRI.
    let context = json!({ "unit": UNIT_HASH });
    assert_eq!(
        refusal(&with(
            document(context),
            PORT,
            "S231:unit",
            json!({ "@id": "S231:1/s" })
        )),
        vec![unmapped(PORT, "unit", "S231:1/s")]
    );
}

#[test]
fn a_string_or_typed_literal_is_the_unit_string_verbatim_under_every_key() {
    // The shape decides: text that looks like an IRI is still text when it is not an IRI node.
    for text in [
        "K",
        "unit:K",
        "http://qudt.org/vocab/unit#K",
        "degC",
        "S231:1/s",
        "m3/s",
    ] {
        for value in [json!(text), json!({ "@value": text, "@type": XSD_STRING })] {
            for (key, term) in KEYS {
                for role in CONNECTOR_ROLES {
                    let graph = loaded(&with(upstream(), role, key, value.clone()));
                    assert_eq!(
                        terms(&graph, role),
                        declares(term, text),
                        "{text} {key} {role}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_quantity_iri_node_stays_its_verbatim_identifier() {
    // `S231:quantity` is read as before: no table, no expansion, bound prefix or not.
    for written in [
        "unit:K",
        "q:Pressure",
        "degC",
        "http://qudt.org/vocab/unit#K",
        "S231:1/s",
    ] {
        for role in CONNECTOR_ROLES {
            let graph = loaded(&with(
                upstream(),
                role,
                "S231:quantity",
                json!({ "@id": written }),
            ));
            assert_eq!(
                terms(&graph, role),
                (None, Some(written.to_owned()), None),
                "{written} {role}"
            );
        }
        for role in PARAMETER_ROLES {
            loaded(&with(
                upstream(),
                role,
                "S231:quantity",
                json!({ "@id": written }),
            ));
        }
    }
}

#[test]
fn a_malformed_iri_node_is_refused_for_its_shape_alone() {
    // The shape rule comes first and a term is refused once: an IRI node that is malformed and
    // unmapped is `malformed-document`, with no second refusal.
    let malformed = |node: &str, term: &str, found: &str| {
        let on = if PARAMETER_ROLES.contains(&node) {
            " on a parameter"
        } else {
            ""
        };
        Diagnostic::error(
            DiagCode::MalformedDocument,
            format!("S231:{term}{on} is {found} — malformed term"),
        )
        .with_subject(node)
    };
    let cases = [
        (
            json!({ "@id": "unit:FOO", "qudt:symbol": "K" }),
            "an IRI node carrying the key `qudt:symbol` beside its @id".to_owned(),
        ),
        (
            json!({ "@id": "unit:K", "@value": "K" }),
            "an object carrying both @id and @value".to_owned(),
        ),
        (
            json!({ "@id": "" }),
            "an IRI node whose @id is empty or whitespace-only".to_owned(),
        ),
        (
            json!({ "@id": "unit:K\u{200B}" }),
            "an IRI node whose @id `unit:K\\u{200B}` carries U+200B, a format character (Cf) \
             that no reader can see"
                .to_owned(),
        ),
    ];
    for (value, found) in cases {
        for (key, term) in KEYS {
            for role in ROLES {
                assert_eq!(
                    refusal(&with(upstream(), role, key, value.clone())),
                    vec![malformed(role, term, &found)],
                    "{value} {key} {role}"
                );
            }
        }
    }
}

#[test]
fn a_context_term_cannot_launder_a_blank_or_invisible_identifier_into_a_unit() {
    // A whole-token `@context` term wins over every other reading of a token. A term spelled as
    // whitespace, or carrying a zero-width space, bound to a mapped IRI must not turn an `@id`
    // no reader can see into the unit `K`: the shape rule judges the `@id` as written.
    let mut context = upstream_context();
    context[" "] = json!(format!("{UNIT_HASH}K"));
    context["K\u{200B}"] = json!(format!("{UNIT_HASH}K"));
    for (key, term) in KEYS {
        assert_eq!(
            refusal(&with(
                document(context.clone()),
                PORT,
                key,
                json!({ "@id": " " })
            )),
            vec![
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!(
                        "S231:{term} is an IRI node whose @id is empty or whitespace-only — \
                         malformed term"
                    ),
                )
                .with_subject(PORT)
            ],
            "{key}"
        );
        assert_eq!(
            refusal(&with(
                document(context.clone()),
                PORT,
                key,
                json!({ "@id": "K\u{200B}" })
            )),
            vec![
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!(
                        "S231:{term} is an IRI node whose @id `K\\u{{200B}}` carries U+200B, a \
                         format character (Cf) that no reader can see — malformed term"
                    ),
                )
                .with_subject(PORT)
            ],
            "{key}"
        );
    }
}

#[test]
fn an_iri_node_on_an_integer_connector_is_refused_once_as_a_term_there() {
    for value in [
        json!({ "@id": "unit:UNITLESS" }),
        json!({ "@id": "unit:FOO" }),
    ] {
        assert_eq!(
            refusal(&with(upstream(), INTEGER_OUTPUT, "qudt:hasUnit", value)),
            vec![
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    "§7.4.1 term (unit/quantity/displayUnit) declared on an Integer connector, \
                     which permits only min and max",
                )
                .with_subject(INTEGER_OUTPUT)
            ]
        );
    }
}

#[test]
fn an_unmapped_iri_on_a_node_nothing_reads_is_not_refused() {
    // The rule runs where the node is read, as every term rule does.
    let unset = rendered(&upstream());
    for (key, _) in KEYS {
        assert_eq!(
            rendered(&with(upstream(), ORPHAN, key, json!({ "@id": "unit:FOO" }))),
            unset,
            "{key}"
        );
    }
}

#[test]
fn every_unmapped_iri_in_a_document_is_reported_once_in_a_stable_order() {
    let mut doc = upstream();
    for (role, key) in [
        (ROOT_INPUT, "qudt:hasUnit"),
        (ROOT_OUTPUT, "S231:hasDisplayUnit"),
        (PORT, "S231:unit"),
        (PARAMETER_ROLES[0], "S231:hasUnit"),
        (PARAMETER_ROLES[1], "S231:displayUnit"),
    ] {
        doc = with(doc, role, key, json!({ "@id": "unit:FOO" }));
    }
    let foo = format!("{UNIT_HASH}FOO");
    let mut expected = vec![
        unmapped(ROOT_INPUT, "unit", &foo),
        unmapped(ROOT_OUTPUT, "displayUnit", &foo),
        unmapped(PORT, "unit", &foo),
        unmapped(PARAMETER_ROLES[0], "unit", &foo),
        unmapped(PARAMETER_ROLES[1], "displayUnit", &foo),
    ];
    let by_subject = |a: &Diagnostic, b: &Diagnostic| a.subject.cmp(&b.subject);
    expected.sort_by(by_subject);
    let first = refusal(&doc);
    let mut sorted = first.clone();
    sorted.sort_by(by_subject);
    assert_eq!(sorted, expected, "the same set, each once");
    let mut reversed = doc.clone();
    reversed["@graph"].as_array_mut().expect("@graph").reverse();
    for _ in 0..8 {
        assert_eq!(refusal(&doc), first, "the same order every time");
    }
    let mut reversed_sorted = refusal(&reversed);
    reversed_sorted.sort_by(by_subject);
    assert_eq!(reversed_sorted, expected);
}

#[test]
fn an_upstream_document_exports_the_s231_spelling_and_reimports_to_the_same_graph() {
    // Decision 3: the exporter is unchanged. A unit read from `qudt:hasUnit {"@id": "unit:K"}`
    // is written as the bare string `"S231:unit": "K"`, where before no unit was written; a
    // round trip normalises the spelling, and re-import reads the same string (RT-2).
    let doc = with(
        with(upstream(), PORT, "qudt:hasUnit", json!({ "@id": "unit:K" })),
        PORT,
        "S231:hasDisplayUnit",
        json!({ "@id": "unit:DEG_C" }),
    );
    let graph = loaded(&doc);
    assert_eq!(
        terms(&graph, PORT),
        (Some("K".to_owned()), None, Some("degC".to_owned()))
    );
    let bytes = export(&graph).expect("the graph exports");
    let exported: Json = serde_json::from_slice(&bytes).expect("export is JSON");
    let nodes = exported["@graph"].as_array().expect("@graph");
    let port = nodes
        .iter()
        .find(|node| node["@id"] == PORT)
        .expect("the port is exported under its authored @id");
    assert_eq!(port["S231:unit"], json!("K"));
    assert_eq!(port["S231:displayUnit"], json!("degC"));
    let text = String::from_utf8(bytes.clone()).expect("UTF-8");
    for absent in ["qudt:", "hasUnit", "hasDisplayUnit", "unit:K", "vocab/unit"] {
        assert!(!text.contains(absent), "the export writes `{absent}`");
    }
    let (again, report) =
        import_cxf(&bytes, &oce_cxf::ResolveOptions::default()).expect("the export re-imports");
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(terms(&again, PORT), terms(&graph, PORT));
    assert_eq!(
        export(&again).expect("exports again"),
        bytes,
        "RT-2 fixpoint"
    );
    // The same document in the engine's own spelling exports the same bytes.
    let own = with(
        with(upstream(), PORT, "S231:unit", json!("K")),
        PORT,
        "S231:displayUnit",
        json!("degC"),
    );
    assert_eq!(export(&loaded(&own)).expect("exports"), bytes);
    assert_eq!(terms(&loaded(&upstream()), PORT), NO_TERMS);
}
