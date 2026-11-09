//! Replay reproduces the engine bit for bit over the whole G36 corpus (design note, test plan:
//! determinism goldens, trace size).
//!
//! * **Every fixture, both sources.** Each of the 46 catalog fixtures (`oce-api`'s blob corpus
//!   list) runs the shared 160-frame schedule of `oce-graph`'s `tests/support/frame_schedule.rs`
//!   (the schedule the blob goldens, run digests and dual-entry tests use, defined for every
//!   fixture) through `ColumnarSource` and through `StepSource` (change-only lists). Every row
//!   must be `Executed`, its time bits the schedule's, and its output bits those the facade's
//!   named path (`prepare_frame`/`execute_frame`) returns for the same frame; the two sources'
//!   canonical traces must be byte-identical.
//! * **Trace size is the formula.** For every fixture the frame columns hold exactly
//!   `rows × (9 + Σ width(output) [+ mask] [+ Σ width(input)])` bytes, with the widths derived
//!   here from the oracle's output values, for outputs only and with inputs retained. The table
//!   it prints is the PR's per-fixture size table.
//! * **The controller's conformance golden.** `cooling_only_controller` driven minute by minute
//!   from its golden-gen reference table, through both sources, gives the outputs of
//!   `crates/oce-conformance/tests/fixtures/golden/g36_traces/cooling_only_controller.csv` bit for
//!   bit. The other 45 traces are driven by input functions written in code inside the
//!   conformance crate's test binaries, which a leverage crate cannot reach; the named-path
//!   comparison above covers them.

#![allow(clippy::print_stdout)] // The size table is the evidence the PR pastes.

// `frame_schedule.rs` names `oce_model::{Value, ValueType}`; the facade re-exports both.
extern crate oce_api as oce_model;

#[path = "../../../oce-api/tests/support/blob_corpus.rs"]
mod blob_corpus;
#[path = "support/controller.rs"]
mod controller;
#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "../../../oce-graph/tests/support/frame_schedule.rs"]
mod frame_schedule;

use blob_corpus::FIXTURES;
use fixtures::{Frames, bits, columnar, load, named_path, path_map, steps};
use frame_schedule::{TICKS, input_value, time_at};
use oce_api::{Engine, Value};
use oce_replay::{
    FrameSource, FrameStatus, MissingPolicy, Replay, ReplayOptions, TimeBase, Trace, TraceColumn,
};

fn schedule(engine: &Engine) -> Frames {
    let definitions = engine.input_definitions().unwrap();
    (0..TICKS)
        .map(|k| {
            let values = definitions
                .iter()
                .enumerate()
                .map(|(slot, d)| input_value(d.value_type, d.min.as_ref(), d.max.as_ref(), slot, k))
                .collect();
            (time_at(k), values)
        })
        .collect()
}

fn cell(column: &TraceColumn, row: usize) -> (u8, u64) {
    match column {
        TraceColumn::Real(v) => (0, v[row].to_bits()),
        TraceColumn::Integer(v) => (1, v[row] as u64),
        TraceColumn::Boolean(v) => (2, u64::from(v[row])),
        TraceColumn::Enum(v) => (3, u64::from(v[row])),
    }
}

fn width(value: &Value) -> u64 {
    match value {
        Value::Real(_) | Value::Integer(_) => 8,
        Value::Boolean(_) => 1,
        Value::Enum { .. } => 4,
        Value::String(_) => panic!("no String outputs"),
    }
}

fn replay<S: FrameSource>(name: &str, map: oce_replay::InputMap, source: S, inputs: bool) -> Trace {
    let mut options = ReplayOptions::new(TimeBase::Recorded);
    options.trace.inputs = inputs;
    let (_, trace) = Replay::new(load(name), map, source, options)
        .unwrap()
        .run()
        .unwrap();
    trace
}

fn assert_matches_oracle(
    name: &str,
    how: &str,
    trace: &Trace,
    frames: &Frames,
    oracle: &[Option<Vec<Value>>],
) {
    assert_eq!(trace.rows(), frames.len(), "{name} {how}: rows");
    let slots = load(name).frame_slots().unwrap();
    let paths: Vec<&str> = trace.outputs().iter().map(|(p, _)| p.as_str()).collect();
    let expected: Vec<&str> = slots.outputs().iter().map(String::as_str).collect();
    assert_eq!(paths, expected, "{name} {how}: output columns");
    for (k, ((t, _), outputs)) in frames.iter().zip(oracle).enumerate() {
        assert_eq!(
            trace.times()[k].to_bits(),
            t.to_bits(),
            "{name} {how}: time {k}"
        );
        assert_eq!(
            trace.status()[k],
            FrameStatus::Executed as u8,
            "{name} {how}: status {k}"
        );
        let outputs = outputs.as_ref().unwrap();
        for (j, (path, column)) in trace.outputs().iter().enumerate() {
            assert_eq!(
                cell(column, k),
                bits(&outputs[j]),
                "{name} {how}: {path} frame {k}"
            );
        }
    }
}

