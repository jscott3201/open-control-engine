//! What `simulate` resets between horizons, what it deliberately does not, and what that costs.
//!
//! `simulate` used to clear only the run clock, so a reused engine started the horizon from the
//! words the previous run left behind. It now re-seeds those words, which makes each call a run
//! restart.
//!
//! Three shapes that leave a test green over that defect:
//!
//! - **Fresh engine per run.** `input_staging_tests::a_constant_run_is_bit_reproducible_across_engines`
//!   asserts R-SIM-2 and builds its engine inside its closure, so it compares fresh against fresh
//!   and passed throughout. The carryover tests below reuse one engine.
//! - **Undriven inputs.** A stateful block nothing drives has nothing to carry. The carryover tests
//!   below stage every input of their fixture.
//! - **`y_start = 0.0`.** `IntegratorWithReset::init_state` seeds `[y_start.to_bits(),
//!   PREV_T_UNSET, 0]` (`reals_integrator.rs`), which at `y_start = 0.0` is `[0, u64::MAX, 0]`; at
//!   `t_start = 0.0` the `PREV_T_UNSET`-versus-`0` difference cancels through `tick_dt`. A re-seed
//!   replaced by `words.fill(0)` was therefore invisible to the whole `oce-api` lib target, and a
//!   one-block fixture could not see a re-seed covering only the first `[S]` slot. The fixture
//!   below uses two `[S]` blocks with different non-zero seeds and pins the seeded values by value.
//!
//! The `prev_t` asymmetry between the two refusal gates is owned by
//! `input_staging_tests::a_backwards_tick_still_succeeds_after_a_failed_constant_staging` and
//! `a_failed_collect_still_refuses_a_backwards_tick`, which predate this file.

use super::common::*;
use oce_graph::allocate_state;

/// Two `CDL.Reals.IntegratorWithReset` blocks with distinct non-zero seeds and all inputs
/// host-staged. Connector ids fall in declaration order:
/// `conn#0..2` = first block's `u`/`y_reset_in`/`trigger`, `conn#3` = its `y` (`y_start = 3.25`);
/// `conn#4..6` and `conn#7` the same for the second (`y_start = -1.0`).
///
/// Two blocks, not one, so a re-seed that covers only the first `[S]` slot is detectable; distinct
/// non-zero seeds so a zero-fill is detectable; `k = 1` with `trigger` held false so every tick
/// writes state.
fn two_integrator_model() -> ModelGraph {
    let mut mb = Mb::new();
    let mut inputs = Vec::new();
    for y_start in [3.25_f64, -1.0] {
        let (_, ins, _) = mb.block(
            "CDL.Reals.IntegratorWithReset",
            &[ValueType::Real, ValueType::Real, ValueType::Boolean],
            &[ValueType::Real],
            vec![rp("k", 1.0), rp("y_start", y_start)],
        );
        inputs.extend(ins);
    }
    let mut model = mb.finish();
    model.external_inputs = inputs;
    model
}

fn loaded() -> Engine<MemStore> {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(two_integrator_model(), None)
        .expect("BUILD");
    eng
}

/// Both integrators driven on every step, so both accumulate and a carried start is visible in
/// either column.
fn driven_pairs() -> Vec<(String, Value)> {
    vec![
        ("conn#0".to_string(), Value::Real(2.0)),
        ("conn#1".to_string(), Value::Real(0.0)),
        ("conn#2".to_string(), Value::Boolean(false)),
        ("conn#4".to_string(), Value::Real(5.0)),
        ("conn#5".to_string(), Value::Real(0.0)),
        ("conn#6".to_string(), Value::Boolean(false)),
    ]
}

fn both_columns() -> CollectSpec {
    CollectSpec::Named {
        points: vec!["conn#3".to_string(), "conn#7".to_string()],
        stride: 1,
    }
}

fn driven_spec(t_start: f64, t_stop: f64) -> SimSpec {
    SimSpec {
        t_start,
        t_stop,
        step: 1.0,
        inputs: InputSource::Constant(driven_pairs()),
        collect: both_columns(),
    }
}

