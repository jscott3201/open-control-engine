//! One HostTick v1 frame driver, three callers (GitHub #400, #401): for every G36 catalog fixture
//! the same frames run through
//!
//! * the **facade** (`Engine::prepare_frame` / `execute_frame`),
//! * the **core driver** (`oce_graph::frame::FrameDriver`) on a second load of the same CXF, slot
//!   by slot with no names, and
//! * the **blob entry** (`oce_graph::blob::Executable`) over the executable decoded from the
//!   fixture's compiled-schedule blob,
//!
//! and after load and after every frame the boundary outputs, the sequence, the clock and the whole
//! `RunState` (connector values, state words, slot layout and time) of all three are compared bit
//! for bit. Before some frames each path also receives the same bad frames (a NaN time, time going
//! backwards, a wrong type, a non-finite Real under `NonFiniteInputPolicy::Refuse`, a value outside
//! a declared bound); the three must refuse each with the same refusal for the same slot and leave
//! every bit of state unchanged.
//!
//! The frames come from `oce-graph`'s `tests/support/frame_schedule.rs`, shared by path, and the
//! facade's run of them must reproduce `oce-graph/tests/fixtures/blob_frame_digests.tsv`, which
//! `oce-graph`'s `blob_execution` test also checks against the blob entry built
//! `no_std + alloc`; bless the table deliberately with `OCE_BLESS_FRAME_DIGESTS=1`. The Tier-2
//! golden input schedules live in `oce-conformance` test binaries, which reach the engine only
//! through the facade and cannot see run state, so they are not reused here.

#[path = "../../../oce-graph/tests/support/frame_schedule.rs"]
mod frame_schedule;

use frame_schedule::{DIGEST_TABLE, RunDigest, STEPS, TICKS, input_value, parse_digests, time_at};
use oce_graph::RunState;
use oce_graph::blob::Executable;
use oce_graph::frame::{FrameDriver, FrameInputs, FrameRefusal, InputSlot, NonFiniteInputPolicy};
use oce_model::{ConnectorId, Value, ValueType};

use crate::{Engine, OcError};

fn fixture_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oce-cxf/tests/fixtures/g36")
}

fn digest_table() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../oce-graph")
        .join(DIGEST_TABLE)
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

/// Compare one run state with another, bit for bit.
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

/// One frame's values beside a slot table of `(targets, min, max)`.
struct Slots<'a> {
    table: &'a [(Vec<ConnectorId>, Option<Value>, Option<Value>)],
    values: &'a [Value],
}

impl FrameInputs for Slots<'_> {
    fn slot_count(&self) -> usize {
        self.table.len()
    }
    fn value_count(&self) -> usize {
        self.values.len()
    }
    fn slot(&self, slot: usize) -> InputSlot<'_> {
        let (targets, min, max) = &self.table[slot];
        InputSlot {
            targets,
            min: min.as_ref(),
            max: max.as_ref(),
        }
    }
    fn value(&self, slot: usize) -> &Value {
        &self.values[slot]
    }
}

/// The three callers of the one frame driver.
struct Paths<'b> {
    facade: Engine,
    core: Engine,
    /// The core path's slot table, from its own engine's IO inventory.
    core_slots: Vec<(Vec<ConnectorId>, Option<Value>, Option<Value>)>,
    entry: Executable<'b>,
}

