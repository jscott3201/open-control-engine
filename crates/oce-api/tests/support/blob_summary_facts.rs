//! A blob summary as plain comparable facts, for the blob-summary tests (GitHub #448).
//!
//! `oce_api::BlobSummary` is `#[non_exhaustive]`, so a test cannot build the value it expects.
//! [`Facts`] holds the same fields as plain data and is built three ways: from the facade's
//! summary, from the core's borrowed summary, and from a test-side [`Image`], which is what a
//! blob was written from and so is the oracle for what a summary of it must say.
//!
//! [`Facts::to_json`] is the one spelling the summary goldens are checked in as
//! (`tests/fixtures/blob_summary/`). It is test evidence, not a format the engine publishes.

#![allow(dead_code)]

use std::fmt::Write as _;

use oce_api::{BlobSummary, OcError, inspect_blob};
use oce_graph::blob::{BlobError, ENGINE_VERSION, Summary, inspect};

use super::blob_image::Image;

/// The text a canonical content id starts with, restated here so the goldens do not take it from
/// the code under test.
const CONTENT_ID_TAG: &str = "oce:canon1:";

/// One class table entry: the class path and the recorded signature hash.
pub(crate) type ClassFact = (String, u64);

/// One instance: block id, class index, stateful, state word count and initial-state CRC.
pub(crate) type InstanceFact = (u32, u32, bool, u32, u32);

/// Every field of a blob summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Facts {
    pub(crate) format_version: u16,
    /// Major, minor and patch.
    pub(crate) engine_version: [u16; 3],
    pub(crate) content_id: Option<[u8; 32]>,
    pub(crate) target_bound: bool,
    pub(crate) classes: Vec<ClassFact>,
    pub(crate) instances: Vec<InstanceFact>,
    pub(crate) inputs: Vec<String>,
    pub(crate) outputs: Vec<String>,
}

impl Facts {
    /// The facts of the facade's owned summary.
    pub(crate) fn of_summary(summary: &BlobSummary) -> Self {
        Self {
            format_version: summary.format_version,
            engine_version: [
                summary.engine_version.major,
                summary.engine_version.minor,
                summary.engine_version.patch,
            ],
            content_id: summary.content_id.as_ref().map(|id| {
                // The id's text is the tag and the hex of the digest it holds.
                assert_eq!(id.as_str(), format!("{CONTENT_ID_TAG}{}", hex(id.digest())));
                *id.digest()
            }),
            target_bound: summary.target_bound,
            classes: summary
                .classes
                .iter()
                .map(|class| (class.class_path.clone(), class.signature_hash))
                .collect(),
            instances: summary
                .instances
                .iter()
                .map(|i| {
                    (
                        i.block,
                        i.class,
                        i.stateful,
                        i.state_words,
                        i.initial_state_crc,
                    )
                })
                .collect(),
            inputs: summary.inputs.clone(),
            outputs: summary.outputs.clone(),
        }
    }

    /// The facts of the core's borrowed summary.
    pub(crate) fn of_core(summary: &Summary<'_>) -> Self {
        Self {
            format_version: summary.format_version,
            engine_version: [
                summary.engine_version.major,
                summary.engine_version.minor,
                summary.engine_version.patch,
            ],
            content_id: summary.content_id,
            target_bound: summary.target_bound,
            classes: summary
                .classes
                .iter()
                .map(|class| (class.class_path.to_owned(), class.signature_hash))
                .collect(),
            instances: summary
                .instances
                .iter()
                .map(|i| {
                    (
                        i.block,
                        i.class,
                        i.stateful,
                        i.state_words,
                        i.initial_state_crc,
                    )
                })
                .collect(),
            inputs: summary.inputs.iter().map(|&path| path.to_owned()).collect(),
            outputs: summary
                .outputs
                .iter()
                .map(|&path| path.to_owned())
                .collect(),
        }
    }

