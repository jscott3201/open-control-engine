//! Exactly-once HostTick core controls. Counters detect bypass and double evaluation;
//! the hand-derived alternating Pre output detects an incorrect state transition.

use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{Engine, OcError};
use oce_blocks::{Block, BlockKind, BlockSignature, Ctx};
use oce_model::{ParamTable, Value};

thread_local! {
    pub(super) static TRANSITIONS: Cell<usize> = const { Cell::new(0) };
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
fn accepted_frames_enter_the_shared_core_once_and_refusals_never_enter() {
    let mut engine = Engine::in_memory();
    engine
        .load_cxf(include_bytes!("../tests/fixtures/frame_pre.jsonld"))
        .unwrap();
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
    TRANSITIONS.set(0);
    let stale_time = engine.prepare_frame(-1.0, &[]).unwrap();
    for (position, time) in [0.0, 0.0, 2.0].into_iter().enumerate() {
        let prepared = engine.prepare_frame(time, &[]).unwrap();
        assert_eq!(TRANSITIONS.get(), position, "preparation does not evaluate");
        let frame = engine.execute_frame(prepared).unwrap();
        assert_eq!(frame.sequence(), position as u64 + 1);
        assert_eq!(emits.load(Ordering::SeqCst), position + 1);
        assert_eq!(updates.load(Ordering::SeqCst), position + 1);
        assert_eq!(TRANSITIONS.get(), position + 1);
        assert!(
            frame
                .outputs()
                .iter()
                .all(|(_, v)| v.bit_eq(&Value::Boolean(position % 2 == 1)))
        );
    }
    let snapshot = engine.state_snapshot().unwrap();
    assert!(matches!(
        engine.execute_frame(stale_time),
        Err(OcError::TimeRegression { .. })
    ));
    assert!(matches!(
        engine.prepare_frame(f64::NAN, &[]),
        Err(OcError::NonFiniteTime { .. })
    ));
    assert_eq!(TRANSITIONS.get(), 3);
    assert_eq!(emits.load(Ordering::SeqCst), 3);
    assert_eq!(updates.load(Ordering::SeqCst), 3);
    assert_eq!(
        snapshot.as_bytes(),
        engine.state_snapshot().unwrap().as_bytes()
    );
    assert!(!engine.durable_restore_ready);
    assert_eq!(engine.prev_t.map(f64::to_bits), Some(2.0_f64.to_bits()));
    assert_eq!(engine.state.t.to_bits(), 2.0_f64.to_bits());
}
