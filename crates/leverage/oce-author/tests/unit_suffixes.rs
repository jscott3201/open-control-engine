//! Time unit suffixes scaled from the vocabulary's table, and `unit-suffix-mismatch` (issue
//! #501; vocabulary note, "Units", decision 14 and rulings 5 and 6; authoring decision 13 as
//! amended). Through the text form: what a scaled suffix reads as, what `fmt` keeps, what the wire
//! form and the emitter see, which suffix a parameter can take, and which spellings stay refused.
//! Expected values are exact: an integer the product equals, or the decimal it equals.

use oce_author::{
    Assignment, EmitOptions, Literal, Location, MALFORMED_NUMBER, NON_FINITE_NUMBER, ParamValue,
    PrintError, Sequence, UNIT_NOT_BASE, UNIT_SUFFIX_MISMATCH, emit, fmt_text, from_wire_str,
    parse_text, print_text, to_wire_string, validate, validate_text_author_side,
};

const HEAD: &str = "sequence s\nderived_from none\n";

/// A clean two-block sequence whose `TrueDelay` takes `delay_time` as written.
fn delay(delay_time: &str) -> String {
    format!(
        "{HEAD}in u : Boolean\nout y : Boolean\nblock del = CDL.Logical.TrueDelay(delayTime = {delay_time})\nu -> del.u\ndel.y -> y\n"
    )
}

fn delay_time(model: &Sequence) -> &Assignment {
    &model.blocks[0].assignments[0]
}

fn real(model: &Sequence) -> f64 {
    match delay_time(model).value {
        ParamValue::Literal(Literal::Real(v)) => v,
        ref other => panic!("not a Real: {other:?}"),
    }
}

#[test]
fn each_scaled_suffix_reads_as_its_exact_base_value() {
    let cases: &[(&str, f64, Literal)] = &[
        ("1 h", 3_600.0, Literal::Integer(1)),
        ("1.1 h", 3_960.0, Literal::Real(1.1)),
        ("1.5 min", 90.0, Literal::Real(1.5)),
        ("2 d", 172_800.0, Literal::Integer(2)),
        ("30 min", 1_800.0, Literal::Integer(30)),
        ("0 d", 0.0, Literal::Integer(0)),
        ("-0.0 h", -0.0, Literal::Real(-0.0)),
        // Past 2^53: 540431955284459580 s exactly, rounded once to the nearer neighbour.
        (
            "9007199254740993 min",
            540_431_955_284_459_584.0,
            Literal::Integer(9_007_199_254_740_993),
        ),
    ];
    for (written, base, number) in cases {
        let model = parse_text(&delay(written)).unwrap_or_else(|e| panic!("{written}: {e}"));
        assert_eq!(real(&model).to_bits(), base.to_bits(), "{written}");
        let symbol = written.rsplit(' ').next().expect("a suffix");
        assert_eq!(
            delay_time(&model).suffix.as_deref(),
            Some(symbol),
            "{written}"
        );
        assert_eq!(
            delay_time(&model).scaled_from.as_ref(),
            Some(number),
            "{written}"
        );
    }
    // A base-unit suffix keeps the number as written and no scaled-from number.
    let seconds = parse_text(&delay("3600 s")).expect("parses");
    assert_eq!(
        delay_time(&seconds).value,
        ParamValue::Literal(Literal::Integer(3_600))
    );
    assert_eq!(delay_time(&seconds).scaled_from, None);
}

#[test]
fn fmt_keeps_each_scaled_suffix_as_written_and_never_moves_the_value() {
    for (written, printed) in [
        ("1 h", "1 h"),
        ("1.50 h", "1.5 h"),
        ("1.10 h", "1.1 h"),
        ("2.0e0 d", "2.0 d"),
        ("-0.0 h", "-0.0 h"),
        ("9007199254740993 min", "9007199254740993 min"),
    ] {
        let once = fmt_text(&delay(written)).expect(written);
        assert!(
            once.contains(&format!("delayTime = {printed})")),
            "{written}: {once}"
        );
        assert_eq!(fmt_text(&once).as_deref(), Ok(once.as_str()), "{written}");
        let before = parse_text(&delay(written)).expect(written);
        let after = parse_text(&once).expect(written);
        assert_eq!(real(&before).to_bits(), real(&after).to_bits(), "{written}");
        assert_eq!(
            print_text(&after).as_deref(),
            Ok(once.as_str()),
            "{written}"
        );
        // The same bytes on a second run.
        assert_eq!(fmt_text(&delay(written)).as_deref(), Ok(once.as_str()));
    }
}

