//! Every G36 fixture document gives every block input exactly one driver (CDL §7.10).
//!
//! Issue #472 made the resolver refuse an input driven by both a block output and a root boundary
//! input, a shape it had accepted warning-free. This sweep is the evidence that the refusal moves
//! no shipped G36 fixture: each document still imports, and over its lowered graph every
//! frame-driven (`external_inputs`) connector has wire in-degree 0 and every other input has
//! exactly 1. The counts are taken from the imported `ModelGraph` directly, independently of the
//! resolver's own check.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::Dir;

/// The number of G36 fixture documents; a fixture added or removed must move this pin.
const G36_DOCUMENTS: usize = 47;

fn g36_documents() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/g36");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("read g36 fixture directory")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonld"))
        .collect();
    paths.sort();
    paths
}

#[test]
fn every_g36_input_has_exactly_one_driver() {
    let documents = g36_documents();
    assert_eq!(documents.len(), G36_DOCUMENTS);
    for path in documents {
        let bytes = std::fs::read(&path).expect("read fixture");
        let (graph, _) = import_cxf(&bytes, &ResolveOptions::default())
            .unwrap_or_else(|e| panic!("{} must import: {e:?}", path.display()));
        let mut wires: BTreeMap<u32, usize> = BTreeMap::new();
        for connection in &graph.connections {
            *wires.entry(connection.to.0).or_default() += 1;
        }
        let external: BTreeSet<u32> = graph.external_inputs.iter().map(|id| id.0).collect();
        for connector in graph.connectors.iter().filter(|c| c.dir == Dir::In) {
            let degree = wires.get(&connector.id.0).copied().unwrap_or(0);
            let expected = usize::from(!external.contains(&connector.id.0));
            assert_eq!(
                degree,
                expected,
                "{}: connector {} ({:?}) external={} has wire in-degree {degree}",
                path.display(),
                connector.id.0,
                connector.iri,
                external.contains(&connector.id.0)
            );
        }
    }
}
