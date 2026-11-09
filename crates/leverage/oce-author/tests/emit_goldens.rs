//! Emitter byte goldens (authoring note, "Emitter" and "Determinism tests"): the worked example,
//! the every-element sequence and every corpus fixture the importer covers, emitted with the
//! default namespace and compared bit for bit with `tests/fixtures/emit/`; each golden loads
//! warning-free (acceptance precondition 4), emits twice to the same bytes and loads under an
//! overridden namespace too.

mod support;

use oce_author::{EmitOptions, Sequence, emit, import_cxf};

use support::cxf::{corpus_fixture, load_clean};
use support::{compare_or_bless, econ_enable, every_element, fixture};

/// The corpus fixtures the importer covers (see `library_round_trip.rs`).
const CORPUS: &[&str] = &[
    "cooling_only_system_requests",
    "generic_time_suppression",
    "multizone_vav_relief_fan_group",
    "reheat_overrides",
];

fn goldens() -> Vec<(String, Sequence)> {
    let mut all = vec![
        ("econ_enable".to_owned(), econ_enable()),
        ("every_element".to_owned(), every_element()),
    ];
    for name in CORPUS {
        let bytes = std::fs::read(corpus_fixture(name)).expect("read fixture");
        all.push(((*name).to_owned(), import_cxf(&bytes).expect("import")));
    }
    all
}

fn emitted(sequence: &Sequence, options: &EmitOptions) -> String {
    String::from_utf8(emit(sequence, options).expect("emit")).expect("UTF-8")
}

#[test]
fn every_golden_matches_its_checked_in_bytes() {
    for (name, sequence) in goldens() {
        let text = emitted(&sequence, &EmitOptions::new());
        compare_or_bless(&fixture(&format!("emit/{name}.jsonld")), &text);
    }
}

#[test]
fn every_golden_loads_warning_free() {
    for (name, _) in goldens() {
        let text = std::fs::read(fixture(&format!("emit/{name}.jsonld"))).expect("golden");
        load_clean(&text);
    }
}

#[test]
fn emitting_twice_yields_identical_bytes() {
    for (name, sequence) in goldens() {
        let options = EmitOptions::new();
        assert_eq!(
            emit(&sequence, &options).expect("emit"),
            emit(&sequence.clone(), &options).expect("emit"),
            "{name}"
        );
    }
}

#[test]
fn the_default_namespace_and_an_override_both_load() {
    for namespace in [
        oce_author::DEFAULT_NAMESPACE,
        "https://example.com/plant/ahu1/",
        "urn:site:b2:",
        "http://example.org/library#",
    ] {
        for (name, sequence) in goldens() {
            let text = emitted(&sequence, &EmitOptions::new().with_namespace(namespace));
            assert!(
                text.contains(&format!("\"{namespace}{}\"", sequence.name)),
                "{name} under {namespace}"
            );
            load_clean(text.as_bytes());
        }
    }
}

#[test]
fn the_worked_example_follows_the_notes_node_table() {
    let text = emitted(&econ_enable(), &EmitOptions::new());
    let value: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let nodes: Vec<(String, String)> = value["@graph"]
        .as_array()
        .expect("graph")
        .iter()
        .map(|n| {
            (
                n["@id"]
                    .as_str()
                    .expect("id")
                    .trim_start_matches("urn:oce:author:econ_enable")
                    .to_owned(),
                n["@type"].as_str().expect("type").to_owned(),
            )
        })
        .collect();
    let expected = [
        ("", "S231:Block"),
        (".dTHys", "S231:Parameter"),
        (
            ".sub",
            "http://example.org#Buildings.Controls.OBC.CDL.Reals.Subtract",
        ),
        (".sub.u1", "S231:RealInput"),
        (".sub.u2", "S231:RealInput"),
        (".sub.y", "S231:RealOutput"),
        (
            ".hys",
            "http://example.org#Buildings.Controls.OBC.CDL.Reals.Hysteresis",
        ),
        (".hys.uLow", "S231:Parameter"),
        (".hys.uHigh", "S231:Parameter"),
        (".hys.u", "S231:RealInput"),
        (".hys.y", "S231:BooleanOutput"),
        (".TOut", "S231:RealInput"),
        (".TOutCut", "S231:RealInput"),
        (".ecoEna", "S231:BooleanOutput"),
    ];
    let expected: Vec<(String, String)> = expected
        .iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect();
    assert_eq!(nodes, expected);
    // `dTHys` is Real with an Integer literal: written as a Real; `uHigh` is the bare reference.
    assert!(text.contains("\"@value\": \"1.0\""));
    assert!(text.contains("\"S231:value\": \"dTHys\""));
    // `@brick` and `@provisional` are not emitted (decision 2).
    assert!(!text.contains("Enable_Command") && !text.contains("provisional"));
}

#[test]
fn the_writer_is_ascii_with_one_trailing_newline() {
    let mut s = every_element();
    let message = s
        .blocks
        .iter_mut()
        .find(|b| b.name.as_str() == "assMes")
        .and_then(|b| b.assignments.first_mut())
        .expect("assMes.message");
    message.value = oce_author::ParamValue::Literal(oce_author::Literal::Text(
        "Temp\u{e9}rature \u{1f321}".to_owned(),
    ));
    let text = emitted(&s, &EmitOptions::new());
    assert!(text.is_ascii());
    assert!(text.contains("Temp\\u00e9rature \\ud83c\\udf21"), "{text}");
    assert!(text.ends_with("}\n") && !text.ends_with("\n\n"));
    assert!(!text.contains('\t'));
}

#[test]
fn text_form_annotations_and_suffixes_never_reach_the_bytes() {
    let plain = emitted(&econ_enable(), &EmitOptions::new());
    let mut annotated = econ_enable();
    annotated.params[0].suffix = Some("K".to_owned());
    annotated.params[0].annotations.comments = vec!["hysteresis band".to_owned()];
    annotated.blocks[1].annotations.blank_line_before = true;
    annotated.blocks[1].assignments[0].suffix = Some("K".to_owned());
    annotated.blocks[1].assignments[0].annotations.comments = vec!["low".to_owned()];
    annotated.connections[0].annotations.comments = vec!["cut-off first".to_owned()];
    annotated.extends = Some(
        oce_author::ProvenanceId::new("cxf:fnv1a128:00112233445566778899aabbccddeeff").expect("id"),
    );
    assert_ne!(annotated, econ_enable());
    assert_eq!(emitted(&annotated, &EmitOptions::new()), plain);
}