    /// What a summary of `image.to_bytes()` must say, read from the lists the bytes are written
    /// from. `target_bound` comes from the catalog's per-class flag, not from the function the
    /// reader calls. Panics on a path that is not UTF-8 or a kind byte that is not 0 or 1: the
    /// reader refuses those, so no summary exists to compare.
    pub(crate) fn of_image(image: &Image) -> Self {
        let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).expect("a UTF-8 path");
        let classes: Vec<ClassFact> = image
            .classes
            .iter()
            .map(|(path, hash)| (text(path), *hash))
            .collect();
        let target_bound = classes.iter().any(|(path, _)| {
            oce_blocks::catalog()
                .iter()
                .any(|entry| entry.class_path == path && entry.target_bound)
        });
        Self {
            format_version: image.format,
            engine_version: image.version,
            content_id: (image.content_id != [0; 32]).then_some(image.content_id),
            target_bound,
            classes,
            instances: image
                .instances
                .iter()
                .enumerate()
                .map(|(block, inst)| {
                    assert!(inst.kind <= 1, "a kind tag the reader refuses");
                    (
                        u32::try_from(block).unwrap(),
                        inst.class,
                        inst.kind == 1,
                        inst.state_len,
                        inst.state_crc,
                    )
                })
                .collect(),
            inputs: image.inputs.iter().map(|i| text(&i.path)).collect(),
            outputs: image.outputs.iter().map(|o| text(&o.path)).collect(),
        }
    }

    /// The golden spelling: one JSON object, keys in this fixed order, one list entry per line,
    /// two-space indentation, LF line ends and a final newline. Hashes and CRCs are lowercase hex
    /// strings, so no reader has to carry a 64-bit integer through a float, and the content id is
    /// its text form, `oce:canon1:` and 64 hex digits.
    pub(crate) fn to_json(&self) -> String {
        let mut out = String::from("{\n");
        writeln!(out, "  \"format_version\": {},", self.format_version).unwrap();
        let [major, minor, patch] = self.engine_version;
        writeln!(out, "  \"engine_version\": \"{major}.{minor}.{patch}\",").unwrap();
        match self.content_id {
            Some(id) => {
                writeln!(out, "  \"content_id\": \"{CONTENT_ID_TAG}{}\",", hex(&id)).unwrap();
            }
            None => out.push_str("  \"content_id\": null,\n"),
        }
        writeln!(out, "  \"target_bound\": {},", self.target_bound).unwrap();
        list(
            &mut out,
            "classes",
            self.classes.iter().map(|(path, hash)| {
                format!(
                    "{{\"class_path\": {}, \"signature_hash\": \"{hash:016x}\"}}",
                    quoted(path)
                )
            }),
            true,
        );
        list(
            &mut out,
            "instances",
            self.instances
                .iter()
                .map(|(block, class, stateful, words, crc)| {
                    format!(
                        "{{\"block\": {block}, \"class\": {class}, \"stateful\": {stateful}, \
                         \"state_words\": {words}, \"initial_state_crc\": \"{crc:08x}\"}}"
                    )
                }),
            true,
        );
        list(
            &mut out,
            "inputs",
            self.inputs.iter().map(|path| quoted(path)),
            true,
        );
        list(
            &mut out,
            "outputs",
            self.outputs.iter().map(|path| quoted(path)),
            false,
        );
        out.push_str("}\n");
        out
    }

    /// Read the golden spelling back with `serde_json`, an independent reader: the inverse of
    /// [`Self::to_json`] for every value it writes. Panics on any other text.
    pub(crate) fn from_json(text: &str) -> Self {
        let value: serde_json::Value = serde_json::from_str(text).expect("valid JSON");
        let object = value.as_object().expect("a JSON object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        let expected = [
            "classes",
            "content_id",
            "engine_version",
            "format_version",
            "inputs",
            "instances",
            "outputs",
            "target_bound",
        ];
        assert_eq!(keys, expected, "the summary's keys");
        let number = |value: &serde_json::Value| value.as_u64().expect("an unsigned integer");
        let string = |value: &serde_json::Value| value.as_str().expect("a string").to_owned();
        let strings = |key: &str| -> Vec<String> {
            object[key]
                .as_array()
                .expect("an array")
                .iter()
                .map(string)
                .collect()
        };
        let version: Vec<u16> = string(&object["engine_version"])
            .split('.')
            .map(|part| part.parse().expect("a version component"))
            .collect();
        Self {
            format_version: u16::try_from(number(&object["format_version"])).unwrap(),
            engine_version: version.try_into().expect("major.minor.patch"),
            content_id: match &object["content_id"] {
                serde_json::Value::Null => None,
                id => {
                    let text = string(id);
                    let digits = text.strip_prefix(CONTENT_ID_TAG).expect("the id tag");
                    Some(unhex(digits).try_into().expect("32 bytes"))
                }
            },
            target_bound: object["target_bound"].as_bool().expect("a boolean"),
            classes: object["classes"]
                .as_array()
                .expect("an array")
                .iter()
                .map(|class| {
                    (
                        string(&class["class_path"]),
                        u64::from_str_radix(&string(&class["signature_hash"]), 16).unwrap(),
                    )
                })
                .collect(),
            instances: object["instances"]
                .as_array()
                .expect("an array")
                .iter()
                .map(|i| {
                    (
                        u32::try_from(number(&i["block"])).unwrap(),
                        u32::try_from(number(&i["class"])).unwrap(),
                        i["stateful"].as_bool().expect("a boolean"),
                        u32::try_from(number(&i["state_words"])).unwrap(),
                        u32::from_str_radix(&string(&i["initial_state_crc"]), 16).unwrap(),
                    )
                })
                .collect(),
            inputs: strings("inputs"),
            outputs: strings("outputs"),
        }
    }
}

