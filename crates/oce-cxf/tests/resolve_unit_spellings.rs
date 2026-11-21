//! The unit spellings modelica-json and CXF-Core write are read as the modeled properties they
//! name (issue #631; design note `docs/design/unit-spellings.md`, decision 1; owner ruling of
//! 2026-11-12): `qudt:hasUnit` and `S231:hasUnit` as `unit`, `S231:hasDisplayUnit` as
//! `displayUnit`, each matched after `@context` expansion, on every role.
//!
//! Before, the three keys fell into the pass-through map: the §7.10 unit gate, the host's point
//! list, the content id and the export all went without the units a document declared under them.
//!
//! Two spellings of one property on one node are refused even when they agree, a key that names a
//! spelling only after trimming is refused as a near miss, and `qudt:hasQuantityKind` is not read.
//! Every refusal is pinned as the complete diagnostic list, stated by hand. What an IRI-node value
//! reads as is `resolve_unit_iris.rs`'s, and the refusal of a `hasUnit` key under an unbound
//! `qudt` prefix or in a near miss of a QUDT namespace is `resolve_unread_qudt_has_unit.rs`'s.

use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value as Json, json};

use crate::unit_documents::{
    CONNECTOR_ROLES, INTEGER_OUTPUT, NO_TERMS, PARAMETER_ROLES, QUDT_HASH, QUDT_SLASH, ROLES, S231,
    UNIT_HASH, display_only, document, loaded, node_mut, refusal, rendered, terms, unit_only,
    upstream, upstream_context, with,
};

/// Every key that names `unit` under [`upstream_context`] with `s231` also bound to the S231
/// namespace: the engine's own, CXF-Core's shape term, modelica-json's, and each by full IRI or
/// another prefix.
fn unit_keys() -> Vec<String> {
    vec![
        "S231:unit".to_owned(),
        "S231:hasUnit".to_owned(),
        "qudt:hasUnit".to_owned(),
        "s231:hasUnit".to_owned(),
        format!("{S231}unit"),
        format!("{S231}hasUnit"),
        format!("{QUDT_HASH}hasUnit"),
        format!("{QUDT_SLASH}hasUnit"),
    ]
}

/// Every key that names `displayUnit` under the same context.
fn display_unit_keys() -> Vec<String> {
    vec![
        "S231:displayUnit".to_owned(),
        "S231:hasDisplayUnit".to_owned(),
        "s231:hasDisplayUnit".to_owned(),
        format!("{S231}displayUnit"),
        format!("{S231}hasDisplayUnit"),
    ]
}

/// [`upstream_context`] with a second prefix bound to the S231 namespace.
fn context() -> Json {
    let mut context = upstream_context();
    context["s231"] = json!(S231);
    context
}

fn base() -> Json {
    document(context())
}

/// The refusal of a second spelling of `S231:<term>` on `node`.
fn written_twice(node: &str, term: &str, first: &str, second: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::MalformedDocument,
        format!(
            "node `{node}` writes `S231:{term}` twice, as `{first}` and as `{second}`; a property \
             must have exactly one spelling"
        ),
    )
    .with_subject(node)
}

#[test]
fn every_spelling_reads_the_unit_on_every_connector_role() {
    let unset = loaded(&base());
    for role in CONNECTOR_ROLES {
        assert_eq!(terms(&unset, role), NO_TERMS, "{role}");
    }
    // A string and an IRI node, so the reading is the key's and not one value shape's.
    for value in [json!("K"), json!({ "@id": "unit:K" })] {
        for role in CONNECTOR_ROLES {
            for key in unit_keys() {
                let graph = loaded(&with(base(), role, &key, value.clone()));
                assert_eq!(terms(&graph, role), unit_only("K"), "{role} {key} {value}");
            }
            for key in display_unit_keys() {
                let graph = loaded(&with(base(), role, &key, value.clone()));
                assert_eq!(
                    terms(&graph, role),
                    display_only("K"),
                    "{role} {key} {value}"
                );
            }
        }
    }
}

