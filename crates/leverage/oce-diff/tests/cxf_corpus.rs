//! The CXF adapter over the content-id corpus (issue #666): every tracked document the facade's
//! content-id pin lists, the 46 G36 catalog fixtures among them.
//!
//! - A document diffed against itself, or against a second load of itself, is empty.
//! - A re-spelled copy (its `@graph` and node lists reversed, its `S231:` names written in full,
//!   its instance IRIs written as compact IRIs) keeps its content id and diffs empty both ways.
//! - Equal content ids never coexist with a graph or interface entry, over every pair of the
//!   corpus and its copies; every pair diffs without a refusal.
//! - The same two documents give the same bytes on every run.

mod support;

use oce_diff::{Diff, Source, render_markdown, render_text};

use support::loaded::{
    Loaded, compacted, compare_loaded, corpus, corpus_bytes, expanded, load, namespace, reordered,
};

#[path = "../../../oce-api/tests/support/blob_corpus.rs"]
mod blob_corpus;

/// The corpus, loaded, each with its path.
fn loaded_corpus() -> Vec<(String, Loaded)> {
    corpus()
        .into_iter()
        .map(|(path, id)| {
            let loaded = load(&corpus_bytes(&path));
            assert_eq!(loaded.id.to_string(), id, "{path} loads to its pinned id");
            (path, loaded)
        })
        .collect()
}

/// Assert that `diff` is the empty CXF diff of a document with `blocks` blocks.
fn assert_empty(diff: &Diff, blocks: usize, context: &str) {
    assert_eq!(diff.source, Source::Cxf, "{context}");
    assert!(diff.summary.equal, "{context}\n{}", diff.to_json());
    assert!(
        diff.summary.graph_equal && diff.summary.interface_equal,
        "{context}"
    );
    assert_eq!(diff.summary.parameters_equal, None, "{context}");
    assert_eq!(diff.summary.metadata_equal, None, "{context}");
    assert_eq!(diff.summary.blocks.by_name, blocks, "{context}");
    assert_eq!(
        (
            diff.summary.blocks.renamed,
            diff.summary.blocks.base_only,
            diff.summary.blocks.head_only
        ),
        (0, 0, 0),
        "{context}"
    );
    assert!(diff.notes.is_empty(), "{context}: {:?}", diff.notes);
    assert!(
        diff.parameters.is_empty() && diff.metadata.is_empty(),
        "{context}"
    );
    assert_eq!(diff.base, diff.head, "{context}");
}

#[test]
fn the_corpus_is_the_content_id_pin_and_holds_the_g36_catalog_fixtures() {
    let paths: Vec<String> = corpus().into_iter().map(|(path, _)| path).collect();
    assert_eq!(paths.len(), 47);
    for name in blob_corpus::FIXTURES {
        let path = format!("crates/oce-cxf/tests/fixtures/g36/{name}.jsonld");
        assert!(paths.contains(&path), "{path} is in the corpus");
    }
    assert_eq!(blob_corpus::FIXTURES.len(), 46);
}

#[test]
fn every_corpus_document_diffed_against_itself_or_a_second_load_is_empty() {
    for (path, first) in loaded_corpus() {
        let second = load(&corpus_bytes(&path));
        let blocks = first.topology.blocks.len();
        assert_empty(&compare_loaded(&first, &first), blocks, &path);
        assert_empty(&compare_loaded(&first, &second), blocks, &path);
        assert_empty(&compare_loaded(&second, &first), blocks, &path);
        // Loading twice changes nothing the diff reads.
        assert_eq!(first.topology, second.topology, "{path}");
        assert_eq!(first.id, second.id, "{path}");
    }
}

/// The re-spelled copies of a document: each with its name.
fn copies(bytes: &[u8], namespace: &str) -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("reordered", reordered(bytes)),
        ("expanded", expanded(bytes)),
        ("compacted", compacted(bytes, namespace)),
        (
            "reordered, expanded and compacted",
            compacted(&expanded(&reordered(bytes)), namespace),
        ),
    ]
}

