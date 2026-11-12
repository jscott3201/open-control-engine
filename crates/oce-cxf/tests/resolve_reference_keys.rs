//! A reference object is read as its `@id` alone, so one that carries any other key is refused as
//! `non-subset-construct`, subject the node holding it (GitHub issue #588). Before, the key was
//! kept for round-trip and never read: `hasParameter: {"@id": k, "S231:value": 99}` loaded with
//! the node's own value, and a `containsBlock` entry typed `S231:ExtensionBlock` loaded and ran
//! its leaf.
//!
//! Every refusal is pinned as the complete diagnostic list, stated by hand, except in the corpus
//! sweep, which derives each expected diagnostic from the fixture with its own reading of the
//! fixture's `@context`.

use std::path::Path;
use std::process::Command;

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Map, Value as Json, json};

const S231_NS: &str = "http://data.ashrae.org/S231#";
const MINIMAL_NESTED: &[u8] =
    include_bytes!("fixtures/composite_contract/accepted/minimal_nested.jsonld");
const AGREEING_PARAMETERS: &[u8] =
    include_bytes!("fixtures/composite_contract/accepted/agreeing_parameter_values.jsonld");

/// The eight properties whose values are reference objects, in the order the check reads them.
const REFERENCE_SLOTS: [&str; 8] = [
    "S231:hasInput",
    "S231:hasOutput",
    "S231:hasParameter",
    "S231:hasConstant",
    "S231:containsBlock",
    "S231:hasInstance",
    "S231:isConnectedTo",
    "S231:isOfDataType",
];

