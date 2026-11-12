//! Private slot-entry controls a public test cannot reach: every preflight cause the named path's
//! fault controls inject (sequence exhaustion, a restored clock beyond the limit, a forged stale
//! map) refuses the slot entry with the named path's error and preserves every execution bit; the
//! instrumented core counts one transition per accepted slot frame and none per refusal; the
//! reused warning records match the named path's and stop allocating once warm; Enum and String
//! slots, which no public CXF can declare, refuse like the named path on detached probes; and a
//! hand-built boundary with an output of every value kind is typed by `output_types()` in lexical
//! slot order, exact Enum class included, with every frame's values agreeing (issue #460).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use allocation_counter::measure;
use oce_blocks::{Block, BlockKind, BlockSignature, Ctx};
use oce_model::{
    Attrs, BlockId, BlockInstance, BoundaryOutput, Connector, ConnectorId, Dir, EnumClassId,
    ModelGraph, ParamTable, Value, ValueType, enum_descriptor,
};

use crate::frame::transition_count;
use crate::{Engine, FrameSlots, OcError, PreparedInputFrame};

const DELAY: &[u8] = include_bytes!("../tests/fixtures/frame_delay.jsonld");
const PRE: &[u8] = include_bytes!("../tests/fixtures/frame_pre.jsonld");
const ASSERTION: &[u8] = include_bytes!("../tests/fixtures/assertion_model.jsonld");
const A: &str = "urn:frame:a";
const B: &str = "urn:frame:b";

fn loaded(bytes: &[u8]) -> Engine {
    let mut engine = Engine::new();
    engine.load_cxf(bytes).unwrap();
    engine
}

/// The delay fixture's frame in slot order (`a`, then `b`).
fn values() -> [Value; 2] {
    [Value::Real(-0.0), Value::Real(2.0)]
}

fn plan(engine: &Engine, time: f64) -> PreparedInputFrame {
    let [a, b] = values();
    engine.prepare_frame(time, &[(A, a), (B, b)]).unwrap()
}

/// Every execution bit a refusal must preserve, with checkpoint and snapshot bytes.
fn image(engine: &Engine) -> String {
    let checkpoint = engine
        .checkpoint()
        .map(|c| crate::state_codec::encode_snapshot(&c.image, true).unwrap())
        .map_err(|e| e.to_string());
    let snapshot = engine
        .state_snapshot()
        .map(|s| s.into_bytes())
        .map_err(|e| e.to_string());
    let bits = |values: &[Value]| -> Vec<crate::state::WireValue> {
        values
            .iter()
            .map(|value| crate::state::WireValue::from_value(value).unwrap())
            .collect()
    };
    format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{}|{:?}|{}|{}|{}|{:?}|{:?}",
        bits(&engine.state.values),
        engine.state.words,
        bits(&engine.state.scratch),
        engine.state.slots,
        engine.state.slot_of,
        engine.state.t.to_bits(),
        engine.prev_t.map(f64::to_bits),
        engine.durable_restore_ready,
        engine.accepted_frame_sequence,
        engine.params_dirty,
        checkpoint,
        snapshot
    )
}

const PREFLIGHT_CAUSES: [&str; 11] = [
    "unloaded",
    "dirty",
    "stale",
    "nan",
    "infinity",
    "negative-infinity",
    "regression",
    "range",
    "step",
    "unrepresentable",
    "sequence",
];

/// Put `engine` into the state that refuses `cause` and return the frame time to submit; a
/// "stale" cause replaces the map's incarnation.
fn inject(cause: &str, engine: &mut Engine, slots: &mut FrameSlots) -> f64 {
    let mut time = 4.0;
    match cause {
        "unloaded" => engine.loaded = false,
        "dirty" => {
            engine.halt().unwrap();
            engine
                .set_param("urn:frame:delay.samplePeriod", Value::Real(2.0))
                .unwrap();
        }
        "stale" => slots.generation = Arc::new(()),
        "nan" => time = f64::NAN,
        "infinity" => time = f64::INFINITY,
        "negative-infinity" => time = f64::NEG_INFINITY,
        "regression" => {
            engine.prev_t = Some(5.0);
            engine.state.t = 5.0;
        }
        "range" => time = f64::MAX,
        "step" => {
            // Only a restored clock beyond the magnitude limit can still overflow.
            engine.prev_t = Some(-f64::MAX);
            engine.state.t = -f64::MAX;
            time = crate::MAX_FRAME_TIME_MAGNITUDE;
        }
        // In range, but 1e305 * 1e6 overflows the sampled clock's rounding even fresh.
        "unrepresentable" => time = 1.0e305,
        "sequence" => engine.accepted_frame_sequence = u64::MAX,
        _ => unreachable!(),
    }
    time
}

