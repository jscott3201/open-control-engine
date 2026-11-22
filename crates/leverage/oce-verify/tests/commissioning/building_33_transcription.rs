//! The Building 33 transcription, checked against the model it was authored from (issue #755).
//! `tests/fixtures/commissioning/building33/cooling_coil_valve.oce` is **authored from the
//! vendored model, not vendored**; these tests hold the CXF it emits, and the engine that loads
//! it, to what two scripts read from the Modelica sources without looking at the authored file:
//!
//! - `declarations.txt` (`derive_declarations.py`, from `CoolingCoilValve.mo`): every parameter
//!   with its binding evaluated, every connector with its type, unit, quantity, display unit and
//!   bounds, every block with its class and modifiers, every `connect`. Checked against the
//!   emitted CXF's root parameters, blocks and connections, and against the engine's points and
//!   parameter table; its first line is the digest of the `.mo` it was read from, checked
//!   against the vendored bytes.
//! - `valve_oracle.txt` (`derive_valve.py`, from the `.mo` and the CDL block sources): the valve
//!   signal frame by frame over synthetic inputs that take every branch of the sequence.
//!   Checked against the engine bit for bit.

use std::collections::BTreeSet;

use oce_api::{Engine, Value, ValueType};

use super::building_33_day::example_file;
use super::support;

/// The emitted document's root, the prefix of every path in it.
pub(super) const ROOT: &str = "urn:oce:author:CoolingCoilValve.";

/// The vendored model, relative to this crate.
const MODEL: &str = "../../../third_party/modelica-buildings-cdl/Buildings/Utilities/Plotters/\
                     Examples/BaseClasses/CoolingCoilValve.mo";

/// The rows of `declarations.txt` of kind `kind`, each split on tabs, the kind dropped.
fn declarations(kind: &str) -> Vec<Vec<String>> {
    String::from_utf8(example_file("declarations.txt"))
        .expect("UTF-8")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split('\t').map(str::to_owned).collect::<Vec<_>>())
        .filter(|cells| cells[0] == kind)
        .map(|cells| cells[1..].to_vec())
        .collect()
}

/// The value of a `key=value` cell.
fn cell<'c>(cells: &'c [String], key: &str) -> &'c str {
    cells
        .iter()
        .find_map(|cell| cell.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("no `{key}=` cell in {cells:?}"))
}

fn fnv1a128(bytes: &[u8]) -> String {
    let mut hash: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    for byte in bytes {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    format!("{hash:032x}")
}

fn bits(hex: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(hex, 16).unwrap_or_else(|_| panic!("hex bits {hex}")))
}

/// The emitted CXF, parsed.
fn document() -> serde_json::Value {
    serde_json::from_slice(&example_file("cooling_coil_valve.cxf.json")).expect("JSON")
}

/// The emitted CXF's graph nodes.
fn nodes(document: &serde_json::Value) -> Vec<&serde_json::Value> {
    document["@graph"]
        .as_array()
        .expect("a graph")
        .iter()
        .collect()
}

/// A node's `@id` with the root prefix removed.
fn local(node: &serde_json::Value) -> String {
    let id = node["@id"].as_str().expect("an @id");
    id.strip_prefix(ROOT).unwrap_or(id).to_owned()
}

/// A CXF value as the text `declarations.txt` writes: hex bits for a Real, `true`/`false`, or
/// a reference's name.
fn value_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::String(name) => format!("ref={name}"),
        serde_json::Value::Object(typed) => {
            let text = typed["@value"].as_str().expect("a typed literal");
            format!("{:016x}", text.parse::<f64>().expect("a double").to_bits())
        }
        other => panic!("unexpected value {other}"),
    }
}

/// The engine, loaded with the emitted CXF.
pub(super) fn engine() -> Engine {
    let mut engine = Engine::new();
    engine
        .load_cxf(&example_file("cooling_coil_valve.cxf.json"))
        .expect("the transcription loads");
    engine
}

#[test]
fn the_declarations_were_read_from_the_vendored_model_as_it_is_in_the_tree() {
    let path = support::crate_dir().join(MODEL);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let recorded = declarations("source");
    assert_eq!(recorded, [vec![format!("fnv1a128:{}", fnv1a128(&bytes))]]);
}

