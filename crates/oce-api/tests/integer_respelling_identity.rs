//! Respelling an integral Real literal as an Integer changes nothing (issue #735): for every
//! tracked CXF document, the document with every integral Real `S231:value` literal written as
//! an Integer (a bare JSON integer, or a typed `xsd:integer`) loads to the same content id, the
//! same compiled-schedule blob and the same parameter table, or is refused with the same
//! diagnostics.
//!
//! The literals respelled are bare JSON reals with no fraction (`2.0`, `1e3`), typed
//! `xsd:double`/`float`/`decimal` literals with no fraction, and the same as elements of a
//! preserved array value, each at most 2^53 in magnitude. A negative zero is a Real no Integer
//! spells and is left alone, as are expressions. Each document is compared with itself parsed and
//! written back unchanged, so the comparison sees the respelling and nothing the JSON round trip
//! does. A document the JSON reader refuses is skipped (the engine refuses it unchanged).
//!
//! One kind of document is told apart, as the rule requires: one the base refuses because a Real
//! sits on an Integer parameter (`parameter-kind-mismatch`); respelled, that Real is an Integer and
//! may load. None is tracked today, and the test fails if one appears so it is looked at.

use std::fs;
use std::path::{Path, PathBuf};

use oce_api::Engine;
use serde_json::{Map, Number, Value as Json};

const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const MAX_EXACT: f64 = 9_007_199_254_740_992.0;

/// Every tracked `@graph` document under `crates/` and `third_party/` (build output excluded).
fn tracked_documents() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut found = Vec::new();
    let mut pending = vec![root.join("crates"), root.join("third_party")];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if path.is_dir() {
                if name != "target" {
                    pending.push(path);
                }
            } else if path
                .extension()
                .is_some_and(|extension| extension == "json" || extension == "jsonld")
                && fs::read_to_string(&path).is_ok_and(|text| text.contains("\"@graph\""))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// How an integral Real is written as an Integer.
#[derive(Clone, Copy, Debug)]
enum Respelling {
    Bare,
    Typed,
}

/// The Integer an integral Real `x` is, when the rule respells it.
fn integral(x: f64) -> Option<i64> {
    (x.fract() == 0.0 && x.abs() <= MAX_EXACT && !(x == 0.0 && x.is_sign_negative()))
        .then_some(x as i64)
}

fn integer_literal(integer: i64, respelling: Respelling) -> Json {
    match respelling {
        Respelling::Bare => Json::Number(Number::from(integer)),
        Respelling::Typed => {
            let mut object = Map::new();
            object.insert("@value".into(), Json::String(integer.to_string()));
            object.insert("@type".into(), Json::String(XSD_INTEGER.into()));
            Json::Object(object)
        }
    }
}

/// `value` (an `S231:value`) with every integral Real literal respelled; the count of literals
/// respelled is added to `count`.
fn respell_value(value: &mut Json, respelling: Respelling, count: &mut usize) {
    match value {
        Json::Number(number) if number.is_f64() => {
            if let Some(integer) = number.as_f64().and_then(integral) {
                *value = integer_literal(integer, respelling);
                *count += 1;
            }
        }
        Json::Array(elements) => {
            for element in elements {
                respell_value(element, respelling, count);
            }
        }
        Json::Object(object) => {
            let real_typed = object
                .get("@type")
                .and_then(Json::as_str)
                .is_some_and(|datatype| {
                    ["#double", "#float", "#decimal"]
                        .iter()
                        .any(|suffix| datatype.ends_with(suffix))
                });
            let integer = object
                .get("@value")
                .and_then(Json::as_str)
                .and_then(|lexical| lexical.trim().parse::<f64>().ok())
                .and_then(integral);
            if let (true, Some(integer), 2) = (real_typed, integer, object.len()) {
                *value = integer_literal(integer, Respelling::Typed);
                *count += 1;
            }
        }
        _ => {}
    }
}

/// The document with every node's `S231:value` respelled, and how many literals moved.
fn respelled(document: &Json, respelling: Respelling) -> (Json, usize) {
    let mut document = document.clone();
    let mut count = 0;
    if let Some(Json::Array(nodes)) = document.get_mut("@graph") {
        for node in nodes {
            if let Json::Object(object) = node {
                for (key, value) in object.iter_mut() {
                    if key == "S231:value" || key.ends_with("S231#value") {
                        respell_value(value, respelling, &mut count);
                    }
                }
            }
        }
    }
    (document, count)
}

/// A loaded document: its id and blob, and its block parameters split by whether the block's
/// catalog types them, each `block.name=value` with a Real by its bits.
#[derive(Debug, PartialEq)]
struct Loaded {
    id: String,
    blob: Vec<u8>,
    typed: Vec<String>,
    untyped: Vec<String>,
}

