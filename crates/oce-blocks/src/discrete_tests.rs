use oce_model::{ParamTable, Value};

use oce_model::determinism::CANONICAL_NAN_BITS;

use super::{Block, Ctx, NoopDiagnostics, TriggeredMax, TriggeredMovingMean, TriggeredSampler};

fn sample_tick(block: &dyn Block, region: &mut [u64], u: f64, trigger: bool) -> Value {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let inputs = [Value::Real(u), Value::Boolean(trigger)];
    let mut out = None;
    block.emit_from_state(&cx, &inputs, region, &mut |idx, val| {
        assert_eq!(idx, 0);
        out = Some(val);
    });
    block.update_state(&cx, &inputs, region);
    out.expect("single-output stateful block emits one value")
}

fn assert_real_bits(got: &Value, want: f64) {
    assert!(got.bit_eq(&Value::Real(want)), "got {got:?}, want {want:?}");
}

fn emit_once(block: &dyn Block, inputs: &[Value], region: &[u64]) -> Vec<Value> {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let mut out = Vec::new();
    block.emit_from_state(&cx, inputs, region, &mut |idx, val| {
        assert_eq!(idx, out.len(), "outputs must be emitted in port order");
        out.push(val);
    });
    out
}

#[test]
fn rising_edges_sample_current_real_and_held_high_reuses_prior_sample() {
    let sampler = TriggeredSampler { y_start: 2.5 };
    assert_eq!(sampler.state_len(), 2);
    let mut region = vec![0u64; sampler.state_len()];
    sampler.init_state(&mut region, &ParamTable::default());

    let trace = [
        sample_tick(&sampler, &mut region, 1.0, false),
        sample_tick(&sampler, &mut region, 3.0, true),
        sample_tick(&sampler, &mut region, 4.0, true),
        sample_tick(&sampler, &mut region, 5.0, false),
        sample_tick(&sampler, &mut region, 6.0, true),
    ];
    let expected = [2.5, 3.0, 3.0, 3.0, 6.0];
    for (idx, (got, want)) in trace.iter().zip(expected).enumerate() {
        assert!(
            got.bit_eq(&Value::Real(want)),
            "trace[{idx}] got {got:?}, want {want}"
        );
    }
}

#[test]
fn initial_true_trigger_samples_current_real() {
    let sampler = TriggeredSampler { y_start: -7.0 };
    let mut region = vec![0u64; sampler.state_len()];
    sampler.init_state(&mut region, &ParamTable::default());

    assert!(
        sample_tick(&sampler, &mut region, 9.0, true).bit_eq(&Value::Real(9.0)),
        "pre(trigger) is seeded false, so an initially true trigger is a rising edge"
    );
}

#[test]
fn emit_pass_is_pure_for_rising_trigger() {
    let sampler = TriggeredSampler { y_start: 1.0 };
    let mut region = vec![0u64; sampler.state_len()];
    sampler.init_state(&mut region, &ParamTable::default());
    let inputs = [Value::Real(4.0), Value::Boolean(true)];

    let a = emit_once(&sampler, &inputs, &region);
    let b = emit_once(&sampler, &inputs, &region);
    assert!(a[0].bit_eq(&Value::Real(4.0)));
    assert!(a[0].bit_eq(&b[0]));
    assert!(
        f64::from_bits(region[0]).to_bits() == 1.0f64.to_bits(),
        "emit_from_state must not mutate the held sample"
    );
}

#[test]
fn nan_payload_is_canonicalized_when_held() {
    let sampler = TriggeredSampler { y_start: 0.0 };
    let mut region = vec![0u64; sampler.state_len()];
    sampler.init_state(&mut region, &ParamTable::default());
    let noncanonical_nan = f64::from_bits(0xfff8_0000_0000_0001);
    let canonical_nan = f64::from_bits(0x7ff8_0000_0000_0000);

    assert!(
        sample_tick(&sampler, &mut region, noncanonical_nan, true)
            .bit_eq(&Value::Real(canonical_nan))
    );
    assert_eq!(
        region[0],
        canonical_nan.to_bits(),
        "state stores the deterministic NaN representation"
    );
    assert!(
        sample_tick(&sampler, &mut region, 5.0, false).bit_eq(&Value::Real(canonical_nan)),
        "held NaN remains canonical after the trigger falls"
    );
}

