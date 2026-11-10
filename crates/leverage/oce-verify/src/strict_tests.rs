//! The strict cursor against `serde_json` as a syntax oracle, and its own refusals at their
//! exact places.

use super::*;

fn refusal(text: &str) -> (VectorErrorKind, u32, u32) {
    let refused = scan(text).expect_err("refused");
    (refused.kind, refused.at.line, refused.at.column)
}

/// Whether `serde_json` reads `text` as one JSON document.
fn oracle_accepts(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text).is_ok()
}

/// A deterministic 64-bit generator (SplitMix64), so every run mutates the same way.
struct Sequence(u64);

impl Sequence {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(bound).expect("fits")).expect("fits")
    }
}

/// Documents every JSON reader agrees on, and near misses.
const SEEDS: &[&str] = &[
    r#"{"schema":"oce/vectors/v1","clock":{"step_s":60,"horizon_s":1800},"scenarios":[]}"#,
    r#"{"a":[1,2.5,-0,-0.0,1e3,1E-3,true,false,null,"x"],"b":{"c":{"d":[[],{}]}}}"#,
    r#"[ "\u00e9\ud83d\ude00\n\"\\\/", 0, -1, 9223372036854775807, 0.1e+2 ]"#,
    " \t\r\n{ \"k\" : [ 1 , 2 ] }\n",
    "\"just a string\"",
    "12",
    "-1.5e-7",
    "true",
    "null",
    "[]",
    "{}",
];

/// The bytes a mutation draws from: every structural character, digits, signs, an exponent,
/// escapes, whitespace and letters that begin or break a word.
const ALPHABET: &[u8] = b"{}[]:,\"\\ \n\t0123456789-+.eEtrufalsn/bNI";