#[test]
fn every_spelling_gives_the_graph_the_engines_own_spelling_gives() {
    // The whole graph, not one field: a document written in an upstream spelling loads to the
    // bytes the same document written with `S231:unit` or `S231:displayUnit` loads to.
    for role in CONNECTOR_ROLES {
        let unit = rendered(&with(base(), role, "S231:unit", json!("K")));
        for key in unit_keys() {
            for value in [json!("K"), json!({ "@id": "unit:K" })] {
                assert_eq!(
                    rendered(&with(base(), role, &key, value)),
                    unit,
                    "{role} {key}"
                );
            }
        }
        let display = rendered(&with(base(), role, "S231:displayUnit", json!("degC")));
        for key in display_unit_keys() {
            for value in [json!("degC"), json!({ "@id": "unit:DEG_C" })] {
                assert_eq!(
                    rendered(&with(base(), role, &key, value)),
                    display,
                    "{role} {key}"
                );
            }
        }
    }
}

#[test]
fn every_spelling_on_a_parameter_is_checked_and_reaches_nothing() {
    // A parameter's terms are never used for control: a well-shaped one changes no part of the
    // graph, under any key.
    let unset = rendered(&base());
    for role in PARAMETER_ROLES {
        for key in unit_keys().into_iter().chain(display_unit_keys()) {
            for value in [json!("K"), json!({ "@id": "unit:K" })] {
                assert_eq!(
                    rendered(&with(base(), role, &key, value.clone())),
                    unset,
                    "{role} {key} {value}"
                );
            }
        }
    }
    // The check is real: a value that fits no term shape is refused there under every key, as
    // `S231:unit` is.
    for role in PARAMETER_ROLES {
        for (keys, term) in [(unit_keys(), "unit"), (display_unit_keys(), "displayUnit")] {
            for key in keys {
                assert_eq!(
                    refusal(&with(base(), role, &key, json!(5))),
                    vec![
                        Diagnostic::error(
                            DiagCode::MalformedDocument,
                            format!(
                                "S231:{term} on a parameter is not a string, typed literal, or \
                                 IRI node — malformed term"
                            ),
                        )
                        .with_subject(role)
                    ],
                    "{role} {key}"
                );
            }
        }
    }
}

#[test]
fn the_qudt_key_is_read_in_both_schema_namespaces_under_any_prefix_name() {
    for namespace in [QUDT_HASH, QUDT_SLASH] {
        for prefix in ["qudt", "q2", "QUDT"] {
            let mut context = upstream_context();
            context.as_object_mut().expect("context").remove("qudt");
            context[prefix] = json!(namespace);
            for role in CONNECTOR_ROLES {
                let doc = with(
                    document(context.clone()),
                    role,
                    &format!("{prefix}:hasUnit"),
                    json!({ "@id": "unit:PA" }),
                );
                assert_eq!(
                    terms(&loaded(&doc), role),
                    unit_only("Pa"),
                    "{namespace} {prefix} {role}"
                );
            }
        }
    }
}

#[test]
fn a_qudt_prefix_bound_to_an_unrelated_namespace_names_that_vocabularys_property() {
    // The key is matched by its expanded IRI alone. Bound to a namespace that is no QUDT one and
    // no near miss of one, the compact key `qudt:hasUnit` is some other property, and stays in the
    // pass-through map: the node declares no unit, whatever the value. (Unbound, or bound to a
    // near miss of a QUDT namespace, it is refused: `resolve_unread_qudt_has_unit.rs`.)
    let context = {
        let mut context = upstream_context();
        context["qudt"] = json!("http://example.org/qudt#");
        context
    };
    let unset = rendered(&document(context.clone()));
    for role in ROLES {
        // The key, and its whitespace near miss: trimmed, it still names no modeled property
        // under this binding, so it is no near miss either.
        for key in ["qudt:hasUnit", "qudt:hasUnit ", " qudt:hasUnit"] {
            for value in [json!("K"), json!({ "@id": "unit:K" }), json!(5)] {
                let doc = with(document(context.clone()), role, key, value);
                assert_eq!(rendered(&doc), unset, "{role} {key:?}");
            }
        }
    }
}

#[test]
fn the_s231_spellings_are_read_with_no_s231_binding_like_every_compact_key() {
    // A document that binds no `S231` has compact keys with no IRI at all; they are read in the
    // S231 namespace, the new spellings with the rest.
    let context = json!({ "unit": UNIT_HASH });
    for role in CONNECTOR_ROLES {
        let unit = with(document(context.clone()), role, "S231:hasUnit", json!("K"));
        assert_eq!(terms(&loaded(&unit), role), unit_only("K"), "{role}");
        let display = with(
            document(context.clone()),
            role,
            "S231:hasDisplayUnit",
            json!({ "@id": "unit:DEG_C" }),
        );
        assert_eq!(
            terms(&loaded(&display), role),
            display_only("degC"),
            "{role}"
        );
    }
}

