//! The emitted values ground to exactly what the model says (an oracle cross-check through the
//! engine): every literal and reference of the every-element sequence reads back from the loaded
//! topology bit for bit — Reals by their bits (`-0.0` stays negative), Integer literals widened on
//! Real parameters, array elements under their flattened names, enumeration ordinals and strings.

use crate::support;

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
        (Literal::Member(member), Value::Enum { ordinal, .. }) => {
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
        // A library constant grounds to its Integer: the upstream `OperationModes` values,
        // copied from `Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes` as the oracle.
        (Literal::Member(member), Value::Integer(value)) => {
            let constants: &[(&str, i64)] = match member.type_name.as_str() {
                "OperationModes" => &[
                    ("occupied", 1),
                    ("coolDown", 2),
                    ("setUp", 3),
                    ("warmUp", 4),
                    ("setBack", 5),
                    ("freezeProtection", 6),
                    ("unoccupied", 7),
                ],
                other => panic!("no constant list for {other}"),
            };
            constants
                .iter()
                .any(|(name, v)| *name == member.member && v == value)
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
        checked, 22,
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

/// One row of the bit-exact table: a block of `class` whose parameter `param` is assigned
/// `value`, and the engine value it must ground to.
struct Row {
    class: &'static str,
    param: &'static str,
    value: ParamValue,
    grounds: Value,
}

fn row(class: &'static str, param: &'static str, value: ParamValue, grounds: Value) -> Row {
    Row {
        class,
        param,
        value,
        grounds,
    }
}

fn lit(literal: Literal) -> ParamValue {
    ParamValue::Literal(literal)
}

#[test]
fn the_bit_exact_table_grounds_through_the_engine() {
    use oce_author::{Block, Identifier, MemberRef, SequenceParameter, SignalType};
    const REAL: &str = "CDL.Reals.Sources.Constant";
    const INT: &str = "CDL.Integers.Sources.Constant";
    const BOOL: &str = "CDL.Logical.Sources.Constant";
    let mut rows = Vec::new();
    for v in [
        5e-324,
        -5e-324,
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::MIN,
        -0.0,
        0.0,
        0.1,
        1e-7,
        9_007_199_254_740_993.0,
    ] {
        rows.push(row(REAL, "k", lit(Literal::Real(v)), Value::Real(v)));
    }
    // An Integer literal on a Real parameter widens exactly up to 2^53.
    for v in [1i64 << 53, -(1i64 << 53), 3] {
        rows.push(row(
            REAL,
            "k",
            lit(Literal::Integer(v)),
            Value::Real(v as f64),
        ));
    }
    for v in [i64::MIN, i64::MAX, 9_007_199_254_740_993, -1, 0] {
        rows.push(row(INT, "k", lit(Literal::Integer(v)), Value::Integer(v)));
    }
    rows.push(row(
        BOOL,
        "k",
        lit(Literal::Boolean(true)),
        Value::Boolean(true),
    ));
    // References ground to the sequence parameter's value.
    rows.push(row(
        REAL,
        "k",
        ParamValue::Ref(support::id("tiny")),
        Value::Real(5e-324),
    ));
    rows.push(row(
        INT,
        "k",
        ParamValue::Ref(support::id("least")),
        Value::Integer(i64::MIN),
    ));
    rows.push(row(
        BOOL,
        "k",
        ParamValue::Ref(support::id("flag")),
        Value::Boolean(false),
    ));
    let mut s = oce_author::Sequence::new(support::id("table"));
    s.params.push(SequenceParameter::new(
        support::id("tiny"),
        SignalType::Real,
        Literal::Real(5e-324),
    ));
    s.params.push(SequenceParameter::new(
        support::id("least"),
        SignalType::Integer,
        Literal::Integer(i64::MIN),
    ));
    s.params.push(SequenceParameter::new(
        support::id("flag"),
        SignalType::Boolean,
        Literal::Boolean(false),
    ));
    for (i, r) in rows.iter().enumerate() {
        let mut block = Block::new(support::id(&format!("b{i}")), r.class);
        block.assignments.push(oce_author::Assignment::new(
            Identifier::unchecked(r.param),
            r.value.clone(),
        ));
        s.blocks.push(block);
    }
    // Enumeration members ground to their ordinals.
    let members = ["P", "PI", "PD", "PID"];
    for member in members {
        let mut pid = Block::new(support::id(&format!("pid{member}")), "CDL.Reals.PID");
        pid.assignments.push(oce_author::Assignment::new(
            support::id("controllerType"),
            lit(Literal::Member(MemberRef::parse(&format!(
                "SimpleController.{member}"
            )))),
        ));
        s.blocks.push(pid);
        for (input, port) in [("set", "u_s"), ("meas", "u_m")] {
            s.connections.push(oce_author::Connection::new(
                oce_author::Endpoint::parse(input),
                oce_author::Endpoint::parse(&format!("pid{member}.{port}")),
            ));
        }
    }
    for input in ["set", "meas"] {
        s.inputs.push(oce_author::Boundary::new(
            support::id(input),
            SignalType::Real,
        ));
    }
    assert_eq!(oce_author::validate(&s), vec![], "clean in both stages");
    let bytes = emit(&s, &EmitOptions::new()).expect("emit");
    let (engine, _) = support::cxf::load(&bytes).expect("loads");
    let topology = engine.topology();
    let grounded = |block: &str, param: &str| -> Value {
        topology
            .blocks
            .iter()
            .find(|b| b.instance_path.ends_with(&format!(".{block}")))
            .and_then(|b| b.params.iter().find(|(n, _)| n == param))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("{block}.{param}"))
    };
    for (i, r) in rows.iter().enumerate() {
        let got = grounded(&format!("b{i}"), r.param);
        assert!(
            got.bit_eq(&r.grounds),
            "row {i} ({:?}): engine {got:?}, want {:?}",
            r.value,
            r.grounds
        );
    }
    for (ordinal, member) in members.iter().enumerate() {
        match grounded(&format!("pid{member}"), "controllerType") {
            Value::Enum { ordinal: got, .. } => {
                assert_eq!(
                    usize::try_from(got).expect("small"),
                    ordinal + 1,
                    "{member}"
                );
            }
            other => panic!("{member}: {other:?}"),
        }
    }
}
