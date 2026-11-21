//! A Real parameter written as an Integer loads as the document that writes it as a Real
//! (issue #683).
//!
//! Before this rule the two spellings ran identically and were told apart everywhere else: the
//! parameter table declared the parameter Integer, so a host's Real edit was refused; the
//! compiled-schedule blob carried an Integer; and the canonical content id differed, so `import`
//! then `emit` moved the id. These tests hold the facade to one answer for both spellings: the
//! content id, the blob bytes, the parameter table and its edits, the run and its snapshot.
//!
//! The documents are the tracked fixtures the issue names with one `S231:value` respelled in the
//! test, so each case states the literal it is about. `oce-cxf`'s `real_parameter_literals.rs`
//! covers the resolver rule itself, boundary by boundary.

use oce_api::{Engine, OcError, Value, ValueType};

const PASS_THROUGH: &str =
    include_str!("../../oce-cxf/tests/fixtures/pass_through_miniature.jsonld");
const MINIMAL_LOOP: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

const KEEP_K: &str = "http://example.org#PassThroughMiniature.keep.k";
const Y_START: &str = "http://example.org#MinLoop.del.y_start";
const SAMPLE_PERIOD: &str = "http://example.org#MinLoop.del.samplePeriod";

/// The ids the issue records for `pass_through_miniature.jsonld` on `development` at 71256f99:
/// as tracked (`k` written `0`), and with `k` written `0.0`.
const ISSUE_ID_INTEGER_SPELLING: &str =
    "oce:canon1:c5fa6b2bb788f161e0f2e64363e8824e63cc0e0bc0d87cbdb1ca56e887f0fc38";
const ISSUE_ID_REAL_SPELLING: &str =
    "oce:canon1:3d0c63a264b7755510af2d311cc3f866d83a12a501f3439b7dd41a1aa6f4d8f3";

