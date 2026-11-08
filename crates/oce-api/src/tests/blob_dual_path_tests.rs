//! Same outputs on both paths (GitHub #400): for every G36 catalog fixture, the facade's `std`
//! path and the executable decoded from its compiled-schedule blob take the same frames, and after
//! load and after every tick every boundary output and the whole `RunState` (connector values,
//! state words, slot layout and time) are compared bit for bit.
//!
//! The frames are a deterministic per-input schedule over each fixture's own acceptance domain,
//! with steps from 0.75 s to 300 s so the frames span about 3.3 hours:
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
/// The step before each frame cycles through these seconds, so 160 frames span about 3.3 hours
/// and the G36 timers (5 to 15 minutes) expire, alongside sub-second and one-second steps.
const STEPS: [f64; 5] = [0.75, 1.0, 15.0, 60.0, 300.0];
/// The values an input with no declared bounds cycles through.
const UNBOUNDED_REALS: [f64; 11] = [-0.0, 0.1, 1.0, 0.4, 22.5, 0.6, 0.05, 293.15, 0.7, 2.0, -5.0];

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
    // Each input holds a value for a slot-dependent number of frames (1 to 7), so a condition can
    // persist across the long steps that time alarms and holds.
    let held = k as usize / [2, 5, 8, 11, 14, 1, 4][slot % 7];
    let phase = (held * 7 + slot * 3) % 11;
    match definition.value_type {
        ValueType::Real => {
            let min = definition.min.as_ref().and_then(|v| v.as_real().ok());
            let max = definition.max.as_ref().and_then(|v| v.as_real().ok());
            let x = match (min, max) {
                (Some(lo), Some(hi)) => lo + (hi - lo) * (phase as f64 / 10.0),
                (Some(lo), None) => lo + phase as f64 * 1.5,
                (None, Some(hi)) => hi - phase as f64 * 1.5,
                // Unbounded: small magnitudes (flows, fractions, signed zero) and plant-scale ones.
                (None, None) => UNBOUNDED_REALS[phase],
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
        ValueType::Boolean => Value::Boolean((held / (slot % 3 + 1) + slot).is_multiple_of(2)),
        ValueType::String => Value::String("".into()),
        ValueType::Enum(class) => Value::Enum { class, ordinal: 1 },
    }
}

/// Compare the host's run state with the decoded executable's, bit for bit.
fn compare_states(when: &str, host: &RunState, target: &RunState) -> Result<(), String> {
    let fail = |what: String| Err(format!("{when}: {what}"));
    if host.values.len() != target.values.len() {
        return fail("connector count".into());
    }
    for (index, (h, t)) in host.values.iter().zip(&target.values).enumerate() {
        if !h.bit_eq(t) {
            return fail(format!("connector {index}: {h:?} vs {t:?}"));
        }
    }
    if host.words != target.words {
        return fail("state words".into());
    }
    if host.slot_of != target.slot_of || host.slots.len() != target.slots.len() {
        return fail("slot layout".into());
    }
    for (h, t) in host.slots.iter().zip(&target.slots) {
        if (h.block, h.offset, h.len) != (t.block, t.offset, t.len) {
            return fail(format!("slot of block {}", h.block.0));
        }
    }
    if host.scratch.len() != target.scratch.len() {
        return fail("scratch".into());
    }
    if host.t.to_bits() != target.t.to_bits() {
        return fail("time".into());
    }
    Ok(())
}

/// What one fixture's run exercised.
struct Exercised {
    /// Whether the executable has any state words.
    has_state: bool,
    /// Whether the state words ever differed from their value after load.
    words_changed: bool,
    /// How many boundary outputs took more than one value.
    varying_outputs: usize,
}

/// Drive fixture `name` through `TICKS` frames on both paths and compare after load and after
/// every tick. `perturb_at` stages a different value into the decoded side's first input target
/// at one tick, the negative control: the comparison must then fail.
fn drive(name: &str, perturb_at: Option<u32>) -> Result<Exercised, String> {
    let cxf = std::fs::read(fixture_dir().join(format!("{name}.jsonld"))).unwrap();
    let mut engine = Engine::new();
    engine.load_cxf(&cxf).unwrap();
    let blob = engine.compile_blob().unwrap();
    let mut decoded = decode(&blob.bytes).unwrap();
    compare_states("after load", &engine.state, &decoded.state)?;
    if decoded.inputs.len() != engine.io.frame_definitions.len()
        || decoded.outputs.len() != engine.io.frame_outputs.len()
    {
        return Err("I/O table sizes differ".into());
    }
    for (output, (path, source)) in decoded.outputs.iter().zip(&engine.io.frame_outputs) {
        if (output.path, output.source) != (path.as_str(), *source) {
            return Err(format!("output {path} differs"));
        }
    }
    let initial_words = engine.state.words.clone();
    let mut words_changed = false;
    let mut first_outputs: Option<Vec<Value>> = None;
    let mut varying = vec![false; decoded.outputs.len()];
    let mut time = 0.0;
    for k in 0..TICKS {
        if k > 0 {
            time += STEPS[k as usize % STEPS.len()];
        }
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
        if perturb_at == Some(k)
            && let Some(target) = decoded.inputs.first().and_then(|i| i.targets.first())
        {
            let slot = &mut decoded.state.values[target.0 as usize];
            *slot = match &*slot {
                Value::Real(x) => Value::Real(x + 1.0),
                Value::Integer(n) => Value::Integer(n.wrapping_add(1)),
                Value::Boolean(b) => Value::Boolean(!b),
                other => other.clone(),
            };
        }
        let mut ctx = EvalContext {
            model: &decoded.model,
            schedule: &decoded.schedule,
            blocks: &decoded.blocks,
            diagnostics: &oce_blocks::NoopDiagnostics,
            state: &mut decoded.state,
        };
        eval_tick(&mut ctx, time);

        let when = format!("tick {k} (t = {time} s)");
        if completed.outputs().len() != decoded.outputs.len() {
            return Err(format!("{when}: output count"));
        }
        for ((path, value), output) in completed.outputs().iter().zip(&decoded.outputs) {
            let target = &decoded.state.values[output.source.0 as usize];
            if path != output.path || !value.bit_eq(target) {
                return Err(format!("{when}: {path}: {value:?} vs {target:?}"));
            }
        }
        compare_states(&when, &engine.state, &decoded.state)?;

        words_changed |= engine.state.words != initial_words;
        let now: Vec<Value> = completed.outputs().iter().map(|(_, v)| v.clone()).collect();
        match &first_outputs {
            None => first_outputs = Some(now),
            Some(first) => {
                for ((flag, a), b) in varying.iter_mut().zip(first).zip(&now) {
                    *flag |= !a.bit_eq(b);
                }
            }
        }
    }
    Ok(Exercised {
        has_state: !initial_words.is_empty(),
        words_changed,
        varying_outputs: varying.iter().filter(|v| **v).count(),
    })
}

/// Per fixture, how many boundary outputs the frames must move off their first value: the count
/// observed when the schedule was introduced. The eleven fixed-limit economizer fixtures have no
/// boundary input, so their single output is constant by construction (floor 0). A schedule
/// change that stops exercising an output fails here.
const VARYING_OUTPUT_FLOORS: &[(&str, usize)] = &[
    ("ahu_economizer", 4),
    ("ahu_supply_air_temp_reset", 2),
    ("cooling_only_active_air_flow", 3),
    ("cooling_only_alarms", 1),
    ("cooling_only_controller", 8),
    ("cooling_only_dampers", 2),
    ("cooling_only_system_requests", 2),
    ("generic_air_economizer_high_limits_ashrae_differential", 1),
    ("generic_air_economizer_high_limits_ashrae_fixed_18", 0),
    ("generic_air_economizer_high_limits_ashrae_fixed_21", 0),
    ("generic_air_economizer_high_limits_ashrae_fixed_24", 0),
    (
        "generic_air_economizer_high_limits_title24_differential_offset_0",
        1,
    ),
    (
        "generic_air_economizer_high_limits_title24_differential_offset_1",
        1,
    ),
    (
        "generic_air_economizer_high_limits_title24_differential_offset_2",
        1,
    ),
    (
        "generic_air_economizer_high_limits_title24_differential_offset_3",
        1,
    ),
    ("generic_air_economizer_high_limits_title24_fixed_21", 0),
    ("generic_air_economizer_high_limits_title24_fixed_22", 0),
    ("generic_air_economizer_high_limits_title24_fixed_23", 0),
    ("generic_air_economizer_high_limits_title24_fixed_24", 0),
    ("generic_time_suppression", 1),
    (
        "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21",
        4,
    ),
    ("multizone_vav_economizer_enable", 3),
    ("multizone_vav_economizer_limits_common", 5),
    ("multizone_vav_economizer_modulations_reliefs", 2),
    ("multizone_vav_economizer_modulations_return_fan", 1),
    (
        "multizone_vav_economizer_modulations_return_fan_relief_damper",
        2,
    ),
    ("multizone_vav_freeze_protection", 10),
    ("multizone_vav_outdoor_airflow_ahu", 4),
    ("multizone_vav_outdoor_airflow_sumzone", 4),
    ("multizone_vav_outdoor_airflow_title24_ahu", 6),
    ("multizone_vav_outdoor_airflow_title24_sumzone", 3),
    ("multizone_vav_plant_requests", 4),
    ("multizone_vav_relief_damper", 1),
    ("multizone_vav_relief_fan", 4),
    ("multizone_vav_relief_fan_group", 9),
    ("multizone_vav_return_fan_airflow_tracking", 2),
    ("multizone_vav_return_fan_direct_pressure", 5),
    ("multizone_vav_supply_fan", 2),
    ("multizone_vav_supply_signals", 3),
    ("multizone_vav_supply_temperature", 1),
    ("reheat_overrides", 2),
    ("thermal_zones_control_loops", 2),
    ("thermal_zones_zone_states", 1),
    ("trim_and_respond_have_hol_false", 1),
    ("vav_single_zone", 4),
    ("ventilation_zones_ashrae62_1_setpoints", 4),
];

#[test]
fn the_facade_and_the_decoded_blob_agree_bit_for_bit_after_every_tick() {
    let names = fixtures();
    assert_eq!(names.len(), 46);
    assert_eq!(names.len(), VARYING_OUTPUT_FLOORS.len());
    let total: f64 = (1..TICKS).map(|k| STEPS[k as usize % STEPS.len()]).sum();
    assert!(total > 3.0 * 3600.0, "the frames span {total} s");
    for (name, (floor_name, floor)) in names.iter().zip(VARYING_OUTPUT_FLOORS) {
        assert_eq!(name, floor_name);
        let exercised = drive(name, None).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(
            !exercised.has_state || exercised.words_changed,
            "{name}: the frames never changed a state word"
        );
        assert!(
            exercised.varying_outputs >= *floor,
            "{name}: {} varying outputs, below the recorded floor {floor}",
            exercised.varying_outputs
        );
    }
}

#[test]
fn a_perturbed_input_on_the_decoded_side_fails_the_comparison() {
    let error = drive("ahu_economizer", Some(3))
        .err()
        .expect("the comparison must fail");
    assert!(error.starts_with("tick 3"), "{error}");
}
