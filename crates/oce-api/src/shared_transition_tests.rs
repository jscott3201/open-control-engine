//! Bounded HostTick reuse controls, not universal mode or Modelica equivalence.
//! Seam entries detect bypass; a private block counts even idempotent double updates.

use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use oce_blocks::{Block, BlockKind, BlockSignature, Ctx};
use oce_model::{ParamTable, Value};

use crate::{AssertLevel, CollectSpec, Engine, InputSource, OcError, SimSpec};

const PRE: &[u8] = include_bytes!("../tests/fixtures/frame_pre.jsonld");

thread_local! {
    pub(super) static TRANSITIONS: Cell<usize> = const { Cell::new(0) };
}

fn loaded(bytes: &[u8]) -> Engine {
    let mut engine = Engine::in_memory();
    engine.load_cxf(bytes).unwrap();
    engine.set_realtime_epoch_unix_nanos(0);
    engine
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

fn counted_route(route: &str) {
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
    TRANSITIONS.set(0);
    if route == "simulation" {
        let metrics = engine
            .simulate(&SimSpec {
                t_start: 0.0,
                t_stop: 2.0,
                step: 1.0,
                inputs: InputSource::None,
                collect: CollectSpec::All { stride: 1 },
            })
            .unwrap();
        assert_eq!(metrics.ticks, 3);
    } else {
        for time in [0.0, 1.0, 2.0] {
            match route {
                "native" => {
                    let plan = engine.prepare_frame(time, &[]).unwrap();
                    engine.execute_frame(plan).unwrap();
                }
                "tick" => {
                    engine.tick(time).unwrap();
                }
                "realtime" => {
                    engine.step_realtime(time).unwrap();
                }
                _ => unreachable!(),
            }
        }
    }
    assert_eq!(emits.load(Ordering::SeqCst), 3, "{route}: emits");
    assert_eq!(updates.load(Ordering::SeqCst), 3, "{route}: updates");
    assert_eq!(TRANSITIONS.get(), 3, "{route}: shared seam entries");
    assert!(!engine.durable_restore_ready);
    assert_eq!(engine.prev_t.map(f64::to_bits), Some(2.0_f64.to_bits()));
    assert_eq!(engine.state.t.to_bits(), 2.0_f64.to_bits());
    assert!(
        engine
            .get_output("urn:pre:a")
            .unwrap()
            .bit_eq(&Value::Boolean(false))
    );
}

#[test]
fn native_route_enters_core_and_evaluates_once_per_call() {
    counted_route("native");
}

#[test]
fn tick_route_enters_core_and_evaluates_once_per_call() {
    counted_route("tick");
}

#[test]
fn simulation_route_enters_core_and_evaluates_once_per_call() {
    counted_route("simulation");
}

#[test]
fn realtime_route_enters_core_and_evaluates_once_per_call() {
    counted_route("realtime");
}

fn same_image(left: &Engine, right: &Engine) {
    assert_eq!(left.state.words, right.state.words);
    assert_eq!(
        left.prev_t.map(f64::to_bits),
        right.prev_t.map(f64::to_bits)
    );
    assert_eq!(left.state.t.to_bits(), right.state.t.to_bits());
    assert_eq!(left.state.values.len(), right.state.values.len());
    assert!(
        left.state
            .values
            .iter()
            .zip(&right.state.values)
            .all(|(a, b)| a.bit_eq(b))
    );
    let a = left.outputs().to_map();
    let b = right.outputs().to_map();
    assert_eq!(a.len(), b.len());
    assert!(
        a.iter()
            .zip(&b)
            .all(|((ka, va), (kb, vb))| ka == kb && va.bit_eq(vb))
    );
    // Persistence comparison is a determinism control, not the correctness oracle.
    assert_eq!(
        left.state_snapshot().unwrap().as_bytes(),
        right.state_snapshot().unwrap().as_bytes()
    );
}

#[test]
fn fully_driven_add_and_sampled_delay_match_at_equivalent_lifecycle_boundaries() {
    // Independently derived dyadic arithmetic; UnitDelay samples only at integer seconds,
    // emits the preceding sample, and holds it at the intervening half-second evaluations.
    for (bytes, prefix, expected) in [
        (
            include_bytes!("../tests/fixtures/legacy_frame_add.jsonld").as_slice(),
            "urn:legacy-frame",
            [2.0, 99.0, 3.0, 7.0, 5.0],
        ),
        (
            include_bytes!("../tests/fixtures/frame_delay.jsonld").as_slice(),
            "urn:frame",
            [0.0, 0.0, 2.0, 2.0, 3.0],
        ),
    ] {
        for _ in 0..2 {
            let mut native = loaded(bytes);
            let mut tick = loaded(bytes);
            let mut realtime = loaded(bytes);
            let a = format!("{prefix}:a");
            let b = format!("{prefix}:b");
            let y = format!("{prefix}:y");
            let inputs = [2.0, 99.0, 3.0, 7.0, 5.0];
            for (index, value) in expected.into_iter().enumerate() {
                let t = index as f64 * 0.5;
                let entries = [
                    (a.as_str(), Value::Real(0.0)),
                    (b.as_str(), Value::Real(inputs[index])),
                ];
                let plan = native.prepare_frame(t, &entries).unwrap();
                let frame = native.execute_frame(plan).unwrap();
                assert!(frame.outputs()[0].1.bit_eq(&Value::Real(value)));
                for engine in [&mut tick, &mut realtime] {
                    for (key, v) in &entries {
                        engine.set_input(key, v.clone()).unwrap();
                    }
                }
                tick.tick(t).unwrap();
                realtime.step_realtime(t).unwrap();
                // Each simulation is a fresh prefix from the same seeded lifecycle, NOT a
                // continuation through multiple simulate calls on a previously advanced engine.
                let mut sim = loaded(bytes);
                let (a, b) = (a.clone(), b.clone());
                let metrics = sim
                    .simulate(&SimSpec {
                        t_start: 0.0,
                        t_stop: t,
                        step: 0.5,
                        inputs: InputSource::Closure(Box::new(move |t| {
                            vec![
                                (a.clone(), Value::Real(0.0)),
                                (b.clone(), Value::Real(inputs[(t * 2.0) as usize])),
                            ]
                        })),
                        collect: CollectSpec::Named {
                            points: vec![y.clone()],
                            stride: 1,
                        },
                    })
                    .unwrap();
                assert_eq!(metrics.ticks, index as u64 + 1);
                assert_eq!(metrics.trace.columns(), std::slice::from_ref(&y));
                for (row, actual) in metrics.trace.column(0).unwrap().iter().enumerate() {
                    assert!(actual.bit_eq(&Value::Real(expected[row])));
                    assert_eq!(
                        metrics.trace.times()[row].to_bits(),
                        (row as f64 * 0.5).to_bits()
                    );
                }
                for engine in [&tick, &realtime, &sim] {
                    same_image(&native, engine);
                }
            }
        }
    }
}

#[test]
fn rounded_equal_time_pre_grid_keeps_projection_sets_distinct_and_post_transition() {
    // Five grid indices but only two binary64 times: ties round to even. Pre still
    // advances on EVERY call: false,true,false,true,false; Not is its complement.
    // No external inputs exist. All four modes start from the same parameter seed.
    let start = 2.0_f64;
    let stop = f64::from_bits(start.to_bits() + 1);
    let step = f64::EPSILON / 2.0;
    let mut sim = loaded(PRE);
    let metrics = sim
        .simulate(&SimSpec {
            t_start: start,
            t_stop: stop,
            step,
            inputs: InputSource::None,
            collect: CollectSpec::All { stride: 1 },
        })
        .unwrap();
    assert_eq!(metrics.ticks, 5);
    assert_eq!(
        metrics.trace.columns(),
        &["urn:pre:memory.y", "urn:pre:not.y"]
    );
    let mut native = loaded(PRE);
    let mut tick = loaded(PRE);
    let mut realtime = loaded(PRE);
    for (index, time) in [start, start, start, stop, stop].into_iter().enumerate() {
        let value = Value::Boolean(index % 2 == 1);
        let plan = native.prepare_frame(time, &[]).unwrap();
        let frame = native.execute_frame(plan).unwrap();
        assert_eq!(
            frame
                .outputs()
                .iter()
                .map(|(p, _)| p.as_str())
                .collect::<Vec<_>>(),
            ["urn:pre:a", "urn:pre:z"]
        );
        assert!(frame.outputs().iter().all(|(_, v)| v.bit_eq(&value)));
        tick.tick(time).unwrap();
        let report = realtime.step_realtime(time).unwrap();
        assert_eq!(report.written, 2);
        let writes = realtime.durable_batch.writes();
        assert_eq!(
            writes.iter().map(|w| w.key.as_str()).collect::<Vec<_>>(),
            ["urn:pre:memory.y", "urn:pre:not.y"]
        );
        assert_eq!(
            writes[0].sample.value,
            oce_store::OcValue::Bool(index % 2 == 1)
        );
        assert_eq!(
            writes[1].sample.value,
            oce_store::OcValue::Bool(index % 2 == 0)
        );
        assert_eq!(metrics.trace.times()[index].to_bits(), time.to_bits());
        assert!(metrics.trace.column(0).unwrap()[index].bit_eq(&value));
        assert!(metrics.trace.column(1).unwrap()[index].bit_eq(&Value::Boolean(index % 2 == 0)));
        same_image(&native, &tick);
        same_image(&native, &realtime);
    }
    same_image(&native, &sim);
}

#[test]
fn legacy_modes_and_refusals_leave_native_sequence_untouched() {
    let mut engine = loaded(PRE);
    for expected in 1..=3 {
        engine.tick(2.0).unwrap();
        engine.step_realtime(2.0).unwrap();
        engine
            .simulate(&SimSpec {
                t_start: 0.0,
                t_stop: 2.0,
                step: 1.0,
                inputs: InputSource::None,
                collect: CollectSpec::None,
            })
            .unwrap();
        assert_eq!(engine.accepted_frame_sequence, expected - 1);
        assert!(matches!(
            engine.tick(1.0),
            Err(OcError::TimeRegression { .. })
        ));
        assert!(matches!(
            engine.prepare_frame(1.0, &[]),
            Err(OcError::TimeRegression { .. })
        ));
        let plan = engine.prepare_frame(2.0, &[]).unwrap();
        assert_eq!(engine.execute_frame(plan).unwrap().sequence(), expected);
    }
}

#[test]
fn warnings_are_dropped_without_collection_cost_or_retained_by_the_selected_caller() {
    let bytes = include_bytes!("../tests/fixtures/assertion_model.jsonld");
    let mut native = loaded(bytes);
    let mut realtime = loaded(bytes);
    for input in [false, true, false] {
        let plan = native
            .prepare_frame(0.0, &[("urn:assert#u", Value::Boolean(input))])
            .unwrap();
        let frame = native.execute_frame(plan).unwrap();
        realtime
            .set_input("urn:assert#u", Value::Boolean(input))
            .unwrap();
        let report = realtime.step_realtime(0.0).unwrap();
        assert_eq!(frame.diagnostics().len(), usize::from(!input));
        assert_eq!(report.asserts.len(), usize::from(!input));
        for event in frame.diagnostics().iter().chain(&report.asserts) {
            assert_eq!(event.block, "CDL.Utilities.Assert");
            assert_eq!(event.message, "freezestat tripped");
            assert_eq!(event.t.to_bits(), 0);
            assert_eq!(event.level, AssertLevel::Warning);
        }
    }
    // A collector whose records are merely thrown away would still allocate for a warning.
    // Compare identical warmed lifecycles, with silent=true as the allocation control.
    for simulation in [false, true] {
        let census = [true, false].map(|input| {
            let mut engine = loaded(bytes);
            engine
                .set_input("urn:assert#u", Value::Boolean(input))
                .unwrap();
            engine.tick(0.0).unwrap();
            allocation_counter::measure(|| {
                if simulation {
                    engine
                        .simulate(&SimSpec {
                            t_start: 0.0,
                            t_stop: 2.0,
                            step: 1.0,
                            inputs: InputSource::None,
                            collect: CollectSpec::None,
                        })
                        .unwrap();
                } else {
                    engine.tick(0.0).unwrap();
                }
            })
        });
        assert_eq!(census[0].count_total, census[1].count_total);
        assert_eq!(census[0].bytes_total, census[1].bytes_total);
    }
}
