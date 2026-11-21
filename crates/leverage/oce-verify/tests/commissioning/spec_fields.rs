//! What each value of a spec must be (commissioning note, "Test-spec file"): paths, the time
//! origin, tolerances, sampling, the window, a source's time column and hold, the closed
//! choices, names, and where a number is exact. Each rule is shown at its edge, with the code
//! and the line and column of the value refused.

use oce_verify::{
    Envelope, FunnelReference, Literal, MAX_TIME_INTEGER, SourceTimeColumn, TrendFormat,
};

use crate::{code_of, loaded, one_reference, refusal, top_level};

/// A document built around one value.
type Document = fn(&str) -> String;

/// A reference with one `inputs` source whose members are `members`.
fn one_source(members: &str) -> String {
    one_reference(&format!("\"inputs\": [{{{members}}}]"))
}

/// A complete csv source with `extra` members added.
fn csv_source(extra: &str) -> String {
    let separator = if extra.is_empty() { "" } else { ", " };
    one_source(&format!(
        "\"path\": \"t.csv\", \"format\": \"csv\", \"timeColumn\": \"time\", \"columns\": {{}}{separator}{extra}"
    ))
}

#[test]
fn a_path_is_relative_and_cannot_leave_the_specs_directory() {
    // (the path, why it is refused), for every field that holds a path.
    let bad = [
        ("/abs/x.json", "is absolute"),
        ("/", "is absolute"),
        ("../x.json", "has a `..` segment"),
        ("a/../../x.json", "has a `..` segment"),
        ("a/..", "has a `..` segment"),
        ("./x.json", "has a `.` segment"),
        ("a/./x.json", "has a `.` segment"),
        ("", "has an empty segment"),
        ("a//x.json", "has an empty segment"),
        ("a/", "has an empty segment"),
        ("a\\\\x.json", "holds a backslash"),
        ("..\\\\x.json", "holds a backslash"),
    ];
    let fields: [(&str, Document); 5] = [
        ("the cxf path", |path| {
            one_reference(&format!("\"cxf\": \"{path}\""))
        }),
        ("the controllerOutput path", |path| {
            one_reference(&format!("\"controllerOutput\": \"{path}\""))
        }),
        ("the pointNameMapping path", |path| {
            one_reference(&format!("\"pointNameMapping\": \"{path}\""))
        }),
        ("the source path", |path| {
            one_source(&format!(
                "\"path\": \"{path}\", \"format\": \"csv\", \"timeColumn\": \"t\", \"columns\": {{}}"
            ))
        }),
        ("the source path", |path| {
            one_reference(&format!(
                "\"indicatorSources\": [{{\"path\": \"{path}\", \"format\": \"csv\", \"timeColumn\": \"t\", \"columns\": {{}}}}]"
            ))
        }),
    ];
    for (what, document) in fields {
        for (path, why) in bad {
            let text = document(path);
            let error = crate::load(&text).expect_err("refused");
            assert_eq!(error.code(), "path-invalid", "{what}: {path}");
            assert!(error.detail().starts_with(what), "{}", error.detail());
            assert!(error.detail().contains(why), "{}", error.detail());
            // The place is the path's own string.
            let at = error.at().expect("a place");
            let column = usize::try_from(at.column).expect("fits") - 1;
            assert_eq!(&text[column..=column], "\"", "{text}");
        }
        for good in [
            "x.json",
            "a/b/c.json",
            "a b/c",
            "...",
            "..a/b..",
            ".hidden",
            "C:x",
        ] {
            assert_eq!(code_of(&document(good)), "loaded", "{what}: {good}");
        }
    }
    // A segment may hold anything else, a name that only looks like a parent among them.
    let reference = &loaded(&one_reference("\"cxf\": \"a/..b/c\"")).references()[0].clone();
    assert_eq!(reference.cxf(), Some("a/..b/c"));
    // The fields the verifier ignores hold no path it will open: nothing in them is judged.
    assert_eq!(
        code_of(&top_level("\"modelJsonDirectory\": \"/abs/../x\\\\y\"")),
        "loaded"
    );
}

