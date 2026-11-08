//! Same outputs on both paths (GitHub #400): for every G36 catalog fixture, the facade's `std`
//! path and the executable decoded from its compiled-schedule blob take the same frames, and after
//! load and after every tick every boundary output and the whole `RunState` (connector values,
//! state words, slot layout and time) are compared bit for bit.
//!
//! The frames are a deterministic per-input schedule over each fixture's own acceptance domain:
//! Reals sweep the declared range (or a span around zero when unbounded, signed zero included),
//! Integers step through the low end of their range, and Booleans toggle at a different period per
//! input. The Tier-2 golden input schedules live in `oce-conformance` test binaries, which reach
//! the engine only through the facade and cannot see run state, so they are not reused here. The
//! comparison with the frame driver itself is #401's.

use oce_graph::blob::decode;
use oce_graph::{EvalContext, RunState, eval_tick};
use oce_model::{Value, ValueType};

use crate::{Engine, InputDefinition};

const TICKS: u32 = 160;
const STEP: f64 = 0.75;

fn fixture_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oce-cxf/tests/fixtures/g36")
}

fn fixtures() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixture_dir())
        .unwrap()
        .filter_map(|entry| {
            let name = entry.unwrap().file_name().into_string().unwrap();
            name.strip_suffix(".jsonld").map(str::to_owned)
        })
        .filter(|name| name != "member_list_interface")
        .collect();
    names.sort_unstable();
    names
}

/// The value of input `slot` at tick `k`, inside the definition's acceptance domain.
fn input_value(definition: &InputDefinition, slot: usize, k: u32) -> Value {
    let phase = (k as usize * 7 + slot * 3) % 11;
    match definition.value_type {
        ValueType::Real => {
            let min = definition.min.as_ref().and_then(|v| v.as_real().ok());
            let max = definition.max.as_ref().and_then(|v| v.as_real().ok());
            let x = match (min, max) {
                (Some(lo), Some(hi)) => lo + (hi - lo) * (phase as f64 / 10.0),
                (Some(lo), None) => lo + phase as f64 * 1.5,
                (None, Some(hi)) => hi - phase as f64 * 1.5,
                (None, None) if phase == 0 => -0.0,
                (None, None) => (phase as f64 - 5.0) * 4.25 + 20.0,
            };
            Value::Real(x)
        }
        ValueType::Integer => {
            let lo = definition
                .min
                .as_ref()
                .and_then(|v| v.as_integer().ok())
                .unwrap_or(i64::from(i32::MIN));
            let hi = definition
                .max
                .as_ref()
                .and_then(|v| v.as_integer().ok())
                .unwrap_or(i64::from(i32::MAX));
            let base = if lo <= 0 && 0 <= hi { 0 } else { lo };
            let top = hi.min(base + 6);
            Value::Integer(base + (phase as i64 % (top - base + 1)))
        }
        ValueType::Boolean => Value::Boolean((k as usize / (slot + 1) + slot).is_multiple_of(2)),
        ValueType::String => Value::String("".into()),
        ValueType::Enum(class) => Value::Enum { class, ordinal: 1 },
    }
}

fn assert_states_equal(name: &str, when: &str, host: &RunState, target: &RunState) {
    assert_eq!(
        host.values.len(),
        target.values.len(),
        "{name} {when}: values"
    );
    for (index, (h, t)) in host.values.iter().zip(&target.values).enumerate() {
        assert!(
            h.bit_eq(t),
            "{name} {when}: connector {index}: {h:?} vs {t:?}"
        );
    }
    assert_eq!(host.words, target.words, "{name} {when}: state words");
    assert_eq!(host.slot_of, target.slot_of, "{name} {when}: slot_of");
    assert_eq!(host.slots.len(), target.slots.len(), "{name} {when}: slots");
    for (h, t) in host.slots.iter().zip(&target.slots) {
        assert_eq!(
            (h.block, h.offset, h.len),
            (t.block, t.offset, t.len),
            "{name} {when}"
        );
    }
    assert_eq!(
        host.scratch.len(),
        target.scratch.len(),
        "{name} {when}: scratch"
    );
    assert_eq!(host.t.to_bits(), target.t.to_bits(), "{name} {when}: t");
}

#[test]
fn the_facade_and_the_decoded_blob_agree_bit_for_bit_after_every_tick() {
    let names = fixtures();
    assert_eq!(names.len(), 46);
    for name in &names {
        let cxf = std::fs::read(fixture_dir().join(format!("{name}.jsonld"))).unwrap();
        let mut engine = Engine::new();
        engine.load_cxf(&cxf).unwrap();
        let blob = engine.compile_blob().unwrap();
        let mut decoded = decode(&blob.bytes).unwrap();
        assert_states_equal(name, "after load", &engine.state, &decoded.state);
        assert_eq!(
            decoded.inputs.len(),
            engine.io.frame_definitions.len(),
            "{name}"
        );
        assert_eq!(
            decoded.outputs.len(),
            engine.io.frame_outputs.len(),
            "{name}"
        );
        for (output, (path, source)) in decoded.outputs.iter().zip(&engine.io.frame_outputs) {
            assert_eq!(
                (output.path, output.source),
                (path.as_str(), *source),
                "{name}"
            );
        }
        for k in 0..TICKS {
            let time = f64::from(k) * STEP;
            let observations: Vec<(String, Value)> = engine
                .io
                .frame_definitions
                .iter()
                .enumerate()
                .map(|(slot, d)| (d.path.clone(), input_value(d, slot, k)))
                .collect();
            let entries: Vec<(&str, Value)> = observations
                .iter()
                .map(|(p, v)| (p.as_str(), v.clone()))
                .collect();
            let frame = engine.prepare_frame(time, &entries).unwrap();
            let completed = engine.execute_frame(frame).unwrap();

            for (input, (_, value)) in decoded.inputs.iter().zip(&observations) {
                for target in &input.targets {
                    decoded.state.values[target.0 as usize] = value.clone();
                }
            }
            let mut ctx = EvalContext {
                model: &decoded.model,
                schedule: &decoded.schedule,
                blocks: &decoded.blocks,
                diagnostics: &oce_blocks::NoopDiagnostics,
                state: &mut decoded.state,
            };
            eval_tick(&mut ctx, time);

            let when = format!("tick {k}");
            assert_eq!(completed.outputs().len(), decoded.outputs.len(), "{name}");
            for ((path, value), output) in completed.outputs().iter().zip(&decoded.outputs) {
                assert_eq!(path, output.path, "{name} {when}");
                let target = &decoded.state.values[output.source.0 as usize];
                assert!(
                    value.bit_eq(target),
                    "{name} {when}: {path}: {value:?} vs {target:?}"
                );
            }
            assert_states_equal(name, &when, &engine.state, &decoded.state);
        }
    }
}
