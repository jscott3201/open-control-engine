use oce_model::{ParamTable, Value};

use super::{Block, Ctx, NoopDiagnostics, TriggeredSampler};

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
