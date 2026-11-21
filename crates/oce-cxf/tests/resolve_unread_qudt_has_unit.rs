//! A `hasUnit` key the QUDT schema reading does not reach is refused, never left unread (issue
//! #675; owner ruling of 2026-11-20).
//!
//! `qudt:hasUnit` is read as `unit` by its expanded IRI in the two QUDT schema namespaces
//! (`resolve_unit_spellings.rs`). One edit from that, the key fell into the pass-through map: the
//! node loaded with no unit, or with the unit another key gave it, and the load reported nothing.
//! `"S231:unit": "K"` beside `"qudt:hasUnit": {"@id": "unit:DEG_C"}` under an unbound `qudt`
//! loaded as `K`. Each case is now a refusal, matched on the expanded IRI:
//!
//! - no `qudt` binding: `malformed-document`;
//! - `hasUnit` in a near miss of a QUDT schema namespace (`https:`, scheme or host case, a default
//!   port, a missing, repeated or wrong separator), or in the QUDT unit vocabulary:
//!   `vocabulary-namespace-mismatch`;
//! - either of those only once surrounding whitespace is trimmed: `malformed-document`.
//!
//! Every refusal is pinned as the complete diagnostic list, with each message stated by hand, on
//! every node of the shared composite and not only where a unit is read. A message holds the key
//! and the namespace as written, each an escaped excerpt, and no other text of the document's:
//! the node is the subject.

use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value as Json, json};

use crate::unit_documents::{
    INTEGER_OUTPUT, MEMBER, ORPHAN, PORT, QUDT_HASH, QUDT_SLASH, ROLES, ROOT_INPUT, S231,
    UNIT_HASH, UNIT_SLASH, document, loaded, node_mut, refusal, rendered, terms, unit_only,
    upstream, upstream_context, with,
};

/// The root composite and a block instance: nodes the engine reads no unit on.
const ROOT: &str = "http://example.org#U";
const BLOCK: &str = "http://example.org#U.add";

/// Every node kind of the shared composite: the six roles a unit can sit on, the root, a block
/// instance, an Integer connector and a connector nothing lists.
fn nodes() -> Vec<&'static str> {
    ROLES
        .into_iter()
        .chain([ROOT, BLOCK, INTEGER_OUTPUT, ORPHAN])
        .collect()
}

/// A string, a mapped unit IRI, an unmapped one, a value of no term shape and `null`: the key is
/// refused whatever it carries.
fn values() -> [Json; 5] {
    [
        json!("K"),
        json!({ "@id": "unit:K" }),
        json!({ "@id": "unit:FOO" }),
        json!(5),
        Json::Null,
    ]
}

/// The `@context` modelica-json writes with `qudt` left out.
fn unbound_context() -> Json {
    json!({ "S231": S231, "unit": UNIT_HASH })
}

/// [`crate::unit_documents::upstream_context`] with `qudt` bound to `binding`.
fn context_binding(binding: &str) -> Json {
    let mut context = upstream_context();
    context["qudt"] = json!(binding);
    context
}

/// The refusal of `key` on `node` where the document binds no `qudt` prefix.
fn unbound(node: &str, key: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::MalformedDocument,
        format!(
            "property `{key}` reads as `qudt:hasUnit` under a `qudt` prefix the document's \
             `@context` does not bind, so it names no property and the unit it carries would be \
             left unread; bind `qudt` to `http://qudt.org/schema/qudt#` or \
             `http://qudt.org/schema/qudt/`, or write the unit as `S231:unit`"
        ),
    )
    .with_subject(node)
}

/// The refusal of `key` on `node`, which names `hasUnit` in the namespace `written`.
fn wrong_namespace(node: &str, key: &str, written: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::VocabularyNamespaceMismatch,
        format!(
            "property `{key}` names `hasUnit` in `{written}`; the QUDT schema property `hasUnit` \
             is read only in `http://qudt.org/schema/qudt#` and `http://qudt.org/schema/qudt/`; \
             write it in one of them, or write the unit as `S231:unit`"
        ),
    )
    .with_subject(node)
}