/// The named path's refusal for the same cause: the plan is prepared before the injection, as
/// the named path's own fault controls do, then forged to the same time and incarnation.
fn named_refusal(cause: &str, advanced: bool) -> OcError {
    let mut engine = loaded(DELAY);
    if advanced {
        let plan = plan(&engine, 4.0);
        engine.execute_frame(plan).unwrap();
    }
    let mut frame = plan(&engine, 4.0);
    let mut slots = engine.frame_slots().unwrap();
    frame.time = inject(cause, &mut engine, &mut slots);
    if cause == "stale" {
        frame.generation = Arc::new(());
    }
    engine.execute_frame(frame).unwrap_err()
}

#[test]
fn every_preflight_cause_refuses_the_slot_entry_like_the_named_path_and_preserves_every_bit() {
    for advanced in [false, true] {
        for cause in PREFLIGHT_CAUSES {
            let mut engine = loaded(DELAY);
            let mut slots = engine.frame_slots().unwrap();
            if advanced {
                engine.execute_slots(&slots, 4.0, &values()).unwrap();
            }
            let time = inject(cause, &mut engine, &mut slots);
            let before = image(&engine);
            let error = engine.execute_slots(&slots, time, &values()).unwrap_err();
            let named = named_refusal(cause, advanced);
            assert_eq!(format!("{error:?}"), format!("{named:?}"), "{cause}");
            assert_eq!(image(&engine), before, "{cause}");
        }
    }
}

#[test]
fn the_final_sequence_is_accepted_once_by_the_slot_entry_then_never_wraps() {
    let mut engine = loaded(DELAY);
    let slots = engine.frame_slots().unwrap();
    engine.accepted_frame_sequence = u64::MAX - 1;
    let sequence = engine
        .execute_slots(&slots, 0.0, &values())
        .unwrap()
        .sequence();
    assert_eq!(sequence, u64::MAX);
    let before = image(&engine);
    assert!(matches!(
        engine.execute_slots(&slots, 1.0, &values()),
        Err(OcError::FrameSequenceExhausted)
    ));
    assert_eq!(image(&engine), before);
}

struct CountedBlock {
    inner: Box<dyn Block>,
    emits: Arc<AtomicUsize>,
    updates: Arc<AtomicUsize>,
}

impl Block for CountedBlock {
    fn signature(&self) -> &'static BlockSignature {
        self.inner.signature()
    }
    fn kind(&self) -> BlockKind {
        self.inner.kind()
    }
    fn feeds_through(&self, input: usize, output: usize) -> bool {
        self.inner.feeds_through(input, output)
    }
    fn state_len(&self) -> usize {
        self.inner.state_len()
    }
    fn init_state(&self, words: &mut [u64], params: &ParamTable) {
        self.inner.init_state(words, params);
    }
    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        inputs: &[Value],
        words: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        self.emits.fetch_add(1, Ordering::SeqCst);
        self.inner.emit_from_state(ctx, inputs, words, emit);
    }
    fn update_state(&self, ctx: &Ctx<'_>, inputs: &[Value], words: &mut [u64]) {
        self.updates.fetch_add(1, Ordering::SeqCst);
        self.inner.update_state(ctx, inputs, words);
    }
}

#[test]
fn accepted_slot_frames_enter_the_shared_core_once_and_refusals_never_enter() {
    let mut engine = loaded(PRE);
    let emits = Arc::new(AtomicUsize::new(0));
    let updates = Arc::new(AtomicUsize::new(0));
    let index = engine.state.slots[0].block.0 as usize;
    let inner = engine.blocks.remove(index);
    engine.blocks.insert(
        index,
        Box::new(CountedBlock {
            inner,
            emits: emits.clone(),
            updates: updates.clone(),
        }),
    );
    let slots = engine.frame_slots().unwrap();
    assert!(slots.inputs().is_empty());
    let base = transition_count();
    for (position, time) in [0.0, 0.0, 2.0].into_iter().enumerate() {
        let frame = engine.execute_slots(&slots, time, &[]).unwrap();
        assert_eq!(frame.sequence(), position as u64 + 1);
        assert!(
            frame
                .outputs()
                .all(|value| value.bit_eq(&Value::Boolean(position % 2 == 1)))
        );
        assert_eq!(emits.load(Ordering::SeqCst), position + 1);
        assert_eq!(updates.load(Ordering::SeqCst), position + 1);
        assert_eq!(transition_count(), base + position + 1);
    }
    let snapshot = engine.state_snapshot().unwrap();
    for (time, values) in [
        (1.0, Vec::new()),
        (f64::NAN, Vec::new()),
        (3.0, vec![Value::Real(0.0)]),
    ] {
        assert!(engine.execute_slots(&slots, time, &values).is_err());
    }
    assert_eq!(transition_count(), base + 3);
    assert_eq!(emits.load(Ordering::SeqCst), 3);
    assert_eq!(updates.load(Ordering::SeqCst), 3);
    assert_eq!(
        snapshot.as_bytes(),
        engine.state_snapshot().unwrap().as_bytes()
    );
    assert!(!engine.durable_restore_ready);
}

