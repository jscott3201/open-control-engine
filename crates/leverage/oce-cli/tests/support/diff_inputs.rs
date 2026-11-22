//! The inputs of the `oce diff` tests: the diff note's worked examples as `oce-diff` keeps them
//! (`../oce-diff/tests/fixtures/examples/`, read in place, with that crate's goldens beside them),
//! this crate's own small pair under `tests/fixtures/inputs/diff/`, and each example's two sides
//! written into a scratch directory as CXF (emitted by `oce_author::emit` with the default
//! options, as `oce-diff`'s CXF goldens were) and in the wire form. No CXF is checked in here, so
//! no fixture joins the import corpus.

use std::path::{Path, PathBuf};

use oce_author::{EmitOptions, emit, parse_text, to_wire_string};

use super::{Run, crate_dir, scratch};

/// The diff note's worked examples, by `oce-diff`'s fixture name: (a) the site retune, its
/// literal variant, (b) the pressure request and (c) the plant reset.
pub const EXAMPLES: [&str; 4] = [
    "site_retune",
    "literal_retune",
    "pressure_request",
    "plant_reset",
];

/// The worked examples and `oce-diff`'s pair whose diff holds every kind of every section.
pub const PAIRS: [&str; 5] = [
    "site_retune",
    "literal_retune",
    "pressure_request",
    "plant_reset",
    "every_kind",
];

/// `oce-diff`'s examples directory, relative to the crate directory.
const EXAMPLE_DIR: &str = "../oce-diff/tests/fixtures/examples";

/// One side (`base` or `head`) of an `oce-diff` example in the text form, relative to the crate
/// directory.
pub fn example(name: &str, side: &str) -> String {
    format!("{EXAMPLE_DIR}/{name}.{side}.oce")
}

/// One of `oce-diff`'s goldens of an example (`diff.json`, `cxf.diff.md`, …), as text.
pub fn golden(name: &str, suffix: &str) -> String {
    let path = crate_dir().join(format!("{EXAMPLE_DIR}/{name}.{suffix}"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// This crate's own diff input, relative to the crate directory.
pub fn own(name: &str) -> String {
    format!("tests/fixtures/inputs/diff/{name}")
}

/// The text of a file named relative to the crate directory.
pub fn text_of(relative: &str) -> String {
    std::fs::read_to_string(crate_dir().join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

/// The CXF the emitter writes for an authoring text, with the default options.
pub fn emitted(text: &str) -> Vec<u8> {
    let sequence = parse_text(text).unwrap_or_else(|error| panic!("the text parses: {error:?}"));
    emit(&sequence, &EmitOptions::default()).unwrap_or_else(|error| panic!("it emits: {error}"))
}

/// The canonical wire form of an authoring text.
pub fn wire(text: &str) -> String {
    to_wire_string(&parse_text(text).unwrap_or_else(|error| panic!("parses: {error:?}")))
}

/// A scratch directory holding, for every worked example, `<name>.<side>.jsonld` (its CXF) and
/// `<name>.<side>.json` (its wire form), for both sides.
pub fn example_forms(test: &str) -> PathBuf {
    let dir = scratch(test);
    for name in EXAMPLES {
        for side in ["base", "head"] {
            let text = text_of(&example(name, side));
            write(&dir, &format!("{name}.{side}.jsonld"), &emitted(&text));
            write(&dir, &format!("{name}.{side}.json"), wire(&text).as_bytes());
        }
    }
    dir
}

/// Write `bytes` to `dir/name`.
pub fn write(dir: &Path, name: &str, bytes: &[u8]) {
    std::fs::write(dir.join(name), bytes).unwrap_or_else(|error| panic!("write {name}: {error}"));
}

/// The diff a report carries, written as `oce_diff::Diff::to_json` writes a diff: keys sorted,
/// two-space indentation, one final line feed. The envelope is everything else.
pub fn carried_diff(run: &Run) -> String {
    let report = run.report();
    assert!(report["diff"].is_object(), "no diff\n{}", run.transcript());
    let mut text = serde_json::to_string_pretty(&report["diff"]).expect("a JSON value");
    text.push('\n');
    text
}
