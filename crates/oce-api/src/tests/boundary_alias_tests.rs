//! Identity-level oracle for the declared boundary-output alias map.
//!
//! Value equality alone is vacuous here twice over: the alias reads the driver's own slot, so
//! bit-equality is a tautology for any implementation that stores a `ConnectorId`, and most
//! corpus declared outputs are value-indistinguishable under zero drive (112/130). This oracle
//! asserts the alias's RESOLVED TARGET instead: for every declared output in every fixture, the
//! `ConnectorId` the alias resolves to equals the `ConnectorId` of the driver path derived from
//! the raw document's `isConnectedTo` edges — never from engine output. A varied-drive value
//! check rides along with every output slot staged to a distinct value, the subset where values
//! discriminate fully.
//!
//! Mutation probe, predicted red stated first: repointing one alias at a wrong `ConnectorId`
//! (e.g. inserting the first inventory output instead of `boundary.source` in
//! `IoInventory::build_at_load`) reds THIS oracle on the identity assertion, while a
//! zero-drive value check alone would stay green for every value-indistinguishable pair.

use std::fs;

use oce_model::Dir;

use super::common::{Engine, Value, ValueType};

/// The G36 corpus, sorted; the same discovery every corpus suite uses.
fn corpus_fixtures() -> Vec<std::path::PathBuf> {
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
    assert_eq!(fixtures.len(), 47, "G36 corpus size moved");
    fixtures
}

/// `(declared IRI, document-derived driver IRI)` per root-declared boundary output, from a
/// plain `serde_json` walk over the raw bytes. Drivers follow `isConnectedTo` chains through
/// nested composites' boundary-output nodes to the surviving leaf output port — the flattening
/// contract — and pass-throughs report the declared IRI itself (the lowered connector carries
/// it).
fn document_declared_drivers(bytes: &[u8]) -> Vec<(String, String)> {
    let document: serde_json::Value = serde_json::from_slice(bytes).expect("fixture is JSON");
    let graph = document["@graph"].as_array().expect("@graph is an array");
    let refs = |node: &serde_json::Value, key: &str| -> Vec<String> {
        match &node[key] {
            serde_json::Value::Array(items) => items
                .iter()
                .map(|item| item["@id"].as_str().expect("reference has @id").to_owned())
                .collect(),
            serde_json::Value::Object(map) => {
                vec![map["@id"].as_str().expect("reference has @id").to_owned()]
            }
            _ => Vec::new(),
        }
    };
    let node_id = |node: &serde_json::Value| -> String {
        node["@id"].as_str().expect("node has @id").to_owned()
    };
    let contained: Vec<String> = graph
        .iter()
        .flat_map(|node| refs(node, "S231:containsBlock"))
        .collect();
    let root = graph
        .iter()
        .find(|node| {
            !refs(node, "S231:containsBlock").is_empty() && !contained.contains(&node_id(node))
        })
        .expect("exactly one root");
    let boundary_inputs = refs(root, "S231:hasInput");
    let leaf_outputs: Vec<String> = graph
        .iter()
        .filter(|node| refs(node, "S231:containsBlock").is_empty())
        .flat_map(|node| refs(node, "S231:hasOutput"))
        .collect();
    let nested_boundary_outputs: Vec<String> = graph
        .iter()
        .filter(|node| {
            node_id(node) != node_id(root) && !refs(node, "S231:containsBlock").is_empty()
        })
        .flat_map(|node| refs(node, "S231:hasOutput"))
        .collect();
    let partners_of = |iri: &str| -> Vec<String> {
        let mut partners = Vec::new();
        for node in graph {
            let id = node_id(node);
            let targets = refs(node, "S231:isConnectedTo");
            if id == iri {
                partners.extend(targets);
            } else if targets.iter().any(|t| t == iri) {
                partners.push(id);
            }
        }
        partners
    };

    refs(root, "S231:hasOutput")
        .into_iter()
        .map(|declared| {
            if partners_of(&declared)
                .iter()
                .any(|p| boundary_inputs.contains(p))
            {
                let driver = declared.clone();
                return (declared, driver);
            }
            let mut visited = vec![declared.clone()];
            let mut frontier = vec![declared.clone()];
            let mut terminals: Vec<String> = Vec::new();
            while let Some(current) = frontier.pop() {
                for partner in partners_of(&current) {
                    if visited.contains(&partner) {
                        continue;
                    }
                    visited.push(partner.clone());
                    if leaf_outputs.contains(&partner) {
                        terminals.push(partner);
                    } else if nested_boundary_outputs.contains(&partner) {
                        frontier.push(partner);
                    }
                }
            }
            assert_eq!(terminals.len(), 1, "{declared} has one leaf driver");
            (declared, terminals.remove(0))
        })
        .collect()
}

/// A distinct staged value per slot index, respecting the slot's type. Reals and integers are
/// fully discriminating; booleans alternate (the identity assertion, not this ride-along, is
/// what discriminates same-parity boolean slots).
fn distinct_value(value_type: ValueType, index: usize) -> Option<Value> {
    match value_type {
        ValueType::Real => Some(Value::Real(1000.0 + index as f64)),
        ValueType::Integer => Some(Value::Integer(1000 + index as i64)),
        ValueType::Boolean => Some(Value::Boolean(index.is_multiple_of(2))),
        ValueType::Enum(_) | ValueType::String => None,
    }
}

#[test]
fn every_declared_alias_resolves_to_the_document_derived_driver_connector() {
    let mut checked = 0usize;
    for fixture in corpus_fixtures() {
        let bytes = fs::read(&fixture).expect("read G36 fixture");
        let mut engine = Engine::in_memory();
        engine
            .load_cxf(&bytes)
            .unwrap_or_else(|error| panic!("{} loads: {error:?}", fixture.display()));

        // Stage a distinct value into every output slot BEFORE the identity checks, so the
        // value ride-along below discriminates every Real/Integer pair.
        let out_ids: Vec<_> = engine
            .model
            .connectors
            .iter()
            .filter(|c| c.dir == Dir::Out)
            .map(|c| (c.id, c.value_type))
            .collect();
        for (id, value_type) in &out_ids {
            if let Some(value) = distinct_value(*value_type, id.0 as usize) {
                engine.state.values[id.0 as usize] = value;
            }
        }

        for (declared, driver) in document_declared_drivers(&bytes) {
            // The expected target: the model connector carrying the document-derived driver
            // IRI as an output. The driver IRI comes from the raw document, never from any
            // resolver-derived vector.
            let expected = engine
                .model
                .connectors
                .iter()
                .find(|c| c.dir == Dir::Out && c.iri.as_deref() == Some(driver.as_str()))
                .unwrap_or_else(|| {
                    panic!(
                        "{}: document-derived driver {driver} must be a model output connector",
                        fixture.display()
                    )
                })
                .id;
            let resolved = engine
                .io
                .resolve_output(&declared)
                .unwrap_or_else(|| panic!("{}: alias {declared} must resolve", fixture.display()));
            assert_eq!(
                resolved,
                expected,
                "{}: alias {declared} resolved to connector {} but the document names driver \
                 {driver} (connector {})",
                fixture.display(),
                resolved.0,
                expected.0
            );

            // Varied-drive value ride-along on the staged distinct values.
            let via_alias = engine.get_output(&declared).expect("alias reads");
            let via_driver_slot = engine.state.values[expected.0 as usize].clone();
            assert!(
                via_alias.bit_eq(&via_driver_slot),
                "{}: alias {declared} must read its driver's slot bit-exactly",
                fixture.display()
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 132, "corpus-wide alias oracle count moved");
}
