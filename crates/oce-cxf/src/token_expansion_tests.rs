//! Unit tests for reading one token against the prefix table (`token_expansion.rs`).
//!
//! The oracle throughout is the joined string: an [`Iri`] must answer every question as the
//! string its two pieces spell would, and [`expand_curie`] must classify a token as the joined
//! predicate did before a token was read in place.

use super::*;

/// Texts whose every split is read: ASCII, a multi-byte character on each side of the excerpt
/// limit, whitespace, and the empty text.
fn texts() -> Vec<String> {
    vec![
        String::new(),
        "a:b".to_owned(),
        "http://data.ashrae.org/S231#hasUnit".to_owned(),
        "http://qudt.org/schema/qudt#hasUnit".to_owned(),
        "urn:x y".to_owned(),
        "urn:é\u{2028}ü#hasUnit".to_owned(),
        format!("urn:{}", "k".repeat(EXCERPT_BYTES - 4)),
        format!("urn:{}", "k".repeat(EXCERPT_BYTES - 3)),
        format!("urn:{}é", "k".repeat(EXCERPT_BYTES - 5)),
        format!("urn:{}é{}", "k".repeat(EXCERPT_BYTES - 6), "z".repeat(40)),
    ]
}

/// Every way of holding `text` as two pieces, the two one-piece ways included.
fn splits(text: &str) -> Vec<Iri<'_>> {
    (0..=text.len())
        .filter(|at| text.is_char_boundary(*at))
        .map(|at| Iri {
            head: &text[..at],
            tail: &text[at..],
        })
        .collect()
}

/// Every prefix and suffix of `text` on a character boundary, and each with one character
/// added at either end, so a near miss is asked as well as a hit.
fn pieces(text: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    for at in (0..=text.len()).filter(|at| text.is_char_boundary(*at)) {
        for piece in [&text[..at], &text[at..]] {
            pieces.push(piece.to_owned());
            pieces.push(format!("{piece}~"));
            pieces.push(format!("~{piece}"));
        }
    }
    pieces
}

#[test]
fn an_iri_in_two_pieces_answers_as_the_joined_string_does_at_every_split() {
    for text in texts() {
        for iri in splits(&text) {
            let at = iri.head.len();
            assert_eq!(iri.len(), text.len(), "{text:?} at {at}");
            assert_eq!(iri.is_empty(), text.is_empty(), "{text:?} at {at}");
            assert_eq!(iri.joined(), text, "{text:?} at {at}");
            assert_eq!(iri.joined().capacity(), text.len(), "{text:?} at {at}");
            assert_eq!(iri.to_string(), text, "{text:?} at {at}");
            assert_eq!(
                iri.contains_whitespace(),
                text.contains(char::is_whitespace),
                "{text:?} at {at}"
            );
            assert_eq!(iri.excerpt(), excerpt(&text), "{text:?} at {at}");
            for piece in pieces(&text) {
                assert_eq!(
                    iri.is(&piece),
                    text == piece,
                    "{text:?} at {at} is {piece:?}"
                );
                assert_eq!(
                    iri.ends_with(&piece),
                    text.ends_with(&piece),
                    "{text:?} at {at} ends with {piece:?}"
                );
                assert_eq!(
                    iri.strip_prefix(&piece).map(Iri::joined),
                    text.strip_prefix(&piece).map(str::to_owned),
                    "{text:?} at {at} without {piece:?}"
                );
            }
        }
    }
}

#[test]
fn an_iri_is_one_piece_of_document_text_only_when_one_of_its_pieces_is_empty() {
    assert_eq!(Iri::whole("urn:a").as_whole(), Some("urn:a"));
    let (head, tail) = ("urn:", "a");
    assert_eq!(Iri { head, tail }.as_whole(), None);
    assert_eq!(Iri { head, tail: "" }.as_whole(), Some("urn:"));
    assert_eq!(Iri { head: "", tail }.as_whole(), Some("a"));
    // What is left of a two-piece IRI once its head is stripped is one piece again.
    let rest = Iri { head, tail }.strip_prefix("urn:").expect("a prefix");
    assert_eq!(rest.as_whole(), Some("a"));
}

