//! The fixed refusal precedence and the out-of-subset kinds (importer note, "`ImportError`",
//! "Refusal precedence"; decision 16; "Test plan", hostile corpus): each out-of-subset construct
//! grafted onto an importable corpus fixture is refused with its own kind, and a document with
//! several is refused for the first of nested composite, conditional component, expression
//! value, port attribute and name, whatever its node order.

mod support;

use oce_author::{ImportOptions, import_cxf};
use serde_json::json;

use support::cxf::{REFUSED, corpus_fixture};
use support::edit::{Document, reference};

/// The importable fixture the grafts go onto: root parameters, boundaries, blocks with
/// parameters and ports.
const HOST: &str = "multizone_vav_freeze_protection";

fn host() -> Document {
    let document = Document::read(&corpus_fixture(HOST));
    assert!(import_cxf(&document.bytes(), &ImportOptions::new()).is_ok());
    document
}

/// The code and subject `document` is refused with.
fn refusal(document: &Document) -> (&'static str, String) {
    match import_cxf(&document.bytes(), &ImportOptions::new()) {
        Err(error) => (error.code(), error.subject),
        Ok(_) => ("imported", String::new()),
    }
}

/// An edit that grafts one construct onto a document and returns the node it sits on.
type Graft = Box<dyn Fn(&mut Document) -> String>;

/// One graft per out-of-subset construct: the kind it is refused with and where.
fn grafts() -> Vec<(&'static str, Graft)> {
    vec![
        (
            "nested-composite",
            Box::new(|d: &mut Document| {
                let (block, _) = d.first_block_parameter();
                d.put(&block, "S231:containsBlock", json!([]));
                block
            }),
        ),
        (
            "conditional-component",
            Box::new(|d: &mut Document| {
                let (block, _) = d.first_block_parameter();
                d.put(&block, "S231:isConditionalComponent", json!(true));
                block
            }),
        ),
        (
            "expression-value",
            Box::new(|d: &mut Document| {
                let (_, param) = d.first_block_parameter();
                d.put(&param, "S231:value", json!("2 * k"));
                param
            }),
        ),
        (
            "port-attribute",
            Box::new(|d: &mut Document| {
                let port = d.first_port();
                d.put(&port, "S231:max", json!(1));
                port
            }),
        ),
        (
            "invalid-name",
            Box::new(|d: &mut Document| {
                let bad = format!("{}.bad-name", d.root());
                d.prepend(json!({ "@id": bad }));
                bad
            }),
        ),
        (
            "instance-member",
            Box::new(|d: &mut Document| {
                let (block, _) = d.first_block_parameter();
                d.put(&block, "S231:hasInstance", json!([]));
                block
            }),
        ),
        (
            "constant-member",
            Box::new(|d: &mut Document| {
                let root = d.root();
                d.put(&root, "S231:hasConstant", json!([]));
                root
            }),
        ),
        (
            "array-connector",
            Box::new(|d: &mut Document| {
                let input = d.first("S231:hasInput");
                d.put(&input, "S231:isArray", json!(true));
                input
            }),
        ),
        (
            "array-sequence-parameter",
            Box::new(|d: &mut Document| {
                let param = d.first("S231:hasParameter");
                d.put(&param, "S231:isArray", json!(true));
                param
            }),
        ),
        (
            "class-outside-obc",
            Box::new(|d: &mut Document| {
                let (block, _) = d.first_block_parameter();
                d.put(
                    &block,
                    "@type",
                    json!("http://example.org#Modelica.Blocks.Math.Add"),
                );
                block
            }),
        ),
        (
            "driving-sink",
            Box::new(|d: &mut Document| {
                let output = d.first("S231:hasOutput");
                let input = d.first("S231:hasInput");
                d.put(&output, "S231:isConnectedTo", json!([reference(&input)]));
                output
            }),
        ),
    ]
}

#[test]
fn each_out_of_subset_construct_grafted_onto_an_importable_fixture_has_its_kind() {
    for (code, graft) in grafts() {
        let mut document = host();
        let subject = graft(&mut document);
        assert_eq!(refusal(&document), (code, subject), "{code}");
    }
}

#[test]
fn several_constructs_are_refused_for_the_first_in_the_fixed_order() {
    // All five classes at once, then each removed in turn: the refusal walks the order. The
    // name graft sits first in `@graph` and the nested composite's block comes after the
    // others, so node order would give the reverse.
    let order = [
        "nested-composite",
        "conditional-component",
        "expression-value",
        "port-attribute",
        "invalid-name",
    ];
    let all = grafts();
    for start in 0..order.len() {
        let mut document = host();
        // Apply the later classes first, so the earliest class is the last graft written.
        for code in order[start..].iter().rev() {
            let (_, graft) = all.iter().find(|(c, _)| c == code).expect("a graft");
            graft(&mut document);
        }
        assert_eq!(refusal(&document).0, order[start], "from {}", order[start]);
    }
}

#[test]
fn the_notes_examples_follow_the_order() {
    // `multizone_vav_supply_temperature` is nested, conditional, has `hasConstant` and port
    // bounds; `…_fixed_21` is nested with a 72-byte sequence name; `trim_and_respond_have_hol_false`
    // is conditional with an expression.
    for (name, code) in [
        ("multizone_vav_supply_temperature", "nested-composite"),
        (
            "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21",
            "nested-composite",
        ),
        ("trim_and_respond_have_hol_false", "conditional-component"),
    ] {
        assert!(REFUSED.contains(&(name, code)), "{name}");
        let document = Document::read(&corpus_fixture(name));
        assert_eq!(refusal(&document).0, code, "{name}");
    }
}

#[test]
fn a_sequence_name_longer_than_an_identifier_is_refused_never_truncated() {
    let text = std::fs::read_to_string(corpus_fixture(HOST)).expect("fixture");
    let document = Document::parse(&text);
    let root = document.root();
    let name = root.rsplit('.').next().expect("name").to_owned();
    for (length, code) in [(64, "imported"), (65, "invalid-name")] {
        let long = format!("{name}{}", "x".repeat(length - name.len()));
        let renamed = text.replace(&root, &root.replace(&name, &long));
        let read = import_cxf(renamed.as_bytes(), &ImportOptions::new());
        match (code, read) {
            ("imported", Ok(read)) => assert_eq!(read.sequence.name.as_str(), long),
            (code, Err(error)) => assert_eq!(error.code(), code),
            (code, Ok(_)) => panic!("{length}: expected {code}"),
        }
    }
}

#[test]
fn a_reserved_word_is_carried_for_the_validator() {
    // Decision 13: names are carried, never repaired; the validator reports them. (A duplicate
    // name cannot be spelled in CXF: two declarations of one name are one `@id`, `duplicate-id`.)
    let text =
        std::fs::read_to_string(support::fixture("emit/econ_enable.cxf.json")).expect("golden");
    let renamed = text.replace("econ_enable.sub", "econ_enable.model");
    let read = import_cxf(renamed.as_bytes(), &ImportOptions::new()).expect("carried");
    assert!(
        read.sequence
            .blocks
            .iter()
            .any(|b| b.name.as_str() == "model")
    );
    let codes: Vec<&str> = oce_author::validate_author_side(&read.sequence)
        .iter()
        .map(|f| f.code)
        .collect();
    assert!(codes.contains(&"invalid-name"), "{codes:?}");
}