/// Both recorded columns as raw bits — the comparison the determinism claim is made in.
fn bits(metrics: &crate::SimMetrics) -> Vec<Vec<u64>> {
    (0..metrics.trace.columns().len())
        .map(|j| {
            metrics
                .trace
                .column(j)
                .expect("column in range")
                .iter()
                .map(|v| match v {
                    Value::Real(r) => r.to_bits(),
                    other => panic!("recorded columns are Real, got {other:?}"),
                })
                .collect()
        })
        .collect()
}

#[test]
fn a_reused_engine_runs_the_horizon_from_the_same_start_as_a_fresh_one() {
    let mut reused = loaded();
    let first = bits(&reused.simulate(&driven_spec(0.0, 10.0)).expect("first"));
    let second = bits(&reused.simulate(&driven_spec(0.0, 10.0)).expect("second"));

    let mut fresh = loaded();
    let independent = bits(&fresh.simulate(&driven_spec(0.0, 10.0)).expect("fresh"));

    assert_eq!(
        first, second,
        "a second identical horizon on one engine must reproduce the first (R-SIM-2)"
    );
    assert_eq!(
        first, independent,
        "and must equal what a freshly loaded engine produces from the same spec"
    );

    // Absolute, not relational: the first sample of each column must be that block's SEEDED value.
    // This is what separates a genuine re-seed from a zero-fill, which no amount of run-to-run
    // agreement can distinguish.
    assert_eq!(
        first[0][0],
        3.25_f64.to_bits(),
        "the first column must start at its authored y_start, not at zero"
    );
    assert_eq!(
        first[1][0],
        (-1.0_f64).to_bits(),
        "and the second at its own distinct y_start — a re-seed covering only the first \
         [S] slot fails here"
    );

    // Both columns must move, or the equalities above are satisfied by a flat trace.
    for (j, col) in first.iter().enumerate() {
        assert!(
            col.windows(2).any(|w| w[0] != w[1]),
            "column {j} must vary over the horizon, or the equalities are vacuous"
        );
    }
}

#[test]
fn a_horizon_starts_from_seeded_words_however_the_engine_was_left() {
    // Corrupt the state arena directly, then run. If the re-seed happens, the corruption cannot
    // reach the trace. The paired assertion below proves the corruption was observable.
    let mut perturbed = loaded();
    perturbed.simulate(&driven_spec(0.0, 10.0)).expect("prime");
    for w in perturbed.state.words.iter_mut() {
        *w = 0x4059_0000_0000_0000; // 100.0
    }
    let after = bits(&perturbed.simulate(&driven_spec(0.0, 10.0)).expect("post"));

    let mut fresh = loaded();
    let reference = bits(&fresh.simulate(&driven_spec(0.0, 10.0)).expect("reference"));
    assert_eq!(
        after, reference,
        "simulate re-seeds the words, so a corrupted arena cannot reach the horizon"
    );

    // The control: the same corruption on the tick path — which does not re-seed — must change the
    // output. Without it, the equality above is satisfied by a perturbation nobody could have seen.
    let mut ticked = loaded();
    for (name, value) in driven_pairs() {
        ticked.set_input(&name, value).expect("stage");
    }
    ticked.tick(0.0).expect("tick 0");
    let before_corruption = ticked.get_output("conn#3").expect("y");
    for w in ticked.state.words.iter_mut() {
        *w = 0x4059_0000_0000_0000;
    }
    ticked.tick(1.0).expect("tick 1");
    let after_corruption = ticked.get_output("conn#3").expect("y");
    assert!(
        !before_corruption.bit_eq(&after_corruption),
        "the corruption must be observable through a path that does not re-seed, \
         or the re-seed assertion proves nothing"
    );
}

