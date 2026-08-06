//! What `simulate` resets between horizons, and what it deliberately does not.
//!
//! `simulate` used to clear only the run clock. Every `[S]` block kept the words the previous run
//! left behind, so a second identical horizon on the same engine started mid-run. On
//! `g36/cooling_only_controller` that moved 25 of 210 recorded columns — including a cooling-loop
//! PID command — while two freshly loaded engines agreed exactly. It now re-seeds the state words
//! as well, and the tests here pin both halves of that: the reset happens, and it stops where it
//! was meant to stop.
//!
//! Two traps these tests are shaped around, both of which produce a green suite over a live defect:
//!
//! - **A fresh engine per run is blind to it.** The existing R-SIM-2 pin
//!   (`input_staging_tests::a_constant_run_is_bit_reproducible_across_engines`) builds its engine
//!   inside the closure, so it compares fresh against fresh and passes either way. Every test below
//!   reuses one engine, because that is the only arrangement in which the carryover is observable.
//! - **An undriven model is blind to it.** Measured on the G36 controller: with no inputs staged,
//!   the same three comparisons return 0 differing cells out of 1,260, and with all 282 inputs
//!   driven they return 42. A model whose stateful block is not being driven has nothing to carry.
//!
//! So `equality` alone is not evidence here. Each equality assertion is paired with a perturbation
//! that must move the same trace, which is what makes the equality mean the reset ran rather than
//! that the trace was insensitive all along.

use super::common::*;

/// A single `CDL.Reals.IntegratorWithReset` with all three inputs host-staged: `conn#0` = `u`,
/// `conn#1` = `y_reset_in`, `conn#2` = `trigger`, `conn#3` = `y`. `k = 1`, `y_start = 0`, so with
/// `trigger` held false the output is the running integral of `u` and every tick writes state.
fn integrator_model() -> ModelGraph {
    let mut mb = Mb::new();
    let (_, inputs, _) = mb.block(
        "CDL.Reals.IntegratorWithReset",
        &[ValueType::Real, ValueType::Real, ValueType::Boolean],
        &[ValueType::Real],
        vec![rp("k", 1.0), rp("y_start", 0.0)],
    );
    let mut model = mb.finish();
    model.external_inputs = inputs;
    model
}

fn loaded() -> Engine<MemStore> {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(integrator_model(), None)
        .expect("BUILD");
    eng
}

/// Drive every input on every step, so the integrator actually accumulates. `u = 2.0` makes the
/// carried and fresh trajectories separate immediately rather than after rounding.
fn driven_spec(t_stop: f64) -> SimSpec {
    SimSpec {
        t_start: 0.0,
        t_stop,
        step: 1.0,
        inputs: InputSource::Constant(vec![
            ("conn#0".to_string(), Value::Real(2.0)),
            ("conn#1".to_string(), Value::Real(0.0)),
            ("conn#2".to_string(), Value::Boolean(false)),
        ]),
        collect: CollectSpec::Named {
            points: vec!["conn#3".to_string()],
            stride: 1,
        },
    }
}

/// The recorded `y` column as raw bits — the comparison the determinism claim is made in.
fn y_bits(metrics: &crate::SimMetrics) -> Vec<u64> {
    metrics
        .trace
        .column(0)
        .expect("one recorded column")
        .iter()
        .map(|v| match v {
            Value::Real(r) => r.to_bits(),
            other => panic!("conn#3 is Real, got {other:?}"),
        })
        .collect()
}

#[test]
fn a_reused_engine_runs_the_horizon_from_the_same_start_as_a_fresh_one() {
    let mut reused = loaded();
    let first = y_bits(&reused.simulate(&driven_spec(10.0)).expect("first horizon"));
    let second = y_bits(&reused.simulate(&driven_spec(10.0)).expect("second horizon"));

    let mut fresh = loaded();
    let independent = y_bits(&fresh.simulate(&driven_spec(10.0)).expect("fresh horizon"));

    assert_eq!(
        first, second,
        "a second identical horizon on one engine must reproduce the first (R-SIM-2)"
    );
    assert_eq!(
        first, independent,
        "and must equal what a freshly loaded engine produces from the same spec"
    );

    // The equality above is only evidence if this trace can move at all. A rising integral is what
    // makes a carried start visible; a flat column would satisfy every assertion above while the
    // reset did nothing.
    assert!(
        first.windows(2).any(|w| w[0] != w[1]),
        "the probe column must vary over the horizon, or the equalities are vacuous"
    );
}

#[test]
fn a_horizon_starts_from_seeded_words_however_the_engine_was_left() {
    // Corrupt the state arena directly, then run. If the re-seed happens, the corruption cannot
    // reach the trace. The paired assertion below proves the corruption was real, so this is not
    // an equality that holds because nothing was perturbed.
    let mut perturbed = loaded();
    perturbed.simulate(&driven_spec(10.0)).expect("prime");
    for w in perturbed.state.words.iter_mut() {
        *w = 0x4059_0000_0000_0000; // 100.0
    }
    let after = y_bits(
        &perturbed
            .simulate(&driven_spec(10.0))
            .expect("post-corruption"),
    );

    let mut fresh = loaded();
    let reference = y_bits(&fresh.simulate(&driven_spec(10.0)).expect("reference"));
    assert_eq!(
        after, reference,
        "simulate re-seeds the words, so a corrupted arena cannot reach the horizon"
    );

    // The control: the same corruption applied to the tick path — which does not re-seed — must
    // change the output. Without this, the equality above is satisfied by a perturbation that was
    // never observable in the first place.
    let mut ticked = loaded();
    ticked.set_input("conn#0", Value::Real(2.0)).expect("u");
    ticked.set_input("conn#1", Value::Real(0.0)).expect("reset");
    ticked
        .set_input("conn#2", Value::Boolean(false))
        .expect("trigger");
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
        eng.set_input("conn#0", Value::Real(u)).expect("u");
        eng.set_input("conn#1", Value::Real(0.0)).expect("reset");
        eng.set_input("conn#2", Value::Boolean(false))
            .expect("trigger");
        let spec = SimSpec {
            t_start: 0.0,
            t_stop: 10.0,
            step: 1.0,
            inputs: InputSource::None,
            collect: CollectSpec::Named {
                points: vec!["conn#3".to_string()],
                stride: 1,
            },
        };
        y_bits(&eng.simulate(&spec).expect("horizon"))
    };
    assert_ne!(
        staged(2.0),
        staged(-7.5),
        "a value staged before simulate must still drive the horizon"
    );
}

#[test]
fn a_refused_horizon_leaves_the_previous_run_state_intact() {
    // Both fail-fast gates — the collect resolution and the Constant input resolution — sit above
    // the re-seed, so a spec that never runs a tick must not destroy what the caller had. This is
    // the placement, not an incidental ordering.
    let mut eng = loaded();
    eng.simulate(&driven_spec(10.0)).expect("a real horizon");
    let before = eng.state.words.clone();
    assert!(
        before.iter().any(|&w| w != 0),
        "the primed run must leave non-zero state, or this test cannot discriminate"
    );

    let unknown_column = SimSpec {
        collect: CollectSpec::Named {
            points: vec!["nope".to_string()],
            stride: 1,
        },
        ..driven_spec(10.0)
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
        ..driven_spec(10.0)
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
