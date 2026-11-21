//! The golden diffs (diff note, "Test plan"): the note's three worked examples and the literal
//! variant of the first, each as a checked-in pair in the text form with golden JSON, text and
//! Markdown beside it, compared byte for byte. `every_kind` is a fifth pair, written for this
//! crate, whose diff holds every kind of every section, so each template of both renderers is
//! pinned at least once.
//!
//! The goldens regenerate only through the crate's bless switch
//! (`OCE_BLESS_DIFF=1 cargo nextest run -p oce-diff`). Every diff records the catalog's content
//! id, so a change to the catalog moves the `catalog` line of every JSON golden and nothing
//! else.
//!
//! Beside the bytes: each side diffed against itself is empty, `graph_equal` is the same with
//! the rename pass on and off, and the golden JSON reads back to the same diff, from which both
//! renderings are reproduced.

mod support;

use oce_author::validate_author_side;
use oce_diff::{Diff, Note, render_markdown, render_text};

use support::{
    EXAMPLES, compare, compare_or_bless, compare_without_renames, example, fixture, read,
};

/// The worked examples and the pair that holds every kind.
fn pairs() -> Vec<&'static str> {
    EXAMPLES.into_iter().chain(["every_kind"]).collect()
}

#[test]
fn every_pair_matches_its_json_text_and_markdown_goldens() {
    for name in pairs() {
        let (base, head) = example(name);
        let diff = compare(&base, &head);
        compare_or_bless(
            &fixture(&format!("examples/{name}.diff.json")),
            &diff.to_json(),
        );
        compare_or_bless(
            &fixture(&format!("examples/{name}.diff.txt")),
            &render_text(&diff),
        );
        compare_or_bless(
            &fixture(&format!("examples/{name}.diff.md")),
            &render_markdown(&diff),
        );
    }
}

#[test]
fn each_side_diffed_against_itself_is_empty_and_equal() {
    for name in pairs() {
        let (base, head) = example(name);
        for side in [&base, &head] {
            let diff = compare(side, side);
            assert!(diff.summary.equal, "{name}");
            assert!(diff.summary.graph_equal, "{name}");
            assert!(diff.summary.interface_equal, "{name}");
            assert_eq!(diff.summary.parameters_equal, Some(true), "{name}");
            assert_eq!(diff.summary.metadata_equal, Some(true), "{name}");
            assert!(diff.notes.is_empty(), "{name}");
            assert!(diff.parameters.is_empty(), "{name}");
            assert!(diff.interface.is_empty(), "{name}");
            assert!(diff.graph.is_empty(), "{name}");
            assert!(diff.metadata.is_empty(), "{name}");
            assert_eq!(diff.summary.blocks.by_name, side.blocks.len(), "{name}");
            assert_eq!(diff.summary.blocks.renamed, 0, "{name}");
            assert_eq!(diff.summary.blocks.base_only, 0, "{name}");
            assert_eq!(diff.summary.blocks.head_only, 0, "{name}");
        }
    }
}

#[test]
fn graph_equal_is_the_same_with_the_rename_pass_on_and_off() {
    for name in pairs() {
        let (base, head) = example(name);
        let on = compare(&base, &head);
        let off = compare_without_renames(&base, &head);
        assert_eq!(on.summary.graph_equal, off.summary.graph_equal, "{name}");
        assert!(off.notes.contains(&Note::RenamePassOff), "{name}");
        assert_eq!(off.summary.blocks.renamed, 0, "{name}");
        // The pass only moves blocks between "renamed" and "removed plus added".
        let blocks = on.summary.blocks;
        assert_eq!(
            off.summary.blocks.base_only,
            blocks.base_only + blocks.renamed,
            "{name}"
        );
        assert_eq!(
            off.summary.blocks.head_only,
            blocks.head_only + blocks.renamed,
            "{name}"
        );
        // Nothing outside the graph section and the notes depends on the pass.
        assert_eq!(on.parameters, off.parameters, "{name}");
        assert_eq!(on.interface, off.interface, "{name}");
        assert_eq!(on.metadata, off.metadata, "{name}");
    }
}

#[test]
fn the_golden_json_reads_back_to_the_diff_and_renders_both_goldens() {
    for name in pairs() {
        let (base, head) = example(name);
        let golden = read(&format!("examples/{name}.diff.json"));
        let parsed: Diff = serde_json::from_str(&golden).expect("the golden is a Diff");
        assert_eq!(parsed, compare(&base, &head), "{name}");
        assert_eq!(parsed.to_json(), golden, "{name}");
        assert_eq!(
            render_text(&parsed),
            read(&format!("examples/{name}.diff.txt")),
            "{name}"
        );
        assert_eq!(
            render_markdown(&parsed),
            read(&format!("examples/{name}.diff.md")),
            "{name}"
        );
    }
}

#[test]
fn the_worked_examples_are_sequences_the_validator_accepts() {
    // The diff never validates, but the note's examples are meant to be real sequences: a
    // fixture the author-side validator refused would be a transcription slip.
    for name in EXAMPLES {
        let (base, head) = example(name);
        for (side, model) in [("base", &base), ("head", &head)] {
            let findings = validate_author_side(model);
            assert!(findings.is_empty(), "{name} {side}: {findings:?}");
        }
    }
}

#[test]
fn the_pair_that_holds_every_kind_holds_every_kind() {
    let (base, head) = example("every_kind");
    let json = support::json(&compare(&base, &head));
    let kinds = |section: &str| -> Vec<String> {
        let mut kinds: Vec<String> = support::kinds(&json[section])
            .into_iter()
            .map(|(kind, _)| kind)
            .collect();
        kinds.sort();
        kinds.dedup();
        kinds
    };
    assert_eq!(
        kinds("parameters"),
        ["added", "changed", "removed", "reordered", "type-changed"]
    );
    assert_eq!(
        kinds("interface"),
        [
            "added",
            "display-unit-changed",
            "max-changed",
            "min-changed",
            "quantity-changed",
            "removed",
            "reordered",
            "type-changed",
            "unit-changed"
        ]
    );
    assert_eq!(
        kinds("graph"),
        [
            "added",
            "parameter-defaulted",
            "removed",
            "renamed",
            "reordered",
            "reparameterized",
            "rewired"
        ]
    );
    assert_eq!(
        kinds("metadata"),
        [
            "comment-changed",
            "derived-from-changed",
            "literal-respelled",
            "semantic-changed",
            "sequence-renamed"
        ]
    );
}