#[test]
fn a_staged_input_still_reaches_the_horizon_after_the_reseed() {
    // The reset is `words` only. `set_input`'s contract is that a staged value applies to the next
    // tick, and `simulate` runs ticks — re-seeding `values` too would silently discard the host's
    // staged image. Two different staged values must therefore produce two different traces.
    let staged = |u: f64| {
        let mut eng = loaded();
        for (name, value) in driven_pairs() {
            eng.set_input(&name, value).expect("stage");
        }
        eng.set_input("conn#0", Value::Real(u)).expect("u");
        let spec = SimSpec {
            inputs: InputSource::None,
            ..driven_spec(0.0, 10.0)
        };
        bits(&eng.simulate(&spec).expect("horizon"))
    };
    assert_ne!(
        staged(2.0),
        staged(-7.5),
        "a value staged before simulate must still drive the horizon"
    );
}

#[test]
fn an_undriven_input_inherits_whatever_the_entry_image_holds() {
    // The documented limit, pinned rather than asserted. `InputSource::Constant` writes connector
    // slots DIRECTLY, not through `set_input`, so a previous run's own spec is part of the entry
    // image the next run inherits. A reused engine therefore reproduces a fresh one only when the
    // spec drives every external input the model reads.
    let undriven = SimSpec {
        inputs: InputSource::None,
        ..driven_spec(0.0, 10.0)
    };

    let mut reused = loaded();
    reused
        .simulate(&driven_spec(0.0, 10.0))
        .expect("driven run");
    let after_driven = bits(&reused.simulate(&undriven).expect("undriven run"));

    let mut fresh = loaded();
    let from_fresh = bits(&fresh.simulate(&undriven).expect("undriven run"));

    assert_ne!(
        after_driven, from_fresh,
        "an undriven run inherits the previous run's connector image — this is the documented \
         limit of the re-seed, and if it ever stops being true the rustdoc is wrong"
    );

    // And the other half of the same contract: drive everything, and the two DO agree. Without
    // this, the assertion above would also be satisfied by a re-seed that never worked at all.
    let mut reused_driven = loaded();
    reused_driven
        .simulate(&driven_spec(0.0, 10.0))
        .expect("first");
    let second = bits(
        &reused_driven
            .simulate(&driven_spec(0.0, 10.0))
            .expect("second"),
    );
    assert_eq!(
        second,
        from_fresh_driven(),
        "a fully driven spec reproduces a fresh engine exactly"
    );
}

fn from_fresh_driven() -> Vec<Vec<u64>> {
    let mut eng = loaded();
    bits(&eng.simulate(&driven_spec(0.0, 10.0)).expect("fresh driven"))
}

#[test]
fn a_horizon_is_a_restart_so_chunking_one_run_into_two_does_not_continue_it() {
    // Restart semantics have a cost a host must plan for: two back-to-back calls covering halves of
    // a horizon are not the same as one call covering the whole. Pinned because it is the kind of
    // thing a host discovers from a trajectory rather than from a doc.
    let mut whole = loaded();
    let all = bits(&whole.simulate(&driven_spec(0.0, 20.0)).expect("whole"));

    let mut chunked = loaded();
    chunked.simulate(&driven_spec(0.0, 10.0)).expect("head");
    let tail = bits(&chunked.simulate(&driven_spec(11.0, 20.0)).expect("tail"));

    // Rows 11..=20 of the whole run against the chunked tail, same times either way.
    let whole_tail: Vec<Vec<u64>> = all.iter().map(|c| c[11..=20].to_vec()).collect();
    assert_ne!(
        whole_tail, tail,
        "each simulate re-seeds, so a chunked horizon restarts rather than continuing"
    );
    assert_eq!(
        tail[0][0],
        3.25_f64.to_bits(),
        "the chunked tail starts from the seed, which is what 'restart' means here"
    );
}