#[test]
fn reused_warning_records_match_the_named_path_and_stop_allocating_once_warm() {
    let mut named = loaded(ASSERTION);
    let mut slotted = loaded(ASSERTION);
    let slots = slotted.frame_slots().unwrap();
    let path = slots.inputs()[0].path.clone();
    // Tripped, silent, tripped twice, silent, tripped: each slot frame's warnings are exactly the
    // named path's, and a silent frame shows none of an earlier frame's retained records.
    for (k, ok) in [false, true, false, false, true, false]
        .into_iter()
        .enumerate()
    {
        let time = k as f64;
        let value = Value::Boolean(ok);
        let plan = named
            .prepare_frame(time, &[(path.as_str(), value.clone())])
            .unwrap();
        let completed = named.execute_frame(plan).unwrap();
        let frame = slotted.execute_slots(&slots, time, &[value]).unwrap();
        assert_eq!(
            format!("{:?}", frame.diagnostics()),
            format!("{:?}", completed.diagnostics()),
            "frame {k}"
        );
        assert_eq!(frame.diagnostics().is_empty(), ok, "frame {k}");
    }
    // Warm: the record exists, so further tripped frames overwrite it in place.
    let census = measure(|| {
        for k in 0..16 {
            let frame = slotted
                .execute_slots(&slots, 10.0 + f64::from(k), &[Value::Boolean(false)])
                .unwrap();
            std::hint::black_box(frame.diagnostics());
        }
    });
    assert_eq!(census.count_total, 0, "{census:?}");
    // The positive control: a fresh engine's first tripped frame must allocate its record.
    let mut cold = loaded(ASSERTION);
    let cold_slots = cold.frame_slots().unwrap();
    let first = measure(|| {
        let frame = cold
            .execute_slots(&cold_slots, 0.0, &[Value::Boolean(false)])
            .unwrap();
        std::hint::black_box(frame.diagnostics());
    });
    assert!(first.count_total > 0, "the counter observes allocations");
}

/// A detached probe with one boundary input of `value_type` and a constant output. No registry
/// block has Enum or String signal ports, so no CXF (and no corpus fixture) can declare such an
/// input; the private model gate accepts this probe, as `frame_tests.rs` does for the named path.
fn probe(value_type: ValueType) -> Engine {
    let mut model = ModelGraph::new();
    model.blocks.push(BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from("CDL.Reals.Sources.Constant"),
        inputs: vec![],
        outputs: vec![ConnectorId(0)],
        params: ParamTable {
            values: vec![(Arc::from("k"), Value::Real(0.0))],
        },
        decl_order: 0,
        instance_iri: Some(Arc::from("urn:probe:constant")),
    });
    model.connectors.push(
        Connector::new(ConnectorId(0), BlockId(0), Dir::Out, ValueType::Real, 0)
            .with_iri("urn:probe:y"),
    );
    model.connectors.push(
        Connector::new(ConnectorId(1), BlockId(0), Dir::In, value_type, 1)
            .with_iri("urn:probe:u")
            .with_attrs(Attrs::default_for(value_type))
            .unwrap(),
    );
    model.external_inputs.push(ConnectorId(1));
    let mut engine = Engine::new();
    engine
        .build_model_in_memory(model, Some("urn:probe:model"))
        .unwrap();
    engine
}

/// The probe's run image; detached probes have no state-manifest identity, so no snapshot.
fn run_image(engine: &Engine) -> String {
    format!(
        "{:?} {:?} {} {}",
        engine.state, engine.prev_t, engine.accepted_frame_sequence, engine.durable_restore_ready
    )
}