/// What loading `bytes` gives: the loaded document, or the refusal whole.
fn outcome(bytes: &[u8]) -> Result<Loaded, Vec<String>> {
    let mut engine = Engine::new();
    match engine.load_cxf(bytes) {
        Ok((_, loaded)) => {
            let mut typed = Vec::new();
            let mut untyped = Vec::new();
            for block in &loaded.model.blocks {
                for (name, value) in &block.params {
                    let row = format!("{:?}.{name}={value:?}", block.key);
                    if oce_blocks::parameter_value_type(&block.class_iri, name).is_some() {
                        typed.push(row);
                    } else {
                        untyped.push(row);
                    }
                }
            }
            Ok(Loaded {
                id: engine
                    .content_id()
                    .map(|id| id.as_str().to_owned())
                    .unwrap_or_default(),
                blob: engine
                    .compile_blob()
                    .map(|blob| blob.bytes)
                    .unwrap_or_default(),
                typed,
                untyped,
            })
        }
        Err(error) => Err(error
            .diagnostics()
            .iter()
            .map(|diagnostic| {
                format!(
                    "{}|{}|{}",
                    diagnostic.code.as_str(),
                    diagnostic.subject.as_deref().unwrap_or(""),
                    diagnostic.message
                )
            })
            .collect()),
    }
}

#[test]
fn every_tracked_document_is_one_model_whether_its_integral_reals_are_written_as_integers() {
    let documents = tracked_documents();
    assert!(documents.len() > 200, "{} documents", documents.len());
    let (mut respelled_documents, mut literals, mut loading) = (0usize, 0usize, 0usize);
    for path in &documents {
        let Ok(document) = serde_json::from_slice::<Json>(&fs::read(path).expect("readable"))
        else {
            continue;
        };
        let written_back = serde_json::to_vec(&document).expect("serialises");
        let base = outcome(&written_back);
        let kind_refused = base.as_ref().err().is_some_and(|diags| {
            diags
                .iter()
                .any(|d| d.starts_with("parameter-kind-mismatch|"))
        });
        for respelling in [Respelling::Bare, Respelling::Typed] {
            let (document, count) = respelled(&document, respelling);
            if count == 0 {
                continue;
            }
            assert!(
                !kind_refused,
                "{}: refused for a parameter's kind, so a respelling may load it; look at it",
                path.display()
            );
            literals += count;
            respelled_documents += 1;
            let bytes = serde_json::to_vec(&document).expect("serialises");
            let what = format!(
                "{} respelled {respelling:?} ({count} literals)",
                path.display()
            );
            assert_eq!(&outcome(&bytes), &base, "{what}");
        }
        // A parameter the catalog does not type is a name the class does not declare, which
        // the load gate refuses (issue #748): no loading document carries one, so no value
        // escapes the catalog's types and the comparison above covers every parameter.
        if let Ok(loaded) = &base {
            assert_eq!(loaded.untyped, Vec::<String>::new(), "{}", path.display());
        }
        loading += usize::from(base.is_ok());
    }
    // The sweep is not vacuous: most tracked documents write some integral Real.
    assert!(
        respelled_documents > 100 && literals > 1000 && loading > 100,
        "{respelled_documents} respellings, {literals} literals, {loading} loading documents"
    );
}

#[test]
fn the_respelling_moves_exactly_the_integral_real_literals() {
    let document: Json = serde_json::from_str(
        r#"{ "@graph": [
            { "@id": "a", "S231:value": 2.0 },
            { "@id": "b", "S231:value": 2.5 },
            { "@id": "c", "S231:value": -0.0 },
            { "@id": "d", "S231:value": 1e3 },
            { "@id": "e", "S231:value": [0.0, 1.5, 3] },
            { "@id": "f", "S231:value": { "@value": "4.0", "@type": "http://www.w3.org/2001/XMLSchema#double" } },
            { "@id": "g", "S231:value": "2.0" },
            { "@id": "h", "S231:value": 9007199254740994.0 },
            { "@id": "i", "S231:min": 2.0 }
        ] }"#,
    )
    .expect("json");
    let (bare, count) = respelled(&document, Respelling::Bare);
    assert_eq!(count, 4);
    let values: Vec<String> = bare["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| {
            node.get("S231:value")
                .map(Json::to_string)
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(
        values,
        [
            "2",
            "2.5",
            "-0.0",
            "1000",
            "[0,1.5,3]",
            r#"{"@type":"http://www.w3.org/2001/XMLSchema#integer","@value":"4"}"#,
            r#""2.0""#,
            "9007199254740994.0",
            ""
        ]
    );
    let (typed, count) = respelled(&document, Respelling::Typed);
    assert_eq!(count, 4);
    assert_eq!(
        typed["@graph"][0]["S231:value"].to_string(),
        r#"{"@type":"http://www.w3.org/2001/XMLSchema#integer","@value":"2"}"#
    );
}
