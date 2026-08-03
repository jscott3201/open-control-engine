//! Ordered-vector oracle and behavior controls for `Topology.boundary_outputs`.
//!
//! The corpus membership guard (`authored_point_identity.rs`) is one-directional and would pass
//! trivially for declared names — they are document `@id`s by construction — and
//! `export_golden.rs` derives its emission expectation from the very vector guarded here, so
//! neither can see a dropped, repointed, or reordered entry. This suite compares
//! `Topology.boundary_outputs` as an **ordered Vec** against an expectation derived from the raw
//! document by a plain `serde_json` walk sharing no helper with `oce-cxf`, then pins the named
//! behavior controls: the shared-driver pair, the pass-through identity, the undriven declared
//! output, and the R18-4 order contract through a hand-authored discriminator fixture.

use std::fs;
use std::path::PathBuf;

use oce_api::oce_store::Store;
use oce_api::{DeclaredOutput, Engine, OcError, Topology};

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

fn load_engine(bytes: &[u8]) -> Engine<impl Store> {
    let mut engine = Engine::in_memory();
    engine.load_cxf(bytes).expect("fixture loads");
    engine
}

/// The `(path, driver_path)` expectation for a document, derived by a plain `serde_json` walk
/// sharing no helper with `oce-cxf`.
///
/// Elided entries carry their declared nodes' `@graph` positions (the R18-4 order source, NOT
/// the authored `hasOutput` array order); the driver is the instance output port joined to the
/// declared node by an `isConnectedTo` edge written on either endpoint. Pass-through entries
/// (`path == driver_path`) are appended after all elided entries, ordered by their boundary
/// input's then output's `@graph` position — mirroring the lowering's pair sort.
fn document_expectation(bytes: &[u8]) -> Vec<(String, String)> {
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
    let graph_pos = |iri: &str| -> usize {
        graph
            .iter()
            .position(|node| node_id(node) == iri)
            .unwrap_or(usize::MAX)
    };

    let contained: Vec<String> = graph
        .iter()
        .flat_map(|node| refs(node, "S231:containsBlock"))
        .collect();
    let roots: Vec<&serde_json::Value> = graph
        .iter()
        .filter(|node| {
            !refs(node, "S231:containsBlock").is_empty() && !contained.contains(&node_id(node))
        })
        .collect();
    assert_eq!(roots.len(), 1, "fixture must have exactly one root");
    let root = roots[0];
    let boundary_inputs = refs(root, "S231:hasInput");
    let declared = refs(root, "S231:hasOutput");

    // Leaf output ports: `hasOutput` of blocks without `containsBlock` — the connectors that
    // survive flattening. Nested composites' boundary outputs (`hasOutput` of contained
    // composite nodes) are followed through, mirroring the flattener's boundary elision.
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

    // Undirected edge adjacency: `isConnectedTo` may be written on either endpoint.
    let partners_of = |iri: &str| -> Vec<String> {
        let mut partners: Vec<String> = Vec::new();
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

    // The surviving leaf driver of a declared output: follow edges from the declared node,
    // passing through nested composites' boundary-output nodes, until a leaf output port is
    // reached. Exactly one such terminal must exist (multi-driver documents are refused).
    let leaf_driver = |declared_iri: &str| -> String {
        let mut visited: Vec<String> = vec![declared_iri.to_owned()];
        let mut frontier: Vec<String> = vec![declared_iri.to_owned()];
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
                // Any other partner kind (leaf inputs, other declared outputs) is a consumer
                // of the same signal, never its driver — ignored.
            }
        }
        assert_eq!(
            terminals.len(),
            1,
            "declared output {declared_iri} must reach exactly one leaf output driver, \
             got {terminals:?}"
        );
        terminals.remove(0)
    };

    let mut elided: Vec<(usize, String, String)> = Vec::new();
    let mut pass_through: Vec<(usize, usize, String)> = Vec::new();
    for iri in &declared {
        let partners = partners_of(iri);
        if let Some(input) = partners.iter().find(|p| boundary_inputs.contains(p)) {
            pass_through.push((graph_pos(input), graph_pos(iri), iri.clone()));
            continue;
        }
        elided.push((graph_pos(iri), iri.clone(), leaf_driver(iri)));
    }
    elided.sort_by_key(|(pos, ..)| *pos);
    pass_through.sort();

    let mut expected: Vec<(String, String)> = elided
        .into_iter()
        .map(|(_, path, driver)| (path, driver))
        .collect();
    expected.extend(
        pass_through
            .into_iter()
            .map(|(_, _, path)| (path.clone(), path)),
    );
    expected
}

