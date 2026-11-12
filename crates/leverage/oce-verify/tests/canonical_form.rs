//! The canonical form and `spec_id` (vectors note, "Canonical form"; decision 16): byte goldens
//! derived outside this crate, the load, write, load fixed point, and what does and does not
//! move an id.
//!
//! The goldens under `tests/fixtures/canonical/` and the ids below were produced by an
//! independent implementation of the canonical form and of FNV-1a-128 (a Python script over
//! `json`, `repr` and arbitrary-precision integers; `tests/fixtures/README.md` describes it), not
//! by `to_canonical_json`. They are never blessed.

mod support;

use oce_verify::{MAX_STRING_BYTES, VectorSet, VerifyLimits};

use support::{code_of, fixture_bytes, library_vectors, load, one, set_of, set_on};

/// A source document, its canonical golden and its `spec_id`.
struct Golden {
    name: &'static str,
    source: Vec<u8>,
    canonical: &'static str,
    spec_id: &'static str,
}

fn goldens() -> Vec<Golden> {
    vec![
        Golden {
            name: "AHU-0016",
            source: library_vectors("AHU-0016"),
            canonical: "canonical/AHU-0016.json",
            spec_id: "spec:fnv1a128:21155b0b2204f4353df7751eab35d1ee",
        },
        Golden {
            name: "AHU-0004",
            source: library_vectors("AHU-0004"),
            canonical: "canonical/AHU-0004.json",
            spec_id: "spec:fnv1a128:81931efafc9c025ad8141ae8135e286e",
        },
        Golden {
            name: "every_field",
            source: fixture_bytes("sets/every_field.json"),
            canonical: "canonical/every_field.json",
            spec_id: "spec:fnv1a128:ee654a0cb554fd824490e4a43fe5afd7",
        },
    ]
}

fn loaded(bytes: &[u8]) -> VectorSet {
    VectorSet::from_json_bytes(bytes, &VerifyLimits::default())
        .unwrap_or_else(|error| panic!("the document loads: {error}"))
}

