//! Comment removal against a second implementation, exhaustively over short texts, and its
//! cost: one buffer of the text's length, or none.

use super::*;

/// A second implementation, written another way: it cuts the text with `find` and
/// `strip_prefix`, a whole comment or a whole string at a time. `Err` is the byte offset of the
/// comment that is never closed.
fn reference(text: &str) -> Result<String, usize> {
    let mut out = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        let offset = text.len() - rest.len();
        let taken = if let Some(comment) = rest.strip_prefix("//") {
            let end = comment.find(['\n', '\r']).unwrap_or(comment.len());
            out.push_str(&" ".repeat(2 + end));
            2 + end
        } else if let Some(comment) = rest.strip_prefix("/*") {
            let Some(close) = comment.find("*/") else {
                return Err(offset);
            };
            let whole = &rest[..2 + close + 2];
            out.extend(whole.chars().flat_map(|character| {
                let blank = if character == '\n' { '\n' } else { ' ' };
                std::iter::repeat_n(blank, character.len_utf8())
            }));
            whole.len()
        } else if rest.starts_with('"') {
            // To the closing quote, a backslash taking the character after it; or to the end.
            let mut end = rest.len();
            let mut characters = rest.char_indices().skip(1);
            while let Some((index, character)) = characters.next() {
                match character {
                    '\\' => {
                        characters.next();
                    }
                    '"' => {
                        end = index + 1;
                        break;
                    }
                    _ => {}
                }
            }
            out.push_str(&rest[..end]);
            end
        } else {
            let character = rest.chars().next().expect("not empty");
            out.push(character);
            character.len_utf8()
        };
        rest = &rest[taken..];
    }
    Ok(out)
}

/// Every text of up to `length` characters over `alphabet`.
fn texts(alphabet: &[char], length: usize) -> Vec<String> {
    let mut all = vec![String::new()];
    let mut last = all.clone();
    for _ in 0..length {
        let mut next = Vec::with_capacity(last.len() * alphabet.len());
        for text in &last {
            for character in alphabet {
                let mut longer = text.clone();
                longer.push(*character);
                next.push(longer);
            }
        }
        all.extend(next.iter().cloned());
        last = next;
    }
    all
}

#[test]
fn every_short_text_is_stripped_as_the_second_implementation_strips_it() {
    // Every character the pass tells apart: the two marker characters, a quote, a backslash,
    // both line ends, a plain character and one of two bytes. 8^6 + ... + 1 = 299,593 texts.
    let alphabet = ['/', '*', '"', '\\', '\n', '\r', 'a', '\u{e9}'];
    let all = texts(&alphabet, 6);
    assert_eq!(all.len(), 299_593);
    let (mut stripped, mut open, mut untouched) = (0_u32, 0_u32, 0_u32);
    for text in &all {
        match (strip(text), reference(text)) {
            (Ok(ours), Ok(theirs)) => {
                assert_eq!(ours, theirs, "{text:?}");
                assert_eq!(ours.len(), text.len(), "{text:?}");
                if matches!(ours, Cow::Borrowed(_)) {
                    assert_eq!(ours, *text);
                    untouched += 1;
                } else {
                    assert_ne!(ours, *text, "a buffer is made only for a comment: {text:?}");
                    stripped += 1;
                }
            }
            (Err(at), Err(offset)) => {
                assert_eq!(at, location_of(text.as_bytes(), offset), "{text:?}");
                assert!(text[offset..].starts_with("/*"), "{text:?}");
                open += 1;
            }
            (ours, theirs) => panic!("{text:?}: {ours:?} against {theirs:?}"),
        }
    }
    // All three outcomes are well represented.
    assert!(
        stripped > 10_000 && open > 10_000 && untouched > 200_000,
        "{stripped} {open} {untouched}"
    );
}

#[test]
fn a_stripped_text_keeps_every_other_byte_and_every_line_feed_in_place() {
    let text = "{ // one \u{e9}\n \"a//b\" /* two\n\u{1f321} */ : 1 } // end";
    let stripped = strip(text).expect("closed");
    // Byte for byte: `// one é` is nine bytes, `/* two` six, the thermometer and ` */` seven,
    // and `// end` six.
    let expected = format!(
        "{{ {}\n \"a//b\" {}\n{} : 1 }} {}",
        " ".repeat(9),
        " ".repeat(6),
        " ".repeat(7),
        " ".repeat(6)
    );
    assert_eq!(stripped, expected);
    assert_eq!(stripped.len(), text.len());
    for (ours, theirs) in stripped.bytes().zip(text.bytes()) {
        assert!(
            ours == theirs || (ours == b' ' && theirs != b'\n'),
            "{ours} for {theirs}"
        );
        assert_eq!(ours == b'\n', theirs == b'\n');
    }
    // What is left is the JSON, at the same places.
    assert_eq!(stripped.find(':'), text.find(" : ").map(|at| at + 1));
}

