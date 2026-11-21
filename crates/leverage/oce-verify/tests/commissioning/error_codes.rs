//! Every `SpecError` code is produced by at least one document, and the list is pinned: one
//! small case per code, each asserted to give exactly that code at the place it names. The case
//! table and [`oce_verify::SPEC_ERROR_CODES`] must cover each other, so a new kind needs a case
//! here before the suite passes.

use std::collections::BTreeSet;

use oce_verify::{
    CommissioningSpec, Location, MAX_DEPTH, MAX_LIST_LEN, MAX_SPEC_DETAIL_BYTES, MAX_STRING_BYTES,
    SPEC_ERROR_CODES, SpecError, SpecLimits, VECTOR_ERROR_CODES,
};

use crate::{load, one_reference, top_level};

/// The codes, in the order `SpecErrorKind` declares them: the fourteen strict-JSON refusals,
/// the comment's, then the format's own, then the four found at bind.
const PINNED: &[&str] = &[
    "not-utf8",
    "byte-order-mark",
    "json-syntax",
    "input-too-large",
    "nesting-too-deep",
    "list-too-long",
    "string-too-long",
    "duplicate-key",
    "unknown-key",
    "missing-key",
    "wrong-type",
    "non-finite-number",
    "integer-out-of-range",
    "control-character",
    "unterminated-comment",
    "unknown-schema",
    "too-many-parameters",
    "too-many-keys",
    "empty-name",
    "unknown-value",
    "path-invalid",
    "invalid-time-origin",
    "invalid-tolerance",
    "invalid-sampling",
    "invalid-window",
    "invalid-hold",
    "duplicate-point",
    "unmatched-key",
    "ambiguous-output",
    "conflicting-override",
    "work-limit-exceeded",
];

#[test]
fn the_code_list_is_pinned() {
    assert_eq!(SPEC_ERROR_CODES, PINNED);
}

#[test]
fn the_shared_refusals_are_spelled_as_the_vector_reader_spells_them() {
    // The fourteen strict-JSON codes are the vector reader's first fourteen, in its order.
    assert_eq!(&SPEC_ERROR_CODES[..14], &VECTOR_ERROR_CODES[..14]);
    // And the four format words the two readers share are one word each.
    for shared in [
        "unknown-schema",
        "too-many-parameters",
        "empty-name",
        "invalid-window",
    ] {
        assert!(SPEC_ERROR_CODES.contains(&shared), "{shared}");
        assert!(VECTOR_ERROR_CODES.contains(&shared), "{shared}");
    }
    // `ambiguous-output` is the vector bind's word for a name the replay rule gives to several
    // outputs.
    assert!(oce_verify::VERIFY_ERROR_CODES.contains(&"ambiguous-output"));
    // A path of a spec is not a mutant's: the vector reader's word for its own list stays there.
    assert!(!SPEC_ERROR_CODES.contains(&"mutant-path-invalid"));
    let mut seen = BTreeSet::new();
    for code in SPEC_ERROR_CODES {
        assert!(seen.insert(*code), "{code} twice");
        assert!(
            code.bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'-' || b.is_ascii_digit()),
            "{code}"
        );
        assert!(!code.starts_with('-') && !code.ends_with('-'), "{code}");
    }
}

/// The refusal of `bytes` under the default limits.
fn refused(bytes: &[u8]) -> SpecError {
    CommissioningSpec::from_json_bytes(bytes, &SpecLimits::default()).expect_err("refused")
}