#[test]
fn the_wire_form_and_the_emitter_see_only_the_base_value() {
    let hours = parse_text(&delay("1 h")).expect("parses");
    let seconds = parse_text(&delay("3600.0")).expect("parses");
    assert_eq!(validate(&hours), vec![], "clean in both stages");
    assert_eq!(to_wire_string(&hours), to_wire_string(&seconds));
    let options = EmitOptions::default();
    assert_eq!(
        emit(&hours, &options).expect("emits"),
        emit(&seconds, &options).expect("emits")
    );
    // Text to wire to text drops the suffix (text-only, decision 13); wire to text to wire is
    // byte-stable.
    let wire = to_wire_string(&hours);
    let back = from_wire_str(&wire).expect("reads");
    let text = print_text(&back).expect("prints");
    assert!(text.contains("delayTime = 3600.0)"), "{text}");
    assert_eq!(to_wire_string(&parse_text(&text).expect("parses")), wire);
}

#[test]
fn a_suffix_the_parameter_cannot_take_is_refused_at_its_assignment() {
    let text = format!(
        "{HEAD}param p : Real = 1 h\nin u : Boolean\nin x : Real\nout y : Boolean\nout z : Real\nblock del = CDL.Logical.TrueDelay(delayTime = 5 K)\nblock hol = CDL.Logical.TrueFalseHold(trueHoldDuration = 1 Hz, falseHoldDuration = 2 H)\nblock hys = CDL.Reals.Hysteresis(uLow = 1 h, uHigh = 2 K)\nblock sum = CDL.Reals.MultiSum(nin = 2, k = [1 min, 2])\nu -> del.u\ndel.y -> hol.u\nhol.y -> y\nx -> hys.u\nx -> sum.u1\nx -> sum.u2\nsum.y -> z\n"
    );
    let findings = validate_text_author_side(&text).expect("parses");
    let mismatches: Vec<(&str, Option<Location>)> = findings
        .iter()
        .filter(|f| f.code == UNIT_SUFFIX_MISMATCH)
        .map(|f| (f.subject.as_str(), f.span))
        .collect();
    let at = |line: u32, column: u32| Some(Location { line, column });
    assert_eq!(
        mismatches,
        [
            ("p", at(3, 7)),
            ("del.delayTime", at(8, 35)),
            ("hol.trueHoldDuration", at(9, 39)),
            ("hol.falseHoldDuration", at(9, 64)),
            ("hys.uLow", at(10, 34)),
            // An array element is placed at its value.
            ("sum.k_1", at(11, 46)),
        ],
        "{findings:#?}"
    );
    // A base-unit suffix on a parameter that declares no unit (`uHigh = 2 K`, `k_2` has none)
    // stays an annotation; every finding is one of the six above.
    assert_eq!(findings.len(), mismatches.len(), "{findings:#?}");
    let detail = |subject: &str| {
        &findings
            .iter()
            .find(|f| f.subject == subject)
            .expect(subject)
            .detail
    };
    assert!(detail("del.delayTime").contains("declared in `s`"));
    assert!(detail("hys.uLow").contains("declares no unit"));
    assert!(detail("p").contains("sequence parameter `p` declares no unit"));
}

#[test]
fn a_suffix_that_reduces_to_the_declared_unit_is_accepted() {
    for written in ["1 h", "3600 s", "60 min", "1 d", "0.5 d", "3600.0"] {
        let model = parse_text(&delay(written)).expect(written);
        assert_eq!(validate(&model), vec![], "{written}");
    }
    // A base-unit suffix where no unit is declared is an annotation, as before.
    let annotated = format!(
        "{HEAD}param dT : Real = 1 K\nin x : Real\nout y : Boolean\nblock hys = CDL.Reals.Hysteresis(uLow = 0.0 K, uHigh = dT)\nx -> hys.u\nhys.y -> y\n"
    );
    assert_eq!(validate(&parse_text(&annotated).expect("parses")), vec![]);
}

