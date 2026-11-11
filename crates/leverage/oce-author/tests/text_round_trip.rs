//! Text-form round trips (issue #482; authoring note "Round-trip properties"): text → model →
//! text byte-stable for canonical text over every bundle and the note's worked example; model →
//! text → model over an enumerated model set; wire → text → wire byte equality for every bundle
//! (gate item 2) and for every imported corpus fixture (issue #603); the worked text example equal
//! to its wire example; and the header and trailing comments the text form added to the model
//! never reaching the wire form, the schema or the emitter.

use crate::support;

use oce_author::{
    EmitOptions, ImportOptions, Literal, ParamValue, PrintError, Sequence, WIRE_SCHEMA, emit,
    fmt_text, from_wire_str, import_cxf, parse_text, print_text, to_wire_string,
    validate_author_side, wire_schema,
};

use support::cxf::{IMPORTED, corpus_fixture};
use support::text::{bundle_model, bundles, enumerated_models, note_text_example, print};
use support::{compare_or_bless, econ_enable, fixture};

/// The model with every text-only fact removed: annotations, suffixes, header and trailing
/// comments. Two forms that differ only in those carry the same sequence.
fn without_text_facts(sequence: &Sequence) -> Sequence {
    let mut s = sequence.clone();
    s.annotations = Default::default();
    s.trailing_comments.clear();
    for p in &mut s.params {
        p.suffix = None;
        p.annotations = Default::default();
    }
    for b in s.inputs.iter_mut().chain(s.outputs.iter_mut()) {
        b.annotations = Default::default();
    }
    for b in &mut s.blocks {
        b.annotations = Default::default();
        for a in &mut b.assignments {
            a.suffix = None;
            a.annotations = Default::default();
        }
    }
    for c in &mut s.connections {
        c.annotations = Default::default();
    }
    s
}

#[test]
fn every_bundle_prints_to_its_checked_in_canonical_text() {
    for (name, wire) in bundles() {
        let model = bundle_model(&wire);
        let text = print(&model);
        compare_or_bless(&fixture(&format!("text/{name}.oce")), &text);
        let golden = std::fs::read_to_string(fixture(&format!("text/{name}.oce"))).expect("golden");
        let parsed = parse_text(&golden).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            parsed, model,
            "{name}: canonical text → model loses nothing"
        );
        assert_eq!(
            print(&parsed),
            golden,
            "{name}: text → model → text is byte-stable"
        );
    }
}

#[test]
fn wire_to_text_to_wire_is_byte_identical_for_every_bundle() {
    // Gate item 2.
    let all = bundles();
    assert_eq!(
        all.len(),
        7,
        "three wire bundles and four imported corpus fixtures"
    );
    for (name, wire) in all {
        let text = print(&from_wire_str(&wire).expect("reads"));
        let back = parse_text(&text).unwrap_or_else(|e| panic!("{name}: {e}\n{text}"));
        assert_eq!(to_wire_string(&back), wire, "{name}");
    }
}

/// Where one imported corpus fixture's wire → text → wire round trip first fails, naming the step
/// and, for a byte difference, the first line that differs; `None` when it holds: the fixture
/// imports, its wire form reads back, prints as text, the text parses to a model whose wire form is
/// the same bytes, and `fmt` leaves the text as it is.
fn text_round_trip_failure(name: &str) -> Option<String> {
    let bytes = match std::fs::read(corpus_fixture(name)) {
        Ok(bytes) => bytes,
        Err(error) => return Some(format!("read: {error}")),
    };
    let model = match import_cxf(&bytes, &ImportOptions::new()) {
        Ok(imported) => imported.sequence,
        Err(error) => return Some(format!("import: {error}")),
    };
    let wire = to_wire_string(&model);
    let read = match from_wire_str(&wire) {
        Ok(read) => read,
        Err(error) => return Some(format!("wire read: {error}")),
    };
    let text = match print_text(&read) {
        Ok(text) => text,
        Err(error) => return Some(format!("print: {error}")),
    };
    let parsed = match parse_text(&text) {
        Ok(parsed) => parsed,
        Err(error) => return Some(format!("parse: {error}")),
    };
    let again = to_wire_string(&parsed);
    if again != wire {
        return Some(first_difference("the wire form after text", &wire, &again));
    }
    match fmt_text(&text) {
        Ok(formatted) if formatted == text => None,
        Ok(formatted) => Some(first_difference("fmt", &text, &formatted)),
        Err(error) => Some(format!("fmt: {error}")),
    }
}

