//! Both data files are strict JSON under the authoring wire profile: a closed shape, sorted
//! keys, one canonical spelling, inside the wire reader's bounds. Every refusal is a typed
//! [`VocabError`].

use oce_vocab::{
    CURATED_JSON, CuratedDescription, CuratedVocabulary, GENERATED_JSON, GeneratedVocabulary,
    MAX_VOCAB_BYTES, MAX_VOCAB_STRING_BYTES, VocabError,
};

/// The wire reader's bounds (`oce-author`, `src/wire/mod.rs`), restated because this crate sits
/// below `oce-author` and cannot name them: document bytes, nesting depth, list length and
/// string bytes.
const WIRE_BYTES: usize = 4 * 1024 * 1024;
const WIRE_DEPTH: usize = 8;
const WIRE_LIST_LEN: usize = 65_536;
const WIRE_STRING_BYTES: usize = 4_096;

#[test]
fn the_reader_bounds_are_the_wire_readers() {
    assert_eq!(MAX_VOCAB_BYTES, WIRE_BYTES);
    assert_eq!(MAX_VOCAB_STRING_BYTES, WIRE_STRING_BYTES);
}

#[test]
fn both_files_parse_and_are_their_own_canonical_rewrite() {
    let generated = GeneratedVocabulary::from_json(GENERATED_JSON).expect("generated.json");
    assert_eq!(generated.to_json(), GENERATED_JSON);
    let curated = CuratedVocabulary::from_json(CURATED_JSON).expect("curated.json");
    assert_eq!(curated.to_json(), CURATED_JSON);
}

/// What a raw scan of a document measures, independently of the typed reader: the deepest
/// nesting, the longest list, the longest string (in bytes as written) and whether every
/// object's keys are strictly ascending.
#[derive(Debug, Default)]
struct Measured {
    depth: usize,
    longest_list: usize,
    longest_string: usize,
    keys_sorted: bool,
    strings: usize,
}

