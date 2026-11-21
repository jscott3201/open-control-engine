//! The boundary facts on a topology snapshot (issue #701): the root id, the executable boundary
//! inputs with their type, labels and effective acceptance domain, and each declared output's
//! type and labels.
//!
//! The property is that a topology holds every boundary fact the canonical content id encodes,
//! so two loaded documents with equal topologies have equal ids. It is held here from outside,
//! in its contrapositive: every boundary edit that moves the id moves the topology, one named
//! edit at a time on a small document. `topology_boundary_corpus.rs` holds it over the
//! boundaries of the 47 corpus documents, and `src/content_id/topology_agreement_tests.rs`
//! holds the same facts against the id's own records, field by field.

// Shared helper, declared once for this binary in its root `facade.rs`.
use crate::content_id_corpus as corpus;

use corpus::*;
use oce_api::{DeclaredInput, DeclaredOutput, DomainKey, Engine, Topology, Value, ValueType};
use serde_json::{Value as Json, json};

/// The root of the small document: one block `gai` read by input `u` and driving output `y`,
/// and input `a` wired straight to output `b` (a pass-through), which authors the unit `K`.
const ROOT: &str = "http://example.org#ex.pass";

fn small() -> Json {
    let path = format!(
        "{}/tests/fixtures/content_id/pass_through.jsonld",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn id(local: &str) -> String {
    format!("{ROOT}.{local}")
}

/// The small document with `key` set to `value` on the node `local`.
fn with(document: &Json, local: &str, key: &str, value: Json) -> Json {
    let mut changed = document.clone();
    node_mut(&mut changed, &id(local))[key] = value;
    changed
}

fn double(value: &str) -> Json {
    json!({ "@value": value, "@type": "http://www.w3.org/2001/XMLSchema#double" })
}

fn loaded(document: &Json) -> Engine {
    load(&to_bytes(document))
}

fn input<'a>(topology: &'a Topology, local: &str) -> &'a DeclaredInput {
    let path = id(local);
    topology
        .boundary_inputs
        .iter()
        .find(|input| input.path == path)
        .unwrap_or_else(|| panic!("input {local}"))
}

fn output<'a>(topology: &'a Topology, local: &str) -> &'a DeclaredOutput {
    let path = id(local);
    topology
        .boundary_outputs
        .iter()
        .find(|output| output.path == path)
        .unwrap_or_else(|| panic!("output {local}"))
}

fn input_mut<'a>(topology: &'a mut Topology, local: &str) -> &'a mut DeclaredInput {
    let path = id(local);
    topology
        .boundary_inputs
        .iter_mut()
        .find(|input| input.path == path)
        .unwrap()
}

fn output_mut<'a>(topology: &'a mut Topology, local: &str) -> &'a mut DeclaredOutput {
    let path = id(local);
    topology
        .boundary_outputs
        .iter_mut()
        .find(|output| output.path == path)
        .unwrap()
}

/// Two documents that differ in one boundary fact: their ids differ, their topologies differ,
/// and putting that one fact back on a copy of the second topology gives the first, so nothing
/// else moved.
fn one_fact_moved(base: &Json, head: &Json, restore: impl FnOnce(&mut Topology, &Topology)) {
    let (base, head) = (loaded(base), loaded(head));
    assert_ne!(
        base.content_id().unwrap(),
        head.content_id().unwrap(),
        "the edit is one the content id sees"
    );
    let (old, new) = (base.topology(), head.topology());
    assert_ne!(old, new, "an edit that moves the id moves the topology");
    let mut restored = new.clone();
    restore(&mut restored, &old);
    assert_eq!(restored, old, "only the edited fact differs");
}

#[test]
fn an_input_unit_change_moves_the_topology() {
    let base = with(&small(), "u", "S231:unit", json!("1"));
    let head = with(&small(), "u", "S231:unit", json!("Pa"));
    assert_eq!(
        input(&loaded(&base).topology(), "u").unit.as_deref(),
        Some("1")
    );
    assert_eq!(
        input(&loaded(&head).topology(), "u").unit.as_deref(),
        Some("Pa")
    );
    one_fact_moved(&base, &head, |topology, old| {
        input_mut(topology, "u").unit = input(old, "u").unit.clone();
    });
}

