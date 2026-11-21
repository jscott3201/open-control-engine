//! The topology snapshot against the canonical graph (issue #701): every boundary fact the
//! content id encodes is on [`crate::Topology`], with the value the id hashes.
//!
//! For each corpus document the two are built from one loaded engine and compared record by
//! record: an input's name, type, labels, effective bounds and fan-out, and an output's name,
//! type, labels and driver. The topology spells an identity as the document's IRI and the id as
//! the name after the namespace, a port as its position in a block's list and the id as its
//! index, so the comparison also shows that the id's boundary records can be read off a
//! topology. The whole claim, that equal topologies imply equal ids, is held from outside by
//! `tests/topology_content_bytes.rs`, which rebuilds the id's bytes from a topology alone.

use std::path::PathBuf;

use oce_model::Value;

use super::graph::{
    CanonicalGraph, Driver, InputRecord, Labels, Name, OutputRecord, canonical_graph,
};
use crate::{Engine, Topology};

/// The name the id gives an IRI when no two IRIs of its kind collide: what follows the last
/// `#`, `/` or `:`. Written here a second time, so the test does not borrow the encoder's rule.
fn name(iri: &str) -> Name {
    let text = iri.rsplit(['#', '/', ':']).next().unwrap_or(iri);
    assert!(!text.is_empty(), "{iri}");
    Name {
        form: 0,
        text: text.to_owned(),
    }
}

fn same_bound(recorded: Option<&Value>, shown: Option<&Value>) -> bool {
    match (recorded, shown) {
        (Some(recorded), Some(shown)) => recorded.bit_eq(shown),
        (None, None) => true,
        _ => false,
    }
}

/// Compare the boundary records of `graph` with `topology`; returns `(inputs, outputs)`.
///
/// Every record is taken apart by an exhaustive pattern, with no `..`: a field added to the
/// id's boundary records, or a list added to the graph, does not compile here until this
/// comparison reads it from the view too. The block and connection lists are held, with the
/// rest of the bytes, by the rebuild in `tests/topology_content_bytes.rs`.
fn assert_agrees(what: &str, graph: &CanonicalGraph<'_>, topology: &Topology) -> (usize, usize) {
    let CanonicalGraph {
        inputs,
        outputs,
        blocks: _,
        connections: _,
    } = graph;

    // Inputs: the id sorts by name and the topology by path; under one namespace they agree.
    assert_eq!(inputs.len(), topology.boundary_inputs.len(), "{what}");
    for (record, input) in inputs.iter().zip(&topology.boundary_inputs) {
        let InputRecord {
            name: recorded_name,
            value_type,
            labels:
                Labels {
                    quantity,
                    unit,
                    display_unit,
                },
            min,
            max,
            targets: recorded_targets,
        } = record;
        let at = &input.path;
        assert_eq!(*recorded_name, name(at), "{what}");
        assert_eq!(*value_type, input.value_type, "{what}: {at}");
        assert_eq!(*quantity, input.quantity.as_deref(), "{what}: {at}");
        assert_eq!(*unit, input.unit.as_deref(), "{what}: {at}");
        assert_eq!(*display_unit, input.display_unit.as_deref(), "{what}: {at}");
        assert!(same_bound(*min, input.min.as_ref()), "{what}: {at} min");
        assert!(same_bound(*max, input.max.as_ref()), "{what}: {at} max");
        // The fan-out is every block input that carries the boundary's path, by position.
        let mut targets: Vec<(Name, u32)> = topology
            .blocks
            .iter()
            .flat_map(|block| {
                block
                    .inputs
                    .iter()
                    .enumerate()
                    .filter(|(_, path)| *path == at)
                    .map(|(port, _)| (name(&block.instance_path), port as u32))
            })
            .collect();
        targets.sort_unstable();
        assert_eq!(*recorded_targets, targets, "{what}: {at} targets");
    }

    let mut shown: Vec<_> = topology.boundary_outputs.iter().collect();
    shown.sort_unstable_by_key(|output| name(&output.path));
    assert_eq!(outputs.len(), shown.len(), "{what}");
    for (record, output) in outputs.iter().zip(shown) {
        let OutputRecord {
            name: recorded_name,
            value_type,
            labels:
                Labels {
                    quantity,
                    unit,
                    display_unit,
                },
            driver,
        } = record;
        let at = &output.path;
        assert_eq!(*recorded_name, name(at), "{what}");
        assert_eq!(*value_type, output.value_type, "{what}: {at}");
        assert_eq!(*quantity, output.quantity.as_deref(), "{what}: {at}");
        assert_eq!(*unit, output.unit.as_deref(), "{what}: {at}");
        assert_eq!(
            *display_unit,
            output.display_unit.as_deref(),
            "{what}: {at}"
        );
        match driver {
            Driver::Input(input) => {
                assert_eq!(output.driver_path, output.path, "{what}: {at}");
                let pair = topology
                    .pass_through
                    .iter()
                    .find(|pair| pair.output == *at)
                    .unwrap_or_else(|| panic!("{what}: {at} has a pass-through pair"));
                assert_eq!(*input, name(&pair.input), "{what}: {at}");
            }
            Driver::Block(block, port) => {
                assert_ne!(output.driver_path, output.path, "{what}: {at}");
                let driver = topology
                    .blocks
                    .iter()
                    .find(|candidate| name(&candidate.instance_path) == *block)
                    .unwrap_or_else(|| panic!("{what}: {at} is driven by a listed block"));
                assert_eq!(
                    driver.outputs.get(*port as usize),
                    Some(&output.driver_path),
                    "{what}: {at}"
                );
            }
        }
    }
    (inputs.len(), outputs.len())
}