#[test]
fn a_respelled_copy_keeps_its_id_and_diffs_empty_both_ways() {
    for (path, original) in loaded_corpus() {
        let bytes = corpus_bytes(&path);
        let blocks = original.topology.blocks.len();
        for (spelling, copy) in copies(&bytes, &namespace(&original.topology)) {
            assert_ne!(copy, bytes, "{path} {spelling} is another spelling");
            let copy = load(&copy);
            let context = format!("{path} {spelling}");
            assert_eq!(copy.id, original.id, "{context}");
            assert_empty(&compare_loaded(&original, &copy), blocks, &context);
            assert_empty(&compare_loaded(&copy, &original), blocks, &context);
        }
    }
}

#[test]
fn a_reordered_copy_moves_the_loader_order_and_still_diffs_empty() {
    // The copies are not trivially equal: reversing the `@graph` and `containsBlock` reverses the
    // loader's block order, which the diff never compares.
    let mut moved = 0;
    for (path, original) in loaded_corpus() {
        let copy = load(&reordered(&corpus_bytes(&path)));
        let order = |loaded: &Loaded| -> Vec<String> {
            loaded
                .topology
                .blocks
                .iter()
                .map(|block| block.instance_path.clone())
                .collect()
        };
        if order(&copy) != order(&original) {
            moved += 1;
        }
        let diff = compare_loaded(&original, &copy);
        assert!(diff.summary.equal, "{path}");
        assert!(
            !diff.to_json().contains("reordered"),
            "{path}: no entry is reordered"
        );
    }
    // Twelve corpus documents have one block, which no order can move.
    assert!(
        moved >= 30,
        "the copies moved the block order of {moved} documents"
    );
}

#[test]
fn equal_content_ids_never_coexist_with_a_graph_or_interface_entry() {
    // Each document with its four copies is a family of one id; every pair inside a family, and
    // every pair of two families' originals, is diffed.
    let families: Vec<Vec<(String, Loaded)>> = loaded_corpus()
        .into_iter()
        .map(|(path, original)| {
            let bytes = corpus_bytes(&path);
            let mut family: Vec<(String, Loaded)> = copies(&bytes, &namespace(&original.topology))
                .into_iter()
                .map(|(spelling, copy)| (format!("{path} {spelling}"), load(&copy)))
                .collect();
            family.insert(0, (path, original));
            family
        })
        .collect();
    type Named = (String, Loaded);
    let mut pairs: Vec<(&Named, &Named)> = Vec::new();
    for family in &families {
        for base in family {
            for head in family {
                pairs.push((base, head));
            }
        }
    }
    for base in &families {
        for head in &families {
            if !std::ptr::eq(base, head) {
                pairs.push((&base[0], &head[0]));
            }
        }
    }
    let (mut equal_ids, mut unequal_ids, mut roots_only) = (0, 0, Vec::new());
    for ((base_name, base), (head_name, head)) in pairs {
        let diff = compare_loaded(base, head);
        if base.id == head.id {
            equal_ids += 1;
            assert!(
                diff.graph.is_empty() && diff.interface.is_empty(),
                "{base_name} against {head_name}: equal ids, and\n{}",
                diff.to_json()
            );
            assert!(diff.summary.equal, "{base_name} against {head_name}");
        } else {
            unequal_ids += 1;
            // The converse does not hold, and only a root tells them apart: the id covers each
            // name's root, which this level strips.
            if diff.summary.equal {
                assert_ne!(
                    diff.base.sequence, diff.head.sequence,
                    "{base_name} against {head_name}"
                );
                roots_only.push(diff.base.sequence.clone());
            }
        }
    }
    // 47 families of five; no two corpus documents share an id.
    assert_eq!(equal_ids, 47 * 25);
    assert_eq!(unequal_ids, 47 * 46);
    // Two of the one-block economizer high-limit documents bind the same block to the same
    // values under two roots: the one pair, both ways round.
    assert_eq!(
        roots_only,
        [
            "g36.source.generic_air_economizer_high_limits_ashrae_differential",
            "g36.source.generic_air_economizer_high_limits_title24_differential_offset_0"
        ]
    );
}

#[test]
fn the_same_two_documents_give_the_same_bytes_on_every_run() {
    let corpus = corpus();
    for window in corpus.windows(2) {
        let [(base, _), (head, _)] = window else {
            continue;
        };
        let render = || {
            let diff = compare_loaded(&load(&corpus_bytes(base)), &load(&corpus_bytes(head)));
            (diff.to_json(), render_text(&diff), render_markdown(&diff))
        };
        let first = render();
        for _ in 0..2 {
            assert_eq!(render(), first, "{base} against {head}");
        }
    }
}
