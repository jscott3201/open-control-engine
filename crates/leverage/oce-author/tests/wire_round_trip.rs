//! Wire → model → wire byte equality: the reference bundles, every checked-in bundle and the
//! authoring note's worked example (issue #463 acceptance).

mod support;

use oce_author::{from_wire_str, to_wire_string, validate};

use support::{compare_or_bless, econ_enable, every_element, fixture};

/// The JSON block of the note's "Wire form" section, verbatim.
fn note_wire_example() -> String {
    let note = std::fs::read_to_string(
        support::crate_dir().join("../../../docs/design/authoring-layer.md"),
    )
    .expect("read the authoring note");
    let section = note
        .split("## Wire form")
        .nth(1)
        .expect("the note has a Wire form section");
    let start = section.find("```json\n").expect("a json block") + "```json\n".len();
    let end = section[start..].find("```").expect("the block closes") + start;
    section[start..end].to_owned()
}

#[test]
fn reference_bundles_match_their_checked_in_wire_bytes() {
    for (file, sequence) in [
        ("wire/econ_enable.json", econ_enable()),
        ("wire/every_element.json", every_element()),
    ] {
        let written = to_wire_string(&sequence);
        compare_or_bless(&fixture(file), &written);
        let read = from_wire_str(&written).expect("canonical bundle reads back");
        assert_eq!(read, sequence, "{file}: wire → model loses nothing");
    }
}

#[test]
fn reference_bundles_validate_clean() {
    for sequence in [econ_enable(), every_element()] {
        let findings = validate(&sequence);
        assert!(findings.is_empty(), "{}: {findings:#?}", sequence.name);
    }
}

#[test]
fn every_checked_in_bundle_round_trips_byte_for_byte() {
    let dir = fixture("wire");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("read fixtures/wire")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    entries.sort();
    assert!(
        entries.len() >= 3,
        "the bundle corpus is present: {entries:?}"
    );
    for path in entries {
        let text = std::fs::read_to_string(&path).expect("read bundle");
        let model =
            from_wire_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert_eq!(to_wire_string(&model), text, "{}", path.display());
        assert_eq!(from_wire_str(&to_wire_string(&model)).as_ref(), Ok(&model));
    }
}

#[test]
fn the_notes_worked_wire_example_parses_and_canonicalizes_stably() {
    let example = note_wire_example();
    let model = from_wire_str(&example).expect("the note's example parses");
    assert_eq!(model.name.as_str(), "econ_enable");
    assert_eq!(model.params.len(), 1);
    assert_eq!(model.connections.len(), 1);
    let canonical = to_wire_string(&model);
    compare_or_bless(&fixture("wire/note_example.json"), &canonical);
    let again = from_wire_str(&canonical).expect("canonical form reads back");
    assert_eq!(again, model);
    assert_eq!(to_wire_string(&again), canonical);
}

#[test]
fn the_notes_abridged_example_reports_its_missing_block() {
    // The note abridges the document: `sub` is connected but not declared.
    let model = from_wire_str(&note_wire_example()).expect("parses");
    let codes: Vec<_> = validate(&model)
        .into_iter()
        .map(|f| (f.code, f.subject))
        .collect();
    assert_eq!(
        codes,
        vec![("unknown-source", "connection[0]".to_owned())],
        "only the abridgement is wrong"
    );
}

#[test]
fn writing_twice_yields_identical_bytes() {
    let sequence = every_element();
    assert_eq!(to_wire_string(&sequence), to_wire_string(&sequence.clone()));
}

#[test]
fn non_canonical_spacing_reads_to_the_same_model() {
    let canonical = to_wire_string(&econ_enable());
    let squeezed: String = canonical
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("")
        .replace(" -> ", "->");
    let model = from_wire_str(&squeezed).expect("compact JSON reads");
    assert_eq!(model, econ_enable());
    assert_eq!(to_wire_string(&model), canonical);
}
