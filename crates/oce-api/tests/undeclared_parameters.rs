//! A parameter a block binds is one its class declares, or the document is refused (issue
//! #748): never a value the block drops while it runs on its default.
//!
//! Before, `Integers.Sources.Pulse { period = 4, amplitud = 3 }` loaded clean, carried `amplitud`
//! into the model, the content id and the blob, and output the default amplitude 1. Each family
//! of parameter below has its misspelling refused end to end, naming the instance, the name and
//! the parameter it probably meant, and its right spelling loaded and ticked with the value
//! written. Then the neighbours that must still load (conditional members, composite parameters,
//! element spellings other rules own), the blob decoder's own refusal, and `class_ports`.

use crate::blob_image;
use crate::blob_models;
use crate::parameter_documents;

use blob_image::{Image, Val};
use blob_models::{chain, refusal};
use oce_api::{ClassPortsError, DiagCode, Engine, Value, class_ports};
use oce_graph::blob::{BlobError, decode};
use oce_graph::{EvalContext, eval_tick};
use parameter_documents::{
    Param, Shape, document, loaded, outputs, outputs_driven, param, source, table, value, vector,
};

const BLOCK: &str = "http://example.org#S.blk";

/// The unknown-name findings a refused `doc` carries, as `(subject, message)`; panics if it
/// loads.
fn unknown_names(doc: &str) -> Vec<(String, String)> {
    let error = match Engine::new().load_cxf(doc.as_bytes()) {
        Ok(_) => panic!("expected a refusal\n{doc}"),
        Err(error) => error,
    };
    error
        .diagnostics()
        .iter()
        .filter(|d| d.code == DiagCode::UnknownParameter)
        .map(|d| {
            (
                d.subject.as_deref().unwrap_or_default().to_owned(),
                d.message.clone(),
            )
        })
        .collect()
}

/// The message the load gate refuses `name` on `class` with.
fn message(name: &str, class: &str, nearest: Option<&str>) -> String {
    match nearest {
        Some(nearest) => format!(
            "parameter `{name}` on block `{class}` is not a parameter of its class; did you mean \
             `{nearest}`?"
        ),
        None => format!("parameter `{name}` on block `{class}` is not a parameter of its class"),
    }
}

const PULSE: &str = "CDL.Integers.Sources.Pulse";

#[test]
fn an_integer_parameter_misspelt_is_refused_and_spelt_right_runs_as_written() {
    // The issue's document.
    let misspelt = document(
        &source(PULSE, "Integer"),
        &[value("period", "4"), value("amplitud", "3")],
        &[],
    );
    assert_eq!(
        unknown_names(&misspelt),
        vec![(
            BLOCK.to_owned(),
            message("amplitud", PULSE, Some("amplitude"))
        )]
    );
    let right = document(
        &source(PULSE, "Integer"),
        &[value("period", "4"), value("amplitude", "3")],
        &[],
    );
    let mut engine = loaded(&right);
    assert_eq!(
        outputs(&mut engine, &[0.0, 1.0, 2.0, 3.0], None),
        [3, 3, 0, 0].map(Value::Integer)
    );
}

#[test]
fn a_real_parameter_in_the_wrong_case_is_refused_and_spelt_right_runs_as_written() {
    let constant = "CDL.Reals.Sources.Constant";
    let misspelt = document(&source(constant, "Real"), &[value("K", "2.5")], &[]);
    let refused = unknown_names(&misspelt);
    assert_eq!(
        refused,
        vec![(BLOCK.to_owned(), message("K", constant, Some("k")))]
    );
    let mut engine = loaded(&document(
        &source(constant, "Real"),
        &[value("k", "2.5")],
        &[],
    ));
    assert_eq!(outputs(&mut engine, &[0.0], None), [Value::Real(2.5)]);
}

