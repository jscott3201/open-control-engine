//! Non-finite `min`/`max`/`nominal` refusal on Real connectors and boundary declarations.
//!
//! A NaN bound admits no value, so before this rule a boundary input carrying one loaded and then
//! refused every frame. Each non-finite class is checked on each attribute and owner, alongside
//! finite extremes that must still be accepted.

use super::common::*;
use oce_model::BoundaryInput;

const NON_FINITE: [u64; 5] = [
    0x7ff8_0000_0000_0000,
    0xfff8_0000_0000_0000,
    0x7ff8_0000_0000_0042,
    0x7ff0_0000_0000_0000,
    0xfff0_0000_0000_0000,
];

const FINITE_EXTREMES: [f64; 6] = [f64::MAX, f64::MIN, -0.0, 0.0, 5e-324, f64::MIN_POSITIVE];

#[derive(Clone, Copy, Debug)]
enum Field {
    Min,
    Max,
    Nominal,
}

fn attrs(field: Field, value: f64) -> RealAttrs {
    let mut attrs = RealAttrs::default();
    match field {
        Field::Min => attrs.min = Some(value),
        Field::Max => attrs.max = Some(value),
        Field::Nominal => attrs.nominal = Some(value),
    }
    attrs
}

/// A well-formed Constant C0 -> Abs (C1 in, C2 out), with the given attributes on C1 only.
fn edge_model(input: RealAttrs) -> ModelGraph {
    ModelGraph {
        blocks: vec![
            constant_block(0, &[0]),
            block(1, "CDL.Reals.Abs", &[1], &[2]),
        ],
        connectors: vec![
            real_conn(0, 0, Dir::Out, RealAttrs::default()),
            real_conn(1, 1, Dir::In, input),
            real_conn(2, 1, Dir::Out, RealAttrs::default()),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    }
}

/// A root boundary input `urn:b:u` (declaration attributes as given) feeding Abs's external C0.
fn boundary_model(declaration: RealAttrs) -> ModelGraph {
    ModelGraph {
        boundary_inputs: vec![BoundaryInput {
            iri: Arc::from("urn:b:u"),
            attrs: Attrs::Real(declaration),
        }],
        ..real_to_real_model("CDL.Reals.Abs", vec![])
    }
}

fn field_name(field: Field) -> &'static str {
    match field {
        Field::Min => "min",
        Field::Max => "max",
        Field::Nominal => "nominal",
    }
}

#[test]
fn every_non_finite_attribute_on_a_connector_is_refused_before_unification() {
    for field in [Field::Min, Field::Max, Field::Nominal] {
        for bits in NON_FINITE {
            let value = f64::from_bits(bits);
            let mut model = edge_model(attrs(field, value));
            let error = unify_attributes(&mut model).expect_err("non-finite attribute");
            assert_eq!(codes(&error.diagnostics), [DiagCode::NonFiniteBound]);
            let diagnostic = &error.diagnostics[0];
            assert_eq!(diagnostic.severity, Severity::Error);
            assert_eq!(diagnostic.subject.as_deref(), Some("connector#1"));
            assert_eq!(
                diagnostic.message,
                format!(
                    "S231:{} on a Real connector is {value}; a numeric attribute must be finite",
                    field_name(field)
                )
            );
            // Nothing propagated to the unset peer: unification never ran.
            let peer = model.connectors[0].attrs.as_real().unwrap();
            assert!(peer.min.is_none() && peer.max.is_none() && peer.nominal.is_none());
            // The pure checker reports the same cause on its own.
            let error = validate(&edge_model(attrs(field, value))).expect_err("validate refuses");
            assert_eq!(codes(&error.diagnostics), [DiagCode::NonFiniteBound]);
        }
    }
}

#[test]
fn a_nan_bound_against_a_finite_peer_reports_the_root_cause_not_a_mismatch() {
    let mut model = edge_model(attrs(Field::Min, f64::NAN));
    model.connectors[0] = real_conn(0, 0, Dir::Out, attrs(Field::Min, 1.0));
    let error = unify_and_validate(&mut model).expect_err("NaN bound");
    assert_eq!(codes(&error.diagnostics), [DiagCode::NonFiniteBound]);
    // Failure is transactional: both authored bounds are unchanged.
    assert_eq!(
        model.connectors[0]
            .attrs
            .as_real()
            .unwrap()
            .min
            .map(f64::to_bits),
        Some(1.0_f64.to_bits())
    );
    assert!(
        model.connectors[1]
            .attrs
            .as_real()
            .unwrap()
            .min
            .unwrap()
            .is_nan()
    );
}

