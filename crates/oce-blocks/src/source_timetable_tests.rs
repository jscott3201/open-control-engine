use oce_model::{EnumClassId, ParamTable, Value};

use crate::source_timetable::{
    EXTRAPOLATION_MEMBERS, SMOOTHNESS_MEMBERS, TIMETABLE_EXTRAPOLATION_DEFAULT_MEMBER,
    TIMETABLE_SMOOTHNESS_DEFAULT_MEMBER, extrapolation_from_member, smoothness_from_member,
};

use super::{
    Block, BlockKind, Ctx, IntegerTimeTable, LogicalTimeTable, NoopDiagnostics, PortKind,
    RealTimeTable, Time, TimeTableExtrapolation, TimeTableSmoothness, lookup,
};

fn table_params(cells: &[(usize, usize, f64)]) -> Vec<(std::sync::Arc<str>, Value)> {
    cells
        .iter()
        .map(|(row, col, value)| (format!("table_{row}_{col}").into(), Value::Real(*value)))
        .collect()
}

fn outs_at(block: &dyn Block, t: Time) -> Vec<Value> {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let mut out = Vec::new();
    block.step_algebraic(&cx, &[], &mut |idx, val| {
        assert_eq!(idx, out.len());
        out.push(val);
    });
    out
}

fn assert_reals(got: &[Value], want: &[f64]) {
    assert_eq!(got.len(), want.len());
    for (idx, (got, want)) in got.iter().zip(want).enumerate() {
        assert!(
            got.bit_eq(&Value::Real(*want)),
            "port {idx}: got {got:?}, want {want:?}; full output {got:?}"
        );
    }
}

fn assert_values(got: &[Value], want: &[Value]) {
    assert_eq!(got.len(), want.len());
    for (idx, (got, want)) in got.iter().zip(want).enumerate() {
        assert!(
            got.bit_eq(want),
            "port {idx}: got {got:?}, want {want:?}; full output {got:?}"
        );
    }
}

fn values_bit_eq(left: &[Value], right: &[Value]) -> bool {
    assert_eq!(
        left.len(),
        right.len(),
        "discrimination controls compare equal-arity outputs"
    );
    left.iter()
        .zip(right)
        .all(|(left, right)| left.bit_eq(right))
}

fn real_default_fixture(extra: Option<(&str, Value)>) -> RealTimeTable {
    let mut values = table_params(&[
        (1, 1, 0.0),
        (1, 2, 0.0),
        (2, 1, 1.0),
        (2, 2, 10.0),
        (3, 1, 2.0),
        (3, 2, 30.0),
    ]);
    if let Some((name, value)) = extra {
        values.push((name.into(), value));
    }
    RealTimeTable::from_params(&ParamTable { values })
}

fn integer_default_fixture(extra: Option<Value>) -> IntegerTimeTable {
    let mut values = table_params(&[
        (1, 1, 0.0),
        (1, 2, 0.0),
        (2, 1, 2.0),
        (2, 2, 10.0),
        (3, 1, 4.0),
        (3, 2, 30.0),
    ]);
    values.push(("period".into(), Value::Real(6.0)));
    if let Some(value) = extra {
        values.push(("timeScale".into(), value));
    }
    IntegerTimeTable::from_params(&ParamTable { values })
}

fn logical_default_fixture(extra: Option<Value>) -> LogicalTimeTable {
    let mut values = table_params(&[
        (1, 1, 0.0),
        (1, 2, 0.0),
        (2, 1, 2.0),
        (2, 2, 1.0),
        (3, 1, 4.0),
        (3, 2, 0.0),
    ]);
    values.push(("period".into(), Value::Real(6.0)));
    if let Some(value) = extra {
        values.push(("timeScale".into(), value));
    }
    LogicalTimeTable::from_params(&ParamTable { values })
}

#[test]
fn authored_defaults_match_explicit_values_and_non_defaults_change_outputs() {
    let probes = [0.5, 2.5];
    let absent = real_default_fixture(None);
    for (name, default, non_default) in [
        (
            "smoothness",
            Value::String("LinearSegments".into()),
            Value::String("ConstantSegments".into()),
        ),
        (
            "extrapolation",
            Value::String("Periodic".into()),
            Value::String("HoldLastPoint".into()),
        ),
        ("offset_1", Value::Real(0.0), Value::Real(5.0)),
        ("timeScale", Value::Real(1.0), Value::Real(2.0)),
    ] {
        let explicit = real_default_fixture(Some((name, default)));
        for t in probes {
            assert_values(&outs_at(&absent, t), &outs_at(&explicit, t));
        }
        let changed = real_default_fixture(Some((name, non_default)));
        assert!(
            probes
                .iter()
                .any(|t| !values_bit_eq(&outs_at(&absent, *t), &outs_at(&changed, *t))),
            "{name} positive control did not change outputs"
        );
    }

    let absent = integer_default_fixture(None);
    let explicit = integer_default_fixture(Some(Value::Real(1.0)));
    assert_values(&outs_at(&absent, 2.5), &outs_at(&explicit, 2.5));
    assert!(
        !values_bit_eq(
            &outs_at(&absent, 2.5),
            &outs_at(&integer_default_fixture(Some(Value::Real(2.0))), 2.5)
        ),
        "integer timeScale positive control did not change outputs"
    );

    let absent = logical_default_fixture(None);
    let explicit = logical_default_fixture(Some(Value::Real(1.0)));
    assert_values(&outs_at(&absent, 2.5), &outs_at(&explicit, 2.5));
    assert!(
        !values_bit_eq(
            &outs_at(&absent, 2.5),
            &outs_at(&logical_default_fixture(Some(Value::Real(2.0))), 2.5)
        ),
        "logical timeScale positive control did not change outputs"
    );
}