#[test]
fn a_unit_and_a_display_unit_in_the_upstream_spellings_are_two_properties() {
    // The form every unit-carrying connector of the vendored translations has.
    for role in CONNECTOR_ROLES {
        let doc = with(
            with(upstream(), role, "qudt:hasUnit", json!({ "@id": "unit:K" })),
            role,
            "S231:hasDisplayUnit",
            json!({ "@id": "unit:DEG_C" }),
        );
        assert_eq!(
            terms(&loaded(&doc), role),
            (Some("K".to_owned()), None, Some("degC".to_owned())),
            "{role}"
        );
    }
}

#[test]
fn qudt_has_quantity_kind_is_not_read_under_any_binding() {
    // Decision 1: modelica-json's quantity table is lossy at `PressureDifference`, so the kind
    // is not read. It stays in the pass-through map: it sets no quantity, it is no second
    // spelling of `S231:quantity`, and its value is not judged.
    for key in [
        "qudt:hasQuantityKind".to_owned(),
        format!("{QUDT_HASH}hasQuantityKind"),
        format!("{QUDT_SLASH}hasQuantityKind"),
        "S231:hasQuantityKind".to_owned(),
        "S231:hasQuantity".to_owned(),
    ] {
        let unset = rendered(&base());
        for role in ROLES {
            for value in [json!({ "@id": "q:Pressure" }), json!("Pressure"), json!(5)] {
                assert_eq!(
                    rendered(&with(base(), role, &key, value)),
                    unset,
                    "{key} {role}"
                );
            }
        }
        for role in CONNECTOR_ROLES {
            let doc = with(
                with(base(), role, "S231:quantity", json!("PressureDifference")),
                role,
                &key,
                json!({ "@id": "q:Pressure" }),
            );
            assert_eq!(
                terms(&loaded(&doc), role),
                (None, Some("PressureDifference".to_owned()), None),
                "{key} {role}"
            );
        }
    }
}

#[test]
fn two_spellings_of_one_property_are_refused_whether_or_not_they_agree() {
    let qudt_hash = format!("{QUDT_HASH}hasUnit");
    let qudt_slash = format!("{QUDT_SLASH}hasUnit");
    // `(term, the spelling named first, the spelling named second)`. The compact key the DTO
    // models is named first; two other spellings are named in key order.
    let pairs = [
        ("unit", "S231:unit", "qudt:hasUnit"),
        ("unit", "S231:unit", "S231:hasUnit"),
        ("unit", "S231:hasUnit", "qudt:hasUnit"),
        ("unit", "S231:unit", qudt_slash.as_str()),
        ("unit", qudt_hash.as_str(), qudt_slash.as_str()),
        ("unit", qudt_hash.as_str(), "qudt:hasUnit"),
        ("unit", "S231:hasUnit", "s231:hasUnit"),
        ("displayUnit", "S231:displayUnit", "S231:hasDisplayUnit"),
        ("displayUnit", "S231:hasDisplayUnit", "s231:hasDisplayUnit"),
    ];
    // Agreeing as written, agreeing only through the table, and disagreeing.
    let values = [
        (json!("K"), json!("K")),
        (json!({ "@id": "unit:K" }), json!({ "@id": "unit:K" })),
        (json!("K"), json!({ "@id": "unit:K" })),
        (json!("K"), json!("degC")),
        (json!({ "@id": "unit:K" }), json!({ "@id": "unit:DEG_C" })),
    ];
    for (term, first, second) in pairs {
        for (one, other) in &values {
            for role in ROLES {
                let doc = with(
                    with(base(), role, first, one.clone()),
                    role,
                    second,
                    other.clone(),
                );
                assert_eq!(
                    refusal(&doc),
                    vec![written_twice(role, term, first, second)],
                    "{role} {first} {second} {one} {other}"
                );
            }
        }
    }
}

#[test]
fn three_spellings_and_an_explicit_null_are_each_a_second_spelling() {
    for role in ROLES {
        let mut doc = base();
        let node = node_mut(&mut doc, role);
        node.insert("S231:unit".to_owned(), json!("K"));
        node.insert("S231:hasUnit".to_owned(), json!("K"));
        node.insert("qudt:hasUnit".to_owned(), json!("K"));
        assert_eq!(
            refusal(&doc),
            vec![
                written_twice(role, "unit", "S231:unit", "S231:hasUnit"),
                written_twice(role, "unit", "S231:unit", "qudt:hasUnit"),
            ],
            "{role}"
        );
        // An explicit `null` under the compact key is a spelling the document wrote, though the
        // DTO reads it as absent.
        let null = with(
            with(base(), role, "S231:unit", Json::Null),
            role,
            "qudt:hasUnit",
            json!("K"),
        );
        assert_eq!(
            refusal(&null),
            vec![written_twice(role, "unit", "S231:unit", "qudt:hasUnit")],
            "{role}"
        );
    }
}

