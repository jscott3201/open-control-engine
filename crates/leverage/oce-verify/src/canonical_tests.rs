//! The canonical writer's bytes: key order, layout and escapes, against hand-written documents.

use super::*;

fn raw(text: &str) -> Json {
    Json::Raw(text.to_owned())
}

#[test]
fn keys_are_sorted_by_their_utf8_bytes_whatever_order_they_are_given_in() {
    // By UTF-8 bytes: "" < "B" (42) < "a" (61) < "b" < "z" < "é" (c3 a9) < U+FFFF (ef bf bf)
    // < U+10000 (f0 90 80 80). By UTF-16 code units U+10000 (d800 dc00) would sort before
    // U+FFFF, so the last two rows tell the two orders apart.
    let keys = ["z", "\u{10000}", "a", "é", "", "b", "\u{ffff}", "B"];
    let object = Json::object(keys.iter().map(|key| ((*key).to_owned(), raw("0"))));
    let expected = "{\n  \"\": 0,\n  \"B\": 0,\n  \"a\": 0,\n  \"b\": 0,\n  \"z\": 0,\n  \"\\u00e9\": 0,\n  \"\\uffff\": 0,\n  \"\\ud800\\udc00\": 0\n}\n";
    assert_eq!(object.to_document(), expected);
    // Any other order of the same entries writes the same bytes.
    let reversed = Json::object(keys.iter().rev().map(|key| ((*key).to_owned(), raw("0"))));
    assert_eq!(reversed.to_document(), expected);
}

#[test]
fn a_prefix_sorts_before_the_key_it_starts() {
    let object = Json::object(
        ["ab", "a", "a.b", "a_b", "aB"]
            .iter()
            .map(|key| ((*key).to_owned(), raw("0"))),
    );
    // 2e (.) < 42 (B) < 5f (_) < 62 (b), and the bare prefix first.
    assert_eq!(
        object.to_document(),
        "{\n  \"a\": 0,\n  \"a.b\": 0,\n  \"aB\": 0,\n  \"a_b\": 0,\n  \"ab\": 0\n}\n"
    );
}

#[test]
fn nesting_is_indented_two_spaces_and_empty_containers_stay_on_one_line() {
    let document = Json::object([
        entry(
            "list",
            Json::List(vec![raw("1"), Json::List(vec![]), raw("true")]),
        ),
        entry("empty", Json::object([])),
        entry(
            "nested",
            Json::object([entry(
                "inner",
                Json::List(vec![Json::object([entry("k", raw("null"))])]),
            )]),
        ),
    ]);
    let expected = "\
{
  \"empty\": {},
  \"list\": [
    1,
    [],
    true
  ],
  \"nested\": {
    \"inner\": [
      {
        \"k\": null
      }
    ]
  }
}
";
    assert_eq!(document.to_document(), expected);
    assert_eq!(Json::List(vec![]).to_document(), "[]\n");
    assert_eq!(Json::object([]).to_document(), "{}\n");
    assert_eq!(raw("1.5").to_document(), "1.5\n");
}

#[test]
fn a_string_is_ascii_only_with_lowercase_hex_and_surrogate_pairs() {
    let cases = [
        ("", "\"\""),
        ("plain text ~ !", "\"plain text ~ !\""),
        ("a\"b\\c", "\"a\\\"b\\\\c\""),
        ("two\nlines", "\"two\\nlines\""),
        ("a/b", "\"a/b\""),
        ("é", "\"\\u00e9\""),
        ("→ — ·", "\"\\u2192 \\u2014 \\u00b7\""),
        ("😀", "\"\\ud83d\\ude00\""),
        ("\u{10ffff}", "\"\\udbff\\udfff\""),
        ("\u{a0}\u{ff}\u{abcd}", "\"\\u00a0\\u00ff\\uabcd\""),
        // Never in a loaded set (the reader refuses them), but the writer still escapes them.
        ("\u{7f}\t\u{0}", "\"\\u007f\\u0009\\u0000\""),
    ];
    for (text, expected) in cases {
        let written = Json::str(text).to_document();
        assert_eq!(written, format!("{expected}\n"), "{text:?}");
        assert!(written.is_ascii());
        assert!(!written.contains("\\U") && !written.contains("\\u00E9"));
    }
}

#[test]
fn a_written_string_reads_back_as_the_same_string() {
    for text in ["", "a\"b\\c/d\ne", "é→😀\u{10ffff}\u{a0}", "~ {}[]:,"] {
        let written = Json::str(text).to_document();
        let read: String = serde_json::from_str(&written).expect("JSON");
        assert_eq!(read, text);
    }
}

#[test]
fn a_serde_value_is_written_with_sorted_keys_at_every_depth() {
    let value: serde_json::Value = serde_json::from_str(
        r#"{"b": {"z": 1, "a": [true, null, "x", {"q": 2, "p": 3}]}, "a": 0}"#,
    )
    .expect("JSON");
    let expected = "\
{
  \"a\": 0,
  \"b\": {
    \"a\": [
      true,
      null,
      \"x\",
      {
        \"p\": 3,
        \"q\": 2
      }
    ],
    \"z\": 1
  }
}
";
    assert_eq!(sorted(&value).to_document(), expected);
}