#[test]
fn enum_catalog_members_map_to_the_authored_typed_defaults() {
    assert_eq!(
        smoothness_from_member(TIMETABLE_SMOOTHNESS_DEFAULT_MEMBER),
        TimeTableSmoothness::LinearSegments
    );
    assert_eq!(
        extrapolation_from_member(TIMETABLE_EXTRAPOLATION_DEFAULT_MEMBER),
        TimeTableExtrapolation::Periodic
    );
    assert_eq!(
        TimeTableSmoothness::from_value(&Value::String("LinearSegments".into())),
        Some(TimeTableSmoothness::LinearSegments)
    );
    assert_eq!(
        TimeTableExtrapolation::from_value(&Value::String("Periodic".into())),
        Some(TimeTableExtrapolation::Periodic)
    );
}

#[test]
fn real_time_table_interpolates_offsets_and_duplicate_timestamps_after_event() {
    let mut values = table_params(&[
        (1, 1, 0.0),
        (1, 2, 0.0),
        (1, 3, 10.0),
        (2, 1, 1.0),
        (2, 2, 0.0),
        (2, 3, 20.0),
        (3, 1, 1.0),
        (3, 2, 1.0),
        (3, 3, 30.0),
        (4, 1, 2.0),
        (4, 2, 4.0),
        (4, 3, 40.0),
        (5, 1, 3.0),
        (5, 2, 9.0),
        (5, 3, 50.0),
    ]);
    values.extend([
        (
            "smoothness".into(),
            Value::Enum {
                class: EnumClassId::SMOOTHNESS,
                ordinal: 1,
            },
        ),
        (
            "extrapolation".into(),
            Value::Enum {
                class: EnumClassId::EXTRAPOLATION,
                ordinal: 2,
            },
        ),
        ("offset_1".into(), Value::Real(0.5)),
        ("offset_2".into(), Value::Real(-1.0)),
    ]);
    let block = RealTimeTable::from_params(&ParamTable { values });
    assert_eq!(block.kind(), BlockKind::Algebraic);
    assert_eq!(block.state_len(), 0);
    assert_eq!(
        block.resolved_signature().outputs.as_ref(),
        &[PortKind::Real, PortKind::Real]
    );

    assert_reals(&outs_at(&block, 1.0), &[1.5, 29.0]);
    assert_reals(&outs_at(&block, 1.5), &[3.0, 34.0]);
    assert_reals(&outs_at(&block, 4.0), &[14.5, 59.0]);
}

#[test]
fn real_time_table_constant_hold_and_periodic_modes_match_source_contract() {
    let mut constant = table_params(&[
        (1, 1, 0.0),
        (1, 2, 2.0),
        (2, 1, 2.0),
        (2, 2, 6.0),
        (3, 1, 4.0),
        (3, 2, 10.0),
    ]);
    constant.extend([
        (
            "smoothness".into(),
            Value::Enum {
                class: EnumClassId::SMOOTHNESS,
                ordinal: 2,
            },
        ),
        (
            "extrapolation".into(),
            Value::Enum {
                class: EnumClassId::EXTRAPOLATION,
                ordinal: 1,
            },
        ),
    ]);
    let block = RealTimeTable::from_params(&ParamTable { values: constant });
    assert_reals(&outs_at(&block, -1.0), &[2.0]);
    assert_reals(&outs_at(&block, 1.999), &[2.0]);
    assert_reals(&outs_at(&block, 2.0), &[6.0]);
    assert_reals(&outs_at(&block, 9.0), &[10.0]);

    let mut periodic = table_params(&[
        (1, 1, 0.0),
        (1, 2, 0.0),
        (2, 1, 1.0),
        (2, 2, 10.0),
        (3, 1, 2.0),
        (3, 2, 20.0),
    ]);
    periodic.push(("timeScale".into(), Value::Real(2.0)));
    let block = RealTimeTable::from_params(&ParamTable { values: periodic });
    assert_reals(&outs_at(&block, -1.0), &[15.0]);
    assert_reals(&outs_at(&block, 5.0), &[5.0]);
}