#[test]
fn a_block_comment_closes_at_the_first_star_slash_after_its_opening() {
    for (text, expected) in [
        ("/**/", Ok("    ")),
        ("/***/", Ok("     ")),
        ("/*/*/", Ok("     ")),
        ("/* /* */ */", Ok("         */")),
        ("/*\n*/", Ok("  \n  ")),
        ("/*\r\n*/", Ok("   \n  ")),
        ("a/**/b", Ok("a    b")),
        ("/*/", Err((1, 1))),
        ("/*", Err((1, 1))),
        ("/* *", Err((1, 1))),
        ("/**", Err((1, 1))),
        ("\n\n  /*", Err((3, 3))),
        ("\u{e9}/*", Err((1, 3))),
        ("/**/ /*/", Err((1, 6))),
    ] {
        let expected = expected
            .map(Cow::Borrowed)
            .map_err(|(line, column)| Location::new(line, column));
        assert_eq!(strip(text), expected, "{text:?}");
    }
}

#[test]
fn a_line_comment_ends_before_its_line_end_or_at_the_end_of_the_text() {
    for (text, expected) in [
        ("//", "  "),
        ("// a", "    "),
        ("// a\nb", "    \nb"),
        ("// a\rb", "    \rb"),
        ("// a\r\nb", "    \r\nb"),
        ("a // b // c\nd", "a          \nd"),
        ("///", "   "),
        ("// /* open\n1", "          \n1"),
        ("//\n//\n", "  \n  \n"),
    ] {
        assert_eq!(strip(text).expect("no block comment"), expected, "{text:?}");
    }
}

#[test]
fn a_string_is_never_a_comment_and_an_escape_takes_one_byte_with_it() {
    for text in [
        "\"//\"",
        "\"/* */\"",
        "\"a\\\"//b\"",
        "\"\\\\\"",
        "\"\\\\\\\"/*\"",
        "\"never closed // and /* on",
        "\"\\",
        "\"line\nfeed // inside\"",
        "\"\u{e9}//\u{1f321}/*\"",
        "/",
        "a/b",
        "1 / 2 * 3",
        "*/",
        "",
    ] {
        let stripped = strip(text).expect("no comment");
        assert!(matches!(stripped, Cow::Borrowed(_)), "{text:?}");
        assert_eq!(stripped, text);
    }
    // After a string closes, a comment is a comment again.
    assert_eq!(strip("\"a\"//b").expect("closed"), "\"a\"   ");
    assert_eq!(strip("\"\\\\\"//b").expect("closed"), "\"\\\\\"   ");
    assert_eq!(strip("\"\\\"\"/**/").expect("closed"), "\"\\\"\"    ");
    // A quote inside a comment opens no string.
    assert_eq!(
        strip("/* \" */ // \"\n1").expect("closed"),
        format!("{}\n1", " ".repeat(12))
    );
}

#[test]
fn the_pass_allocates_one_buffer_of_the_texts_length_or_nothing() {
    let measure = |text: &str| {
        let mut ok = false;
        let info = allocation_counter::measure(|| ok = strip(text).is_ok());
        (info.count_total, info.bytes_total, ok)
    };
    // No comment, whatever else: nothing.
    for text in [
        "",
        "{}",
        "\"// /* \"",
        &"[".repeat(100_000),
        &"\"".repeat(100_001),
    ] {
        assert_eq!(measure(text), (0, 0, true), "{} bytes", text.len());
    }
    // A comment, of any shape and number: one buffer, exactly as long as the text.
    for text in [
        "//".to_owned(),
        "/**/".to_owned(),
        "//\n".repeat(100_000),
        "/**/".repeat(100_000),
        "/*/*/".repeat(100_000),
        format!("/*{}*/", "*".repeat(400_000)),
        format!("{}//", " ".repeat(400_000)),
        format!("//{}", "/*".repeat(200_000)),
    ] {
        let length = u64::try_from(text.len()).expect("fits");
        assert_eq!(measure(&text), (1, length, true), "{length} bytes");
    }
    // An open comment: the same one buffer, and the refusal itself allocates nothing.
    for text in [
        "/*".to_owned(),
        format!("/*{}", "*".repeat(400_000)),
        format!("/*{}", "/".repeat(400_000)),
    ] {
        let length = u64::try_from(text.len()).expect("fits");
        assert_eq!(measure(&text), (1, length, false), "{length} bytes");
    }
}