impl Paths<'_> {
    fn set_policy(&mut self, policy: NonFiniteInputPolicy) {
        self.facade.set_non_finite_input_policy(policy);
        self.core.set_non_finite_input_policy(policy);
        self.entry.set_non_finite_input_policy(policy);
    }

    /// Run one frame on all three paths: the facade's completed outputs (or its refusal), and the
    /// core driver's and the entry's sequence (or refusal).
    #[allow(clippy::type_complexity)]
    fn execute(
        &mut self,
        time: f64,
        values: &[Value],
        entry_values: &[Value],
    ) -> (
        Result<crate::CompletedFrame, OcError>,
        Result<u64, FrameRefusal>,
        Result<u64, FrameRefusal>,
    ) {
        let entries: Vec<(&str, Value)> = self
            .facade
            .io
            .frame_definitions
            .iter()
            .zip(values)
            .map(|(definition, value)| (definition.path.as_str(), value.clone()))
            .collect();
        let facade = self
            .facade
            .prepare_frame(time, &entries)
            .and_then(|plan| self.facade.execute_frame(plan));
        let core = &mut self.core;
        let collector = crate::observations::AssertCollector::default();
        let policy = core.non_finite_input_policy;
        let core_result = FrameDriver {
            model: &core.model,
            schedule: &core.schedule,
            blocks: &core.blocks,
            state: &mut core.state,
            prev_t: &mut core.prev_t,
            sequence: &mut core.accepted_frame_sequence,
        }
        .execute(
            time,
            &Slots {
                table: &self.core_slots,
                values,
            },
            policy,
            &collector,
        );
        let entry = self
            .entry
            .execute_frame(time, entry_values, &oce_blocks::NoopDiagnostics);
        (facade, core_result, entry)
    }

    /// Compare all three after an accepted frame or a refusal.
    fn compare(&self, when: &str) -> Result<(), String> {
        compare_states(when, &self.facade.state, &self.core.state)?;
        compare_states(when, &self.facade.state, &self.entry.decoded().state)?;
        let clocks =
            [self.facade.prev_t, self.core.prev_t, self.entry.time()].map(|t| t.map(f64::to_bits));
        let sequences = [
            self.facade.accepted_frame_sequence,
            self.core.accepted_frame_sequence,
            self.entry.sequence(),
        ];
        if clocks[1..].iter().any(|c| *c != clocks[0])
            || sequences[1..].iter().any(|s| *s != sequences[0])
        {
            return Err(format!(
                "{when}: clocks {clocks:?}, sequences {sequences:?}"
            ));
        }
        Ok(())
    }
}

/// A refusal as `(kind, slot or time bits)`, comparable across the facade and the core.
fn refusal_key(refusal: &FrameRefusal) -> String {
    match refusal {
        FrameRefusal::NonFiniteTime { now } => format!("NonFiniteTime {:x}", now.to_bits()),
        FrameRefusal::TimeOutOfRange { now, .. } => format!("TimeOutOfRange {:x}", now.to_bits()),
        FrameRefusal::TimeRegression { now, prev } => {
            format!("TimeRegression {:x} {:x}", now.to_bits(), prev.to_bits())
        }
        FrameRefusal::NonFiniteTimeStep { now, .. } => format!("TimeStep {:x}", now.to_bits()),
        FrameRefusal::ModelTimeUnrepresentable { now } => {
            format!("Unrepresentable {:x}", now.to_bits())
        }
        FrameRefusal::InputCount { .. } => "InputCount".into(),
        FrameRefusal::InputType { slot } => format!("InputType {slot}"),
        FrameRefusal::InputNonFinite { slot } => format!("InputNonFinite {slot}"),
        FrameRefusal::InputDomain { slot } => format!("InputDomain {slot}"),
        FrameRefusal::SequenceExhausted => "SequenceExhausted".into(),
    }
}

/// The facade's refusal in the same key form; a path names its slot by canonical order.
fn facade_key(engine: &Engine, error: &OcError) -> String {
    let slot = |path: &str| {
        engine
            .io
            .frame_definitions
            .iter()
            .position(|definition| definition.path == path)
            .map_or_else(|| format!("unknown path {path}"), |slot| slot.to_string())
    };
    match error {
        OcError::NonFiniteTime { now } => format!("NonFiniteTime {:x}", now.to_bits()),
        OcError::TimeOutOfRange { now, .. } => format!("TimeOutOfRange {:x}", now.to_bits()),
        OcError::TimeRegression { now, prev } => {
            format!("TimeRegression {:x} {:x}", now.to_bits(), prev.to_bits())
        }
        OcError::NonFiniteTimeStep { now, .. } => format!("TimeStep {:x}", now.to_bits()),
        OcError::ModelTimeUnrepresentable { now } => format!("Unrepresentable {:x}", now.to_bits()),
        OcError::InputType(path) => format!("InputType {}", slot(path)),
        OcError::InputNonFinite(path) => format!("InputNonFinite {}", slot(path)),
        OcError::InputDomain(path) => format!("InputDomain {}", slot(path)),
        OcError::FrameSequenceExhausted => "SequenceExhausted".into(),
        other => format!("facade-only {other:?}"),
    }
}

/// A value of another type than `value`'s, for the wrong-type bad frame.
fn wrong_type(value: &Value) -> Value {
    match value {
        Value::Real(_) => Value::Boolean(true),
        Value::Integer(_) => Value::Real(0.0),
        _ => Value::Integer(0),
    }
}

/// A value just above a declared maximum.
fn above(max: &Value) -> Option<Value> {
    match max {
        Value::Real(m) if m.is_finite() => Some(Value::Real(m + m.abs().max(1.0))),
        Value::Integer(m) => m.checked_add(1).map(Value::Integer),
        _ => None,
    }
}

