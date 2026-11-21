//! Determinism (diff note, "Test plan"; `TESTING.md` pillar 4): the same inputs give
//! byte-identical JSON, text and Markdown; permuting either side's statements changes only
//! `reordered` entries; the rename result is invariant under every permutation of the examples'
//! blocks, enumerated without a random number; every section is sorted by its rule; and the
//! diff of head against base mirrors the diff of base against head.

mod support;

use oce_author::Sequence;
use oce_diff::{Diff, render_markdown, render_text};
use serde_json::Value;

use support::{EXAMPLES, compare, example};

fn pairs() -> Vec<(&'static str, Sequence, Sequence)> {
    EXAMPLES
        .into_iter()
        .chain(["every_kind"])
        .map(|name| {
            let (base, head) = example(name);
            (name, base, head)
        })
        .collect()
}

/// Every permutation of `0..length`, by Heap's algorithm: enumerated, never drawn.
fn permutations(length: usize) -> Vec<Vec<usize>> {
    fn heap(k: usize, order: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if k <= 1 {
            out.push(order.clone());
            return;
        }
        for i in 0..k {
            heap(k - 1, order, out);
            let swap = if k.is_multiple_of(2) { i } else { 0 };
            order.swap(swap, k - 1);
        }
    }
    let mut out = Vec::new();
    heap(length, &mut (0..length).collect(), &mut out);
    out
}

fn permuted<T: Clone>(list: &[T], order: &[usize]) -> Vec<T> {
    order.iter().map(|&at| list[at].clone()).collect()
}

/// The diff's JSON with the `reordered` entries of every section taken out.
fn without_reordered(diff: &Diff) -> Value {
    let mut json = support::json(diff);
    for section in ["parameters", "interface", "graph", "metadata"] {
        json[section]
            .as_array_mut()
            .expect("a section")
            .retain(|entry| entry["kind"] != "reordered");
    }
    // The four flags follow the sections; with the reorderings gone they are recomputed by the
    // caller's own comparison, so they are left out here.
    json["summary"]["equal"].take();
    for flag in ["graph_equal", "parameters_equal", "interface_equal"] {
        json["summary"][flag].take();
    }
    json
}

#[test]
fn the_same_inputs_give_the_same_bytes_every_time() {
    for (name, base, head) in pairs() {
        let first = compare(&base, &head);
        for _ in 0..3 {
            let again = compare(&base, &head);
            assert_eq!(again.to_json(), first.to_json(), "{name}");
            assert_eq!(render_text(&again), render_text(&first), "{name}");
            assert_eq!(render_markdown(&again), render_markdown(&first), "{name}");
        }
        // And from another thread, with its own allocator state and hasher seeds.
        let (base, head, expected) = (base.clone(), head.clone(), first.to_json());
        let other = std::thread::spawn(move || compare(&base, &head).to_json())
            .join()
            .expect("the thread finishes");
        assert_eq!(other, expected, "{name}");
    }
}

#[test]
fn the_number_of_permutations_enumerated_is_the_factorial() {
    assert_eq!(permutations(0).len(), 1);
    assert_eq!(permutations(1).len(), 1);
    assert_eq!(permutations(5).len(), 120);
    let mut six = permutations(6);
    assert_eq!(six.len(), 720);
    six.sort();
    six.dedup();
    assert_eq!(six.len(), 720, "every permutation is a different one");
}

#[test]
fn the_rename_result_is_the_same_under_every_permutation_of_the_examples_blocks() {
    for (name, base, head) in pairs() {
        // `every_kind` holds a reordering already; the rule under test is the same.
        let expected = without_reordered(&compare(&base, &head));
        for order in permutations(base.blocks.len()) {
            let mut shuffled = base.clone();
            shuffled.blocks = permuted(&base.blocks, &order);
            assert_eq!(
                without_reordered(&compare(&shuffled, &head)),
                expected,
                "{name}: base blocks in order {order:?}"
            );
        }
        for order in permutations(head.blocks.len()) {
            let mut shuffled = head.clone();
            shuffled.blocks = permuted(&head.blocks, &order);
            assert_eq!(
                without_reordered(&compare(&base, &shuffled)),
                expected,
                "{name}: head blocks in order {order:?}"
            );
        }
    }
}

