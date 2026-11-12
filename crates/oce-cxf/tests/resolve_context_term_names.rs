//! An `@context` term spelled as an IRI or a compact IRI must bind its own expansion (GitHub issue
//! #591). A whole-token term match wins over every other reading of a token, so before this rule
//! a context could redirect a standard term wherever the document used it: a root typed
//! `S231:ExtensionBlock` loaded and ran its child once the context mapped `S231:ExtensionBlock`
//! elsewhere. JSON-LD 1.1 refuses such a definition as an invalid IRI mapping (Create Term
//! Definition, step 14.2.4); the expansion pass now refuses it as `malformed-document`, subject the
//! term, before any node is read. A term that binds its own expansion where it is defined but not
//! once a later context map has rebound its prefix, which JSON-LD allows, is refused as
//! `non-subset-construct`.
//!
//! Every refusal is pinned as the complete diagnostic list, stated by hand.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value as Json, json};

const S231_NS: &str = "http://data.ashrae.org/S231#";
const CONSTANT: &str = "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant";
const ROOT: &str = "http://example.org#R";
const FOO: &str = "http://example.org#Foo";

/// A root of type `class` with one `Constant` child, under `context`.
fn root_typed(class: &str, context: Json) -> Json {
    json!({
        "@context": context,
        "@graph": [
            {
                "@id": ROOT,
                "@type": class,
                "S231:containsBlock": { "@id": "http://example.org#R.con" }
            },
            {
                "@id": "http://example.org#R.con",
                "@type": CONSTANT,
                "S231:hasParameter": { "@id": "http://example.org#R.con.k" },
                "S231:hasOutput": { "@id": "http://example.org#R.con.y" }
            },
            { "@id": "http://example.org#R.con.k", "S231:value": 1.0 },
            {
                "@id": "http://example.org#R.con.y",
                "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            }
        ]
    })
}

fn import(doc: &Json) -> Result<usize, CxfError> {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    import_cxf(&bytes, &ResolveOptions::default()).map(|(graph, report)| {
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
        graph.blocks.len()
    })
}

fn loads(doc: &Json) -> usize {
    import(doc).unwrap_or_else(|e| panic!("expected a load: {e:?}"))
}

fn refusal(doc: &Json) -> Vec<Diagnostic> {
    match import(doc) {
        Err(CxfError::Validation(diags)) => diags,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// The refusal of `term`, bound to `iri`, whose own expansion is `own` (`None`: it has none).
fn redirect(term: &str, iri: &str, own: Option<&str>) -> Diagnostic {
    let reading = match own {
        Some(own) => format!("its own expansion is `{own}`"),
        None => "it has no expansion of its own".to_owned(),
    };
    Diagnostic::error(
        DiagCode::MalformedDocument,
        format!(
            "@context term `{term}` is spelled as an IRI but binds `{iri}`, and {reading}; \
             JSON-LD 1.1 refuses this as an invalid IRI mapping (Create Term Definition, step \
             14.2.4), because the document could then redirect that spelling wherever it appears"
        ),
    )
    .with_subject(term)
}

/// The #582 refusal of the root as an extension block: what `S231:ExtensionBlock` still means.
fn extension_block_root() -> Diagnostic {
    Diagnostic::error(
        DiagCode::ExtensionBlockUnsupported,
        "extension block (typed S231:ExtensionBlock, without S231:hasFmuPath): its behaviour is a \
         Functional Mockup Unit, and the engine does not execute FMUs (FMI is out of scope before \
         1.0)",
    )
    .with_subject(ROOT)
}

#[test]
fn the_controls_load_and_an_extension_block_root_is_refused() {
    assert_eq!(
        loads(&root_typed("S231:Block", json!({ "S231": S231_NS }))),
        1
    );
    assert_eq!(
        refusal(&root_typed(
            "S231:ExtensionBlock",
            json!({ "S231": S231_NS })
        )),
        vec![extension_block_root()]
    );
}

#[test]
fn a_compact_iri_term_cannot_redirect_a_type() {
    // The issue's first reproduction. Before: the root read as `Foo`, an ordinary composite, and
    // the document loaded and ran its child.
    let context = json!({ "S231": S231_NS, "S231:ExtensionBlock": FOO });
    assert_eq!(
        refusal(&root_typed("S231:ExtensionBlock", context)),
        vec![redirect(
            "S231:ExtensionBlock",
            FOO,
            Some("http://data.ashrae.org/S231#ExtensionBlock")
        )]
    );
}

#[test]
fn a_full_iri_term_cannot_redirect_a_type() {
    // The issue's second reproduction: the term named by the full IRI, the root typed by the same
    // IRI. Before: the root read as `Foo` and loaded. A root typed by the compact spelling was
    // never redirected, since a term matches a token as written; the context is refused all the
    // same.
    let full = format!("{S231_NS}ExtensionBlock");
    let mut context = json!({ "S231": S231_NS });
    context[full.as_str()] = json!(FOO);
    assert_eq!(
        refusal(&root_typed(&full, context.clone())),
        vec![redirect(&full, FOO, Some(&full))]
    );
    assert_eq!(
        refusal(&root_typed("S231:ExtensionBlock", context)),
        vec![redirect(&full, FOO, Some(&full))]
    );
}

#[test]
fn a_term_cannot_redirect_an_identity() {
    // The child's `@id` and its `containsBlock` reference, both compact. Before: both read as
    // `Elsewhere`, and the document loaded without a word.
    let context = json!({
        "S231": S231_NS,
        "ex": "http://example.org#",
        "ex:R.con": "http://example.org#Elsewhere"
    });
    let doc = json!({
        "@context": context,
        "@graph": [
            { "@id": ROOT, "@type": "S231:Block", "S231:containsBlock": { "@id": "ex:R.con" } },
            {
                "@id": "ex:R.con",
                "@type": CONSTANT,
                "S231:hasParameter": { "@id": "http://example.org#R.con.k" },
                "S231:hasOutput": { "@id": "http://example.org#R.con.y" }
            },
            { "@id": "http://example.org#R.con.k", "S231:value": 1.0 },
            {
                "@id": "http://example.org#R.con.y",
                "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            }
        ]
    });
    assert_eq!(
        refusal(&doc),
        vec![redirect(
            "ex:R.con",
            "http://example.org#Elsewhere",
            Some("http://example.org#R.con")
        )]
    );
}

#[test]
fn a_term_cannot_redirect_a_data_type() {
    // `S231:Real` read as Boolean on every connector that declares it. Before: the document loaded
    // without a word.
    let context = json!({ "S231": S231_NS, "S231:Real": "http://data.ashrae.org/S231#Boolean" });
    assert_eq!(
        refusal(&root_typed("S231:Block", context)),
        vec![redirect(
            "S231:Real",
            "http://data.ashrae.org/S231#Boolean",
            Some("http://data.ashrae.org/S231#Real")
        )]
    );
}

#[test]
fn a_term_cannot_redirect_a_full_iri_property_key() {
    // A bound keyed by its full IRI. Before: the key read as `notMax`, and the document loaded
    // without a word. (A compact `S231:max` key is read literally, so it was never redirected; its
    // term is refused all the same.)
    let full = format!("{S231_NS}max");
    for term in [full.as_str(), "S231:max"] {
        let mut doc = root_typed("S231:Block", json!({ "S231": S231_NS }));
        doc["@context"][term] = json!("http://example.org#notMax");
        doc["@graph"][3][full.as_str()] = json!(5.0);
        let own = format!("{S231_NS}max");
        assert_eq!(
            refusal(&doc),
            vec![redirect(term, "http://example.org#notMax", Some(&own))],
            "{term}"
        );
    }
}

#[test]
fn a_term_bound_to_its_own_expansion_is_accepted() {
    // The identity mapping changes nothing: `S231:ExtensionBlock` keeps its meaning, so the root is
    // still refused as an extension block, by its own code.
    let full = format!("{S231_NS}ExtensionBlock");
    let compact = json!({ "S231": S231_NS, "S231:ExtensionBlock": full });
    assert_eq!(
        refusal(&root_typed("S231:ExtensionBlock", compact)),
        vec![extension_block_root()]
    );
    let mut absolute = json!({ "S231": S231_NS });
    absolute[full.as_str()] = json!(full);
    assert_eq!(
        refusal(&root_typed("S231:ExtensionBlock", absolute)),
        vec![extension_block_root()]
    );
    // A compact IRI bound to its own expansion through another prefix, and an absolute IRI with
    // no prefix of its own bound to itself.
    let other = json!({
        "S231": S231_NS,
        "ex": "http://example.org#",
        "ex:Foo": FOO,
        "urn:oce:term": "urn:oce:term"
    });
    assert_eq!(loads(&root_typed("S231:Block", other)), 1);
}

#[test]
fn every_context_map_is_judged_whatever_a_later_map_says() {
    // In a list of maps a later binding replaces an earlier one, but JSON-LD refuses an invalid
    // definition where it is made: a redirect is refused whether a later map restores the
    // identity mapping or the redirect is the later map.
    let full = format!("{S231_NS}ExtensionBlock");
    let refused = vec![redirect(
        "S231:ExtensionBlock",
        FOO,
        Some("http://data.ashrae.org/S231#ExtensionBlock"),
    )];
    for context in [
        json!([{ "S231": S231_NS, "S231:ExtensionBlock": FOO }, { "S231:ExtensionBlock": full }]),
        json!([{ "S231": S231_NS, "S231:ExtensionBlock": full }, { "S231:ExtensionBlock": FOO }]),
        json!([{ "S231": S231_NS }, { "S231:ExtensionBlock": FOO }]),
    ] {
        assert_eq!(
            refusal(&root_typed("S231:ExtensionBlock", context.clone())),
            refused,
            "{context}"
        );
    }
}

/// The refusal of `term`, bound to `iri`, whose spelling a later map moved to `now`.
fn rebound(term: &str, iri: &str, now: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::NonSubsetConstruct,
        format!(
            "@context term `{term}` is spelled as an IRI and binds `{iri}`, its own expansion \
             where it is defined, but a later context map binds its prefix, so the same spelling \
             now expands to `{now}`; a term spelled as an IRI must bind its own expansion under \
             the whole context, or the document could redirect that spelling wherever it appears"
        ),
    )
    .with_subject(term)
}

#[test]
fn a_later_map_cannot_move_the_prefix_an_accepted_term_was_read_through() {
    // JSON-LD accepts both, so the engine refuses them as outside its subset: each term binds its
    // own expansion where it is defined, and a term keeps its mapping when a later map rebinds its
    // prefix. But the spelling then reads one way
    // as a whole token and another way everywhere else. In the first, `S231` is bound to the S231
    // namespace by the end, so the namespace check passes, and the root typed
    // `S231:ExtensionBlock` would read as a class of another namespace and load.
    let hostile = "http://example.org/hostile#";
    let cases = [
        (
            json!([
                { "S231": hostile, "S231:ExtensionBlock": format!("{hostile}ExtensionBlock") },
                { "S231": S231_NS }
            ]),
            rebound(
                "S231:ExtensionBlock",
                &format!("{hostile}ExtensionBlock"),
                "http://data.ashrae.org/S231#ExtensionBlock",
            ),
        ),
        (
            json!([{ "S231:ExtensionBlock": "S231:ExtensionBlock" }, { "S231": S231_NS }]),
            rebound(
                "S231:ExtensionBlock",
                "S231:ExtensionBlock",
                "http://data.ashrae.org/S231#ExtensionBlock",
            ),
        ),
    ];
    for (context, refused) in cases {
        assert_eq!(
            refusal(&root_typed("S231:ExtensionBlock", context.clone())),
            vec![refused],
            "{context}"
        );
    }
    // A later map that rebinds the prefix to the same IRI, or binds a prefix the term does not
    // read through, moves nothing.
    let full = format!("{S231_NS}ExtensionBlock");
    for later in [
        json!({ "S231": S231_NS }),
        json!({ "ex": "http://example.org#" }),
    ] {
        let context = json!([{ "S231": S231_NS, "S231:ExtensionBlock": full }, later]);
        assert_eq!(
            refusal(&root_typed("S231:ExtensionBlock", context.clone())),
            vec![extension_block_root()],
            "{context}"
        );
    }
}

#[test]
fn a_node_scoped_redirect_is_refused_as_a_scoped_context() {
    // Scoped contexts are refused before any of their terms is read (an earlier rule).
    let mut doc = root_typed("S231:ExtensionBlock", json!({ "S231": S231_NS }));
    doc["@graph"][0]["@context"] = json!({ "S231:ExtensionBlock": FOO });
    let diags = refusal(&doc);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct);
    assert_eq!(diags[0].subject.as_deref(), Some(ROOT));
}

#[test]
fn near_misses_are_judged_by_json_ld_s_own_shape() {
    // Not spelled as an IRI, so free to bind anything: a bare term (`S231:ExtensionBlock` keeps
    // its meaning), a colon only first or last, and a keyword-form name, which is never a term.
    for (term, iri) in [
        ("ExtensionBlock", FOO),
        ("ex:", "http://example.org#"),
        (":ex", "http://example.org#"),
        ("@ex", FOO),
    ] {
        let mut context = json!({ "S231": S231_NS });
        context[term] = json!(iri);
        assert_eq!(
            refusal(&root_typed("S231:ExtensionBlock", context.clone())),
            vec![extension_block_root()],
            "{term}"
        );
        assert_eq!(loads(&root_typed("S231:Block", context)), 1, "{term}");
    }

    // Spelled as an IRI: a slash anywhere, the identity mapping with its prefix unbound (it then
    // reads as an IRI of the scheme `S231`), and another suffix through a bound prefix.
    let cases = [
        (
            json!({ "S231": S231_NS, "a/b": FOO }),
            redirect("a/b", FOO, None),
        ),
        (
            json!({ "S231:Block": "http://data.ashrae.org/S231#Block" }),
            redirect(
                "S231:Block",
                "http://data.ashrae.org/S231#Block",
                Some("S231:Block"),
            ),
        ),
        (
            json!({ "S231": S231_NS, "ex": "http://example.org#", "ex:Foo": "http://example.org#Bar" }),
            redirect("ex:Foo", "http://example.org#Bar", Some(FOO)),
        ),
    ];
    for (context, refused) in cases {
        assert_eq!(
            refusal(&root_typed("S231:Block", context.clone())),
            vec![refused],
            "{context}"
        );
    }
}

#[test]
fn every_redirect_in_a_context_is_refused_in_term_order() {
    let context = json!({
        "S231": S231_NS,
        "S231:Real": "http://data.ashrae.org/S231#Boolean",
        "S231:ExtensionBlock": FOO,
        "a/b": FOO
    });
    let expected = vec![
        redirect(
            "S231:ExtensionBlock",
            FOO,
            Some("http://data.ashrae.org/S231#ExtensionBlock"),
        ),
        redirect(
            "S231:Real",
            "http://data.ashrae.org/S231#Boolean",
            Some("http://data.ashrae.org/S231#Real"),
        ),
        redirect("a/b", FOO, None),
    ];
    let doc = root_typed("S231:ExtensionBlock", context);
    let first = refusal(&doc);
    assert_eq!(first, expected);
    for _ in 0..8 {
        assert_eq!(refusal(&doc), first);
    }
}
