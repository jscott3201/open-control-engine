//! Executable limits behind the packaged Rust-shape descriptors.

use crate::{
    ContractDomain, IoClass, PointDirection, PointValueType, Value, ValueType, contract_descriptors,
};
use oce_model::{Attrs, BlockId, Connector, Dir, EnumClassId, ModelGraph, RealAttrs};
use std::sync::Arc;

#[test]
fn descriptors_cover_every_domain_with_explicit_shapes_and_semantic_limits() {
    let descriptors = contract_descriptors();
    assert_eq!(
        descriptors
            .iter()
            .map(|descriptor| descriptor.domain)
            .collect::<Vec<_>>(),
        [
            ContractDomain::Catalog,
            ContractDomain::Diagnostics,
            ContractDomain::Io,
            ContractDomain::Values,
            ContractDomain::Parameters,
            ContractDomain::Assertions,
            ContractDomain::ExecutionProfile,
        ]
    );
    for descriptor in descriptors {
        assert_eq!(descriptor.revision, 1);
        let json: serde_json::Value = serde_json::from_str(descriptor.schema).unwrap();
        if descriptor.domain == ContractDomain::Catalog {
            assert_eq!(json["properties"]["schema_revision"]["const"], 1);
        } else {
            assert_eq!(json["revision"], 1);
            assert!(json["types"].is_object());
            assert!(!json["semantics"].as_array().unwrap().is_empty());
        }
    }
    assert_eq!(descriptors, contract_descriptors());
}

#[test]
fn io_schema_keeps_enum_projection_string_omission_and_available_attributes() {
    let mut model = ModelGraph::new();
    let kinds = [
        ValueType::Real,
        ValueType::Integer,
        ValueType::Boolean,
        ValueType::Enum(EnumClassId::SIMPLE_CONTROLLER),
        ValueType::String,
    ];
    for (index, kind) in kinds.into_iter().enumerate() {
        let mut connector = Connector::new(
            crate::ConnectorId(index as u32),
            BlockId(0),
            Dir::In,
            kind,
            index as u32,
        );
        if index == 0 {
            connector.attrs = Attrs::Real(RealAttrs {
                quantity: Some(Arc::from("Temperature")),
                unit: Some(Arc::from("K")),
                display_unit: Some(Arc::from("degC")),
                min: Some(-0.0),
                max: Some(400.0),
                ..RealAttrs::default()
            });
        }
        model.connectors.push(connector);
    }
    let rows = crate::io::point_rows_at_load(&model);
    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows.iter()
            .map(|row| row.info.value_type)
            .collect::<Vec<_>>(),
        [
            PointValueType::Real,
            PointValueType::Int,
            PointValueType::Bool,
            PointValueType::Int
        ]
    );
    assert_eq!(
        rows.iter().map(|row| row.info.io_class).collect::<Vec<_>>(),
        [
            IoClass::AnalogInput,
            IoClass::AnalogInput,
            IoClass::DigitalInput,
            IoClass::AnalogInput
        ]
    );
    assert!(
        rows.iter()
            .all(|row| row.info.direction == PointDirection::In)
    );
    assert_eq!(rows[0].info.min.unwrap().to_bits(), (-0.0_f64).to_bits());
    assert_eq!(rows[0].info.max.unwrap().to_bits(), 400.0_f64.to_bits());
    assert_eq!(rows[0].info.unit.as_deref(), Some("K"));
    assert_eq!(rows[0].info.quantity.as_deref(), Some("Temperature"));
    assert_eq!(rows[0].info.display_unit.as_deref(), Some("degC"));
    assert!(rows[3].info.unit.is_none());
    assert!(
        rows.iter()
            .all(|row| row.info.trend.is_none() && row.info.in_pointlist && !row.info.hardwired)
    );
}

#[test]
fn value_contract_preserves_bits_integer_extremes_and_enum_class_and_ordinal() {
    for bits in [
        0,
        1 << 63,
        1,
        0x7ff0000000000000,
        0xfff0000000000000,
        0x7ff8000000000001,
        0x7ff8000000000002,
    ] {
        let value = Value::Real(f64::from_bits(bits));
        assert!(value.bit_eq(&value.clone()));
        assert!(!value.bit_eq(&Value::Real(f64::from_bits(bits ^ 1))));
    }
    for integer in [
        i64::MIN,
        -2147483648,
        2147483647,
        9007199254740993,
        i64::MAX,
    ] {
        let value = Value::Integer(integer);
        assert!(value.bit_eq(&value.clone()));
        assert!(!value.bit_eq(&Value::Integer(integer ^ 1)));
    }
    for ordinal in [0, 1, 4, 5, u32::MAX] {
        // Construction does not imply validity. The receipt/schema work adds no enum validator.
        let value = Value::Enum {
            class: EnumClassId::SIMPLE_CONTROLLER,
            ordinal,
        };
        assert!(value.bit_eq(&value.clone()));
        assert!(!value.bit_eq(&Value::Enum {
            class: EnumClassId::ZERO_TIME,
            ordinal
        }));
        assert!(!value.bit_eq(&Value::Enum {
            class: EnumClassId::SIMPLE_CONTROLLER,
            ordinal: ordinal ^ 1
        }));
    }
}

#[test]
fn parameter_schema_reports_static_bounds_without_inventing_unit_provenance() {
    let mut model = ModelGraph::new();
    model.blocks.push(oce_model::BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from("CDL.Reals.Limiter"),
        inputs: vec![],
        outputs: vec![],
        params: oce_model::ParamTable {
            values: vec![
                (Arc::from("uMin"), Value::Real(-0.0)),
                (Arc::from("uMax"), Value::Real(10.0)),
            ],
        },
        decl_order: 0,
        instance_iri: Some(Arc::from("urn:limit")),
    });
    let rows = crate::ParamTable::build_at_load(&model).to_vec();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "urn:limit.uMin");
    assert!(rows[0].1.bit_eq(&Value::Real(-0.0)));
    assert!(
        rows.iter()
            .all(|(_, _, attrs)| attrs.unit.is_none() && attrs.quantity.is_none())
    );
    // uMin <= uMax is cross-parameter, so absent static bounds do not mean unconstrained.
    assert!(
        rows.iter()
            .all(|(_, _, attrs)| attrs.min.is_none() && attrs.max.is_none())
    );
    assert!(
        crate::catalog()
            .iter()
            .find(|entry| entry.class_path == "CDL.Reals.Limiter")
            .unwrap()
            .param_rules
            .iter()
            .any(|rule| matches!(
                rule,
                crate::CatalogRule::RealLessOrEqual {
                    lower: "uMin",
                    upper: "uMax"
                }
            ))
    );
}