#[test]
fn the_time_origin_is_an_instant_with_an_explicit_offset() {
    for good in [
        "2018-06-07T00:00:00-07:00",
        "2018-06-07T07:00:00Z",
        "2018-06-07T00:00:00.123456789+05:30",
        "0000-01-01T00:00:00Z",
        "9999-12-31T23:59:59+23:59",
        "2024-02-29T12:00:00Z",
    ] {
        let spec = loaded(&top_level(&format!("\"time\": {{\"origin\": \"{good}\"}}")));
        assert_eq!(spec.time_origin().map(|origin| origin.as_str()), Some(good));
    }
    for bad in [
        "",
        "2018-06-07",
        "2018-06-07T00:00:00",
        "2018-06-07 00:00:00Z",
        "2018-06-07t00:00:00Z",
        "2018-06-07T00:00:00z",
        "2018-06-07T00:00:00 PDT",
        "2018-06-07T00:00:00-0700",
        "2018-06-07T00:00:00-7:00",
        "2018-13-01T00:00:00Z",
        "2018-02-30T00:00:00Z",
        "2023-02-29T00:00:00Z",
        "2018-06-07T24:00:00Z",
        "2018-06-07T23:59:60Z",
        "2018-06-07T00:00:00.Z",
        "2018-06-07T00:00:00.1234567890Z",
        "2018-06-07T00:00:00+24:00",
        " 2018-06-07T00:00:00Z",
        "2018-06-07T00:00:00Z ",
        "1528354800",
    ] {
        let text = top_level(&format!("\"time\": {{\"origin\": \"{bad}\"}}"));
        assert_eq!(code_of(&text), "invalid-time-origin", "{bad:?}");
    }
    // Two spellings of one instant are one instant and two documents.
    let pdt = loaded(&top_level(
        "\"time\": {\"origin\": \"2018-06-07T00:00:00-07:00\"}",
    ));
    let utc = loaded(&top_level(
        "\"time\": {\"origin\": \"2018-06-07T07:00:00Z\"}",
    ));
    assert_eq!(
        pdt.time_origin().map(|origin| origin.unix_nanos()),
        utc.time_origin().map(|origin| origin.unix_nanos())
    );
    assert_ne!(pdt.spec_id(), utc.spec_id());
    // The refusal sits on the origin's string: line 1, after `{"references": [], "time": {"origin": `.
    let text = top_level("\"time\": {\"origin\": \"noon\"}");
    assert_eq!(refusal(&text), ("invalid-time-origin", 1, 39));
    assert_eq!(&text[38..], "\"noon\"}}");
    // `time` is a closed object that must hold its origin, and the origin is a string.
    assert_eq!(code_of(&top_level("\"time\": {}")), "missing-key");
    assert_eq!(
        code_of(&top_level("\"time\": {\"origin\": 0}")),
        "wrong-type"
    );
    assert_eq!(
        code_of(&top_level("\"time\": \"2018-06-07T00:00:00Z\"")),
        "wrong-type"
    );
}

