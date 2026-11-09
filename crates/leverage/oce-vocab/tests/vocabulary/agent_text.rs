//! Every string in both data files is plain text an agent can be shown: no absolute path, no
//! UUID-shaped string, no `modelica://` link, no HTML tag; no line break or padding; and each
//! fits an editorial bound (design note test plan, "Agent-facing text", ruled to run over every
//! string in both files).

use oce_vocab::{CURATED_JSON, GENERATED_JSON, vocabulary};
use serde_json::Value;

/// The longest description, generated or curated, in bytes.
const MAX_DESCRIPTION_BYTES: usize = 200;
/// The longest note text, in bytes.
const MAX_NOTE_BYTES: usize = 400;
/// The longest evidence string, in bytes.
const MAX_EVIDENCE_BYTES: usize = 300;
/// The longest of every other string (a name, a unit, a class path, a pin), in bytes.
const MAX_OTHER_BYTES: usize = 100;

/// The HTML the upstream `Documentation` strings use: the closed list of the design note. A
/// comparison sign in a description (`u<uLow`, `timeScale <> 1`) is not on it.
const HTML_MARKERS: &[&str] = &["<html", "<p>", "<code>", "<a ", "<br", "<img", "<i>", "</"];

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

/// Whether `text` contains an 8-4-4-4-12 hexadecimal group, the shape the no-secret scan
/// refuses.
fn has_uuid_shape(text: &str) -> bool {
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];
    let bytes = text.as_bytes();
    (0..bytes.len()).any(|start| {
        let mut at = start;
        for (index, len) in GROUPS.iter().enumerate() {
            if index > 0 {
                if bytes.get(at) != Some(&b'-') {
                    return false;
                }
                at += 1;
            }
            if bytes.len() < at + len || !bytes[at..at + len].iter().all(|b| is_hex(*b)) {
                return false;
            }
            at += len;
        }
        true
    })
}

/// Whether `text` holds an absolute path: a Unix root or home, a file URL or a Windows drive.
fn has_absolute_path(text: &str) -> bool {
    let unix = text.split_whitespace().any(|word| {
        let word = word.trim_start_matches(['(', '"', '\'', '`', '[']);
        word.starts_with('/') && word.len() > 1 && word.as_bytes()[1].is_ascii_alphabetic()
    });
    let bytes = text.as_bytes();
    let drive = bytes
        .windows(3)
        .any(|window| window[0].is_ascii_alphabetic() && window[1] == b':' && window[2] == b'\\');
    unix || drive
        || [
            "/Users/",
            "/home/",
            "/var/",
            "/tmp/",
            "/private/",
            "file://",
            "~/",
        ]
        .iter()
        .any(|marker| text.contains(marker))
}

/// Why a string may not be shown to an agent, if it may not.
fn defect(text: &str) -> Option<&'static str> {
    if has_absolute_path(text) {
        Some("an absolute path")
    } else if has_uuid_shape(text) {
        Some("a UUID-shaped string")
    } else if text.contains("modelica://") {
        Some("a modelica:// link")
    } else if HTML_MARKERS.iter().any(|marker| text.contains(marker)) {
        Some("an HTML tag")
    } else if text.chars().any(char::is_control) {
        Some("a control character")
    } else if text.trim() != text {
        Some("surrounding white space")
    } else if !text.is_ascii() {
        Some("a non-ASCII character")
    } else {
        None
    }
}