/// Submit `value` to the probe through both entries; both must refuse with `expected` (by
/// `Debug`) and leave the run image untouched.
fn refuse_on_probe(engine: &mut Engine, value: Value, expected: &OcError) {
    let before = run_image(engine);
    let slots = engine.frame_slots().unwrap();
    let path = slots.inputs()[0].path.clone();
    let named = engine
        .prepare_frame(1.0, &[(path.as_str(), value.clone())])
        .and_then(|plan| engine.execute_frame(plan))
        .unwrap_err();
    let slotted = engine.execute_slots(&slots, 1.0, &[value]).unwrap_err();
    assert_eq!(format!("{named:?}"), format!("{expected:?}"));
    assert_eq!(format!("{slotted:?}"), format!("{expected:?}"));
    assert_eq!(run_image(engine), before);
}

#[test]
fn enum_and_string_slots_refuse_type_and_domain_like_the_named_path() {
    let class = EnumClassId::SIMPLE_CONTROLLER;
    let members = enum_descriptor(class).unwrap().members.len() as u32;
    let mut engine = probe(ValueType::Enum(class));
    let path = "urn:probe:u".to_owned();
    let other_class = Value::Enum {
        class: EnumClassId::SMOOTHNESS,
        ordinal: 1,
    };
    for wrong in [
        other_class,
        Value::Integer(1),
        Value::Real(1.0),
        Value::Boolean(true),
        Value::String("PI".into()),
    ] {
        refuse_on_probe(&mut engine, wrong, &OcError::InputType(path.clone()));
    }
    for ordinal in [0, members + 1, u32::MAX] {
        refuse_on_probe(
            &mut engine,
            Value::Enum { class, ordinal },
            &OcError::InputDomain(path.clone()),
        );
    }
    // Every legal ordinal is accepted by both entries alike.
    let slots = engine.frame_slots().unwrap();
    for ordinal in 1..=members {
        let value = Value::Enum { class, ordinal };
        let sequence = engine
            .execute_slots(&slots, f64::from(ordinal), &[value])
            .unwrap()
            .sequence();
        assert_eq!(sequence, u64::from(ordinal));
    }

    let mut engine = probe(ValueType::String);
    for wrong in [
        Value::Real(0.0),
        Value::Integer(0),
        Value::Boolean(false),
        Value::Enum { class, ordinal: 1 },
    ] {
        refuse_on_probe(&mut engine, wrong, &OcError::InputType(path.clone()));
    }
    let slots = engine.frame_slots().unwrap();
    let frame = engine
        .execute_slots(&slots, 0.0, &[Value::String("owned".into())])
        .unwrap();
    assert_eq!(frame.sequence(), 1);
}

/// A detached probe whose boundary declares an output of every value kind, in an order that is
/// not the lexical one. No registry block has an Enum or String output port, so no CXF (and no
/// corpus fixture) can declare such an output; here they are output connectors no block writes,
/// which hold whatever the test stages. `first` and `last` share the Enum driver: one driver may
/// serve several declarations.
fn output_probe() -> Engine {
    let kinds = [
        ValueType::Real,
        ValueType::Enum(EnumClassId::SIMPLE_CONTROLLER),
        ValueType::String,
        ValueType::Enum(EnumClassId::SMOOTHNESS),
        ValueType::Integer,
        ValueType::Boolean,
    ];
    let mut model = ModelGraph::new();
    model.blocks.push(BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from("CDL.Reals.Sources.Constant"),
        inputs: vec![],
        outputs: vec![ConnectorId(0)],
        params: ParamTable {
            values: vec![(Arc::from("k"), Value::Real(-0.0))],
        },
        decl_order: 0,
        instance_iri: Some(Arc::from("urn:probe:constant")),
    });
    for (index, value_type) in kinds.into_iter().enumerate() {
        let id = ConnectorId(index as u32);
        model.connectors.push(
            Connector::new(id, BlockId(0), Dir::Out, value_type, index as u32)
                .with_iri(format!("urn:probe:driver{index}")),
        );
    }
    // Declaration order: Real, Enum, String, Enum, Integer, Boolean, then the shared Enum driver.
    for (iri, source) in [
        ("urn:probe:out:zeta", 0_u32),
        ("urn:probe:out:first", 1),
        ("urn:probe:out:mid", 2),
        ("urn:probe:out:beta", 3),
        ("urn:probe:out:gamma", 4),
        ("urn:probe:out:delta", 5),
        ("urn:probe:out:last", 1),
    ] {
        model.boundary_outputs.push(BoundaryOutput {
            iri: Arc::from(iri),
            source: ConnectorId(source),
            attrs: Attrs::default_for(kinds[source as usize]),
        });
    }
    let mut engine = Engine::new();
    engine
        .build_model_in_memory(model, Some("urn:probe:outputs"))
        .unwrap();
    engine
}