#[test]
fn an_input_quantity_change_moves_the_topology() {
    let base = with(&small(), "u", "S231:quantity", json!("Pressure"));
    let head = with(&small(), "u", "S231:quantity", json!("Density"));
    assert_eq!(
        input(&loaded(&head).topology(), "u").quantity.as_deref(),
        Some("Density")
    );
    one_fact_moved(&base, &head, |topology, old| {
        input_mut(topology, "u").quantity = input(old, "u").quantity.clone();
    });
}

#[test]
fn an_input_display_unit_change_moves_the_topology() {
    let base = with(&small(), "u", "S231:displayUnit", json!("degC"));
    let head = with(&small(), "u", "S231:displayUnit", json!("degF"));
    assert_eq!(
        input(&loaded(&head).topology(), "u")
            .display_unit
            .as_deref(),
        Some("degF")
    );
    one_fact_moved(&base, &head, |topology, old| {
        input_mut(topology, "u").display_unit = input(old, "u").display_unit.clone();
    });
}

#[test]
fn the_unit_of_an_input_that_feeds_only_a_pass_through_moves_the_topology() {
    let base = with(&small(), "a", "S231:unit", json!("K"));
    let head = with(&small(), "a", "S231:unit", json!("Pa"));
    let topology = loaded(&head).topology();
    assert_eq!(input(&topology, "a").unit.as_deref(), Some("Pa"));
    // The input has no block to feed, and is a boundary input all the same.
    assert!(
        topology
            .blocks
            .iter()
            .all(|block| !block.inputs.contains(&id("a")))
    );
    one_fact_moved(&base, &head, |topology, old| {
        input_mut(topology, "a").unit = input(old, "a").unit.clone();
    });
}

#[test]
fn an_input_that_gains_a_lower_bound_moves_the_topology() {
    let base = small();
    let head = with(&base, "u", "S231:min", double("0.0"));
    assert!(input(&loaded(&base).topology(), "u").min.is_none());
    let bound = input(&loaded(&head).topology(), "u").min.clone();
    assert!(matches!(bound, Some(Value::Real(x)) if x.to_bits() == 0.0_f64.to_bits()));
    one_fact_moved(&base, &head, |topology, old| {
        input_mut(topology, "u").min = input(old, "u").min.clone();
    });
}

#[test]
fn an_input_that_gains_an_upper_bound_moves_the_topology() {
    let base = small();
    let head = with(&base, "u", "S231:max", double("40.5"));
    let bound = input(&loaded(&head).topology(), "u").max.clone();
    assert!(matches!(bound, Some(Value::Real(x)) if x.to_bits() == 40.5_f64.to_bits()));
    one_fact_moved(&base, &head, |topology, old| {
        input_mut(topology, "u").max = input(old, "u").max.clone();
    });
}

#[test]
fn a_bound_of_negative_zero_is_not_a_bound_of_zero() {
    let base = with(&small(), "u", "S231:min", double("0.0"));
    let head = with(&small(), "u", "S231:min", double("-0.0"));
    let bound = input(&loaded(&head).topology(), "u").min.clone();
    assert!(matches!(bound, Some(Value::Real(x)) if x.to_bits() == (-0.0_f64).to_bits()));
    // Equal under `==`, different bits: the id tells them apart, so equality here must too.
    one_fact_moved(&base, &head, |topology, old| {
        input_mut(topology, "u").min = input(old, "u").min.clone();
    });
}

#[test]
fn a_pass_through_output_unit_change_moves_the_topology() {
    let base = small();
    let head = with(&base, "b", "S231:unit", json!("degC"));
    assert_eq!(
        output(&loaded(&base).topology(), "b").unit.as_deref(),
        Some("K")
    );
    assert_eq!(
        output(&loaded(&head).topology(), "b").unit.as_deref(),
        Some("degC")
    );
    one_fact_moved(&base, &head, |topology, old| {
        output_mut(topology, "b").unit = output(old, "b").unit.clone();
    });
}