/// The refusal of `key` on `node`, which is a refused `hasUnit` key only once trimmed; `without`
/// is what it is then.
fn padded(node: &str, key: &str, without: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::MalformedDocument,
        format!(
            "property key `{key}` carries surrounding whitespace; without it, {without}; write \
             the unit as `S231:unit`, or as `hasUnit` in `http://qudt.org/schema/qudt#` or \
             `http://qudt.org/schema/qudt/`"
        ),
    )
    .with_subject(node)
}

/// What a padded key is once trimmed, under no `qudt` binding.
const UNBOUND_WITHOUT: &str =
    "it reads as `qudt:hasUnit` under a `qudt` prefix the document's `@context` does not bind";

/// What a padded key is once trimmed, in the namespace `written`.
fn namespace_without(written: &str) -> String {
    format!("it names `hasUnit` in `{written}`, where the QUDT schema property is not read")
}

/// `(the namespace as written, how it misses)`: every near-miss class of a QUDT schema namespace,
/// and the QUDT unit vocabulary with its own near misses. Each is a legal `@context` binding and,
/// followed by `hasUnit`, a legal full-IRI key.
const UNREAD_NAMESPACES: [(&str, &str); 19] = [
    ("https://qudt.org/schema/qudt#", "https, `#`"),
    ("https://qudt.org/schema/qudt/", "https, `/`"),
    ("HTTP://qudt.org/schema/qudt#", "scheme case"),
    ("hTTp://qudt.org/schema/qudt/", "scheme case, `/`"),
    ("http://QUDT.org/schema/qudt#", "host case"),
    ("http://QUDT.ORG/schema/qudt/", "host case, `/`"),
    ("http://qudt.org:80/schema/qudt#", "the default port"),
    ("http://qudt.org:/schema/qudt#", "an empty port"),
    (
        "https://qudt.org:443/schema/qudt/",
        "https and its default port",
    ),
    ("http://qudt.org/schema/qudt", "no separator"),
    ("http://qudt.org/schema/qudt/#", "a separator too many"),
    ("http://qudt.org/schema/qudt#/", "a separator too many"),
    ("http://qudt.org/schema/qudt##", "the separator twice"),
    ("HTTPS://QUDT.ORG:443/schema/qudt//", "every class at once"),
    ("http://qudt.org/vocab/unit#", "the unit vocabulary"),
    ("http://qudt.org/vocab/unit/", "the unit vocabulary, `/`"),
    ("https://qudt.org/vocab/unit#", "the unit vocabulary, https"),
    (
        "http://QUDT.org/vocab/unit/",
        "the unit vocabulary, host case",
    ),
    (
        "http://qudt.org/vocab/unit",
        "the unit vocabulary, no separator",
    ),
];

#[test]
fn the_qudt_key_under_no_qudt_binding_is_refused_on_every_node() {
    // With and without the other prefixes: a document that binds nothing at all reads its
    // compact `S231:` keys in the S231 namespace, and still has no `qudt`.
    for context in [unbound_context(), json!({})] {
        for node in nodes() {
            for value in values() {
                let doc = with(
                    document(context.clone()),
                    node,
                    "qudt:hasUnit",
                    value.clone(),
                );
                assert_eq!(
                    refusal(&doc),
                    vec![unbound(node, "qudt:hasUnit")],
                    "{context} {node} {value}"
                );
            }
        }
    }
}

#[test]
fn a_unit_contradicted_by_the_unbound_key_is_refused_where_it_loaded_as_the_other() {
    // The case the ruling was made on: `K` under the engine's key, degrees Celsius under the
    // upstream one, and no `qudt` binding. It loaded as `K` with no diagnostic, so the §7.10 gate
    // could not refuse `K` wired to `degC`.
    for node in ROLES {
        let contradicted = |context: Json| {
            with(
                with(document(context), node, "S231:unit", json!("K")),
                node,
                "qudt:hasUnit",
                json!({ "@id": "unit:DEG_C" }),
            )
        };
        assert_eq!(
            refusal(&contradicted(unbound_context())),
            vec![unbound(node, "qudt:hasUnit")],
            "{node}"
        );
        // The same node under a read binding is two spellings of `unit`: either way a refusal,
        // and no longer a matter of the binding alone.
        assert_eq!(
            refusal(&contradicted(upstream_context())),
            vec![
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!(
                        "node `{node}` writes `S231:unit` twice, as `S231:unit` and as \
                         `qudt:hasUnit`; a property must have exactly one spelling"
                    ),
                )
                .with_subject(node)
            ],
            "{node}"
        );
    }
}

