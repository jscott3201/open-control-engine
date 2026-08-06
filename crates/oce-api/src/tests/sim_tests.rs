//! What `simulate` resets between horizons, what it deliberately does not, and what that costs.
//!
//! `simulate` used to clear only the run clock. Every `[S]` block kept the words the previous run
//! left behind, so a second identical horizon on the same engine started mid-run. On
//! `g36/cooling_only_controller` that moved 25 of 210 recorded columns — including a cooling-loop
//! PID command — while two freshly loaded engines agreed exactly. It now re-seeds the state words
//! as well, which makes each call a run **restart**. The tests here pin the reset, the two places
//! it deliberately stops, and the two consequences of restart semantics.
//!
//! Three ways to write a test here that stays green over the live defect, all of which were real:
//!
//! - **A fresh engine per run is blind.** The existing R-SIM-2 pin
//!   (`input_staging_tests::a_constant_run_is_bit_reproducible_across_engines`) builds its engine
//!   inside the closure, so it compares fresh against fresh and passed throughout. Every test below
//!   reuses one engine, because that is the only arrangement in which carryover is observable.
//! - **An undriven model is blind.** Measured on the G36 controller: with no inputs staged, the
//!   same comparisons return 0 differing cells out of 1,260, and with all 282 inputs driven they
//!   return 42. A stateful block that is not being driven has nothing to carry.
//! - **A `y_start` of zero is blind.** `IntegratorWithReset::init_state` seeds
//!   `[y_start.to_bits(), PREV_T_UNSET, 0]`. At `y_start = 0.0` that is `[0, u64::MAX, 0]`, and at
//!   `t_start = 0.0` the `PREV_T_UNSET`-versus-`0` difference cancels through `tick_dt` — so a
//!   re-seed replaced by `words.fill(0)` is indistinguishable. An earlier revision of this file had
//!   exactly that hole: the zero-fill mutant kept all four tests and the whole `oce-api` lib suite
//!   green, and only an out-of-crate G36 golden caught it. The fixture below therefore uses two
//!   `[S]` blocks with *different, non-zero* seeds, and pins the seeded value absolutely.
//!
//! So equality is never the whole assertion. Each equality is paired with a perturbation that must
//! move the same trace, and the seed is pinned by value rather than by agreement.

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
fn a_refused_horizon_does_not_reseed_state_words() {
    // Both fail-fast gates sit above the re-seed, so a spec that never ticks leaves the state arena
    // alone. `prev_t` is a different matter and is pinned separately below — the earlier name for
    // this test claimed the whole engine state was untouched, which is false.
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

#[test]
fn the_two_refusal_gates_differ_on_the_run_clock() {
    // Pre-existing and deliberate, previously pinned nowhere: the collect gate sits ABOVE the
    // `prev_t` reset and the input-name gate BELOW it, so a refused input name clears the run clock
    // and a refused column does not. That asymmetry decides whether a later backwards `tick`
    // returns `Ok` or `TimeRegression`, which is public error surface.
    let refuse = |spec: SimSpec| {
        let mut eng = loaded();
        eng.simulate(&driven_spec(0.0, 10.0)).expect("prime");
        assert!(eng.simulate(&spec).is_err(), "the spec must be refused");
        // backwards relative to the primed horizon end at t = 10
        eng.tick(1.0).is_ok()
    };

    assert!(
        !refuse(SimSpec {
            collect: CollectSpec::Named {
                points: vec!["nope".to_string()],
                stride: 1
            },
            ..driven_spec(0.0, 10.0)
        }),
        "a refused COLLECT leaves prev_t intact, so a backwards tick is still TimeRegression"
    );
    assert!(
        refuse(SimSpec {
            inputs: InputSource::Constant(vec![("nope".to_string(), Value::Real(1.0))]),
            ..driven_spec(0.0, 10.0)
        }),
        "a refused INPUT NAME has already cleared prev_t, so a backwards tick succeeds"
    );
}
