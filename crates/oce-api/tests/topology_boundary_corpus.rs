//! The boundary facts on a topology snapshot over the corpus (issue #701): the 47 documents
//! `content_id/pin.tsv` pins, the 46 G36 catalog fixtures and `member_list_interface`.
//!
//! For every document the root id is the load report's and is not what the block paths share,
//! the boundary inputs are the input definitions with the declarations' labels, and each
//! declared output has its frame output's type and its authored labels. Then the property, in
//! its contrapositive: every label or bound edit of a boundary that moves the canonical content
//! id moves the topology. `topology_boundary.rs` holds the same one named edit at a time.

// Shared helper, declared once for this binary in its root `facade.rs`.
use crate::content_id_corpus as corpus;

use std::collections::BTreeSet;
use std::path::PathBuf;

use corpus::*;
use oce_api::{Engine, Topology, ValueType};
use serde_json::{Value as Json, json};

fn double(value: &str) -> Json {
    json!({ "@value": value, "@type": "http://www.w3.org/2001/XMLSchema#double" })
}

/// The documents `content_id/pin.tsv` pins, as paths from the repository root.
fn pinned() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pin =
        std::fs::read_to_string(root.join("crates/oce-api/tests/fixtures/content_id/pin.tsv"))
            .unwrap();
    let paths: Vec<PathBuf> = pin
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| root.join(line.split('\t').nth(1).expect("a path column")))
        .collect();
    assert_eq!(paths.len(), 47);
    paths
}

/// The longest prefix every block path shares: what a reader without the root id could derive.
fn shared_prefix(topology: &Topology) -> String {
    let mut paths = topology.blocks.iter().map(|block| &block.instance_path);
    let mut prefix = paths.next().cloned().unwrap_or_default();
    for path in paths {
        let shared = prefix
            .char_indices()
            .zip(path.chars())
            .find(|((_, a), b)| a != b)
            .map_or(prefix.len().min(path.len()), |((at, _), _)| at);
        prefix.truncate(shared);
    }
    prefix
}

#[test]
fn the_root_id_is_the_load_reports_model_id_and_the_block_paths_do_not_give_it() {
    let (mut one_block, mut catalog) = (0, 0);
    for path in pinned() {
        let mut engine = Engine::new();
        let (report, model) = engine.load_cxf(&std::fs::read(&path).unwrap()).unwrap();
        let topology = engine.topology();
        let name = path.display();
        assert_eq!(topology.model_id, report.model_id, "{name}");
        assert_eq!(topology.model_id, model.model_id, "{name}");
        assert!(!topology.model_id.as_str().is_empty(), "{name}");
        catalog += usize::from(!path.ends_with("member_list_interface.jsonld"));

        // Every authored identity of these flat documents is written under the root.
        let under = format!("{}.", topology.model_id.as_str());
        let boundary = topology
            .boundary_inputs
            .iter()
            .map(|input| &input.path)
            .chain(topology.boundary_outputs.iter().map(|output| &output.path));
        for path in topology
            .blocks
            .iter()
            .map(|block| &block.instance_path)
            .chain(boundary)
        {
            let local = path.strip_prefix(&under);
            assert!(
                local.is_some_and(|local| !local.is_empty()),
                "{name}: {path}"
            );
        }

        // The prefix the blocks share is the root's only when there are several blocks: with
        // one block it is that block's whole path, a level too deep.
        let prefix = shared_prefix(&topology);
        if topology.blocks.len() == 1 {
            one_block += 1;
            assert_eq!(prefix, topology.blocks[0].instance_path, "{name}");
            assert_ne!(prefix, under, "{name}");
        } else {
            assert!(prefix.starts_with(&under), "{name}");
        }
    }
    assert_eq!((catalog, one_block), (46, 12));
}

#[test]
fn boundary_inputs_are_the_input_definitions_with_the_declarations_labels() {
    let (mut inputs, mut labelled, mut bounded, mut enums) = (0, 0, 0, 0);
    for name in corpus() {
        let document = fixture_json(&name);
        let engine = load(&fixture_bytes(&name));
        let topology = engine.topology();
        let definitions = engine.input_definitions().unwrap();
        assert_eq!(topology.boundary_inputs.len(), definitions.len(), "{name}");
        for (input, definition) in topology.boundary_inputs.iter().zip(&definitions) {
            assert_eq!(input.path, definition.path, "{name}");
            assert_eq!(input.value_type, definition.value_type, "{name}");
            for (bound, defined) in [(&input.min, &definition.min), (&input.max, &definition.max)] {
                match (bound, defined) {
                    (Some(bound), Some(defined)) => assert!(bound.bit_eq(defined), "{name}"),
                    (None, None) => {}
                    other => panic!("{name}: {}: {other:?}", input.path),
                }
            }
            // The labels are the ones the document writes on the declared node.
            let declared = node(&document, &input.path).expect("the declaration is a node");
            let authored = |key: &str| declared.get(key).map(|v| v.as_str().unwrap().to_owned());
            assert_eq!(input.quantity, authored("S231:quantity"), "{name}");
            assert_eq!(input.unit, authored("S231:unit"), "{name}");
            assert_eq!(input.display_unit, authored("S231:displayUnit"), "{name}");
            inputs += 1;
            labelled += usize::from(input.unit.is_some());
            bounded += usize::from(input.min.is_some() || input.max.is_some());
            enums += usize::from(matches!(input.value_type, ValueType::Enum(_)));
        }
        // UTF-8 path order, each path once, and exactly the paths the graph stages into.
        assert!(
            topology
                .boundary_inputs
                .windows(2)
                .all(|pair| pair[0].path.as_bytes() < pair[1].path.as_bytes()),
            "{name}"
        );
        let staged: BTreeSet<&String> = topology.external_inputs.iter().collect();
        let listed: BTreeSet<&String> = topology.boundary_inputs.iter().map(|i| &i.path).collect();
        assert_eq!(listed, staged, "{name}");
    }
    // No corpus document declares an enumeration-typed input; the count says so.
    assert_eq!((inputs, labelled, bounded, enums), (183, 109, 52, 0));
}

