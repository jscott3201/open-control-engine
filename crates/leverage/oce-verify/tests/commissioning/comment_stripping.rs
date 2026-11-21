//! Comments (commissioning note, decision 3): `//` and `/* */` outside strings are removed
//! before the strict read, and nothing else is tolerated. A refusal's line and column still
//! point into the text as written, comments included; a marker inside a string is text; and a
//! block comment that is never closed is a typed refusal.

use oce_verify::Location;

use crate::{code_of, load, load_mapping, loaded, one_reference, refusal};

const PLAIN: &str = r#"{"references": [{"model": "M", "sequence": "s", "sampling": 60}]}"#;

#[test]
fn a_comment_may_stand_wherever_whitespace_may() {
    let plain = loaded(PLAIN);
    for commented in [
        "// before\n{\"references\": [{\"model\": \"M\", \"sequence\": \"s\", \"sampling\": 60}]}",
        "/* before */{\"references\": [{\"model\": \"M\", \"sequence\": \"s\", \"sampling\": 60}]}",
        "{\"references\": [{\"model\": \"M\", \"sequence\": \"s\", \"sampling\": 60}]}// after",
        "{\"references\": [{\"model\": \"M\", \"sequence\": \"s\", \"sampling\": 60}]}/* after */",
        "{/**/\"references\"/**/:/**/[/**/{/**/\"model\"/**/:/**/\"M\"/**/,/**/\"sequence\":\"s\",\"sampling\":/**/60/**/}/**/]/**/}/**/",
        "{\"references\": [{\"model\": \"M\", // one\n \"sequence\": \"s\", // two\r\n \"sampling\": 60}]}",
        "{\"references\": [{\"model\": \"M\", /* a\n * block\n * over lines\n */ \"sequence\": \"s\", \"sampling\": 60}]}",
        "{\"references\": [{\"model\": \"M\", /* \"sequence\": \"x\", */ \"sequence\": \"s\", \"sampling\": 60}]}",
        "{\"references\": [{\"model\": \"M\", \"sequence\": \"s\", \"sampling\": 60 // , \"sampling\": 61\n}]}",
        "{\"references\": [{\"model\": \"M\", \"sequence\": \"s\", \"sampling\": 60}]}\n// the end, with no line feed",
        "{\"references\": [{\"model\": \"M\", \"sequence\": \"s\", \"sampling\": 60}]}//",
        "/***/{\"references\": [{\"model\": \"M\", \"sequence\": \"s\", \"sampling\": 60}]}/****/",
        "/* // */{\"references\": [{\"model\": \"M\", \"sequence\": \"s\", \"sampling\": 60}]}// /* open",
        "/* é 🌡 \u{0} \u{7f} \t */{\"references\": [{\"model\": \"M\", \"sequence\": \"s\", \"sampling\": 60}]}",
    ] {
        let spec = load(commented).unwrap_or_else(|error| panic!("{error}\n{commented}"));
        assert_eq!(spec, plain, "{commented}");
        assert_eq!(spec.spec_id(), plain.spec_id());
    }
}

#[test]
fn a_comment_marker_inside_a_string_is_text() {
    for (written, expected) in [
        ("http://example.org/a", "http://example.org/a"),
        ("/* kept */", "/* kept */"),
        ("// kept", "// kept"),
        ("a /* b", "a /* b"),
        ("a */ b", "a */ b"),
        (
            "\\\"// after an escaped quote",
            "\"// after an escaped quote",
        ),
        ("\\\\", "\\"),
        ("ends in a backslash \\\\", "ends in a backslash \\"),
        ("\\\\\\\"//", "\\\"//"),
        ("\\u002f\\u002f", "//"),
        ("\\/\\/ and \\/* *\\/", "// and /* */"),
    ] {
        // A comment follows the string, so a string that ended early or late would show.
        let document = format!(
            "{{\"references\": [{{\"model\": \"{written}\" /* c */, // d\n \"sequence\": \"s\"}}]}}"
        );
        let spec = load(&document).unwrap_or_else(|error| panic!("{error}\n{document}"));
        assert_eq!(spec.references()[0].model(), expected, "{document}");
    }
    // In a key as well as in a value, and the canonical form keeps the text.
    let spec = loaded(&one_reference("\"outputs\": {\"s.//y/*\": {}}"));
    assert_eq!(spec.references()[0].outputs()[0].key(), "s.//y/*");
    assert!(spec.to_canonical_json().contains("\"s.//y/*\": {}"));
    // A string that never ends swallows the comment that follows; the reader refuses the
    // string, at the end of the text or at the raw line feed it may not hold.
    let swallowed = "{\"references\": [{\"model\": \"M // not a comment }]}";
    let end = u32::try_from(swallowed.len()).expect("fits") + 1;
    assert_eq!(refusal(swallowed), ("json-syntax", 1, end));
    let (code, line, column) = refusal("{\"references\": [{\"model\": \"M /* open\n}]}");
    assert_eq!((code, line, column), ("control-character", 1, 37));
}