/// Summarise `bytes` through the core reader and through the facade, require the two to agree
/// (the same facts, or the same refusal wrapped as `OcError::Blob`), and return what they said.
pub(crate) fn summarise(bytes: &[u8]) -> Result<Facts, BlobError> {
    let core = inspect(bytes).map(|summary| Facts::of_core(&summary));
    let facade = match inspect_blob(bytes) {
        Ok(summary) => Ok(Facts::of_summary(&summary)),
        Err(OcError::Blob(error)) => Err(error),
        Err(other) => panic!("a refusal that is not OcError::Blob: {other:?}"),
    };
    assert_eq!(core, facade, "the facade's summary is not the core's");
    core
}

/// The invariants documented on `BlobSummary`, for any summary the reader returns.
pub(crate) fn assert_invariants(facts: &Facts, what: &str) {
    assert_eq!(facts.format_version, 1, "{what}: format version");
    assert_eq!(
        facts.engine_version,
        [
            ENGINE_VERSION.major,
            ENGINE_VERSION.minor,
            ENGINE_VERSION.patch
        ],
        "{what}: engine version"
    );
    assert_ne!(facts.content_id, Some([0; 32]), "{what}: a zero id is None");
    for pair in facts.classes.windows(2) {
        assert!(
            pair[0].0.as_bytes() < pair[1].0.as_bytes(),
            "{what}: classes strictly increasing by path"
        );
    }
    for (path, _) in &facts.classes {
        assert!(
            oce_blocks::lookup(path).is_some(),
            "{what}: class {path:?} is registered"
        );
    }
    for (index, (block, class, ..)) in facts.instances.iter().enumerate() {
        assert_eq!(*block as usize, index, "{what}: block id is the position");
        assert!(
            (*class as usize) < facts.classes.len(),
            "{what}: block {block} class index {class} is in the table"
        );
    }
    for pair in facts.inputs.windows(2) {
        assert!(
            pair[0].as_bytes() < pair[1].as_bytes(),
            "{what}: inputs strictly increasing by path"
        );
    }
    for pair in facts.outputs.windows(2) {
        assert!(
            pair[0].as_bytes() <= pair[1].as_bytes(),
            "{what}: outputs non-decreasing by path"
        );
    }
}

/// Write `"key": [` … `]`, one entry per line, or `"key": []` for an empty list.
fn list(out: &mut String, key: &str, entries: impl Iterator<Item = String>, comma: bool) {
    let entries: Vec<String> = entries.collect();
    let end = if comma { ",\n" } else { "\n" };
    if entries.is_empty() {
        write!(out, "  \"{key}\": []{end}").unwrap();
        return;
    }
    writeln!(out, "  \"{key}\": [").unwrap();
    for (index, entry) in entries.iter().enumerate() {
        let separator = if index + 1 == entries.len() { "" } else { "," };
        writeln!(out, "    {entry}{separator}").unwrap();
    }
    write!(out, "  ]{end}").unwrap();
}

/// A JSON string literal: `"` and `\` escaped, control characters as `\u00XX`, all else verbatim.
fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            control if u32::from(control) < 0x20 => {
                write!(out, "\\u{:04x}", u32::from(control)).unwrap();
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        write!(out, "{byte:02x}").unwrap();
        out
    })
}

fn unhex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "an even number of hex digits");
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).expect("a hex byte"))
        .collect()
}
