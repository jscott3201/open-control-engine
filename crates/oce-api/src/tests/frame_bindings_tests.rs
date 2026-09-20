//! Load-time fan-out type invariants and complete-frame staging of every target.

use super::common::*;

#[test]
fn scrambled_connector_ids_refuse_before_execution() {
    let mut builder = Mb::new();
    let (_, inputs, _) = builder.block(
        "CDL.Reals.Add",
        &[ValueType::Real; 2],
        &[ValueType::Real],
        vec![],
    );
    let mut model = builder.finish();
    model.external_inputs = inputs;
    model.connectors[0].id = ConnectorId(1);
    model.connectors[1].id = ConnectorId(0);
    let mut engine = Engine::in_memory();
    let error = engine.build_model_in_memory(model, None).unwrap_err();
    let OcError::Validate(error) = error else {
        panic!("wrong refusal: {error:?}")
    };
    assert_eq!(error.diagnostics.len(), 2);
    assert!(
        error
            .diagnostics
            .iter()
            .all(|d| d.message.contains("connector id invariant"))
    );
}

fn fanout(types: &[ValueType], class: &str, params: Vec<(Arc<str>, Value)>) -> ModelGraph {
    let mut builder = Mb::new();
    let (_, inputs, _) = builder.block(class, types, &[ValueType::Real], params);
    let mut model = builder.finish();
    for &id in &inputs {
        model.connectors[id.0 as usize].iri = Some(Arc::from("urn:point#u"));
    }
    model.external_inputs = inputs;
    model
}

#[test]
fn heterogeneous_fanout_refuses_during_load() {
    let mut engine = Engine::in_memory();
    let model = fanout(
        &[ValueType::Real, ValueType::Boolean],
        "CDL.Reals.Ramp",
        vec![
            rp("raisingSlewRate", 2.0),
            rp("fallingSlewRate", -3.0),
            rp("Td", 0.1),
        ],
    );
    let error = engine.build_model_in_memory(model, None).unwrap_err();
    assert!(
        matches!(&error, OcError::Store(oce_store::StoreError::Validation(detail))
        if detail.contains("duplicate point DomainKey") && detail.contains("urn:point#u")),
        "{error:?}"
    );
}

#[test]
fn one_logical_input_stages_every_homogeneous_target_on_each_transition() {
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(fanout(&[ValueType::Real; 2], "CDL.Reals.Add", vec![]), None)
        .unwrap();
    let binding = engine.io.input_binding("urn:point#u").unwrap();
    assert_eq!(binding.targets.len(), 2);
    for target in &binding.targets {
        assert_eq!(
            engine.model.connectors[target.0 as usize].value_type,
            ValueType::Real
        );
    }
    for value in [4.0, -2.0, 0.0] {
        let prepared = engine
            .prepare_frame(0.0, &[("urn:point#u", Value::Real(value))])
            .unwrap();
        engine.execute_frame(prepared).unwrap();
        assert!(
            engine
                .get_output("conn#2")
                .unwrap()
                .bit_eq(&Value::Real(value + value))
        );
        for id in [0, 1] {
            assert!(engine.state.values[id].bit_eq(&Value::Real(value)));
        }
    }
}