#[test]
fn an_unterminated_block_comment_is_refused_where_it_opens() {
    for (document, line, column) in [
        ("/*", 1, 1),
        ("/* never closed", 1, 1),
        ("/*/", 1, 1),
        ("/* * /", 1, 1),
        ("{\"references\": []} /* trailing", 1, 20),
        ("{\"references\": [] /* inside }", 1, 19),
        (
            "{\n  \"references\": [],\n  /* opened on line three\n}\n",
            3,
            3,
        ),
        ("// é\n\té /* after two bytes of é", 2, 5),
        ("/* closed */ /* open", 1, 14),
        ("/**/ /*", 1, 6),
    ] {
        let error = load(document).expect_err("refused");
        assert_eq!(error.code(), "unterminated-comment", "{document:?}");
        assert_eq!(
            error.at(),
            Some(Location::new(line, column)),
            "{document:?}"
        );
        assert_eq!(error.detail(), "a `/*` comment is never closed");
        assert_eq!(error.subject(), format!("line {line} column {column}"));
    }
    // An open comment is found before anything inside or after it is judged.
    assert_eq!(code_of("{\"references\": NaN} /*"), "unterminated-comment");
    assert_eq!(
        code_of("{\"schema\": \"other\"} /*"),
        "unterminated-comment"
    );
    // A `/*` inside a string or a line comment opens nothing.
    assert_eq!(
        code_of("{\"references\": [], \"modelJsonDirectory\": \"/*\"}"),
        "loaded"
    );
    assert_eq!(code_of("{\"references\": []} // /*"), "loaded");
    // Block comments do not nest: the first `*/` closes, and what follows is not JSON.
    let (code, line, column) = refusal("/* a /* b */ c */ {\"references\": []}");
    assert_eq!((code, line, column), ("json-syntax", 1, 14));
}

#[test]
fn nothing_but_the_two_comment_forms_is_tolerated() {
    for (document, expected) in [
        // Other comment syntaxes.
        ("# a hash comment\n{\"references\": []}", "json-syntax"),
        ("<!-- markup -->{\"references\": []}", "json-syntax"),
        ("-- dashes\n{\"references\": []}", "json-syntax"),
        ("{\"references\": []} ; semicolon", "json-syntax"),
        // A lone slash is no comment.
        ("/ {\"references\": []}", "json-syntax"),
        ("{\"references\": [] /}", "json-syntax"),
        ("{\"references\": []} */", "json-syntax"),
        // What JSON5 and JavaScript allow and strict JSON does not.
        ("{\"references\": [],}", "json-syntax"),
        ("{\"references\": [,]}", "json-syntax"),
        ("{references: []}", "json-syntax"),
        ("{'references': []}", "json-syntax"),
        ("{\"references\": [], \"sampling\": +5}", "json-syntax"),
        ("{\"references\": [], \"sampling\": .5}", "json-syntax"),
        ("{\"references\": [], \"sampling\": 5.}", "json-syntax"),
        ("{\"references\": [], \"sampling\": 0x10}", "json-syntax"),
        (
            "{\"references\": [], \"sampling\": NaN}",
            "non-finite-number",
        ),
        (
            "{\"references\": [], \"sampling\": Infinity}",
            "non-finite-number",
        ),
        (
            "{\"references\": [], \"sampling\": -Infinity}",
            "non-finite-number",
        ),
        // A comment does not hide a trailing comma or join two tokens.
        ("{\"references\": [], /* c */}", "json-syntax"),
        ("{\"references\": [1/**/2]}", "json-syntax"),
        ("{\"references\": [], \"sampling\": 6/**/0}", "json-syntax"),
        (
            "{\"references\": [], \"sampling\": tr/**/ue}",
            "json-syntax",
        ),
        // A document of comments alone is no document.
        ("// only a comment", "json-syntax"),
        ("/* only a comment */", "json-syntax"),
        ("", "json-syntax"),
        // Two documents are not one.
        (
            "{\"references\": []} /* and */ {\"references\": []}",
            "json-syntax",
        ),
    ] {
        assert_eq!(code_of(document), expected, "{document:?}");
    }
    // A comment splits what it stands between: `6/**/0` is two tokens, never `60`.
    let (code, line, column) = refusal("{\"references\": [], \"sampling\": 6/**/0}");
    assert_eq!((code, line, column), ("json-syntax", 1, 37));
}