#[test]
fn a_null_under_an_upstream_spelling_reads_as_absent() {
    let unset = rendered(&base());
    for role in ROLES {
        for key in ["qudt:hasUnit", "S231:hasUnit", "S231:hasDisplayUnit"] {
            assert_eq!(
                rendered(&with(base(), role, key, Json::Null)),
                unset,
                "{role} {key}"
            );
        }
    }
}

#[test]
fn a_key_that_names_a_spelling_only_after_trimming_is_refused() {
    let near_misses = [
        ("qudt:hasUnit ", "unit"),
        (" qudt:hasUnit", "unit"),
        ("qudt:hasUnit\t", "unit"),
        ("\u{a0}qudt:hasUnit", "unit"),
        ("S231:hasUnit ", "unit"),
        ("S231:hasDisplayUnit\n", "displayUnit"),
        (" S231:hasDisplayUnit ", "displayUnit"),
    ];
    for (key, term) in near_misses {
        for role in ROLES {
            assert_eq!(
                refusal(&with(base(), role, key, json!("K"))),
                vec![
                    Diagnostic::error(
                        DiagCode::MalformedDocument,
                        format!(
                            "property key {key:?} on node `{role}` carries surrounding \
                             whitespace; without it, it names the modeled term `{term}`; write it \
                             as `S231:{term}`"
                        ),
                    )
                    .with_subject(role)
                ],
                "{key:?} {role}"
            );
        }
    }
    // By full IRI in each QUDT schema namespace too.
    for namespace in [QUDT_HASH, QUDT_SLASH] {
        let key = format!("{namespace}hasUnit ");
        let diags = refusal(&with(base(), ROLES[0], &key, json!("K")));
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, DiagCode::MalformedDocument);
        assert_eq!(
            diags[0].message,
            format!(
                "property key {key:?} on node `{}` carries surrounding whitespace; without it, \
                 it names the modeled term `unit`; write it as `S231:unit`",
                ROLES[0]
            )
        );
    }
}

#[test]
fn a_megabyte_of_whitespace_around_a_spelling_is_quoted_in_at_most_64_bytes() {
    let key = format!("qudt:hasUnit{}", " ".repeat(1 << 20));
    let diags = refusal(&with(base(), ROLES[0], &key, json!("K")));
    let quoted = format!("qudt:hasUnit{}…", " ".repeat(64 - "qudt:hasUnit".len()));
    assert_eq!(
        diags,
        vec![
            Diagnostic::error(
                DiagCode::MalformedDocument,
                format!(
                    "property key {quoted:?} on node `{}` carries surrounding whitespace; \
                     without it, it names the modeled term `unit`; write it as `S231:unit`",
                    ROLES[0]
                ),
            )
            .with_subject(ROLES[0])
        ]
    );
}

#[test]
fn a_near_miss_of_a_spelling_that_is_not_whitespace_stays_a_pass_through_key() {
    // Only surrounding whitespace is a near miss. Another case, a longer or shorter term or a
    // bare term is some other key, as for every modeled term. (`unit:hasUnit`, the property in
    // the unit vocabulary's namespace, is refused: `resolve_unread_qudt_has_unit.rs`.)
    let unset = rendered(&base());
    for key in [
        "qudt:hasunit",
        "qudt:HasUnit",
        "qudt:hasUnits",
        "qudt:unit",
        "qudt:hasDisplayUnit",
        "S231:hasunit",
        "S231:hasDisplayunit",
        "unit:hasDisplayUnit",
        "hasUnit",
    ] {
        for role in ROLES {
            assert_eq!(
                rendered(&with(base(), role, key, json!("K"))),
                unset,
                "{key} {role}"
            );
        }
    }
}

