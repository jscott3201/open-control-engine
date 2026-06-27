//! Edge and determinism tests for scalar `CDL.Reals` transcendental blocks.

use std::cell::RefCell;

use oce_model::Value;

use super::{
    Acos, Asin, Atan, Atan2, Block, BlockKind, Cos, Ctx, Diagnostics, Exp, Log, Log10,
    NoopDiagnostics, PortKind, Sin, Tan, lookup,
};

const CANONICAL_NAN_BITS: u64 = 0x7ff8_0000_0000_0000;
const PI_BITS: u64 = 0x4009_21fb_5444_2d18;
const NEG_PI_BITS: u64 = 0xc009_21fb_5444_2d18;
const FRAC_PI_2_BITS: u64 = 0x3ff9_21fb_5444_2d18;
const NEG_FRAC_PI_2_BITS: u64 = 0xbff9_21fb_5444_2d18;
const FRAC_PI_4_BITS: u64 = 0x3fe9_21fb_5444_2d18;
const NEG_FRAC_PI_4_BITS: u64 = 0xbfe9_21fb_5444_2d18;

#[derive(Default)]
struct CapturingDiagnostics {
    events: RefCell<Vec<(String, String, f64)>>,
}

impl Diagnostics for CapturingDiagnostics {
    fn warn(&self, source: &str, message: &str, t: f64) {
        self.events
            .borrow_mut()
            .push((source.to_string(), message.to_string(), t));
    }
}

fn real_out(block: &dyn Block, inputs: &[Value]) -> f64 {
    let diag = NoopDiagnostics;
    real_out_with_diag(block, inputs, &diag)
}

fn real_out_with_diag(block: &dyn Block, inputs: &[Value], diag: &dyn Diagnostics) -> f64 {
    let cx = Ctx::new(0.0, diag);
    let mut out = None;
    block.step_algebraic(&cx, inputs, &mut |idx, val| {
        assert_eq!(idx, 0, "scalar Reals blocks have one output");
        out = Some(val);
    });
    match out.expect("block must emit one output") {
        Value::Real(y) => y,
        other => panic!("expected Real output, got {other:?}"),
    }
}

fn real_inputs(xs: &[f64]) -> Vec<Value> {
    xs.iter().copied().map(Value::Real).collect()
}

fn assert_real_bits(block: &dyn Block, inputs: &[f64], expected_bits: u64) {
    let y = real_out(block, &real_inputs(inputs));
    assert_eq!(y.to_bits(), expected_bits, "actual={y:?}");
}

#[test]
fn registry_exposes_transcendental_block_signatures() {
    let cases: &[(&str, usize)] = &[
        ("CDL.Reals.Acos", 1),
        ("CDL.Reals.Asin", 1),
        ("CDL.Reals.Atan", 1),
        ("CDL.Reals.Atan2", 2),
        ("CDL.Reals.Cos", 1),
        ("CDL.Reals.Exp", 1),
        ("CDL.Reals.Log", 1),
        ("CDL.Reals.Log10", 1),
        ("CDL.Reals.Sin", 1),
        ("CDL.Reals.Tan", 1),
    ];

    for &(class_path, input_count) in cases {
        let entry = lookup(class_path).expect("transcendental class must be registered");
        let block = (entry.make)(&Default::default());
        let signature = block.signature();
        assert_eq!(signature.class_path, class_path);
        assert_eq!(signature.inputs.len(), input_count);
        assert!(signature.inputs.iter().all(|kind| *kind == PortKind::Real));
        assert_eq!(signature.outputs, &[PortKind::Real]);
        assert!(!signature.stateful);
        assert_eq!(block.kind(), BlockKind::Algebraic);
        assert_eq!(block.state_len(), 0);
        for input_idx in 0..input_count {
            assert!(block.feeds_through(input_idx, 0));
        }
    }
}

#[test]
fn basic_trigonometric_anchors_are_bit_pinned() {
    assert_real_bits(&Sin, &[0.0], 0.0f64.to_bits());
    assert_real_bits(&Sin, &[-0.0], (-0.0f64).to_bits());
    assert_real_bits(&Cos, &[0.0], 1.0f64.to_bits());
    assert_real_bits(&Tan, &[0.0], 0.0f64.to_bits());
    assert_real_bits(&Tan, &[-0.0], (-0.0f64).to_bits());

    assert_real_bits(&Asin, &[0.0], 0.0f64.to_bits());
    assert_real_bits(&Asin, &[-0.0], (-0.0f64).to_bits());
    assert_real_bits(&Asin, &[1.0], FRAC_PI_2_BITS);
    assert_real_bits(&Acos, &[1.0], 0.0f64.to_bits());
    assert_real_bits(&Acos, &[-1.0], PI_BITS);
    assert_real_bits(&Atan, &[0.0], 0.0f64.to_bits());
    assert_real_bits(&Atan, &[-0.0], (-0.0f64).to_bits());
    assert_real_bits(&Atan, &[1.0], FRAC_PI_4_BITS);
    assert_real_bits(&Atan, &[f64::INFINITY], FRAC_PI_2_BITS);
    assert_real_bits(&Atan, &[f64::NEG_INFINITY], NEG_FRAC_PI_2_BITS);
}

