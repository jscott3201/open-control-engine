//! The CXF importer refuses a term (`S231:unit`, `S231:quantity`, `S231:displayUnit`) carrying a
//! Unicode format (`Cf`) or control (`Cc`) character as `wrong-shape` naming the key, on every
//! role it reads one: a boundary's string, and a sequence or block parameter's string, typed
//! literal `@value` or IRI node `@id` (issue #629; owner ruling of 2026-11-12). The character
//! class is the engine loader's own, reached through the facade (`oce_api::is_format_or_control`),
//! and the last test holds the two readers to the same verdict on each character.
//!
//! Before, such a term imported: none of these characters is blank to `str::trim`.

use oce_api::{DiagCode, Engine, is_format_or_control};
use oce_author::{ImportError, ImportErrorKind, ImportOptions, SourceForm, import_cxf};
use serde_json::{Value as Json, json};

use crate::support::fixture;

const BOUNDARY: &str = "urn:oce:author:econ_enable.TOut";
const PARAMETER: &str = "urn:oce:author:econ_enable.dTHys";
const BLOCK_PARAMETER: &str = "urn:oce:author:econ_enable.hys.uLow";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const TERMS: [&str; 3] = ["S231:unit", "S231:quantity", "S231:displayUnit"];

/// The characters the ruling names: a zero-width space, a byte-order mark, U+2060, U+180E, a soft
/// hyphen, two bidirectional controls, an astral tag, NUL, U+0001, U+001F, DEL and a C1 control,
/// and the controls that are also whitespace (tab, line feed, U+000B, U+000C, carriage return,
/// U+0085).
const INVISIBLE: [char; 19] = [
    '\u{200B}',
    '\u{FEFF}',
    '\u{2060}',
    '\u{180E}',
    '\u{AD}',
    '\u{202E}',
    '\u{2066}',
    '\u{E0001}',
    '\u{0}',
    '\u{1}',
    '\u{1F}',
    '\u{7F}',
    '\u{9F}',
    '\t',
    '\n',
    '\u{B}',
    '\u{C}',
    '\r',
    '\u{85}',
];

/// Characters outside `Cf` and `Cc` that the rule leaves alone: a no-break space, a line
/// separator, a variation selector, a Hangul filler, a private-use character, `°` and `µ`.
const OUTSIDE: [char; 7] = [
    '\u{A0}', '\u{2028}', '\u{FE0F}', '\u{3164}', '\u{E000}', '°', 'µ',
];

fn golden() -> Json {
    let text = std::fs::read_to_string(fixture("emit/econ_enable.cxf.json")).expect("golden");
    serde_json::from_str(&text).expect("golden parses")
}

fn with(id: &str, key: &str, value: &Json) -> Json {
    let mut doc = golden();
    let node = doc["@graph"]
        .as_array_mut()
        .expect("@graph")
        .iter_mut()
        .find(|node| node["@id"] == id)
        .unwrap_or_else(|| panic!("no node `{id}`"));
    node[key] = value.clone();
    doc
}

fn import(doc: &Json) -> Result<(), ImportError> {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    import_cxf(&bytes, &ImportOptions::new()).map(|_| ())
}

/// The refusal's subject and kind, its rendering checked to carry no invisible character.
fn refusal(doc: &Json) -> (String, ImportErrorKind) {
    match import(doc) {
        Err(error) => {
            assert_eq!(error.source, SourceForm::Cxf);
            let shown = error.to_string();
            assert!(
                !shown
                    .chars()
                    .any(|c| is_format_or_control(c) || INVISIBLE.contains(&c)),
                "the refusal carries an invisible character as it is: {shown:?}"
            );
            (error.subject, error.kind)
        }
        Ok(()) => panic!("expected a refusal, but it imported"),
    }
}

fn wrong_shape(subject: &str, key: &str) -> (String, ImportErrorKind) {
    (
        subject.to_owned(),
        ImportErrorKind::WrongShape {
            key: key.to_owned(),
        },
    )
}

