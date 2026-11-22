//! The parameter scope sees the Real a Real parameter holds (issue #735, the owner's ruling of
//! 2026-11-21): `2` and `2.0` on a Real parameter are one model through every binding that reads
//! them, a sibling member's expression or reference and a block parameter bound to a sequence
//! parameter alike.
//!
//! The issue's three counterexamples are the first three tests, each in both spellings. The
//! rest hold the scope to the declared type of a sequence parameter, through an expression and
//! through a reference, and a property test draws random kinds, values and arrival paths and
//! requires every outcome to be the exact value or a typed refusal.

use crate::parameter_documents;

use oce_api::Value;
use parameter_documents::{
    Param, declared, document, load, loaded, outputs, param, refusal_codes, source, typed, value,
};

/// Content id and blob bytes of `doc`, which must load.
fn identity(doc: &str) -> (String, Vec<u8>) {
    let engine = loaded(doc);
    (
        engine
            .content_id()
            .expect("a content id")
            .as_str()
            .to_owned(),
        engine.compile_blob().expect("a blob").bytes,
    )
}

/// The refusal of `doc` rendered whole: every diagnostic's code, subject and message.
fn refusal(doc: &str) -> Vec<String> {
    match load(doc) {
        Ok(_) => panic!("expected a refusal\n{doc}"),
        Err(error) => error
            .diagnostics()
            .iter()
            .map(|diagnostic| {
                format!(
                    "{}|{}|{}",
                    diagnostic.code.as_str(),
                    diagnostic.subject.as_deref().unwrap_or(""),
                    diagnostic.message
                )
            })
            .collect(),
    }
}