/// The reversal and every rotation of `0..length`.
fn rearrangements(length: usize) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = (0..length)
        .map(|shift| (0..length).map(|at| (at + shift) % length).collect())
        .collect();
    out.push((0..length).rev().collect());
    out
}

#[test]
fn permuting_either_sides_statements_changes_only_reordered_entries() {
    for (name, base, head) in pairs() {
        let expected = without_reordered(&compare(&base, &head));
        for side in ["base", "head"] {
            let original = if side == "base" { &base } else { &head };
            let lists: [(&str, usize); 5] = [
                ("params", original.params.len()),
                ("inputs", original.inputs.len()),
                ("outputs", original.outputs.len()),
                ("blocks", original.blocks.len()),
                ("connections", original.connections.len()),
            ];
            for (list, length) in lists {
                for order in rearrangements(length) {
                    let mut shuffled = original.clone();
                    match list {
                        "params" => shuffled.params = permuted(&original.params, &order),
                        "inputs" => shuffled.inputs = permuted(&original.inputs, &order),
                        "outputs" => shuffled.outputs = permuted(&original.outputs, &order),
                        "blocks" => shuffled.blocks = permuted(&original.blocks, &order),
                        _ => shuffled.connections = permuted(&original.connections, &order),
                    }
                    let diff = if side == "base" {
                        compare(&shuffled, &head)
                    } else {
                        compare(&base, &shuffled)
                    };
                    assert_eq!(
                        without_reordered(&diff),
                        expected,
                        "{name}: {side} {list} in order {order:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_reversed_list_is_reported_reordered_and_nothing_else_is() {
    let (base, _) = example("pressure_request");
    let mut reversed = base.clone();
    reversed.blocks.reverse();
    reversed.connections.reverse();
    reversed.inputs.reverse();
    let json = support::json(&compare(&base, &reversed));
    assert_eq!(
        support::kinds(&json["graph"]),
        [
            ("reordered".to_owned(), "blocks".to_owned()),
            ("reordered".to_owned(), "connections".to_owned())
        ]
    );
    assert_eq!(
        support::kinds(&json["interface"]),
        [("reordered".to_owned(), "inputs".to_owned())]
    );
    assert_eq!(json["metadata"], serde_json::json!([]));
    assert_eq!(json["summary"]["equal"], serde_json::json!(false));
}

#[test]
fn every_section_is_sorted_by_subject_then_kind_then_the_rest() {
    /// The kinds of each section in enum order.
    const ORDER: [(&str, &[&str]); 4] = [
        (
            "parameters",
            &["added", "removed", "changed", "type-changed", "reordered"],
        ),
        (
            "interface",
            &[
                "added",
                "removed",
                "type-changed",
                "unit-changed",
                "quantity-changed",
                "display-unit-changed",
                "min-changed",
                "max-changed",
                "reordered",
            ],
        ),
        (
            "graph",
            &[
                "added",
                "removed",
                "renamed",
                "reparameterized",
                "rewired",
                "reordered",
                "parameter-defaulted",
            ],
        ),
        (
            "metadata",
            &[
                "sequence-renamed",
                "derived-from-changed",
                "semantic-changed",
                "comment-changed",
                "literal-respelled",
            ],
        ),
    ];
    let mut entries = 0;
    for (name, base, head) in pairs() {
        for diff in [compare(&base, &head), compare(&head, &base)] {
            let json = support::json(&diff);
            for (section, order) in ORDER {
                let keys: Vec<(Vec<u8>, usize)> = support::kinds(&json[section])
                    .into_iter()
                    .map(|(kind, subject)| {
                        let rank = order
                            .iter()
                            .position(|known| *known == kind)
                            .unwrap_or_else(|| panic!("{section} has no kind `{kind}`"));
                        (subject.into_bytes(), rank)
                    })
                    .collect();
                entries += keys.len();
                assert!(
                    keys.windows(2).all(|pair| pair[0] <= pair[1]),
                    "{name}: {section} is not sorted"
                );
            }
        }
    }
    assert!(entries > 80, "the check saw {entries} entries");
}

/// The entry `forward` holds, as the diff taken the other way round would hold it.
fn mirrored(mut entry: Value, renames: &[(String, String)]) -> Value {
    let object = entry.as_object_mut().expect("an entry");
    for (a, b) in [("old", "new"), ("old_type", "new_type")] {
        if object.contains_key(a) || object.contains_key(b) {
            let old = object.remove(a).unwrap_or(Value::Null);
            let new = object.remove(b).unwrap_or(Value::Null);
            object.insert(a.to_owned(), new);
            object.insert(b.to_owned(), old);
        }
    }
    let kind = object["kind"].as_str().expect("kind").to_owned();
    match kind.as_str() {
        "added" => object["kind"] = Value::from("removed"),
        "removed" => object["kind"] = Value::from("added"),
        "renamed" => {
            let subject = object.remove("subject").expect("subject");
            let from = object.remove("from").expect("from");
            object.insert("subject".to_owned(), from);
            object.insert("from".to_owned(), subject);
        }
        "rewired" => {
            // A rewired target is named in head terms: map a renamed block back.
            let subject = object["subject"].as_str().expect("subject").to_owned();
            if let Some((block, port)) = subject.split_once('.')
                && let Some((from, _)) = renames.iter().find(|(_, to)| to == block)
            {
                object["subject"] = Value::from(format!("{from}.{port}"));
            }
        }
        "parameter-defaulted" => {
            let side = if object["side"] == "base" {
                "head"
            } else {
                "base"
            };
            object["side"] = Value::from(side);
        }
        _ => {}
    }
    // `affects` is read from head, which is the other model the other way round.
    if matches!(kind.as_str(), "changed" | "type-changed") {
        object.remove("affects");
    }
    entry
}

/// A section as a sorted list of its entries' canonical text, `affects` dropped from the kinds
/// that read it from head.
fn canonical(section: &Value, transform: impl Fn(Value) -> Value) -> Vec<String> {
    let mut entries: Vec<String> = section
        .as_array()
        .expect("a section")
        .iter()
        .cloned()
        .map(|entry| transform(entry).to_string())
        .collect();
    entries.sort();
    entries
}

#[test]
fn the_diff_of_head_against_base_mirrors_the_diff_of_base_against_head() {
    for (name, base, head) in pairs() {
        let forward = compare(&base, &head);
        let backward = compare(&head, &base);
        let renames: Vec<(String, String)> = forward
            .graph
            .iter()
            .filter_map(|change| match change {
                oce_diff::GraphChange::Renamed { subject, from, .. } => {
                    Some((from.clone(), subject.clone()))
                }
                _ => None,
            })
            .collect();
        let (forward_json, backward_json) = (support::json(&forward), support::json(&backward));
        for section in ["parameters", "interface", "graph", "metadata"] {
            let drop_affects = |mut entry: Value| {
                if matches!(entry["kind"].as_str(), Some("changed" | "type-changed")) {
                    entry.as_object_mut().expect("an entry").remove("affects");
                }
                entry
            };
            assert_eq!(
                canonical(&forward_json[section], |entry| mirrored(entry, &renames)),
                canonical(&backward_json[section], drop_affects),
                "{name}: {section}"
            );
        }
        let (a, b) = (forward.summary, backward.summary);
        assert_eq!(a.equal, b.equal, "{name}");
        assert_eq!(a.graph_equal, b.graph_equal, "{name}");
        assert_eq!(a.parameters_equal, b.parameters_equal, "{name}");
        assert_eq!(a.interface_equal, b.interface_equal, "{name}");
        assert_eq!(a.metadata_equal, b.metadata_equal, "{name}");
        assert_eq!(a.blocks.by_name, b.blocks.by_name, "{name}");
        assert_eq!(a.blocks.renamed, b.blocks.renamed, "{name}");
        assert_eq!(a.blocks.base_only, b.blocks.head_only, "{name}");
        assert_eq!(a.blocks.head_only, b.blocks.base_only, "{name}");
        assert_eq!(forward.notes, backward.notes, "{name}");
        assert_eq!(forward.base, backward.head, "{name}");
        assert_eq!(forward.head, backward.base, "{name}");
    }
}