#[test]
fn infinite_samples_are_held_bit_exactly() {
    let sampler = TriggeredSampler { y_start: 0.0 };
    let mut region = vec![0u64; sampler.state_len()];
    sampler.init_state(&mut region, &ParamTable::default());

    assert!(
        sample_tick(&sampler, &mut region, f64::INFINITY, true).bit_eq(&Value::Real(f64::INFINITY))
    );
    assert!(
        sample_tick(&sampler, &mut region, f64::NEG_INFINITY, true)
            .bit_eq(&Value::Real(f64::INFINITY)),
        "holding trigger high does not resample current u"
    );
    assert!(
        sample_tick(&sampler, &mut region, f64::NEG_INFINITY, false)
            .bit_eq(&Value::Real(f64::INFINITY))
    );
    assert!(
        sample_tick(&sampler, &mut region, f64::NEG_INFINITY, true)
            .bit_eq(&Value::Real(f64::NEG_INFINITY))
    );
}

#[test]
fn triggered_max_initial_false_emits_raw_current_input_before_any_trigger() {
    let max = TriggeredMax;
    assert_eq!(max.state_len(), 3);
    let mut region = vec![0u64; max.state_len()];
    max.init_state(&mut region, &ParamTable::default());

    let first = sample_tick(&max, &mut region, -2.5, false);
    assert_real_bits(&first, -2.5);

    let held = sample_tick(&max, &mut region, -9.0, false);
    assert_real_bits(&held, -2.5);

    let rising = sample_tick(&max, &mut region, -9.0, true);
    assert_real_bits(&rising, 9.0);
}

#[test]
fn triggered_max_initial_true_applies_event_to_initial_value() {
    let max = TriggeredMax;
    let mut region = vec![0u64; max.state_len()];
    max.init_state(&mut region, &ParamTable::default());

    let first = sample_tick(&max, &mut region, -2.5, true);
    assert_real_bits(&first, 2.5);
    assert!(
        sample_tick(&max, &mut region, 10.0, true).bit_eq(&Value::Real(2.5)),
        "holding trigger high does not resample"
    );
    assert_real_bits(&sample_tick(&max, &mut region, 10.0, false), 2.5);
    assert_real_bits(&sample_tick(&max, &mut region, 10.0, true), 10.0);
}

#[test]
fn triggered_max_emit_pass_is_pure_for_initial_and_event_paths() {
    let max = TriggeredMax;
    let mut region = vec![0u64; max.state_len()];
    max.init_state(&mut region, &ParamTable::default());
    let inputs = [Value::Real(-4.0), Value::Boolean(true)];

    let a = emit_once(&max, &inputs, &region);
    let b = emit_once(&max, &inputs, &region);
    assert_real_bits(&a[0], 4.0);
    assert!(a[0].bit_eq(&b[0]));
    assert_eq!(
        region,
        vec![0, 0, 0],
        "emit_from_state must not mutate state"
    );
}

#[test]
fn triggered_max_non_finite_samples_follow_reals_max_policy() {
    let max = TriggeredMax;
    let mut region = vec![0u64; max.state_len()];
    max.init_state(&mut region, &ParamTable::default());
    let noncanonical_nan = f64::from_bits(0xfff8_0000_0000_0001);
    let canonical_nan = f64::from_bits(CANONICAL_NAN_BITS);

    assert_real_bits(
        &sample_tick(&max, &mut region, noncanonical_nan, false),
        canonical_nan,
    );
    assert_eq!(region[0], CANONICAL_NAN_BITS);

    assert_real_bits(&sample_tick(&max, &mut region, 5.0, true), 5.0);
    assert_real_bits(
        &sample_tick(&max, &mut region, f64::NEG_INFINITY, false),
        5.0,
    );
    assert_real_bits(
        &sample_tick(&max, &mut region, f64::NEG_INFINITY, true),
        f64::INFINITY,
    );
}

#[test]
fn triggered_max_signed_zero_policy_matches_deterministic_max() {
    let max = TriggeredMax;
    let mut region = vec![0u64; max.state_len()];
    max.init_state(&mut region, &ParamTable::default());

    assert_real_bits(&sample_tick(&max, &mut region, -0.0, false), -0.0);
    assert_real_bits(&sample_tick(&max, &mut region, -0.0, true), 0.0);
}

