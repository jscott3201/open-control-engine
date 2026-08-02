//! Corpus guard: a document-loaded point's identity is an authored `@id` from the document.
//!
//! JSON-LD `@graph` is an unordered set, so a positional connector-index path renumbers under
//! semantically identical re-serializations; a durable store keyed on one can graft a point's
//! samples onto another point's history with no error. CXF ingest rejects any connector node
//! without an `@id`, so every facade surface of a CXF-loaded model must carry authored
//! identities (as written — `@context` expansion is a known gap) and the positional spelling
//! survives only as the in-crate fallback for hand-built, IRI-less models.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use oce_api::oce_store::Store;
use oce_api::{Engine, PointDirection, PointInfo, Topology};

/// The positional fallback prefix, assembled with `concat!` so a repository-wide search for the
/// banned spelling finds genuine leak sites only, never this guard.
const POSITIONAL_PREFIX: &str = concat!("conn", "#");

/// Every `.jsonld` document in the G36 corpus, sorted for deterministic iteration order.
fn corpus_fixtures() -> Vec<PathBuf> {
    let fixture_dir = format!(
        "{}/../oce-cxf/tests/fixtures/g36",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut fixtures = fs::read_dir(fixture_dir)
        .expect("read G36 fixture corpus")
        .map(|entry| entry.expect("fixture directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonld")
        })
        .collect::<Vec<_>>();
    fixtures.sort();
    assert_eq!(fixtures.len(), 46, "G36 corpus size moved");
    fixtures
}

/// Load one corpus document into a fresh in-memory engine.
fn load_fixture(path: &PathBuf) -> (String, Engine<impl Store>) {
    let fixture_name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("UTF-8 fixture stem")
        .to_owned();
    let bytes = fs::read(path).expect("read G36 fixture");
    let mut engine = Engine::in_memory();
    engine
        .load_cxf(&bytes)
        .unwrap_or_else(|error| panic!("{fixture_name} loads: {error:?}"));
    (fixture_name, engine)
}

/// Every point path visible on the six facade surfaces: point_list, topology block ports,
/// connection endpoints, external_inputs, pass_through pairs, and `to_map` keys.
fn facade_surface_paths<S: Store>(engine: &Engine<S>) -> Vec<(&'static str, String)> {
    let mut paths = Vec::new();
    for point in engine.point_list(None).expect("point inventory") {
        paths.push(("point_list", point.path));
    }
    let topology: Topology = engine.topology();
    for block in &topology.blocks {
        for input in &block.inputs {
            paths.push(("block input", input.clone()));
        }
        for output in &block.outputs {
            paths.push(("block output", output.clone()));
        }
    }
    for connection in &topology.connections {
        paths.push(("connection from", connection.from.clone()));
        paths.push(("connection to", connection.to.clone()));
    }
    for input in &topology.external_inputs {
        paths.push(("external_input", input.clone()));
    }
    for pair in &topology.pass_through {
        paths.push(("pass_through input", pair.input.clone()));
        paths.push(("pass_through output", pair.output.clone()));
    }
    for (key, _) in engine.outputs().to_map() {
        paths.push(("to_map key", key));
    }
    paths
}

fn point_list_paths<S: Store>(engine: &Engine<S>, direction: PointDirection) -> BTreeSet<String> {
    engine
        .point_list(None)
        .expect("point inventory")
        .into_iter()
        .filter(|point: &PointInfo| point.direction == direction)
        .map(|point| point.path)
        .collect()
}

#[test]
fn no_facade_surface_of_a_loaded_document_emits_a_positional_connector_path() {
    let mut offending_fixtures = Vec::new();
    for fixture in corpus_fixtures() {
        let (fixture_name, engine) = load_fixture(&fixture);
        let positional: Vec<(&str, String)> = facade_surface_paths(&engine)
            .into_iter()
            .filter(|(_, path)| path.starts_with(POSITIONAL_PREFIX))
            .collect();
        if !positional.is_empty() {
            offending_fixtures.push((fixture_name, positional.len(), positional));
        }
    }
    assert!(
        offending_fixtures.is_empty(),
        "{}/46 fixtures leak positional {POSITIONAL_PREFIX}<N> paths onto a facade surface: {:?}",
        offending_fixtures.len(),
        offending_fixtures
            .iter()
            .map(|(name, count, sample)| {
                (
                    name.as_str(),
                    *count,
                    sample
                        .first()
                        .map(|(surface, path)| (*surface, path.as_str())),
                )
            })
            .collect::<Vec<_>>()
    );
}

#[test]
fn external_inputs_are_a_subset_of_point_list_input_paths() {
    for fixture in corpus_fixtures() {
        let (fixture_name, engine) = load_fixture(&fixture);
        let in_paths = point_list_paths(&engine, PointDirection::In);
        for input in &engine.topology().external_inputs {
            assert!(
                in_paths.contains(input),
                "{fixture_name}: external input {input} is missing from point_list In paths"
            );
        }
        // The reverse inclusion is false by construction: point_list enumerates every
        // non-String connector, external_inputs only the boundary inputs.
    }
}

#[test]
fn to_map_keys_and_point_list_output_paths_are_the_same_set() {
    for fixture in corpus_fixtures() {
        let (fixture_name, engine) = load_fixture(&fixture);
        let out_paths = point_list_paths(&engine, PointDirection::Out);
        let to_map_keys: BTreeSet<String> = engine
            .outputs()
            .to_map()
            .into_iter()
            .map(|(key, _)| key)
            .collect();
        // Only `point_list Out ⊆ to_map` holds by construction: to_map includes String-typed
        // output connectors while point_list excludes them (CDL §7.8 metadata-only). No G36
        // fixture declares a String output connector, so equality holds over this corpus.
        assert_eq!(
            to_map_keys, out_paths,
            "{fixture_name}: to_map keys diverge from point_list Out paths"
        );
    }
}

#[test]
fn external_inputs_preserve_shared_boundary_multiplicity() {
    let fixture = corpus_fixtures()
        .into_iter()
        .find(|path| {
            path.file_stem()
                .is_some_and(|stem| stem == "cooling_only_controller")
        })
        .expect("cooling_only_controller fixture present");
    let (_, engine) = load_fixture(&fixture);
    let external_inputs = engine.topology().external_inputs;
    let unique: BTreeSet<&String> = external_inputs.iter().collect();
    // A shared boundary IRI names one host point staged into several internal connectors; the
    // topology contract preserves that multiplicity rather than deduplicating paths.
    assert_eq!(
        external_inputs.len(),
        43,
        "external_inputs entry count moved"
    );
    assert_eq!(unique.len(), 14, "unique boundary path count moved");
}