#[test]
fn has_unit_under_a_prefix_bound_to_a_near_miss_namespace_is_refused_on_every_node() {
    for (written, how) in UNREAD_NAMESPACES {
        // Under `qudt`, as modelica-json names the prefix, and under any other name.
        for prefix in ["qudt", "q2"] {
            let mut context = upstream_context();
            context.as_object_mut().expect("context").remove("qudt");
            context[prefix] = json!(written);
            let key = format!("{prefix}:hasUnit");
            for node in nodes() {
                for value in values() {
                    let doc = with(document(context.clone()), node, &key, value.clone());
                    assert_eq!(
                        refusal(&doc),
                        vec![wrong_namespace(node, &key, written)],
                        "{how}: {written} {key} {node} {value}"
                    );
                }
            }
        }
    }
}

#[test]
fn has_unit_keyed_by_its_full_iri_in_a_near_miss_namespace_is_refused_whatever_qudt_binds() {
    for (written, how) in UNREAD_NAMESPACES {
        let key = format!("{written}hasUnit");
        for context in [upstream_context(), unbound_context(), json!({})] {
            for node in nodes() {
                let doc = with(
                    document(context.clone()),
                    node,
                    &key,
                    json!({ "@id": "unit:K" }),
                );
                assert_eq!(
                    refusal(&doc),
                    vec![wrong_namespace(node, &key, written)],
                    "{how}: {key} {context} {node}"
                );
            }
        }
    }
}

#[test]
fn the_unit_prefix_itself_does_not_carry_the_property() {
    // `unit` is bound to the unit vocabulary in every modelica-json translation, so `unit:hasUnit`
    // is the slip of writing the property under the prefix of its value.
    for namespace in [UNIT_HASH, UNIT_SLASH] {
        let mut context = upstream_context();
        context["unit"] = json!(namespace);
        for node in nodes() {
            let doc = with(document(context.clone()), node, "unit:hasUnit", json!("K"));
            assert_eq!(
                refusal(&doc),
                vec![wrong_namespace(node, "unit:hasUnit", namespace)],
                "{namespace} {node}"
            );
        }
    }
}

#[test]
fn a_context_term_that_expands_to_an_unread_has_unit_is_refused_like_the_key_it_stands_for() {
    // The key is matched on its expanded IRI, so a whole-token `@context` term is no way around
    // either refusal.
    for node in nodes() {
        let mut near = upstream_context();
        near["hu"] = json!("https://qudt.org/schema/qudt#hasUnit");
        assert_eq!(
            refusal(&with(document(near), node, "hu", json!("K"))),
            vec![wrong_namespace(node, "hu", "https://qudt.org/schema/qudt#")],
            "{node}"
        );
        let mut unbound_term = unbound_context();
        unbound_term["hu"] = json!("qudt:hasUnit");
        assert_eq!(
            refusal(&with(document(unbound_term), node, "hu", json!("K"))),
            vec![unbound(node, "hu")],
            "{node}"
        );
    }
    // The same term bound to a read IRI is the property, as before.
    let mut read = upstream_context();
    read["hu"] = json!(format!("{QUDT_SLASH}hasUnit"));
    let graph = loaded(&with(document(read), PORT, "hu", json!("K")));
    assert_eq!(terms(&graph, PORT), unit_only("K"));
}

