//! Numeric boundaries through the wire form and the validator: signed zero, the `i32` and `i64`
//! limits, integers past 2^53, subnormals and the extremes of binary64, and which literal kinds
//! each parameter kind accepts.

mod support;

use oce_author::{
    Block, Boundary, Connection, Endpoint, Literal, ParamValue, Sequence, SignalType,
    from_wire_str, to_wire_string, validate,
};

use support::{codes, id};

/// A sequence whose one block of `class` has `name = value`, driving output `y` of `kind`.
fn constant(class: &str, kind: SignalType, value: Literal) -> Sequence {
    let mut s = Sequence::new(id("numbers"));
    s.outputs.push(Boundary::new(id("y"), kind));
    let mut block = Block::new(id("c"), class);
    block.assignments.push(oce_author::Assignment::new(
        id("k"),
        ParamValue::Literal(value),
    ));
    s.blocks.push(block);
    s.connections.push(Connection::new(
        Endpoint::parse("c.y"),
        Endpoint::parse("y"),
    ));
    s
}

fn real_constant(value: Literal) -> Sequence {
    constant("CDL.Reals.Sources.Constant", SignalType::Real, value)
}

fn integer_constant(value: Literal) -> Sequence {
    constant("CDL.Integers.Sources.Constant", SignalType::Integer, value)
}

/// The value as written on the wire, and as read back.
fn wire_value(sequence: &Sequence) -> (String, Literal) {
    let text = to_wire_string(sequence);
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("\"value\""))
        .expect("a value line")
        .trim()
        .trim_start_matches("\"value\": ")
        .to_owned();
    let read = from_wire_str(&text).expect("reads back");
    assert_eq!(&read, sequence, "wire → model is exact for {line}");
    let ParamValue::Literal(literal) = read.blocks[0].assignments[0].value.clone() else {
        panic!("a literal");
    };
    (line, literal)
}

#[test]
fn signed_zero_survives_the_wire_and_the_validator() {
    let (text, literal) = wire_value(&real_constant(Literal::Real(-0.0)));
    assert_eq!(text, "-0.0");
    let Literal::Real(v) = literal else {
        panic!("real")
    };
    assert_eq!(v.to_bits(), (-0.0f64).to_bits());
    assert_ne!(
        Literal::Real(-0.0),
        Literal::Real(0.0),
        "the model compares bits"
    );
    assert_eq!(
        codes(&validate(&real_constant(Literal::Real(-0.0)))),
        vec![]
    );
    // An integer token `-0` is the Integer zero; Integer has no signed zero.
    let read = from_wire_str(
        &to_wire_string(&integer_constant(Literal::Integer(0)))
            .replace("\"value\": 0", "\"value\": -0"),
    )
    .expect("reads");
    assert_eq!(
        read.blocks[0].assignments[0].value,
        ParamValue::Literal(Literal::Integer(0))
    );
}

#[test]
fn integer_limits_round_trip_and_validate() {
    for value in [
        i64::from(i32::MAX),
        i64::from(i32::MIN),
        i64::from(i32::MAX) + 1,
        i64::from(i32::MIN) - 1,
        i64::MAX,
        i64::MIN,
    ] {
        let (text, literal) = wire_value(&integer_constant(Literal::Integer(value)));
        assert_eq!(text, value.to_string());
        assert_eq!(literal, Literal::Integer(value));
        assert_eq!(
            codes(&validate(&integer_constant(Literal::Integer(value)))),
            vec![],
            "{value}"
        );
    }
}

#[test]
fn integers_past_two_to_the_fifty_three_stay_exact() {
    let past = (1i64 << 53) + 1;
    let (text, literal) = wire_value(&integer_constant(Literal::Integer(past)));
    assert_eq!(text, "9007199254740993");
    assert_eq!(literal, Literal::Integer(past));
    // On a Real parameter it has no exact Real (it would load as 2^53), so stage 1 refuses it
    // (issue #464 review); 2^53 itself widens exactly.
    assert_eq!(
        codes(&validate(&real_constant(Literal::Integer(past)))),
        vec![("parameter-out-of-range", "c.k".to_owned())]
    );
    assert_eq!(
        codes(&validate(&real_constant(Literal::Integer(past - 1)))),
        vec![]
    );
    let (text, _) = wire_value(&real_constant(Literal::Real(9_007_199_254_740_992.0)));
    assert_eq!(text, "9007199254740992.0");
}

#[test]
fn reals_use_the_shortest_round_trip_spelling() {
    for (value, spelled) in [
        (1.0, "1.0"),
        (0.1, "0.1"),
        (1e-7, "1e-7"),
        (100.0, "100.0"),
        (1e16, "1e16"),
        (-2.5e-300, "-2.5e-300"),
        (f64::MAX, "1.7976931348623157e308"),
        (f64::MIN_POSITIVE, "2.2250738585072014e-308"),
        (5e-324, "5e-324"),
        (2.225_073_858_507_201e-308, "2.225073858507201e-308"),
    ] {
        let (text, literal) = wire_value(&real_constant(Literal::Real(value)));
        assert_eq!(text, spelled);
        let Literal::Real(back) = literal else {
            panic!("real")
        };
        assert_eq!(back.to_bits(), value.to_bits(), "{spelled}");
    }
}

#[test]
fn subnormals_are_finite_parameters() {
    for value in [5e-324_f64, -5e-324, 2.225_073_858_507_201e-308] {
        assert!(value != 0.0 && !value.is_normal());
        assert_eq!(
            codes(&validate(&real_constant(Literal::Real(value)))),
            vec![],
            "{value:e}"
        );
    }
}