/// A code, a document refused with it, and the line and column of the refusal (`None` for a
/// refusal of the bytes as a whole, and for the two count bounds, which are checked by key).
type LoadCase = (&'static str, Vec<u8>, Option<(u32, u32)>);

/// One document per load code.
fn load_cases() -> Vec<LoadCase> {
    let text = |text: &str| text.as_bytes().to_vec();
    let nested = "[".repeat(MAX_DEPTH + 1);
    let long_list = format!(
        "{{\"references\": [], \"controller\": [{}]}}",
        vec!["0"; MAX_LIST_LEN + 1].join(",")
    );
    let long_string = format!(
        "{{\"references\": [], \"controller\": \"{}\"}}",
        "a".repeat(MAX_STRING_BYTES + 1)
    );
    let many_parameters: Vec<String> = (0..257).map(|index| format!("\"p{index}\": 0")).collect();
    let many_keys: Vec<String> = (0..4_097)
        .map(|index| format!("\"s.y{index}\": {{}}"))
        .collect();
    vec![
        ("not-utf8", b"{\"references\": [\xff]}".to_vec(), None),
        (
            "byte-order-mark",
            text("\u{feff}{\"references\": []}"),
            None,
        ),
        ("json-syntax", text("{\"references\": [],}"), Some((1, 19))),
        ("input-too-large", vec![b' '; 4 * 1024 * 1024 + 1], None),
        ("nesting-too-deep", text(&nested), Some((1, 9))),
        ("list-too-long", text(&long_list), Some((1, 34))),
        ("string-too-long", text(&long_string), Some((1, 34))),
        (
            "duplicate-key",
            text("{\"references\": [], \"references\": []}"),
            Some((1, 20)),
        ),
        (
            "unknown-key",
            text("{\"references\": [], \"data\": 1}"),
            Some((1, 20)),
        ),
        ("missing-key", text(" {\"sampling\": 5}"), Some((1, 2))),
        ("wrong-type", text("{\"references\": {}}"), Some((1, 16))),
        (
            "non-finite-number",
            text("{\"references\": [], \"sampling\": NaN}"),
            Some((1, 32)),
        ),
        (
            "integer-out-of-range",
            text("{\"references\": [], \"sampling\": 9007199254740993}"),
            Some((1, 32)),
        ),
        (
            "control-character",
            text("{\"references\": [], \"controller\": \"a\\tb\"}"),
            Some((1, 36)),
        ),
        (
            "unterminated-comment",
            // A file with CRLF line ends: the `/*` is on line 3, and a carriage return is a
            // byte of its line, never a line of its own.
            text("{\r\n  \"references\": []\r\n} /* open\r\n"),
            Some((3, 3)),
        ),
        (
            "unknown-schema",
            text("{\"references\": [], \"schema\": \"oce/vectors/v1\"}"),
            Some((1, 30)),
        ),
        (
            "too-many-parameters",
            text(&one_reference(&format!(
                "\"parameters\": {{{}}}",
                many_parameters.join(", ")
            ))),
            None,
        ),
        (
            "too-many-keys",
            text(&one_reference(&format!(
                "\"outputs\": {{{}}}",
                many_keys.join(", ")
            ))),
            None,
        ),
        (
            "empty-name",
            text("{\"references\": [{\"model\": \"M\", \"sequence\": \"\"}]}"),
            Some((1, 44)),
        ),
        (
            "unknown-value",
            text(&one_reference("\"envelope\": \"tube\"")),
            Some((1, 61)),
        ),
        (
            "path-invalid",
            text(&one_reference("\"cxf\": \"../s.json\"")),
            Some((1, 56)),
        ),
        (
            "invalid-time-origin",
            text(&top_level("\"time\": {\"origin\": \"2018-06-07\"}")),
            Some((1, 39)),
        ),
        (
            "invalid-tolerance",
            text(&top_level("\"tolerances\": {\"atoly\": -1}")),
            Some((1, 44)),
        ),
        (
            "invalid-sampling",
            text(&top_level("\"sampling\": 0")),
            Some((1, 32)),
        ),
        (
            "invalid-window",
            text(&one_reference("\"window\": {\"start\": 1, \"stop\": 0}")),
            Some((1, 59)),
        ),
        (
            "invalid-hold",
            text(&one_reference(
                "\"inputs\": [{\"path\": \"t\", \"format\": \"csv\", \"timeColumn\": 1, \"columns\": {}, \"hold\": {\"maxAge\": -1}}]",
            )),
            Some((1, 142)),
        ),
        (
            "duplicate-point",
            text(&one_reference(
                "\"pointNameMapping\": [{\"cdl\": {\"name\": \"u\", \"type\": \"int\"}, \"device\": {\"name\": \"A\", \"type\": \"int\"}}, {\"cdl\": {\"name\": \"u\", \"type\": \"int\"}, \"device\": {\"name\": \"B\", \"type\": \"int\"}}]",
            )),
            Some((1, 166)),
        ),
    ]
}

#[test]
fn every_load_code_is_produced_at_the_place_it_names() {
    let cases = load_cases();
    for (code, bytes, place) in &cases {
        let error = refused(bytes);
        assert_eq!(error.code(), *code, "{}", error.detail());
        assert!(SPEC_ERROR_CODES.contains(&error.code()));
        assert_eq!(error.kind().code(), error.code());
        match place {
            Some((line, column)) => {
                let at = Location::new(
                    usize::try_from(*line).expect("fits"),
                    usize::try_from(*column).expect("fits"),
                );
                assert_eq!(error.at(), Some(at), "{code}: {}", error.detail());
                assert_eq!(error.subject(), at.to_string(), "{code}");
            }
            None if matches!(*code, "too-many-parameters" | "too-many-keys") => {
                // Placed in the text, at the key past the bound: the 257th, and the 4,097th.
                let at = error.at().expect("the key's place");
                let text = std::str::from_utf8(bytes).expect("UTF-8");
                let column = usize::try_from(at.column).expect("fits") - 1;
                let past = if *code == "too-many-parameters" {
                    "\"p256\""
                } else {
                    "\"s.y4096\""
                };
                assert!(text[column..].starts_with(past), "{code}");
            }
            None => {
                assert_eq!(error.at(), None, "{code}");
                assert_eq!(error.subject(), "document", "{code}");
            }
        }
        assert!(!error.detail().is_empty(), "{code}");
        assert_eq!(
            error.to_string(),
            format!("{}: {}", error.subject(), error.detail())
        );
    }
    // The table and the list cover each other, but for the four codes a load never gives: they
    // are found when a reference's keys are matched against a sequence's outputs, and the
    // crate's own tests produce each of them there.
    let produced: BTreeSet<&str> = cases.iter().map(|(code, _, _)| *code).collect();
    let listed: BTreeSet<&str> = SPEC_ERROR_CODES.iter().copied().collect();
    let at_bind: BTreeSet<&str> = [
        "unmatched-key",
        "ambiguous-output",
        "conflicting-override",
        "work-limit-exceeded",
    ]
    .into();
    assert_eq!(
        produced.union(&at_bind).copied().collect::<BTreeSet<_>>(),
        listed
    );
    assert!(produced.is_disjoint(&at_bind));
    assert_eq!(cases.len(), SPEC_ERROR_CODES.len() - 4, "one case per code");
}

#[test]
fn the_first_failing_stage_wins() {
    // Bytes, then comments, then strict JSON, then the tag, then the shape and the rules.
    let cases: [(&[u8], &str); 8] = [
        // Too large and not UTF-8: the length is judged before a byte is read.
        (&[0xff; 4 * 1024 * 1024 + 1], "input-too-large"),
        // Not UTF-8 and an open comment.
        (b"/* \xff", "not-utf8"),
        // A byte-order mark and an open comment.
        ("\u{feff}/*".as_bytes(), "byte-order-mark"),
        // An open comment and a syntax error before it.
        (b"{,} /*", "unterminated-comment"),
        // A syntax error after a foreign tag.
        (b"{\"schema\": \"x\", \"a\": 01}", "json-syntax"),
        // A foreign tag and an unknown key before it.
        (b"{\"zzz\": 1, \"schema\": \"x\"}", "unknown-schema"),
        // An unknown key before a rule broken later.
        (
            b"{\"zzz\": 1, \"sampling\": 0, \"references\": []}",
            "unknown-key",
        ),
        // A rule broken before a key is found missing at the object's end.
        (b"{\"sampling\": 0}", "invalid-sampling"),
    ];
    for (bytes, expected) in cases {
        assert_eq!(refused(bytes).code(), expected);
    }
    // Within one stage the first place in document order wins.
    let error = load("{\"references\": [], \"a\": 1, \"b\": 2}").expect_err("refused");
    assert_eq!(error.at(), Some(Location::new(1, 20)));
    let error = load("{\"references\": [{\"model\": \"M\", \"sequence\": \"s\", \"sampling\": -1, \"cxf\": \"/x\"}]}")
        .expect_err("refused");
    assert_eq!(error.code(), "invalid-sampling");
}

#[test]
fn a_sentence_is_bounded_printable_ascii_whatever_the_document_wrote() {
    let long = "k".repeat(MAX_STRING_BYTES);
    let hostile = "\u{1b}[31m\u{202e}caf\u{e9}\n\u{1f321}`";
    let documents = [
        // An unknown key of the longest length a string may have.
        format!("{{\"references\": [], \"{long}\": 1}}"),
        // A key of terminal escapes, a bidirectional override, a line feed and a backtick.
        "{\"references\": [], \"\\u001b[31m\"}".to_owned(),
        format!(
            "{{\"references\": [], \"{}\": 1}}",
            hostile.replace('\u{1b}', "x").replace('\n', "\\n")
        ),
        // A long unknown value, a long path, a long duplicate key and a long foreign tag.
        one_reference(&format!("\"envelope\": \"{long}\"")),
        one_reference(&format!("\"cxf\": \"/{long}\"")),
        format!("{{\"references\": [], \"controller\": {{\"{long}\": 1, \"{long}\": 2}}}}"),
        format!("{{\"schema\": \"{long}\"}}"),
        top_level(&format!("\"time\": {{\"origin\": \"{long}\"}}")),
    ];
    for document in documents {
        let error = load(&document).expect_err("refused");
        for text in [error.detail(), error.subject()] {
            assert!(text.len() <= MAX_SPEC_DETAIL_BYTES, "{} bytes", text.len());
            assert!(
                text.bytes().all(|byte| (b' '..=b'~').contains(&byte)),
                "{text:?}"
            );
            assert!(!text.is_empty());
        }
    }
    assert_eq!(MAX_SPEC_DETAIL_BYTES, 512);
    // What is quoted is cut to a readable length, and says that it was cut.
    let error = load(&format!("{{\"references\": [], \"{long}\": 1}}")).expect_err("refused");
    assert_eq!(
        error.detail(),
        format!("unknown key `{}...`", "k".repeat(93))
    );
    // A character outside printable ASCII is spelled, never passed through.
    let error = load("{\"references\": [], \"caf\u{e9}\\n\u{202e}\": 1}").expect_err("refused");
    assert_eq!(error.detail(), "unknown key `caf\\u{e9}\\n\\u{202e}`");
}