#[test]
fn spellings_outside_the_table_keep_their_parse_codes() {
    let refused = |written: &str, code: &str, column: u32| {
        let error = parse_text(&delay(written)).expect_err(written);
        assert_eq!(
            (error.code, error.location.line, error.location.column),
            (code, 5, column),
            "{written}: {error}"
        );
        error.detail
    };
    // The number starts at column 47 of line 5 and its suffix after one space.
    refused("1 degC", UNIT_NOT_BASE, 49);
    refused("1 kPa", UNIT_NOT_BASE, 49);
    refused("1 m3/h", UNIT_NOT_BASE, 49);
    refused("1 hh", UNIT_NOT_BASE, 49);
    refused("1 hr", UNIT_NOT_BASE, 49);
    refused("1h", MALFORMED_NUMBER, 47);
    refused("1e999 h", NON_FINITE_NUMBER, 47);
    // Finite as written, past binary64 once scaled: refused at the number.
    let detail = refused("1e308 d", NON_FINITE_NUMBER, 47);
    assert!(detail.contains("once scaled to `s`"), "{detail}");
    refused("-1.7976931348623157e308 min", NON_FINITE_NUMBER, 47);
    // The largest that fits is accepted.
    let model = parse_text(&delay("2e303 d")).expect("fits");
    assert_eq!(real(&model).to_bits(), 1.728e308_f64.to_bits());
}

#[test]
fn the_printer_refuses_a_scaled_suffix_that_does_not_agree_with_its_value() {
    let base = parse_text(&delay("1 h")).expect("parses");
    type Edit = fn(&mut Assignment);
    let cases: [(Edit, &str); 5] = [
        (|a| a.scaled_from = None, "has no written number"),
        (|a| a.suffix = None, "but no suffix"),
        (
            |a| a.value = ParamValue::Literal(Literal::Real(3_601.0)),
            "but the value is",
        ),
        (
            |a| a.value = ParamValue::Literal(Literal::Integer(3_600)),
            "but the value is",
        ),
        (|a| a.suffix = Some("s".to_owned()), "is a base unit"),
    ];
    for (edit, needle) in cases {
        let mut model = base.clone();
        edit(&mut model.blocks[0].assignments[0]);
        match print_text(&model) {
            Err(PrintError::Unspellable { subject, detail }) => {
                assert_eq!(subject, "del.delayTime");
                assert!(detail.contains(needle), "{needle}: {detail}");
            }
            other => panic!("{needle}: {other:?}"),
        }
    }
    // The same rule on a sequence parameter.
    let mut param = parse_text(&format!("{HEAD}param p : Real = 2 min\n")).expect("parses");
    assert_eq!(param.params[0].value, Literal::Real(120.0));
    let printed = print_text(&param).expect("prints");
    assert!(printed.contains("\nparam p : Real = 2 min\n"), "{printed}");
    assert_eq!(parse_text(&printed).expect("parses"), param);
    param.params[0].value = Literal::Real(2.0);
    assert!(matches!(
        print_text(&param),
        Err(PrintError::Unspellable { ref subject, .. }) if subject == "p"
    ));
}

/// A model built in code whose scaled suffix, written number and value no longer agree (a tool
/// that set `delayTime` to 1.0 and left the `1 h` annotation) is caught by stage 1, not emitted:
/// the printer's own agreement rule, reported as `unit-suffix-mismatch` at the assignment.
#[test]
fn stage_one_refuses_a_suffix_that_does_not_agree_with_its_value() {
    type Edit = fn(&mut Assignment);
    let what = "parameter `delayTime` of `CDL.Logical.TrueDelay`";
    let cases: [(Edit, &str); 10] = [
        (
            |a| a.scaled_from = None,
            "the scaled suffix `h` has no written number",
        ),
        (
            |a| a.scaled_from = Some(Literal::Integer(2)),
            "`2 h` is `7200.0` in `s`, but the value is Real `3600.0`",
        ),
        (
            |a| a.suffix = Some("s".to_owned()),
            "the suffix `s` is a base unit, so it keeps no written number to scale (`1`)",
        ),
        (
            |a| a.suffix = None,
            "a written number `1` for a scaled suffix, but no suffix",
        ),
        (
            |a| a.scaled_from = Some(Literal::Boolean(true)),
            "the written number before `h` is Boolean, not a number",
        ),
        (
            |a| a.value = ParamValue::Literal(Literal::Integer(3_600)),
            "`1 h` is `3600.0` in `s`, but the value is Integer `3600`",
        ),
        (
            |a| a.value = ParamValue::Literal(Literal::Real(1.0)),
            "`1 h` is `3600.0` in `s`, but the value is Real `1.0`",
        ),
        (
            |a| {
                a.suffix = Some("d".to_owned());
                a.scaled_from = Some(Literal::Real(1e308));
            },
            "`1e308 d` is not finite once scaled to `s`",
        ),
        (
            |a| a.value = ParamValue::Literal(Literal::Boolean(true)),
            "the suffix `h` follows a value that is not a number",
        ),
        (
            |a| a.suffix = Some("degC".to_owned()),
            "`degC` in `degC` is not a base unit; a suffix is built from base units, or is one of the scaled time units `min`, `h` and `d` alone (affine, prefixed and other scaled units are refused)",
        ),
    ];
    let base = parse_text(&delay("1 h")).expect("parses");
    assert_eq!(validate(&base), vec![]);
    for (edit, why) in cases {
        let mut model = base.clone();
        edit(&mut model.blocks[0].assignments[0]);
        let findings = validate(&model);
        let found: Vec<(&str, &str, &str)> = findings
            .iter()
            .map(|f| (f.code, f.subject.as_str(), f.detail.as_str()))
            .collect();
        assert_eq!(
            found,
            [(
                UNIT_SUFFIX_MISMATCH,
                "del.delayTime",
                format!("{what}: {why}").as_str()
            )],
            "{why}"
        );
        // Whatever stage 1 refuses, the emitter refuses to write.
        assert!(emit(&model, &EmitOptions::default()).is_err(), "{why}");
    }
    // The same rule on a sequence parameter.
    let mut param = parse_text(&format!("{HEAD}param p : Real = 2 min\n")).expect("parses");
    param.params[0].value = Literal::Real(2.0);
    let findings = validate(&param);
    let on_p: Vec<(&str, &str)> = findings
        .iter()
        .filter(|f| f.subject == "p")
        .map(|f| (f.code, f.detail.as_str()))
        .collect();
    assert_eq!(
        on_p,
        [(
            UNIT_SUFFIX_MISMATCH,
            "sequence parameter `p`: `2 min` is `120.0` in `s`, but the value is Real `2.0`"
        )]
    );
}

