//! Every object of the format is closed: an unknown key is refused at every level (the keys the
//! format does not have yet among them), a required key cannot be absent, a value of another
//! JSON type is refused, an expectation is exactly one of its two forms, and any tag but
//! `oce/vectors/v1` is refused by name before the shape is judged.

mod support;

use oce_verify::{Expectation, Literal};

use support::{code_of, fixture_bytes, load, one, refusal, scenario, set_of};

const WINDOW: &str = r#"{"output": "y", "from_s": 0, "to_s": 1800, "equals": false}"#;
const INSTANT: &str =
    r#"{"diagnostic": {"block": "CDL.Utilities.Assert", "message": "m"}, "at_s": 60}"#;

/// A set with every optional key present, so each object can gain or lose a key.
fn full() -> String {
    format!(
        "{{\"schema\": \"oce/vectors/v1\", \"clock\": {{\"step_s\": 60, \"horizon_s\": 1800}}, \"params\": {{\"a.k\": 1}}, \"scenarios\": [{{\"name\": \"s\", \"description\": \"d\", \"params\": {{\"b.k\": 2.5}}, \"inputs\": {{\"u\": [{{\"t\": 0, \"value\": 1}}], \"v\": true}}, \"expect\": [{WINDOW}, {INSTANT}]}}]}}"
    )
}

/// `document` with `addition` written as the first member of the first object that starts with
/// `anchor`.
fn with_member(document: &str, anchor: &str, addition: &str) -> String {
    assert!(document.contains(anchor), "{anchor}");
    document.replacen(anchor, &format!("{{{addition}, {}", &anchor[1..]), 1)
}

#[test]
fn the_full_document_loads_with_every_optional_key() {
    let set = load(&full()).expect("loads");
    assert_eq!(set.params().map(<[_]>::len), Some(1));
    let scenario = &set.scenarios()[0];
    assert_eq!(scenario.description(), Some("d"));
    assert_eq!(scenario.params().map(<[_]>::len), Some(1));
    assert_eq!(scenario.inputs().len(), 2);
    assert_eq!(scenario.expectations().len(), 2);
}

#[test]
fn an_unknown_key_is_refused_in_every_object() {
    // (the object's first bytes, a key that object does not define)
    let objects = [
        ("{\"schema\"", "version"),
        ("{\"step_s\"", "t0_s"),
        ("{\"name\"", "tags"),
        ("{\"t\"", "at"),
        ("{\"output\"", "tolerance"),
        ("{\"diagnostic\"", "code"),
        ("{\"block\"", "instance"),
    ];
    for (anchor, key) in objects {
        let document = with_member(&full(), anchor, &format!("\"{key}\": 1"));
        let refused = load(&document).expect_err("refused");
        assert_eq!(refused.code(), "unknown-key", "{key} in {anchor}");
        assert_eq!(refused.detail(), format!("unknown key `{key}`"));
    }
    // The same member is refused whatever its value: nothing unknown is skipped.
    for value in ["null", "[]", "{}", "\"x\"", "true", "[[[1]]]"] {
        let document = with_member(&full(), "{\"name\"", &format!("\"extra\": {value}"));
        assert_eq!(code_of(&document), "unknown-key", "{value}");
    }
    // Keys are compared exactly: case and surrounding space matter.
    for near in ["Schema", "schema ", "CLOCK", "Scenarios"] {
        let document = with_member(&full(), "{\"schema\"", &format!("\"{near}\": 1"));
        assert_eq!(code_of(&document), "unknown-key", "{near}");
    }
}