fn typed(lexical: &str, datatype: &str) -> String {
    format!(r#"{{ "@value": "{lexical}", "@type": "{XSD}{datatype}" }}"#)
}

/// `doc` with the `S231:value` of the node `iri` replaced by the JSON text `literal`. The value
/// is a scalar token or one `{...}` object in every node this file edits.
fn with_value(doc: &str, iri: &str, literal: &str) -> String {
    let node = doc
        .find(&format!(r#""@id": "{iri}","#))
        .expect("the parameter node");
    let key = r#""S231:value": "#;
    let start = node + doc[node..].find(key).expect("the node's value") + key.len();
    let rest = &doc[start..];
    let length = if rest.starts_with('{') {
        rest.find('}').expect("a closed typed literal") + 1
    } else {
        rest.find([' ', ',', '\n']).expect("the end of the token")
    };
    format!("{}{literal}{}", &doc[..start], &rest[length..])
}

fn load(doc: &str) -> Engine {
    let mut engine = Engine::new();
    let (report, _) = engine.load_cxf(doc.as_bytes()).expect("the document loads");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    engine
}

fn content_id(doc: &str) -> String {
    load(doc)
        .content_id()
        .expect("a content id")
        .as_str()
        .to_owned()
}

fn blob(doc: &str) -> Vec<u8> {
    load(doc).compile_blob().expect("a blob").bytes
}

/// The declared type and current value of one parameter in the live table.
fn table_row(engine: &Engine, path: &str) -> (Value, ValueType) {
    let (_, value, attrs) = engine
        .params()
        .to_vec()
        .into_iter()
        .find(|(row, _, _)| row == path)
        .expect("the parameter is in the table");
    (value, attrs.value_type)
}

/// Run `frames` frames of `minimal_loop` from time 0 with a 1 s step and a fixed setpoint, and
/// return the snapshot bytes: every connector value and every state word, by raw bits.
fn run_minimal_loop(doc: &str, frames: u32) -> Vec<u8> {
    let mut engine = load(doc);
    for index in 0..frames {
        let inputs = [("http://example.org#MinLoop.uSet", Value::Real(0.75))];
        let frame = engine
            .prepare_frame(f64::from(index), &inputs)
            .expect("the frame prepares");
        engine.execute_frame(frame).expect("the frame executes");
    }
    engine.state_snapshot().expect("a snapshot").into_bytes()
}

#[test]
fn the_edits_reach_the_values_they_name() {
    // The helper is the instrument of every test below: check it against the fixtures by hand.
    assert!(PASS_THROUGH.contains(
        "\"@id\": \"http://example.org#PassThroughMiniature.keep.k\",\n      \"S231:value\": 0\n"
    ));
    let edited = with_value(PASS_THROUGH, KEEP_K, "0.0");
    assert!(edited.contains(
        "\"@id\": \"http://example.org#PassThroughMiniature.keep.k\",\n      \"S231:value\": 0.0\n"
    ));
    assert_eq!(edited.len(), PASS_THROUGH.len() + 2);
    assert!(MINIMAL_LOOP.contains(&format!(r#""@id": "{Y_START}", "S231:value": 0 }}"#)));
    let edited = with_value(MINIMAL_LOOP, Y_START, &typed("0.0", "double"));
    assert!(edited.contains(&format!(
        r#""@id": "{Y_START}", "S231:value": {{ "@value": "0.0", "@type": "{XSD}double" }} }}"#
    )));
    let edited = with_value(MINIMAL_LOOP, SAMPLE_PERIOD, "1");
    assert!(edited.contains(&format!(
        "\"@id\": \"{SAMPLE_PERIOD}\",\n      \"S231:value\": 1 }}"
    )));
    assert_eq!(with_value(PASS_THROUGH, KEEP_K, "0"), PASS_THROUGH);
}

#[test]
fn the_issues_two_spellings_have_one_content_id_and_it_is_the_real_spellings() {
    let as_written = content_id(PASS_THROUGH);
    assert_eq!(as_written, ISSUE_ID_REAL_SPELLING);
    assert_ne!(as_written, ISSUE_ID_INTEGER_SPELLING);
    for literal in [
        "0.0".to_owned(),
        typed("0", "integer"),
        typed("-0", "integer"),
        typed("0", "long"),
        typed("0.0", "double"),
        typed("0", "decimal"),
        r#""1 - 1""#.to_owned(),
    ] {
        assert_eq!(
            content_id(&with_value(PASS_THROUGH, KEEP_K, &literal)),
            as_written,
            "{literal}"
        );
    }
}

#[test]
fn a_different_number_and_a_negative_zero_are_still_different_documents() {
    let zero = content_id(PASS_THROUGH);
    let one = content_id(&with_value(PASS_THROUGH, KEEP_K, "1"));
    assert_ne!(one, zero);
    assert_eq!(one, content_id(&with_value(PASS_THROUGH, KEEP_K, "1.0")));
    let negative_zero = content_id(&with_value(PASS_THROUGH, KEEP_K, "-0.0"));
    assert_ne!(negative_zero, zero);
    assert_ne!(negative_zero, one);
}

#[test]
fn both_spellings_compile_to_the_same_blob_bytes() {
    let as_integer = blob(PASS_THROUGH);
    assert_eq!(as_integer, blob(&with_value(PASS_THROUGH, KEEP_K, "0.0")));
    assert_eq!(
        blob(MINIMAL_LOOP),
        blob(&with_value(MINIMAL_LOOP, Y_START, "0.0"))
    );
    // Not vacuous: another value is another blob.
    assert_ne!(as_integer, blob(&with_value(PASS_THROUGH, KEEP_K, "1")));
}

#[test]
fn both_spellings_run_to_the_same_snapshot_bit_for_bit() {
    let as_integer = run_minimal_loop(MINIMAL_LOOP, 8);
    let as_real = run_minimal_loop(&with_value(MINIMAL_LOOP, Y_START, "0.0"), 8);
    assert_eq!(as_integer, as_real);
    // The run moved the state, and repeats.
    assert_ne!(as_integer, run_minimal_loop(MINIMAL_LOOP, 0));
    assert_eq!(as_integer, run_minimal_loop(MINIMAL_LOOP, 8));
    // A different start value is a different run.
    assert_ne!(
        as_integer,
        run_minimal_loop(&with_value(MINIMAL_LOOP, Y_START, "3"), 8)
    );
}

#[test]
fn the_parameter_table_holds_and_declares_a_real_for_either_spelling() {
    for literal in ["0", "0.0"] {
        let engine = load(&with_value(MINIMAL_LOOP, Y_START, literal));
        let (value, declared) = table_row(&engine, Y_START);
        assert!(value.bit_eq(&Value::Real(0.0)), "{literal}: {value:?}");
        assert_eq!(declared, ValueType::Real, "{literal}");
        assert!(
            engine
                .get_param(Y_START)
                .expect("the parameter")
                .bit_eq(&Value::Real(0.0))
        );
    }
}

#[test]
fn a_real_edit_is_accepted_on_a_real_parameter_written_as_an_integer() {
    // The host-side defect the issue's ruling records: the table declared such a parameter
    // Integer, so this edit was `ParamType`.
    let mut engine = load(MINIMAL_LOOP);
    engine.halt().expect("halt");
    engine
        .set_param(Y_START, Value::Real(-0.5))
        .expect("a Real edit of a Real parameter");
    engine.resume().expect("resume");
    assert!(
        engine
            .get_param(Y_START)
            .expect("the parameter")
            .bit_eq(&Value::Real(-0.5))
    );
    // The edited engine is the document that writes the edited value.
    assert_eq!(
        engine.content_id().expect("a content id").as_str(),
        content_id(&with_value(MINIMAL_LOOP, Y_START, "-0.5"))
    );
}

#[test]
fn an_integer_edit_of_a_real_parameter_is_a_type_refusal_and_changes_nothing() {
    // `set_param` does not coerce. With the table declaring Real, an Integer edit never reaches
    // the range checks at all.
    let mut engine = load(MINIMAL_LOOP);
    let before = engine.content_id().expect("a content id");
    engine.halt().expect("halt");
    for value in [
        Value::Integer(1),
        Value::Integer(0),
        Value::Integer(i64::MAX),
    ] {
        assert!(matches!(
            engine.set_param(Y_START, value),
            Err(OcError::ParamType { path }) if path == Y_START
        ));
    }
    engine.resume().expect("resume");
    assert_eq!(engine.content_id().expect("a content id"), before);
}

#[test]
fn a_bounded_real_parameter_written_as_an_integer_keeps_its_bound_on_edits() {
    // The ruling asked for this to be confirmed or refuted: by reading, an Integer edit skipped
    // `set_param`'s inclusive bound check (it reads the value with `as_real`). `UnitDelay`'s
    // `samplePeriod` has an inclusive lower bound. Written as the Integer `1` it is now a Real
    // in the table with that bound declared, a Real edit below the bound is `ParamRange`, and an
    // Integer edit is `ParamType` before any bound is read. (Before this rule the Integer edit
    // did skip that check and was still refused, as `ParamRange`, by the class rules that run
    // after it, which widen an Integer: the skip was real and had no effect on the outcome.)
    let mut engine = load(&with_value(MINIMAL_LOOP, SAMPLE_PERIOD, "1"));
    let (value, declared) = table_row(&engine, SAMPLE_PERIOD);
    assert!(value.bit_eq(&Value::Real(1.0)));
    assert_eq!(declared, ValueType::Real);
    let (_, _, attrs) = engine
        .params()
        .to_vec()
        .into_iter()
        .find(|(row, _, _)| row == SAMPLE_PERIOD)
        .expect("the row");
    let lower = attrs.min.expect("samplePeriod declares a lower bound");
    assert!(lower > 0.0);

    engine.halt().expect("halt");
    for below in [
        Value::Real(0.0),
        Value::Real(-1.0),
        Value::Real(lower / 2.0),
    ] {
        assert!(matches!(
            engine.set_param(SAMPLE_PERIOD, below),
            Err(OcError::ParamRange { path }) if path == SAMPLE_PERIOD
        ));
    }
    for integer in [Value::Integer(-1), Value::Integer(0), Value::Integer(2)] {
        assert!(matches!(
            engine.set_param(SAMPLE_PERIOD, integer),
            Err(OcError::ParamType { path }) if path == SAMPLE_PERIOD
        ));
    }
    engine
        .set_param(SAMPLE_PERIOD, Value::Real(lower))
        .expect("the bound itself is inside");
    engine
        .set_param(SAMPLE_PERIOD, Value::Real(2.0))
        .expect("an edit inside the bound");
    engine.resume().expect("resume");
}

#[test]
fn an_integer_past_two_to_the_fifty_third_on_a_real_parameter_refuses_the_load() {
    for literal in [
        "9007199254740993",
        "-9007199254740993",
        "9223372036854775807",
        "-9223372036854775808",
    ] {
        let mut engine = Engine::new();
        let error = engine
            .load_cxf(with_value(PASS_THROUGH, KEEP_K, literal).as_bytes())
            .expect_err("an inexact Integer on a Real parameter");
        assert!(matches!(error, OcError::Cxf(_)), "{literal}: {error:?}");
        let diagnostics: Vec<_> = error.all_diagnostics().collect();
        assert_eq!(diagnostics.len(), 1, "{literal}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code.as_str(), "parameter-out-of-range");
        assert!(diagnostics[0].is_error());
        assert_eq!(diagnostics[0].subject.as_deref(), Some(KEEP_K));
        assert_eq!(
            diagnostics[0].message,
            format!(
                "parameter `k` on `http://example.org#PassThroughMiniature.keep` binds the \
                 Integer {literal} to a Real parameter of `CDL.Reals.MultiplyByParameter`; an \
                 Integer above 2^53 in magnitude is not exact as a Real"
            )
        );
        // Nothing was loaded.
        assert!(engine.content_id().is_err());
    }
    // The bound itself loads, as the Real it is exactly.
    for (literal, expected) in [
        ("9007199254740992", 9_007_199_254_740_992.0),
        ("-9007199254740992", -9_007_199_254_740_992.0),
    ] {
        let engine = load(&with_value(PASS_THROUGH, KEEP_K, literal));
        assert!(
            engine
                .get_param(KEEP_K)
                .expect("the parameter")
                .bit_eq(&Value::Real(expected))
        );
    }
}

#[test]
fn a_refused_load_leaves_the_loaded_model_as_it_was() {
    let mut engine = load(PASS_THROUGH);
    let before = engine.content_id().expect("a content id");
    let refused = with_value(PASS_THROUGH, KEEP_K, "9007199254740993");
    assert!(engine.load_cxf(refused.as_bytes()).is_err());
    assert_eq!(engine.content_id().expect("a content id"), before);
}

#[test]
fn loading_an_integer_spelling_twice_gives_the_same_id_blob_and_table() {
    let first = load(MINIMAL_LOOP);
    let second = load(MINIMAL_LOOP);
    assert_eq!(first.content_id().unwrap(), second.content_id().unwrap());
    assert_eq!(
        first.compile_blob().unwrap().bytes,
        second.compile_blob().unwrap().bytes
    );
    let rows = |engine: &Engine| format!("{:?}", engine.params().to_vec());
    assert_eq!(rows(&first), rows(&second));
}
