//! The boundary refusals, over synthetic definitions. No CXF document the engine loads has an
//! Enum or a String boundary port today (the importer refuses a non-scalar pass-through as
//! `non-subset-construct`, and no elementary block takes or gives an Enum signal), so the two
//! codes are pinned here, with an Enum class identity taken from the facade's table.

use oce_api::{InputDefinition, ValueType};

use super::boundary;

fn input(path: &str, value_type: ValueType) -> InputDefinition {
    InputDefinition {
        path: path.to_owned(),
        value_type,
        min: None,
        max: None,
    }
}

fn simple_controller() -> ValueType {
    ValueType::Enum(
        oce_api::enumeration("CDL.Types.SimpleController")
            .and_then(|class| class.id)
            .expect("the facade knows the CDL enumeration"),
    )
}

fn code(
    inputs: &[InputDefinition],
    outputs: &[(&str, ValueType)],
) -> Option<(&'static str, String)> {
    let paths: Vec<String> = outputs.iter().map(|(path, _)| (*path).to_owned()).collect();
    let types: Vec<ValueType> = outputs.iter().map(|(_, value_type)| *value_type).collect();
    boundary(inputs, &paths, &types).err().map(|error| {
        assert_eq!(error.subject(), "subject");
        (error.code(), error.path().unwrap_or_default().to_owned())
    })
}

#[test]
fn real_integer_and_boolean_ports_bind() {
    let inputs = [
        input("urn:x#top.b", ValueType::Boolean),
        input("urn:x#top.n", ValueType::Integer),
        input("urn:x#top.u", ValueType::Real),
    ];
    let outputs = [
        ("urn:x#top.y", ValueType::Real),
        ("urn:x#top.z", ValueType::Boolean),
    ];
    assert_eq!(code(&inputs, &outputs), None);
    assert_eq!(code(&[], &[]), None);
}

#[test]
fn an_enum_input_or_output_is_enum_port_unsupported() {
    assert_eq!(
        code(
            &[input("urn:x#top.mode", simple_controller())],
            &[("urn:x#top.y", ValueType::Real)]
        ),
        Some(("enum-port-unsupported", "urn:x#top.mode".to_owned()))
    );
    assert_eq!(
        code(
            &[input("urn:x#top.u", ValueType::Real)],
            &[("urn:x#top.mode", simple_controller())]
        ),
        Some(("enum-port-unsupported", "urn:x#top.mode".to_owned()))
    );
}

#[test]
fn a_string_input_or_output_is_unsupported_port_type() {
    assert_eq!(
        code(&[input("urn:x#top.label", ValueType::String)], &[]),
        Some(("unsupported-port-type", "urn:x#top.label".to_owned()))
    );
    assert_eq!(
        code(&[], &[("urn:x#top.label", ValueType::String)]),
        Some(("unsupported-port-type", "urn:x#top.label".to_owned()))
    );
}

#[test]
fn inputs_are_judged_in_slot_order_before_any_output() {
    let inputs = [
        input("urn:x#top.a", ValueType::Real),
        input("urn:x#top.b", ValueType::String),
        input("urn:x#top.c", simple_controller()),
    ];
    let outputs = [("urn:x#top.0", simple_controller())];
    assert_eq!(
        code(&inputs, &outputs),
        Some(("unsupported-port-type", "urn:x#top.b".to_owned()))
    );
    assert_eq!(
        code(&inputs[2..], &outputs),
        Some(("enum-port-unsupported", "urn:x#top.c".to_owned()))
    );
}