/// FNV-1a-128, written again here so the ids are checked against something other than the
/// crate's own hash.
fn reference_hash(bytes: &[u8]) -> u128 {
    let mut hash = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d_u128;
    for byte in bytes {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    hash
}

#[test]
fn each_document_is_written_as_its_independently_derived_golden() {
    for golden in goldens() {
        let expected = String::from_utf8(fixture_bytes(golden.canonical)).expect("ASCII");
        let written = loaded(&golden.source).to_canonical_json();
        assert!(
            written == expected,
            "{}: the canonical form differs from its golden\n--- expected\n{expected}\n--- written\n{written}",
            golden.name
        );
    }
}

#[test]
fn load_write_load_is_a_fixed_point() {
    for golden in goldens() {
        let first = loaded(&golden.source);
        let written = first.to_canonical_json();
        let second = loaded(written.as_bytes());
        assert_eq!(
            first, second,
            "{}: the set survives its canonical form",
            golden.name
        );
        assert_eq!(second.to_canonical_json(), written, "{}", golden.name);
        assert_eq!(second.spec_id(), first.spec_id(), "{}", golden.name);
        // The golden itself is canonical: it loads and writes back to its own bytes.
        let checked_in = fixture_bytes(golden.canonical);
        assert_eq!(
            loaded(&checked_in).to_canonical_json().as_bytes(),
            checked_in
        );
    }
}

#[test]
fn each_spec_id_is_its_golden_and_the_reference_hash_of_the_golden_bytes() {
    for golden in goldens() {
        let set = loaded(&golden.source);
        assert_eq!(set.spec_id(), golden.spec_id, "{}", golden.name);
        let bytes = fixture_bytes(golden.canonical);
        assert_eq!(
            format!("spec:fnv1a128:{:032x}", reference_hash(&bytes)),
            golden.spec_id,
            "{}",
            golden.name
        );
        assert_eq!(set.spec_id().len(), "spec:fnv1a128:".len() + 32);
    }
}

#[test]
fn the_canonical_form_is_ascii_with_line_feeds_and_one_final_line_feed() {
    for golden in goldens() {
        let written = loaded(&golden.source).to_canonical_json();
        assert!(written.is_ascii(), "{}", golden.name);
        assert!(
            written.ends_with("}\n") && !written.ends_with("\n\n"),
            "{}",
            golden.name
        );
        assert!(
            !written.contains('\r') && !written.contains('\t'),
            "{}",
            golden.name
        );
        assert!(
            written
                .bytes()
                .all(|byte| byte == b'\n' || (0x20..0x7f).contains(&byte)),
            "{}",
            golden.name
        );
        for line in written.lines() {
            assert!(
                !line.ends_with(' '),
                "{}: trailing space in {line:?}",
                golden.name
            );
            let indent = line.len() - line.trim_start_matches(' ').len();
            assert_eq!(indent % 2, 0, "{}: {line:?}", golden.name);
        }
    }
}

#[test]
fn the_smallest_set_is_written_byte_for_byte() {
    let written = load(&set_on("60.0", "0", ""))
        .expect("loads")
        .to_canonical_json();
    assert_eq!(
        written,
        "{\n  \"clock\": {\n    \"horizon_s\": 0,\n    \"step_s\": 60.0\n  },\n  \"scenarios\": [],\n  \"schema\": \"oce/vectors/v1\"\n}\n"
    );
    let scenario = load(&one("", "")).expect("loads").to_canonical_json();
    assert_eq!(
        scenario,
        "{\n  \"clock\": {\n    \"horizon_s\": 1800,\n    \"step_s\": 60\n  },\n  \"scenarios\": [\n    {\n      \"expect\": [],\n      \"inputs\": {},\n      \"name\": \"s\"\n    }\n  ],\n  \"schema\": \"oce/vectors/v1\"\n}\n"
    );
}

#[test]
fn a_number_is_written_in_the_kind_and_spelling_the_form_fixes() {
    // (as written, as the canonical form writes it); values, where every kind is allowed.
    let cases = [
        ("0", "0"),
        ("-0", "0"),
        ("-0.0", "-0.0"),
        ("0.0", "0.0"),
        ("0e0", "0.0"),
        ("1", "1"),
        ("1.0", "1.0"),
        ("1.50", "1.5"),
        ("1E0", "1.0"),
        ("1e+2", "100.0"),
        ("100", "100"),
        ("0.0001", "0.0001"),
        ("1e-4", "0.0001"),
        ("0.00009999999999999999", "9.999999999999999e-5"),
        ("0.00001", "1e-5"),
        ("1.5E-7", "1.5e-7"),
        ("9999999999999998.0", "9999999999999998.0"),
        ("9999999999999998", "9999999999999998"),
        ("10000000000000000.0", "1e16"),
        ("1e16", "1e16"),
        ("10000000000000000", "10000000000000000"),
        ("123456789012345678.0", "1.2345678901234568e17"),
        ("5e-324", "5e-324"),
        ("4.9406564584124654e-324", "5e-324"),
        ("2.2250738585072014e-308", "2.2250738585072014e-308"),
        ("1.7976931348623157E+308", "1.7976931348623157e308"),
        ("-1.7976931348623157e308", "-1.7976931348623157e308"),
        ("1e-400", "0.0"),
        ("-1e-400", "-0.0"),
        ("0.1", "0.1"),
        ("0.30000000000000004", "0.30000000000000004"),
        (
            "0.3000000000000000444089209850062616169452667236328125",
            "0.30000000000000004",
        ),
        ("9223372036854775807", "9223372036854775807"),
        ("-9223372036854775808", "-9223372036854775808"),
        ("9007199254740993", "9007199254740993"),
        ("9007199254740993.0", "9007199254740992.0"),
        ("true", "true"),
        ("false", "false"),
    ];
    for (written, canonical) in cases {
        let document = load(&one(&format!("\"u\": {written}"), "")).expect("loads");
        let text = document.to_canonical_json();
        let line = text
            .lines()
            .find(|line| line.trim_start().starts_with("\"u\":"))
            .expect("the input line");
        assert_eq!(line.trim(), format!("\"u\": {canonical}"), "{written}");
        // And the spelling is stable: writing it again changes nothing.
        let again = load(&text)
            .expect("the canonical form loads")
            .to_canonical_json();
        assert_eq!(again, text, "{written}");
    }
}

#[test]
fn a_time_keeps_the_kind_it_was_written_as() {
    let written = load(&set_on("6e1", "1800", ""))
        .expect("loads")
        .to_canonical_json();
    assert!(written.contains("\"step_s\": 60.0") && written.contains("\"horizon_s\": 1800"));
    let negative_zero = load(&set_on("60", "-0.0", ""))
        .expect("loads")
        .to_canonical_json();
    assert!(
        negative_zero.contains("\"horizon_s\": -0.0,\n"),
        "the token is kept as written"
    );
    let integer_zero = load(&set_on("60", "-0", ""))
        .expect("loads")
        .to_canonical_json();
    assert!(
        integer_zero.contains("\"horizon_s\": 0,\n"),
        "-0 is the integer zero"
    );
}

#[test]
fn spelling_does_not_move_the_id_and_meaning_does() {
    let base = one("\"u\": 1, \"v\": [{\"t\": 0, \"value\": true}]", "");
    let id = load(&base).expect("loads").spec_id();
    // The same set, spelled differently: whitespace, member order, escapes, an exponent that
    // stays an integer token is not one, so only spellings of the same token kinds appear here.
    let respelled = [
        base.replace(", ", ",\n\t ").replace(": ", " :  "),
        base.replace(
            "\"u\": 1, \"v\": [{\"t\": 0, \"value\": true}]",
            "\"v\": [{\"value\": true, \"t\": 0}], \"u\": 1",
        ),
        base.replace("\"u\"", "\"\\u0075\"")
            .replace("oce/vectors/v1", "oce\\/vectors\\/v1"),
        base.replace("\"t\": 0", "\"t\": -0"),
        format!("\n\n{base}\n"),
    ];
    for document in &respelled {
        assert_ne!(document, &base);
        assert_eq!(load(document).expect("loads").spec_id(), id, "{document}");
    }
    // A different set: a number's kind, a value, a name, a time, an absent or empty params,
    // a constant against a one-step list, scenario order.
    let changed = [
        base.replace("\"u\": 1", "\"u\": 1.0"),
        base.replace("\"u\": 1", "\"u\": 2"),
        base.replace("\"u\": 1", "\"w\": 1"),
        base.replace("\"value\": true", "\"value\": false"),
        base.replace("\"step_s\": 60", "\"step_s\": 60.0"),
        base.replace("\"step_s\": 60", "\"step_s\": 30"),
        base.replace("\"name\": \"s\"", "\"name\": \"S\""),
        base.replace("\"name\": \"s\"", "\"name\": \"s\", \"params\": {}"),
        base.replace("\"name\": \"s\"", "\"name\": \"s\", \"description\": \"\""),
        base.replace("[{\"t\": 0, \"value\": true}]", "true"),
        base.replace(
            "\"expect\": []",
            "\"expect\": [{\"output\": \"y\", \"from_s\": 0, \"to_s\": 0, \"equals\": 1}]",
        ),
        base.replace("\"scenarios\": [", "\"params\": {}, \"scenarios\": ["),
    ];
    let mut ids = std::collections::BTreeSet::from([id.clone()]);
    for document in &changed {
        let other = load(document)
            .unwrap_or_else(|e| panic!("{document}: {e}"))
            .spec_id();
        assert_ne!(other, id, "{document}");
        assert!(
            ids.insert(other),
            "two different sets share an id: {document}"
        );
    }
    // Scenario order is part of the set; expectation order too.
    let named = |name: &str| format!("{{\"name\": \"{name}\", \"inputs\": {{}}, \"expect\": []}}");
    let forward = load(&set_of(&format!("{}, {}", named("a"), named("b")))).expect("loads");
    let backward = load(&set_of(&format!("{}, {}", named("b"), named("a")))).expect("loads");
    assert_ne!(forward.spec_id(), backward.spec_id());
}

#[test]
fn map_keys_are_written_in_utf8_byte_order() {
    let set = load(&one(
        "\"z\": 1, \"\\ud800\\udc00\": 2, \"a\": 3, \"é\": 4, \"b\": 5, \"\\uffff\": 6, \"B\": 7, \"a.b\": 8, \"a_b\": 9",
        "",
    ))
    .expect("loads");
    let names: Vec<&str> = set.scenarios()[0]
        .inputs()
        .iter()
        .map(|input| input.name())
        .collect();
    assert_eq!(
        names,
        [
            "B",
            "a",
            "a.b",
            "a_b",
            "b",
            "z",
            "é",
            "\u{ffff}",
            "\u{10000}"
        ]
    );
    let written = set.to_canonical_json();
    let keys: Vec<&str> = written
        .lines()
        .filter(|line| line.starts_with("        \""))
        .map(|line| line.trim().split(':').next().expect("a key"))
        .collect();
    assert_eq!(
        keys,
        [
            "\"B\"",
            "\"a\"",
            "\"a.b\"",
            "\"a_b\"",
            "\"b\"",
            "\"z\"",
            "\"\\u00e9\"",
            "\"\\uffff\"",
            "\"\\ud800\\udc00\""
        ]
    );
}

#[test]
fn the_canonical_form_loads_again_only_while_it_is_within_the_bounds() {
    // A string of 4,096 bytes of two-byte characters loads; written canonically each character
    // is six bytes, so the canonical document holds a string past the bound and does not load.
    // The proviso is documented on `to_canonical_json`; this pins it.
    let name = "é".repeat(MAX_STRING_BYTES / 2);
    let document = set_of(&format!(
        "{{\"name\": \"{name}\", \"inputs\": {{}}, \"expect\": []}}"
    ));
    let set = load(&document).expect("the string is at the bound as written");
    assert_eq!(set.scenarios()[0].name(), name);
    let written = set.to_canonical_json();
    assert!(written.contains(&"\\u00e9".repeat(MAX_STRING_BYTES / 2)));
    assert_eq!(code_of(&written), "string-too-long");
    // One character fewer than a sixth of the bound, and the canonical form is within it.
    let short = "é".repeat(MAX_STRING_BYTES / 6);
    let document = set_of(&format!(
        "{{\"name\": \"{short}\", \"inputs\": {{}}, \"expect\": []}}"
    ));
    let written = load(&document).expect("loads").to_canonical_json();
    assert_eq!(load(&written).expect("loads").to_canonical_json(), written);
}

#[test]
fn writing_is_deterministic() {
    for golden in goldens() {
        let set = loaded(&golden.source);
        assert_eq!(set.to_canonical_json(), set.to_canonical_json());
        assert_eq!(set.spec_id(), loaded(&golden.source).spec_id());
    }
}