#[test]
fn boundary_input_and_output_declarations_are_checked() {
    for field in [Field::Min, Field::Max, Field::Nominal] {
        for bits in NON_FINITE {
            let value = f64::from_bits(bits);
            let mut model = boundary_model(attrs(field, value));
            let error = unify_and_validate(&mut model).expect_err("boundary input");
            assert_eq!(codes(&error.diagnostics), [DiagCode::NonFiniteBound]);
            assert_eq!(error.diagnostics[0].subject.as_deref(), Some("urn:b:u"));
            assert!(error.diagnostics[0].message.contains("Real boundary input"));

            let mut model = ModelGraph {
                blocks: vec![constant_block(0, &[0])],
                connectors: vec![real_conn(0, 0, Dir::Out, RealAttrs::default())],
                boundary_outputs: vec![boundary_output(
                    "urn:b:y",
                    0,
                    Attrs::Real(attrs(field, value)),
                )],
                ..ModelGraph::new()
            };
            let error = unify_and_validate(&mut model).expect_err("boundary output");
            assert_eq!(codes(&error.diagnostics), [DiagCode::NonFiniteBound]);
            assert_eq!(error.diagnostics[0].subject.as_deref(), Some("urn:b:y"));
            assert!(
                error.diagnostics[0]
                    .message
                    .contains("Real boundary output")
            );
        }
    }
}

#[test]
fn finite_extremes_are_accepted_on_every_owner_and_attribute() {
    for field in [Field::Min, Field::Max, Field::Nominal] {
        for value in FINITE_EXTREMES {
            unify_and_validate(&mut edge_model(attrs(field, value))).expect("finite connector");
            unify_and_validate(&mut boundary_model(attrs(field, value)))
                .expect("finite boundary declaration");
        }
    }
}

#[test]
fn every_offending_attribute_is_reported_in_deterministic_order() {
    let both = RealAttrs {
        min: Some(f64::NEG_INFINITY),
        max: Some(f64::NAN),
        nominal: Some(f64::INFINITY),
        ..RealAttrs::default()
    };
    let mut model = boundary_model(both.clone());
    model.connectors[0] = real_conn(0, 0, Dir::In, both);
    let first = unify_attributes(&mut model.clone()).expect_err("non-finite");
    for _ in 0..3 {
        let again = unify_attributes(&mut model.clone()).expect_err("non-finite");
        assert_eq!(again.diagnostics, first.diagnostics);
    }
    let rendered: Vec<_> = first
        .diagnostics
        .iter()
        .map(|d| (d.subject.as_deref().unwrap().to_owned(), d.message.clone()))
        .collect();
    let line = |owner: &str, name: &str, value: &str| {
        format!("S231:{name} on a Real {owner} is {value}; a numeric attribute must be finite")
    };
    assert_eq!(
        rendered,
        [
            ("connector#0".into(), line("connector", "max", "NaN")),
            ("connector#0".into(), line("connector", "min", "-inf")),
            ("connector#0".into(), line("connector", "nominal", "inf")),
            ("urn:b:u".into(), line("boundary input", "max", "NaN")),
            ("urn:b:u".into(), line("boundary input", "min", "-inf")),
            ("urn:b:u".into(), line("boundary input", "nominal", "inf")),
        ]
    );
}

#[test]
fn integer_bounds_are_out_of_scope() {
    let mut model = one_block_model(
        "CDL.Integers.Abs",
        &[ValueType::Integer],
        &[ValueType::Integer],
        vec![],
    );
    model.connectors[0] = int_conn(
        0,
        0,
        Dir::In,
        IntAttrs {
            min: Some(i64::MIN),
            max: Some(i64::MAX),
        },
    );
    // Integer declaration attributes carry no f64, so there is nothing to check.
    model.boundary_inputs = vec![BoundaryInput {
        iri: Arc::from("urn:b:n"),
        attrs: Attrs::Integer(IntAttrs::default()),
    }];
    unify_and_validate(&mut model).expect("Integer bounds are always finite");
}
