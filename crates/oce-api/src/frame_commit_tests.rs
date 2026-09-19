//! Fault controls for every actual precommit stage, and an instrumented evaluator boundary.
//! Private time/sequence injection reaches states unavailable through the opaque public plan.
//! There is no invented fallible postcommit stage. State bytes are preservation controls, not oracles.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "../tests/support/recording_store.rs"]
mod recording_store;
use recording_store::{RecordingStore, StoreCallSnapshot};

const DELAY: &[u8] = include_bytes!("../tests/fixtures/frame_delay.jsonld");

fn loaded() -> Engine<RecordingStore> {
    let mut engine = Engine::with_store(Arc::new(RecordingStore::default()));
    engine.load_cxf(DELAY).unwrap();
    engine
}

fn plan<S: Store>(engine: &Engine<S>, time: f64) -> PreparedInputFrame {
    engine
        .prepare_frame(
            time,
            &[
                ("urn:frame:a", Value::Real(-0.0)),
                ("urn:frame:b", Value::Real(2.0)),
            ],
        )
        .unwrap()
}

fn bits(values: impl Iterator<Item = Value>) -> Vec<crate::state::WireValue> {
    values
        .map(|value| crate::state::WireValue::from_value(&value).unwrap())
        .collect()
}

fn image<S: Store>(engine: &Engine<S>) -> String {
    let checkpoint = engine
        .checkpoint()
        .map(|c| crate::state_codec::encode_snapshot(&c.image, true).unwrap())
        .map_err(|e| e.to_string());
    let snapshot = engine
        .state_snapshot()
        .map(|s| s.into_bytes())
        .map_err(|e| e.to_string());
    format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{}|{:?}|{}|{}|{}|{:?}|{:?}|{:?}",
        bits(engine.state.values.iter().cloned()),
        engine.state.words,
        bits(engine.state.scratch.iter().cloned()),
        engine.state.slots,
        engine.state.slot_of,
        engine.state.t.to_bits(),
        engine.prev_t.map(f64::to_bits),
        engine.durable_restore_ready,
        engine.accepted_frame_sequence,
        engine.params_dirty,
        bits(engine.outputs.iter().map(|(_, value)| value.clone())),
        checkpoint,
        snapshot
    )
}

#[test]
fn each_preflight_refusal_preserves_every_execution_bit_and_sequence() {
    for advanced in [false, true] {
        for cause in [
            "unloaded",
            "dirty",
            "stale",
            "nan",
            "infinity",
            "negative-infinity",
            "regression",
            "range",
            "sequence",
        ] {
            let mut engine = loaded();
            let retained = if advanced {
                let plan = plan(&engine, 4.0);
                Some(engine.execute_frame(plan).unwrap())
            } else {
                None
            };
            let mut frame = plan(&engine, 4.0);
            match cause {
                "unloaded" => engine.loaded = false,
                "dirty" => {
                    engine.halt().unwrap();
                    engine
                        .set_param("urn:frame:delay.samplePeriod", Value::Real(2.0))
                        .unwrap();
                }
                "stale" => frame.generation = Arc::new(()),
                "nan" => frame.time = f64::NAN,
                "infinity" => frame.time = f64::INFINITY,
                "negative-infinity" => frame.time = f64::NEG_INFINITY,
                "regression" => {
                    engine.prev_t = Some(5.0);
                    engine.state.t = 5.0;
                }
                "range" => frame.time = f64::MAX,
                "sequence" => engine.accepted_frame_sequence = u64::MAX,
                _ => unreachable!(),
            }
            let before = image(&engine);
            let retained_before = format!("{retained:?}");
            engine.store().reset_calls();
            engine.store().arm_hot_path_guard();
            let error = engine.execute_frame(frame).unwrap_err();
            assert!(
                match cause {
                    "unloaded" => matches!(error, OcError::State(EngineStateError::NoLoadedModel)),
                    "dirty" => matches!(
                        error,
                        OcError::State(EngineStateError::PendingParameterEdits)
                    ),
                    "stale" => matches!(error, OcError::StalePreparedFrame),
                    "nan" | "infinity" | "negative-infinity" =>
                        matches!(error, OcError::NonFiniteTime { .. }),
                    "regression" => matches!(error, OcError::TimeRegression { .. }),
                    "range" => matches!(error, OcError::ModelTimeUnrepresentable { .. }),
                    "sequence" => matches!(error, OcError::FrameSequenceExhausted),
                    _ => unreachable!(),
                },
                "{cause}: {error:?}"
            );
            assert_eq!(image(&engine), before, "{cause}");
            assert_eq!(format!("{retained:?}"), retained_before);
            assert_eq!(
                engine.store().calls(),
                StoreCallSnapshot::default(),
                "{cause}"
            );
        }
    }
}

