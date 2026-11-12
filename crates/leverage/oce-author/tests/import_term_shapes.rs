//! The CXF importer refuses a term (`S231:unit`, `S231:quantity`, `S231:displayUnit`) of the wrong
//! shape as `wrong-shape` naming the key, at the node that carries it, on every role it reads one
//! (GitHub issue #602; owner ruling of 2026-11-12). A boundary's term is a string that is not empty
//! or whitespace-only: before #602 a blank one imported, and only `validate` reported it, as
//! `invalid-semantic-point`. A parameter's term was already held to the exact shapes since #559:
//! these tests pin the shapes the ruling names there too. Block ports carry no attribute here.

use oce_author::{ImportError, ImportErrorKind, ImportOptions, SourceForm, import_cxf};
use serde_json::{Value as Json, json};

use crate::support::fixture;

const BOUNDARY: &str = "urn:oce:author:econ_enable.TOut";
const PARAMETER: &str = "urn:oce:author:econ_enable.dTHys";
const BLOCK_PARAMETER: &str = "urn:oce:author:econ_enable.hys.uLow";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const TERMS: [&str; 3] = ["S231:unit", "S231:quantity", "S231:displayUnit"];

fn with(id: &str, key: &str, value: &Json) -> Json {
    let text = std::fs::read_to_string(fixture("emit/econ_enable.cxf.json")).expect("golden");
    let mut doc: Json = serde_json::from_str(&text).expect("golden parses");
    let node = doc["@graph"]
        .as_array_mut()
        .expect("@graph")
        .iter_mut()
        .find(|node| node["@id"] == id)
        .unwrap_or_else(|| panic!("no node `{id}`"));
    node[key] = value.clone();
    doc
}

fn import(doc: &Json) -> Result<(), ImportError> {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    import_cxf(&bytes, &ImportOptions::new()).map(|_| ())
}

fn refusal(doc: &Json) -> (String, ImportErrorKind) {
    match import(doc) {
        Err(error) => {
            assert_eq!(error.source, SourceForm::Cxf);
            (error.subject, error.kind)
        }
        Ok(()) => panic!("expected a refusal, but it imported"),
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

#[test]
fn a_well_shaped_term_imports() {
    for key in TERMS {
        import(&with(BOUNDARY, key, &json!("K"))).expect("a boundary's string term");
        for node in [PARAMETER, BLOCK_PARAMETER] {
            for value in [
                json!("K"),
                json!({ "@value": "K", "@type": XSD_STRING }),
                json!({ "@id": "unit:K" }),
            ] {
                import(&with(node, key, &value)).unwrap_or_else(|e| panic!("{node} {key}: {e}"));
            }
        }
    }
}

#[test]
fn a_blank_boundary_term_is_refused_at_import() {
    for key in TERMS {
        for value in [json!(""), json!(" "), json!("\t\n"), json!("\u{a0}")] {
            assert_eq!(
                refusal(&with(BOUNDARY, key, &value)),
                wrong_shape(BOUNDARY, key),
                "{key} {value}"
            );
        }
    }
}

#[test]
fn every_ruled_shape_is_refused_on_every_role() {
    let shapes = [
        // Blank, in each of the three shapes.
        json!(""),
        json!({ "@value": " ", "@type": XSD_STRING }),
        json!({ "@id": "" }),
        // Both `@id` and `@value`, with and without `@type`.
        json!({ "@id": "unit:K", "@value": "degC", "@type": XSD_STRING }),
        json!({ "@id": "unit:K", "@value": "degC" }),
        // A key beside the shape.
        json!({ "@value": "K", "@type": XSD_STRING, "@language": "en" }),
        json!({ "@id": "unit:K", "qudt:symbol": "K" }),
    ];
    for key in TERMS {
        for value in &shapes {
            for node in [BOUNDARY, PARAMETER, BLOCK_PARAMETER] {
                assert_eq!(
                    refusal(&with(node, key, value)),
                    wrong_shape(node, key),
                    "{node} {key} {value}"
                );
            }
        }
    }
}
