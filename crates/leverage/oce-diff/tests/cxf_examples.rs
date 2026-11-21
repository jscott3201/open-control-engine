//! The note's adapter test over its worked examples (diff note, "Test plan", adapters; issue
//! #666): each side of (a), its literal variant, (b) and (c) emitted with `oce_author::emit`,
//! loaded, and diffed at the CXF level.
//!
//! - The CXF diff "agrees with the authoring diff on graph entries after grounding and reports
//!   `parameters_equal: null`": for (a), whose retune is two sequence parameters, each
//!   parameter's `changed` entry is a `reparameterized` entry on each assignment it `affects`
//!   (`dehum.uHigh` and `dehum.uLow`), with the same old and new values; for the literal variant
//!   the graph entries are the authoring diff's own; for (b) and (c) they are the authoring
//!   diff's once each Integer literal on a Real parameter is the Real the emitter writes.
//! - Golden JSON, text and Markdown of the four CXF diffs, byte for byte, regenerated only
//!   through the crate's bless switch (`OCE_BLESS_DIFF=1 cargo nextest run -p oce-diff`).

mod support;

use oce_diff::{
    Diff, DiffOptions, GraphChange, ParameterChange, diff_topology, render_markdown, render_text,
};

use support::grounding::{grounded, seen_by_both};
use support::loaded::{compare_loaded, loaded};
use support::{EXAMPLES, compare, compare_or_bless, example, fixture};

/// The CXF diff of a worked example's two sides, emitted and loaded.
fn cxf(name: &str) -> Diff {
    let (base, head) = example(name);
    compare_loaded(&loaded(&base), &loaded(&head))
}

#[test]
fn every_example_matches_its_cxf_json_text_and_markdown_goldens() {
    for name in EXAMPLES {
        let diff = cxf(name);
        compare_or_bless(
            &fixture(&format!("examples/{name}.cxf.diff.json")),
            &diff.to_json(),
        );
        compare_or_bless(
            &fixture(&format!("examples/{name}.cxf.diff.txt")),
            &render_text(&diff),
        );
        compare_or_bless(
            &fixture(&format!("examples/{name}.cxf.diff.md")),
            &render_markdown(&diff),
        );
        // The golden reads back to the same diff, strictly.
        let read: Diff =
            serde_json::from_str(&support::read(&format!("examples/{name}.cxf.diff.json")))
                .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(read, diff, "{name}");
    }
}

#[test]
fn a_sequence_parameter_retune_is_reparameterized_on_each_leaf_it_affects() {
    let (base, head) = example("site_retune");
    let authoring = compare(&base, &head);
    assert!(authoring.summary.graph_equal);
    assert_eq!(authoring.summary.parameters_equal, Some(false));
    let mut expected: Vec<GraphChange> = Vec::new();
    for entry in &authoring.parameters {
        let ParameterChange::Changed {
            affects, old, new, ..
        } = entry
        else {
            panic!("only changed values: {entry:?}");
        };
        for subject in affects {
            let entry = serde_json::json!({ "kind": "reparameterized", "subject": subject,
                                            "old": old, "new": new });
            expected.push(serde_json::from_value(entry).expect("an entry"));
        }
    }
    expected.sort_by(|a, b| a.subject().cmp(b.subject()));
    let diff = cxf("site_retune");
    assert_eq!(diff.graph, expected);
    let subjects: Vec<&str> = diff.graph.iter().map(GraphChange::subject).collect();
    assert_eq!(subjects, ["dehum.uHigh", "dehum.uLow"]);
    assert!(!diff.summary.graph_equal);
    assert_eq!(diff.summary.parameters_equal, None);
    assert_eq!(diff.summary.metadata_equal, None);
    assert!(diff.parameters.is_empty() && diff.metadata.is_empty());
    // The literal variant authors the same retune as two literals: the same loaded graphs, the
    // same CXF diff, and its graph entries are the authoring diff's own.
    let literal = cxf("literal_retune");
    assert_eq!(literal.graph, diff.graph);
    let (base, head) = example("literal_retune");
    assert_eq!(literal.graph, compare(&base, &head).graph);
}

#[test]
fn the_other_examples_agree_with_the_authoring_diff_after_grounding() {
    for name in ["pressure_request", "plant_reset"] {
        let (base, head) = example(name);
        let authoring = compare(&base, &head);
        let grounded_authoring = compare(&grounded(&base), &grounded(&head));
        let diff = cxf(name);
        // The same entries; a CXF entry lists a block's parameters by name, an authoring one in
        // authored order, which `seen_by_both` sorts.
        assert_eq!(
            seen_by_both(&diff)["graph"],
            seen_by_both(&grounded_authoring)["graph"],
            "{name}"
        );
        assert_eq!(diff.graph.len(), grounded_authoring.graph.len(), "{name}");
        assert_eq!(diff.notes, authoring.notes, "{name}");
        assert_eq!(diff.summary.blocks, authoring.summary.blocks, "{name}");
        assert_eq!(
            diff.summary.graph_equal, authoring.summary.graph_equal,
            "{name}"
        );
        assert_eq!(diff.summary.parameters_equal, None, "{name}");
        // Grounding changes values only: every rename, rewiring and confidence is the
        // authoring diff's, and so is every entry without a parameter.
        let without_values = |diff: &Diff| -> Vec<(String, String)> {
            support::kinds(&support::json(diff)["graph"])
        };
        assert_eq!(without_values(&diff), without_values(&authoring), "{name}");
        let rewired = |diff: &Diff| -> Vec<GraphChange> {
            let entries = diff.graph.iter();
            entries
                .filter(|e| matches!(e, GraphChange::Rewired { .. }))
                .cloned()
                .collect()
        };
        assert_eq!(rewired(&diff), rewired(&authoring), "{name}");
    }
}

#[test]
fn graph_equal_is_the_same_with_the_rename_pass_on_and_off() {
    for name in EXAMPLES {
        let (base, head) = example(name);
        let (base, head) = (loaded(&base), loaded(&head));
        let mut off = DiffOptions::default();
        off.renames = false;
        let on = diff_topology(&base.topology, &head.topology, &DiffOptions::default());
        let off = diff_topology(&base.topology, &head.topology, &off);
        let (on, off) = (on.expect("diffs"), off.expect("diffs"));
        assert_eq!(on.summary.graph_equal, off.summary.graph_equal, "{name}");
    }
}
