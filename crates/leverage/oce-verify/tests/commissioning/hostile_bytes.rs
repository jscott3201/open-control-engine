//! Hostile input never panics and never half-loads: every prefix of a valid document, every
//! single-byte change to one, and a seeded sweep of mutated documents each give either a spec
//! (or a mapping) that survives its own canonical form, or one of the pinned codes with a
//! bounded, printable sentence.

use oce_verify::{
    CommissioningSpec, MAX_SPEC_DETAIL_BYTES, PointMapping, SPEC_ERROR_CODES, SpecError, SpecLimits,
};

use crate::fixture_text;
use crate::support::Sequence;

/// Hold a refusal to the contract and return its code.
fn judged(error: &SpecError) -> &'static str {
    assert!(SPEC_ERROR_CODES.contains(&error.code()), "{}", error.code());
    // A load never gives a code that needs a sequence's outputs.
    assert!(!matches!(
        error.code(),
        "unmatched-key" | "ambiguous-output" | "conflicting-override" | "work-limit-exceeded"
    ));
    for text in [error.subject(), error.detail()] {
        assert!(
            !text.is_empty() && text.len() <= MAX_SPEC_DETAIL_BYTES,
            "{text:?}"
        );
        assert!(
            text.bytes().all(|byte| (b' '..=b'~').contains(&byte)),
            "{text:?}"
        );
    }
    assert_eq!(
        error.at().is_none(),
        matches!(
            error.code(),
            "not-utf8" | "byte-order-mark" | "input-too-large"
        ),
        "{}: only a refusal of the bytes as a whole has no place",
        error.code()
    );
    error.code()
}

/// Load `bytes` as a spec: a pinned code, or a spec whose canonical form is a fixed point.
fn judge(bytes: &[u8]) -> &'static str {
    match CommissioningSpec::from_json_bytes(bytes, &SpecLimits::default()) {
        Err(error) => judged(&error),
        Ok(spec) => {
            let written = spec.to_canonical_json();
            let again = CommissioningSpec::from_json_str(&written, &SpecLimits::default())
                .unwrap_or_else(|error| panic!("the canonical form does not load: {error}"));
            assert_eq!(again, spec);
            assert_eq!(again.to_canonical_json(), written);
            assert_eq!(again.spec_id(), spec.spec_id());
            assert_eq!(again.findings(), spec.findings());
            "loaded"
        }
    }
}

/// Load `bytes` as a mapping file, held to the same contract.
fn judge_mapping(bytes: &[u8]) -> &'static str {
    match PointMapping::from_json_bytes(bytes, &SpecLimits::default()) {
        Err(error) => judged(&error),
        Ok(mapping) => {
            let written = mapping.to_canonical_json();
            let again = PointMapping::from_json_str(&written, &SpecLimits::default())
                .unwrap_or_else(|error| panic!("the canonical form does not load: {error}"));
            assert_eq!(again, mapping);
            assert_eq!(again.to_canonical_json(), written);
            "loaded"
        }
    }
}

const SMALL: &str = r#"// a small spec
{"schema": "oce/commissioning-spec/v1", "time": {"origin": "2018-06-07T00:00:00Z"},
 "tolerances": {"atoly": 0.5}, /* global */ "sampling": 5, "controller": [null],
 "references": [{"model": "Mé", "sequence": "s", "cxf": "a/s.json", "runController": true,
   "window": {"start": 0, "stop": 60}, "envelope": "funnel", "parameters": {"k": -0.0},
   "outputs": {"s.y*": {"rtoly": 1e-3}}, "indicators": {"s.y": ["s.u"]},
   "pointNameMapping": [{"cdl": {"name": "u", "type": "int"},
                         "device": {"name": "U", "unit": "%", "type": "bool"}}],
   "inputs": [{"path": "t.csv", "format": "csv", "timeColumn": "t", "columns": {"a": "U"},
               "hold": {"maxAge": 1.5}}]}]}"#;

const SMALL_MAPPING: &str = r#"[ // two points
 {"cdl": {"name": "u", "unit": "K", "type": "float"}, "device": {"name": "Ué", "unit": "degF", "type": "float"}},
 {"cdl": {"name": "b", "type": "int"}, /* no unit */ "device": {"name": "B", "type": "bool"}}]"#;

#[test]
fn the_small_documents_load() {
    assert_eq!(judge(SMALL.as_bytes()), "loaded");
    assert_eq!(judge_mapping(SMALL_MAPPING.as_bytes()), "loaded");
}