#[test]
fn boundary_outputs_carry_the_frame_output_types_and_the_authored_labels() {
    let (mut outputs, mut authored_units, mut pass_through) = (0, 0, 0);
    for name in corpus() {
        let document = fixture_json(&name);
        let engine = load(&fixture_bytes(&name));
        let topology = engine.topology();
        let slots = engine.frame_slots().unwrap();
        let listed: BTreeSet<&String> = topology.boundary_outputs.iter().map(|o| &o.path).collect();
        assert_eq!(listed, slots.outputs().iter().collect(), "{name}");
        assert_eq!(listed.len(), topology.boundary_outputs.len(), "{name}");
        for output in &topology.boundary_outputs {
            let slot = slots.output_slot(&output.path).expect("an output slot");
            assert_eq!(
                Some(&output.value_type),
                slots.output_types().get(slot),
                "{name}"
            );
            // What the declared node authors survives unification as written.
            let declared = node(&document, &output.path).expect("the declaration is a node");
            for (key, label) in [
                ("S231:quantity", &output.quantity),
                ("S231:unit", &output.unit),
                ("S231:displayUnit", &output.display_unit),
            ] {
                if let Some(written) = declared.get(key) {
                    assert_eq!(
                        label.as_deref(),
                        written.as_str(),
                        "{name}: {}",
                        output.path
                    );
                    authored_units += usize::from(key == "S231:unit");
                }
            }
            outputs += 1;
            pass_through += usize::from(output.path == output.driver_path);
        }
    }
    assert_eq!((outputs, authored_units, pass_through), (132, 78, 2));
}

#[test]
fn a_respelled_document_has_an_equal_topology_and_an_equal_id() {
    // Compact IRIs against the `base` prefix and every object's keys reversed: the same
    // document, so the same expanded paths, root id included.
    for name in corpus() {
        let document = fixture_json(&name);
        let base = load(&fixture_bytes(&name));
        let respelled = load(&reverse_keys(&compact_ids(&document)));
        assert_eq!(base.topology(), respelled.topology(), "{name}");
        assert_eq!(
            base.content_id().unwrap(),
            respelled.content_id().unwrap(),
            "{name}"
        );
    }
}

/// The root's Real boundary declarations under `key` (`S231:hasInput` or `S231:hasOutput`).
fn real_boundaries(document: &Json, key: &str) -> Vec<String> {
    let root = root(document);
    refs(node(document, &root).unwrap().get(key))
        .into_iter()
        .filter(|id| {
            node(document, id).is_some_and(|n| n["S231:isOfDataType"]["@id"] == "S231:Real")
        })
        .collect()
}

/// Over the whole corpus: each label of the Real boundaries under `key`, and for an input a
/// tightened lower bound, each as one document. A variant the engine refuses is no document, and
/// one whose id does not move claims nothing. Returns `(moved, unmoved, refused)`.
///
/// Every variant is a load, so a document takes its first three boundaries, and one of more
/// than 20 blocks its first only.
fn label_and_bound_edits(key: &str) -> (usize, usize, usize) {
    let (mut moved, mut unmoved, mut refused) = (0, 0, 0);
    for name in corpus() {
        let document = fixture_json(&name);
        let base = load(&fixture_bytes(&name));
        let (base_id, base_topology) = (base.content_id().unwrap(), base.topology());
        let reach = if base_topology.blocks.len() <= 20 {
            3
        } else {
            1
        };
        let mut variants: Vec<Json> = Vec::new();
        for boundary in real_boundaries(&document, key).into_iter().take(reach) {
            for (label, value) in [
                ("S231:quantity", "ChangedQuantity"),
                ("S231:unit", "changedUnit"),
                ("S231:displayUnit", "changedDisplay"),
            ] {
                let mut changed = document.clone();
                node_mut(&mut changed, &boundary)[label] = json!(value);
                variants.push(changed);
            }
            let input = base_topology
                .boundary_inputs
                .iter()
                .find(|input| key == "S231:hasInput" && input.path == boundary);
            if let Some(input) = input {
                let min = input.min.as_ref().and_then(|v| v.as_real().ok());
                let max = input.max.as_ref().and_then(|v| v.as_real().ok());
                let tightened = min.map_or(-1.0e30, f64::next_up);
                if max.is_none_or(|max| tightened <= max) {
                    let mut changed = document.clone();
                    node_mut(&mut changed, &boundary)["S231:min"] =
                        double(&format!("{tightened:?}"));
                    variants.push(changed);
                }
            }
        }
        for variant in variants {
            let Some(engine) = try_load(&to_bytes(&variant)) else {
                refused += 1;
                continue;
            };
            if engine.content_id().unwrap() == base_id {
                unmoved += 1;
                continue;
            }
            assert_ne!(engine.topology(), base_topology, "{name}");
            moved += 1;
        }
    }
    (moved, unmoved, refused)
}

#[test]
fn every_input_label_and_bound_edit_that_moves_the_id_moves_the_topology() {
    assert_eq!(label_and_bound_edits("S231:hasInput"), (252, 0, 0));
}

#[test]
fn every_output_label_edit_that_moves_the_id_moves_the_topology() {
    // The four refusals are unit edits the load gate rejects against the driver's own unit.
    assert_eq!(label_and_bound_edits("S231:hasOutput"), (197, 0, 4));
}