#[test]
fn every_parameter_of_the_model_is_a_root_parameter_with_the_value_its_binding_evaluates_to() {
    let document = document();
    let nodes = nodes(&document);
    let root = nodes
        .iter()
        .find(|node| node["@type"] == "S231:Block")
        .expect("the root block");
    let names: Vec<String> = root["S231:hasParameter"]
        .as_array()
        .expect("root parameters")
        .iter()
        .map(local)
        .collect();
    let expected = declarations("param");
    assert_eq!(
        names,
        expected
            .iter()
            .map(|row| row[0].clone())
            .collect::<Vec<_>>(),
        "the model's fifteen parameters, in its order"
    );
    for row in &expected {
        let node = nodes
            .iter()
            .find(|node| local(node) == row[0])
            .unwrap_or_else(|| panic!("a node for {}", row[0]));
        let value = value_text(&node["S231:value"]);
        assert_eq!(value, cell(row, "value"), "{}", row[0]);
    }
}

/// One boundary connector as the engine declares it: path, direction, type, unit, quantity,
/// display unit, min and max.
struct Declared {
    path: String,
    input: bool,
    value_type: ValueType,
    unit: Option<String>,
    quantity: Option<String>,
    display_unit: Option<String>,
    min: Option<f64>,
    max: Option<f64>,
}

/// The engine's boundary: its inputs and outputs from the topology view (what bind reads the
/// unit and quantity from), each output's bounds from the point that drives it.
fn boundary(engine: &Engine) -> Vec<Declared> {
    let topology = engine.topology();
    let real = |value: &Option<Value>| match value {
        Some(Value::Real(x)) => Some(*x),
        None => None,
        other => panic!("a Real bound, not {other:?}"),
    };
    let mut declared: Vec<Declared> = topology
        .boundary_inputs
        .iter()
        .map(|input| Declared {
            path: input.path.clone(),
            input: true,
            value_type: input.value_type,
            unit: input.unit.clone(),
            quantity: input.quantity.clone(),
            display_unit: input.display_unit.clone(),
            min: real(&input.min),
            max: real(&input.max),
        })
        .collect();
    let points = engine.io().to_vec();
    for output in &topology.boundary_outputs {
        let driver = points
            .iter()
            .find(|point| point.path == output.driver_path)
            .expect("the output's driver is a point");
        declared.push(Declared {
            path: output.path.clone(),
            input: false,
            value_type: output.value_type,
            unit: output.unit.clone(),
            quantity: output.quantity.clone(),
            display_unit: output.display_unit.clone(),
            min: driver.min,
            max: driver.max,
        });
    }
    declared
}

#[test]
fn every_connector_has_the_model_s_direction_type_unit_quantity_display_unit_and_bounds() {
    let declared = boundary(&engine());
    let expected = declarations("port");
    assert_eq!(declared.len(), expected.len(), "six boundary connectors");
    for row in &expected {
        let (direction, name, kind) = (&row[0], &row[1], &row[2]);
        let port = declared
            .iter()
            .find(|port| port.path == format!("{ROOT}{name}"))
            .unwrap_or_else(|| panic!("a connector {name}"));
        assert_eq!(port.input, direction == "in", "{name}");
        let value_type = if kind == "Real" {
            ValueType::Real
        } else {
            ValueType::Boolean
        };
        assert_eq!(port.value_type, value_type, "{name}");
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        assert_eq!(text(&port.unit), cell(row, "unit"), "{name} unit");
        assert_eq!(
            text(&port.quantity),
            cell(row, "quantity"),
            "{name} quantity"
        );
        assert_eq!(
            text(&port.display_unit),
            cell(row, "displayUnit"),
            "{name} displayUnit"
        );
        let bound = |value: Option<f64>| value.map_or(String::new(), |x| format!("{x}"));
        assert_eq!(bound(port.min), cell(row, "min"), "{name} min");
        assert_eq!(bound(port.max), cell(row, "max"), "{name} max");
    }
}

#[test]
fn every_block_is_the_model_s_class_and_every_modifier_reaches_the_engine_s_parameter_table() {
    let document = document();
    let nodes = nodes(&document);
    let blocks: Vec<(String, String)> = nodes
        .iter()
        .filter_map(|node| {
            let kind = node["@type"].as_str()?;
            let class = kind.strip_prefix("http://example.org#")?;
            Some((local(node), class.to_owned()))
        })
        .collect();
    let expected: Vec<(String, String)> = declarations("block")
        .into_iter()
        .map(|row| (row[0].clone(), row[1].clone()))
        .collect();
    assert_eq!(blocks, expected, "fifteen blocks, in the model's order");
    // Each modifier as it is grounded in the engine, and as the CXF writes it: a reference
    // where the model writes a lone parameter name, a literal where it writes an expression.
    let engine = engine();
    for row in declarations("assign") {
        let (block, name) = (&row[0], &row[1]);
        let path = format!("{ROOT}{block}.{name}");
        let value = engine
            .get_param(&path)
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        match (&value, row[2].as_str()) {
            (Value::Boolean(b), "true" | "false") => assert_eq!(b.to_string(), row[2], "{path}"),
            (Value::Real(x), hex) => assert_eq!(x.to_bits(), bits(hex).to_bits(), "{path}"),
            other => panic!("{path}: {other:?}"),
        }
        let node = nodes
            .iter()
            .find(|node| local(node) == format!("{block}.{name}"))
            .unwrap_or_else(|| panic!("a node for {path}"));
        let written = value_text(&node["S231:value"]);
        let reference = row.iter().find_map(|cell| cell.strip_prefix("ref="));
        match reference {
            Some(name) if !name.is_empty() => assert_eq!(written, format!("ref={name}"), "{path}"),
            _ => assert!(
                !written.starts_with("ref="),
                "{path}: a literal, as the model"
            ),
        }
    }
}