#[test]
fn an_s231_spelling_in_a_foreign_namespace_is_refused_like_every_modeled_term() {
    for (namespace, written, term) in [
        ("http://data.ashrae.org/S231P#", "hasUnit", "unit"),
        ("https://data.ashrae.org/S231#", "hasUnit", "unit"),
        (
            "https://data.ashrae.org/S231P#",
            "hasDisplayUnit",
            "displayUnit",
        ),
    ] {
        let key = format!("{namespace}{written}");
        for role in ROLES {
            assert_eq!(
                refusal(&with(base(), role, &key, json!("K"))),
                vec![
                    Diagnostic::error(
                        DiagCode::VocabularyNamespaceMismatch,
                        format!(
                            "property `{key}` on node `{role}` names the modeled term `{term}` in \
                             `{namespace}`; the S231 vocabulary is read only in `{S231}`; write \
                             it as `S231:{term}`"
                        ),
                    )
                    .with_subject(role)
                ],
                "{key} {role}"
            );
        }
    }
}

#[test]
fn a_scoped_context_on_the_term_object_stays_refused_under_every_spelling() {
    for (keys, term) in [(unit_keys(), "unit"), (display_unit_keys(), "displayUnit")] {
        for key in keys {
            for role in ROLES {
                let value = json!({ "@id": "unit:K", "@context": { "unit": UNIT_HASH } });
                assert_eq!(
                    refusal(&with(base(), role, &key, value)),
                    vec![
                        Diagnostic::error(
                            DiagCode::NonSubsetConstruct,
                            format!(
                                "S231:{term} term declares a scoped `@context`; node-scoped \
                                 contexts are not supported; declare required bindings in the \
                                 document-level `@context`"
                            ),
                        )
                        .with_subject(role)
                    ],
                    "{key} {role}"
                );
            }
        }
    }
}

#[test]
fn an_upstream_spelling_on_an_integer_connector_is_refused_as_any_term_there_is() {
    for key in ["qudt:hasUnit", "S231:hasUnit", "S231:hasDisplayUnit"] {
        for value in [json!("1"), json!({ "@id": "unit:UNITLESS" })] {
            assert_eq!(
                refusal(&with(base(), INTEGER_OUTPUT, key, value)),
                vec![
                    Diagnostic::error(
                        DiagCode::MalformedDocument,
                        "§7.4.1 term (unit/quantity/displayUnit) declared on an Integer \
                         connector, which permits only min and max",
                    )
                    .with_subject(INTEGER_OUTPUT)
                ],
                "{key}"
            );
        }
    }
}

#[test]
fn a_spelling_written_twice_on_one_node_is_refused_as_any_repeated_key_is() {
    // serde_json keeps one value of a repeated key, so the document is written by hand.
    let text = serde_json::to_string(&base()).expect("serialize");
    // The node itself, not a reference to it: a reference object closes after its `@id`.
    let needle = format!("{{\"@id\":\"{}\",", ROLES[2]);
    assert_eq!(
        text.matches(&needle).count(),
        1,
        "the port node is found once"
    );
    let twice = text.replacen(
        &needle,
        &format!("{needle}\"qudt:hasUnit\":\"K\",\"qudt:hasUnit\":\"degC\","),
        1,
    );
    let diags = match oce_cxf::import_cxf(twice.as_bytes(), &oce_cxf::ResolveOptions::default()) {
        Err(oce_cxf::CxfError::Validation(diags)) => diags,
        other => panic!("expected a refusal, got {other:?}"),
    };
    assert_eq!(
        diags,
        vec![
            Diagnostic::error(
                DiagCode::MalformedDocument,
                format!(
                    "node `{}` writes the key `qudt:hasUnit` 2 times; a key may appear once per \
                     node, and a repeat would silently keep only one of the values",
                    ROLES[2]
                ),
            )
            .with_subject(ROLES[2])
        ]
    );
}

#[test]
fn refusals_come_in_one_order_whatever_the_node_order() {
    let mut doc = base();
    for role in ROLES {
        let node = node_mut(&mut doc, role);
        node.insert("S231:hasUnit".to_owned(), json!("K"));
        node.insert("qudt:hasUnit".to_owned(), json!({ "@id": "unit:K" }));
    }
    let first = refusal(&doc);
    assert_eq!(first.len(), ROLES.len(), "{first:?}");
    for role in ROLES {
        assert!(
            first.contains(&written_twice(role, "unit", "S231:hasUnit", "qudt:hasUnit")),
            "{role}: {first:?}"
        );
    }
    let mut reversed = doc.clone();
    reversed["@graph"].as_array_mut().expect("@graph").reverse();
    for _ in 0..8 {
        assert_eq!(refusal(&doc), first);
        assert_eq!(refusal(&reversed), first);
    }
}