/// Scan canonical JSON text. It relies only on JSON's lexical rules, not on the layout.
fn measure(text: &str) -> Measured {
    enum Frame {
        Object { last_key: Option<String> },
        Array { items: usize },
    }
    let bytes = text.as_bytes();
    let mut measured = Measured {
        keys_sorted: true,
        ..Measured::default()
    };
    let mut stack: Vec<Frame> = Vec::new();
    let mut at = 0;
    let mut expect_key = false;
    while at < bytes.len() {
        match bytes[at] {
            b'{' => {
                stack.push(Frame::Object { last_key: None });
                measured.depth = measured.depth.max(stack.len());
                expect_key = true;
            }
            b'[' => {
                stack.push(Frame::Array { items: 0 });
                measured.depth = measured.depth.max(stack.len());
                // A non-empty array's first item is counted when it starts.
                let mut next = at + 1;
                while bytes[next].is_ascii_whitespace() {
                    next += 1;
                }
                if bytes[next] != b']'
                    && let Some(Frame::Array { items }) = stack.last_mut()
                {
                    *items = 1;
                }
            }
            b'}' | b']' => {
                if let Some(Frame::Array { items }) = stack.pop() {
                    measured.longest_list = measured.longest_list.max(items);
                }
                expect_key = false;
            }
            b',' => match stack.last_mut() {
                Some(Frame::Array { items }) => *items += 1,
                Some(Frame::Object { .. }) => expect_key = true,
                None => {}
            },
            b'"' => {
                let start = at + 1;
                at = start;
                while bytes[at] != b'"' {
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
                let written = &text[start..at];
                measured.longest_string = measured.longest_string.max(written.len());
                measured.strings += 1;
                if expect_key {
                    if let Some(Frame::Object { last_key }) = stack.last_mut() {
                        if last_key.as_deref().is_some_and(|last| last >= written) {
                            measured.keys_sorted = false;
                        }
                        *last_key = Some(written.to_owned());
                    }
                    expect_key = false;
                }
            }
            _ => {}
        }
        at += 1;
    }
    assert!(stack.is_empty(), "unbalanced document");
    measured
}

#[test]
fn the_scan_sees_depth_lists_strings_and_key_order() {
    let measured = measure("{\"a\": [1, 2, {\"b\": \"x\\\"yz\", \"c\": []}], \"d\": \"\"}");
    assert_eq!(measured.depth, 4);
    assert_eq!(measured.longest_list, 3);
    assert_eq!(measured.longest_string, 5);
    assert!(measured.keys_sorted);
    assert!(!measure("{\"b\": 1, \"a\": 2}").keys_sorted);
    assert!(!measure("{\"a\": 1, \"a\": 2}").keys_sorted);
    assert!(!measure("{\"z\": {\"b\": 1, \"a\": 2}, \"zz\": 0}").keys_sorted);
    assert!(measure("[{\"b\": 1}, {\"a\": 2}]").keys_sorted);
}

#[test]
fn both_files_have_sorted_keys_and_sit_inside_the_wire_readers_bounds() {
    for (name, text) in [("generated", GENERATED_JSON), ("curated", CURATED_JSON)] {
        let measured = measure(text);
        assert!(measured.keys_sorted, "{name}: keys out of order");
        assert!(text.len() <= WIRE_BYTES, "{name}");
        assert!(measured.depth <= WIRE_DEPTH, "{name}: {measured:?}");
        assert!(
            measured.longest_list <= WIRE_LIST_LEN,
            "{name}: {measured:?}"
        );
        assert!(
            measured.longest_string <= WIRE_STRING_BYTES,
            "{name}: {measured:?}"
        );
        assert!(
            measured.strings > 100,
            "{name}: the scan found {measured:?}"
        );
        assert!(text.is_ascii(), "{name}: ASCII only");
        assert!(text.ends_with("}\n") && !text.ends_with("\n\n"), "{name}");
        assert!(!text.contains('\r') && !text.contains('\t'), "{name}");
        assert!(!text.contains("null"), "{name}: no null");
    }
    // The closed shape nests five deep: document, classes, class, connectors, connector.
    assert_eq!(measure(GENERATED_JSON).depth, 5);
    assert_eq!(measure(CURATED_JSON).depth, 5);
}

/// A minimal generated document with one class, as canonical text.
fn small_generated() -> String {
    let mut generated = GeneratedVocabulary::from_json(GENERATED_JSON).expect("generated.json");
    generated.classes.truncate(1);
    generated.to_json()
}

/// A minimal curated document with one class, as canonical text.
fn small_curated() -> String {
    let mut curated = CuratedVocabulary::from_json(CURATED_JSON).expect("curated.json");
    curated.classes.truncate(1);
    curated.to_json()
}

fn generated_error(text: &str) -> VocabError {
    GeneratedVocabulary::from_json(text).expect_err("refused")
}

fn curated_error(text: &str) -> VocabError {
    CuratedVocabulary::from_json(text).expect_err("refused")
}

fn is_json(error: &VocabError, needle: &str) -> bool {
    matches!(error, VocabError::Json { detail, .. } if detail.contains(needle))
}

#[test]
fn objects_are_closed_and_every_key_is_required_exactly_once() {
    let text = small_generated();
    let unknown = text.replacen("\"format\": 1", "\"extra\": 0,\n  \"format\": 1", 1);
    assert!(is_json(&generated_error(&unknown), "unknown field `extra`"));
    let unknown_nested = text.replacen(
        "\"dims\": 0,",
        "\"dims\": 0,\n          \"kind\": \"Real\",",
        1,
    );
    assert!(is_json(
        &generated_error(&unknown_nested),
        "unknown field `kind`"
    ));
    let missing = text.replacen("          \"dims\": 0,\n", "", 1);
    assert!(is_json(&generated_error(&missing), "missing field `dims`"));
    let missing_top = text.replacen("  \"format\": 1,\n", "", 1);
    assert!(is_json(
        &generated_error(&missing_top),
        "missing field `format`"
    ));
    let repeated = text.replacen("\"dims\": 0,", "\"dims\": 0,\n          \"dims\": 0,", 1);
    assert!(is_json(
        &generated_error(&repeated),
        "duplicate field `dims`"
    ));

    let text = small_curated();
    let unknown = text.replacen("\"kind\":", "\"note\": \"\",\n          \"kind\":", 1);
    assert!(is_json(&curated_error(&unknown), "unknown field `note`"));
    let missing = text.replacen("  \"format\": 1\n", "  \"formats\": 1\n", 1);
    assert!(is_json(&curated_error(&missing), "unknown field `formats`"));
}

#[test]
fn null_wrong_types_and_unknown_enumerators_are_refused() {
    let text = small_generated();
    for (from, to, needle) in [
        ("\"dims\": 0", "\"dims\": null", "invalid type: null"),
        ("\"dims\": 0", "\"dims\": -1", "invalid value"),
        (
            "\"dims\": 0",
            "\"dims\": 0.5",
            "invalid type: floating point",
        ),
        ("\"dims\": 0", "\"dims\": 256", "invalid value"),
        ("\"dims\": 0", "\"dims\": \"0\"", "invalid type: string"),
        (
            "\"upstream\": true",
            "\"upstream\": 1",
            "invalid type: integer",
        ),
        (
            "\"side\": \"output\"",
            "\"side\": \"Output\"",
            "unknown variant `Output`",
        ),
        (
            "\"side\": \"output\"",
            "\"side\": \"both\"",
            "unknown variant `both`",
        ),
        ("\"unit\": \"\"", "\"unit\": null", "invalid type: null"),
        (
            "\"parameters\": [",
            "\"parameters\": null, \"x\": [",
            "invalid type: null",
        ),
        ("\"format\": 1", "\"format\": 4294967296", "invalid value"),
    ] {
        assert!(text.contains(from), "{from}");
        let error = generated_error(&text.replacen(from, to, 1));
        assert!(is_json(&error, needle), "{to}: {error:?}");
    }
    let text = small_curated();
    for kind in ["Convention", "port_order", "portOrder", "note", ""] {
        let edited = text.replacen(
            "\"kind\": \"port-order\"",
            &format!("\"kind\": \"{kind}\""),
            1,
        );
        let error = curated_error(&edited);
        assert!(is_json(&error, "unknown variant"), "{kind}: {error:?}");
    }
}

#[test]
fn text_that_is_not_a_json_document_of_the_shape_is_refused_with_its_position() {
    for text in [
        "",
        "{",
        "[]",
        "null",
        "{}x",
        "\"classes\"",
        "{\"classes\": []} {}",
    ] {
        let error = generated_error(text);
        assert!(
            matches!(error, VocabError::Json { .. }),
            "{text}: {error:?}"
        );
        assert!(
            matches!(curated_error(text), VocabError::Json { .. }),
            "{text}"
        );
    }
    assert_eq!(
        generated_error("{\n  \"classes\": [],\n  \"format\": 1,\n  \"source\": 7\n}\n"),
        VocabError::Json {
            line: 4,
            column: 13,
            detail: "invalid type: integer `7`, expected struct Source at line 4 column 13"
                .to_owned(),
        }
    );
    assert_eq!(generated_error("\u{feff}{}"), VocabError::ByteOrderMark);
    assert_eq!(curated_error("\u{feff}{}"), VocabError::ByteOrderMark);
}

#[test]
fn a_document_that_is_not_the_canonical_spelling_is_refused_with_its_first_line() {
    let text = small_generated();
    for (edited, line) in [
        (text.replacen("{\n", "{\r\n", 1), 1),
        (text.replacen("  \"classes\"", "    \"classes\"", 1), 2),
        (text.trim_end().to_owned(), text.lines().count()),
        (format!("{text}\n"), text.lines().count() + 2),
        (text.replacen("\"dims\": 0", "\"dims\":0", 1), 8),
        (text.replacen("\"dims\": 0", "\"dims\": 00", 1), 0),
        (
            text.replacen("Constant output signal", "Constant output \\u0073ignal", 1),
            7,
        ),
    ] {
        match GeneratedVocabulary::from_json(&edited) {
            Err(VocabError::NotCanonical { line: found }) => assert_eq!(found, line, "{edited}"),
            Err(VocabError::Json { .. }) => assert_eq!(line, 0, "{edited}"),
            other => panic!("{other:?}"),
        }
    }
    // Keys out of order: the same data, `format` written before `classes`.
    let reordered = "{\n  \"format\": 1,\n  \"classes\": []\n}\n";
    assert_eq!(
        curated_error(reordered),
        VocabError::NotCanonical { line: 2 }
    );
    assert!(CuratedVocabulary::from_json("{\n  \"classes\": [],\n  \"format\": 1\n}\n").is_ok());
    // A positional array where an object belongs is not the shape's spelling either.
    let positional = text.replacen("\"format\": 1", "\"format\":1", 1);
    assert!(matches!(
        generated_error(&positional),
        VocabError::NotCanonical { .. }
    ));
}

#[test]
fn the_format_and_the_source_pins_are_checked() {
    let text = small_generated();
    assert_eq!(
        generated_error(&text.replacen("\"format\": 1", "\"format\": 2", 1)),
        VocabError::UnsupportedFormat { format: 2 }
    );
    assert_eq!(
        curated_error(&small_curated().replacen("\"format\": 1", "\"format\": 0", 1)),
        VocabError::UnsupportedFormat { format: 0 }
    );
    let commit = "a131864e4c4df22ebcd52bb8da439de0087ac365";
    for (bad, field) in [
        (&commit[1..], "buildings_commit"),
        (
            "A131864E4C4DF22EBCD52BB8DA439DE0087AC365",
            "buildings_commit",
        ),
        (
            "g131864e4c4df22ebcd52bb8da439de0087ac365",
            "buildings_commit",
        ),
        ("", "buildings_commit"),
    ] {
        assert_eq!(
            generated_error(&text.replacen(commit, bad, 1)),
            VocabError::InvalidSource { field },
            "{bad}"
        );
    }
    let digest = "bb2bf4a7f868af884df4b2f05258a754625e10920716739527b46631a4383314";
    assert_eq!(
        generated_error(&text.replacen(digest, &digest[..40], 1)),
        VocabError::InvalidSource { field: "sha256" }
    );
    let json_commit = "85721b828a6ff8d9d3c1a48ff9a59808d2fa31fb";
    assert_eq!(
        generated_error(&text.replacen(json_commit, &format!("{json_commit}0"), 1)),
        VocabError::InvalidSource {
            field: "modelica_json_commit"
        }
    );
}

#[test]
fn strings_are_bounded_at_the_limit_and_refuse_control_characters_and_padding() {
    let text = small_generated();
    let description = "Constant output signal";
    let at_limit = "x".repeat(MAX_VOCAB_STRING_BYTES);
    assert!(GeneratedVocabulary::from_json(&text.replacen(description, &at_limit, 1)).is_ok());
    assert_eq!(
        generated_error(&text.replacen(description, &format!("{at_limit}x"), 1)),
        VocabError::StringTooLong {
            path: "classes[0].connectors[0].description".to_owned(),
            bytes: MAX_VOCAB_STRING_BYTES + 1,
            limit: MAX_VOCAB_STRING_BYTES,
        }
    );
    for escaped in [
        "a\\u000ab",
        "a\\u0009b",
        "a\\u0000b",
        "a\\u007fb",
        "a\\u0085b",
    ] {
        assert_eq!(
            generated_error(&text.replacen(description, escaped, 1)),
            VocabError::ControlCharacter {
                path: "classes[0].connectors[0].description".to_owned()
            },
            "{escaped}"
        );
    }
    for padded in [" Constant", "Constant ", "\\u00a0Constant"] {
        assert_eq!(
            generated_error(&text.replacen(description, padded, 1)),
            VocabError::SurroundingWhitespace {
                path: "classes[0].connectors[0].description".to_owned()
            },
            "{padded}"
        );
    }
    // A description may be empty; a name may not.
    assert!(GeneratedVocabulary::from_json(&text.replacen(description, "", 1)).is_ok());
    assert_eq!(
        generated_error(&text.replacen("\"name\": \"y\"", "\"name\": \"\"", 1)),
        VocabError::EmptyField {
            path: "classes[0].connectors[0].name".to_owned()
        }
    );
    assert_eq!(
        generated_error(&text.replacen(
            "\"class\": \"CDL.Reals.Sources.Constant\"",
            "\"class\": \"\"",
            1
        )),
        VocabError::EmptyField {
            path: "classes[0].class".to_owned()
        }
    );
}

#[test]
fn connector_dims_are_zero_one_or_two() {
    let text = small_generated();
    for dims in [1, 2] {
        let edited = text.replacen("\"dims\": 0", &format!("\"dims\": {dims}"), 1);
        assert!(GeneratedVocabulary::from_json(&edited).is_ok(), "{dims}");
    }
    for dims in [3u8, 255] {
        let edited = text.replacen("\"dims\": 0", &format!("\"dims\": {dims}"), 1);
        assert_eq!(
            generated_error(&edited),
            VocabError::InvalidDims {
                path: "classes[0].connectors[0].dims".to_owned(),
                dims,
            }
        );
    }
}

#[test]
fn a_curated_entry_needs_a_subject_text_and_evidence() {
    let curated = CuratedVocabulary::from_json(CURATED_JSON).expect("curated.json");
    let mut note_without_evidence = curated.clone();
    note_without_evidence.classes[0].notes[0].evidence.clear();
    assert_eq!(
        curated_error(&note_without_evidence.to_json()),
        VocabError::EmptyField {
            path: "classes[0].notes[0].evidence".to_owned()
        }
    );
    let mut note_without_text = curated.clone();
    note_without_text.classes[0].notes[0].text.clear();
    assert_eq!(
        curated_error(&note_without_text.to_json()),
        VocabError::EmptyField {
            path: "classes[0].notes[0].text".to_owned()
        }
    );
    let described = curated
        .classes
        .iter()
        .position(|class| !class.descriptions.is_empty())
        .expect("a curated description");
    for field in ["subject", "text", "evidence"] {
        let mut edited = curated.clone();
        let description = &mut edited.classes[described].descriptions[0];
        match field {
            "subject" => description.subject.clear(),
            "text" => description.text.clear(),
            _ => description.evidence.clear(),
        }
        assert_eq!(
            curated_error(&edited.to_json()),
            VocabError::EmptyField {
                path: format!("classes[{described}].descriptions[0].{field}")
            }
        );
    }
}

#[test]
fn the_document_size_bound_is_checked_before_anything_is_parsed() {
    let oversized = " ".repeat(MAX_VOCAB_BYTES + 1);
    let expected = VocabError::TooLarge {
        bytes: MAX_VOCAB_BYTES + 1,
        limit: MAX_VOCAB_BYTES,
    };
    assert_eq!(generated_error(&oversized), expected);
    assert_eq!(curated_error(&oversized), expected);
    // At the limit the size check passes and the content is what is refused.
    let at_limit = " ".repeat(MAX_VOCAB_BYTES);
    assert!(matches!(
        generated_error(&at_limit),
        VocabError::Json { .. }
    ));
}

#[test]
fn the_byte_bound_refuses_a_list_before_it_can_reach_the_wire_readers_list_bound() {
    // The smallest list item the canonical spelling can write: a curated description whose
    // three fields are one character each.
    let smallest = CuratedDescription {
        evidence: "e".to_owned(),
        subject: "s".to_owned(),
        text: "t".to_owned(),
    };
    let mut curated = CuratedVocabulary::from_json(&small_curated()).expect("curated");
    curated.classes[0].notes.clear();
    curated.classes[0].descriptions = vec![smallest.clone()];
    let one = curated.to_json().len();
    curated.classes[0].descriptions.push(smallest.clone());
    let per_item = curated.to_json().len() - one;
    assert_eq!(per_item, 96);
    assert!(per_item * WIRE_LIST_LEN > MAX_VOCAB_BYTES);
    curated.classes[0].descriptions = vec![smallest; WIRE_LIST_LEN + 1];
    let text = curated.to_json();
    assert_eq!(
        curated_error(&text),
        VocabError::TooLarge {
            bytes: text.len(),
            limit: MAX_VOCAB_BYTES,
        }
    );
}