/// `what` differs: the first line (1-based) at which `actual` leaves `expected`, with both lines
/// (`None` past the end of the shorter one).
fn first_difference(what: &str, expected: &str, actual: &str) -> String {
    let (want, got): (Vec<&str>, Vec<&str>) =
        (expected.lines().collect(), actual.lines().collect());
    let at = want
        .iter()
        .zip(&got)
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| want.len().min(got.len()));
    format!(
        "{what} differs at line {}: expected {:?}, got {:?}",
        at + 1,
        want.get(at),
        got.get(at)
    )
}

#[test]
fn wire_to_text_to_wire_is_byte_identical_for_every_imported_corpus_fixture() {
    // Gate item 2 over the whole importable corpus (issue #603), not only the seven bundles above:
    // every fixture `library_round_trip` imports. Its partition test holds `IMPORTED` and `REFUSED`
    // to the 46 corpus files, so the list cannot shrink without failing there, and the count is
    // asserted here too.
    assert_eq!(IMPORTED.len(), 31, "the importable corpus fixtures");
    let failures: Vec<String> = IMPORTED
        .iter()
        .filter_map(|name| text_round_trip_failure(name).map(|why| format!("{name}: {why}")))
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} imported corpus fixtures fail wire -> text -> wire:\n{}",
        failures.len(),
        IMPORTED.len(),
        failures.join("\n")
    );
}

#[test]
fn a_text_round_trip_difference_is_reported_at_its_first_line() {
    // The control for the report above: the first differing line, both lines, and a length
    // difference reported where the shorter text ends.
    assert_eq!(
        first_difference("x", "a\nb\nc\n", "a\nB\nc\n"),
        "x differs at line 2: expected Some(\"b\"), got Some(\"B\")"
    );
    assert_eq!(
        first_difference("x", "a\nb\n", "a\n"),
        "x differs at line 2: expected Some(\"b\"), got None"
    );
    assert_eq!(text_round_trip_failure(IMPORTED[0]), None);
    assert!(
        text_round_trip_failure("no_such_fixture").is_some_and(|why| why.starts_with("read: ")),
        "a missing fixture is reported, not skipped"
    );
}

#[test]
fn the_defects_bundle_carries_values_no_text_spelling_has() {
    // `defects.json` holds a `unit` atom with a space (`deg C`) and a class path that is not
    // dotted words (`urn:oce:lowering#PassThrough.Real`), both refused by stage 1. Printing it is
    // refused with a typed error naming the first, never a panic and never a silent change.
    let wire = std::fs::read_to_string(fixture("wire/defects.json")).expect("defects");
    let error = print_text(&from_wire_str(&wire).expect("reads")).expect_err("unspellable");
    let PrintError::Unspellable { subject, detail } = &error else {
        panic!("{error}");
    };
    assert_eq!(subject, "TOut");
    assert!(detail.contains("deg C"), "{detail}");
}

#[test]
fn the_worked_text_example_is_canonical() {
    let text = note_text_example();
    let model = parse_text(&text).expect("the note's example parses");
    assert_eq!(print(&model), text);
    assert_eq!(
        model.annotations.comments,
        vec![" Economizer enable on outdoor-air temperature with a deadband.".to_owned()]
    );
    assert!(model.params[0].annotations.blank_line_before);
    assert_eq!(model.params[0].suffix.as_deref(), Some("K"));
    assert_eq!(model.blocks[1].assignments[0].suffix.as_deref(), Some("K"));
}

#[test]
fn the_worked_text_example_carries_the_same_sequence_as_the_wire_form() {
    let text_model = parse_text(&note_text_example()).expect("parses");
    // The full model: the reference sequence and its checked-in wire bundle.
    assert_eq!(without_text_facts(&text_model), econ_enable());
    let wire = std::fs::read_to_string(fixture("wire/econ_enable.json")).expect("bundle");
    assert_eq!(to_wire_string(&text_model), wire);
    // The note's abridged wire example: every element it lists equals the text's.
    let abridged = std::fs::read_to_string(fixture("note/wire_example.json")).expect("example");
    let note = from_wire_str(&abridged).expect("the note's wire example reads");
    assert_eq!(note.name, text_model.name);
    assert_eq!(note.derived_from, text_model.derived_from);
    let plain = without_text_facts(&text_model);
    assert_eq!(note.params, plain.params);
    assert_eq!(note.inputs[0], plain.inputs[0]);
    assert_eq!(note.blocks[0], plain.blocks[1]);
    assert_eq!(note.connections[0], plain.connections[2]);
    // `dTHys = 1 K` stays the Integer 1 and `uLow = 0.0 K` the Real 0.0 (the suffix is text-only).
    assert_eq!(text_model.params[0].value, Literal::Integer(1));
    assert_eq!(
        text_model.blocks[1].assignments[0].value,
        ParamValue::Literal(Literal::Real(0.0))
    );
}