#[test]
fn a_tolerance_is_a_number_that_is_not_negative() {
    for (value, expected) in [
        ("0", "loaded"),
        ("0.0", "loaded"),
        ("-0", "loaded"),
        ("-0.0", "loaded"),
        ("5e-324", "loaded"),
        ("1.7976931348623157e308", "loaded"),
        ("-5e-324", "invalid-tolerance"),
        ("-1", "invalid-tolerance"),
        ("-0.002", "invalid-tolerance"),
        ("1e999", "non-finite-number"),
        ("NaN", "non-finite-number"),
        ("Infinity", "non-finite-number"),
        ("\"0.5\"", "wrong-type"),
        ("true", "wrong-type"),
        ("null", "wrong-type"),
        ("[0.5]", "wrong-type"),
    ] {
        for name in oce_verify::TOLERANCE_NAMES {
            // At the top level and under an `outputs` key alike.
            let global = top_level(&format!("\"tolerances\": {{\"{name}\": {value}}}"));
            assert_eq!(code_of(&global), expected, "{name}: {value}");
            let keyed = one_reference(&format!(
                "\"outputs\": {{\"s.y\": {{\"{name}\": {value}}}}}"
            ));
            assert_eq!(code_of(&keyed), expected, "{name}: {value}");
        }
    }
    // The refusal names the tolerance and sits on its value.
    let text = top_level("\"tolerances\": {\"atolx\": 1, \"rtoly\": -0.5}");
    let error = crate::load(&text).expect_err("refused");
    assert_eq!(error.detail(), "tolerance `rtoly` must not be negative");
    assert_eq!(refusal(&text), ("invalid-tolerance", 1, 56));
    assert_eq!(&text[55..], "-0.5}}");
    // A tolerance object is closed over the six names, and each is written once.
    assert_eq!(
        code_of(&top_level("\"tolerances\": {\"atol\": 1}")),
        "unknown-key"
    );
    assert_eq!(
        code_of(&top_level("\"tolerances\": {\"ATOLX\": 1}")),
        "unknown-key"
    );
    assert_eq!(
        code_of(&top_level("\"tolerances\": {\"variable\": \"TSup*\"}")),
        "unknown-key"
    );
    assert_eq!(
        code_of(&top_level("\"tolerances\": {\"atolx\": 1, \"atolx\": 1}")),
        "duplicate-key"
    );
    assert_eq!(code_of(&top_level("\"tolerances\": {}")), "loaded");
    assert_eq!(code_of(&top_level("\"tolerances\": []")), "wrong-type");
    // Whatever tolerance a spec loads, neither envelope refuses it as a tolerance: the edges of
    // what the reader accepts are inside what the comparison requires.
    for value in [
        "0",
        "-0.0",
        "5e-324",
        "0.002",
        "10",
        "1.7976931348623157e308",
    ] {
        let members: Vec<String> = oce_verify::TOLERANCE_NAMES
            .iter()
            .map(|name| format!("\"{name}\": {value}"))
            .collect();
        let spec = loaded(&top_level(&format!(
            "\"tolerances\": {{{}}}",
            members.join(", ")
        )));
        let tolerances = spec.tolerances().expect("present").resolved();
        let series = oce_verify::Series::new(&[0.0, 1.0], &[0.0, 1.0]);
        for envelope in oce_verify::Envelope::ALL {
            let compared = envelope.compare(
                series,
                series,
                &tolerances,
                &oce_verify::CompareLimits::default(),
            );
            assert!(
                !matches!(
                    compared,
                    Err(oce_verify::CompareError::InvalidTolerance { .. })
                ),
                "{value} under {envelope}: {compared:?}"
            );
        }
    }
    // Absent is not zero: an empty object and an explicit zero are different documents.
    let empty = loaded(&top_level("\"tolerances\": {}"));
    let zero = loaded(&top_level("\"tolerances\": {\"atoly\": 0}"));
    assert_eq!(empty.tolerances().and_then(|t| t.atoly()), None);
    assert_eq!(zero.tolerances().and_then(|t| t.atoly()), Some(0.0));
    assert_ne!(empty.spec_id(), zero.spec_id());
}

#[test]
fn a_sampling_is_above_zero() {
    for (value, expected) in [
        ("120", "loaded"),
        ("0.5", "loaded"),
        ("5e-324", "loaded"),
        ("0", "invalid-sampling"),
        ("0.0", "invalid-sampling"),
        ("-0.0", "invalid-sampling"),
        ("-60", "invalid-sampling"),
        ("-5e-324", "invalid-sampling"),
        ("\"60\"", "wrong-type"),
        ("null", "wrong-type"),
    ] {
        assert_eq!(
            code_of(&top_level(&format!("\"sampling\": {value}"))),
            expected,
            "top level: {value}"
        );
        assert_eq!(
            code_of(&one_reference(&format!("\"sampling\": {value}"))),
            expected,
            "reference: {value}"
        );
    }
    let text = top_level("\"sampling\": 0");
    assert_eq!(refusal(&text), ("invalid-sampling", 1, 32));
    assert_eq!(&text[31..], "0}");
    // A reference's own sampling wins over the spec's, and neither is invented.
    let spec = loaded(
        r#"{"sampling": 120, "references": [{"model": "M", "sequence": "a", "sampling": 60},
            {"model": "M", "sequence": "b"}]}"#,
    );
    assert_eq!(spec.sampling_for(&spec.references()[0]), Some(60.0));
    assert_eq!(spec.sampling_for(&spec.references()[1]), Some(120.0));
    let bare = loaded(&one_reference(""));
    assert_eq!(bare.sampling_for(&bare.references()[0]), None);
}

