//! Boundary bounds (importer note, "Bounds on boundary declarations" and decisions 4 to 6; issue
//! #492): `min` and `max` survive both forms on both boundary kinds, emit as the corpus spells them
//! and load warning-free with the input's effective domain equal to the declared bounds bit for
//! bit; a bound on an input moves the canonical content id and one on an output does not; the
//! text spelling converts exactly as the loader does; and each refusal has its exact code, in the
//! wire reader, the text parser, the builder and the validator.

mod support;

use oce_api::Value;
use oce_author::{
    Block, Bound, Boundary, BuildError, Connection, EmitOptions, Endpoint, Sequence,
    SequenceBuilder, SignalType, WireError, emit, from_wire_str, parse_text, print_text,
    to_wire_string, validate, validate_author_side,
};

use support::cxf::{identity, load_clean};
use support::text::bounds;
use support::{codes, id};

/// A clean pass-through: Real input `u` → `CDL.Reals.Abs` → Real output `y`, Integer input `n`
/// → `CDL.Integers.Abs` → Integer output `m`, each boundary bounded as given.
fn bounded(
    u: (Option<Bound>, Option<Bound>),
    y: (Option<Bound>, Option<Bound>),
    n: (Option<Bound>, Option<Bound>),
    m: (Option<Bound>, Option<Bound>),
) -> Sequence {
    let mut s = Sequence::new(id("bounded"));
    for (list, name, kind, (min, max)) in [
        (0, "u", SignalType::Real, u),
        (0, "n", SignalType::Integer, n),
        (1, "y", SignalType::Real, y),
        (1, "m", SignalType::Integer, m),
    ] {
        let mut b = Boundary::new(id(name), kind);
        b.min = min;
        b.max = max;
        if list == 0 {
            s.inputs.push(b);
        } else {
            s.outputs.push(b);
        }
    }
    s.blocks.push(Block::new(id("abs"), "CDL.Reals.Abs"));
    s.blocks.push(Block::new(id("absInt"), "CDL.Integers.Abs"));
    for (from, to) in [
        ("u", "abs.u"),
        ("abs.y", "y"),
        ("n", "absInt.u"),
        ("absInt.y", "m"),
    ] {
        s.connections
            .push(Connection::new(Endpoint::parse(from), Endpoint::parse(to)));
    }
    s
}

const NONE: (Option<Bound>, Option<Bound>) = (None, None);

fn round_trips(s: &Sequence) {
    let wire = to_wire_string(s);
    let read = from_wire_str(&wire).unwrap_or_else(|e| panic!("{wire}\n{e}"));
    assert_eq!(&read, s);
    assert_eq!(to_wire_string(&read), wire);
    let text = print_text(s).expect("spellable");
    let parsed = parse_text(&text).unwrap_or_else(|e| panic!("{text}\n{e}"));
    assert_eq!(&parsed, s, "{text}");
    assert_eq!(print_text(&parsed).expect("spellable"), text);
}

#[test]
fn every_bound_spelling_round_trips_on_every_side_of_both_kinds() {
    let mut cases = 0;
    for bound in bounds() {
        let real = Some(bound);
        let integer = matches!(bound, Bound::Integer(_)).then_some(bound);
        for s in [
            bounded((real, None), NONE, NONE, NONE),
            bounded(NONE, (None, real), NONE, NONE),
            bounded(NONE, NONE, (integer, None), (None, integer)),
            bounded(
                (real, real),
                (real, real),
                (integer, integer),
                (integer, integer),
            ),
        ] {
            round_trips(&s);
            cases += 1;
        }
    }
    assert_eq!(cases, 4 * bounds().len());
}

#[test]
fn the_wire_and_text_spellings_are_the_ones_the_note_gives() {
    let s = bounded(
        (Some(Bound::Integer(0)), Some(Bound::Real(1.5))),
        (None, Some(Bound::Real(-0.0))),
        (Some(Bound::Integer(-3)), None),
        NONE,
    );
    let wire = to_wire_string(&s);
    assert!(
        wire.contains("\"min\": [\n        0\n      ],\n      \"max\": [\n        1.5\n      ],"),
        "{wire}"
    );
    assert!(
        wire.contains("\"min\": [],\n      \"max\": [\n        -0.0\n      ],"),
        "{wire}"
    );
    let text = print_text(&s).expect("spellable");
    assert!(text.contains("in u : Real @min 0 @max 1.5\n"), "{text}");
    assert!(text.contains("in n : Integer @min -3\n"), "{text}");
    assert!(text.contains("out y : Real @max -0.0\n"), "{text}");
    // `@min` and `@max` print right after `@displayUnit`, before the semantic tags.
    let mut tagged = s.clone();
    tagged.inputs[0].display_unit = Some("degC".to_owned());
    tagged.inputs[0].semantic.unit = Some("K".to_owned());
    tagged.inputs[0].semantic.brick = Some("brick:Sensor".to_owned());
    let text = print_text(&tagged).expect("spellable");
    assert!(
        text.contains(
            "in u : Real @unit K @displayUnit degC @min 0 @max 1.5 @brick brick:Sensor\n"
        ),
        "{text}"
    );
}