#[test]
fn every_value_kind_is_typed_in_lexical_slot_order_with_the_exact_enum_class() {
    let controller = EnumClassId::SIMPLE_CONTROLLER;
    let smoothness = EnumClassId::SMOOTHNESS;
    assert_ne!(controller, smoothness);
    let mut engine = output_probe();
    let slots = engine.frame_slots().unwrap();
    // Lexical by declared identity, never declaration order or connector index.
    let paths = [
        "urn:probe:out:beta",
        "urn:probe:out:delta",
        "urn:probe:out:first",
        "urn:probe:out:gamma",
        "urn:probe:out:last",
        "urn:probe:out:mid",
        "urn:probe:out:zeta",
    ];
    assert_eq!(slots.outputs(), paths.map(str::to_owned));
    // An Enum output carries its exact class, not `Integer` and not merely "an enumeration".
    let types = [
        ValueType::Enum(smoothness),
        ValueType::Boolean,
        ValueType::Enum(controller),
        ValueType::Integer,
        ValueType::Enum(controller),
        ValueType::String,
        ValueType::Real,
    ];
    assert_eq!(slots.output_types(), types);

    // Stage values no seed holds into the connectors no block writes, then run both entries: every
    // accepted frame's outputs carry exactly the map's types, class identity included.
    let members = enum_descriptor(controller).unwrap().members.len() as u32;
    assert!(members >= 2);
    let top = Value::Enum {
        class: controller,
        ordinal: members,
    };
    let second = Value::Enum {
        class: smoothness,
        ordinal: 2,
    };
    let staged = [
        (1, top.clone()),
        (2, Value::String("held".into())),
        (3, second.clone()),
        (4, Value::Integer(i64::from(i32::MIN))),
        (5, Value::Boolean(true)),
    ];
    for (connector, value) in &staged {
        engine.state.values[*connector] = value.clone();
    }
    let expected = [
        second,
        Value::Boolean(true),
        top.clone(),
        Value::Integer(i64::from(i32::MIN)),
        top,
        Value::String("held".into()),
        Value::Real(-0.0),
    ];
    for k in 0..3_u32 {
        let time = f64::from(k);
        let accepted = engine.execute_slots(&slots, time, &[]).unwrap();
        assert_eq!(accepted.output_count(), types.len());
        for (slot, value) in accepted.outputs().enumerate() {
            assert_eq!(value.value_type(), types[slot], "frame {k}: slot {slot}");
            assert!(value.bit_eq(&expected[slot]), "frame {k}: slot {slot}");
        }
        let plan = engine.prepare_frame(time, &[]).unwrap();
        let completed = engine.execute_frame(plan).unwrap();
        for (slot, (path, value)) in completed.outputs().iter().enumerate() {
            assert_eq!(path, paths[slot]);
            assert_eq!(value.value_type(), types[slot], "frame {k}: named {path}");
            assert!(value.bit_eq(&expected[slot]), "frame {k}: named {path}");
        }
    }
    // The map read after the frames is the map read before them, a clone carries it, and the
    // read allocates nothing.
    assert_eq!(slots.output_types(), types);
    assert_eq!(slots.clone().output_types(), types);
    let census = measure(|| {
        std::hint::black_box(slots.output_types());
    });
    assert_eq!(census.count_total, 0, "{census:?}");
}

#[test]
fn a_superseded_map_keeps_its_types_and_is_still_refused() {
    let mut engine = output_probe();
    let slots = engine.frame_slots().unwrap();
    let types = slots.output_types().to_vec();
    assert_eq!(types.len(), 7);
    let before = run_image(&engine);
    // A forged incarnation, as a reload or dirty resume leaves an old map: the types are still
    // readable, the frame is refused before any mutation, and a fresh map types the same boundary.
    let mut stale = slots.clone();
    stale.generation = Arc::new(());
    assert_eq!(stale.output_types(), types);
    assert!(matches!(
        engine.execute_slots(&stale, 0.0, &[]),
        Err(OcError::StalePreparedFrame)
    ));
    assert_eq!(run_image(&engine), before);
    assert_eq!(engine.frame_slots().unwrap().output_types(), types);
    assert_eq!(
        engine.execute_slots(&slots, 0.0, &[]).unwrap().sequence(),
        1
    );
}

#[test]
fn the_debug_form_of_a_map_still_prints_counts_only() {
    let engine = output_probe();
    let slots = engine.frame_slots().unwrap();
    assert_eq!(
        format!("{slots:?}"),
        "FrameSlots { inputs: 0, outputs: 7, .. }"
    );
}