#[test]
fn only_the_binding_in_force_decides_in_a_list_of_context_maps() {
    let near = "https://qudt.org/schema/qudt#";
    // A later map replaces an earlier one: a near miss replaced by a read namespace reads the
    // unit, and the reverse is refused.
    let repaired =
        json!([{ "S231": S231, "unit": UNIT_HASH, "qudt": near }, { "qudt": QUDT_HASH }]);
    let doc = with(
        document(repaired),
        MEMBER,
        "qudt:hasUnit",
        json!({ "@id": "unit:PA" }),
    );
    assert_eq!(terms(&loaded(&doc), MEMBER), unit_only("Pa"));
    let broken = json!([{ "S231": S231, "unit": UNIT_HASH, "qudt": QUDT_HASH }, { "qudt": near }]);
    let doc = with(
        document(broken),
        MEMBER,
        "qudt:hasUnit",
        json!({ "@id": "unit:PA" }),
    );
    assert_eq!(
        refusal(&doc),
        vec![wrong_namespace(MEMBER, "qudt:hasUnit", near)]
    );
}

#[test]
fn a_key_that_is_an_unread_has_unit_only_once_trimmed_is_refused() {
    // Unbound, the trimmed key names no modeled property, so the whitespace near-miss rule for a
    // modeled key did not reach it and the padded key passed through too.
    let padded_keys = [
        ("qudt:hasUnit ", "qudt:hasUnit "),
        (" qudt:hasUnit", " qudt:hasUnit"),
        ("  qudt:hasUnit  ", "  qudt:hasUnit  "),
        ("qudt:hasUnit\t", "qudt:hasUnit\\u{0009}"),
        ("\nqudt:hasUnit", "\\u{000A}qudt:hasUnit"),
        ("\u{a0}qudt:hasUnit", "\u{a0}qudt:hasUnit"),
    ];
    for (key, quoted) in padded_keys {
        for node in nodes() {
            let doc = with(document(unbound_context()), node, key, json!("K"));
            assert_eq!(
                refusal(&doc),
                vec![padded(node, quoted, UNBOUND_WITHOUT)],
                "{key:?} {node}"
            );
        }
    }
    // By full IRI and through a near-miss binding, in every unread namespace.
    for (written, how) in UNREAD_NAMESPACES {
        let without = namespace_without(written);
        let full = format!(" {written}hasUnit");
        assert_eq!(
            refusal(&with(upstream(), ROOT_INPUT, &full, json!("K"))),
            vec![padded(ROOT_INPUT, &full, &without)],
            "{how}: {full:?}"
        );
        let doc = with(
            document(context_binding(written)),
            ROOT_INPUT,
            "qudt:hasUnit ",
            json!("K"),
        );
        assert_eq!(
            refusal(&doc),
            vec![padded(ROOT_INPUT, "qudt:hasUnit ", &without)],
            "{how}: {written}"
        );
    }
}

#[test]
fn a_megabyte_of_key_is_quoted_in_at_most_64_bytes() {
    let megabyte = 1 << 20;
    // Whitespace around the unbound key.
    let key = format!("qudt:hasUnit{}", " ".repeat(megabyte));
    let quoted = format!("qudt:hasUnit{}…", " ".repeat(64 - "qudt:hasUnit".len()));
    assert_eq!(
        refusal(&with(document(unbound_context()), PORT, &key, json!("K"))),
        vec![padded(PORT, &quoted, UNBOUND_WITHOUT)]
    );
    // A `@context` term of a megabyte, bound to a near-miss namespace: the key is cut, and the
    // namespace is the 29 bytes it is.
    let term = "q".repeat(megabyte);
    let mut context = upstream_context();
    context[term.as_str()] = json!("https://qudt.org/schema/qudt#");
    let key = format!("{term}:hasUnit");
    assert_eq!(
        refusal(&with(document(context), PORT, &key, json!("K"))),
        vec![wrong_namespace(
            PORT,
            &format!("{}…", "q".repeat(64)),
            "https://qudt.org/schema/qudt#"
        )]
    );
    // A megabyte of separators: both the key and the namespace as written are cut.
    let key = format!("http://qudt.org/schema/qudt{}hasUnit", "/".repeat(megabyte));
    let cut = format!("http://qudt.org/schema/qudt{}…", "/".repeat(64 - 27));
    let diags = refusal(&with(upstream(), PORT, &key, json!("K")));
    assert_eq!(diags, vec![wrong_namespace(PORT, &cut, &cut)]);
    assert!(diags[0].message.len() < 512, "{}", diags[0].message.len());
}