#[test]
fn integer_time_table_uses_periodic_step_lookup_with_tolerance() {
    let mut values = table_params(&[
        (1, 1, 0.0),
        (1, 2, -2.0),
        (1, 3, 7.0),
        (2, 1, 2.0),
        (2, 2, 3.0),
        (2, 3, 8.0),
        (3, 1, 5.0),
        (3, 2, 4.0),
        (3, 3, 9.0),
    ]);
    values.push(("period".into(), Value::Real(6.0)));
    let block = IntegerTimeTable::from_params(&ParamTable { values });
    assert_eq!(
        block.resolved_signature().outputs.as_ref(),
        &[PortKind::Integer, PortKind::Integer]
    );
    assert_values(
        &outs_at(&block, -1.0),
        &[Value::Integer(4), Value::Integer(9)],
    );
    assert_values(
        &outs_at(&block, 0.0),
        &[Value::Integer(-2), Value::Integer(7)],
    );
    assert_values(
        &outs_at(&block, 1.999_999_5),
        &[Value::Integer(3), Value::Integer(8)],
    );
    assert_values(
        &outs_at(&block, 6.25),
        &[Value::Integer(-2), Value::Integer(7)],
    );
}

#[test]
fn logical_time_table_maps_periodic_integer_table_through_greater_than_zero() {
    let mut values = table_params(&[
        (1, 1, 0.0),
        (1, 2, 0.0),
        (1, 3, 1.0),
        (2, 1, 1.0),
        (2, 2, 1.0),
        (2, 3, 0.0),
        (3, 1, 3.0),
        (3, 2, 0.0),
        (3, 3, 1.0),
    ]);
    values.push(("period".into(), Value::Real(4.0)));
    let block = LogicalTimeTable::from_params(&ParamTable { values });
    assert_eq!(
        block.resolved_signature().outputs.as_ref(),
        &[PortKind::Boolean, PortKind::Boolean]
    );
    assert_values(
        &outs_at(&block, 0.5),
        &[Value::Boolean(false), Value::Boolean(true)],
    );
    assert_values(
        &outs_at(&block, 1.0),
        &[Value::Boolean(true), Value::Boolean(false)],
    );
    assert_values(
        &outs_at(&block, -0.5),
        &[Value::Boolean(false), Value::Boolean(true)],
    );
}

#[test]
fn source_time_table_registry_constructors_resolve_parameters() {
    let mut real_values = table_params(&[(1, 1, 0.0), (1, 2, 1.0), (2, 1, 2.0), (2, 2, 5.0)]);
    real_values.push((
        "extrapolation".into(),
        Value::Enum {
            class: EnumClassId::EXTRAPOLATION,
            ordinal: 2,
        },
    ));
    let real = (lookup("CDL.Reals.Sources.TimeTable").unwrap().make)(&ParamTable {
        values: real_values,
    });
    assert_reals(&outs_at(real.as_ref(), 1.0), &[3.0]);

    let mut integer_values = table_params(&[(1, 1, 0.0), (1, 2, 2.0)]);
    integer_values.push(("period".into(), Value::Real(3.0)));
    let integer = (lookup("CDL.Integers.Sources.TimeTable").unwrap().make)(&ParamTable {
        values: integer_values,
    });
    assert_values(&outs_at(integer.as_ref(), 0.0), &[Value::Integer(2)]);

    let mut logical_values = table_params(&[(1, 1, 0.0), (1, 2, 1.0)]);
    logical_values.push(("period".into(), Value::Real(3.0)));
    let logical = (lookup("CDL.Logical.Sources.TimeTable").unwrap().make)(&ParamTable {
        values: logical_values,
    });
    assert_values(&outs_at(logical.as_ref(), 0.0), &[Value::Boolean(true)]);
}

#[test]
fn member_token_mappings_agree_with_from_value_for_every_member() {
    for member in SMOOTHNESS_MEMBERS {
        assert_eq!(
            Some(smoothness_from_member(member)),
            TimeTableSmoothness::from_value(&Value::String((*member).into())),
            "{member}"
        );
    }
    for member in EXTRAPOLATION_MEMBERS {
        assert_eq!(
            Some(extrapolation_from_member(member)),
            TimeTableExtrapolation::from_value(&Value::String((*member).into())),
            "{member}"
        );
    }
}

#[test]
fn absent_period_wraps_on_the_unit_modulus() {
    // Times [0, 1] with period absent: the defensive fallback modulus keeps every probe
    // time inside [0, 1), so row 0 is held. A different fallback value would surface
    // row 1 at t >= 1.
    let table = table_params(&[(1, 1, 0.0), (1, 2, 10.0), (2, 1, 1.0), (2, 2, 20.0)]);
    let block = IntegerTimeTable::from_params(&ParamTable { values: table });
    for t in [0.5, 1.5, 2.5] {
        assert_values(&outs_at(&block, t), &[Value::Integer(10)]);
    }
}