#[test]
fn the_connections_are_the_model_s_twenty_one_connect_statements_and_no_other() {
    let document = document();
    let mut pairs = BTreeSet::new();
    for node in nodes(&document) {
        let from = local(node);
        let targets = match &node["S231:isConnectedTo"] {
            serde_json::Value::Array(targets) => targets.iter().collect::<Vec<_>>(),
            serde_json::Value::Null => Vec::new(),
            target => vec![target],
        };
        for target in targets {
            let mut ends = [from.clone(), local(target)];
            ends.sort();
            pairs.insert(format!("{}\t{}", ends[0], ends[1]));
        }
    }
    let expected: BTreeSet<String> = declarations("connect")
        .into_iter()
        .map(|row| row.join("\t"))
        .collect();
    assert_eq!(expected.len(), 21);
    assert_eq!(pairs, expected);
}

/// One frame of `valve_oracle.txt`: the inputs and the derived valve signal.
pub(super) struct OracleRow {
    pub t: f64,
    pub inputs: [f64; 4],
    pub fan_sta: bool,
    pub valve: f64,
    pub branch: String,
}

/// The oracle's frames.
pub(super) fn oracle_rows() -> Vec<OracleRow> {
    String::from_utf8(example_file("valve_oracle.txt"))
        .expect("UTF-8")
        .lines()
        .filter(|line| line.starts_with("row\t"))
        .map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            let number = |i: usize| cells[i].parse::<f64>().expect("a number");
            OracleRow {
                t: number(1),
                inputs: [number(2), number(3), number(4), number(5)],
                fan_sta: cells[6] == "true",
                valve: bits(cells[7]),
                branch: cells[9].to_owned(),
            }
        })
        .collect()
}

/// The valve signal the engine gives for each oracle frame, from the CXF `cxf`.
pub(super) fn valve_series(cxf: &[u8]) -> Vec<f64> {
    let mut engine = Engine::new();
    engine.load_cxf(cxf).expect("the CXF loads");
    let names =
        ["TSup", "TSupSet", "TOut", "uFanFee", "uFanSta", "yCooVal"].map(|n| format!("{ROOT}{n}"));
    oracle_rows()
        .iter()
        .map(|row| {
            let mut entries: Vec<(&str, Value)> = row
                .inputs
                .iter()
                .zip(&names)
                .map(|(x, name)| (name.as_str(), Value::Real(*x)))
                .collect();
            entries.push((names[4].as_str(), Value::Boolean(row.fan_sta)));
            let frame = engine.prepare_frame(row.t, &entries).expect("prepares");
            let done = engine.execute_frame(frame).expect("executes");
            match done.outputs().iter().find(|(path, _)| *path == names[5]) {
                Some((_, Value::Real(y))) => *y,
                other => panic!("t {}: {other:?}", row.t),
            }
        })
        .collect()
}

#[test]
fn the_valve_signal_matches_the_independent_derivation_bit_for_bit_on_every_branch() {
    let rows = oracle_rows();
    let engine = valve_series(&example_file("cooling_coil_valve.cxf.json"));
    for (row, y) in rows.iter().zip(&engine) {
        assert!(
            y.to_bits() == row.valve.to_bits(),
            "t {} ({}): engine {y:?}, derivation {:?}",
            row.t,
            row.branch,
            row.valve
        );
    }
    // Every branch the README lists is taken, and the comparison is not vacuous.
    let branches: BTreeSet<&str> = rows.iter().map(|row| row.branch.as_str()).collect();
    assert_eq!(branches.len(), 15, "{branches:?}");
    let kinds: BTreeSet<&str> = branches
        .iter()
        .map(|branch| branch.split(':').next().expect("a kind"))
        .collect();
    assert_eq!(kinds, BTreeSet::from(["limited", "off", "pi"]));
    assert_eq!(rows.len(), 64);
    assert!(engine.iter().any(|y| *y > 0.5) && engine.contains(&0.0));
}