/// A root with one `Constant` child, its parameter `k` bound to 1.0.
fn constant_root() -> Json {
    json!({
        "@context": { "S231": S231_NS },
        "@graph": [
            {
                "@id": "http://example.org#R",
                "@type": "S231:Block",
                "S231:containsBlock": { "@id": "http://example.org#R.con" }
            },
            {
                "@id": "http://example.org#R.con",
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
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

fn refusal(doc: &Json) -> Vec<Diagnostic> {
    match import(doc) {
        Err(CxfError::Validation(diags)) => diags,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// The refusal of the `slot` reference `id` on `owner`, carrying `keys` (as the message names
/// them).
fn carries(owner: &str, slot: &str, id: &str, keys: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::NonSubsetConstruct,
        format!(
            "{slot} reference `{id}` on node `{owner}` carries {keys} beside its `@id`; a \
             reference is read as its `@id` alone, so the engine would drop it without effect: \
             state it on the referenced node instead"
        ),
    )
    .with_subject(owner)
}

/// The node of `doc` whose `@id` is `id`.
fn node<'a>(doc: &'a mut Json, id: &str) -> &'a mut Map<String, Json> {
    doc["@graph"]
        .as_array_mut()
        .expect("@graph")
        .iter_mut()
        .find(|node| node["@id"] == id)
        .and_then(Json::as_object_mut)
        .unwrap_or_else(|| panic!("no node `{id}`"))
}

/// The `index`th reference object of `slot` on `owner` (a lone object is index 0).
fn reference<'a>(
    doc: &'a mut Json,
    owner: &str,
    slot: &str,
    index: usize,
) -> &'a mut Map<String, Json> {
    let value = node(doc, owner).get_mut(slot).expect("slot");
    let value = match value {
        Json::Array(items) => &mut items[index],
        single => single,
    };
    value.as_object_mut().expect("reference object")
}

#[test]
fn a_containsblock_entry_cannot_type_its_leaf_an_extension_block() {
    // The issue's first reproduction. Before: the keys were dropped and the leaf loaded and ran.
    let mut doc = constant_root();
    let entry = reference(&mut doc, "http://example.org#R", "S231:containsBlock", 0);
    entry.insert("@type".into(), json!("S231:ExtensionBlock"));
    entry.insert("S231:hasFmuPath".into(), json!("con.fmu"));
    assert_eq!(
        refusal(&doc),
        vec![carries(
            "http://example.org#R",
            "S231:containsBlock",
            "http://example.org#R.con",
            "the keys `@type`, `S231:hasFmuPath`"
        )]
    );
}

#[test]
fn a_hasparameter_entry_cannot_carry_a_value() {
    // The issue's second reproduction. Before: the document loaded with `k` at 1.0.
    let mut doc = constant_root();
    reference(&mut doc, "http://example.org#R.con", "S231:hasParameter", 0)
        .insert("S231:value".into(), json!(99));
    assert_eq!(
        refusal(&doc),
        vec![carries(
            "http://example.org#R.con",
            "S231:hasParameter",
            "http://example.org#R.con.k",
            "the key `S231:value`"
        )]
    );
}

#[test]
fn every_reference_bearing_property_is_refused_with_a_key_beside_its_id() {
    let nested = |owner: &str| format!("http://example.org#cc.minimal_nested{owner}");
    let agreeing = |owner: &str| format!("http://example.org#cc.agreeing_parameter_values{owner}");
    // (fixture, owner, property, index, key, the reference's canonical `@id`)
    let cases = [
        (
            MINIMAL_NESTED,
            nested(""),
            "S231:hasInput",
            0,
            "@type",
            nested(".u"),
        ),
        (
            MINIMAL_NESTED,
            nested(""),
            "S231:hasOutput",
            0,
            "@index",
            nested(".y"),
        ),
        (
            MINIMAL_NESTED,
            nested(""),
            "S231:hasParameter",
            1,
            "S231:value",
            nested(".kTop"),
        ),
        (
            MINIMAL_NESTED,
            nested(".sub"),
            "S231:hasConstant",
            0,
            "S231:value",
            nested(".sub.kInner"),
        ),
        (
            MINIMAL_NESTED,
            nested(""),
            "S231:containsBlock",
            0,
            "S231:hasFmuPath",
            nested(".sub"),
        ),
        (
            AGREEING_PARAMETERS,
            agreeing(".del"),
            "S231:hasInstance",
            0,
            "@index",
            agreeing(".del.u"),
        ),
        (
            MINIMAL_NESTED,
            nested(".u"),
            "S231:isConnectedTo",
            0,
            "http://data.ashrae.org/S231#label",
            nested(".sub.u"),
        ),
        (
            MINIMAL_NESTED,
            nested(".kBase"),
            "S231:isOfDataType",
            0,
            "@type",
            format!("{S231_NS}Real"),
        ),
    ];
    let mut covered: Vec<&str> = cases.iter().map(|case| case.2).collect();
    covered.sort_unstable();
    let mut slots = REFERENCE_SLOTS.to_vec();
    slots.sort_unstable();
    assert_eq!(covered, slots, "one case per reference-bearing property");
    for (fixture, owner, slot, index, key, id) in cases {
        let original: Json = serde_json::from_slice(fixture).expect("fixture parses");
        import(&original).unwrap_or_else(|e| panic!("{slot}: the fixture loads as written: {e:?}"));
        let mut doc = original;
        reference(&mut doc, &owner, slot, index).insert(key.into(), json!("x"));
        assert_eq!(
            refusal(&doc),
            vec![carries(&owner, slot, &id, &format!("the key `{key}`"))],
            "{slot}"
        );
    }
}

#[test]
fn a_refusal_names_four_keys_and_counts_the_rest() {
    let keys = |n: usize| {
        let mut doc = constant_root();
        let entry = reference(&mut doc, "http://example.org#R.con", "S231:hasOutput", 0);
        for key in ["a", "b", "c", "d", "e", "f"].into_iter().take(n) {
            entry.insert(key.into(), json!(1));
        }
        refusal(&doc)
    };
    let expected = |named: &str| {
        vec![carries(
            "http://example.org#R.con",
            "S231:hasOutput",
            "http://example.org#R.con.y",
            named,
        )]
    };
    assert_eq!(keys(2), expected("the keys `a`, `b`"));
    assert_eq!(keys(4), expected("the keys `a`, `b`, `c`, `d`"));
    assert_eq!(keys(5), expected("the keys `a`, `b`, `c`, `d` and 1 more"));
    assert_eq!(keys(6), expected("the keys `a`, `b`, `c`, `d` and 2 more"));
}

#[test]
fn a_long_key_is_named_as_an_excerpt_and_the_id_whole() {
    // Each named key is cut to 64 bytes on a character boundary and marked `…` (GitHub #602); the
    // reference's canonical `@id` is the identity the message is about and is never cut.
    let id = format!("http://example.org#R.con.{}", "y".repeat(200));
    let mut doc = constant_root();
    let graph = doc["@graph"].as_array_mut().expect("@graph");
    graph[1]["S231:hasOutput"] =
        json!({ "@id": id, &format!("{}ü{}", "k".repeat(63), "z".repeat(1 << 20)): 1 });
    graph[3]["@id"] = json!(id);
    assert_eq!(
        refusal(&doc),
        vec![carries(
            "http://example.org#R.con",
            "S231:hasOutput",
            &id,
            &format!("the key `{}…`", "k".repeat(63)),
        )]
    );
}

#[test]
fn a_key_is_refused_whatever_its_value() {
    // `null`, an empty list and an empty object carry nothing in JSON-LD, but the key is still not
    // the engine's to drop; and a key spelled with `@` is refused like any other.
    for value in [json!(null), json!([]), json!({}), json!("")] {
        let mut doc = constant_root();
        reference(&mut doc, "http://example.org#R.con", "S231:hasParameter", 0)
            .insert("@index".into(), value.clone());
        assert_eq!(
            refusal(&doc),
            vec![carries(
                "http://example.org#R.con",
                "S231:hasParameter",
                "http://example.org#R.con.k",
                "the key `@index`"
            )],
            "{value}"
        );
    }
}

#[test]
fn identities_are_named_canonically_and_a_full_iri_property_key_is_read_too() {
    // Compact identities expand before the check, and a reference-bearing property keyed by its
    // full IRI is read as the property.
    let doc = json!({
        "@context": { "S231": S231_NS, "ex": "http://example.org#" },
        "@graph": [
            {
                "@id": "ex:R",
                "@type": "S231:Block",
                "http://data.ashrae.org/S231#containsBlock": [
                    { "@id": "ex:R.con", "@type": "S231:Block" }
                ]
            },
            {
                "@id": "ex:R.con",
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                "S231:hasParameter": { "@id": "ex:R.con.k" },
                "S231:hasOutput": { "@id": "ex:R.con.y" }
            },
            { "@id": "ex:R.con.k", "S231:value": 1.0 },
            {
                "@id": "ex:R.con.y",
                "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real", "S231:label": "Real" }
            }
        ]
    });
    assert_eq!(
        refusal(&doc),
        vec![
            carries(
                "http://example.org#R",
                "S231:containsBlock",
                "http://example.org#R.con",
                "the key `@type`"
            ),
            carries(
                "http://example.org#R.con.y",
                "S231:isOfDataType",
                "http://data.ashrae.org/S231#Real",
                "the key `S231:label`"
            ),
        ]
    );
}

#[test]
fn the_refusal_comes_with_the_index_findings_and_after_the_expansion_pass() {
    // The index step reports a duplicate `@id` beside it.
    let mut doc = constant_root();
    reference(&mut doc, "http://example.org#R", "S231:containsBlock", 0)
        .insert("@index".into(), json!("0"));
    let duplicate = node(&mut doc, "http://example.org#R.con.k").clone();
    doc["@graph"]
        .as_array_mut()
        .expect("@graph")
        .push(duplicate.into());
    assert_eq!(
        refusal(&doc),
        vec![
            carries(
                "http://example.org#R",
                "S231:containsBlock",
                "http://example.org#R.con",
                "the key `@index`"
            ),
            Diagnostic::error(
                DiagCode::DuplicateId,
                "duplicate @id `http://example.org#R.con.k` on @graph nodes 2 and 4",
            )
            .with_subject("http://example.org#R.con.k"),
        ]
    );

    // A scoped `@context` in the reference, or an `@id` the context cannot canonicalize, is
    // refused by the expansion pass, whose refusals return alone.
    let mut scoped = constant_root();
    let entry = reference(&mut scoped, "http://example.org#R", "S231:containsBlock", 0);
    entry.insert("@context".into(), json!({}));
    entry.insert("@index".into(), json!("0"));
    let diags = refusal(&scoped);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct);
    assert!(diags[0].message.contains("scoped `@context`"), "{diags:?}");

    let mut relative = constant_root();
    let entry = reference(
        &mut relative,
        "http://example.org#R",
        "S231:containsBlock",
        0,
    );
    entry.insert("@id".into(), json!("R.con"));
    entry.insert("@index".into(), json!("0"));
    let diags = refusal(&relative);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::RelativeIri);
}