fn vector_of(topology: &Topology) -> Vec<(String, String)> {
    topology
        .boundary_outputs
        .iter()
        .map(|output| (output.path.clone(), output.driver_path.clone()))
        .collect()
}

/// The ordered-Vec oracle (R18-3/R18-4). A mutation that reverses `materialize`'s `@graph`
/// iteration reds HERE (predicted red, confirmed by execution: every multi-output fixture's
/// vector arrives reversed). `export_golden`'s order test
/// (`emission_order must derive from the ModelGraph vectors alone`) stayed GREEN under that
/// same mutation — it derives its expected emission order from `ModelGraph.boundary_outputs`
/// itself, the vector under test — and its single-fixture byte golden, which did red, covers
/// one document and cannot see a driver repoint at all. This oracle covers all 46, in order,
/// against document-derived drivers.
#[test]
fn boundary_output_vector_matches_the_document_derived_expectation_in_order() {
    let mut total = 0usize;
    for fixture in corpus_fixtures() {
        let bytes = fs::read(&fixture).expect("read G36 fixture");
        let engine = load_engine(&bytes);
        let expected = document_expectation(&bytes);
        let actual = vector_of(&engine.topology());
        assert_eq!(
            actual,
            expected,
            "{}: Topology.boundary_outputs diverged from the document-derived ordered \
             expectation",
            fixture.display()
        );
        total += actual.len();
    }
    assert_eq!(
        total, 132,
        "corpus-wide declared boundary-output count moved"
    );
}

#[test]
fn shared_driver_serves_two_declared_outputs_with_bit_equal_values() {
    // cooling_only_active_air_flow: `actMin.y` drives BOTH declared outputs — the corpus's only
    // driver fan-out, which is why a declared output can never be a connector rename.
    let prefix = "http://example.org#g36.source.cooling_only_active_air_flow";
    let min_flow = format!("{prefix}.VActMin_flow");
    let hea_max_flow = format!("{prefix}.VActHeaMax_flow");
    let driver = format!("{prefix}.actMin.y");

    let fixture = corpus_fixtures()
        .into_iter()
        .find(|path| {
            path.file_stem()
                .is_some_and(|stem| stem == "cooling_only_active_air_flow")
        })
        .expect("cooling_only_active_air_flow fixture present");
    let bytes = fs::read(&fixture).expect("read fixture");
    let mut engine = load_engine(&bytes);
    engine.tick(0.0).expect("tick");

    let topology = engine.topology();
    let entries: Vec<&DeclaredOutput> = topology
        .boundary_outputs
        .iter()
        .filter(|output| output.driver_path == driver)
        .collect();
    assert_eq!(
        entries
            .iter()
            .map(|output| output.path.clone())
            .collect::<Vec<_>>(),
        vec![min_flow.clone(), hea_max_flow.clone()],
        "both declared outputs enumerate against the one shared driver, in graph order"
    );

    let pairs = engine
        .watch(&[min_flow.as_str(), hea_max_flow.as_str(), driver.as_str()])
        .expect("all three keys resolve");
    assert_eq!(pairs[0].0, min_flow, "watch echoes the caller's keys");
    assert!(
        pairs[0].1.bit_eq(&pairs[1].1) && pairs[1].1.bit_eq(&pairs[2].1),
        "two declared keys and their shared driver return bit-equal values: {pairs:?}"
    );
}

