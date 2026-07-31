use oce_model::{BlockId, Connector, ConnectorId, Dir, EnumClassId, ModelGraph, Value, ValueType};
use oce_store::{Durability, OcValue, PointStatus};

use crate::engine::sample_to_value;
use crate::io::IoInventory;
use crate::sim::projected_output_batch;

#[test]
fn only_projected_outputs_receive_exact_store_samples() {
    let mut model = ModelGraph::new();
    model.connectors = vec![
        Connector::new(ConnectorId(0), BlockId(0), Dir::Out, ValueType::Real, 0)
            .with_iri("test:temperature"),
        Connector::new(ConnectorId(1), BlockId(0), Dir::Out, ValueType::Boolean, 1)
            .with_iri("test:enabled"),
        Connector::new(
            ConnectorId(2),
            BlockId(0),
            Dir::Out,
            ValueType::Enum(EnumClassId(17)),
            2,
        )
        .with_iri("test:mode"),
        Connector::new(ConnectorId(3), BlockId(0), Dir::Out, ValueType::String, 3)
            .with_iri("test:metadata"),
    ];
    let io = IoInventory::build_at_load(&model);
    let values = vec![
        Value::Real(294.25),
        Value::Boolean(true),
        Value::Enum {
            class: EnumClassId(17),
            ordinal: 3,
        },
        Value::String("metadata".into()),
    ];

    let batch = projected_output_batch(&io, &values, 9_000_000_000);
    assert_eq!(batch.len(), 3, "String output must not be written");
    let expected = [
        ("test:temperature", OcValue::Real(294.25)),
        ("test:enabled", OcValue::Bool(true)),
        ("test:mode", OcValue::Int(3)),
    ];
    for (write, (key, value)) in batch.iter().zip(expected) {
        assert_eq!(write.key.as_str(), key);
        assert_eq!(write.sample.value, value);
        assert_eq!(write.sample.status, PointStatus::Ok);
        assert_eq!(write.sample.at_unix_nanos, 9_000_000_000);
        assert_eq!(write.durability, Durability::Telemetry);
    }
}

#[test]
fn enum_output_carrier_round_trips_as_integer_ordinal() {
    let class = EnumClassId(17);
    let model = ModelGraph {
        connectors: vec![Connector::new(
            ConnectorId(0),
            BlockId(0),
            Dir::Out,
            ValueType::Enum(class),
            0,
        )],
        ..ModelGraph::new()
    };
    let written = projected_output_batch(
        &IoInventory::build_at_load(&model),
        &[Value::Enum { class, ordinal: 3 }],
        42,
    )[0]
    .sample
    .value
    .clone();
    assert_eq!(written, OcValue::Int(3));

    let sample = oce_store::PointSample {
        value: written,
        status: PointStatus::Ok,
        at_unix_nanos: 42,
    };
    let round_tripped = sample_to_value(sample, ValueType::Enum(class), "test:mode").unwrap();
    assert!(round_tripped.bit_eq(&Value::Enum { class, ordinal: 3 }));
}