#[test]
fn emitted_bounds_load_warning_free_with_the_declared_domain_bit_for_bit() {
    for bound in bounds() {
        let Bound::Real(value) = bound else { continue };
        if value.is_sign_negative() && value != 0.0 {
            continue;
        }
        // `min` = -bound and `max` = bound for Reals; Integers on the Integer input.
        let low = Bound::Real(-value);
        let s = bounded(
            (Some(low), Some(bound)),
            (Some(low), Some(bound)),
            (
                Some(Bound::Integer(-7)),
                Some(Bound::Integer(i64::from(i32::MAX))),
            ),
            (Some(Bound::Integer(0)), None),
        );
        assert_eq!(validate(&s), vec![], "{bound:?}");
        let engine = load_clean(&emit(&s, &EmitOptions::new()).expect("emit"));
        let inputs = engine.input_definitions().expect("definitions");
        let u = inputs.iter().find(|d| d.path.ends_with(".u")).expect("u");
        assert!(
            matches!(u.min, Some(Value::Real(v)) if v.to_bits() == (-value).to_bits())
                && matches!(u.max, Some(Value::Real(v)) if v.to_bits() == value.to_bits()),
            "{bound:?}: {u:?}"
        );
        let n = inputs.iter().find(|d| d.path.ends_with(".n")).expect("n");
        assert!(
            matches!(n.min, Some(Value::Integer(-7)))
                && matches!(n.max, Some(Value::Integer(v)) if v == i64::from(i32::MAX)),
            "{n:?}"
        );
    }
}

#[test]
fn an_integer_bound_on_a_real_input_widens_exactly() {
    let limit = oce_author::MAX_EXACT_INTEGER;
    let s = bounded(
        (Some(Bound::Integer(-limit)), Some(Bound::Integer(limit))),
        NONE,
        NONE,
        NONE,
    );
    assert_eq!(validate(&s), vec![]);
    let engine = load_clean(&emit(&s, &EmitOptions::new()).expect("emit"));
    let inputs = engine.input_definitions().expect("definitions");
    let u = inputs.iter().find(|d| d.path.ends_with(".u")).expect("u");
    assert!(
        matches!(u.min, Some(Value::Real(v)) if v == -(limit as f64))
            && matches!(u.max, Some(Value::Real(v)) if v == limit as f64),
        "{u:?}"
    );
}

#[test]
fn the_text_spelling_of_a_bound_converts_exactly_as_the_loader_does() {
    for spelling in [
        "0.1",
        "0.30000000000000004",
        "1e-7",
        "5e-324",
        "2.2250738585072014e-308",
        "1.7976931348623157e308",
        "273.15",
        "9007199254740993.0",
        "1e-400",
        "123456789.123456789",
    ] {
        let source = format!(
            "sequence bounded\nextends none\nin u : Real @max {spelling}\nout y : Real\nblock abs = CDL.Reals.Abs\nu -> abs.u\nabs.y -> y\n"
        );
        let s = parse_text(&source).unwrap_or_else(|e| panic!("{spelling}: {e}"));
        let expected: f64 = spelling.parse().expect("f64");
        assert_eq!(s.inputs[0].max, Some(Bound::Real(expected)), "{spelling}");
        let engine = load_clean(&emit(&s, &EmitOptions::new()).expect("emit"));
        let inputs = engine.input_definitions().expect("definitions");
        assert!(
            matches!(inputs[0].max, Some(Value::Real(v)) if v.to_bits() == expected.to_bits()),
            "{spelling}: {:?}",
            inputs[0]
        );
    }
}