#[test]
fn every_proper_prefix_of_a_document_is_refused() {
    for (document, judge) in [
        (SMALL, judge as fn(&[u8]) -> &'static str),
        (SMALL_MAPPING, judge_mapping),
    ] {
        let bytes = document.as_bytes();
        let mut codes = std::collections::BTreeSet::new();
        for end in 0..bytes.len() {
            let code = judge(&bytes[..end]);
            assert_ne!(code, "loaded", "the first {end} bytes are no document");
            codes.insert(code);
        }
        // A cut lands in the JSON, in a two-byte character, or inside the block comment.
        assert_eq!(
            codes.into_iter().collect::<Vec<_>>(),
            ["json-syntax", "not-utf8", "unterminated-comment"]
        );
    }
}

#[test]
fn every_single_byte_change_is_judged_without_a_panic() {
    for (document, judge, most_refused) in [
        (SMALL, judge as fn(&[u8]) -> &'static str, 10_000_u32),
        (SMALL_MAPPING, judge_mapping, 3_000),
    ] {
        let bytes = document.as_bytes();
        let (mut loaded, mut refused) = (0_u32, 0_u32);
        for at in 0..bytes.len() {
            for replacement in [
                b' ', b'0', b'9', b'"', b'{', b'}', b'[', b']', b',', b':', b'-', b'e', b'.',
                b'\\', b'/', b'*', b'\n', 0x00, 0x7f, 0xff,
            ] {
                if bytes[at] == replacement {
                    continue;
                }
                let mut changed = bytes.to_vec();
                changed[at] = replacement;
                if judge(&changed) == "loaded" {
                    loaded += 1;
                } else {
                    refused += 1;
                }
            }
            // And the byte removed.
            let mut shorter = bytes.to_vec();
            shorter.remove(at);
            if judge(&shorter) == "loaded" {
                loaded += 1;
            } else {
                refused += 1;
            }
        }
        // Most changes break the document; some (a digit in a number, a letter in a name, a
        // byte of a comment) leave another valid one.
        assert!(refused > most_refused, "refused {refused}");
        assert!(loaded > 100, "loaded {loaded}");
    }
}

/// The bytes a mutation draws from: structure, digits, signs, an exponent, escapes, whitespace,
/// letters that begin a word, the comment markers, and bytes that are not UTF-8 or are control
/// characters.
const ALPHABET: &[u8] = b"{}[]:,\"\\ \n\r\t0123456789-+.eEtrufalsn/*_?\x00\x1f\x7f\x80\xc2\xff";

/// The values a mutation writes over a member's value: one of each JSON type, the edges of the
/// rules (an empty string, zero, a negative number, a real that overflows) and a path that
/// leaves its directory.
const VALUES: &[&[u8]] = &[
    b"null",
    b"true",
    b"0",
    b"-1",
    b"1e999",
    b"\"\"",
    b"\"x\"",
    b"\"../x\"",
    b"[]",
    b"{}",
    b"[[\"a\"]]",
];

fn mutate(seed: &[u8], random: &mut Sequence) -> Vec<u8> {
    let mut bytes = seed.to_vec();
    for _ in 0..=random.below(4) {
        let at = random.below(bytes.len() + 1);
        let byte = ALPHABET[random.below(ALPHABET.len())];
        match random.below(7) {
            0 if at < bytes.len() => bytes[at] = byte,
            1 if at < bytes.len() => {
                bytes.remove(at);
            }
            2 => bytes.insert(at, byte),
            // Repeat a slice: duplicated members, doubled lists, runaway nesting.
            3 if at < bytes.len() => {
                // Mostly a few bytes; one time in four enough to hold a whole member or pair.
                let most = if random.below(4) == 0 { 400 } else { 40 };
                let end = (at + 1 + random.below(most)).min(bytes.len());
                let slice = bytes[at..end].to_vec();
                bytes.splice(at..at, slice);
            }
            // A comment marker dropped anywhere: in code, in a string, in another comment.
            4 => {
                let markers: [&[u8]; 5] = [b"//", b"/*", b"*/", b"/**/", b"/*/"];
                let marker = markers[random.below(markers.len())];
                bytes.splice(at..at, marker.iter().copied());
            }
            // Another value where a member's value is: a wrong type, an empty name, a zero.
            5 => {
                let members: Vec<usize> = bytes
                    .windows(3)
                    .enumerate()
                    .filter(|(_, window)| *window == b"\": ")
                    .map(|(index, _)| index + 3)
                    .collect();
                if !members.is_empty() {
                    let start = members[random.below(members.len())];
                    let end = bytes[start..]
                        .iter()
                        .position(|byte| b",}]\n".contains(byte))
                        .map_or(bytes.len(), |offset| start + offset);
                    let value = VALUES[random.below(VALUES.len())];
                    bytes.splice(start..end, value.iter().copied());
                }
            }
            // Cut a slice out.
            _ if at < bytes.len() => {
                let end = (at + 1 + random.below(40)).min(bytes.len());
                bytes.drain(at..end);
            }
            _ => bytes.push(byte),
        }
    }
    bytes
}

#[test]
fn twenty_thousand_mutated_specs_are_each_a_spec_or_a_pinned_refusal() {
    let seeds: Vec<Vec<u8>> = [
        SMALL.to_owned(),
        fixture_text("listing_12_1.json"),
        fixture_text("listing_12_1_commented.json"),
        fixture_text("building_33.json"),
        fixture_text("every_field.json"),
        fixture_text("canonical/every_field.json"),
    ]
    .into_iter()
    .map(String::into_bytes)
    .collect();
    let mut random = Sequence(0x0658);
    let mut seen = std::collections::BTreeMap::new();
    for round in 0..20_000 {
        let mutated = mutate(&seeds[round % seeds.len()], &mut random);
        *seen.entry(judge(&mutated)).or_insert(0_u32) += 1;
    }
    // The sweep reached well past the syntax layer: specs that still load, comment refusals,
    // shape refusals and rule refusals all occurred.
    let count = |code: &str| seen.get(code).copied().unwrap_or(0);
    assert!(count("loaded") > 1_000, "{seen:?}");
    assert!(count("json-syntax") > 4_000, "{seen:?}");
    for reached in [
        "not-utf8",
        "control-character",
        "unterminated-comment",
        "duplicate-key",
        "unknown-key",
        "missing-key",
        "wrong-type",
        "unknown-value",
        "path-invalid",
    ] {
        assert!(count(reached) >= 10, "{reached} hardly occurred: {seen:?}");
    }
    assert!(seen.len() >= 14, "{seen:?}");
    assert_eq!(seen.values().sum::<u32>(), 20_000);
}

#[test]
fn ten_thousand_mutated_mappings_are_each_a_mapping_or_a_pinned_refusal() {
    let seeds: Vec<Vec<u8>> = [
        SMALL_MAPPING.to_owned(),
        fixture_text("listing_12_2.json"),
        fixture_text("canonical/listing_12_2.json"),
    ]
    .into_iter()
    .map(String::into_bytes)
    .collect();
    let mut random = Sequence(0x0659);
    let mut seen = std::collections::BTreeMap::new();
    for round in 0..10_000 {
        let mutated = mutate(&seeds[round % seeds.len()], &mut random);
        *seen.entry(judge_mapping(&mutated)).or_insert(0_u32) += 1;
    }
    let count = |code: &str| seen.get(code).copied().unwrap_or(0);
    assert!(count("loaded") > 500, "{seen:?}");
    for reached in [
        "json-syntax",
        "unterminated-comment",
        "duplicate-key",
        "unknown-key",
        "missing-key",
        "wrong-type",
        "unknown-value",
        "empty-name",
        "non-finite-number",
    ] {
        assert!(count(reached) >= 10, "{reached} hardly occurred: {seen:?}");
    }
    assert_eq!(seen.values().sum::<u32>(), 10_000);
}

#[test]
fn the_same_seed_judges_the_same_way_twice() {
    let run = || {
        let mut random = Sequence(7);
        (0..500)
            .map(|_| judge(&mutate(SMALL.as_bytes(), &mut random)))
            .collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}

#[test]
fn bytes_that_are_no_text_at_all_are_refused_by_code() {
    for (bytes, expected) in [
        (&b""[..], "json-syntax"),
        (b"\x00", "json-syntax"),
        (b"\xff\xfe", "not-utf8"),
        (b"\xef\xbb\xbf{}", "byte-order-mark"),
        (b"\xef\xbb", "not-utf8"),
        (b"{\"references\": [\"\xc3\"]}", "not-utf8"),
        (b"/* \xed\xa0\x80 */ {}", "not-utf8"),
        (b"//", "json-syntax"),
        (b"/", "json-syntax"),
        (b"\"", "json-syntax"),
        (b"\\", "json-syntax"),
        (b"\"\\", "json-syntax"),
        (b"/*\"*/\"", "json-syntax"),
        (b"\"/*", "json-syntax"),
    ] {
        assert_eq!(judge(bytes), expected, "{bytes:?}");
        // A mapping file passes the same stages.
        let as_mapping = judge_mapping(bytes);
        assert_eq!(as_mapping, expected, "{bytes:?}");
    }
}
