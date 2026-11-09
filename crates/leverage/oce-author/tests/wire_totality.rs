//! Totality of the wire form over validate-clean models: a model `validate` accepts always
//! writes a document `from_wire_str` reads back to the same model (within the list and document
//! bounds), over every fixture and an enumerated set of string values; text-form annotations are
//! never written; and the String bounds that would break this are findings.

mod support;

use oce_author::{
    Annotations, Block, Boundary, Connection, Endpoint, Literal, MAX_WIRE_STRING_BYTES, Sequence,
    SignalType, from_wire_str, to_wire_string, validate,
};

use support::{codes, fixture, id};

fn round_trips(sequence: &Sequence) {
    let text = to_wire_string(sequence);
    let read = from_wire_str(&text)
        .unwrap_or_else(|e| panic!("a clean model must read back: {e}\n{text}"));
    assert_eq!(&read, sequence);
    assert_eq!(to_wire_string(&read), text);
}

#[test]
fn every_clean_fixture_round_trips_through_the_model() {
    let mut clean = 0;
    for entry in std::fs::read_dir(fixture("wire")).expect("fixtures") {
        let path = entry.expect("entry").path();
        let model = from_wire_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
        if validate(&model).is_empty() {
            clean += 1;
            round_trips(&model);
        }
    }
    assert!(clean >= 2, "the clean fixtures are present");
}

/// A clean sequence whose `CDL.Utilities.Assert` message is `message`.
fn with_message(message: &str) -> Sequence {
    let mut s = Sequence::new(id("texts"));
    s.inputs.push(Boundary::new(id("u"), SignalType::Boolean));
    let mut assert = Block::new(id("ass"), "CDL.Utilities.Assert");
    assert.assignments.push(oce_author::Assignment::new(
        id("message"),
        Literal::Text(message.to_owned()),
    ));
    s.blocks.push(assert);
    s.connections.push(Connection::new(
        Endpoint::parse("u"),
        Endpoint::parse("ass.u"),
    ));
    s
}

/// Strings over the characters the writer escapes differently, at and around the bound.
fn generated_messages() -> Vec<String> {
    let mut out = vec![
        String::new(),
        "plain".to_owned(),
        "line\nbreak".to_owned(),
        "quote \" and backslash \\".to_owned(),
        "degree \u{b0}C, euro \u{20ac}, astral \u{1f525}".to_owned(),
        "/slash and {braces} [ ] , : ->".to_owned(),
    ];
    for (unit, cost) in [("a", 1), ("\"", 2), ("\u{e9}", 6), ("\u{1f525}", 12)] {
        let fit = MAX_WIRE_STRING_BYTES / cost;
        out.push(unit.repeat(fit));
        out.push(unit.repeat(fit + 1));
    }
    out
}

#[test]
fn generated_string_values_are_clean_and_total_or_flagged() {
    let (mut clean, mut flagged) = (0, 0);
    for message in generated_messages() {
        let s = with_message(&message);
        let findings = validate(&s);
        if findings.is_empty() {
            clean += 1;
            round_trips(&s);
        } else {
            flagged += 1;
            assert_eq!(
                codes(&findings),
                vec![("parameter-out-of-range", "ass.message".to_owned())]
            );
            assert!(
                findings[0].detail.contains("bytes as written"),
                "{}",
                findings[0].detail
            );
            assert!(
                from_wire_str(&to_wire_string(&s)).is_err(),
                "flagged because the reader refuses it"
            );
        }
    }
    assert_eq!((clean, flagged), (10, 4));
}

#[test]
fn control_characters_in_a_string_value_are_flagged() {
    for c in ['\t', '\r', '\u{0}', '\u{1b}', '\u{7f}', '\u{85}'] {
        let s = with_message(&format!("a{c}b"));
        let findings = validate(&s);
        assert_eq!(
            codes(&findings),
            vec![("parameter-out-of-range", "ass.message".to_owned())],
            "{c:?}"
        );
        assert!(findings[0].detail.contains("control character"));
        assert!(from_wire_str(&to_wire_string(&s)).is_err());
    }
}

#[test]
fn annotations_and_suffixes_never_reach_the_wire() {
    let plain = support::econ_enable();
    let mut annotated = plain.clone();
    let mut notes = Annotations::default();
    notes.comments = vec![" Economizer enable.".to_owned()];
    notes.blank_line_before = true;
    annotated.params[0].annotations = notes.clone();
    annotated.params[0].suffix = Some("K".to_owned());
    annotated.inputs[0].annotations = notes.clone();
    annotated.outputs[0].annotations = notes.clone();
    annotated.blocks[1].annotations = notes.clone();
    annotated.blocks[1].assignments[0].annotations = notes.clone();
    annotated.blocks[1].assignments[0].suffix = Some("K".to_owned());
    annotated.connections[0].annotations = notes;
    assert_ne!(
        annotated, plain,
        "equality is structural and sees annotations"
    );
    assert_eq!(to_wire_string(&annotated), to_wire_string(&plain));
    assert_eq!(validate(&annotated), validate(&plain));
    assert_eq!(
        from_wire_str(&to_wire_string(&annotated)).as_ref(),
        Ok(&plain)
    );
}

#[test]
fn too_many_haystack_tags_are_flagged() {
    // At most `MAX_HAYSTACK_TAGS` (16, amended 2026-11-09 for the text form's line bound, issue
    // #482).
    let mut s = support::econ_enable();
    s.inputs[0].semantic.haystack = vec!["t".to_owned(); oce_author::MAX_HAYSTACK_TAGS];
    assert!(validate(&s).is_empty(), "accepted at the limit");
    round_trips(&s);
    s.inputs[0].semantic.haystack.push("t".to_owned());
    assert_eq!(
        codes(&validate(&s)),
        vec![("invalid-semantic-point", "TOut".to_owned())]
    );
}