/// A named bad frame: its time, values and the policy it runs under.
type BadFrame = (&'static str, f64, Vec<Value>, NonFiniteInputPolicy);

/// The bad frames tried before frame `k`: each must be refused alike by all three paths.
fn bad_frames(paths: &Paths<'_>, good: &[Value], time: f64) -> Vec<BadFrame> {
    let admit = NonFiniteInputPolicy::Admit;
    let definitions = &paths.facade.io.frame_definitions;
    let mut frames: Vec<BadFrame> = vec![("NaN time", f64::NAN, good.to_vec(), admit)];
    if let Some(prev) = paths.facade.prev_t {
        frames.push(("time going backwards", prev - 1.0, good.to_vec(), admit));
    }
    if let Some(slot) = definitions.len().checked_sub(1) {
        let mut values = good.to_vec();
        values[slot] = wrong_type(&values[slot]);
        frames.push(("wrong type in the last slot", time, values, admit));
    }
    if let Some(slot) = definitions
        .iter()
        .position(|definition| definition.value_type == ValueType::Real)
    {
        let mut values = good.to_vec();
        values[slot] = Value::Real(f64::NAN);
        frames.push((
            "non-finite Real under Refuse",
            time,
            values,
            NonFiniteInputPolicy::Refuse,
        ));
    }
    if let Some((slot, value)) = definitions
        .iter()
        .enumerate()
        .find_map(|(slot, definition)| Some((slot, above(definition.max.as_ref()?)?)))
    {
        let mut values = good.to_vec();
        values[slot] = value;
        frames.push(("above the declared maximum", time, values, admit));
    }
    frames
}

/// What one fixture's run exercised.
struct Exercised {
    /// Whether the executable has any state words.
    has_state: bool,
    /// Whether the state words ever differed from their value after load.
    words_changed: bool,
    /// How many boundary outputs took more than one value.
    varying_outputs: usize,
    /// How many bad frames all three paths refused alike.
    refusals: usize,
    /// The facade's run digest over the accepted frames.
    digest: String,
}

/// Drive fixture `name` through `TICKS` frames on all three paths, trying the bad frames before
/// every 40th frame from the seventh, and compare after load, after every refusal and after every
/// frame. `perturb_at` changes the blob entry's first input at one frame, the negative control: the
/// comparison must then fail.
fn drive(name: &str, perturb_at: Option<u32>) -> Result<Exercised, String> {
    let cxf = std::fs::read(fixture_dir().join(format!("{name}.jsonld"))).unwrap();
    let mut facade = Engine::new();
    facade.load_cxf(&cxf).unwrap();
    let mut core = Engine::new();
    core.load_cxf(&cxf).unwrap();
    let blob = facade.compile_blob().unwrap();
    let entry = Executable::from_blob(&blob.bytes).unwrap();
    let core_slots = core
        .io
        .frame_definitions
        .iter()
        .map(|definition| {
            let binding = core.io.input_binding(&definition.path).unwrap();
            let targets = binding.targets.clone();
            (targets, definition.min.clone(), definition.max.clone())
        })
        .collect();
    let mut paths = Paths {
        facade,
        core,
        core_slots,
        entry,
    };
    paths.compare("after load")?;
    let facade = &paths.facade;
    if paths.entry.inputs().len() != facade.io.frame_definitions.len()
        || paths.entry.outputs().len() != facade.io.frame_outputs.len()
    {
        return Err("I/O table sizes differ".into());
    }
    for (output, (path, source)) in paths.entry.outputs().iter().zip(&facade.io.frame_outputs) {
        if (output.path, output.source) != (path.as_str(), *source) {
            return Err(format!("output {path} differs"));
        }
    }
    let initial_words = facade.state.words.clone();
    let mut words_changed = false;
    let mut first_outputs: Option<Vec<Value>> = None;
    let mut varying = vec![false; facade.io.frame_outputs.len()];
    let mut refusals = 0;
    let mut digest = RunDigest::default();
    for k in 0..TICKS {
        let time = time_at(k);
        let values: Vec<Value> = paths
            .facade
            .io
            .frame_definitions
            .iter()
            .enumerate()
            .map(|(slot, d)| input_value(d.value_type, d.min.as_ref(), d.max.as_ref(), slot, k))
            .collect();
        // The entry derives its frame from the blob's own I/O table, as a target would.
        let mut entry_values: Vec<Value> = paths
            .entry
            .inputs()
            .iter()
            .enumerate()
            .map(|(slot, i)| input_value(i.value_type, i.min.as_ref(), i.max.as_ref(), slot, k))
            .collect();
        if values.len() != entry_values.len()
            || values.iter().zip(&entry_values).any(|(a, b)| !a.bit_eq(b))
        {
            return Err(format!(
                "tick {k}: the blob's I/O table derives other frames"
            ));
        }

        if k % 40 == 7 {
            for (what, bad_time, bad, policy) in bad_frames(&paths, &values, time) {
                let when = format!("tick {k}, {what}");
                paths.set_policy(policy);
                let (facade, core, entry) = paths.execute(bad_time, &bad, &bad);
                paths.set_policy(NonFiniteInputPolicy::Admit);
                let (Err(facade), Err(core), Err(entry)) = (facade, core, entry) else {
                    return Err(format!("{when}: a path accepted the frame"));
                };
                let keys = [
                    facade_key(&paths.facade, &facade),
                    refusal_key(&core),
                    refusal_key(&entry),
                ];
                if keys[1..].iter().any(|key| *key != keys[0]) {
                    return Err(format!("{when}: refusals differ: {keys:?}"));
                }
                paths.compare(&when)?;
                refusals += 1;
            }
        }

        if perturb_at == Some(k)
            && let Some(first) = entry_values.first_mut()
        {
            *first = match &*first {
                Value::Real(x) => Value::Real(x + 1.0),
                Value::Integer(n) => Value::Integer(n.wrapping_add(1)),
                Value::Boolean(b) => Value::Boolean(!b),
                other => other.clone(),
            };
        }
        let when = format!("tick {k} (t = {time} s)");
        let (completed, core, entry) = paths.execute(time, &values, &entry_values);
        let completed = completed.map_err(|error| format!("{when}: facade: {error}"))?;
        let sequences = [
            Ok(completed.sequence()),
            core.map_err(|r| format!("core: {r}")),
            entry.map_err(|r| format!("entry: {r}")),
        ];
        if sequences[1..].iter().any(|s| *s != sequences[0]) {
            return Err(format!("{when}: sequences {sequences:?}"));
        }
        if completed.outputs().len() != paths.entry.outputs().len() {
            return Err(format!("{when}: output count"));
        }
        for (slot, (path, value)) in completed.outputs().iter().enumerate() {
            let source = paths.facade.io.frame_outputs[slot].1;
            let core_value = oce_graph::frame::output_value(&paths.core.state, source);
            let entry = paths.entry.output(slot);
            if entry.is_none_or(|entry| !value.bit_eq(entry)) || !value.bit_eq(core_value) {
                return Err(format!(
                    "{when}: {path}: {value:?} vs {core_value:?} vs {entry:?}"
                ));
            }
        }
        paths.compare(&when)?;

        let state = &paths.facade.state;
        digest.frame(
            time,
            completed.sequence(),
            completed.outputs().iter().map(|(_, value)| value),
            &state.values,
            &state.words,
        );
        words_changed |= state.words != initial_words;
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
        refusals,
        digest: digest.hex(),
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
    let mut digests = String::new();
    let mut refusals = 0;
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
        // Every fixture is refused at least the NaN time and the backwards time, four times over.
        assert!(
            exercised.refusals >= 7,
            "{name}: {} refusals",
            exercised.refusals
        );
        refusals += exercised.refusals;
        digests.push_str(&format!("{name}\t{}\n", exercised.digest));
    }
    assert!(refusals > 46 * 8, "{refusals} refusals across the corpus");
    if oce_bless::enabled("OCE_BLESS_FRAME_DIGESTS") {
        std::fs::write(digest_table(), &digests).unwrap();
    }
    let table = std::fs::read_to_string(digest_table()).unwrap();
    let expected = parse_digests(&table);
    let actual = parse_digests(&digests);
    for ((name, want), (_, got)) in expected.iter().zip(&actual) {
        assert_eq!(
            want, got,
            "{name}: the facade's run digest moved; review and bless with OCE_BLESS_FRAME_DIGESTS=1"
        );
    }
    assert_eq!(expected.len(), actual.len());
}

#[test]
fn a_perturbed_input_on_the_decoded_side_fails_the_comparison() {
    let error = drive("ahu_economizer", Some(3))
        .err()
        .expect("the comparison must fail");
    assert!(error.starts_with("tick 3"), "{error}");
}