#[test]
fn a_refusal_points_into_the_text_as_written_comments_included() {
    // The same defect, an unknown key, behind comments of every shape: the place is where the
    // key is in the commented text, in lines and in bytes.
    for (document, line, column) in [
        ("{\"references\": [], \"bad\": 1}", 1, 20),
        ("{\"references\": [], /* c */ \"bad\": 1}", 1, 28),
        ("{\"references\": [], // c\n\"bad\": 1}", 2, 1),
        (
            "{\"references\": [], /* one\ntwo\nthree */ \"bad\": 1}",
            3,
            10,
        ),
        ("// one\n// two\n{\"references\": [],\n  \"bad\": 1}", 4, 3),
        // Columns count bytes: é is two, and the thermometer four.
        ("{\"references\": [], /* é */ \"bad\": 1}", 1, 29),
        ("{\"references\": [], /* 🌡 */ \"bad\": 1}", 1, 31),
        ("{\"references\": [], /* é\n */ \"bad\": 1}", 2, 5),
        // A carriage return ends a line comment and starts no line.
        ("{\"references\": [], // c\r\"bad\": 1}", 1, 25),
        ("{\"references\": [], // c\r\n\"bad\": 1}", 2, 1),
        // A tab is one byte.
        ("{\"references\": [],\t/*\t*/\t\"bad\": 1}", 1, 26),
    ] {
        assert_eq!(
            refusal(document),
            ("unknown-key", line, column),
            "{document:?}"
        );
        // The place holds the key.
        let at = usize::try_from(column).expect("fits") - 1;
        let text = document
            .split('\n')
            .nth(usize::try_from(line).expect("fits") - 1)
            .expect("the line");
        assert!(text[at..].starts_with("\"bad\""), "{document:?}");
    }
    // The commented listing: a defect planted in it is reported on its own line there.
    let commented = crate::fixture_text("listing_12_1_commented.json");
    let planted = commented.replace("\"sampling\": 60", "\"sampling\": -60");
    let (code, line, column) = refusal(&planted);
    assert_eq!(code, "invalid-sampling");
    let text = planted
        .lines()
        .nth(usize::try_from(line).expect("fits") - 1)
        .expect("the line");
    assert_eq!(
        &text[usize::try_from(column).expect("fits") - 1..],
        "-60",
        "{text}"
    );
    assert_eq!(
        line, 28,
        "the line of the commented file, not of the listing"
    );
}

#[test]
fn a_mapping_files_comments_are_removed_the_same_way() {
    let plain =
        r#"[{"cdl": {"name": "u", "type": "float"}, "device": {"name": "U", "type": "float"}}]"#;
    let commented = "// the one point\n[{\"cdl\": {\"name\": \"u\", /* K */ \"type\": \"float\"},\n  \"device\": {\"name\": \"U\", \"type\": \"float\"}} // end\n]";
    assert_eq!(
        load_mapping(commented).expect("loads"),
        load_mapping(plain).expect("loads")
    );
    let error = load_mapping("[ /* open").expect_err("refused");
    assert_eq!(error.code(), "unterminated-comment");
    assert_eq!(error.at(), Some(Location::new(1, 3)));
    let error = load_mapping("[\n // c\n {\"cdl\": 1}]").expect_err("refused");
    assert_eq!(
        (error.code(), error.at()),
        ("wrong-type", Some(Location::new(3, 10)))
    );
    // An inline mapping's comments too.
    let inline = one_reference(&format!("\"pointNameMapping\": /* inline */ {commented}"));
    assert_eq!(code_of(&inline), "loaded");
}

#[test]
fn a_byte_order_mark_is_not_a_comment() {
    let error = load("\u{feff}{\"references\": []}").expect_err("refused");
    assert_eq!(error.code(), "byte-order-mark");
    assert_eq!((error.at(), error.subject()), (None, "document"));
    // Inside a comment it is part of the comment.
    assert_eq!(code_of("/* \u{feff} */ {\"references\": []}"), "loaded");
    // After a comment and outside one it is not JSON.
    assert_eq!(code_of("// c\n\u{feff}{\"references\": []}"), "json-syntax");
}
