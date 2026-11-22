//! A parameter is the value the document wrote, exactly, or the document is refused (issue
//! #734): never a block that loads and runs on its default.
//!
//! Before, `Integers.Sources.Pulse { period = 4, amplitude = 4.0 }` loaded clean and output the
//! default amplitude 1, because the block's Integer reader fell back to the default for a Real
//! and no rule judged the kind of `amplitude`. Every case here loads a one-block document built in
//! the test, then either ticks it and observes an output that depends on the parameter, or
//! states the typed refusal. `oce-validate`'s `params_kinds` tests sweep every parameter of every
//! class; these run the cells end to end, with the blob round trip and a resume.

use crate::parameter_documents;

use oce_api::Value;
use oce_graph::blob::decode;
use oce_graph::{EvalContext, eval_tick};
use parameter_documents::{
    Param, Shape, Values, document, loaded, outputs, param, refusal_codes, source, table, typed,
    value, vector,
};

const PULSE: &str = "CDL.Integers.Sources.Pulse";

/// `Integers.Sources.Pulse` with `period = 4` and `amplitude` written `literal`.
fn pulse(amplitude: &str) -> String {
    document(
        &source(PULSE, "Integer"),
        &[value("period", "4"), value("amplitude", amplitude)],
        &[],
    )
}

#[test]
fn an_integer_amplitude_runs_as_written_in_every_integer_spelling() {
    // High for the first half of the period, then the offset 0.
    for literal in [
        "3".to_owned(),
        typed("3", "integer"),
        typed("3", "long"),
        r#""3""#.to_owned(),
        r#""1 + 2""#.to_owned(),
        r#""integer(3.0)""#.to_owned(),
    ] {
        let mut engine = loaded(&pulse(&literal));
        assert!(
            param(&engine, "amplitude").bit_eq(&Value::Integer(3)),
            "{literal}"
        );
        assert_eq!(
            outputs(&mut engine, &[0.0, 3.0], None),
            [Value::Integer(3), Value::Integer(0)],
            "{literal}"
        );
    }
    // An Integer past 2^53 is still an Integer on an Integer parameter, held without loss.
    let mut engine = loaded(&pulse("9007199254740993"));
    assert_eq!(
        outputs(&mut engine, &[0.0], None),
        [Value::Integer(9_007_199_254_740_993)]
    );
}

#[test]
fn a_real_amplitude_is_refused_where_it_ran_on_the_default() {
    // Each of these loaded and output 1 (the default amplitude) before.
    for literal in [
        "3.0".to_owned(),
        "3.5".to_owned(),
        "3e0".to_owned(),
        "35e-1".to_owned(),
        "-0".to_owned(),
        "-0.0".to_owned(),
        "9223372036854775808".to_owned(),
        typed("3.0", "double"),
        typed("3", "decimal"),
        r#""3.0""#.to_owned(),
        r#""6 / 2""#.to_owned(),
        "true".to_owned(),
        r#""\"three\"""#.to_owned(),
        r#""true""#.to_owned(),
    ] {
        assert_eq!(
            refusal_codes(&pulse(&literal)),
            ["parameter-kind-mismatch"],
            "{literal}"
        );
    }
    // A value that is no scalar at all is refused at grounding, as before.
    for literal in ["null", "[3]"] {
        assert_eq!(
            refusal_codes(&pulse(literal)),
            ["grounding-failed"],
            "{literal}"
        );
    }
}

#[test]
fn the_offset_is_judged_like_the_amplitude() {
    let doc = |offset: &str| {
        document(
            &source(PULSE, "Integer"),
            &[value("period", "4"), value("offset", offset)],
            &[],
        )
    };
    let mut engine = loaded(&doc("-2"));
    assert_eq!(
        outputs(&mut engine, &[0.0, 3.0], None),
        [Value::Integer(-1), Value::Integer(-2)]
    );
    assert_eq!(refusal_codes(&doc("-2.0")), ["parameter-kind-mismatch"]);
}