#[test]
fn every_fixture_replays_to_the_named_path_bits_through_both_sources_at_the_formula_size() {
    println!("fixture\tinputs\toutputs\trows\tB/frame outputs only\tB/frame with inputs");
    for name in FIXTURES {
        let engine = load(name);
        let frames = schedule(&engine);
        let oracle = named_path(load(name), &frames);
        assert!(
            oracle.iter().all(Option::is_some),
            "{name}: the schedule is accepted"
        );
        let first = oracle[0].as_ref().unwrap();
        let outputs_width: u64 = first.iter().map(width).sum();
        let inputs_width: u64 = frames[0].1.iter().map(width).sum();

        let map = path_map(&engine, &MissingPolicy::Refuse);
        let from_columns = replay(name, map.clone(), columnar(&map, &frames), false);
        assert_matches_oracle(name, "columnar", &from_columns, &frames, &oracle);
        let from_steps = replay(name, map.clone(), steps(&map, &frames), false);
        assert_matches_oracle(name, "steps", &from_steps, &frames, &oracle);
        assert_eq!(
            from_columns.canonical_bytes(),
            from_steps.canonical_bytes(),
            "{name}: the two sources' traces differ"
        );
        assert!(from_columns.is_clean(), "{name}");

        let rows = frames.len() as u64;
        assert_eq!(from_columns.bytes_per_frame(), 9 + outputs_width, "{name}");
        assert_eq!(
            from_columns.frame_bytes(),
            rows * (9 + outputs_width),
            "{name}"
        );
        let with_inputs = replay(name, map.clone(), columnar(&map, &frames), true);
        assert_matches_oracle(name, "columnar+inputs", &with_inputs, &frames, &oracle);
        let per_frame = 9 + outputs_width + inputs_width;
        assert_eq!(with_inputs.bytes_per_frame(), per_frame, "{name}");
        assert_eq!(with_inputs.frame_bytes(), rows * per_frame, "{name}");
        for (slot, (_, column)) in with_inputs.inputs().unwrap().iter().enumerate() {
            for (k, (_, values)) in frames.iter().enumerate() {
                assert_eq!(
                    cell(column, k),
                    bits(&values[slot]),
                    "{name}: input {slot} frame {k}"
                );
            }
        }
        println!(
            "{name}\t{}\t{}\t{rows}\t{}\t{per_frame}",
            frames[0].1.len(),
            first.len(),
            9 + outputs_width
        );
    }
}

#[test]
fn the_controller_day_replays_to_its_conformance_golden_bit_for_bit_through_both_sources() {
    let reference = controller::reference();
    let golden = controller::golden();
    assert_eq!(reference.rows.len(), 1_441);
    assert_eq!(golden.rows.len(), 1_441);
    let engine = load(controller::FIXTURE);
    let map = controller::map(&engine, &reference, &MissingPolicy::Refuse);
    assert_eq!(
        map.ignored_columns().len(),
        reference.names.len() - 1 - controller::MAPPING.len()
    );
    let traces = [
        replay(
            controller::FIXTURE,
            map.clone(),
            controller::columnar(&map, &reference),
            false,
        ),
        replay(
            controller::FIXTURE,
            map.clone(),
            controller::steps(&map, &reference),
            false,
        ),
    ];
    for trace in &traces {
        assert!(trace.is_clean());
        assert_eq!(trace.rows(), 1_441);
        for (row, golden_row) in golden.rows.iter().enumerate() {
            assert_eq!(trace.times()[row].to_bits(), golden_row[0].to_bits());
            for (index, path) in golden.names.iter().enumerate().skip(1) {
                let (_, column) = trace
                    .outputs()
                    .iter()
                    .find(|(p, _)| p == path)
                    .unwrap_or_else(|| panic!("no output column {path}"));
                let expected = golden_row[index];
                match column {
                    TraceColumn::Real(v) => {
                        assert_eq!(v[row].to_bits(), expected.to_bits(), "{path} row {row}");
                    }
                    #[allow(clippy::cast_precision_loss)]
                    TraceColumn::Integer(v) => {
                        assert_eq!(v[row] as f64, expected, "{path} row {row}")
                    }
                    other => panic!("unexpected column kind {:?}", other.kind()),
                }
            }
        }
    }
    let [columns, steps] = traces.map(|t| t.canonical_bytes());
    assert_eq!(columns, steps, "the two sources' traces differ");
}
