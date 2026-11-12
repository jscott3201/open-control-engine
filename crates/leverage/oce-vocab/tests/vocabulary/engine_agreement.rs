//! The curated claims held against a loaded engine, through the facade.
//!
//! A curated note is advice an agent acts on, so each seeded claim that the engine can decide
//! is decided here: which classes break an algebraic loop, which input is which, what
//! `delayOnInit` changes, how `TrueHoldWithReset`'s connectors behave and what `Sampler` does
//! between sample instants. Each case is one block in a flat CXF document, loaded with
//! `Engine::load_cxf` and run with complete frames.

use std::collections::BTreeMap;

use oce_api::{CatalogDefault, CatalogPortKind, Engine, Value, catalog, class_metadata};
use oce_vocab::{NoteKind, class_vocab, vocabulary};
use serde_json::{Value as Json, json};

/// The sequence every case is written as.
const ROOT: &str = "urn:oce:vocab:case";

fn type_name(kind: CatalogPortKind) -> &'static str {
    match kind {
        CatalogPortKind::Real => "Real",
        CatalogPortKind::Integer => "Integer",
        CatalogPortKind::Boolean => "Boolean",
    }
}

fn connector(id: String, kind: CatalogPortKind, side: &str, targets: &[String]) -> Json {
    let mut node = json!({
        "@id": id,
        "@type": format!("S231:{}{side}", type_name(kind)),
        "S231:isOfDataType": {"@id": format!("S231:{}", type_name(kind))},
    });
    if !targets.is_empty() {
        let targets: Vec<Json> = targets
            .iter()
            .map(|target| json!({"@id": target}))
            .collect();
        node["S231:isConnectedTo"] = Json::Array(targets);
    }
    node
}

fn parameter(id: String, value: &Value) -> Json {
    let (data_type, value) = match value {
        Value::Real(real) => (
            "Real",
            json!({"@value": format!("{real:?}"), "@type": "http://www.w3.org/2001/XMLSchema#double"}),
        ),
        Value::Integer(integer) => ("Integer", json!(integer)),
        Value::Boolean(boolean) => ("Boolean", json!(boolean)),
        other => panic!("no CXF spelling here for {other:?}"),
    };
    json!({
        "@id": id,
        "@type": "S231:Parameter",
        "S231:isOfDataType": {"@id": format!("S231:{data_type}")},
        "S231:value": value,
    })
}

/// A flat CXF document with one block of the named class `class_path`: every block input is
/// fed by a boundary input of the same name, every block output drives a boundary output named
/// `<port>Out`, and `feedback` additionally wires one block output to one block input (which
/// then has no boundary input).
pub fn one_block(
    class_path: &str,
    parameters: &[(&str, Value)],
    feedback: Option<(&str, &str)>,
) -> Vec<u8> {
    let entry = catalog()
        .iter()
        .find(|entry| entry.class_path == class_path)
        .unwrap_or_else(|| panic!("{class_path} is a catalog class"));
    let block = format!("{ROOT}.blk");
    let reference = |id: &String| json!({ "@id": id });
    let mut graph = Vec::new();
    let (mut boundary_inputs, mut block_inputs) = (Vec::new(), Vec::new());
    for port in &entry.inputs {
        let name = port.name.expect("a named class");
        let block_port = format!("{block}.{name}");
        if feedback.is_none_or(|(_, into)| into != name) {
            let boundary = format!("{ROOT}.{name}");
            graph.push(connector(
                boundary.clone(),
                port.kind,
                "Input",
                std::slice::from_ref(&block_port),
            ));
            boundary_inputs.push(reference(&boundary));
        }
        graph.push(connector(block_port.clone(), port.kind, "Input", &[]));
        block_inputs.push(reference(&block_port));
    }
    let (mut boundary_outputs, mut block_outputs) = (Vec::new(), Vec::new());
    for port in &entry.outputs {
        let name = port.name.expect("a named class");
        let block_port = format!("{block}.{name}");
        let boundary = format!("{ROOT}.{name}Out");
        let mut targets = vec![boundary.clone()];
        if let Some((from, into)) = feedback
            && from == name
        {
            targets.push(format!("{block}.{into}"));
        }
        graph.push(connector(block_port.clone(), port.kind, "Output", &targets));
        graph.push(connector(boundary.clone(), port.kind, "Output", &[]));
        block_outputs.push(reference(&block_port));
        boundary_outputs.push(reference(&boundary));
    }
    let mut block_parameters = Vec::new();
    for (name, value) in parameters {
        let id = format!("{block}.{name}");
        graph.push(parameter(id.clone(), value));
        block_parameters.push(reference(&id));
    }
    graph.push(json!({
        "@id": block,
        "@type": format!("http://example.org#Buildings.Controls.OBC.{class_path}"),
        "S231:hasParameter": block_parameters,
        "S231:hasInput": block_inputs,
        "S231:hasOutput": block_outputs,
    }));
    graph.push(json!({
        "@id": ROOT,
        "@type": "S231:Block",
        "S231:hasInput": boundary_inputs,
        "S231:hasOutput": boundary_outputs,
        "S231:containsBlock": [{"@id": format!("{ROOT}.blk")}],
    }));
    let document = json!({
        "@context": {"S231": "http://data.ashrae.org/S231#"},
        "@graph": graph,
    });
    serde_json::to_vec(&document).expect("serialize")
}