#[test]
fn a_window_holds_both_ends_in_order() {
    for (window, expected) in [
        ("{\"start\": 0, \"stop\": 86400}", "loaded"),
        ("{\"stop\": 86400, \"start\": 0}", "loaded"),
        ("{\"start\": 5, \"stop\": 5}", "loaded"),
        ("{\"start\": -3600, \"stop\": 0}", "loaded"),
        ("{\"start\": -0.0, \"stop\": 0}", "loaded"),
        ("{\"start\": 0, \"stop\": -0.0}", "loaded"),
        (
            "{\"start\": 1e308, \"stop\": 1.7976931348623157e308}",
            "loaded",
        ),
        (
            "{\"start\": 5, \"stop\": 4.999999999999999}",
            "invalid-window",
        ),
        ("{\"start\": 1, \"stop\": 0}", "invalid-window"),
        ("{\"start\": 0, \"stop\": -5e-324}", "invalid-window"),
        ("{\"start\": 0}", "missing-key"),
        ("{\"stop\": 0}", "missing-key"),
        ("{}", "missing-key"),
        ("{\"start\": 0, \"stop\": 1, \"step\": 1}", "unknown-key"),
        ("{\"start\": 0, \"stop\": 1, \"start\": 0}", "duplicate-key"),
        ("{\"start\": \"0\", \"stop\": 1}", "wrong-type"),
        ("{\"start\": 0, \"stop\": null}", "wrong-type"),
        ("{\"start\": 0, \"stop\": 1e999}", "non-finite-number"),
        ("[0, 86400]", "wrong-type"),
    ] {
        let text = one_reference(&format!("\"window\": {window}"));
        assert_eq!(code_of(&text), expected, "{window}");
    }
    // The refusal of a window out of order sits on the window, which holds both ends.
    let text = one_reference("\"window\": {\"start\": 2, \"stop\": 1}");
    assert_eq!(refusal(&text), ("invalid-window", 1, 59));
    let error = crate::load(&text).expect_err("refused");
    assert_eq!(error.detail(), "start must not be after stop");
    // The window is optional to load: an LBL spec has none.
    assert_eq!(loaded(&one_reference("")).references()[0].window(), None);
}

#[test]
fn a_number_read_as_a_real_is_exact_or_refused() {
    // An integer token is held to 2^53, where it still names the value it spells.
    let limit = MAX_TIME_INTEGER;
    for (field, document) in [
        (
            "sampling",
            (|v| top_level(&format!("\"sampling\": {v}"))) as fn(&str) -> String,
        ),
        ("tolerance", |v| {
            top_level(&format!("\"tolerances\": {{\"atolx\": {v}}}"))
        }),
        ("window", |v| {
            one_reference(&format!("\"window\": {{\"start\": 0, \"stop\": {v}}}"))
        }),
        ("maxAge", |v| {
            csv_source(&format!("\"hold\": {{\"maxAge\": {v}}}"))
        }),
    ] {
        assert_eq!(code_of(&document(&limit.to_string())), "loaded", "{field}");
        assert_eq!(
            code_of(&document(&(limit + 1).to_string())),
            "integer-out-of-range",
            "{field}"
        );
        assert_eq!(
            code_of(&document("9223372036854775808")),
            "integer-out-of-range",
            "{field}: outside i64"
        );
        // The same magnitude written as a real rounds as every real does, and loads.
        assert_eq!(
            code_of(&document("9007199254740993.0")),
            "loaded",
            "{field}"
        );
        assert_eq!(code_of(&document("1e300")), "loaded", "{field}");
    }
    // A negative integer past the range is refused for its range before its sign is judged.
    assert_eq!(
        code_of(&top_level(&format!("\"sampling\": {}", -(limit + 1)))),
        "integer-out-of-range"
    );
    assert_eq!(
        code_of(&one_reference(&format!(
            "\"window\": {{\"start\": {}, \"stop\": 0}}",
            -limit
        ))),
        "loaded"
    );
    // A parameter value is not read as a real here: an integer token keeps all of i64.
    let spec = loaded(&one_reference(
        "\"parameters\": {\"a\": 9223372036854775807, \"b\": -9223372036854775808, \"c\": -0, \"d\": -0.0, \"e\": false}",
    ));
    let values: Vec<Literal> = spec.references()[0]
        .parameters()
        .expect("present")
        .iter()
        .map(|each| each.value())
        .collect();
    assert_eq!(
        values,
        [
            Literal::Integer(i64::MAX),
            Literal::Integer(i64::MIN),
            Literal::Integer(0),
            Literal::Real(-0.0),
            Literal::Boolean(false),
        ]
    );
    for bad in ["\"1\"", "null", "[1]", "{}"] {
        assert_eq!(
            code_of(&one_reference(&format!("\"parameters\": {{\"a\": {bad}}}"))),
            "wrong-type",
            "{bad}"
        );
    }
}