#[test]
fn an_excerpt_is_cut_at_the_limit_whichever_piece_the_limit_falls_in() {
    let long = "k".repeat(3 * EXCERPT_BYTES);
    let cut = format!("{}…", "k".repeat(EXCERPT_BYTES));
    for at in [
        0,
        1,
        EXCERPT_BYTES - 1,
        EXCERPT_BYTES,
        EXCERPT_BYTES + 1,
        long.len(),
    ] {
        let iri = Iri {
            head: &long[..at],
            tail: &long[at..],
        };
        assert_eq!(iri.excerpt(), cut, "split at {at}");
    }
}

fn table(entries: &[(&str, &str)]) -> PrefixTable {
    entries
        .iter()
        .map(|(term, iri)| ((*term).to_owned(), (*iri).to_owned()))
        .collect()
}

/// What a token read through its prefix is, by the rule as it was written before a token was
/// read in place: the bound IRI and the suffix joined, and that string held to
/// [`is_absolute_iri`].
fn joined_reading(token: &str, table: &PrefixTable) -> Option<String> {
    let (prefix, suffix) = token.split_once(':')?;
    let joined = format!("{}{suffix}", table.get(prefix)?);
    is_absolute_iri(&joined).then_some(joined)
}

#[test]
fn a_token_through_a_declared_prefix_expands_exactly_when_the_joined_iri_is_absolute() {
    let table = table(&[
        ("ex", "http://example.org#"),
        ("u", "urn:x"),
        ("2024", "urn:year:"),
        ("a.b", "a:b"),
    ]);
    let suffixes = [
        "",
        "x",
        "x y",
        " x",
        "x ",
        "x\ty",
        "x\u{a0}y",
        "x\u{2028}",
        "a:b",
        ":",
        "é",
        "x/y#z",
    ];
    for prefix in ["ex", "u", "2024", "a.b"] {
        for suffix in suffixes {
            let token = format!("{prefix}:{suffix}");
            let expected = joined_reading(&token, &table);
            match expand_curie(&token, &table) {
                Expansion::Expanded(iri) => {
                    assert_eq!(Some(iri.joined()), expected, "{token:?}");
                }
                Expansion::Relative => assert_eq!(expected, None, "{token:?}"),
                Expansion::Verbatim => panic!("{token:?} is read through a declared prefix"),
            }
        }
    }
}

#[test]
fn a_whole_token_term_match_is_the_bound_iri_in_one_piece() {
    let table = table(&[("degC", "http://qudt.org/vocab/unit/DEG_C"), ("p:q", "p:q")]);
    for (token, iri) in [("degC", "http://qudt.org/vocab/unit/DEG_C"), ("p:q", "p:q")] {
        let Expansion::Expanded(expanded) = expand_token(token, &table) else {
            panic!("{token} is a declared term");
        };
        assert_eq!(expanded.as_whole(), Some(iri));
    }
    // The compact reading of a term never matches the term itself.
    assert!(matches!(expand_curie("degC", &table), Expansion::Relative));
}

#[test]
fn a_token_kept_as_written_and_a_token_with_no_absolute_form_expand_to_nothing() {
    let table = table(&[("http", "urn:not-a-scheme:"), ("ex", "http://example.org#")]);
    for token in ["http://example.org#x", "urn:x", "mailto:a@b.example"] {
        assert!(
            matches!(expand_token(token, &table), Expansion::Verbatim),
            "{token}"
        );
    }
    for token in [
        "x", "", ":x", "2024:x", "2024://x", "ab:", "ab:c d", "ex:c d", "ab:// x",
    ] {
        assert!(
            matches!(expand_token(token, &table), Expansion::Relative),
            "{token}"
        );
    }
}
