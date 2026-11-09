//! Private slot-entry controls a public test cannot reach: every preflight cause the named path's
//! fault controls inject (sequence exhaustion, a restored clock beyond the limit, a forged stale
//! map) refuses the slot entry with the named path's error and preserves every execution bit; the
//! instrumented core counts one transition per accepted slot frame and none per refusal; and the
//! reused warning records match the named path's and stop allocating once warm.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use allocation_counter::measure;
use oce_blocks::{Block, BlockKind, BlockSignature, Ctx};
use oce_model::{ParamTable, Value};

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