#[test]
fn a_time_column_is_a_position_from_one_or_a_header_name() {
    let column = |value: &str| {
        one_source(&format!(
            "\"path\": \"t\", \"format\": \"csv\", \"timeColumn\": {value}, \"columns\": {{}}"
        ))
    };
    for (value, expected) in [
        ("1", SourceTimeColumn::Index(1)),
        ("3", SourceTimeColumn::Index(3)),
        ("4294967295", SourceTimeColumn::Index(u32::MAX)),
        ("\"time\"", SourceTimeColumn::Name("time".to_owned())),
        ("\"1\"", SourceTimeColumn::Name("1".to_owned())),
        (
            "\"Time Stamp (PDT)\"",
            SourceTimeColumn::Name("Time Stamp (PDT)".to_owned()),
        ),
    ] {
        let spec = loaded(&column(value));
        let inputs = spec.references()[0].inputs().expect("present");
        assert_eq!(inputs[0].time_column(), &expected, "{value}");
    }
    for (value, expected) in [
        ("0", "integer-out-of-range"),
        ("-0", "integer-out-of-range"),
        ("-1", "integer-out-of-range"),
        ("4294967296", "integer-out-of-range"),
        ("9223372036854775807", "integer-out-of-range"),
        ("9223372036854775808", "integer-out-of-range"),
        ("1.0", "wrong-type"),
        ("1e0", "wrong-type"),
        ("1.5", "wrong-type"),
        ("true", "wrong-type"),
        ("null", "wrong-type"),
        ("[1]", "wrong-type"),
        ("{}", "wrong-type"),
        ("\"\"", "empty-name"),
    ] {
        assert_eq!(code_of(&column(value)), expected, "{value}");
    }
    let text = column("0");
    let (code, line, at) = refusal(&text);
    assert_eq!((code, line), ("integer-out-of-range", 1));
    assert_eq!(&text[usize::try_from(at).expect("fits") - 1..][..1], "0");
}