#[test]
fn every_refusal_in_a_document_is_reported_in_a_stable_order() {
    let mut doc = constant_root();
    for (owner, slot) in [
        ("http://example.org#R.con.y", "S231:isOfDataType"),
        ("http://example.org#R.con", "S231:hasOutput"),
        ("http://example.org#R.con", "S231:hasParameter"),
        ("http://example.org#R", "S231:containsBlock"),
    ] {
        reference(&mut doc, owner, slot, 0).insert("@index".into(), json!("0"));
    }
    let expected = vec![
        carries(
            "http://example.org#R",
            "S231:containsBlock",
            "http://example.org#R.con",
            "the key `@index`",
        ),
        carries(
            "http://example.org#R.con",
            "S231:hasOutput",
            "http://example.org#R.con.y",
            "the key `@index`",
        ),
        carries(
            "http://example.org#R.con",
            "S231:hasParameter",
            "http://example.org#R.con.k",
            "the key `@index`",
        ),
        carries(
            "http://example.org#R.con.y",
            "S231:isOfDataType",
            "http://data.ashrae.org/S231#Real",
            "the key `@index`",
        ),
    ];
    let first = refusal(&doc);
    assert_eq!(first, expected);
    for _ in 0..8 {
        assert_eq!(refusal(&doc), first);
    }
}