#[test]
fn a_boolean_parameter_misspelt_is_refused_and_spelt_right_runs_as_written() {
    let hysteresis = Shape {
        class: "CDL.Reals.Hysteresis",
        output: "Boolean",
        inputs: &[("u", "Real")],
    };
    let with = |name: &str| {
        document(
            &hysteresis,
            &[
                value("uLow", "0.0"),
                value("uHigh", "1.0"),
                value(name, "true"),
            ],
            &[],
        )
    };
    assert_eq!(
        unknown_names(&with("pre_y_strat")),
        vec![(
            BLOCK.to_owned(),
            message("pre_y_strat", "CDL.Reals.Hysteresis", Some("pre_y_start"))
        )]
    );
    // Inside the band the output holds `pre_y_start`: true as written, not the default false.
    let mut engine = loaded(&with("pre_y_start"));
    assert_eq!(
        outputs(&mut engine, &[0.0], Some(Value::Real(0.5))),
        [Value::Boolean(true)]
    );
}

#[test]
fn an_enumeration_parameter_misspelt_is_refused_and_spelt_right_is_held() {
    let pid = Shape {
        class: "CDL.Reals.PID",
        output: "Real",
        inputs: &[("u_s", "Real"), ("u_m", "Real")],
    };
    let member = r#""Buildings.Controls.OBC.CDL.Types.SimpleController.P""#;
    assert_eq!(
        unknown_names(&document(&pid, &[value("controlerType", member)], &[])),
        vec![(
            BLOCK.to_owned(),
            message("controlerType", "CDL.Reals.PID", Some("controllerType"))
        )]
    );
    let mut engine = loaded(&document(
        &pid,
        &[value("controllerType", member), value("k", "0.5")],
        &[],
    ));
    // A P controller with gain 0.5 on an error of 1: 0.5 at once (the default PI starts at 0.5
    // too, but its integral then grows; P holds).
    let driven = [("u_s", Value::Real(1.0)), ("u_m", Value::Real(0.0))];
    assert_eq!(
        outputs_driven(&mut engine, &[0.0, 10.0], &driven),
        [Value::Real(0.5), Value::Real(0.5)]
    );
}

#[test]
fn an_array_parameter_misspelt_is_refused_at_each_element_and_spelt_right_sums_as_written() {
    let sum = Shape {
        class: "CDL.Reals.MultiSum",
        output: "Real",
        inputs: &[("u1", "Real"), ("u2", "Real")],
    };
    let nin = value("nin", "2");
    assert_eq!(
        unknown_names(&document(
            &sum,
            &[value("nin", "2"), vector("K", 2, "[2.0, 3.0]")],
            &[]
        )),
        vec![
            (
                BLOCK.to_owned(),
                message("K_1", "CDL.Reals.MultiSum", Some("k_<i>"))
            ),
            (
                BLOCK.to_owned(),
                message("K_2", "CDL.Reals.MultiSum", Some("k_<i>"))
            ),
        ]
    );
    let mut engine = loaded(&document(&sum, &[nin, vector("k", 2, "[2.0, 3.0]")], &[]));
    let driven = [("u1", Value::Real(1.0)), ("u2", Value::Real(10.0))];
    assert_eq!(
        outputs_driven(&mut engine, &[0.0], &driven),
        [Value::Real(32.0)]
    );
    // The bare base written as a scalar is no element: refused, naming the array.
    assert_eq!(
        unknown_names(&document(
            &sum,
            &[value("nin", "2"), value("k", "2.0")],
            &[]
        )),
        vec![(
            BLOCK.to_owned(),
            message("k", "CDL.Reals.MultiSum", Some("k_<i>"))
        )]
    );
}

#[test]
fn a_table_parameter_misspelt_is_refused_and_spelt_right_interpolates_as_written() {
    let time_table = source("CDL.Reals.Sources.TimeTable", "Real");
    let refused = unknown_names(&document(
        &time_table,
        &[table("tabel", "[0.0, 2.0, 1.0, 4.0]")],
        &[],
    ));
    assert_eq!(refused.len(), 4, "{refused:?}");
    assert_eq!(
        refused[0].1,
        message(
            "tabel_1_1",
            "CDL.Reals.Sources.TimeTable",
            Some("table_<row>_<col>")
        )
    );
    let mut engine = loaded(&document(
        &time_table,
        &[table("table", "[0.0, 2.0, 1.0, 4.0]")],
        &[],
    ));
    assert_eq!(
        outputs(&mut engine, &[0.0, 0.5], None),
        [Value::Real(2.0), Value::Real(3.0)]
    );
}