#[test]
fn the_scan_refuses_each_defect_and_accepts_comparison_signs() {
    // The home-directory and UUID samples are assembled here so this file does not itself hold
    // the shapes the repository's no-secret scan refuses.
    let home_path = format!("see /{}/someone/model.mo", "Users");
    let runner_path = format!("/{}/runner/work", "home");
    let uuid = ["123e4567", "e89b", "12d3", "a456", "426614174000"].join("-");
    let uuid_in_text = format!("id {uuid} here");
    for (text, expected) in [
        (home_path.as_str(), "an absolute path"),
        (runner_path.as_str(), "an absolute path"),
        ("output of (/var/folders/x)", "an absolute path"),
        ("file:///etc/passwd", "an absolute path"),
        ("C:\\Users\\x", "an absolute path"),
        ("~/Development/x", "an absolute path"),
        ("/tmp/out.jsonld", "an absolute path"),
        (uuid_in_text.as_str(), "a UUID-shaped string"),
        ("modelica://Buildings/Resources/x.png", "a modelica:// link"),
        ("<html><p>text</p></html>", "an HTML tag"),
        ("text <code>y</code>", "an HTML tag"),
        ("see <a href=\"x\">", "an HTML tag"),
        ("line<br/>break", "an HTML tag"),
        ("<img src=\"x\">", "an HTML tag"),
        ("vector <i>y</i>", "an HTML tag"),
        ("closing </b>", "an HTML tag"),
        ("two\nlines", "a control character"),
        ("tab\there", "a control character"),
        (" padded", "surrounding white space"),
        ("padded ", "surrounding white space"),
        ("20 \u{b0}C", "a non-ASCII character"),
    ] {
        assert_eq!(defect(text), Some(expected), "{text:?}");
    }
    for text in [
        "if y=true and u<uLow, switch to y=false",
        "Input for support point x1, with x1 < x2",
        "Table matrix (time = first column is time in seconds, unless timeScale <> 1)",
        "Input for the time constant (T>0 required; T=0 is ideal derivative block)",
        "1/s",
        "J/kg",
        "third_party/modelica-buildings-cdl/cxf-core/CXF-Core.jsonld",
        "crates/oce-blocks/src/discrete_tests.rs (a test)",
        "a131864e4c4df22ebcd52bb8da439de0087ac365",
        "y = u1 / u2: u1 is the dividend and u2 the divisor.",
        "",
    ] {
        assert_eq!(defect(text), None, "{text:?}");
    }
    // One hexadecimal digit short of the last group is not the shape.
    assert!(!has_uuid_shape(&uuid[..uuid.len() - 1]));
}

/// Every string of a JSON document, keys included, with the key it is a value of (`""` for a
/// key or an array item).
fn strings<'a>(value: &'a Value, key: &'a str, out: &mut Vec<(&'a str, &'a str)>) {
    match value {
        Value::String(text) => out.push((key, text)),
        Value::Array(items) => items.iter().for_each(|item| strings(item, key, out)),
        Value::Object(map) => {
            for (name, item) in map {
                out.push(("", name));
                strings(item, name, out);
            }
        }
        _ => {}
    }
}

fn bound(key: &str, in_note: bool) -> usize {
    match key {
        "description" => MAX_DESCRIPTION_BYTES,
        "text" if in_note => MAX_NOTE_BYTES,
        "text" => MAX_DESCRIPTION_BYTES,
        "evidence" => MAX_EVIDENCE_BYTES,
        _ => MAX_OTHER_BYTES,
    }
}

#[test]
fn no_string_in_either_file_holds_a_path_a_uuid_a_link_or_html_and_each_fits_its_bound() {
    let mut scanned = 0;
    for (name, text) in [("generated", GENERATED_JSON), ("curated", CURATED_JSON)] {
        let document: Value = serde_json::from_str(text).expect("JSON");
        let mut found = Vec::new();
        strings(&document, "", &mut found);
        for (key, string) in &found {
            assert_eq!(defect(string), None, "{name}: {key}: {string:?}");
            // A curated description's text is held to the description bound, a note's to the
            // note bound; the tighter one is checked here and the note bound below.
            assert!(
                string.len() <= bound(key, true),
                "{name}: {key} is {} bytes: {string:?}",
                string.len()
            );
        }
        scanned += found.len();
    }
    assert!(scanned > 4_000, "only {scanned} strings scanned");
    for class in &vocabulary().curated().classes {
        for description in &class.descriptions {
            assert!(
                description.text.len() <= bound("text", false),
                "{description:?}"
            );
        }
    }
}

#[test]
fn after_the_merge_every_description_note_and_evidence_says_something() {
    for class in vocabulary().classes() {
        assert!(!class.description.is_empty(), "{}", class.class_path);
        assert_eq!(defect(class.description), None, "{}", class.class_path);
        for connector in &class.connectors {
            assert!(!connector.description.is_empty() && !connector.name.is_empty());
        }
        for parameter in &class.parameters {
            assert!(!parameter.description.is_empty() && !parameter.name.is_empty());
        }
        for note in class.notes {
            assert!(!note.text.is_empty() && !note.evidence.is_empty());
            // A note is one or more full sentences; a description is a phrase.
            assert!(
                note.text.ends_with('.'),
                "{}: {}",
                class.class_path,
                note.text
            );
        }
    }
    for class in &vocabulary().curated().classes {
        for description in &class.descriptions {
            assert!(!description.evidence.is_empty());
            assert!(!description.text.ends_with('.'), "{}", description.text);
        }
    }
}