#[test]
fn every_label_of_an_output_a_block_drives_moves_the_topology() {
    // `y` is an elided declaration: `gai.y` drives it and no pass-through is involved.
    let base = small();
    let topology = loaded(&base).topology();
    assert_ne!(
        output(&topology, "y").path,
        output(&topology, "y").driver_path
    );
    assert_eq!(output(&topology, "y").unit, None);

    let unit = with(&base, "y", "S231:unit", json!("K"));
    assert_eq!(
        output(&loaded(&unit).topology(), "y").unit.as_deref(),
        Some("K")
    );
    one_fact_moved(&base, &unit, |topology, old| {
        output_mut(topology, "y").unit = output(old, "y").unit.clone();
    });

    let quantity = with(
        &base,
        "y",
        "S231:quantity",
        json!("ThermodynamicTemperature"),
    );
    assert_eq!(
        output(&loaded(&quantity).topology(), "y")
            .quantity
            .as_deref(),
        Some("ThermodynamicTemperature")
    );
    one_fact_moved(&base, &quantity, |topology, old| {
        output_mut(topology, "y").quantity = output(old, "y").quantity.clone();
    });

    let display = with(&base, "y", "S231:displayUnit", json!("degC"));
    assert_eq!(
        output(&loaded(&display).topology(), "y")
            .display_unit
            .as_deref(),
        Some("degC")
    );
    one_fact_moved(&base, &display, |topology, old| {
        output_mut(topology, "y").display_unit = output(old, "y").display_unit.clone();
    });
}

#[test]
fn a_boundary_type_change_moves_the_topology() {
    // The pass-through pair retyped from Real to Integer: `b`'s unit goes with its type.
    let base = small();
    let mut head = base.clone();
    for (local, kind) in [("a", "S231:IntegerInput"), ("b", "S231:IntegerOutput")] {
        let node = node_mut(&mut head, &id(local));
        node["@type"] = json!(kind);
        node["S231:isOfDataType"] = json!({ "@id": "S231:Integer" });
        node.as_object_mut().unwrap().remove("S231:unit");
    }
    let (old, new) = (loaded(&base).topology(), loaded(&head).topology());
    assert_eq!(input(&old, "a").value_type, ValueType::Real);
    assert_eq!(input(&new, "a").value_type, ValueType::Integer);
    assert_eq!(output(&old, "b").value_type, ValueType::Real);
    assert_eq!(output(&new, "b").value_type, ValueType::Integer);
    // An Integer input's domain is the CDL Integer range, where a Real's is unbounded.
    assert!(input(&old, "a").min.is_none());
    assert!(matches!(input(&new, "a").min, Some(Value::Integer(_))));
    one_fact_moved(&base, &head, |topology, old| {
        *input_mut(topology, "a") = input(old, "a").clone();
        *output_mut(topology, "b") = output(old, "b").clone();
    });
}

#[test]
fn an_inputs_bounds_are_its_effective_domain_and_not_its_declarations() {
    // The declaration admits 0 and up; the one connector it feeds admits 5 and up.
    let declared = with(&small(), "u", "S231:min", double("0.0"));
    let narrowed = with(&declared, "gai.u", "S231:min", double("5.0"));
    let engine = loaded(&narrowed);
    let topology = engine.topology();
    let bound = input(&topology, "u").min.clone();
    assert!(matches!(bound, Some(Value::Real(x)) if x.to_bits() == 5.0_f64.to_bits()));
    let definitions = engine.input_definitions().unwrap();
    let definition = definitions.iter().find(|d| d.path == id("u")).unwrap();
    assert!(definition.min.as_ref().unwrap().bit_eq(&bound.unwrap()));
    // The internal bound moved the effective domain, so it moved the id and the topology.
    one_fact_moved(&declared, &narrowed, |topology, old| {
        input_mut(topology, "u").min = input(old, "u").min.clone();
    });
}

#[test]
fn an_inputs_labels_are_its_declarations_and_not_a_connectors_it_feeds() {
    // `displayUnit` is presentation: unification never copies it from a connector to the
    // declaration, and an internal connector's attributes are outside the id.
    let base = small();
    let head = with(&base, "gai.u", "S231:displayUnit", json!("degF"));
    let (old, new) = (loaded(&base), loaded(&head));
    assert_eq!(old.content_id().unwrap(), new.content_id().unwrap());
    assert_eq!(input(&new.topology(), "u").display_unit, None);
    assert_eq!(old.topology(), new.topology());
}