#[test]
fn a_refused_name_resolution_does_not_reseed_state_words() {
    // The two name-resolution gates sit above the re-seed. A `Closure` spec resolves its names
    // inside the loop and is a different case, not covered here; `prev_t` is a different case
    // again, owned by the two `input_staging_tests` named in the module header.
    let mut eng = loaded();
    eng.simulate(&driven_spec(0.0, 10.0))
        .expect("a real horizon");
    let before = eng.state.words.clone();
    assert_ne!(
        before,
        allocate_state(&eng.model, &eng.blocks).words,
        "the primed run must leave state DIFFERENT FROM A FRESH SEED, or this test cannot \
         discriminate — a non-zero check would pass on the seed alone, since init_state writes \
         PREV_T_UNSET = u64::MAX"
    );

    let unknown_column = SimSpec {
        collect: CollectSpec::Named {
            points: vec!["nope".to_string()],
            stride: 1,
        },
        ..driven_spec(0.0, 10.0)
    };
    assert!(
        matches!(eng.simulate(&unknown_column), Err(OcError::UnknownPoint(p)) if p == "nope"),
        "an unknown recorded column is refused"
    );
    assert_eq!(
        eng.state.words, before,
        "a refused collect resolution must not re-seed state"
    );

    let unknown_input = SimSpec {
        inputs: InputSource::Constant(vec![("nope".to_string(), Value::Real(1.0))]),
        ..driven_spec(0.0, 10.0)
    };
    assert!(
        matches!(eng.simulate(&unknown_input), Err(OcError::UnknownPoint(p)) if p == "nope"),
        "an unknown Constant input name is refused"
    );
    assert_eq!(
        eng.state.words, before,
        "a refused input resolution must not re-seed state either"
    );
}

/// The store interaction the rustdoc claims, in both directions. It is here because an earlier
/// revision of that rustdoc asserted the opposite — that a store-bound point can never be
/// hand-staged — which is true only when the snapshot has a sample for it.
mod store_bound_staging {
    use super::*;
    use crate::PointValueType;
    use oce_store::{Durability, OcValue, PointSample, PointStatus, PointStore, PointWrite};
    use std::sync::Arc;

    const FIXTURE: &[u8] =
        include_bytes!("../../../oce-cxf/tests/fixtures/g36/ahu_supply_air_temp_reset.jsonld");

    fn trace_with(store: &Arc<MemStore>, point: &str, staged: f64) -> Vec<Vec<u64>> {
        let mut eng = Engine::with_store(Arc::clone(store));
        eng.load_cxf(FIXTURE).expect("load");
        eng.set_input(point, Value::Real(staged)).expect("stage");
        let spec = SimSpec {
            t_start: 0.0,
            t_stop: 3.0,
            step: 1.0,
            inputs: InputSource::None,
            collect: CollectSpec::All { stride: 1 },
        };
        let m = eng.simulate(&spec).expect("horizon");
        (0..m.trace.columns().len())
            .map(|j| {
                m.trace
                    .column(j)
                    .expect("column")
                    .iter()
                    .map(|v| match v {
                        Value::Real(r) => r.to_bits(),
                        Value::Integer(i) => *i as u64,
                        Value::Boolean(b) => u64::from(*b),
                        other => panic!("unexpected {other:?}"),
                    })
                    .collect()
            })
            .collect()
    }

    fn an_input_point(store: &Arc<MemStore>) -> String {
        let mut eng = Engine::with_store(Arc::clone(store));
        eng.load_cxf(FIXTURE).expect("load");
        eng.io()
            .iter()
            .find(|p| {
                p.direction == PointDirection::In && matches!(p.value_type, PointValueType::Real)
            })
            .expect("a Real input")
            .path
            .clone()
    }

    #[test]
    fn a_store_bound_point_with_no_sample_keeps_the_hand_staged_value() {
        let store = Arc::new(MemStore::new());
        let point = an_input_point(&store);
        assert_ne!(
            trace_with(&store, &point, 19.5),
            trace_with(&store, &point, 30.0),
            "with nothing in the store the slot holds last, so set_input drives the horizon"
        );
    }

    #[test]
    fn a_store_sample_overrides_the_hand_staged_value() {
        let store = Arc::new(MemStore::new());
        let point = an_input_point(&store);
        store
            .write_points(&[PointWrite {
                key: oce_store::DomainKey::new(point.clone()),
                sample: PointSample {
                    value: OcValue::Real(22.0),
                    status: PointStatus::Ok,
                    at_unix_nanos: 1,
                },
                durability: Durability::Telemetry,
            }])
            .expect("off-tick write");
        assert_eq!(
            trace_with(&store, &point, 19.5),
            trace_with(&store, &point, 30.0),
            "once the store carries a sample it is re-staged every tick and set_input is overridden"
        );
    }
}