#[test]
fn final_sequence_is_accepted_once_then_never_wraps_or_rewinds() {
    let mut engine = loaded();
    let checkpoint = engine.checkpoint().unwrap();
    engine.accepted_frame_sequence = u64::MAX - 1;
    let frame = plan(&engine, 0.0);
    assert_eq!(engine.execute_frame(frame).unwrap().sequence(), u64::MAX);
    engine.restore_checkpoint(&checkpoint).unwrap();
    engine.load_cxf(DELAY).unwrap();
    let frame = plan(&engine, 0.0);
    let before = image(&engine);
    assert!(matches!(
        engine.execute_frame(frame),
        Err(OcError::FrameSequenceExhausted)
    ));
    assert_eq!(image(&engine), before);
}

struct CountedBlock {
    inner: Box<dyn oce_blocks::Block>,
    emits: Arc<AtomicUsize>,
    updates: Arc<AtomicUsize>,
}

impl oce_blocks::Block for CountedBlock {
    fn signature(&self) -> &'static oce_blocks::BlockSignature {
        self.inner.signature()
    }
    fn kind(&self) -> oce_blocks::BlockKind {
        self.inner.kind()
    }
    fn feeds_through(&self, input: usize, output: usize) -> bool {
        self.inner.feeds_through(input, output)
    }
    fn emit_from_state(
        &self,
        ctx: &oce_blocks::Ctx<'_>,
        inputs: &[Value],
        words: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        self.emits.fetch_add(1, Ordering::SeqCst);
        self.inner.emit_from_state(ctx, inputs, words, emit);
    }
    fn update_state(&self, ctx: &oce_blocks::Ctx<'_>, inputs: &[Value], words: &mut [u64]) {
        self.updates.fetch_add(1, Ordering::SeqCst);
        self.inner.update_state(ctx, inputs, words);
    }
}

#[test]
fn accepted_transition_emits_and_updates_exactly_once_without_reallocating_run_arenas() {
    let mut engine = loaded();
    let index = engine.state.slots[0].block.0 as usize;
    let emits = Arc::new(AtomicUsize::new(0));
    let updates = Arc::new(AtomicUsize::new(0));
    let inner = engine.blocks.remove(index);
    engine.blocks.insert(
        index,
        Box::new(CountedBlock {
            inner,
            emits: emits.clone(),
            updates: updates.clone(),
        }),
    );
    let words = engine.state.words.as_ptr();
    let values = engine.state.values.as_ptr();
    for expected in 1..=3 {
        let frame = plan(&engine, 0.0);
        engine.execute_frame(frame).unwrap();
        assert_eq!(emits.load(Ordering::SeqCst), expected);
        assert_eq!(
            updates.load(Ordering::SeqCst),
            expected,
            "also detects idempotent double state updates"
        );
        assert_eq!(engine.state.words.as_ptr(), words);
        assert_eq!(engine.state.values.as_ptr(), values);
        assert_eq!(engine.state.t.to_bits(), 0);
        assert_eq!(engine.prev_t.map(f64::to_bits), Some(0));
    }
}

#[test]
fn loaded_empty_executable_commits_without_allocations_or_outputs() {
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(oce_model::ModelGraph::new(), None)
        .unwrap();
    let frame = engine.prepare_frame(-0.0, &[]).unwrap();
    let census = allocation_counter::measure(|| {
        let result = engine.execute_frame(frame).unwrap();
        assert_eq!(result.sequence(), 1);
        assert_eq!(result.time().to_bits(), (-0.0_f64).to_bits());
        assert!(result.outputs().is_empty());
        assert!(result.diagnostics().is_empty());
    });
    assert_eq!(census.count_total, 0);
    assert!(!engine.durable_restore_ready);
}

#[test]
fn retained_context_is_private_and_survives_replacement_without_rebinding() {
    let mut engine = loaded();
    let prepared = plan(&engine, 0.0);
    let result = engine.execute_frame(prepared).unwrap();
    assert!(Arc::ptr_eq(&result._generation, &engine.frame_generation));
    let cloned = result.clone();
    assert!(Arc::ptr_eq(&result._generation, &cloned._generation));
    let debug = format!("{result:?}");
    assert!(!debug.contains("generation"));
    engine.load_cxf(DELAY).unwrap();
    assert!(!Arc::ptr_eq(&result._generation, &engine.frame_generation));
    assert_eq!(format!("{result:?}"), debug);
}