#[test]
fn a_megabyte_node_id_is_the_subject_and_is_not_copied_into_the_message() {
    // The message holds the key and the namespace and no other authored text, so a long `@id`
    // does not set its size: the same bytes as for a short one.
    let long = format!("http://example.org#U.{}", "n".repeat(1 << 20));
    for (context, key, expected) in [
        (
            unbound_context(),
            "qudt:hasUnit",
            unbound(&long, "qudt:hasUnit"),
        ),
        (
            unbound_context(),
            " qudt:hasUnit",
            padded(&long, " qudt:hasUnit", UNBOUND_WITHOUT),
        ),
        (
            context_binding("https://qudt.org/schema/qudt#"),
            "qudt:hasUnit",
            wrong_namespace(&long, "qudt:hasUnit", "https://qudt.org/schema/qudt#"),
        ),
    ] {
        let mut doc = document(context);
        node_mut(&mut doc, ORPHAN).insert("@id".to_owned(), json!(long));
        let diags = refusal(&with(doc, &long, key, json!("K")));
        assert_eq!(diags, vec![expected], "{key:?}");
        assert!(diags[0].message.len() < 400, "{}", diags[0].message.len());
        assert!(!diags[0].message.contains("example.org"), "{key:?}");
    }
}

#[test]
fn a_format_or_control_character_in_the_key_is_quoted_as_an_escape() {
    // A prefix may be spelled with a BEL and a right-to-left override; neither reaches a message
    // as it is.
    let prefix = "q\u{7}\u{202E}";
    let key = format!("{prefix}:hasUnit");
    let escaped = "q\\u{0007}\\u{202E}:hasUnit";
    let mut near = upstream_context();
    near[prefix] = json!("https://qudt.org/schema/qudt#");
    let diags = refusal(&with(document(near.clone()), PORT, &key, json!("K")));
    assert_eq!(
        diags,
        vec![wrong_namespace(
            PORT,
            escaped,
            "https://qudt.org/schema/qudt#"
        )]
    );
    let padded_key = format!("{key} ");
    let diags = refusal(&with(document(near), PORT, &padded_key, json!("K")));
    assert_eq!(
        diags,
        vec![padded(
            PORT,
            &format!("{escaped} "),
            &namespace_without("https://qudt.org/schema/qudt#")
        )]
    );
    // Through a whole-token term, under no `qudt` binding.
    let term = "h\u{200B}u";
    let mut unbound_term = unbound_context();
    unbound_term[term] = json!("qudt:hasUnit");
    assert_eq!(
        refusal(&with(document(unbound_term), PORT, term, json!("K"))),
        vec![unbound(PORT, "h\\u{200B}u")]
    );
}

#[test]
fn what_only_resembles_an_unread_has_unit_stays_a_pass_through_key() {
    // Bound to a namespace that is no QUDT one and no near miss of one, `hasUnit` is that
    // vocabulary's property: JSON-LD's reading, and no unit of this document's.
    for binding in [
        "http://example.org/qudt#",
        "http://example.org/schema/qudt#",
        "http://qudt.org.example.org/schema/qudt#",
        "http://www.qudt.org/schema/qudt#",
        "http://qudt.org:8080/schema/qudt#",
        "http://qudt.org/schema/QUDT#",
        "http://qudt.org/schema#",
        "http://qudt.org/vocab/quantitykind#",
        "ftp://qudt.org/schema/qudt#",
    ] {
        let context = context_binding(binding);
        let unset = rendered(&document(context.clone()));
        for node in nodes() {
            for key in ["qudt:hasUnit", "qudt:hasUnit ", " qudt:hasUnit"] {
                let doc = with(document(context.clone()), node, key, json!("K"));
                assert_eq!(rendered(&doc), unset, "{binding} {node} {key:?}");
            }
        }
    }
    // Under no `qudt` binding, only the key itself is refused: another prefix, another case and
    // another local name are other keys, and so is `hasQuantityKind`, which is never read.
    let unset = rendered(&document(unbound_context()));
    for key in [
        "QUDT:hasUnit",
        "Qudt:hasUnit",
        "q:hasUnit",
        "qudt:HasUnit",
        "qudt:hasunit",
        "qudt:hasUnits",
        "qudt:unit",
        "qudt:hasQuantityKind",
        "qudt:has Unit",
        "hasUnit",
    ] {
        for node in nodes() {
            let doc = with(document(unbound_context()), node, key, json!("K"));
            assert_eq!(rendered(&doc), unset, "{key} {node}");
        }
    }
    // In a near-miss namespace, only `hasUnit` is refused.
    let near = context_binding("https://qudt.org/schema/qudt#");
    let unset = rendered(&document(near.clone()));
    for key in [
        "qudt:hasQuantityKind",
        "qudt:HasUnit",
        "qudt:hasUnits",
        "qudt:symbol",
    ] {
        for node in nodes() {
            let doc = with(document(near.clone()), node, key, json!("K"));
            assert_eq!(rendered(&doc), unset, "{key} {node}");
        }
    }
}