#[test]
fn a_name_another_class_declares_is_refused_on_a_class_that_does_not() {
    let not = Shape {
        class: "CDL.Logical.Not",
        output: "Boolean",
        inputs: &[("u", "Boolean")],
    };
    assert_eq!(
        unknown_names(&document(&not, &[value("nin", "1")], &[])),
        vec![(BLOCK.to_owned(), message("nin", "CDL.Logical.Not", None))]
    );
    let mut engine = loaded(&document(&not, &[], &[]));
    assert_eq!(
        outputs(&mut engine, &[0.0], Some(Value::Boolean(false))),
        [Value::Boolean(true)]
    );
}

#[test]
fn a_composite_parameter_of_any_name_loads_and_a_block_reads_it_by_reference() {
    // `amplitud` is the root's own declaration, not a block binding: a composite declares its
    // parameters, and the block reads one by name.
    let root = [parameter_documents::declared("amplitud", "Integer", "3")];
    let mut engine = loaded(&document(
        &source(PULSE, "Integer"),
        &[value("period", "4"), value("amplitude", r#""amplitud""#)],
        &root,
    ));
    assert!(param(&engine, "amplitude").bit_eq(&Value::Integer(3)));
    assert_eq!(outputs(&mut engine, &[0.0], None), [Value::Integer(3)]);
}

#[test]
fn element_spellings_other_rules_own_are_refused_by_them_alone() {
    let sum = Shape {
        class: "CDL.Reals.MultiSum",
        output: "Real",
        inputs: &[("u1", "Real"), ("u2", "Real")],
    };
    for (name, expected) in [
        ("k_01", "non-canonical-element-name"),
        ("k_0", "parameter-out-of-range"),
        ("k_3", "parameter-out-of-range"),
    ] {
        let doc = document(&sum, &[value("nin", "2"), value(name, "1.0")], &[]);
        let codes = parameter_documents::refusal_codes(&doc);
        assert_eq!(codes, vec![expected.to_owned()], "{name}");
    }
}

/// A one-leaf document whose `TimeTable` carries a conditional input guarded by `guard`, with
/// `extra` added to its parameters.
fn conditional(guard: &str, extra: &[Param]) -> String {
    let mut params = vec![
        table("table", "[0.0, 2.0, 1.0, 4.0]"),
        value(
            "extrapolation",
            r#""Buildings.Controls.OBC.CDL.Types.Extrapolation.HoldLastPoint""#,
        ),
    ];
    params.extend(extra.iter().map(|p| Param {
        name: p.name.clone(),
        members: p.members.clone(),
    }));
    let doc = document(&source("CDL.Reals.Sources.TimeTable", "Real"), &params, &[]);
    let block = format!(r#""@id": "{BLOCK}","#);
    let with_input = doc.replacen(
        &format!("{block}\n      \"@type\""),
        &format!("{block} \"S231:hasInput\": {{ \"@id\": \"{BLOCK}.uHol\" }},\n      \"@type\""),
        1,
    );
    assert_ne!(with_input, doc, "the input reference is added");
    with_input.replacen(
        "  ]\n}",
        &format!(
            ",\n    {{ \"@id\": \"{BLOCK}.uHol\", \"@type\": \"S231:RealInput\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }}, \
             \"S231:isConditionalComponent\": true, \"S231:conditionalExpression\": {guard:?} }}\n  ]\n}}"
        ),
        1,
    )
}

#[test]
fn a_conditional_member_guarded_by_a_declared_parameter_loads_and_one_by_an_undeclared_is_refused()
{
    let periodic = "extrapolation == Buildings.Controls.OBC.CDL.Types.Extrapolation.Periodic";
    let mut engine = loaded(&conditional(periodic, &[]));
    assert_eq!(
        outputs(&mut engine, &[0.0, 1.0], None),
        [Value::Real(2.0), Value::Real(4.0)]
    );
    // The guard may read a Boolean the block binds, but the class must declare it: `have_hol`
    // prunes the input and is then refused, not dropped.
    let refused = unknown_names(&conditional("have_hol", &[value("have_hol", "false")]));
    assert_eq!(
        refused,
        vec![(
            BLOCK.to_owned(),
            message("have_hol", "CDL.Reals.Sources.TimeTable", None)
        )]
    );
}

#[test]
fn the_blob_of_a_document_spelt_right_round_trips_and_ticks_as_the_engine_does() {
    let doc = document(
        &source(PULSE, "Integer"),
        &[value("period", "4"), value("amplitude", "3")],
        &[],
    );
    let blob = loaded(&doc).compile_blob().expect("a blob");
    let mut decoded = decode(&blob.bytes).expect("the blob decodes");
    let times = [0.0, 1.0, 2.0, 3.0];
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
    let expected = outputs(&mut loaded(&doc), &times, None);
    assert_eq!(parameter_documents::Values(from_blob), expected);

    // The same blob with the amplitude's name misspelt, resealed: the decoder refuses it before
    // building the block on its default.
    let mut image = Image::from_decoded(&decode(&blob.bytes).expect("decodes"));
    let (name, _) = image.instances[0]
        .params
        .iter_mut()
        .find(|(name, _)| name == b"amplitude")
        .expect("the amplitude");
    *name = b"amplitudf".to_vec();
    let position = image.instances[0]
        .params
        .iter()
        .position(|(name, _)| name == b"amplitudf")
        .expect("renamed");
    assert_eq!(
        refusal(&image),
        BlobError::UndeclaredParameter {
            block: 0,
            parameter: u32::try_from(position).expect("small"),
        }
    );
}

#[test]
fn a_hostile_blob_carrying_an_undeclared_name_is_refused_before_its_value_is_read() {
    // The constant's `k` renamed, a name added to the parameterless `Abs`, and a non-finite
    // value under an undeclared name: the name is judged first.
    for (block, name, value) in [
        (0usize, b"kk".to_vec(), Val::real(1.0)),
        (0, b"K".to_vec(), Val::real(1.0)),
        (0, b"k_1".to_vec(), Val::real(1.0)),
        (0, "\u{1f525}".as_bytes().to_vec(), Val::real(1.0)),
        (0, b"kk".to_vec(), Val::real(f64::NAN)),
        (1, b"ghost".to_vec(), Val::real(1.0)),
    ] {
        let mut image = chain();
        if block == 0 {
            image.instances[0].params[0] = (name, value);
        } else {
            image.instances[1].params.push((name, value));
        }
        assert_eq!(
            refusal(&image),
            BlobError::UndeclaredParameter {
                block: u32::try_from(block).expect("small"),
                parameter: 0
            }
        );
    }
    // The control: the constant's own `k` decodes.
    decode(&chain().to_bytes()).expect("the chain decodes");
}

#[test]
fn class_ports_refuses_an_undeclared_name_as_the_load_gate_does() {
    for name in ["bogus", "k_x", "k_1_2", "K_1"] {
        let error = class_ports(
            "CDL.Reals.MultiSum",
            &[("nin", Value::Integer(2)), (name, Value::Real(1.0))],
        )
        .expect_err("refused");
        let ClassPortsError::InvalidParameters { diagnostics, .. } = error else {
            panic!("{name}: {error:?}");
        };
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec![DiagCode::UnknownParameter],
            "{name}"
        );
    }
    let ports = class_ports(
        "CDL.Reals.MultiSum",
        &[("nin", Value::Integer(2)), ("k_2", Value::Real(1.0))],
    )
    .expect("declared names name ports");
    assert_eq!(ports.inputs.len(), 2);
}