#[test]
fn pass_through_declared_output_enumerates_with_path_equal_to_driver_path() {
    // Both return-fan fixtures declare y1RetFan as a direct input→output pass-through: the
    // lowered connector itself carries the declared IRI, so the entry is self-driven.
    let mut seen = 0usize;
    for fixture in corpus_fixtures() {
        let bytes = fs::read(&fixture).expect("read G36 fixture");
        let engine = load_engine(&bytes);
        let vector = engine.topology().boundary_outputs;
        let pass_entries: Vec<&DeclaredOutput> = vector
            .iter()
            .filter(|output| output.path.ends_with(".y1RetFan"))
            .collect();
        for entry in &pass_entries {
            assert_eq!(
                entry.path,
                entry.driver_path,
                "{}: a pass-through declared output is its own driver",
                fixture.display()
            );
            seen += 1;
        }
        assert!(
            pass_entries.len() <= 1,
            "{}: y1RetFan appears once per vector",
            fixture.display()
        );
    }
    assert_eq!(
        seen, 2,
        "the corpus carries exactly two y1RetFan pass-throughs"
    );
}

#[test]
fn undriven_declared_output_is_absent_from_the_vector_and_unknown_to_lookup() {
    // R18-5: the warning is the undriven declaration's ONLY representation — no vector entry,
    // no alias. The fixture lives in the composite-contract corpus's warned category.
    let path = format!(
        "{}/../oce-cxf/tests/fixtures/composite_contract/warned/undriven_boundary_output.jsonld",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = fs::read(path).expect("read warned corpus fixture");
    let mut engine = Engine::in_memory();
    let report = engine.load_cxf(&bytes).expect("warned fixture loads");
    assert_eq!(report.warnings.len(), 1, "exactly one advisory");
    assert!(
        engine.topology().boundary_outputs.is_empty(),
        "an undriven declared output must not enumerate"
    );
    let declared = "http://example.org#M.y";
    assert!(
        matches!(engine.get_output(declared), Err(OcError::UnknownPoint(ref p)) if p == declared),
        "an undriven declared output must not resolve"
    );
    assert!(
        matches!(engine.watch(&[declared]), Err(OcError::UnknownPoint(ref p)) if p == declared),
        "an undriven declared output must not watch"
    );
}

/// Equality over `Topology` must see the new field. A mutation that deletes `boundary_outputs`
/// from `PartialEq for Topology` reds here (predicted red: the two snapshots below differ ONLY
/// in that field).
#[test]
fn topology_equality_discriminates_on_boundary_outputs_alone() {
    let fixture = corpus_fixtures()
        .into_iter()
        .find(|path| {
            path.file_stem()
                .is_some_and(|stem| stem == "ahu_economizer")
        })
        .expect("ahu_economizer fixture present");
    let bytes = fs::read(&fixture).expect("read fixture");
    let engine = load_engine(&bytes);
    let reference = engine.topology();
    assert_eq!(
        reference,
        engine.topology(),
        "identical snapshots compare equal"
    );

    let mut repointed = engine.topology();
    assert!(!repointed.boundary_outputs.is_empty(), "not vacuous");
    repointed.boundary_outputs[0].driver_path = "http://example.org#not.the.driver".to_owned();
    assert_ne!(
        reference, repointed,
        "a repointed boundary output must break Topology equality"
    );

    let mut truncated = engine.topology();
    truncated.boundary_outputs.pop();
    assert_ne!(
        reference, truncated,
        "a dropped boundary output must break Topology equality"
    );
}

// ---- R18-4 order-contract discriminator (D8) --------------------------------------------------
//
// 29/29 multi-output engine fixtures agree on `@graph` order vs `hasOutput` array order, so any
// corpus-based order pin is vacuous. These three hand-authored documents disagree on purpose.
// There is deliberately NO whole-`@graph`-permutation invariance golden: the model rustdoc
// disclaims that invariance — node order is load-bearing in the resolver.

/// A two-output document: `@graph` places node A before node B, while the authored `hasOutput`
/// array lists them as `outputs` (callers pass `[B, A]` or `[A, B]`), and `graph_order` controls
/// which declared node appears first in `@graph`.
fn discriminator_document(outputs: [&str; 2], graph_order: [&str; 2]) -> Vec<u8> {
    let root = "http://example.org#D8";
    let node = |name: &str| -> String {
        format!(
            r##"{{ "@id": "{root}.{name}", "@type": "S231:RealOutput",
                  "S231:isOfDataType": {{ "@id": "S231:Real" }} }}"##
        )
    };
    let constant = |block: &str, target: &str| -> String {
        format!(
            r##"{{ "@id": "{root}.{block}",
                  "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                  "S231:hasParameter": {{ "@id": "{root}.{block}.k" }},
                  "S231:hasOutput": {{ "@id": "{root}.{block}.y" }} }},
                {{ "@id": "{root}.{block}.k",
                  "S231:value": {{ "@value": "1.0", "@type": "http://www.w3.org/2001/XMLSchema#double" }} }},
                {{ "@id": "{root}.{block}.y", "@type": "S231:RealOutput",
                  "S231:isOfDataType": {{ "@id": "S231:Real" }},
                  "S231:isConnectedTo": {{ "@id": "{root}.{target}" }} }}"##
        )
    };
    format!(
        r##"{{
          "@context": {{ "S231": "http://data.ashrae.org/S231P#" }},
          "@graph": [
            {{ "@id": "{root}", "@type": "S231:Block",
              "S231:containsBlock": [
                {{ "@id": "{root}.c1" }},
                {{ "@id": "{root}.c2" }}
              ],
              "S231:hasOutput": [
                {{ "@id": "{root}.{first}" }},
                {{ "@id": "{root}.{second}" }}
              ] }},
            {node_first},
            {node_second},
            {c1},
            {c2}
          ]
        }}"##,
        first = outputs[0],
        second = outputs[1],
        node_first = node(graph_order[0]),
        node_second = node(graph_order[1]),
        c1 = constant("c1", "A"),
        c2 = constant("c2", "B"),
    )
    .into_bytes()
}