fn loaded(class_path: &str, parameters: &[(&str, Value)]) -> Engine {
    let mut engine = Engine::new();
    engine
        .load_cxf(&one_block(class_path, parameters, None))
        .unwrap_or_else(|error| panic!("{class_path}: {error}"));
    engine
}

/// Run one frame and return the boundary output `<port>Out`.
fn frame(engine: &mut Engine, time: f64, inputs: &[(&str, Value)], output: &str) -> Value {
    let named: Vec<(String, Value)> = inputs
        .iter()
        .map(|(name, value)| (format!("{ROOT}.{name}"), value.clone()))
        .collect();
    let entries: Vec<(&str, Value)> = named
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect();
    let prepared = engine
        .prepare_frame(time, &entries)
        .unwrap_or_else(|error| panic!("prepare at {time}: {error}"));
    let completed = engine
        .execute_frame(prepared)
        .unwrap_or_else(|error| panic!("execute at {time}: {error}"));
    let wanted = format!("{ROOT}.{output}Out");
    completed
        .outputs()
        .iter()
        .find(|(identity, _)| *identity == wanted)
        .map(|(_, value)| value.clone())
        .unwrap_or_else(|| panic!("no output {wanted} in {:?}", completed.outputs()))
}

fn real(value: f64) -> Value {
    Value::Real(value)
}

fn boolean(value: bool) -> Value {
    Value::Boolean(value)
}

fn assert_real(got: &Value, want: f64) {
    assert!(got.bit_eq(&Value::Real(want)), "got {got:?}, want {want:?}");
}

fn assert_boolean(got: &Value, want: bool) {
    assert!(
        got.bit_eq(&Value::Boolean(want)),
        "got {got:?}, want {want}"
    );
}

/// Whether the engine refuses a feedback loop through the class alone as an algebraic loop.
fn refuses_a_self_loop(
    class_path: &str,
    parameters: &[(&str, Value)],
    from: &str,
    into: &str,
) -> bool {
    let bytes = one_block(class_path, parameters, Some((from, into)));
    // The receipt path is the one that carries the coded per-member loop records.
    match Engine::new().load_cxf_with_receipt(&bytes) {
        Ok(_) => false,
        Err(failure) => {
            let records = failure.diagnostics().records();
            let codes: Vec<&str> = records.iter().map(|record| record.key().code()).collect();
            assert!(
                !codes.is_empty() && codes.iter().all(|code| *code == "algebraic-loop"),
                "{class_path}: refused for another reason: {} {codes:?}",
                failure.error()
            );
            true
        }
    }
}