#[test]
fn atan2_quadrants_and_signed_zero_are_pinned() {
    assert_real_bits(&Atan2, &[0.0, 1.0], 0.0f64.to_bits());
    assert_real_bits(&Atan2, &[-0.0, 1.0], (-0.0f64).to_bits());
    assert_real_bits(&Atan2, &[1.0, 0.0], FRAC_PI_2_BITS);
    assert_real_bits(&Atan2, &[-1.0, 0.0], NEG_FRAC_PI_2_BITS);
    assert_real_bits(&Atan2, &[0.0, -1.0], PI_BITS);
    assert_real_bits(&Atan2, &[-0.0, -1.0], NEG_PI_BITS);

    assert_real_bits(&Atan2, &[1.0, 1.0], FRAC_PI_4_BITS);
    assert_real_bits(&Atan2, &[-1.0, 1.0], NEG_FRAC_PI_4_BITS);

    // The upstream docs forbid simultaneous zero; the warning path is tested separately, and the
    // returned branch values remain deterministic for malformed runtime values.
    assert_real_bits(&Atan2, &[0.0, 0.0], 0.0f64.to_bits());
    assert_real_bits(&Atan2, &[-0.0, 0.0], (-0.0f64).to_bits());
    assert_real_bits(&Atan2, &[0.0, -0.0], PI_BITS);
    assert_real_bits(&Atan2, &[-0.0, -0.0], NEG_PI_BITS);
}

#[test]
fn exp_log_and_log10_edges_are_pinned() {
    assert_real_bits(&Exp, &[0.0], 1.0f64.to_bits());
    assert_real_bits(&Exp, &[f64::NEG_INFINITY], 0.0f64.to_bits());
    assert_real_bits(&Exp, &[f64::INFINITY], f64::INFINITY.to_bits());
    assert_real_bits(&Exp, &[710.0], f64::INFINITY.to_bits());

    assert_real_bits(&Log, &[1.0], 0.0f64.to_bits());
    assert_real_bits(&Log, &[f64::INFINITY], f64::INFINITY.to_bits());
    assert_real_bits(&Log, &[0.0], f64::NEG_INFINITY.to_bits());
    assert_real_bits(&Log, &[-0.0], f64::NEG_INFINITY.to_bits());
    assert_real_bits(&Log, &[-1.0], CANONICAL_NAN_BITS);

    assert_real_bits(&Log10, &[1.0], 0.0f64.to_bits());
    assert_real_bits(&Log10, &[1000.0], 3.0f64.to_bits());
    assert_real_bits(&Log10, &[f64::INFINITY], f64::INFINITY.to_bits());
    assert_real_bits(&Log10, &[0.0], f64::NEG_INFINITY.to_bits());
    assert_real_bits(&Log10, &[-1.0], CANONICAL_NAN_BITS);
}