#[test]
fn a_source_names_its_file_its_format_its_time_and_its_columns() {
    let spec = loaded(&csv_source(""));
    let source = &spec.references()[0].inputs().expect("present")[0];
    assert_eq!(source.format(), TrendFormat::Csv);
    assert_eq!((source.table(), source.hold_max_age_s()), (None, None));
    // Each of the four is required; `table` and `hold` are not.
    for (key, value) in [
        ("path", "\"t.csv\""),
        ("format", "\"csv\""),
        ("timeColumn", "\"time\""),
        ("columns", "{}"),
    ] {
        let complete = [
            ("path", "\"t.csv\""),
            ("format", "\"csv\""),
            ("timeColumn", "\"time\""),
            ("columns", "{}"),
        ];
        let without: Vec<String> = complete
            .iter()
            .filter(|(each, _)| *each != key)
            .map(|(each, value)| format!("\"{each}\": {value}"))
            .collect();
        let text = one_source(&without.join(", "));
        let error = crate::load(&text).expect_err("refused");
        assert_eq!(error.code(), "missing-key", "{key}");
        assert_eq!(error.detail(), format!("missing key `{key}`"));
        // And each takes one JSON type.
        let wrong = if value.starts_with('"') { "7" } else { "\"x\"" };
        let mut members: Vec<String> = without.clone();
        members.push(format!("\"{key}\": {wrong}"));
        let expected = if key == "timeColumn" {
            "loaded"
        } else {
            "wrong-type"
        };
        assert_eq!(code_of(&one_source(&members.join(", "))), expected, "{key}");
    }
    assert_eq!(code_of(&csv_source("\"table\": \"tab1\"")), "loaded");
    assert_eq!(code_of(&csv_source("\"table\": \"\"")), "empty-name");
    assert_eq!(code_of(&csv_source("\"table\": 1")), "wrong-type");
    assert_eq!(code_of(&csv_source("\"delimiter\": \";\"")), "unknown-key");
    assert_eq!(code_of(&csv_source("\"hold\": {\"maxAge\": 0}")), "loaded");
    assert_eq!(
        code_of(&csv_source("\"hold\": {\"maxAge\": -0.0}")),
        "loaded"
    );
    assert_eq!(
        code_of(&csv_source("\"hold\": {\"maxAge\": -5e-324}")),
        "invalid-hold"
    );
    assert_eq!(
        code_of(&csv_source("\"hold\": {\"maxAge\": -1}")),
        "invalid-hold"
    );
    assert_eq!(code_of(&csv_source("\"hold\": {}")), "missing-key");
    assert_eq!(code_of(&csv_source("\"hold\": 10")), "wrong-type");
    assert_eq!(
        code_of(&csv_source(
            "\"hold\": {\"maxAge\": 1, \"policy\": \"last\"}"
        )),
        "unknown-key"
    );
    let text = csv_source("\"hold\": {\"maxAge\": -2}");
    let (code, _, at) = refusal(&text);
    assert_eq!(code, "invalid-hold");
    assert_eq!(&text[usize::try_from(at).expect("fits") - 1..][..2], "-2");
    // A columns map takes names to names; neither side is empty, and no column is named twice.
    assert_eq!(
        code_of(&one_source(
            "\"path\": \"t\", \"format\": \"csv\", \"timeColumn\": \"t\", \"columns\": {\"a\": \"\"}"
        )),
        "empty-name"
    );
    assert_eq!(
        code_of(&one_source(
            "\"path\": \"t\", \"format\": \"csv\", \"timeColumn\": \"t\", \"columns\": {\"\": \"a\"}"
        )),
        "empty-name"
    );
    assert_eq!(
        code_of(&one_source(
            "\"path\": \"t\", \"format\": \"csv\", \"timeColumn\": \"t\", \"columns\": {\"a\": 3}"
        )),
        "wrong-type"
    );
    assert_eq!(
        code_of(&one_source(
            "\"path\": \"t\", \"format\": \"csv\", \"timeColumn\": \"t\", \"columns\": {\"a\": \"x\", \"a\": \"y\"}"
        )),
        "duplicate-key"
    );
    // The three source lists are read by one reader.
    for list in ["inputs", "expected", "indicatorSources"] {
        let text = one_reference(&format!(
            "\"{list}\": [{{\"path\": \"t\", \"format\": \"zip\", \"timeColumn\": 1, \"columns\": {{}}}}]"
        ));
        assert_eq!(code_of(&text), "unknown-value", "{list}");
        assert_eq!(
            code_of(&one_reference(&format!("\"{list}\": {{}}"))),
            "wrong-type"
        );
        assert_eq!(
            code_of(&one_reference(&format!("\"{list}\": [[]]"))),
            "wrong-type"
        );
        assert_eq!(
            code_of(&one_reference(&format!("\"{list}\": []"))),
            "loaded"
        );
    }
}

#[test]
fn a_closed_choice_takes_its_spellings_and_no_other() {
    for (value, expected) in [
        ("funnel", Some(Envelope::Funnel)),
        ("aligned-grid", Some(Envelope::AlignedGrid)),
        ("Funnel", None),
        ("aligned_grid", None),
        ("alignedGrid", None),
        ("", None),
        ("funnel ", None),
    ] {
        let text = one_reference(&format!("\"envelope\": \"{value}\""));
        match expected {
            Some(envelope) => {
                assert_eq!(loaded(&text).references()[0].envelope(), Some(envelope));
            }
            None => assert_eq!(code_of(&text), "unknown-value", "{value:?}"),
        }
    }
    for (value, expected) in [
        ("engine", Some(FunnelReference::Engine)),
        ("trend", Some(FunnelReference::Trend)),
        ("Engine", None),
        ("trended", None),
        ("expectation", None),
    ] {
        let text = one_reference(&format!("\"funnelReference\": \"{value}\""));
        match expected {
            Some(role) => {
                assert_eq!(loaded(&text).references()[0].funnel_reference(), Some(role));
            }
            None => assert_eq!(code_of(&text), "unknown-value", "{value:?}"),
        }
    }
    for (format, expected) in [
        ("csv", "loaded"),
        ("combiTimeTable", "loaded"),
        ("CSV", "unknown-value"),
        ("combitimetable", "unknown-value"),
        ("mos", "unknown-value"),
        ("parquet", "unknown-value"),
    ] {
        let text = one_source(&format!(
            "\"path\": \"t\", \"format\": \"{format}\", \"timeColumn\": 1, \"columns\": {{}}"
        ));
        assert_eq!(code_of(&text), expected, "{format}");
    }
    // The sentence names what was written and what is accepted, and sits on the value.
    let text = one_reference("\"envelope\": \"tube\"");
    let error = crate::load(&text).expect_err("refused");
    assert_eq!(
        error.detail(),
        "the envelope is `tube`; expected `funnel` or `aligned-grid`"
    );
    assert_eq!(refusal(&text), ("unknown-value", 1, 61));
    // A choice is a string: a boolean or a number is the wrong type, not an unknown value.
    assert_eq!(code_of(&one_reference("\"envelope\": true")), "wrong-type");
    assert_eq!(
        code_of(&one_reference("\"funnelReference\": 0")),
        "wrong-type"
    );
}