/// The fixture's `@context` as a term table: a map, or a list of maps merged in order.
fn context_table(doc: &Json) -> Map<String, Json> {
    let mut table = Map::new();
    let maps = match &doc["@context"] {
        Json::Array(maps) => maps.clone(),
        other => vec![other.clone()],
    };
    for map in maps {
        if let Json::Object(map) = map {
            table.extend(map);
        }
    }
    table
}

/// `token` read against `table` as JSON-LD reads an identity: a whole term, then a compact IRI
/// through a declared prefix, else as written.
fn expanded(token: &str, table: &Map<String, Json>) -> String {
    if let Some(Json::String(iri)) = table.get(token) {
        return iri.clone();
    }
    if let Some((prefix, suffix)) = token.split_once(':')
        && !suffix.starts_with("//")
        && let Some(Json::String(iri)) = table.get(prefix)
    {
        return format!("{iri}{suffix}");
    }
    token.to_owned()
}

/// Every git-tracked `.jsonld` under `crates/` and `third_party/`.
fn corpus() -> Vec<std::path::PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let output = Command::new("git")
        .args(["ls-files", "-z", "--", "crates", "third_party"])
        .current_dir(&root)
        .output()
        .expect("enumerate tracked files");
    assert!(output.status.success(), "git ls-files failed");
    let mut paths: Vec<_> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter_map(|path| std::str::from_utf8(path).ok())
        .filter(|rel| rel.ends_with(".jsonld"))
        .map(|rel| root.join(rel))
        .collect();
    paths.sort();
    paths
}

#[test]
fn in_every_loading_fixture_the_first_reference_of_each_property_is_refused_with_a_key() {
    // For every tracked fixture that loads, give the first compact-keyed reference of each of the
    // eight properties one key beside its `@id`: the document is refused with exactly one
    // diagnostic per reference so changed.
    let mut loading = 0;
    let mut covered = std::collections::BTreeMap::<&str, usize>::new();
    for path in corpus() {
        let bytes = std::fs::read(&path).expect("fixture readable");
        if import_cxf(&bytes, &ResolveOptions::default()).is_err() {
            continue;
        }
        loading += 1;
        let mut doc: Json = serde_json::from_slice(&bytes).expect("a loading fixture parses");
        let table = context_table(&doc);
        let mut expected = Vec::new();
        for slot in REFERENCE_SLOTS {
            let key = match slot {
                "S231:containsBlock" => "@type",
                "S231:hasParameter" | "S231:hasConstant" => "S231:value",
                _ => "@index",
            };
            let graph = doc["@graph"].as_array_mut().expect("@graph");
            let Some(owner) = graph.iter_mut().find_map(|node| {
                let owner = node["@id"].as_str()?.to_owned();
                let entry = match node.get_mut(slot)? {
                    Json::Array(items) => items.first_mut()?,
                    single => single,
                };
                let entry = entry.as_object_mut()?;
                let id = entry.get("@id")?.as_str()?.to_owned();
                entry.insert(key.into(), json!(99));
                Some((owner, id))
            }) else {
                continue;
            };
            *covered.entry(slot).or_default() += 1;
            let (owner, id) = owner;
            expected.push(carries(
                &expanded(&owner, &table),
                slot,
                &expanded(&id, &table),
                &format!("the key `{key}`"),
            ));
        }
        expected.sort_by(|a, b| {
            (a.subject.as_deref(), &a.message).cmp(&(b.subject.as_deref(), &b.message))
        });
        assert_eq!(refusal(&doc), expected, "{}", path.display());
    }
    // The census pins 127 loading fixtures; every property is exercised by at least one.
    assert_eq!(loading, 127, "loading fixtures");
    assert_eq!(covered.len(), REFERENCE_SLOTS.len(), "{covered:?}");
}
