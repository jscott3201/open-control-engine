//! Shared test support: loading the G36 catalog fixtures through the facade, the named-path
//! oracle, and builders that turn a schedule of slot-ordered frames into each source.
//!
//! The fixture list is `oce-api`'s blob corpus (`crates/oce-api/tests/support/blob_corpus.rs`)
//! and the frames are `oce-graph`'s shared schedule (`tests/support/frame_schedule.rs`), both
//! included by path by the test crates that need them, so this crate restates neither.

#![allow(dead_code)]

use std::path::PathBuf;

use oce_api::{Engine, NonFiniteInputPolicy, Value, ValueType};
use oce_replay::{
    Column, ColumnKind, ColumnMap, ColumnSpec, ColumnarSource, InputMap, MissingPolicy,
    ReplayLimits, StepSource, TimeColumn,
};

/// The workspace root.
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The CXF bytes of one G36 catalog fixture.
pub fn g36_bytes(name: &str) -> Vec<u8> {
    let path = workspace_root().join(format!("crates/oce-cxf/tests/fixtures/g36/{name}.jsonld"));
    std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

/// A fresh engine with `bytes` loaded.
pub fn load_bytes(bytes: &[u8]) -> Engine {
    let mut engine = Engine::new();
    engine
        .load_cxf(bytes)
        .unwrap_or_else(|err| panic!("load: {err}"));
    engine
}

/// A fresh engine with one G36 fixture loaded.
pub fn load(name: &str) -> Engine {
    load_bytes(&g36_bytes(name))
}

/// The column kind that carries `value_type`.
pub fn kind_of(value_type: ValueType) -> ColumnKind {
    match value_type {
        ValueType::Real => ColumnKind::Real,
        ValueType::Integer => ColumnKind::Integer,
        ValueType::Boolean => ColumnKind::Boolean,
        ValueType::Enum(_) => ColumnKind::Enum("enum".to_owned()),
        ValueType::String => panic!("no String inputs"),
    }
}

/// One column per input, named by its full canonical path.
pub fn path_specs(engine: &Engine) -> Vec<ColumnSpec> {
    engine
        .input_definitions()
        .unwrap()
        .iter()
        .map(|d| ColumnSpec::new(d.path.clone(), kind_of(d.value_type)))
        .collect()
}

/// The exact-path map of `engine` with `missing` as the run-level policy.
pub fn path_map(engine: &Engine, missing: &MissingPolicy) -> InputMap {
    let specs = path_specs(engine);
    InputMap::resolve(
        &engine.input_definitions().unwrap(),
        &specs,
        &ColumnMap::by_name(&specs),
        missing,
        &[],
    )
    .unwrap()
}

/// Frames as `(t, values in slot order)`, every value present.
pub type Frames = Vec<(f64, Vec<Value>)>;

/// A columnar source over `frames` (every cell present).
pub fn columnar(map: &InputMap, frames: &Frames) -> ColumnarSource {
    columnar_with_missing(map, frames, &[])
}

/// A columnar source over `frames` with the listed `(frame, slot)` cells missing (NaN for Real,
/// a cleared validity flag otherwise).
pub fn columnar_with_missing(
    map: &InputMap,
    frames: &Frames,
    missing: &[(usize, usize)],
) -> ColumnarSource {
    let time = TimeColumn::Seconds(frames.iter().map(|(t, _)| *t).collect());
    let gone = |k: usize, slot: usize| missing.contains(&(k, slot));
    let columns = (0..map.len())
        .map(|slot| {
            let valid: Vec<bool> = (0..frames.len()).map(|k| !gone(k, slot)).collect();
            let column = match map.inputs()[slot].value_type {
                ValueType::Real => Column::Real(
                    frames
                        .iter()
                        .enumerate()
                        .map(|(k, (_, v))| {
                            if gone(k, slot) {
                                f64::NAN
                            } else {
                                v[slot].as_real().unwrap()
                            }
                        })
                        .collect(),
                ),
                ValueType::Integer => Column::Integer {
                    values: frames
                        .iter()
                        .map(|(_, v)| v[slot].as_integer().unwrap())
                        .collect(),
                    valid: Some(valid),
                },
                ValueType::Boolean => Column::Boolean {
                    values: frames
                        .iter()
                        .map(|(_, v)| matches!(v[slot], Value::Boolean(true)))
                        .collect(),
                    valid: Some(valid),
                },
                other => panic!("unsupported input type {other:?}"),
            };
            (map.column(slot).to_owned(), column)
        })
        .collect();
    ColumnarSource::new(map, time, columns, &ReplayLimits::default()).unwrap()
}

/// A step source over `frames`: each input's change list keeps only the frames where its value
/// changes, and every frame time is declared as an instant so it is a row.
pub fn steps(map: &InputMap, frames: &Frames) -> StepSource {
    let lists = (0..map.len())
        .map(|slot| {
            let mut list: Vec<(f64, Value)> = Vec::new();
            for (t, values) in frames {
                let changed = list
                    .last()
                    .is_none_or(|(_, last)| !bits_eq(last, &values[slot]));
                if changed {
                    list.push((*t, values[slot].clone()));
                }
            }
            (map.column(slot).to_owned(), list)
        })
        .collect();
    let instants = frames.iter().map(|(t, _)| *t).collect();
    StepSource::with_instants(map, lists, instants, &ReplayLimits::default()).unwrap()
}

/// Value equality by exact bits.
pub fn bits_eq(a: &Value, b: &Value) -> bool {
    bits(a) == bits(b)
}

/// A value's kind tag and exact bits.
pub fn bits(value: &Value) -> (u8, u64) {
    match value {
        Value::Real(x) => (0, x.to_bits()),
        Value::Integer(x) => (1, *x as u64),
        Value::Boolean(x) => (2, u64::from(*x)),
        Value::Enum { ordinal, .. } => (3, u64::from(*ordinal)),
        Value::String(_) => (4, 0),
    }
}

/// The named path (`prepare_frame` then `execute_frame`) over `frames` under the `Refuse`
/// non-finite policy: per frame, `Some(outputs in slot order)` or `None` when refused.
pub fn named_path(mut engine: Engine, frames: &Frames) -> Vec<Option<Vec<Value>>> {
    engine.set_non_finite_input_policy(NonFiniteInputPolicy::Refuse);
    let definitions = engine.input_definitions().unwrap();
    frames
        .iter()
        .map(|(t, values)| {
            let entries: Vec<(&str, Value)> = definitions
                .iter()
                .zip(values)
                .map(|(d, v)| (d.path.as_str(), v.clone()))
                .collect();
            let prepared = engine.prepare_frame(*t, &entries).ok()?;
            let completed = engine.execute_frame(prepared).ok()?;
            Some(completed.outputs().iter().map(|(_, v)| v.clone()).collect())
        })
        .collect()
}