/// Each form a role reads a term in, wrapping `text`.
fn forms(node: &str, text: &str) -> Vec<Json> {
    let bare = json!(text);
    if node == BOUNDARY {
        return vec![bare];
    }
    vec![
        bare,
        json!({ "@value": text, "@type": XSD_STRING }),
        json!({ "@id": text }),
    ]
}

#[test]
fn an_invisible_character_is_refused_at_every_position_on_every_role_and_form() {
    import(&golden()).expect("the golden imports");
    for c in INVISIBLE {
        // At the start, in the middle and at the end of a visible term, and alone (a whitespace
        // control alone is blank, refused the same way).
        for text in [
            format!("{c}K"),
            format!("de{c}gC"),
            format!("K{c}"),
            c.to_string(),
        ] {
            for key in TERMS {
                for node in [BOUNDARY, PARAMETER, BLOCK_PARAMETER] {
                    for value in forms(node, &text) {
                        assert_eq!(
                            refusal(&with(node, key, &value)),
                            wrong_shape(node, key),
                            "{node} {key} {value}"
                        );
                    }
                }
            }
        }
    }
    // Padded with spaces, and a 1 MiB term.
    let long = format!("K\u{200B}{}", "x".repeat(1 << 20));
    for text in [" \u{200B} ", " K\u{FEFF} ", "\t\u{200B}", long.as_str()] {
        for key in TERMS {
            for node in [BOUNDARY, PARAMETER, BLOCK_PARAMETER] {
                assert_eq!(
                    refusal(&with(node, key, &json!(text))),
                    wrong_shape(node, key),
                    "{node} {key}"
                );
            }
        }
    }
}

#[test]
fn visible_terms_and_what_the_rule_does_not_name_still_import() {
    for c in OUTSIDE {
        let text = format!("K{c}");
        for key in TERMS {
            for node in [BOUNDARY, PARAMETER, BLOCK_PARAMETER] {
                for value in forms(node, &text) {
                    import(&with(node, key, &value))
                        .unwrap_or_else(|e| panic!("{node} {key} {value}: {e}"));
                }
            }
        }
    }
    // A parameter's bound is not a term: a string bound carrying one is not this rule's, as in the
    // engine.
    for node in [PARAMETER, BLOCK_PARAMETER] {
        for key in ["S231:min", "S231:max"] {
            import(&with(node, key, &json!("yMin\u{200B}")))
                .unwrap_or_else(|e| panic!("{node} {key}: {e}"));
        }
    }
}

/// Whether the engine's loader refuses `doc` for the term on `node`, read through the facade.
fn engine_refuses(doc: &Json, node: &str) -> bool {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    match Engine::new().load_cxf(&bytes) {
        Ok(_) => false,
        Err(error) => {
            let diags = error.diagnostics();
            assert!(
                diags
                    .iter()
                    .all(|d| d.code == DiagCode::MalformedDocument
                        && d.subject.as_deref() == Some(node)),
                "only the term's refusal: {diags:?}"
            );
            !diags.is_empty()
        }
    }
}

#[test]
fn the_importer_and_the_engine_refuse_the_same_characters() {
    // Both readers' verdict on each character, at the end of a visible term, on a boundary unit and
    // a parameter quantity: the importer refuses exactly where the engine does.
    let golden = golden();
    let mut engine_loads = Engine::new();
    engine_loads
        .load_cxf(&serde_json::to_vec(&golden).expect("serialize"))
        .expect("the engine loads the golden");
    for (c, invisible) in INVISIBLE
        .iter()
        .map(|&c| (c, true))
        .chain(OUTSIDE.iter().map(|&c| (c, false)))
    {
        let text = format!("K{c}");
        for (node, key) in [(BOUNDARY, "S231:unit"), (PARAMETER, "S231:quantity")] {
            let doc = with(node, key, &json!(text));
            assert_eq!(
                import(&doc).is_err(),
                invisible,
                "the importer on U+{:04X}",
                u32::from(c)
            );
            assert_eq!(
                engine_refuses(&doc, node),
                invisible,
                "the engine on U+{:04X}",
                u32::from(c)
            );
        }
    }
}