/// A Real is scaled from the binary64 it parses to, through that value's shortest round-trip
/// decimal, never from the digits as typed (so `fmt`, which prints that decimal, never moves a
/// value). Two consequences are pinned: a Real typed with more digits than binary64 holds scales
/// as its shortest spelling does, and an Integer and a Real spelling of one number past 2^53 scale
/// differently, because the Real is rounded when it is parsed and the Integer is not.
#[test]
fn a_real_scales_from_its_binary64_value_and_an_integer_from_its_digits() {
    let bits = |written: &str| real(&parse_text(&delay(written)).expect(written)).to_bits();
    // The Integer: the exact product 540431955284459580, rounded once.
    assert_eq!(
        bits("9007199254740993 min"),
        540_431_955_284_459_584.0_f64.to_bits()
    );
    // The Real spellings parse to 2^53 first, so they scale to 2^53 minutes.
    for real_spelling in ["9007199254740993.0 min", "9.007199254740993e15 min"] {
        assert_eq!(
            bits(real_spelling),
            540_431_955_284_459_520.0_f64.to_bits(),
            "{real_spelling}"
        );
    }
    // Digits past binary64's precision: scaled as the shortest spelling (`1.1`, `0.1`, `1.5`) is.
    assert_eq!(bits("1.10000000000000009 h"), 3_960.0_f64.to_bits());
    assert_eq!(bits("0.10000000000000001 h"), 360.0_f64.to_bits());
    assert_eq!(bits("1.4999999999999999 d"), 129_600.0_f64.to_bits());
    // An exact Integer midpoint: 4803839602528536 min is 288230376151712160 s, halfway between
    // ...128 and ...192; ties to even gives ...128.
    assert_eq!(bits("4803839602528536 min"), 0x4390_0000_0000_0006);
    assert_eq!(
        f64::from_bits(0x4390_0000_0000_0006),
        288_230_376_151_712_128.0
    );
}

#[test]
fn a_scaled_literal_at_the_token_bound_is_read_and_one_past_it_is_refused() {
    let at_bound = format!("1.{}", "0".repeat(254));
    assert_eq!(at_bound.len(), oce_author::MAX_TOKEN_BYTES);
    let model = parse_text(&delay(&format!("{at_bound} h"))).expect("at the bound");
    assert_eq!(real(&model).to_bits(), 3_600.0_f64.to_bits());
    assert_eq!(delay_time(&model).scaled_from, Some(Literal::Real(1.0)));
    // `fmt` prints the shortest spelling, well inside the bound.
    assert!(
        fmt_text(&delay(&format!("{at_bound} h")))
            .expect("formats")
            .contains("delayTime = 1.0 h)")
    );
    let past = format!("{at_bound}0 h");
    let error = parse_text(&delay(&past)).expect_err("one past the bound");
    assert_eq!(
        (error.code, error.location.line, error.location.column),
        (oce_author::TOKEN_TOO_LONG, 5, 47)
    );
}
