//! The CXF importer reads a reference as exactly `{"@id": <string>}` and refuses one that carries
//! any other key as `wrong-shape`, naming the property, at the node that holds it (GitHub issue
//! #588, where the engine's reader dropped such keys without a word until it refused them too).
//! The importer has done this since its reference reader was written; these tests pin it for
//! every reference-bearing property it reads. It reads neither `S231:hasConstant` nor
//! `S231:hasInstance`: either property is outside its subset (`constant-member`,
//! `instance-member`) whatever its references carry.

use oce_author::{ImportError, ImportErrorKind, ImportOptions, SourceForm, import_cxf};
use serde_json::{Map, Value as Json, json};

use crate::support::fixture;

const ROOT: &str = "urn:oce:author:econ_enable";

fn document() -> Json {
    let text = std::fs::read_to_string(fixture("emit/econ_enable.cxf.json")).expect("golden");
    serde_json::from_str(&text).expect("golden parses")
}

fn import(doc: &Json) -> Result<(), ImportError> {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    import_cxf(&bytes, &ImportOptions::new()).map(|_| ())
}

/// The refusal of `doc`, as (subject, kind).
fn refusal(doc: &Json) -> (String, ImportErrorKind) {
    match import(doc) {
        Err(error) => {
            assert_eq!(error.source, SourceForm::Cxf);
            (error.subject, error.kind)
        }
        Ok(()) => panic!("expected a refusal, but it imported"),
    }
}

fn node<'a>(doc: &'a mut Json, id: &str) -> &'a mut Map<String, Json> {
    doc["@graph"]
        .as_array_mut()
        .expect("@graph")
        .iter_mut()
        .find(|node| node["@id"] == id)
        .and_then(Json::as_object_mut)
        .unwrap_or_else(|| panic!("no node `{id}`"))
}

/// The first reference object of `slot` on `owner`.
fn reference<'a>(doc: &'a mut Json, owner: &str, slot: &str) -> &'a mut Map<String, Json> {
    let value = node(doc, owner).get_mut(slot).expect("slot");
    let value = match value {
        Json::Array(items) => &mut items[0],
        single => single,
    };
    value.as_object_mut().expect("reference object")
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
fn every_reference_bearing_property_read_is_refused_with_a_key_beside_its_id() {
    import(&document()).expect("the golden imports as written");
    let at = |local: &str| format!("{ROOT}{local}");
    for (owner, slot, key) in [
        (at(""), "S231:hasParameter", "S231:value"),
        (at(""), "S231:hasInput", "@type"),
        (at(""), "S231:hasOutput", "@index"),
        (at(""), "S231:containsBlock", "S231:hasFmuPath"),
        (at(".hys"), "S231:hasParameter", "S231:value"),
        (at(".hys"), "S231:hasInput", "@index"),
        (at(".hys"), "S231:hasOutput", "@index"),
        (at(".hys.y"), "S231:isConnectedTo", "@index"),
        (at(".dTHys"), "S231:isOfDataType", "@type"),
        (at(".hys.u"), "S231:isOfDataType", "@type"),
        (at(".ecoEna"), "S231:isOfDataType", "@type"),
    ] {
        for value in [json!("x"), json!(null)] {
            let mut doc = document();
            reference(&mut doc, &owner, slot).insert(key.into(), value.clone());
            assert_eq!(
                refusal(&doc),
                wrong_shape(&owner, slot),
                "{owner} {slot} {key}: {value}"
            );
        }
    }
}

#[test]
fn a_containsblock_entry_typed_an_extension_block_is_refused_as_wrong_shape() {
    let mut doc = document();
    let entry = reference(&mut doc, ROOT, "S231:containsBlock");
    entry.insert("@type".into(), json!("S231:ExtensionBlock"));
    entry.insert("S231:hasFmuPath".into(), json!("sub.fmu"));
    assert_eq!(refusal(&doc), wrong_shape(ROOT, "S231:containsBlock"));
}

#[test]
fn the_properties_the_importer_does_not_read_are_refused_whatever_they_reference() {
    for (slot, kind) in [
        ("S231:hasConstant", ImportErrorKind::ConstantMember),
        ("S231:hasInstance", ImportErrorKind::InstanceMember),
    ] {
        for entry in [
            json!({ "@id": format!("{ROOT}.dTHys") }),
            json!({ "@id": format!("{ROOT}.dTHys"), "S231:value": 99 }),
        ] {
            let mut doc = document();
            node(&mut doc, &format!("{ROOT}.hys")).insert(slot.into(), entry.clone());
            assert_eq!(
                refusal(&doc),
                (format!("{ROOT}.hys"), kind.clone()),
                "{slot}: {entry}"
            );
        }
    }
}