#[test]
fn topology_equality_reads_the_root_id_and_every_boundary_field() {
    let document = with(&small(), "u", "S231:min", double("1.0"));
    let reference = loaded(&document).topology();
    assert_eq!(reference, loaded(&document).topology());
    type Edit = (&'static str, fn(&mut Topology));
    let edits: [Edit; 12] = [
        ("root id", |t| t.model_id = DomainKey::new("urn:other:root")),
        ("input path", |t| t.boundary_inputs[0].path.push('x')),
        ("input type", |t| {
            t.boundary_inputs[0].value_type = ValueType::Boolean;
        }),
        ("input quantity", |t| {
            t.boundary_inputs[0].quantity = Some("Q".into());
        }),
        ("input unit", |t| {
            t.boundary_inputs[0].unit = Some("U".into())
        }),
        ("input display unit", |t| {
            t.boundary_inputs[0].display_unit = Some("D".into());
        }),
        ("input min", |t| {
            t.boundary_inputs[1].min = Some(Value::Real(2.0));
        }),
        ("input max", |t| {
            t.boundary_inputs[1].max = Some(Value::Real(2.0));
        }),
        ("input removed", |t| {
            t.boundary_inputs.pop();
        }),
        ("output type", |t| {
            t.boundary_outputs[0].value_type = ValueType::Boolean;
        }),
        ("output quantity", |t| {
            t.boundary_outputs[0].quantity = Some("Q".into());
        }),
        ("output display unit", |t| {
            t.boundary_outputs[0].display_unit = Some("D".into());
        }),
    ];
    for (what, edit) in edits {
        let mut edited = reference.clone();
        edit(&mut edited);
        assert_ne!(edited, reference, "{what}");
    }
    let mut unit = reference.clone();
    unit.boundary_outputs[1].unit = Some("degC".into());
    assert_ne!(unit, reference, "output unit");
    // A bound compares by bits: one NaN payload equals itself and no other.
    let nan = |payload: u64| Some(Value::Real(f64::from_bits(0x7ff8_0000_0000_0000 | payload)));
    let (mut first, mut second, mut third) = (reference.clone(), reference.clone(), reference);
    first.boundary_inputs[1].min = nan(1);
    second.boundary_inputs[1].min = nan(1);
    third.boundary_inputs[1].min = nan(2);
    assert_eq!(first, second);
    assert_ne!(first, third);
}

#[test]
fn an_engine_with_no_model_has_an_empty_topology_and_an_empty_root_id() {
    let topology = Engine::new().topology();
    assert_eq!(topology.model_id, DomainKey::default());
    assert_eq!(topology.model_id.as_str(), "");
    assert!(topology.boundary_inputs.is_empty());
    assert!(topology.boundary_outputs.is_empty());
    assert_eq!(topology, Engine::new().topology());
}

#[test]
fn a_shared_input_is_one_boundary_input_and_a_pass_through_output_drives_itself() {
    let name = "multizone_vav_return_fan_airflow_tracking";
    let engine = load(&fixture_bytes(name));
    let topology = engine.topology();
    let shared = format!("{NAMESPACE}g36.source.{name}.u1SupFan");
    let passed = format!("{NAMESPACE}g36.source.{name}.y1RetFan");
    let count = |paths: Vec<&String>| paths.into_iter().filter(|path| **path == shared).count();
    assert_eq!(count(topology.external_inputs.iter().collect()), 2);
    assert_eq!(
        count(topology.boundary_inputs.iter().map(|i| &i.path).collect()),
        1
    );
    let input = topology
        .boundary_inputs
        .iter()
        .find(|input| input.path == shared)
        .unwrap();
    assert_eq!(input.value_type, ValueType::Boolean);
    assert!(input.min.is_none() && input.max.is_none());
    let output = topology
        .boundary_outputs
        .iter()
        .find(|output| output.path == passed)
        .unwrap();
    assert_eq!(output.driver_path, passed);
    assert_eq!(output.value_type, ValueType::Boolean);
    assert_eq!(topology.pass_through.len(), 1);
    assert_eq!(topology.pass_through[0].input, shared);
    assert_eq!(topology.pass_through[0].output, passed);
}
