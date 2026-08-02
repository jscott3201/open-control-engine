//! Owned topology snapshot contracts.

use std::collections::BTreeMap;

use oce_api::{Engine, Value};

const RETURN_FAN: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_airflow_tracking.jsonld"
);
const RETURN_FAN_GOLDEN: &str = include_str!(
    "../../oce-cxf/tests/fixtures/golden/g36_multizone_vav_return_fan_airflow_tracking.modelgraph.txt"
);
/// The authored boundary-input IRI shared by a child input (C4) and the pass-through input (C12):
/// one host point staged into two internal connectors, so it appears twice in `external_inputs`.
const SUPPLY_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.u1SupFan";
const RETURN_FAN_COMMAND: &str =
    "http://example.org#g36.source.multizone_vav_return_fan_airflow_tracking.y1RetFan";

#[derive(Debug)]
enum GoldenValue {
    Real(u64),
    Enum { class: u32, ordinal: u32 },
}

#[derive(Debug)]
struct GoldenBlock {
    id: u32,
    class_iri: String,
    instance_path: Option<String>,
    inputs: Vec<u32>,
    outputs: Vec<u32>,
    params: Vec<(String, GoldenValue)>,
}

#[derive(Debug)]
struct GoldenTopology {
    blocks: Vec<GoldenBlock>,
    connector_paths: BTreeMap<u32, String>,
    connections: Vec<(u32, u32)>,
    external_inputs: Vec<u32>,
}

fn bracketed_ids(value: &str) -> Vec<u32> {
    value
        .trim()
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap()
        .split(',')
        .filter_map(|value| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.trim_start_matches(['B', 'C']).parse().unwrap())
        })
        .collect()
}

fn parse_golden() -> GoldenTopology {
    let mut blocks = Vec::<GoldenBlock>::new();
    let mut connector_paths = BTreeMap::new();
    let mut connections = Vec::new();
    let mut external_inputs = Vec::new();
    for line in RETURN_FAN_GOLDEN.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix('B')
            .filter(|_| trimmed.contains(" decl="))
        {
            let (id, _) = rest.split_once(' ').unwrap();
            let class_iri = rest
                .split_whitespace()
                .find_map(|part| part.strip_prefix("class="))
                .unwrap()
                .to_string();
            let instance = rest.split_once("instance_iri=").unwrap().1;
            let instance_path = instance
                .strip_prefix("Some(\"")
                .and_then(|value| value.strip_suffix("\")"))
                .map(str::to_string);
            blocks.push(GoldenBlock {
                id: id.parse().unwrap(),
                class_iri,
                instance_path,
                inputs: Vec::new(),
                outputs: Vec::new(),
                params: Vec::new(),
            });
        } else if let Some(rest) = trimmed.strip_prefix("inputs=") {
            let (inputs, outputs) = rest.split_once(" outputs=").unwrap();
            let block = blocks.last_mut().unwrap();
            block.inputs = bracketed_ids(inputs);
            block.outputs = bracketed_ids(outputs);
        } else if let Some(rest) = trimmed.strip_prefix("param ") {
            let (name, value) = rest.split_once('=').unwrap();
            let value = if let Some(bits) = value
                .strip_prefix("Real(0x")
                .and_then(|value| value.strip_suffix(')'))
            {
                GoldenValue::Real(u64::from_str_radix(bits, 16).unwrap())
            } else {
                let fields = value
                    .strip_prefix("Enum(")
                    .and_then(|value| value.strip_suffix(')'))
                    .unwrap();
                let (class, ordinal) = fields.split_once(',').unwrap();
                GoldenValue::Enum {
                    class: class.strip_prefix("class=").unwrap().parse().unwrap(),
                    ordinal: ordinal.strip_prefix("ordinal=").unwrap().parse().unwrap(),
                }
            };
            blocks
                .last_mut()
                .unwrap()
                .params
                .push((name.to_string(), value));
        } else if let Some(rest) = trimmed
            .strip_prefix('C')
            .filter(|_| trimmed.contains(" block="))
        {
            let (id, _) = rest.split_once(' ').unwrap();
            let id: u32 = id.parse().unwrap();
            // Every connector in a CXF-resolved golden carries its authored identity IRI —
            // ingest rejects an `@id`-less connector node, so a missing IRI is a golden defect.
            let path = rest
                .split_once("iri=")
                .unwrap()
                .1
                .strip_prefix("Some(\"")
                .and_then(|value| value.strip_suffix("\")"))
                .unwrap_or_else(|| panic!("golden connector C{id} must carry an authored IRI"))
                .to_string();
            connector_paths.insert(id, path);
        } else if let Some((from, to)) = trimmed.split_once(" -> ") {
            connections.push((
                from.strip_prefix('C').unwrap().parse().unwrap(),
                to.strip_prefix('C').unwrap().parse().unwrap(),
            ));
        } else if let Some(ids) = trimmed.strip_prefix("external_inputs: ") {
            external_inputs = bracketed_ids(ids);
        }
    }
    GoldenTopology {
        blocks,
        connector_paths,
        connections,
        external_inputs,
    }
}