fn mutated(seed: &str, random: &mut Sequence) -> String {
    let mut bytes = seed.as_bytes().to_vec();
    for _ in 0..=random.below(3) {
        let at = random.below(bytes.len() + 1);
        let byte = ALPHABET[random.below(ALPHABET.len())];
        match random.below(3) {
            0 if at < bytes.len() => bytes[at] = byte,
            1 if at < bytes.len() => {
                bytes.remove(at);
            }
            _ => bytes.insert(at, byte),
        }
    }
    // The seeds' few non-ASCII characters can be cut; keep only what is still UTF-8.
    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
fn the_scan_and_the_syntax_oracle_agree_on_twenty_thousand_mutated_documents() {
    let mut random = Sequence(519);
    let (mut accepted, mut syntax, mut profile) = (0_u32, 0_u32, 0_u32);
    for round in 0..20_000 {
        let text = mutated(SEEDS[round % SEEDS.len()], &mut random);
        let oracle = oracle_accepts(&text);
        match scan(&text) {
            Ok(_) => {
                assert!(oracle, "the scan accepts what is not JSON: {text:?}");
                accepted += 1;
            }
            Err(refused) if refused.kind == VectorErrorKind::JsonSyntax => {
                assert!(
                    !oracle,
                    "a syntax refusal of valid JSON: {text:?} ({})",
                    refused.detail
                );
                syntax += 1;
            }
            // A profile refusal (a bound, a repeated key, a control character, a number the
            // profile does not take) may be valid JSON or not; either way it is refused.
            Err(_) => profile += 1,
        }
        if !oracle {
            assert!(
                scan(&text).is_err(),
                "the scan accepts what is not JSON: {text:?}"
            );
        }
    }
    // Each outcome occurred often enough that neither arm was vacuous.
    assert!(accepted > 1_000, "accepted {accepted}");
    assert!(syntax > 5_000, "syntax {syntax}");
    assert!(profile > 100, "profile {profile}");
}

#[test]
fn every_seed_document_is_accepted_and_every_near_miss_is_a_syntax_refusal() {
    for seed in SEEDS {
        assert!(scan(seed).is_ok(), "{seed}");
        assert!(oracle_accepts(seed), "{seed}");
    }
    for near_miss in [
        "",
        " ",
        "{",
        "}",
        "[1,]",
        "{\"a\":1,}",
        "{\"a\" 1}",
        "{a:1}",
        "{'a':1}",
        "[1 2]",
        "01",
        "-",
        "1.",
        ".5",
        "+1",
        "1e",
        "1e+",
        "0x10",
        "tru",
        "nul",
        "falsey",
        "\"open",
        "\"\\x\"",
        "\"\\u12\"",
        "\"\\ud800\"",
        "\"\\ud800\\u0041\"",
        "\"\\udc00\"",
        "1 2",
        "{} {}",
        "[]]",
        "// comment\n1",
    ] {
        assert!(!oracle_accepts(near_miss), "{near_miss:?}");
        assert_eq!(
            scan(near_miss).expect_err("refused").kind,
            VectorErrorKind::JsonSyntax,
            "{near_miss:?}"
        );
    }
}

#[test]
fn a_string_is_unescaped_exactly_as_the_oracle_unescapes_it() {
    for token in [
        r#""""#,
        r#""plain""#,
        r#""a\"b\\c\/d\ne""#,
        r#""\u0041\u00e9\u20ac\uffff""#,
        r#""\ud83d\ude00\ud800\udc00\udbff\udfff""#,
        r#""\u004A\u00E9""#,
        "\"é€😀\u{10000}\u{a0}\"",
        r#""\u000a""#,
    ] {
        let expected: String = serde_json::from_str(token).expect("the oracle reads it");
        let mut cursor = Cursor::new(token);
        assert_eq!(cursor.string().expect("read"), expected, "{token}");
        cursor.finish().expect("nothing left");
    }
}

#[test]
fn a_number_keeps_its_kind_and_a_real_reads_to_the_oracles_bits() {
    for (token, expected) in [
        ("0", Number::Integer(0)),
        ("-0", Number::Integer(0)),
        ("60", Number::Integer(60)),
        ("-17", Number::Integer(-17)),
        ("9223372036854775807", Number::Integer(i64::MAX)),
        ("-9223372036854775808", Number::Integer(i64::MIN)),
        ("9007199254740993", Number::Integer(9_007_199_254_740_993)),
    ] {
        assert_eq!(Cursor::new(token).number(), Ok(expected), "{token}");
    }
    for token in [
        "0.0",
        "-0.0",
        "1.0",
        "60.0",
        "1e0",
        "1E0",
        "1e+2",
        "1e-2",
        "0.1",
        "4.3",
        "1e-4",
        "1e16",
        "1.7976931348623157e308",
        "5e-324",
        "2.5e-324",
        "1e-400",
        "-1e-400",
        "9007199254740993.0",
        "0.30000000000000004",
        "123456789012345678901234567890.0",
        "0.000000000000000000000000000000000000000000000000000000000000001",
    ] {
        let oracle: f64 = serde_json::from_str(token).expect("the oracle reads it");
        let Ok(Number::Real(value)) = Cursor::new(token).number() else {
            panic!("{token} is a real");
        };
        assert_eq!(value.to_bits(), oracle.to_bits(), "{token}");
    }
    // Negative zero keeps its sign as a real and loses it as an integer token.
    assert_eq!(Cursor::new("-0.0").number(), Ok(Number::Real(-0.0)));
    assert_eq!(Cursor::new("-0e0").number(), Ok(Number::Real(-0.0)));
    assert_eq!(Cursor::new("1e-400").number(), Ok(Number::Real(0.0)));
}

#[test]
fn an_integer_token_outside_i64_is_refused_one_past_each_end() {
    for token in [
        "9223372036854775808",
        "-9223372036854775809",
        "18446744073709551616",
        "100000000000000000000000000000",
    ] {
        assert_eq!(
            refusal(token),
            (VectorErrorKind::IntegerOutOfRange, 1, 1),
            "{token}"
        );
    }
    assert!(scan("9223372036854775807").is_ok());
    assert!(scan("-9223372036854775808").is_ok());
    // With a fraction or exponent the same digits are a real, and finite.
    assert!(scan("9223372036854775808.0").is_ok());
    assert!(scan("1e30").is_ok());
}

#[test]
fn a_non_finite_number_is_refused_however_it_is_spelled() {
    for token in [
        "NaN",
        "Infinity",
        "-Infinity",
        "1e309",
        "-1e309",
        "1e999999999999",
    ] {
        assert_eq!(
            refusal(&format!("[{token}]")),
            (VectorErrorKind::NonFiniteNumber, 1, 2),
            "{token}"
        );
    }
    // The largest finite real is accepted; the next decade is not.
    assert!(scan("[1.7976931348623157e308]").is_ok());
    assert!(scan("[1.8e308]").is_err());
}

#[test]
fn every_control_character_is_refused_raw_and_escaped_and_line_feed_only_escaped() {
    for code in (0x00_u32..0x20).chain(0x7f..=0x9f) {
        let escaped = format!("\"a\\u{code:04x}\"");
        if code == 0x0a {
            assert!(scan(&escaped).is_ok(), "an escaped line feed is text");
        } else {
            assert_eq!(
                refusal(&escaped),
                (VectorErrorKind::ControlCharacter, 1, 3),
                "{escaped}"
            );
            let upper = format!("\"a\\u{code:04X}\"");
            assert_eq!(refusal(&upper).0, VectorErrorKind::ControlCharacter);
        }
        let raw = format!("\"a{}\"", char::from_u32(code).expect("a scalar"));
        assert_eq!(
            refusal(&raw),
            (VectorErrorKind::ControlCharacter, 1, 3),
            "raw U+{code:04X}"
        );
    }
    for short in ["\\b", "\\f", "\\r", "\\t"] {
        assert_eq!(
            refusal(&format!("\"a{short}\"")),
            (VectorErrorKind::ControlCharacter, 1, 3)
        );
    }
    // The neighbours of the refused ranges are text.
    for neighbour in ['\u{20}', '\u{7e}', '\u{a0}'] {
        assert!(scan(&format!("\"{neighbour}\"")).is_ok());
        assert!(scan(&format!("\"\\u{:04x}\"", u32::from(neighbour))).is_ok());
    }
    // A key is a string too.
    assert_eq!(
        refusal("{\"a\\tb\": 1}"),
        (VectorErrorKind::ControlCharacter, 1, 4)
    );
}

#[test]
fn a_repeated_key_is_refused_whatever_its_spelling_and_at_any_depth() {
    assert_eq!(
        refusal(r#"{"a": 1, "a": 2}"#),
        (VectorErrorKind::DuplicateKey, 1, 10)
    );
    // Compared after unescaping: `\u0061` is `a`.
    assert_eq!(
        refusal(r#"{"a": 1, "\u0061": 2}"#),
        (VectorErrorKind::DuplicateKey, 1, 10)
    );
    assert_eq!(
        refusal("{\"x\": [{\"k\": 1,\n \"k\": 2}]}").0,
        VectorErrorKind::DuplicateKey
    );
    assert_eq!(refusal("{\"x\": [{\"k\": 1,\n \"k\": 2}]}").1, 2);
    // The same key in two different objects is no repeat, and case matters.
    assert!(scan(r#"{"a": {"a": 1}, "b": {"a": 2}, "A": 3}"#).is_ok());
    assert!(scan(r#"[{"a": 1}, {"a": 2}]"#).is_ok());
}

fn nested(depth: usize) -> String {
    format!("{}{}", "[".repeat(depth), "]".repeat(depth))
}

#[test]
fn nesting_is_accepted_at_the_bound_and_refused_one_past() {
    assert!(scan(&nested(MAX_DEPTH)).is_ok());
    assert_eq!(
        refusal(&nested(MAX_DEPTH + 1)),
        (VectorErrorKind::NestingTooDeep, 1, 9)
    );
    let objects = |depth: usize| format!("{}1{}", "{\"k\":".repeat(depth), "}".repeat(depth));
    assert!(scan(&objects(MAX_DEPTH)).is_ok());
    assert_eq!(
        refusal(&objects(MAX_DEPTH + 1)).0,
        VectorErrorKind::NestingTooDeep
    );
    // Depth is nesting, not the count of containers: siblings do not add up.
    let siblings = format!("[{}]", vec![nested(MAX_DEPTH - 1); 50].join(","));
    assert!(scan(&siblings).is_ok());
    // A document far deeper than any stack would like is refused at level nine.
    assert_eq!(
        refusal(&"[".repeat(1_000_000)),
        (VectorErrorKind::NestingTooDeep, 1, 9)
    );
}

#[test]
fn a_list_is_accepted_at_the_bound_and_refused_one_past() {
    let list = |len: usize| format!("[{}]", vec!["0"; len].join(","));
    assert!(scan(&list(MAX_LIST_LEN)).is_ok());
    assert_eq!(
        refusal(&format!("\n {}", list(MAX_LIST_LEN + 1))),
        (VectorErrorKind::ListTooLong, 2, 2),
        "the refusal names where the list opens"
    );
    // The bound is per list.
    let two = format!("[{},{}]", list(MAX_LIST_LEN), list(MAX_LIST_LEN));
    assert!(scan(&two).is_ok());
}

#[test]
fn a_string_is_accepted_at_the_bound_and_refused_one_past_as_written() {
    let string = |len: usize| format!("\"{}\"", "a".repeat(len));
    assert!(scan(&string(MAX_STRING_BYTES)).is_ok());
    assert_eq!(
        refusal(&string(MAX_STRING_BYTES + 1)),
        (VectorErrorKind::StringTooLong, 1, 1)
    );
    // An escape counts as the bytes it is written with: 682 `\u00e9` are 4,092 bytes, 683 are
    // 4,098; and raw two-byte characters count two each.
    let escaped = |count: usize| format!("\"{}\"", "\\u00e9".repeat(count));
    assert!(scan(&escaped(682)).is_ok());
    assert_eq!(refusal(&escaped(683)).0, VectorErrorKind::StringTooLong);
    let raw = |count: usize| format!("\"{}\"", "é".repeat(count));
    assert!(scan(&raw(MAX_STRING_BYTES / 2)).is_ok());
    assert_eq!(
        refusal(&raw(MAX_STRING_BYTES / 2 + 1)).0,
        VectorErrorKind::StringTooLong
    );
    // A key is held to the same bound.
    let key = format!("{{{}: 1}}", string(MAX_STRING_BYTES + 1));
    assert_eq!(refusal(&key), (VectorErrorKind::StringTooLong, 1, 2));
    // An unterminated string past the bound is refused for its length, without reading on.
    let unterminated = format!("\"{}", "a".repeat(MAX_STRING_BYTES * 4));
    assert_eq!(refusal(&unterminated).0, VectorErrorKind::StringTooLong);
}

#[test]
fn a_refusal_names_the_line_and_column_it_stopped_at() {
    assert_eq!(
        refusal("{\n  \"a\": 1,\n  \"b\": [1, 2,, 3]\n}"),
        (VectorErrorKind::JsonSyntax, 3, 14)
    );
    assert_eq!(
        refusal("{\"a\": 1} x"),
        (VectorErrorKind::JsonSyntax, 1, 10)
    );
    assert_eq!(
        refusal("\r\n\t [1,\n\n NaN]"),
        (VectorErrorKind::NonFiniteNumber, 4, 2)
    );
    // Columns count bytes: `é` is two.
    assert_eq!(refusal("[\"é\", 01]"), (VectorErrorKind::JsonSyntax, 1, 9));
}

#[test]
fn the_scan_returns_a_root_schema_string_and_nothing_else() {
    assert_eq!(
        scan(r#"{"clock": {"schema": "inner"}, "schema": "oce/vectors/v1"}"#),
        Ok(Some("oce/vectors/v1".to_owned()))
    );
    assert_eq!(
        scan(r#"{"schema": "cxf-library\/vectors\/v1"}"#),
        Ok(Some("cxf-library/vectors/v1".to_owned())),
        "unescaped"
    );
    assert_eq!(scan(r#"{"schema": 1}"#), Ok(None), "not a string");
    assert_eq!(
        scan(r#"{"clock": {"schema": "inner"}}"#),
        Ok(None),
        "not at the root"
    );
    assert_eq!(
        scan(r#"[{"schema": "oce/vectors/v1"}]"#),
        Ok(None),
        "the root is a list"
    );
    assert_eq!(scan(r#""oce/vectors/v1""#), Ok(None));
}

#[test]
fn the_first_defect_in_document_order_is_the_one_refused() {
    // A control character before a syntax error, and the reverse.
    assert_eq!(
        refusal("[\"\\t\", 01]").0,
        VectorErrorKind::ControlCharacter
    );
    assert_eq!(refusal("[01, \"\\t\"]").0, VectorErrorKind::JsonSyntax);
    // A repeated key before an out-of-range integer, and the reverse.
    assert_eq!(
        refusal(r#"{"a": 1, "a": 99999999999999999999}"#).0,
        VectorErrorKind::DuplicateKey
    );
    assert_eq!(
        refusal(r#"{"a": 99999999999999999999, "a": 1}"#).0,
        VectorErrorKind::IntegerOutOfRange
    );
}

#[test]
fn a_typed_read_of_the_wrong_kind_says_what_it_found() {
    for (text, found) in [
        ("[]", "a list"),
        ("\"x\"", "a string"),
        ("1", "a number"),
        ("true", "a boolean"),
        ("null", "null"),
    ] {
        let refused = Cursor::new(text)
            .expect(Kind::Object, "a clock, an object")
            .expect_err("not an object");
        assert_eq!(refused.kind, VectorErrorKind::WrongType);
        assert_eq!(
            refused.detail,
            format!("expected a clock, an object, found {found}")
        );
    }
    assert!(Cursor::new(" {").expect(Kind::Object, "an object").is_ok());
}