#[test]
fn documented_domain_violations_warn_and_return_deterministic_values() {
    let diag = CapturingDiagnostics::default();
    assert_eq!(
        real_out_with_diag(&Atan2, &real_inputs(&[0.0, 0.0]), &diag).to_bits(),
        0.0f64.to_bits()
    );
    assert_eq!(
        real_out_with_diag(&Atan2, &real_inputs(&[-0.0, 0.0]), &diag).to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(
        real_out_with_diag(&Atan2, &real_inputs(&[0.0, -0.0]), &diag).to_bits(),
        PI_BITS
    );
    assert_eq!(
        real_out_with_diag(&Atan2, &real_inputs(&[-0.0, -0.0]), &diag).to_bits(),
        NEG_PI_BITS
    );

    assert_eq!(
        real_out_with_diag(&Log, &real_inputs(&[0.0]), &diag).to_bits(),
        f64::NEG_INFINITY.to_bits()
    );
    assert_eq!(
        real_out_with_diag(&Log, &real_inputs(&[-1.0]), &diag).to_bits(),
        CANONICAL_NAN_BITS
    );
    assert_eq!(
        real_out_with_diag(&Log10, &real_inputs(&[-0.0]), &diag).to_bits(),
        f64::NEG_INFINITY.to_bits()
    );
    assert_eq!(
        real_out_with_diag(&Log10, &real_inputs(&[f64::NAN]), &diag).to_bits(),
        CANONICAL_NAN_BITS
    );

    let events = diag.events.borrow();
    assert_eq!(events.len(), 8);
    assert!(events[0].0 == "CDL.Reals.Atan2");
    assert!(events[0].1.contains("shall not both be zero"));
    assert!(events[4].0 == "CDL.Reals.Log");
    assert!(events[4].1.contains("must be greater than zero"));
    assert!(events[6].0 == "CDL.Reals.Log10");
    assert!(events[6].1.contains("must be greater than zero"));
    assert!(
        events
            .iter()
            .all(|(_, _, t)| t.to_bits() == 0.0f64.to_bits())
    );
}

#[test]
fn valid_log_and_atan2_inputs_do_not_warn() {
    let diag = CapturingDiagnostics::default();
    let _ = real_out_with_diag(&Atan2, &real_inputs(&[1.0, 1.0]), &diag);
    let _ = real_out_with_diag(&Log, &real_inputs(&[1.0]), &diag);
    let _ = real_out_with_diag(&Log10, &real_inputs(&[10.0]), &diag);
    assert!(diag.events.borrow().is_empty());
}

#[test]
fn domain_and_non_finite_nan_outputs_are_canonicalized() {
    assert_real_bits(&Sin, &[f64::INFINITY], CANONICAL_NAN_BITS);
    assert_real_bits(&Sin, &[f64::NEG_INFINITY], CANONICAL_NAN_BITS);
    assert_real_bits(&Sin, &[f64::NAN], CANONICAL_NAN_BITS);
    assert_real_bits(&Cos, &[f64::INFINITY], CANONICAL_NAN_BITS);
    assert_real_bits(&Cos, &[f64::NEG_INFINITY], CANONICAL_NAN_BITS);
    assert_real_bits(&Cos, &[f64::NAN], CANONICAL_NAN_BITS);
    assert_real_bits(&Tan, &[f64::INFINITY], CANONICAL_NAN_BITS);
    assert_real_bits(&Tan, &[f64::NEG_INFINITY], CANONICAL_NAN_BITS);
    assert_real_bits(&Tan, &[f64::NAN], CANONICAL_NAN_BITS);
    assert_real_bits(&Asin, &[1.0 + f64::EPSILON], CANONICAL_NAN_BITS);
    assert_real_bits(&Asin, &[-1.0 - f64::EPSILON], CANONICAL_NAN_BITS);
    assert_real_bits(&Asin, &[f64::INFINITY], CANONICAL_NAN_BITS);
    assert_real_bits(&Asin, &[f64::NEG_INFINITY], CANONICAL_NAN_BITS);
    assert_real_bits(&Asin, &[f64::NAN], CANONICAL_NAN_BITS);
    assert_real_bits(&Acos, &[-1.0 - f64::EPSILON], CANONICAL_NAN_BITS);
    assert_real_bits(&Acos, &[1.0 + f64::EPSILON], CANONICAL_NAN_BITS);
    assert_real_bits(&Acos, &[f64::INFINITY], CANONICAL_NAN_BITS);
    assert_real_bits(&Acos, &[f64::NEG_INFINITY], CANONICAL_NAN_BITS);
    assert_real_bits(&Acos, &[f64::NAN], CANONICAL_NAN_BITS);
    assert_real_bits(&Atan, &[f64::NAN], CANONICAL_NAN_BITS);
    assert_real_bits(&Atan2, &[f64::NAN, 1.0], CANONICAL_NAN_BITS);
    assert_real_bits(&Atan2, &[1.0, f64::NAN], CANONICAL_NAN_BITS);
    assert_real_bits(&Atan2, &[f64::INFINITY, 1.0], FRAC_PI_2_BITS);
    assert_real_bits(&Atan2, &[f64::NEG_INFINITY, 1.0], NEG_FRAC_PI_2_BITS);
    assert_real_bits(&Atan2, &[1.0, f64::INFINITY], 0.0f64.to_bits());
    assert_real_bits(&Atan2, &[1.0, f64::NEG_INFINITY], PI_BITS);
    assert_real_bits(&Atan2, &[f64::INFINITY, f64::INFINITY], FRAC_PI_4_BITS);
    assert_real_bits(
        &Atan2,
        &[f64::NEG_INFINITY, f64::INFINITY],
        NEG_FRAC_PI_4_BITS,
    );
    assert_real_bits(&Exp, &[f64::INFINITY], f64::INFINITY.to_bits());
    assert_real_bits(&Exp, &[f64::NEG_INFINITY], 0.0f64.to_bits());
    assert_real_bits(&Exp, &[f64::NAN], CANONICAL_NAN_BITS);
    assert_real_bits(&Log, &[f64::NEG_INFINITY], CANONICAL_NAN_BITS);
    assert_real_bits(&Log, &[f64::NAN], CANONICAL_NAN_BITS);
    assert_real_bits(&Log10, &[f64::NEG_INFINITY], CANONICAL_NAN_BITS);
    assert_real_bits(&Log10, &[f64::NAN], CANONICAL_NAN_BITS);
}

#[test]
fn repeated_runs_are_bit_identical_for_transcendental_samples() {
    let cases: &[(&dyn Block, &[f64])] = &[
        (&Sin, &[1.25]),
        (&Cos, &[1.25]),
        (&Tan, &[0.75]),
        (&Asin, &[0.25]),
        (&Acos, &[0.25]),
        (&Atan, &[-2.5]),
        (&Atan2, &[-3.0, 4.0]),
        (&Exp, &[3.125]),
        (&Log, &[3.125]),
        (&Log10, &[3.125]),
    ];

    for &(block, inputs) in cases {
        let first = real_out(block, &real_inputs(inputs)).to_bits();
        for _ in 0..100 {
            assert_eq!(real_out(block, &real_inputs(inputs)).to_bits(), first);
        }
    }
}