fn assert_param_matches(actual: &Value, expected: &GoldenValue) {
    match (actual, expected) {
        (Value::Real(actual), GoldenValue::Real(expected)) => {
            assert_eq!(actual.to_bits(), *expected)
        }
        (
            Value::Enum {
                class: actual_class,
                ordinal: actual_ordinal,
            },
            GoldenValue::Enum { class, ordinal },
        ) => {
            assert_eq!(actual_class.0, *class);
            assert_eq!(*actual_ordinal, *ordinal);
        }
        _ => panic!("parameter kind differs: actual={actual:?}, expected={expected:?}"),
    }
}

#[test]
fn topology_matches_checked_in_resolver_golden() {
    let golden = parse_golden();
    let mut engine = Engine::in_memory();
    engine
        .load_cxf(RETURN_FAN.as_bytes())
        .expect("fixture loads");
    let topology = engine.topology();

    let authored: Vec<_> = golden
        .blocks
        .iter()
        .filter(|block| !block.class_iri.starts_with("urn:oce:lowering#"))
        .collect();
    assert_eq!(topology.blocks.len(), authored.len());
    for (actual, expected) in topology.blocks.iter().zip(authored) {
        assert_eq!(
            actual.instance_path,
            expected.instance_path.as_deref().unwrap_or_else(|| panic!(
                "authored golden block B{} must have an instance IRI",
                expected.id
            ))
        );
        assert_eq!(actual.class_iri, expected.class_iri);
        assert_eq!(
            actual.inputs,
            expected
                .inputs
                .iter()
                .map(|id| golden.connector_paths[id].clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            actual.outputs,
            expected
                .outputs
                .iter()
                .map(|id| golden.connector_paths[id].clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(actual.params.len(), expected.params.len());
        for ((actual_name, actual_value), (expected_name, expected_value)) in
            actual.params.iter().zip(&expected.params)
        {
            assert_eq!(actual_name, expected_name);
            assert_param_matches(actual_value, expected_value);
        }
    }

    let expected_connections: Vec<_> = golden
        .connections
        .iter()
        .map(|(from, to)| {
            (
                golden.connector_paths[from].clone(),
                golden.connector_paths[to].clone(),
            )
        })
        .collect();
    let actual_connections: Vec<_> = topology
        .connections
        .iter()
        .map(|edge| (edge.from.clone(), edge.to.clone()))
        .collect();
    assert_eq!(actual_connections, expected_connections);

    let expected_external: Vec<_> = golden
        .external_inputs
        .iter()
        .map(|id| golden.connector_paths[id].clone())
        .collect();
    assert_eq!(topology.external_inputs, expected_external);
    assert_eq!(
        topology
            .external_inputs
            .iter()
            .filter(|path| path.as_str() == SUPPLY_FAN_STATUS)
            .count(),
        2,
        "a shared boundary IRI keeps one entry per consuming internal connector"
    );
    assert_eq!(topology.pass_through.len(), 1);
    assert_eq!(topology.pass_through[0].input, SUPPLY_FAN_STATUS);
    assert_eq!(topology.pass_through[0].output, RETURN_FAN_COMMAND);
    assert!(
        topology
            .blocks
            .iter()
            .all(|block| !block.class_iri.starts_with("urn:oce:lowering#"))
    );
}

#[test]
fn independent_loads_produce_equal_reconstructable_topologies() {
    let mut first = Engine::in_memory();
    first
        .load_cxf(RETURN_FAN.as_bytes())
        .expect("fixture loads");
    let mut second = Engine::in_memory();
    second
        .load_cxf(RETURN_FAN.as_bytes())
        .expect("fixture loads independently");
    let left = first.topology();
    let right = second.topology();
    assert_eq!(left, right);
    assert!(
        left.blocks
            .iter()
            .all(|block| !block.class_iri.starts_with("urn:oce:lowering#"))
    );
    assert!(!left.pass_through.is_empty());
    assert!(
        left.pass_through
            .iter()
            .any(|pair| pair.input == SUPPLY_FAN_STATUS && pair.output == RETURN_FAN_COMMAND)
    );
    assert!(left.external_inputs.contains(&SUPPLY_FAN_STATUS.to_owned()));
    assert!(
        left.blocks
            .iter()
            .any(|block| block.inputs.contains(&SUPPLY_FAN_STATUS.to_owned()))
    );
    for edge in &left.connections {
        assert!(
            left.blocks
                .iter()
                .any(|block| block.outputs.contains(&edge.from))
        );
        assert!(
            left.blocks
                .iter()
                .any(|block| block.inputs.contains(&edge.to))
        );
    }
}

#[test]
fn unloaded_engine_has_an_empty_non_panicking_topology() {
    let topology = Engine::in_memory().topology();
    assert!(topology.blocks.is_empty());
    assert!(topology.connections.is_empty());
    assert!(topology.external_inputs.is_empty());
    assert!(topology.pass_through.is_empty());
}