#[test]
fn a_real_parameter_accepts_an_integer_literal_and_an_integer_parameter_refuses_a_real() {
    assert_eq!(
        codes(&validate(&real_constant(Literal::Integer(3)))),
        vec![]
    );
    assert_eq!(
        codes(&validate(&integer_constant(Literal::Integer(3)))),
        vec![]
    );
    for refused in [
        Literal::Real(3.0),
        Literal::Real(-0.0),
        Literal::Boolean(true),
        Literal::Text("3".to_owned()),
    ] {
        assert_eq!(
            codes(&validate(&integer_constant(refused.clone()))),
            vec![("parameter-kind-mismatch", "c.k".to_owned())],
            "{refused:?}"
        );
    }
}

#[test]
fn non_finite_reals_are_findings_in_the_model_and_null_on_the_wire() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let s = real_constant(Literal::Real(value));
        assert_eq!(
            codes(&validate(&s)),
            vec![("non-finite-parameter", "c.k".to_owned())]
        );
        let text = to_wire_string(&s);
        assert!(text.contains("\"value\": null"), "{text}");
        assert!(from_wire_str(&text).is_err(), "null is not a value");
    }
}

#[test]
fn sequence_parameter_integer_widening_reaches_a_real_reference() {
    let mut s = real_constant(Literal::Integer(0));
    s.params.push(oce_author::SequenceParameter::new(
        id("big"),
        SignalType::Real,
        Literal::Integer(i64::MAX),
    ));
    s.blocks[0].assignments[0].value = ParamValue::Ref(id("big"));
    // `i64::MAX` has no exact Real: refused on the Real sequence parameter, the reference withheld.
    assert_eq!(
        codes(&validate(&s)),
        vec![("parameter-out-of-range", "big".to_owned())]
    );
    s.params[0].value_type = oce_author::ParameterType::Integer;
    assert_eq!(
        codes(&validate(&s)),
        vec![("parameter-out-of-range", "c.k".to_owned())],
        "an Integer sequence parameter feeds a Real one only within 2^53"
    );
    s.params[0].value = Literal::Integer(1 << 53);
    assert_eq!(codes(&validate(&s)), vec![]);
    let mut int = integer_constant(Literal::Integer(0));
    int.params.push(oce_author::SequenceParameter::new(
        id("r"),
        SignalType::Real,
        Literal::Integer(2),
    ));
    int.blocks[0].assignments[0].value = ParamValue::Ref(id("r"));
    assert_eq!(
        codes(&validate(&int)),
        vec![("parameter-kind-mismatch", "c.k".to_owned())]
    );
}

#[test]
fn a_real_below_the_smallest_subnormal_underflows_to_zero() {
    let text = to_wire_string(&real_constant(Literal::Real(1.0)))
        .replace("\"value\": 1.0", "\"value\": 1e-400");
    let read = from_wire_str(&text).expect("an underflowing real is finite");
    assert_eq!(
        read.blocks[0].assignments[0].value,
        ParamValue::Literal(Literal::Real(0.0))
    );
    let negative = text.replace("1e-400", "-1e-400");
    let read = from_wire_str(&negative).expect("reads");
    assert_eq!(
        read.blocks[0].assignments[0].value,
        ParamValue::Literal(Literal::Real(-0.0))
    );
}

#[test]
fn an_integer_beyond_two_to_the_53_on_a_real_parameter_is_refused() {
    use oce_author::{MAX_EXACT_INTEGER, SequenceParameter};
    let at = |value: i64| {
        let mut s = support::econ_enable();
        s.blocks[1].assignments[0].value = ParamValue::Literal(Literal::Integer(value));
        // keep uLow <= uHigh: uHigh is a large Real literal
        s.blocks[1].assignments[1].value = ParamValue::Literal(Literal::Real(1e300));
        support::codes(&oce_author::validate_author_side(&s))
    };
    for ok in [MAX_EXACT_INTEGER, -MAX_EXACT_INTEGER, 0, -1] {
        assert_eq!(at(ok), vec![], "{ok}");
    }
    for refused in [
        MAX_EXACT_INTEGER + 1,
        -MAX_EXACT_INTEGER - 1,
        i64::MAX,
        i64::MIN,
    ] {
        assert_eq!(
            at(refused),
            vec![("parameter-out-of-range", "hys.uLow".to_owned())],
            "{refused}"
        );
    }
    // On a Real sequence parameter, reported there (the reference is withheld).
    let mut s = support::econ_enable();
    s.params[0].value = Literal::Integer(9_007_199_254_740_993);
    let findings = oce_author::validate_author_side(&s);
    assert_eq!(
        support::codes(&findings),
        vec![("parameter-out-of-range", "dTHys".to_owned())]
    );
    assert!(
        findings[0].detail.contains("2^53"),
        "{}",
        findings[0].detail
    );
    // Through an Integer sequence parameter into a Real block parameter: at the assignment.
    let mut s = support::econ_enable();
    s.params.push(SequenceParameter::new(
        support::id("big"),
        oce_author::SignalType::Integer,
        Literal::Integer(-9_007_199_254_740_993),
    ));
    s.blocks[1].assignments[0].value = ParamValue::Ref(support::id("big"));
    assert_eq!(
        support::codes(&oce_author::validate_author_side(&s)),
        vec![("parameter-out-of-range", "hys.uLow".to_owned())]
    );
}
