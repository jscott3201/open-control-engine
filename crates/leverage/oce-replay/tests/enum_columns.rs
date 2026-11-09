//! Enum inputs through every source (decision 21): a one-based `u32` ordinal of the input's own
//! class, the class path recorded from the source's declaration, a cleared validity flag missing.
//!
//! No G36 corpus fixture has an Enum boundary port, and the facade exposes no enum class
//! constructor, so the class identity is taken from an Enum *parameter* of a loaded fixture and
//! the input definitions are synthetic; the engine is not driven.

#[path = "support/fixtures.rs"]
mod fixtures;

use oce_api::{InputDefinition, Value, ValueType};
use oce_replay::{
    Column, ColumnKind, ColumnMap, ColumnSpec, ColumnarSource, CsvFormat, CsvSource, FrameSource,
    InputMap, MapError, MissingPolicy, ReplayLimits, SourceError, StepSource, TimeColumn,
};

const CLASS: &str = "CDL.Types.SimpleController";

/// An enum class identity, from the first Enum parameter of a fixture that has one.
fn enum_value() -> Value {
    for name in [
        "cooling_only_controller",
        "cooling_only_dampers",
        "multizone_vav_economizer_limits_common",
    ] {
        let engine = fixtures::load(name);
        if let Some((_, value, _)) = engine
            .params()
            .iter()
            .find(|(_, v, _)| matches!(v, Value::Enum { .. }))
        {
            return value;
        }
    }
    panic!("no fixture has an Enum parameter");
}

fn setup() -> (InputMap, Value) {
    let value = enum_value();
    let Value::Enum { class, .. } = value.clone() else {
        unreachable!()
    };
    let definitions = vec![
        InputDefinition {
            path: "m.mode".to_owned(),
            value_type: ValueType::Enum(class),
            min: None,
            max: None,
        },
        InputDefinition {
            path: "m.x".to_owned(),
            value_type: ValueType::Real,
            min: None,
            max: None,
        },
    ];
    let specs = [
        ColumnSpec::new("mode", ColumnKind::Enum(CLASS.to_owned())),
        ColumnSpec::new("x", ColumnKind::Real),
    ];
    let map = InputMap::resolve(
        &definitions,
        &specs,
        &ColumnMap::by_name(&specs),
        &MissingPolicy::Refuse,
        &[],
    )
    .unwrap();
    (map, value)
}

fn ordinals(source: &mut dyn FrameSource) -> Vec<Option<u32>> {
    let mut out = Vec::new();
    while let Some((_, cells)) = source.next_row().unwrap() {
        out.push(match &cells[0] {
            Some(Value::Enum { ordinal, .. }) => Some(*ordinal),
            Some(other) => panic!("{other:?}"),
            None => None,
        });
    }
    out
}

#[test]
fn enum_columns_carry_ordinals_of_the_inputs_class_through_every_source() {
    let (map, value) = setup();
    let Value::Enum { class, .. } = value else {
        unreachable!()
    };
    assert_eq!(map.kind(0), &ColumnKind::Enum(CLASS.to_owned()));

    let columns = vec![
        (
            "mode".to_owned(),
            Column::Enum {
                class: CLASS.to_owned(),
                ordinals: vec![1, 2, 3],
                valid: Some(vec![true, false, true]),
            },
        ),
        ("x".to_owned(), Column::Real(vec![0.0; 3])),
    ];
    let mut source = ColumnarSource::new(
        &map,
        TimeColumn::Seconds(vec![0.0, 1.0, 2.0]),
        columns,
        &ReplayLimits::default(),
    )
    .unwrap();
    assert_eq!(ordinals(&mut source), [Some(1), None, Some(3)]);

    // The declared class path must match the spec's.
    let columns = vec![
        (
            "mode".to_owned(),
            Column::Enum {
                class: "CDL.Types.Other".to_owned(),
                ordinals: vec![1],
                valid: None,
            },
        ),
        ("x".to_owned(), Column::Real(vec![0.0])),
    ];
    assert_eq!(
        ColumnarSource::new(
            &map,
            TimeColumn::Seconds(vec![0.0]),
            columns,
            &ReplayLimits::default()
        )
        .unwrap_err(),
        SourceError::ColumnKind {
            column: "mode".to_owned()
        }
    );

    let steps = vec![
        (
            "mode".to_owned(),
            vec![
                (0.0, Value::Enum { class, ordinal: 2 }),
                (5.0, Value::Enum { class, ordinal: 1 }),
            ],
        ),
        ("x".to_owned(), vec![(0.0, Value::Real(1.0))]),
    ];
    let mut source = StepSource::new(&map, steps, &ReplayLimits::default()).unwrap();
    assert_eq!(ordinals(&mut source), [Some(2), Some(1)]);
    // A step of another type is refused.
    let steps = vec![
        ("mode".to_owned(), vec![(0.0, Value::Integer(2))]),
        ("x".to_owned(), vec![(0.0, Value::Real(1.0))]),
    ];
    assert_eq!(
        StepSource::new(&map, steps, &ReplayLimits::default()).unwrap_err(),
        SourceError::BadStep {
            column: "mode".to_owned(),
            step: 0
        }
    );

    let text = "t,mode,x\n0,1,0\n1,,0\n2,3,0\n3,x,0\n";
    let mut source = CsvSource::new(
        &map,
        text.as_bytes(),
        CsvFormat::seconds("t"),
        &ReplayLimits::default(),
    )
    .unwrap();
    let mut seen = Vec::new();
    for _ in 0..3 {
        let (_, cells) = source.next_row().unwrap().unwrap();
        seen.push(cells[0].as_ref().map(|v| match v {
            Value::Enum { ordinal, .. } => *ordinal,
            other => panic!("{other:?}"),
        }));
    }
    assert_eq!(seen, [Some(1), None, Some(3)]);
    assert_eq!(
        source.next_row().unwrap_err(),
        SourceError::BadCell {
            row: 4,
            column: "mode".to_owned()
        }
    );
}

#[test]
fn an_enum_column_cannot_carry_another_kind_and_fallbacks_check_the_class() {
    let (_, value) = setup();
    let Value::Enum { class, .. } = value else {
        unreachable!()
    };
    let definitions = vec![InputDefinition {
        path: "m.mode".to_owned(),
        value_type: ValueType::Enum(class),
        min: Some(Value::Enum { class, ordinal: 1 }),
        max: Some(Value::Enum { class, ordinal: 3 }),
    }];
    let specs = [ColumnSpec::new("mode", ColumnKind::Integer)];
    assert!(matches!(
        InputMap::resolve(
            &definitions,
            &specs,
            &ColumnMap::by_name(&specs),
            &MissingPolicy::Refuse,
            &[]
        ),
        Err(MapError::TypeMismatch { .. })
    ));
    let specs = [ColumnSpec::new("mode", ColumnKind::Enum(CLASS.to_owned()))];
    let fallback = |ordinal: u32| {
        InputMap::resolve(
            &definitions,
            &specs,
            &ColumnMap::by_name(&specs),
            &MissingPolicy::Refuse,
            &[(
                "mode".to_owned(),
                MissingPolicy::Fallback(Value::Enum { class, ordinal }),
            )],
        )
    };
    assert!(fallback(1).is_ok());
    assert!(fallback(3).is_ok());
    assert_eq!(
        fallback(4).unwrap_err(),
        MapError::FallbackDomain {
            input: "m.mode".to_owned()
        }
    );
    assert_eq!(
        fallback(0).unwrap_err(),
        MapError::FallbackDomain {
            input: "m.mode".to_owned()
        }
    );
}