fn discriminator_vector(outputs: [&str; 2], graph_order: [&str; 2]) -> Vec<(String, String)> {
    let document = discriminator_document(outputs, graph_order);
    let engine = load_engine(&document);
    vector_of(&engine.topology())
}

#[test]
fn boundary_output_order_follows_graph_position_not_the_has_output_array() {
    let root = "http://example.org#D8";
    let entry = |name: &str, driver: &str| (format!("{root}.{name}"), format!("{root}.{driver}"));

    // Discriminating document: hasOutput authored [B, A], @graph places A before B. The vector
    // follows @graph — [A, B] — proving the array order is NOT the source.
    assert_eq!(
        discriminator_vector(["B", "A"], ["A", "B"]),
        vec![entry("A", "c1.y"), entry("B", "c2.y")],
        "the vector must follow @graph node position, not the authored hasOutput array"
    );

    // Control 1: permuting the declared nodes' @graph positions MUST move the vector.
    assert_eq!(
        discriminator_vector(["B", "A"], ["B", "A"]),
        vec![entry("B", "c2.y"), entry("A", "c1.y")],
        "permuting the declared nodes' @graph positions must move the vector"
    );

    // Control 2: permuting ONLY the hasOutput array MUST NOT move the vector.
    assert_eq!(
        discriminator_vector(["A", "B"], ["A", "B"]),
        discriminator_vector(["B", "A"], ["A", "B"]),
        "permuting only the hasOutput array must not move the vector"
    );
}
