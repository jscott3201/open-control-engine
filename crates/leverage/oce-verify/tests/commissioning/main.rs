//! The commissioning spec and point-mapping readers (design note `docs/design/commissioning.md`,
//! "Test-spec file" and "Point mapping"; issue #658), as one test binary.
//!
//! Each file of this directory is one topic and is compiled here as a module; the last test
//! below fails on a file that is not, so a test file cannot sit in the tree uncompiled. Select
//! one topic with `cargo nextest run -p oce-verify --test commissioning -E 'test(/^<file>::/)'`.

#[path = "../support/mod.rs"]
mod support;

mod canonical_form;
mod closed_objects;
mod comment_stripping;
mod error_codes;
mod hostile_bytes;
mod key_selection;
mod limit_boundaries;
mod listings;
mod load_cost;
mod point_mapping;
mod spec_fields;

use oce_verify::{CommissioningSpec, Location, PointMapping, SpecError, SpecLimits};

/// The text of a fixture under `tests/fixtures/commissioning/`.
fn fixture_text(name: &str) -> String {
    String::from_utf8(support::fixture_bytes(&format!("commissioning/{name}"))).expect("UTF-8")
}

/// Load `text` as a spec under the default limits.
fn load(text: &str) -> Result<CommissioningSpec, SpecError> {
    CommissioningSpec::from_json_str(text, &SpecLimits::default())
}

/// The spec `text` is, which must load.
fn loaded(text: &str) -> CommissioningSpec {
    load(text).unwrap_or_else(|error| panic!("the spec loads: {} {error}\n{text}", error.code()))
}

/// The code `text` is refused with as a spec under the default limits, or `loaded`.
fn code_of(text: &str) -> &'static str {
    match load(text) {
        Ok(_) => "loaded",
        Err(error) => error.code(),
    }
}

/// The code, line and column `text` is refused with as a spec; the refusal must sit in the text.
fn refusal(text: &str) -> (&'static str, u32, u32) {
    let error = load(text).expect_err("the spec is refused");
    let Location { line, column } = error
        .at()
        .unwrap_or_else(|| panic!("{} carries no place: {error}", error.code()));
    (error.code(), line, column)
}

/// Load `text` as a point-mapping file under the default limits.
fn load_mapping(text: &str) -> Result<PointMapping, SpecError> {
    PointMapping::from_json_str(text, &SpecLimits::default())
}

/// A spec of one reference: `members` are the reference's members after `model` and `sequence`
/// (sequence `s`), written as the inside of a JSON object, or empty.
fn one_reference(members: &str) -> String {
    let separator = if members.is_empty() { "" } else { ", " };
    format!("{{\"references\": [{{\"model\": \"M\", \"sequence\": \"s\"{separator}{members}}}]}}")
}

/// A spec with no reference and the given top-level `members`, written as the inside of a JSON
/// object, or empty.
fn top_level(members: &str) -> String {
    let separator = if members.is_empty() { "" } else { ", " };
    format!("{{\"references\": []{separator}{members}}}")
}

#[test]
fn every_test_file_of_this_directory_is_compiled_into_this_binary() {
    let directory = support::crate_dir().join("tests/commissioning");
    let root = std::fs::read_to_string(directory.join("main.rs")).expect("read main.rs");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&directory).expect("read the directory") {
        let name = entry.expect("an entry").file_name();
        let name = name.to_str().expect("a UTF-8 file name").to_owned();
        if let Some(module) = name.strip_suffix(".rs")
            && module != "main"
        {
            files.push(module.to_owned());
        } else {
            assert_eq!(name, "main.rs", "only Rust files live here");
        }
    }
    files.sort();
    assert!(files.len() >= 11, "{files:?}");
    let declared: Vec<String> = root
        .lines()
        .filter_map(|line| line.strip_prefix("mod ")?.strip_suffix(';'))
        // `support` is the crate's shared helper module, one directory up.
        .filter(|module| *module != "support")
        .map(str::to_owned)
        .collect();
    assert_eq!(
        declared, files,
        "each file is one `mod` line of main.rs, in name order, and each line a file"
    );
}