#[test]
fn every_unread_key_is_reported_and_in_one_order_whatever_the_node_order() {
    // Three keys on each of two nodes, in a document that binds no `qudt`: the unbound key, a
    // full IRI in a near-miss namespace, and a padded one. Refusals sort by subject, then code,
    // then message.
    let near = "https://qudt.org/schema/qudt#hasUnit";
    let mut doc = document(unbound_context());
    for node in [PORT, MEMBER] {
        let node = node_mut(&mut doc, node);
        node.insert("qudt:hasUnit".to_owned(), json!("K"));
        node.insert(near.to_owned(), json!("K"));
        node.insert(" qudt:hasUnit".to_owned(), json!("K"));
    }
    // "property `…" sorts before "property key `…".
    let of = |node: &str| {
        vec![
            unbound(node, "qudt:hasUnit"),
            padded(node, " qudt:hasUnit", UNBOUND_WITHOUT),
            wrong_namespace(node, near, "https://qudt.org/schema/qudt#"),
        ]
    };
    // `…U.add.u1` sorts before `…U.gai.y`.
    let expected: Vec<Diagnostic> = of(PORT).into_iter().chain(of(MEMBER)).collect();
    let mut reversed = doc.clone();
    reversed["@graph"].as_array_mut().expect("@graph").reverse();
    for _ in 0..8 {
        assert_eq!(refusal(&doc), expected);
        assert_eq!(refusal(&reversed), expected);
    }
}

#[test]
fn an_unread_key_beside_another_refused_key_reports_both() {
    // The refusal joins the other property-key refusals of the document; none hides another.
    let doc = with(
        with(
            document(unbound_context()),
            PORT,
            "qudt:hasUnit",
            json!("K"),
        ),
        MEMBER,
        "S231:min ",
        json!(0),
    );
    assert_eq!(
        refusal(&doc),
        vec![
            unbound(PORT, "qudt:hasUnit"),
            Diagnostic::error(
                DiagCode::MalformedDocument,
                format!(
                    "property key \"S231:min \" on node `{MEMBER}` carries surrounding \
                     whitespace; without it, it names the modeled term `min`; write it as \
                     `S231:min`"
                ),
            )
            .with_subject(MEMBER),
        ]
    );
}

#[test]
fn the_read_spellings_load_as_before_under_every_read_binding() {
    // The controls: nothing here refuses a key the engine reads.
    for namespace in [QUDT_HASH, QUDT_SLASH] {
        for key in ["qudt:hasUnit".to_owned(), format!("{namespace}hasUnit")] {
            let doc = with(
                document(context_binding(namespace)),
                MEMBER,
                &key,
                json!({ "@id": "unit:K" }),
            );
            assert_eq!(terms(&loaded(&doc), MEMBER), unit_only("K"), "{key}");
        }
    }
    // Bound to the S231 namespace, `qudt:hasUnit` is `S231:hasUnit` under another prefix.
    let doc = with(
        document(context_binding(S231)),
        MEMBER,
        "qudt:hasUnit",
        json!("K"),
    );
    assert_eq!(terms(&loaded(&doc), MEMBER), unit_only("K"));
}