#[test]
fn the_keys_the_format_does_not_have_yet_are_refused_never_ignored() {
    // `within`, `of` and `when` belong to an expectation, `mutants` to the set: each is an
    // unknown key today, in the object it will belong to and anywhere else.
    for (anchor, member) in [
        ("{\"schema\"", r#""mutants": []"#),
        (
            "{\"schema\"",
            r#""mutants": [{"path": "mutants/late_alarm.cxf.json"}]"#,
        ),
        ("{\"output\"", r#""within": {"atoly": 0.5}"#),
        ("{\"output\"", r#""of": "yRef""#),
        ("{\"output\"", r#""when": ["occupied"]"#),
        ("{\"diagnostic\"", r#""when": ["occupied"]"#),
        ("{\"name\"", r#""when": ["occupied"]"#),
        ("{\"name\"", r#""mutants": []"#),
        ("{\"output\"", r#""tolerance": 1e-9"#),
    ] {
        let document = with_member(&full(), anchor, member);
        assert_eq!(code_of(&document), "unknown-key", "{member}");
    }
    assert_eq!(code_of(&full()), "loaded");
}

/// The design note, read in place. `scripts/ci/classify-changes.sh` lists it as a document a
/// test reads, so a change to the note runs this crate's suite.
const VECTORS_NOTE: &str = include_str!("../../../../docs/design/vectors.md");

/// The first `json` block after the note's "The vector format" heading: its worked example.
fn worked_example_in_the_note() -> &'static str {
    let section = VECTORS_NOTE
        .split_once("\n## The vector format\n")
        .expect("the note has a section named The vector format")
        .1;
    let block = section
        .split_once("\n```json\n")
        .expect("the section has a json block")
        .1;
    let end = block.find("\n```\n").expect("the json block is closed");
    &block[..=end]
}

#[test]
fn the_worked_example_fixture_is_the_notes_block_byte_for_byte() {
    let fixture = String::from_utf8(fixture_bytes("note/worked_example.json")).expect("UTF-8");
    let noted = worked_example_in_the_note();
    assert!(noted.starts_with("{\n  \"schema\": \"oce/vectors/v1\",\n"));
    assert!(noted.ends_with("\n}\n"));
    assert!(
        fixture == noted,
        "tests/fixtures/note/worked_example.json is no longer the worked example of docs/design/vectors.md; copy the block again\n--- note\n{noted}\n--- fixture\n{fixture}"
    );
}

#[test]
fn the_notes_worked_example_is_refused_for_its_mutants_and_loads_without_them() {
    let text = String::from_utf8(fixture_bytes("note/worked_example.json")).expect("UTF-8");
    let refused = load(&text).expect_err("the example lists mutants");
    assert_eq!(refused.code(), "unknown-key");
    assert_eq!(refused.detail(), "unknown key `mutants`");
    assert_eq!(refused.subject(), "line 5 column 3");

    let start = text.find("  \"mutants\"").expect("the mutants member");
    let end = text.find("  \"scenarios\"").expect("the scenarios member");
    let without = format!("{}{}", &text[..start], &text[end..]);
    let set = load(&without).expect("the rest of the example is the format");
    assert_eq!(set.clock().tick_count(), 31);
    assert_eq!(
        set.params().map(<[_]>::len),
        Some(0),
        "an empty map is present"
    );
    let names: Vec<&str> = set.scenarios().iter().map(|s| s.name()).collect();
    assert_eq!(names, ["transient_overlap", "short_delay"]);
    let short_delay = &set.scenarios()[1];
    let params = short_delay.params().expect("its own params");
    assert_eq!(params[0].path(), "persist.delayTime");
    assert_eq!(params[0].value(), Literal::Real(300.0));
    assert_eq!(short_delay.expectations().len(), 3);
    let Expectation::Diagnostic(instant) = &short_delay.expectations()[2] else {
        panic!("the third expectation is the diagnostic instant");
    };
    assert_eq!(instant.block(), "CDL.Utilities.Assert");
    assert_eq!(instant.message(), Some("Both valves open."));
    assert_eq!(
        (instant.at_s().to_bits(), instant.tick()),
        (360.0_f64.to_bits(), 6)
    );
}

/// `document` without the member `key` of the first object starting with `anchor`. The member
/// is written `"key": value` and is followed or preceded by `, `.
fn without_member(document: &str, anchor: &str, key: &str) -> String {
    let object = document.find(anchor).expect("the object");
    let start = object
        + document[object..]
            .find(&format!("\"{key}\":"))
            .expect("the key");
    let bytes = document.as_bytes();
    let (mut depth, mut end, mut in_string) = (0_i32, start, false);
    while end < bytes.len() {
        match bytes[end] {
            b'"' => in_string = !in_string,
            b'{' | b'[' if !in_string => depth += 1,
            b'}' | b']' if !in_string && depth == 0 => break,
            b'}' | b']' if !in_string => depth -= 1,
            b',' if !in_string && depth == 0 => break,
            _ => {}
        }
        end += 1;
    }
    if bytes[end] == b',' {
        format!("{}{}", &document[..start], &document[end + 2..])
    } else {
        // The last member: drop the `, ` before it.
        format!("{}{}", &document[..start - 2], &document[end..])
    }
}

#[test]
fn a_required_key_cannot_be_absent_and_an_optional_one_can() {
    // (the object's first bytes, its required keys, its optional keys)
    let objects: [(&str, &[&str], &[&str]); 7] = [
        (
            "{\"schema\"",
            &["schema", "clock", "scenarios"],
            &["params"],
        ),
        ("{\"step_s\"", &["step_s", "horizon_s"], &[]),
        (
            "{\"name\"",
            &["name", "inputs", "expect"],
            &["description", "params"],
        ),
        ("{\"t\"", &["t", "value"], &[]),
        ("{\"output\"", &["output", "from_s", "to_s", "equals"], &[]),
        ("{\"diagnostic\"", &["diagnostic", "at_s"], &[]),
        ("{\"block\"", &["block"], &["message"]),
    ];
    for (anchor, required, optional) in objects {
        for key in required {
            let document = without_member(&full(), anchor, key);
            let refused = load(&document).expect_err("refused");
            assert_eq!(
                refused.code(),
                "missing-key",
                "{key} of {anchor}: {document}"
            );
            assert_eq!(refused.detail(), format!("missing key `{key}`"));
        }
        for key in optional {
            let document = without_member(&full(), anchor, key);
            assert_eq!(
                code_of(&document),
                "loaded",
                "{key} of {anchor}: {document}"
            );
        }
    }
}

#[test]
fn an_absent_optional_key_is_absent_and_an_empty_one_is_present() {
    let absent = load(&set_of(&scenario("", ""))).expect("loads");
    assert_eq!(absent.params(), None);
    assert_eq!(absent.scenarios()[0].params(), None);
    assert_eq!(absent.scenarios()[0].description(), None);
    let empty = load(
        r#"{"schema": "oce/vectors/v1", "clock": {"step_s": 60, "horizon_s": 1800}, "params": {},
            "scenarios": [{"name": "s", "description": "", "params": {}, "inputs": {}, "expect": []}]}"#,
    )
    .expect("loads");
    assert_eq!(empty.params().map(<[_]>::len), Some(0));
    assert_eq!(empty.scenarios()[0].params().map(<[_]>::len), Some(0));
    assert_eq!(empty.scenarios()[0].description(), Some(""));
    assert_ne!(absent, empty, "the difference is part of the set");
    assert_ne!(absent.spec_id(), empty.spec_id());
    // An empty message is a message: it matches a warning whose message is empty.
    let message = load(&one(
        "",
        r#"{"diagnostic": {"block": "B", "message": ""}, "at_s": 0}"#,
    ))
    .expect("loads");
    let Expectation::Diagnostic(instant) = &message.scenarios()[0].expectations()[0] else {
        panic!("an instant");
    };
    assert_eq!(instant.message(), Some(""));
}

#[test]
fn null_is_never_a_value() {
    let document = full();
    for (written, replaced) in [
        ("\"schema\": \"oce/vectors/v1\"", "\"schema\": null"),
        (
            "\"clock\": {\"step_s\": 60, \"horizon_s\": 1800}",
            "\"clock\": null",
        ),
        ("\"step_s\": 60", "\"step_s\": null"),
        ("\"params\": {\"a.k\": 1}", "\"params\": null"),
        ("\"a.k\": 1", "\"a.k\": null"),
        ("\"description\": \"d\"", "\"description\": null"),
        ("\"params\": {\"b.k\": 2.5}", "\"params\": null"),
        ("\"v\": true", "\"v\": null"),
        ("\"t\": 0", "\"t\": null"),
        ("\"value\": 1", "\"value\": null"),
        ("\"output\": \"y\"", "\"output\": null"),
        ("\"equals\": false", "\"equals\": null"),
        ("\"message\": \"m\"", "\"message\": null"),
        ("\"at_s\": 60", "\"at_s\": null"),
        ("\"name\": \"s\"", "\"name\": null"),
    ] {
        assert!(document.contains(written), "{written}");
        let refused = load(&document.replacen(written, replaced, 1)).expect_err("refused");
        assert_eq!(refused.code(), "wrong-type", "{replaced}");
        assert!(
            refused.detail().ends_with("found null"),
            "{}",
            refused.detail()
        );
    }
    assert_eq!(code_of("null"), "wrong-type");
}

#[test]
fn a_value_of_another_json_type_is_refused_where_it_stands() {
    let document = full();
    for (written, replaced) in [
        // A list is never read as an object's members in order.
        (
            "\"clock\": {\"step_s\": 60, \"horizon_s\": 1800}",
            "\"clock\": [60, 1800]",
        ),
        ("{\"t\": 0, \"value\": 1}", "[0, 1]"),
        (
            "\"diagnostic\": {\"block\": \"CDL.Utilities.Assert\", \"message\": \"m\"}",
            "\"diagnostic\": [\"CDL.Utilities.Assert\", \"m\"]",
        ),
        (
            "\"diagnostic\": {\"block\": \"CDL.Utilities.Assert\", \"message\": \"m\"}",
            "\"diagnostic\": \"CDL.Utilities.Assert\"",
        ),
        // Times are numbers, not strings or booleans.
        ("\"step_s\": 60", "\"step_s\": \"60\""),
        ("\"horizon_s\": 1800", "\"horizon_s\": true"),
        ("\"t\": 0", "\"t\": \"0\""),
        ("\"at_s\": 60", "\"at_s\": \"60\""),
        ("\"to_s\": 1800", "\"to_s\": [1800]"),
        // Names and text are strings.
        ("\"name\": \"s\"", "\"name\": 7"),
        ("\"description\": \"d\"", "\"description\": [\"d\"]"),
        ("\"output\": \"y\"", "\"output\": {\"name\": \"y\"}"),
        ("\"schema\": \"oce/vectors/v1\"", "\"schema\": 1"),
        (
            "\"schema\": \"oce/vectors/v1\"",
            "\"schema\": [\"oce/vectors/v1\"]",
        ),
        ("\"message\": \"m\"", "\"message\": 0"),
        // A value is a number or a boolean: no string, list or object.
        ("\"equals\": false", "\"equals\": \"false\""),
        ("\"equals\": false", "\"equals\": [false]"),
        ("\"value\": 1", "\"value\": \"1\""),
        ("\"value\": 1", "\"value\": {\"member\": \"Mode.on\"}"),
        ("\"a.k\": 1", "\"a.k\": \"1\""),
        ("\"b.k\": 2.5", "\"b.k\": [2.5]"),
        // An input is a constant or a step list: an object (a series, a file) is refused.
        (
            "\"v\": true",
            "\"v\": {\"csv\": \"trend.csv\", \"column\": \"v\"}",
        ),
        ("\"v\": true", "\"v\": \"on\""),
        ("\"u\": [{\"t\": 0, \"value\": 1}]", "\"u\": [[0, 1]]"),
        ("\"u\": [{\"t\": 0, \"value\": 1}]", "\"u\": [1]"),
        // The containers themselves.
        ("\"params\": {\"a.k\": 1}", "\"params\": [[\"a.k\", 1]]"),
        ("\"inputs\": {", "\"inputs\": [], \"x\": {"),
        ("\"expect\": [", "\"expect\": {}, \"x\": ["),
        ("\"scenarios\": [", "\"scenarios\": {}, \"x\": ["),
    ] {
        assert!(document.contains(written), "{written}");
        let refused = load(&document.replacen(written, replaced, 1)).expect_err("refused");
        assert_eq!(refused.code(), "wrong-type", "{replaced}: {refused}");
        assert!(refused.at().is_some());
    }
    for root in ["[]", "\"oce/vectors/v1\"", "1", "true", "[{}]"] {
        assert_eq!(code_of(root), "wrong-type", "{root}");
    }
}

#[test]
fn an_expectation_is_exactly_one_of_its_two_forms() {
    let ambiguous = [
        "{}",
        r#"{"output": "y", "from_s": 0, "to_s": 60, "equals": false, "diagnostic": {"block": "B"}, "at_s": 60}"#,
        r#"{"output": "y", "from_s": 0, "to_s": 60, "equals": false, "at_s": 60}"#,
        r#"{"diagnostic": {"block": "B"}, "at_s": 60, "equals": true}"#,
        r#"{"output": "y", "at_s": 60}"#,
        r#"{"diagnostic": {"block": "B"}, "from_s": 0}"#,
    ];
    for expectation in ambiguous {
        let (code, subject) = refusal(&one("", expectation));
        assert_eq!(code, "ambiguous-expectation", "{expectation}");
        assert_eq!(subject, "line 1 column 127", "at the expectation");
    }
    // One form with a key missing is that form, incomplete: the missing key is named.
    for (expectation, missing) in [
        (r#"{"output": "y"}"#, "from_s"),
        (r#"{"from_s": 0, "to_s": 60, "equals": false}"#, "output"),
        (r#"{"output": "y", "from_s": 0, "equals": false}"#, "to_s"),
        (r#"{"output": "y", "from_s": 0, "to_s": 60}"#, "equals"),
        (r#"{"at_s": 60}"#, "diagnostic"),
        (r#"{"diagnostic": {"block": "B"}}"#, "at_s"),
    ] {
        let refused = load(&one("", expectation)).expect_err("refused");
        assert_eq!(refused.code(), "missing-key", "{expectation}");
        assert_eq!(refused.detail(), format!("missing key `{missing}`"));
    }
    // An unknown key in an expectation is reported as that, whichever form the rest is.
    assert_eq!(
        code_of(&one("", r#"{"output": "y", "at_s": 60, "within": {}}"#)),
        "unknown-key"
    );
    assert_eq!(code_of(&one("", WINDOW)), "loaded");
    assert_eq!(code_of(&one("", INSTANT)), "loaded");
}

#[test]
fn any_schema_tag_but_the_one_is_refused_by_name() {
    for tag in [
        "cxf-library/vectors/v1",
        "cxf-library/vectors/v2",
        "oce/vectors/v2",
        "oce/vectors/v0",
        "oce/vectors/v10",
        "oce/vectors/v1 ",
        " oce/vectors/v1",
        "OCE/vectors/v1",
        "oce/vectors",
        "oce/receipt/v1",
        "",
    ] {
        let document = set_of("").replace("oce/vectors/v1", tag);
        let refused = load(&document).expect_err("refused");
        assert_eq!(refused.code(), "unknown-schema", "{tag:?}");
        assert_eq!(refused.subject(), "schema");
        assert!(
            refused.detail().contains(&format!("`{tag}`")),
            "{}",
            refused.detail()
        );
    }
    // The tag is compared after unescaping, so an escaped spelling of the one tag is the tag.
    assert_eq!(
        code_of(&set_of("").replace("oce/vectors/v1", "oce\\/vectors\\/v1")),
        "loaded"
    );
    assert_eq!(
        code_of(&set_of("").replace("oce/vectors/v1", "\\u006fce/vectors/v1")),
        "loaded"
    );
}

#[test]
fn a_foreign_tag_is_refused_before_the_shape_and_after_the_strict_json_pass() {
    // A document of another shape entirely is refused for its tag, wherever the tag is written.
    for foreign in [
        r#"{"schema": "cxf-library/vectors/v1", "vectors": [], "tolerance": 1e-9}"#,
        r#"{"vectors": [], "tolerance": 1e-9, "schema": "cxf-library/vectors/v1"}"#,
        r#"{"clock": [60, 1800], "scenarios": null, "schema": "oce/vectors/v2"}"#,
        r#"{"schema": "oce/commissioning-spec/v1"}"#,
    ] {
        assert_eq!(code_of(foreign), "unknown-schema", "{foreign}");
    }
    // Strict JSON comes first: the tag of a document that is not strict JSON is not consulted.
    assert_eq!(
        code_of(r#"{"schema": "cxf-library/vectors/v1", "a": 1, "a": 2}"#),
        "duplicate-key"
    );
    assert_eq!(
        code_of(r#"{"schema": "cxf-library/vectors/v1", "a": NaN}"#),
        "non-finite-number"
    );
    assert_eq!(
        code_of(r#"{"schema": "cxf-library/vectors/v1","#),
        "json-syntax"
    );
    // With no tag to read, the shape speaks: a missing key, or a tag of the wrong type.
    assert_eq!(code_of(r#"{"vectors": []}"#), "unknown-key");
    assert_eq!(code_of(r"{}"), "missing-key");
    assert_eq!(code_of(r#"{"schema": 2}"#), "wrong-type");
    // A nested `schema` is not the set's tag.
    assert_eq!(
        code_of(r#"{"clock": {"schema": "oce/vectors/v2", "step_s": 1, "horizon_s": 1}}"#),
        "unknown-key"
    );
}

#[test]
fn an_empty_name_is_refused_in_every_name_position() {
    let cases = [
        (
            r#"{"schema": "oce/vectors/v1", "clock": {"step_s": 60, "horizon_s": 1800}, "params": {"": 1}, "scenarios": []}"#.to_owned(),
            "params",
        ),
        (set_of(r#"{"name": "", "inputs": {}, "expect": []}"#), "scenarios[0].name"),
        (
            set_of(r#"{"name": "s", "params": {"": true}, "inputs": {}, "expect": []}"#),
            "scenarios[0].params",
        ),
        (one("\"\": 1", ""), "scenarios[0].inputs"),
        (
            one("", r#"{"output": "", "from_s": 0, "to_s": 60, "equals": false}"#),
            "scenarios[0].expect[0].output",
        ),
        (
            one("", &format!("{WINDOW}, {}", r#"{"diagnostic": {"block": ""}, "at_s": 60}"#)),
            "scenarios[0].expect[1].diagnostic.block",
        ),
    ];
    for (document, subject) in cases {
        assert_eq!(
            refusal(&document),
            ("empty-name", subject.to_owned()),
            "{document}"
        );
    }
    // A name of one space is a name; what it resolves to is the binder's question.
    assert_eq!(code_of(&one("\" \": 1", "")), "loaded");
}

#[test]
fn scenario_names_are_unique_byte_for_byte() {
    let named = |name: &str| format!("{{\"name\": \"{name}\", \"inputs\": {{}}, \"expect\": []}}");
    let set =
        |names: &[&str]| set_of(&names.iter().map(|n| named(n)).collect::<Vec<_>>().join(","));
    assert_eq!(
        refusal(&set(&["a", "b", "c", "b"])),
        ("duplicate-scenario", "scenarios[3].name".to_owned())
    );
    // Compared after unescaping, and exactly: case and trailing space make another name.
    assert_eq!(code_of(&set(&["caf\\u00e9", "café"])), "duplicate-scenario");
    assert_eq!(code_of(&set(&["a", "A", "a ", " a"])), "loaded");
}

#[test]
fn a_value_keeps_the_kind_it_was_written_as() {
    let set = load(&one(
        "\"a\": 1, \"b\": 1.0, \"c\": 1e0, \"d\": -0, \"e\": -0.0, \"f\": true, \"g\": 9007199254740993, \"h\": 1E2",
        r#"{"output": "y", "from_s": 0, "to_s": 60, "equals": 0}, {"output": "y", "from_s": 0, "to_s": 60, "equals": 0.0}"#,
    ))
    .expect("loads");
    let scenario = &set.scenarios()[0];
    let values: Vec<Literal> = scenario
        .inputs()
        .iter()
        .map(|input| {
            assert!(input.is_constant());
            assert_eq!(input.steps().len(), 1, "a constant is one step at t = 0");
            assert_eq!(input.steps()[0].time_s().to_bits(), 0.0_f64.to_bits());
            input.steps()[0].value()
        })
        .collect();
    assert_eq!(
        values,
        [
            Literal::Integer(1),
            Literal::Real(1.0),
            Literal::Real(1.0),
            Literal::Integer(0),
            Literal::Real(-0.0),
            Literal::Boolean(true),
            Literal::Integer(9_007_199_254_740_993),
            Literal::Real(100.0),
        ]
    );
    let equals: Vec<Literal> = scenario
        .expectations()
        .iter()
        .map(|expectation| match expectation {
            Expectation::Window(window) => window.equals(),
            other => panic!("a window, found {other:?}"),
        })
        .collect();
    assert_eq!(equals, [Literal::Integer(0), Literal::Real(0.0)]);
    assert_ne!(
        equals[0], equals[1],
        "typing waits for the binder: 0 is not 0.0 here"
    );
}

#[test]
fn member_order_is_free_and_changes_nothing() {
    let ordered = load(&full()).expect("loads");
    let shuffled = load(
        r#"{"scenarios": [{"expect": [{"equals": false, "to_s": 1800, "from_s": 0, "output": "y"},
                {"at_s": 60, "diagnostic": {"message": "m", "block": "CDL.Utilities.Assert"}}],
              "inputs": {"v": true, "u": [{"value": 1, "t": 0}]}, "params": {"b.k": 2.5},
              "description": "d", "name": "s"}],
            "params": {"a.k": 1}, "clock": {"horizon_s": 1800, "step_s": 60},
            "schema": "oce/vectors/v1"}"#,
    )
    .expect("loads");
    assert_eq!(ordered, shuffled);
    assert_eq!(ordered.spec_id(), shuffled.spec_id());
    assert_eq!(ordered.to_canonical_json(), shuffled.to_canonical_json());
    // Inputs are held sorted by name bytes, whatever order the file wrote them in.
    let names: Vec<&str> = shuffled.scenarios()[0]
        .inputs()
        .iter()
        .map(|i| i.name())
        .collect();
    assert_eq!(names, ["u", "v"]);
}
