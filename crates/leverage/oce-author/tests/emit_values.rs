//! The emitted values ground to exactly what the model says (an oracle cross-check through the
//! engine): every literal and reference of the every-element sequence reads back from the loaded
//! topology bit for bit — Reals by their bits (`-0.0` stays negative), Integer literals widened on
//! Real parameters, array elements under their flattened names, enumeration ordinals and strings.

mod support;

use oce_api::Value;
use oce_author::{EmitOptions, Literal, ParamValue, Sequence, emit};

use support::cxf::load_clean;
use support::every_element;

fn expected(sequence: &Sequence, value: &ParamValue) -> Literal {
    match value {
        ParamValue::Literal(literal) => literal.clone(),
        ParamValue::Ref(name) => sequence
            .params
            .iter()
            .find(|p| p.name == *name)
            .map(|p| p.value.clone())
            .expect("referenced parameter"),
        _ => panic!("unknown value shape"),
    }
}

fn agrees(literal: &Literal, value: &Value) -> bool {
    match (literal, value) {
        (Literal::Real(a), Value::Real(b)) => a.to_bits() == b.to_bits(),
        (Literal::Integer(a), Value::Real(b)) => (*a as f64).to_bits() == b.to_bits(),
        (Literal::Integer(a), Value::Integer(b)) => a == b,
        (Literal::Boolean(a), Value::Boolean(b)) => a == b,
        (Literal::Text(a), Value::String(b)) => a.as_str() == &**b,
        (Literal::EnumMember(member), Value::Enum { ordinal, .. }) => {
            let members: &[&str] = match member.type_name.as_str() {
                "SimpleController" => &["P", "PI", "PD", "PID"],
                "Smoothness" => &["LinearSegments", "ConstantSegments"],
                other => panic!("no member list for {other}"),
            };
            members
                .iter()
                .position(|m| *m == member.member)
                .map(|p| p + 1)
                == usize::try_from(*ordinal).ok()
        }
        _ => false,
    }
}

#[test]
fn every_assignment_grounds_to_the_models_value() {
    let sequence = every_element();
    let bytes = emit(&sequence, &EmitOptions::new()).expect("emit");
    let engine = load_clean(&bytes);
    let topology = engine.topology();
    let mut checked = 0;
    for block in &sequence.blocks {
        let path = format!("urn:oce:author:every_element.{}", block.name);
        let loaded = topology
            .blocks
            .iter()
            .find(|b| b.instance_path == path)
            .unwrap_or_else(|| panic!("{path} loaded"));
        for assignment in &block.assignments {
            let want = expected(&sequence, &assignment.value);
            let got = loaded
                .params
                .iter()
                .find(|(name, _)| name == assignment.name.as_str())
                .map(|(_, v)| v)
                .unwrap_or_else(|| panic!("{path}.{} grounded", assignment.name));
            assert!(
                agrees(&want, got),
                "{}.{}: model {want:?}, engine {got:?}",
                block.name,
                assignment.name
            );
            checked += 1;
        }
    }
    assert_eq!(
        checked, 20,
        "every assignment of the every-element sequence"
    );
}

#[test]
fn a_partial_array_is_written_element_by_element() {
    // `sum3` with only `k_2` assigned: the array form would need `k_1` and `k_3`, which the
    // emitter never fills (decision 16), so `k_2` is written under its flattened name.
    let mut sequence = every_element();
    let sum3 = sequence
        .blocks
        .iter_mut()
        .find(|b| b.name.as_str() == "sum3")
        .expect("sum3");
    sum3.assignments
        .retain(|a| a.name.as_str() == "nin" || a.name.as_str() == "k_2");
    let text =
        String::from_utf8(emit(&sequence, &EmitOptions::new()).expect("emit")).expect("utf8");
    assert!(text.contains("\"urn:oce:author:every_element.sum3.k_2\""));
    assert!(!text.contains("\"urn:oce:author:every_element.sum3.k\""));
    let engine = load_clean(text.as_bytes());
    let topology = engine.topology();
    let sum3 = topology
        .blocks
        .iter()
        .find(|b| b.instance_path.ends_with(".sum3"))
        .expect("sum3");
    // The engine grounds exactly what was written; `k_1` and `k_3` take the class default
    // inside the block, so they are not in the authored table.
    let names: Vec<&str> = sum3.params.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["nin", "k_2"]);
    assert!(matches!(&sum3.params[1].1, Value::Real(v) if v.to_bits() == (-2.0f64).to_bits()));
}