#[test]
fn every_enumerated_model_survives_print_then_parse() {
    // The enumeration is built for printer shapes, so it carries deliberate stage-1 defects (an
    // unknown class, dangling references); `text_print.rs` runs the same property over a set that
    // stage 1 accepts. Here every model, clean or not, must print within the line bound.
    let models = enumerated_models();
    assert_eq!(models.len(), 48);
    let mut clean = 0;
    for model in models {
        let text = print(&model);
        assert!(
            text.lines().all(|l| l.len() <= oce_author::MAX_LINE_BYTES),
            "a line past the bound\n{text}"
        );
        if validate_author_side(&model).is_empty() {
            clean += 1;
        }
        let parsed = parse_text(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        assert_eq!(parsed, model, "model → text → model\n{text}");
        assert_eq!(print(&parsed), text, "text → model → text\n{text}");
        assert_eq!(to_wire_string(&parsed), to_wire_string(&model));
    }
    assert_eq!(
        clean, 0,
        "every enumerated model carries its stage-1 defects"
    );
}

#[test]
fn enumerated_models_exercise_both_array_spellings_and_both_block_layouts() {
    let texts: Vec<String> = enumerated_models().iter().map(print).collect();
    let all = texts.concat();
    for needle in [
        "k = [1.0 K, -2 K, 1e-7]",
        // A gap, another order, or an element elsewhere in the block: flat.
        "k_1 = 1.0, k_3 = 3.0",
        "k_2 = 2.0, k_1 = 1.0",
        "k_1 = 1.0,\n  k_2 = 2.0,\n  nin = n,\n  k_1 = 5.0\n",
        "  # one comment\n  k = [",
        // A matrix with an incomplete third row is not a rectangle: flat.
        "K_1_3 = 0.25,\n  K_2_1 = 1.5,",
        "K_3_1 = 9.0",
        "k = [p0, p1]",
        "block sub = CDL.Reals.Subtract\n",
        "block odd = CDL.Reals.Nope(\n  x_1 = 1.0,\n",
        "  # one comment\n",
        "\n#  indented # with a hash\n",
        "derived_from cxf:fnv1a128:0123#abc\n",
        "\n# the end\n##\n",
        "@haystack air @haystack temp",
        "-9223372036854775808",
        "5e-324",
        "1.7976931348623157e308",
        "SimpleController.PI",
    ] {
        assert!(
            all.contains(needle),
            "no enumerated text contains {needle:?}"
        );
    }
}

#[test]
fn header_and_trailing_comments_never_reach_the_wire_form_the_schema_or_the_emitter() {
    // The one model change the text form required (issue #482).
    let plain = econ_enable();
    let mut annotated = plain.clone();
    annotated.annotations.comments = vec![" header".to_owned()];
    annotated.annotations.blank_line_before = true;
    annotated.trailing_comments = vec![" trailing".to_owned()];
    assert_ne!(annotated, plain, "equality is structural and compares them");
    assert_eq!(to_wire_string(&annotated), to_wire_string(&plain));
    let options = EmitOptions::default();
    assert_eq!(
        emit(&annotated, &options).expect("emits"),
        emit(&plain, &options).expect("emits")
    );
    let golden = std::fs::read(fixture("emit/econ_enable.cxf.json")).expect("golden");
    assert_eq!(emit(&annotated, &options).expect("emits"), golden);
    assert_eq!(wire_schema(), WIRE_SCHEMA);
    // A wire document and an import read with both empty.
    let read = from_wire_str(&to_wire_string(&annotated)).expect("reads");
    assert!(read.annotations.comments.is_empty() && read.trailing_comments.is_empty());
}