#[test]
fn the_emitter_writes_bounds_as_the_corpus_does() {
    let mut s = bounded(
        (Some(Bound::Integer(0)), Some(Bound::Real(1.0))),
        NONE,
        (Some(Bound::Integer(1)), Some(Bound::Integer(4))),
        NONE,
    );
    s.inputs[0].semantic.unit = Some("1".to_owned());
    s.inputs[0].display_unit = Some("1".to_owned());
    let text = String::from_utf8(emit(&s, &EmitOptions::new()).expect("emit")).expect("utf8");
    // A typed xsd:double on a Real boundary (an Integer widened), after `displayUnit`; a bare JSON
    // integer on an Integer boundary.
    assert!(
        text.contains(concat!(
            "      \"S231:displayUnit\": \"1\",\n",
            "      \"S231:min\": {\n        \"@value\": \"0.0\",\n        \"@type\": \"http://www.w3.org/2001/XMLSchema#double\"\n      },\n",
            "      \"S231:max\": {\n        \"@value\": \"1.0\",\n        \"@type\": \"http://www.w3.org/2001/XMLSchema#double\"\n      },\n",
        )),
        "{text}"
    );
    assert!(
        text.contains("      \"S231:min\": 1,\n      \"S231:max\": 4,\n"),
        "{text}"
    );
}

#[test]
fn an_input_bound_moves_the_content_id_and_an_output_bound_does_not() {
    let id_of = |s: &Sequence| identity(&load_clean(&emit(s, &EmitOptions::new()).expect("emit")));
    let plain = id_of(&bounded(NONE, NONE, NONE, NONE));
    let input = id_of(&bounded((Some(Bound::Real(0.0)), None), NONE, NONE, NONE));
    let input_max = id_of(&bounded((None, Some(Bound::Real(0.0))), NONE, NONE, NONE));
    let integer_input = id_of(&bounded(NONE, NONE, (None, Some(Bound::Integer(9))), NONE));
    let output = id_of(&bounded(
        NONE,
        (Some(Bound::Real(0.0)), Some(Bound::Real(1.0))),
        NONE,
        (Some(Bound::Integer(0)), None),
    ));
    assert_ne!(input, plain);
    assert_ne!(input_max, plain);
    assert_ne!(input_max, input, "min and max are distinct sides");
    assert_ne!(integer_input, plain);
    assert_eq!(output, plain, "output bounds are outside the content id");
    // Declaring exactly the CDL `i32` range an unbounded Integer input reports moves nothing.
    let i32_range = id_of(&bounded(
        NONE,
        NONE,
        (
            Some(Bound::Integer(i64::from(i32::MIN))),
            Some(Bound::Integer(i64::from(i32::MAX))),
        ),
        NONE,
    ));
    assert_eq!(i32_range, plain);
}