#[test]
fn triggered_moving_mean_initial_sample_is_included_before_trigger_edges() {
    let mean = TriggeredMovingMean { n: 3 };
    assert_eq!(mean.state_len(), 7);
    let mut region = vec![0u64; mean.state_len()];
    mean.init_state(&mut region, &ParamTable::default());

    let trace = [
        sample_tick(&mean, &mut region, 6.0, false),
        sample_tick(&mean, &mut region, 3.0, true),
        sample_tick(&mean, &mut region, 99.0, true),
        sample_tick(&mean, &mut region, 9.0, false),
        sample_tick(&mean, &mut region, 12.0, true),
        sample_tick(&mean, &mut region, 15.0, false),
        sample_tick(&mean, &mut region, 18.0, true),
    ];
    let expected = [6.0, 4.5, 4.5, 4.5, 7.0, 7.0, 11.0];
    for (idx, (got, want)) in trace.iter().zip(expected).enumerate() {
        assert!(
            got.bit_eq(&Value::Real(want)),
            "trace[{idx}] got {got:?}, want {want}"
        );
    }
}

#[test]
fn triggered_moving_mean_initial_true_samples_once_not_twice() {
    let mean = TriggeredMovingMean { n: 3 };
    let mut region = vec![0u64; mean.state_len()];
    mean.init_state(&mut region, &ParamTable::default());

    assert_real_bits(&sample_tick(&mean, &mut region, 9.0, true), 9.0);
    assert_real_bits(&sample_tick(&mean, &mut region, 3.0, true), 9.0);
    assert_real_bits(&sample_tick(&mean, &mut region, 3.0, false), 9.0);
    assert_real_bits(&sample_tick(&mean, &mut region, 6.0, true), 7.5);
}

#[test]
fn triggered_moving_mean_window_one_and_zero_direct_window_degrade_to_current_sample() {
    for mean in [TriggeredMovingMean { n: 1 }, TriggeredMovingMean { n: 0 }] {
        let mut region = vec![0u64; mean.state_len()];
        mean.init_state(&mut region, &ParamTable::default());

        assert_real_bits(&sample_tick(&mean, &mut region, 4.0, false), 4.0);
        assert_real_bits(&sample_tick(&mean, &mut region, 5.0, false), 4.0);
        assert_real_bits(&sample_tick(&mean, &mut region, -2.0, true), -2.0);
        assert_real_bits(&sample_tick(&mean, &mut region, 7.0, true), -2.0);
        assert_real_bits(&sample_tick(&mean, &mut region, 8.0, false), -2.0);
        assert_real_bits(&sample_tick(&mean, &mut region, 8.0, true), 8.0);
    }
}

#[test]
fn triggered_moving_mean_emit_pass_is_pure_for_initial_sample() {
    let mean = TriggeredMovingMean { n: 3 };
    let mut region = vec![0u64; mean.state_len()];
    mean.init_state(&mut region, &ParamTable::default());
    let inputs = [Value::Real(6.0), Value::Boolean(false)];

    let a = emit_once(&mean, &inputs, &region);
    let b = emit_once(&mean, &inputs, &region);
    assert_real_bits(&a[0], 6.0);
    assert!(a[0].bit_eq(&b[0]));
    assert_eq!(
        region,
        vec![
            0.0f64.to_bits(),
            0,
            0,
            0,
            0.0f64.to_bits(),
            0.0f64.to_bits(),
            0.0f64.to_bits()
        ],
        "emit_from_state must not mutate the sample ring or counters"
    );
}

#[test]
fn triggered_moving_mean_non_finite_samples_follow_ordered_sum_policy() {
    let mean = TriggeredMovingMean { n: 2 };
    let mut region = vec![0u64; mean.state_len()];
    mean.init_state(&mut region, &ParamTable::default());
    let noncanonical_nan = f64::from_bits(0xfff8_0000_0000_0001);
    let canonical_nan = f64::from_bits(CANONICAL_NAN_BITS);

    assert_real_bits(
        &sample_tick(&mean, &mut region, noncanonical_nan, false),
        canonical_nan,
    );
    assert_eq!(
        region[4], CANONICAL_NAN_BITS,
        "sample ring stores canonical NaN payloads"
    );

    assert_real_bits(&sample_tick(&mean, &mut region, 5.0, true), canonical_nan);

    let mean = TriggeredMovingMean { n: 2 };
    let mut region = vec![0u64; mean.state_len()];
    mean.init_state(&mut region, &ParamTable::default());
    assert_real_bits(
        &sample_tick(&mean, &mut region, f64::INFINITY, false),
        f64::INFINITY,
    );
    assert_real_bits(
        &sample_tick(&mean, &mut region, f64::NEG_INFINITY, true),
        canonical_nan,
    );
}