#[test]
fn counterexample_one_a_sibling_reference_to_a_real_period_is_refused_in_both_spellings() {
    // `Integers.Sources.Pulse { period = 4, amplitude = period }`: before, the `4` spelling
    // output 4 and the `4.0` spelling output 1 (the default amplitude). `period` is a Real
    // parameter, so `amplitude = period` binds a Real to an Integer: one refusal for both.
    let doc = |period: &str| {
        document(
            &source("CDL.Integers.Sources.Pulse", "Integer"),
            &[value("period", period), value("amplitude", r#""period""#)],
            &[],
        )
    };
    let expected = refusal(&doc("4"));
    assert_eq!(
        expected,
        [
            "parameter-kind-mismatch|http://example.org#S.blk|parameter `amplitude` on block \
          `CDL.Integers.Sources.Pulse` must have kind Integer; got Real"
        ]
    );
    for period in [
        "4.0",
        &typed("4", "integer"),
        &typed("4.0", "double"),
        r#""2 * 2""#,
    ] {
        assert_eq!(refusal(&doc(period)), expected, "{period}");
    }
    // The explicit conversion is the author's to write, and runs as written.
    let mut engine = loaded(&document(
        &source("CDL.Integers.Sources.Pulse", "Integer"),
        &[
            value("period", "4.0"),
            value("amplitude", r#""integer(period)""#),
        ],
        &[],
    ));
    assert_eq!(outputs(&mut engine, &[0.0], None), [Value::Integer(4)]);
}

#[test]
fn counterexample_two_a_real_root_parameter_bound_to_an_integer_constant_is_refused_alike() {
    // Root `p` declared `S231:Real`; `Integers.Sources.Constant` binds `k = p`. Before, `4`
    // loaded and `4.0` was refused: the engine and the authoring layer disagreed on `4`.
    let doc = |literal: &str| {
        document(
            &source("CDL.Integers.Sources.Constant", "Integer"),
            &[value("k", r#""p""#)],
            &[declared("p", "Real", literal)],
        )
    };
    let expected = refusal(&doc("4.0"));
    assert_eq!(
        expected,
        [
            "parameter-kind-mismatch|http://example.org#S.blk|parameter `k` on block \
          `CDL.Integers.Sources.Constant` must have kind Integer; got Real"
        ]
    );
    for literal in ["4", &typed("4", "integer"), r#""2 + 2""#] {
        assert_eq!(refusal(&doc(literal)), expected, "{literal}");
    }
}

#[test]
fn counterexample_three_an_integer_square_past_two_to_the_fifty_third_loads_as_the_real() {
    // `Sin { amplitude = 134217728, offset = amplitude*amplitude }`: before, the Integer spelling
    // was refused (2^54 on a Real `offset`) where the Real spelling loaded, and 4294967296 failed
    // as an integer overflow. `amplitude` holds a Real, so both spellings square a Real.
    for (amplitude, square) in [
        ("134217728", 18_014_398_509_481_984.0),
        ("4294967296", 18_446_744_073_709_551_616.0),
    ] {
        let doc = |literal: &str| {
            document(
                &source("CDL.Reals.Sources.Sin", "Real"),
                &[
                    value("amplitude", literal),
                    value("freqHz", "1"),
                    value("offset", r#""amplitude*amplitude""#),
                ],
                &[],
            )
        };
        let as_integer = doc(amplitude);
        let as_real = doc(&format!("{amplitude}.0"));
        assert_eq!(identity(&as_integer), identity(&as_real), "{amplitude}");
        let engine = loaded(&as_integer);
        assert!(
            param(&engine, "offset").bit_eq(&Value::Real(square)),
            "{amplitude}"
        );
        let mut engine = loaded(&as_real);
        // sin(0) = 0: the output at t = 0 is the offset, exactly.
        assert!(
            outputs(&mut engine, &[0.0], None)[0].bit_eq(&Value::Real(square)),
            "{amplitude}"
        );
    }
}

#[test]
fn a_declared_real_root_parameter_is_one_model_through_an_expression_and_a_reference() {
    let reference = |literal: &str| {
        document(
            &source("CDL.Reals.Sources.Constant", "Real"),
            &[value("k", r#""p""#)],
            &[declared("p", "Real", literal)],
        )
    };
    let expression = |literal: &str| {
        document(
            &source("CDL.Reals.Sources.Constant", "Real"),
            &[value("k", r#""p * 2""#)],
            &[declared("p", "Real", literal)],
        )
    };
    for doc in [reference as fn(&str) -> String, expression] {
        let expected = identity(&doc("4.0"));
        for literal in [
            "4",
            &typed("4", "integer"),
            &typed("4", "long"),
            r#""3 + 1""#,
        ] {
            assert_eq!(identity(&doc(literal)), expected, "{literal}");
        }
    }
    let mut engine = loaded(&expression("4"));
    assert_eq!(outputs(&mut engine, &[0.0], None), [Value::Real(8.0)]);
    // An Integer division over the held Real is a Real division: `p / 3` is 4/3 either way.
    let third = |literal: &str| {
        document(
            &source("CDL.Reals.Sources.Constant", "Real"),
            &[value("k", r#""p / 3""#)],
            &[declared("p", "Real", literal)],
        )
    };
    assert_eq!(identity(&third("4")), identity(&third("4.0")));
}

#[test]
fn a_declared_integer_root_parameter_admits_an_integer_and_refuses_a_real() {
    let doc = |literal: &str| {
        document(
            &source("CDL.Integers.Sources.Constant", "Integer"),
            &[value("k", r#""n""#)],
            &[declared("n", "Integer", literal)],
        )
    };
    let mut engine = loaded(&doc("4"));
    assert_eq!(outputs(&mut engine, &[0.0], None), [Value::Integer(4)]);
    let mut engine = loaded(&doc(&typed("4", "integer")));
    assert_eq!(outputs(&mut engine, &[0.0], None), [Value::Integer(4)]);
    // `4.0` on a declared Integer is refused on the declaration: CDL does not narrow a Real.
    let expected = [
        "parameter-kind-mismatch|http://example.org#S.n|parameter `n` is declared \
                     Integer and binds a value of kind Real",
    ];
    for literal in ["4.0", &typed("4.0", "double"), r#""8 / 2""#] {
        assert_eq!(refusal(&doc(literal)), expected, "{literal}");
    }
    // A declared Boolean admits a Boolean only.
    let flag = |literal: &str| {
        document(
            &source("CDL.Logical.Sources.Constant", "Boolean"),
            &[value("k", r#""b""#)],
            &[declared("b", "Boolean", literal)],
        )
    };
    assert_eq!(
        outputs(&mut loaded(&flag("true")), &[0.0], None),
        [Value::Boolean(true)]
    );
    assert_eq!(refusal_codes(&flag("1")), ["parameter-kind-mismatch"]);
}

#[test]
fn a_declared_real_past_two_to_the_fifty_third_is_refused_on_the_declaration() {
    let doc = |literal: &str| {
        document(
            &source("CDL.Reals.Sources.Constant", "Real"),
            &[value("k", r#""p""#)],
            &[declared("p", "Real", literal)],
        )
    };
    // The declaration keeps the Integer it could not hold, so the block parameter bound to it
    // is refused for the same reason, on its own node: two findings, one cause, no value loaded.
    assert_eq!(
        refusal(&doc("9007199254740993")),
        [
            "parameter-out-of-range|http://example.org#S.blk.k|parameter `k` on \
             `http://example.org#S.blk` binds the Integer 9007199254740993 to a Real parameter \
             of `CDL.Reals.Sources.Constant`; an Integer above 2^53 in magnitude is not exact as \
             a Real",
            "parameter-out-of-range|http://example.org#S.p|parameter `p` is declared Real and \
             binds the Integer 9007199254740993; an Integer above 2^53 in magnitude is not exact \
             as a Real",
        ]
    );
    assert_eq!(
        identity(&doc("9007199254740992")),
        identity(&doc("9007199254740992.0"))
    );
}

#[test]
fn an_untyped_root_parameter_keeps_the_kind_the_document_wrote() {
    // No declared type: `4` is the Integer 4, `4.0` the Real 4.0, and each block parameter is
    // judged by what it reads (a Real block parameter widens the Integer).
    let integer = |literal: &str| {
        document(
            &source("CDL.Integers.Sources.Constant", "Integer"),
            &[value("k", r#""n""#)],
            &[value("n", literal)],
        )
    };
    assert_eq!(
        outputs(&mut loaded(&integer("4")), &[0.0], None),
        [Value::Integer(4)]
    );
    assert_eq!(refusal_codes(&integer("4.0")), ["parameter-kind-mismatch"]);
    let real = |literal: &str| {
        document(
            &source("CDL.Reals.Sources.Constant", "Real"),
            &[value("k", r#""n""#)],
            &[value("n", literal)],
        )
    };
    assert_eq!(identity(&real("4")), identity(&real("4.0")));
}

/// A seeded xorshift64* generator (no dependency).
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

/// One literal the property test can write, with the value it means.
struct Spelling {
    literal: String,
    meaning: Value,
}

fn spellings(rng: &mut XorShift) -> Spelling {
    let integer = (rng.next() as i64) >> rng.below(64);
    let small = i64::try_from(rng.below(2001)).unwrap() - 1000;
    let pick = rng.below(12);
    let (literal, meaning) = match pick {
        0 => (small.to_string(), Value::Integer(small)),
        1 => (integer.to_string(), Value::Integer(integer)),
        2 => (format!("{small}.0"), Value::Real(small as f64)),
        // `-3.5` is minus three and a half: the fraction carries the sign of the digits.
        3 => (
            format!("{small}.5"),
            Value::Real(if small < 0 {
                small as f64 - 0.5
            } else {
                small as f64 + 0.5
            }),
        ),
        4 => (format!("{small}e0"), Value::Real(small as f64)),
        5 => (typed(&small.to_string(), "integer"), Value::Integer(small)),
        6 => (
            typed(&format!("{small}.0"), "double"),
            Value::Real(small as f64),
        ),
        7 => ("true".to_owned(), Value::Boolean(true)),
        8 => ("false".to_owned(), Value::Boolean(false)),
        9 => (format!(r#""{small} + 0""#), Value::Integer(small)),
        10 => (format!(r#""{small} / 1""#), Value::Real(small as f64)),
        _ => ("-0.0".to_owned(), Value::Real(-0.0)),
    };
    Spelling { literal, meaning }
}

/// The value a parameter of `declared` kind holds for `meaning`, or `None` when it must be
/// refused: the exact table of `oce_blocks::admits_parameter_value` for scalars.
fn held(declared: &str, meaning: &Value) -> Option<Value> {
    match (declared, meaning) {
        ("Real", Value::Real(x)) => Some(Value::Real(*x)),
        ("Real", Value::Integer(i)) if i.unsigned_abs() <= 1 << 53 => Some(Value::Real(*i as f64)),
        ("Integer", Value::Integer(i)) => Some(Value::Integer(*i)),
        ("Boolean", Value::Boolean(b)) => Some(Value::Boolean(*b)),
        _ => None,
    }
}

#[test]
fn every_random_kind_value_and_arrival_is_the_exact_value_or_a_typed_refusal() {
    let blocks = [
        ("CDL.Reals.Sources.Constant", "Real"),
        ("CDL.Integers.Sources.Constant", "Integer"),
        ("CDL.Logical.Sources.Constant", "Boolean"),
    ];
    let declarations = ["", "Real", "Integer", "Boolean"];
    let mut rng = XorShift(0x7345_0734_0735_0001);
    let (mut exact, mut refused) = (0usize, 0usize);
    for case in 0..1500 {
        let (class, kind) = blocks[rng.below(blocks.len())];
        let spelling = spellings(&mut rng);
        // Arrival: the literal itself, or a root parameter (untyped or declared) it is bound to.
        let declared_as = declarations[rng.below(declarations.len())];
        let arrives_through_root = rng.below(2) == 0;
        let (params, root, expected): (Vec<Param>, Vec<Param>, Option<Value>) =
            if arrives_through_root {
                let root_param = if declared_as.is_empty() {
                    value("p", &spelling.literal)
                } else {
                    declared("p", declared_as, &spelling.literal)
                };
                // The root parameter holds the meaning in its declared kind (or as written),
                // and the block parameter then holds that in its own kind.
                let at_root = if declared_as.is_empty() {
                    Some(spelling.meaning.clone())
                } else {
                    held(declared_as, &spelling.meaning)
                };
                let expected = at_root.and_then(|value| held(kind, &value));
                (vec![value("k", r#""p""#)], vec![root_param], expected)
            } else {
                let expected = held(kind, &spelling.meaning);
                (vec![value("k", &spelling.literal)], Vec::new(), expected)
            };
        let doc = document(&source(class, kind), &params, &root);
        match (load(&doc), expected) {
            (Ok(mut engine), Some(expected)) => {
                assert!(
                    param(&engine, "k").bit_eq(&expected),
                    "case {case}: {class} {} through {declared_as:?}/{arrives_through_root}",
                    spelling.literal
                );
                let output = outputs(&mut engine, &[0.0], None);
                assert!(
                    output[0].bit_eq(&expected),
                    "case {case}: output {output:?}"
                );
                exact += 1;
            }
            (Err(error), None) => {
                let codes: Vec<&str> = error
                    .diagnostics()
                    .iter()
                    .map(|d| d.code.as_str())
                    .collect();
                assert!(
                    !codes.is_empty()
                        && codes.iter().all(|code| matches!(
                            *code,
                            "parameter-kind-mismatch" | "parameter-out-of-range"
                        )),
                    "case {case}: {class} {} refused with {codes:?}",
                    spelling.literal
                );
                refused += 1;
            }
            (outcome, expected) => panic!(
                "case {case}: {class} {} through {declared_as:?}/{arrives_through_root}: \
                 expected {expected:?}, got {:?}",
                spelling.literal,
                outcome.map(|engine| param(&engine, "k"))
            ),
        }
    }
    assert!(
        exact > 300 && refused > 300,
        "{exact} exact, {refused} refused"
    );
}