fn documents() -> Vec<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut documents: Vec<PathBuf> =
        ["../oce-cxf/tests/fixtures/g36", "tests/fixtures/content_id"]
            .into_iter()
            .flat_map(|directory| std::fs::read_dir(manifest.join(directory)).unwrap())
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "jsonld")
            })
            .collect();
    documents.sort_unstable();
    documents
}

#[test]
fn every_boundary_record_of_the_id_is_on_the_topology_with_the_same_value() {
    let (mut documents_seen, mut inputs, mut outputs) = (0, 0, 0);
    for path in documents() {
        let mut engine = Engine::new();
        engine.load_cxf(&std::fs::read(&path).unwrap()).unwrap();
        let graph = canonical_graph(&engine.model, &engine.io).unwrap();
        let what = path.file_name().unwrap().to_string_lossy();
        let (seen_inputs, seen_outputs) = assert_agrees(&what, &graph, &engine.topology());
        documents_seen += 1;
        inputs += seen_inputs;
        outputs += seen_outputs;
    }
    // The 47 corpus documents and the four content-id examples; the counts pin the reach.
    assert_eq!((documents_seen, inputs, outputs), (51, 188, 137));
}

#[test]
fn a_label_the_topology_misreports_is_a_reported_disagreement() {
    // The comparison's own control: one changed label, bound or type must fail it.
    let mut engine = Engine::new();
    engine
        .load_cxf(include_bytes!(
            "../../tests/fixtures/content_id/offset_gain.jsonld"
        ))
        .unwrap();
    let graph = canonical_graph(&engine.model, &engine.io).unwrap();
    let reference = engine.topology();
    assert_eq!(assert_agrees("reference", &graph, &reference), (1, 1));
    let edits: [fn(&mut Topology); 6] = [
        |t| t.boundary_inputs[0].unit = Some("degC".into()),
        |t| t.boundary_inputs[0].quantity = None,
        |t| t.boundary_inputs[0].min = Some(Value::Real(0.0)),
        |t| t.boundary_inputs[0].value_type = oce_model::ValueType::Integer,
        |t| t.boundary_outputs[0].unit = Some("K".into()),
        |t| t.boundary_outputs[0].driver_path.push('x'),
    ];
    for (index, edit) in edits.into_iter().enumerate() {
        let mut edited = reference.clone();
        edit(&mut edited);
        let outcome = std::panic::catch_unwind(|| assert_agrees("edited", &graph, &edited));
        assert!(outcome.is_err(), "edit {index} went unnoticed");
    }
}