/// A finding as `(code, subject)`.
type Pair = (&'static str, String);

/// A refusal case: what it is, the sequence and the findings expected.
type Case = (&'static str, Sequence, Vec<Pair>);

fn pair(code: &'static str, subject: &str) -> Pair {
    (code, subject.to_owned())
}

#[test]
fn each_bound_refusal_has_its_code() {
    let real = |v: f64| Some(Bound::Real(v));
    let int = |v: i64| Some(Bound::Integer(v));
    let mut on_boolean = bounded(NONE, NONE, NONE, NONE);
    let mut flag = Boundary::new(id("flag"), SignalType::Boolean);
    flag.min = int(0);
    on_boolean.inputs.push(flag);
    let mut boolean_out = bounded(NONE, NONE, NONE, NONE);
    let mut done = Boundary::new(id("done"), SignalType::Boolean);
    done.max = real(1.0);
    boolean_out.outputs.push(done);
    let cases: Vec<Case> = vec![
        (
            "NaN",
            bounded((real(f64::NAN), None), NONE, NONE, NONE),
            vec![pair("non-finite-bound", "u")],
        ),
        (
            "infinity on an output",
            bounded(NONE, (None, real(f64::INFINITY)), NONE, NONE),
            vec![pair("non-finite-bound", "y")],
        ),
        (
            "negative infinity on an Integer input",
            bounded(NONE, NONE, (real(f64::NEG_INFINITY), None), NONE),
            vec![pair("non-finite-bound", "n")],
        ),
        (
            "a bound on a Boolean input",
            on_boolean,
            vec![pair("invalid-bound", "flag")],
        ),
        (
            "a bound on a Boolean output",
            boolean_out,
            vec![pair("invalid-bound", "done")],
        ),
        (
            "a Real on an Integer input",
            bounded(NONE, NONE, (real(1.0), None), NONE),
            vec![pair("invalid-bound", "n")],
        ),
        (
            "a Real on an Integer output",
            bounded(NONE, NONE, NONE, (None, real(1.5))),
            vec![pair("invalid-bound", "m")],
        ),
        (
            "an inexact Integer on a Real input",
            bounded(
                (int(oce_author::MAX_EXACT_INTEGER + 1), None),
                NONE,
                NONE,
                NONE,
            ),
            vec![pair("invalid-bound", "u")],
        ),
        (
            "crossed Reals",
            bounded((real(1.0), real(0.5)), NONE, NONE, NONE),
            vec![pair("crossed-bounds", "u")],
        ),
        (
            "crossed mixed literals on an output",
            bounded(NONE, (int(1), real(0.999_999)), NONE, NONE),
            vec![pair("crossed-bounds", "y")],
        ),
        (
            "crossed Integers",
            bounded(NONE, NONE, (int(5), int(-5)), NONE),
            vec![pair("crossed-bounds", "n")],
        ),
        (
            "a crossed pair with an invalid side reports the side only",
            bounded(NONE, NONE, (real(5.0), int(-5)), NONE),
            vec![pair("invalid-bound", "n")],
        ),
        (
            "both sides invalid",
            bounded((real(f64::NAN), real(f64::INFINITY)), NONE, NONE, NONE),
            vec![pair("non-finite-bound", "u"), pair("non-finite-bound", "u")],
        ),
    ];
    for (case, s, expected) in cases {
        assert_eq!(codes(&validate_author_side(&s)), expected, "{case}");
    }
    // Equal bounds admit one value and are accepted; so are `-0.0` beside `0.0`.
    for (min, max) in [
        (real(2.0), real(2.0)),
        (int(3), real(3.0)),
        (real(0.0), real(-0.0)),
    ] {
        let s = bounded((min, max), NONE, NONE, NONE);
        assert_eq!(validate(&s), vec![], "{min:?} {max:?}");
    }
}

#[test]
fn the_builder_refuses_a_non_finite_bound() {
    for (value, side) in [(f64::NAN, "min"), (f64::INFINITY, "max")] {
        let mut b = Boundary::new(id("u"), SignalType::Real);
        if side == "min" {
            b.min = Some(Bound::Real(value));
        } else {
            b.max = Some(Bound::Real(value));
        }
        let error = SequenceBuilder::new("s")
            .and_then(|s| s.input(b.clone()))
            .expect_err("refused");
        assert_eq!(
            error,
            BuildError::NonFiniteBound {
                name: "u".to_owned(),
                side
            }
        );
        assert!(error.to_string().contains("non-finite-bound"), "{error}");
        assert!(SequenceBuilder::new("s").and_then(|s| s.output(b)).is_err());
    }
    let mut ok = Boundary::new(id("u"), SignalType::Real);
    ok.min = Some(Bound::Real(f64::MIN));
    assert!(SequenceBuilder::new("s").and_then(|s| s.input(ok)).is_ok());
}

/// The canonical wire bytes of the clean pass-through with `u` bounded `[0]` / `[1]`.
fn wire() -> String {
    to_wire_string(&bounded(
        (Some(Bound::Integer(0)), Some(Bound::Integer(1))),
        NONE,
        NONE,
        NONE,
    ))
}

#[test]
fn the_wire_reader_refuses_every_malformed_bound_list() {
    let min = "\"min\": [\n        0\n      ]";
    assert!(wire().contains(min));
    type Expect = fn(&WireError) -> bool;
    let cases: [(&str, Expect); 9] = [
        ("\"min\": [0, 1]", |e| {
            matches!(e, WireError::BoundListTooLong { .. })
        }),
        ("\"min\": [0, 1, 2]", |e| {
            matches!(e, WireError::BoundListTooLong { .. })
        }),
        ("\"min\": [0, \"x\"]", |e| {
            matches!(e, WireError::BoundListTooLong { .. })
        }),
        ("\"min\": 0", |e| matches!(e, WireError::WrongType { .. })),
        ("\"min\": \"0\"", |e| {
            matches!(e, WireError::WrongType { .. })
        }),
        ("\"min\": [\"0\"]", |e| {
            matches!(e, WireError::WrongType { .. })
        }),
        ("\"min\": [[0]]", |e| {
            matches!(e, WireError::WrongType { .. })
        }),
        ("\"min\": [1e999]", |e| {
            matches!(e, WireError::NonFiniteNumber { .. })
        }),
        ("\"min\": [99999999999999999999]", |e| {
            matches!(e, WireError::IntegerOutOfRange { .. })
        }),
    ];
    for (replacement, expected) in cases {
        let text = wire().replacen(min, replacement, 1);
        let error = from_wire_str(&text).expect_err(replacement);
        assert!(expected(&error), "{replacement}: {error:?}");
    }
    let missing = wire().replacen(&format!("{min},\n      "), "", 1);
    assert!(
        matches!(from_wire_str(&missing), Err(WireError::MissingKey { ref key, .. }) if key == "min"),
        "{:?}",
        from_wire_str(&missing)
    );
    // `null`, an object and a Boolean are wrong types; `-0` reads as the Integer zero.
    for replacement in [
        "\"min\": null",
        "\"min\": {}",
        "\"min\": [true]",
        "\"min\": [null]",
    ] {
        let text = wire().replacen(min, replacement, 1);
        assert!(
            matches!(from_wire_str(&text), Err(WireError::WrongType { .. })),
            "{replacement}: {:?}",
            from_wire_str(&text)
        );
    }
    let negative_zero = wire().replacen(min, "\"min\": [-0]", 1);
    assert_eq!(
        from_wire_str(&negative_zero).expect("reads").inputs[0].min,
        Some(Bound::Integer(0))
    );
    let real_zero = wire().replacen(min, "\"min\": [-0.0]", 1);
    assert_eq!(
        from_wire_str(&real_zero).expect("reads").inputs[0].min,
        Some(Bound::Real(-0.0))
    );
}

#[test]
fn the_text_parser_refuses_every_malformed_bound() {
    let head = "sequence s\nextends none\n";
    for (line, code) in [
        ("in u : Real @min\n", "stray-token"),
        ("in u : Real @min @max 1\n", "stray-token"),
        ("in u : Real @min 1 K\n", "stray-token"),
        ("in u : Real @min 1 1\n", "stray-token"),
        ("in u : Real @min \"1\"\n", "stray-token"),
        ("in u : Real @min x\n", "stray-token"),
        ("in u : Real @min nan\n", "non-finite-number"),
        ("in u : Real @max Infinity\n", "non-finite-number"),
        ("in u : Real @min -inf\n", "stray-token"),
        ("in u : Real @min - 1\n", "stray-token"),
        ("in u : Real @min 1e999\n", "non-finite-number"),
        ("in u : Real @min +1\n", "malformed-number"),
        ("in u : Real @min 01\n", "malformed-number"),
        ("in u : Real @min 1.\n", "malformed-number"),
        (
            "in u : Real @min 99999999999999999999\n",
            "integer-out-of-range",
        ),
        ("in u : Real @min 0 @min 1\n", "duplicate-tag"),
        ("in u : Real @max 0 @max 0\n", "duplicate-tag"),
        ("in u : Real @minimum 0\n", "unknown-tag"),
    ] {
        let error = parse_text(&format!("{head}{line}")).expect_err(line);
        assert_eq!(error.code, code, "{line}: {error}");
        assert_eq!(error.location.line, 3, "{line}: {error}");
    }
    // A bound before the other tags, after them, and with a trailing comment all parse.
    for (line, min, max) in [
        (
            "in u : Real @max 1 @unit K @min 0\n",
            Some(Bound::Integer(0)),
            Some(Bound::Integer(1)),
        ),
        (
            "in u : Real @min -0 @max 1.0 # comment\n",
            Some(Bound::Integer(0)),
            Some(Bound::Real(1.0)),
        ),
        (
            "in u : Real @provisional @min -2.5\n",
            Some(Bound::Real(-2.5)),
            None,
        ),
    ] {
        let s = parse_text(&format!("{head}{line}")).unwrap_or_else(|e| panic!("{line}: {e}"));
        assert_eq!((s.inputs[0].min, s.inputs[0].max), (min, max), "{line}");
    }
    let s = parse_text(&format!("{head}in u : Real @min -0\n")).expect("parses");
    assert_eq!(
        s.inputs[0].min,
        Some(Bound::Integer(0)),
        "`-0` is the Integer zero"
    );
}

#[test]
fn the_printer_refuses_a_non_finite_bound_rather_than_printing_another_model() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let s = bounded((None, Some(Bound::Real(value))), NONE, NONE, NONE);
        let error = print_text(&s).expect_err("unspellable");
        assert!(error.to_string().contains("u"), "{error}");
        // The wire writer writes the non-finite bound as `null`, which the reader refuses.
        let wire = to_wire_string(&s);
        assert!(from_wire_str(&wire).is_err(), "{value}");
    }
}

#[test]
fn a_bound_is_carried_on_a_boundary_with_every_other_field() {
    let mut s = bounded(
        (Some(Bound::Real(-40.0)), Some(Bound::Integer(60))),
        NONE,
        NONE,
        NONE,
    );
    let b = &mut s.inputs[0];
    b.quantity = Some("ThermodynamicTemperature".to_owned());
    b.display_unit = Some("degC".to_owned());
    b.semantic.unit = Some("K".to_owned());
    b.semantic.haystack = vec!["air".to_owned()];
    b.semantic.provisional = true;
    round_trips(&s);
    assert_eq!(validate(&s), vec![]);
}