#[test]
fn a_comparators_integer_threshold_runs_as_written_and_a_real_one_is_refused() {
    let shape = Shape {
        class: "CDL.Integers.GreaterThreshold",
        output: "Boolean",
        input: Some("Integer"),
    };
    for (t, expected) in [("2", true), ("3", false), (r#""1 + 1""#, true)] {
        let mut engine = loaded(&document(&shape, &[value("t", t)], &[]));
        assert_eq!(
            outputs(&mut engine, &[0.0], Some(Value::Integer(3))),
            [Value::Boolean(expected)],
            "t = {t}"
        );
    }
    for t in ["2.0", "2.5", "true"] {
        assert_eq!(
            refusal_codes(&document(&shape, &[value("t", t)], &[])),
            ["parameter-kind-mismatch"],
            "t = {t}"
        );
    }
}

#[test]
fn a_real_threshold_widens_an_integer_and_refuses_another_kind() {
    let shape = Shape {
        class: "CDL.Reals.GreaterThreshold",
        output: "Boolean",
        input: Some("Real"),
    };
    for (t, expected) in [
        ("2", true),
        ("3", false),
        ("2.5", true),
        (r#""5 / 2""#, true),
    ] {
        let mut engine = loaded(&document(&shape, &[value("t", t)], &[]));
        assert_eq!(
            outputs(&mut engine, &[0.0], Some(Value::Real(2.75))),
            [Value::Boolean(expected)],
            "t = {t}"
        );
    }
    assert_eq!(
        refusal_codes(&document(&shape, &[value("t", "true")], &[])),
        ["parameter-kind-mismatch"]
    );
    assert_eq!(
        refusal_codes(&document(&shape, &[value("t", "9007199254740993")], &[])),
        ["parameter-out-of-range"]
    );
}

/// A Real `TimeTable` from (0, 0) to (2, 2) whose `smoothness` is written `literal`: linear
/// segments output 1.0 at t = 1, constant segments 0.0.
fn time_table(smoothness: &str) -> String {
    document(
        &source("CDL.Reals.Sources.TimeTable", "Real"),
        &[
            table("table", "[0.0, 0.0, 2.0, 2.0]"),
            value("smoothness", smoothness),
        ],
        &[],
    )
}

#[test]
fn an_enumeration_runs_as_the_member_each_admitted_spelling_names() {
    for literal in [
        r#""Buildings.Controls.OBC.CDL.Types.Smoothness.ConstantSegments""#,
        "2",
        r#""\"ConstantSegments\"""#,
        r#""\"Buildings.Controls.OBC.CDL.Types.Smoothness.ConstantSegments\"""#,
    ] {
        let mut engine = loaded(&time_table(literal));
        assert_eq!(
            outputs(&mut engine, &[0.0, 1.0], None),
            [Value::Real(0.0), Value::Real(0.0)],
            "{literal}"
        );
    }
    let mut engine = loaded(&time_table(
        r#""Buildings.Controls.OBC.CDL.Types.Smoothness.LinearSegments""#,
    ));
    assert_eq!(
        outputs(&mut engine, &[0.0, 1.0], None),
        [Value::Real(0.0), Value::Real(1.0)]
    );
}

#[test]
fn a_member_of_another_enumeration_is_refused_where_it_ran_as_the_default() {
    // `LastTwoPoints` is ordinal 2 of `Extrapolation`: before, it passed the ordinal check and
    // the reader, finding no `Smoothness`, ran linear segments (the default).
    assert_eq!(
        refusal_codes(&time_table(
            r#""Buildings.Controls.OBC.CDL.Types.Extrapolation.LastTwoPoints""#
        )),
        ["parameter-kind-mismatch"]
    );
    // Stand-ins naming no member are refused by the class's member rule, as before.
    for literal in ["9", "2.0", "true", r#""\"Nope\"""#] {
        assert_eq!(
            refusal_codes(&time_table(literal)),
            ["parameter-out-of-range"],
            "{literal}"
        );
    }
}

#[test]
fn an_array_element_is_held_in_its_element_type_and_runs_as_written() {
    // `Reals.Sources.TimeTable.offset` is a Real vector: an Integer element widens exactly, and
    // the output at t = 0 is the table's first value (0) plus the offset.
    let doc = |offset: &str| {
        document(
            &source("CDL.Reals.Sources.TimeTable", "Real"),
            &[
                table("table", "[0.0, 0.0, 2.0, 2.0]"),
                vector("offset", 1, offset),
            ],
            &[],
        )
    };
    for (offset, expected) in [("[1]", 1.0), ("[1.5]", 1.5), (r#""fill(2, 1)""#, 2.0)] {
        let mut engine = loaded(&doc(offset));
        assert!(
            param(&engine, "offset_1").bit_eq(&Value::Real(expected)),
            "{offset}"
        );
        assert_eq!(
            outputs(&mut engine, &[0.0], None),
            [Value::Real(expected)],
            "{offset}"
        );
    }
    // A Boolean element is refused by the offset rule, which judges its elements' kind.
    assert_eq!(refusal_codes(&doc("[true]")), ["parameter-out-of-range"]);
    assert_eq!(
        refusal_codes(&doc("[9007199254740993]")),
        ["parameter-out-of-range"]
    );
}

#[test]
fn the_neighbouring_cells_keep_their_behaviour() {
    // A required Integer, a Real and a Boolean constant: refused or widened as before.
    let integer = |k: &str| {
        document(
            &source("CDL.Integers.Sources.Constant", "Integer"),
            &[value("k", k)],
            &[],
        )
    };
    let real = |k: &str| {
        document(
            &source("CDL.Reals.Sources.Constant", "Real"),
            &[value("k", k)],
            &[],
        )
    };
    let boolean = |k: &str| {
        document(
            &source("CDL.Logical.Sources.Constant", "Boolean"),
            &[value("k", k)],
            &[],
        )
    };
    assert_eq!(refusal_codes(&integer("2.0")), ["parameter-kind-mismatch"]);
    assert_eq!(refusal_codes(&boolean("1")), ["parameter-kind-mismatch"]);
    assert_eq!(
        refusal_codes(&real("9007199254740993")),
        ["parameter-out-of-range"]
    );
    let mut engine = loaded(&real("2"));
    assert_eq!(outputs(&mut engine, &[0.0], None), [Value::Real(2.0)]);
    let mut engine = loaded(&real("-0.0"));
    assert!(outputs(&mut engine, &[0.0], None)[0].bit_eq(&Value::Real(-0.0)));
    let mut engine = loaded(&integer("7"));
    assert_eq!(outputs(&mut engine, &[0.0], None), [Value::Integer(7)]);
}

#[test]
fn a_pulse_with_an_integer_amplitude_round_trips_through_the_blob_and_resumes() {
    // `period` written as an Integer is held as the Real 4.0, `amplitude` as the Integer 3: the
    // blob carries both as held, decodes to blocks that tick the same outputs, and a snapshot
    // taken mid-run resumes to the outputs of the uninterrupted run.
    let doc = pulse("3");
    let engine = loaded(&doc);
    let blob = engine.compile_blob().expect("a blob");
    assert_eq!(
        blob.bytes,
        loaded(&document(
            &source(PULSE, "Integer"),
            &[value("period", "4.0"), value("amplitude", "3")],
            &[],
        ))
        .compile_blob()
        .expect("a blob")
        .bytes,
        "the two spellings of the period are one blob"
    );
    let mut decoded = decode(&blob.bytes).expect("the blob decodes");
    let times = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
    let mut from_blob = Vec::new();
    for &time in &times {
        let mut ctx = EvalContext {
            model: &decoded.model,
            schedule: &decoded.schedule,
            blocks: &decoded.blocks,
            diagnostics: &oce_blocks::NoopDiagnostics,
            state: &mut decoded.state,
        };
        eval_tick(&mut ctx, time);
        from_blob.push(decoded.state.values[0].clone());
    }
    let mut uninterrupted = loaded(&doc);
    let expected = outputs(&mut uninterrupted, &times, None);
    assert_eq!(Values(from_blob), expected);
    assert_eq!(
        expected,
        [3, 3, 0, 0, 3, 3].map(Value::Integer),
        "high for half of each 4 s period"
    );

    let mut first = loaded(&doc);
    outputs(&mut first, &times[..3], None);
    let snapshot = first.state_snapshot().expect("a snapshot");
    let mut resumed = loaded(&doc);
    resumed
        .restore_state(&snapshot)
        .expect("the snapshot restores");
    assert_eq!(outputs(&mut resumed, &times[3..], None), expected[3..]);
}

#[test]
fn a_block_parameter_bound_to_a_root_parameter_is_judged_by_the_value_it_reads() {
    // An untyped root parameter keeps what the document wrote: `4` is an Integer, `4.0` a Real.
    let doc = |root: Param| {
        document(
            &source(PULSE, "Integer"),
            &[value("period", "4"), value("amplitude", r#""a""#)],
            &[root],
        )
    };
    let mut engine = loaded(&doc(value("a", "4")));
    assert_eq!(outputs(&mut engine, &[0.0], None), [Value::Integer(4)]);
    assert_eq!(
        refusal_codes(&doc(value("a", "4.0"))),
        ["parameter-kind-mismatch"]
    );
    // Nothing loads on a default: whatever the root parameter holds, the amplitude is that value
    // or the document is refused.
    assert_eq!(
        refusal_codes(&doc(value("a", "true"))),
        ["parameter-kind-mismatch"]
    );
}