/// The self-loop each class with a `loop` note is tried with: its parameters, and the output
/// and input the loop joins.
fn self_loop(class_path: &str) -> (Vec<(&'static str, Value)>, &'static str, &'static str) {
    let sampled = vec![("samplePeriod", real(1.0))];
    match class_path {
        "CDL.Discrete.UnitDelay" | "CDL.Discrete.Sampler" | "CDL.Discrete.ZeroOrderHold" => {
            (sampled, "y", "u")
        }
        "CDL.Logical.Pre" | "CDL.Logical.Latch" => (Vec::new(), "y", "u"),
        "CDL.Logical.TrueDelay" => (vec![("delayTime", real(1.0))], "y", "u"),
        "CDL.Logical.TrueFalseHold" => (vec![("trueHoldDuration", real(1.0))], "y", "u"),
        other => panic!("{other} has a loop note and no self-loop case here"),
    }
}

#[test]
fn every_loop_note_says_what_the_engine_does_with_a_self_loop() {
    let mut breakers = Vec::new();
    let mut feedthrough = Vec::new();
    for class in vocabulary().classes() {
        for note in class
            .notes
            .iter()
            .filter(|note| note.kind == NoteKind::Loop)
        {
            let (parameters, from, into) = self_loop(class.class_path);
            let refused = refuses_a_self_loop(class.class_path, &parameters, from, into);
            // A note either says the class breaks a loop, or that a loop through it alone is
            // refused; the engine must agree with whichever it says.
            let says_breaks = note.text.contains("so it breaks an algebraic loop");
            let says_refused = note
                .text
                .contains("alone is an algebraic loop and the engine refuses it");
            assert!(
                says_breaks != says_refused,
                "{}: {}",
                class.class_path,
                note.text
            );
            assert_eq!(refused, says_refused, "{}: {}", class.class_path, note.text);
            if refused {
                feedthrough.push(class.class_path);
            } else {
                breakers.push(class.class_path);
            }
        }
    }
    assert_eq!(breakers, ["CDL.Logical.Pre", "CDL.Discrete.UnitDelay"]);
    assert_eq!(
        feedthrough,
        [
            "CDL.Logical.Latch",
            "CDL.Logical.TrueDelay",
            "CDL.Logical.TrueFalseHold",
            "CDL.Discrete.Sampler",
            "CDL.Discrete.ZeroOrderHold",
        ]
    );
}

#[test]
fn the_breaker_each_feedthrough_note_recommends_breaks_a_loop_of_that_signal_kind() {
    let kind_of = |class_path: &str| {
        let entry = catalog()
            .iter()
            .find(|entry| entry.class_path == class_path)
            .expect("a catalog class");
        (entry.inputs[0].kind, entry.outputs[0].kind)
    };
    for class in vocabulary().classes() {
        for note in class
            .notes
            .iter()
            .filter(|note| note.kind == NoteKind::Loop)
        {
            let Some(advice) = note.text.split("Break the loop with ").nth(1) else {
                continue;
            };
            let breaker = format!("CDL.{}", advice.trim_end_matches('.'));
            assert!(class_vocab(&breaker).is_some(), "{breaker}");
            // The recommended breaker carries the same signal kind the loop does.
            assert_eq!(
                kind_of(&breaker),
                kind_of(class.class_path),
                "{}",
                class.class_path
            );
        }
    }
    assert_eq!(kind_of("CDL.Logical.Pre").0, CatalogPortKind::Boolean);
    assert_eq!(kind_of("CDL.Discrete.UnitDelay").0, CatalogPortKind::Real);
}

#[test]
fn port_order_notes_agree_with_the_engine() {
    let port_order: Vec<&str> = vocabulary()
        .classes()
        .filter(|class| {
            class
                .notes
                .iter()
                .any(|note| note.kind == NoteKind::PortOrder)
        })
        .map(|class| class.class_path)
        .collect();
    assert_eq!(
        port_order,
        [
            "CDL.Reals.Subtract",
            "CDL.Reals.Divide",
            "CDL.Reals.Greater",
            "CDL.Reals.Less"
        ]
    );
    let inputs = [("u1", real(7.0)), ("u2", real(2.0))];
    let swapped = [("u1", real(2.0)), ("u2", real(7.0))];

    let mut subtract = loaded("CDL.Reals.Subtract", &[]);
    assert_real(&frame(&mut subtract, 0.0, &inputs, "y"), 5.0);
    assert_real(&frame(&mut subtract, 1.0, &swapped, "y"), -5.0);

    let mut divide = loaded("CDL.Reals.Divide", &[]);
    assert_real(&frame(&mut divide, 0.0, &inputs, "y"), 3.5);
    assert_real(&frame(&mut divide, 1.0, &swapped, "y"), 2.0 / 7.0);

    // Fresh engines for each comparison, so the hysteresis state of one does not carry over.
    assert_boolean(
        &frame(&mut loaded("CDL.Reals.Greater", &[]), 0.0, &inputs, "y"),
        true,
    );
    assert_boolean(
        &frame(&mut loaded("CDL.Reals.Greater", &[]), 0.0, &swapped, "y"),
        false,
    );
    assert_boolean(
        &frame(&mut loaded("CDL.Reals.Less", &[]), 0.0, &inputs, "y"),
        false,
    );
    assert_boolean(
        &frame(&mut loaded("CDL.Reals.Less", &[]), 0.0, &swapped, "y"),
        true,
    );
}

#[test]
fn delay_on_init_defaults_to_false_and_true_makes_a_start_up_condition_wait() {
    let delay = class_vocab("CDL.Logical.TrueDelay").expect("a class");
    let note = delay
        .notes_about("delayOnInit")
        .next()
        .expect("the convention note");
    assert_eq!(note.kind, NoteKind::Convention);
    assert!(note.text.contains("against the CDL default false"));
    let metadata = class_metadata()
        .iter()
        .find(|metadata| metadata.class_path == "CDL.Logical.TrueDelay")
        .expect("metadata");
    let default = &metadata
        .parameters
        .iter()
        .find(|parameter| parameter.name == "delayOnInit")
        .expect("delayOnInit")
        .default;
    assert_eq!(*default, CatalogDefault::Boolean(false));

    // Input already true on the first frame, delay of two seconds.
    let on = [("u", boolean(true))];
    let mut default_delay = loaded("CDL.Logical.TrueDelay", &[("delayTime", real(2.0))]);
    assert_boolean(&frame(&mut default_delay, 0.0, &on, "y"), true);

    let convention = [("delayTime", real(2.0)), ("delayOnInit", boolean(true))];
    let mut library_delay = loaded("CDL.Logical.TrueDelay", &convention);
    assert_boolean(&frame(&mut library_delay, 0.0, &on, "y"), false);
    assert_boolean(&frame(&mut library_delay, 1.0, &on, "y"), false);
    assert_boolean(&frame(&mut library_delay, 2.0, &on, "y"), true);
}

#[test]
fn true_hold_with_reset_behaves_as_its_curated_descriptions_say() {
    let hold = class_vocab("CDL.Logical.TrueHoldWithReset").expect("a class");
    let described: BTreeMap<&str, &str> = hold
        .connectors
        .iter()
        .map(|c| (c.name, c.description))
        .chain(hold.parameters.iter().map(|p| (p.name, p.description)))
        .collect();
    assert_eq!(
        described["y"],
        "True while u is true and until duration has elapsed since the output turned true; false while clr is true"
    );
    assert_eq!(
        described["duration"],
        "Minimum time, in seconds, that the output stays true once it turns true, unless clr clears it"
    );

    let mut engine = loaded("CDL.Logical.TrueHoldWithReset", &[("duration", real(1.0))]);
    let mut at = |time: f64, u: bool, clr: bool| {
        frame(
            &mut engine,
            time,
            &[("u", boolean(u)), ("clr", boolean(clr))],
            "y",
        )
    };
    // u turns the output true and starts the hold.
    assert_boolean(&at(0.0, true, false), true);
    // u is false again, but less than `duration` has elapsed since the output turned true.
    assert_boolean(&at(0.25, false, false), true);
    assert_boolean(&at(0.75, false, false), true);
    // `duration` has elapsed.
    assert_boolean(&at(1.0, false, false), false);
    // While clr is true the output is false, even with u true.
    assert_boolean(&at(1.25, true, true), false);
    // clr released with u true: the output turns true and a new hold starts.
    assert_boolean(&at(1.5, true, false), true);
    assert_boolean(&at(1.75, false, false), true);
    // clr cancels a hold in progress, and the hold does not resume when clr is released.
    assert_boolean(&at(2.0, false, true), false);
    assert_boolean(&at(2.25, false, false), false);
}

#[test]
fn sampler_passes_the_input_at_sample_instants_and_holds_in_between() {
    let sampler = class_vocab("CDL.Discrete.Sampler").expect("a class");
    let pitfall = sampler
        .notes
        .iter()
        .find(|note| note.kind == NoteKind::Pitfall)
        .expect("the pitfall note");
    assert!(pitfall.text.starts_with("Sampler is not a delay."));

    let period = [("samplePeriod", real(1.0))];
    let mut sampler = loaded("CDL.Discrete.Sampler", &period);
    let mut delay = loaded("CDL.Discrete.UnitDelay", &period);
    // (time, input, Sampler output, UnitDelay output)
    for (time, input, sampled, delayed) in [
        // The first frame: Sampler outputs the current input, UnitDelay its start value.
        (0.0, 10.0, 10.0, 0.0),
        // Between sample instants: Sampler holds the last sample.
        (0.5, 30.0, 10.0, 0.0),
        // At the next sample instant: the current input again, never the previous sample.
        (1.0, 40.0, 40.0, 10.0),
        (1.5, 50.0, 40.0, 10.0),
        (2.0, 60.0, 60.0, 40.0),
    ] {
        let inputs = [("u", real(input))];
        assert_real(&frame(&mut sampler, time, &inputs, "y"), sampled);
        assert_real(&frame(&mut delay, time, &inputs, "y"), delayed);
    }
}

#[test]
fn every_class_with_a_seeded_note_is_checked_against_the_engine_here() {
    // A new note of a kind the engine can decide needs a case above; this pins the set so one
    // cannot be added without a look at this file.
    let mut kinds: BTreeMap<NoteKind, usize> = BTreeMap::new();
    for class in vocabulary().classes() {
        for note in class.notes {
            *kinds.entry(note.kind).or_default() += 1;
        }
    }
    let counted: Vec<(NoteKind, usize)> = kinds.into_iter().collect();
    assert_eq!(
        counted,
        [
            (NoteKind::Convention, 1),
            (NoteKind::Pitfall, 1),
            (NoteKind::PortOrder, 4),
            (NoteKind::Loop, 7),
        ]
    );
}