#[test]
fn a_name_is_never_empty() {
    for text in [
        "{\"references\": [{\"model\": \"M\", \"sequence\": \"\"}]}".to_owned(),
        one_reference("\"outputs\": {\"\": {}}"),
        one_reference("\"indicators\": {\"\": []}"),
        one_reference("\"indicators\": {\"s.y\": [\"\"]}"),
        one_reference("\"indicators\": {\"s.y\": [\"s.u\", \"\"]}"),
        one_reference("\"parameters\": {\"\": 1}"),
    ] {
        assert_eq!(code_of(&text), "empty-name", "{text}");
    }
    // A model is a label: it may be empty. A sequence prefixes every key: it may not.
    assert_eq!(
        code_of("{\"references\": [{\"model\": \"\", \"sequence\": \"s\"}]}"),
        "loaded"
    );
    // Both are required, and both are strings.
    assert_eq!(
        code_of("{\"references\": [{\"model\": \"M\"}]}"),
        "missing-key"
    );
    assert_eq!(
        code_of("{\"references\": [{\"sequence\": \"s\"}]}"),
        "missing-key"
    );
    assert_eq!(
        code_of("{\"references\": [{\"model\": 1, \"sequence\": \"s\"}]}"),
        "wrong-type"
    );
    assert_eq!(
        code_of("{\"references\": [{\"model\": \"M\", \"sequence\": [\"s\"]}]}"),
        "wrong-type"
    );
    // An indicator name is a string, and an indicator list a list.
    assert_eq!(
        code_of(&one_reference("\"indicators\": {\"s.y\": \"s.u\"}")),
        "wrong-type"
    );
    assert_eq!(
        code_of(&one_reference("\"indicators\": {\"s.y\": [1]}")),
        "wrong-type"
    );
    // An empty spec is a spec: it has references, and none of them.
    assert_eq!(code_of("{\"references\": []}"), "loaded");
    assert_eq!(code_of("{}"), "missing-key");
    assert_eq!(code_of("{\"references\": {}}"), "wrong-type");
    assert_eq!(code_of("[]"), "wrong-type");
}

#[test]
fn the_point_mapping_is_a_path_or_the_mapping_itself() {
    let inline = one_reference(
        "\"pointNameMapping\": [{\"cdl\": {\"name\": \"u\", \"type\": \"float\"}, \"device\": {\"name\": \"U\", \"type\": \"float\"}}]",
    );
    assert_eq!(code_of(&inline), "loaded");
    assert_eq!(
        code_of(&one_reference("\"pointNameMapping\": []")),
        "loaded"
    );
    assert_eq!(
        code_of(&one_reference("\"pointNameMapping\": \"points.json\"")),
        "loaded"
    );
    for wrong in ["{}", "1", "true", "null"] {
        let text = one_reference(&format!("\"pointNameMapping\": {wrong}"));
        assert_eq!(code_of(&text), "wrong-type", "{wrong}");
    }
    // A defect of an inline mapping is refused where it is written in the spec.
    let text = one_reference(
        "\"pointNameMapping\": [\n{\"cdl\": {\"name\": \"u\", \"type\": \"real\"}, \"device\": {\"name\": \"U\", \"type\": \"float\"}}]",
    );
    assert_eq!(refusal(&text), ("unknown-value", 2, 31));
    assert!(text.lines().nth(1).expect("the second line")[30..].starts_with("\"real\""));
}
