//! Frame times are bounded in magnitude so that no elapsed-time difference can overflow, for
//! every model whatever its blocks.
//!
//! Before the facade checked it, a model without sampled blocks accepted t = -1e308 then +1e308:
//! the integrator saw an infinite `dt` and latched NaN. Per-step finiteness alone would not have
//! been enough: -1e308 -> 0 -> 1e308 has finite steps, yet a Timer started at -1e308 reports an
//! infinite elapsed time. A sampled block happened to refuse such times through its own
//! representability guard, so acceptance depended on model contents. The facade now refuses
//! `|t| > MAX_FRAME_TIME_MAGNITUDE` (`f64::MAX / 4`). Any single difference of two accepted times is
//! then at most `f64::MAX / 2`; a running sum of rounded steps can drift above the exact span, and
//! the remaining factor of two absorbs that drift. Every expected value below is hand-derived
//! binary64 arithmetic.

use oce_api::{CompletedFrame, Engine, MAX_FRAME_TIME_MAGNITUDE, OcError, Value};

const MODEL: &[u8] = include_bytes!("fixtures/finite_frame.jsonld");
const SAMPLED: &[u8] = include_bytes!("fixtures/frame_delay.jsonld");

/// The smallest finite magnitude above the limit: one ulp (2^969) past it.
fn just_above() -> f64 {
    f64::from_bits(MAX_FRAME_TIME_MAGNITUDE.to_bits() + 1)
}

fn loaded(bytes: &[u8]) -> Engine {
    let mut engine = Engine::in_memory();
    engine.load_cxf(bytes).expect("fixture loads");
    engine
}

fn frame(trigger: bool) -> Vec<(&'static str, Value)> {
    vec![
        ("urn:fin:u", Value::Real(1.0)),
        ("urn:fin:reset", Value::Real(0.0)),
        ("urn:fin:trigger", Value::Boolean(trigger)),
        ("urn:fin:n", Value::Integer(0)),
    ]
}

fn sampled_frame() -> Vec<(&'static str, Value)> {
    vec![
        ("urn:frame:a", Value::Real(0.5)),
        ("urn:frame:b", Value::Real(2.0)),
    ]
}

fn run(engine: &mut Engine, time: f64, trigger: bool) -> CompletedFrame {
    let plan = engine
        .prepare_frame(time, &frame(trigger))
        .expect("eligible time");
    engine.execute_frame(plan).expect("eligible time executes")
}

/// Outputs in canonical order: elapsed (Timer), sum (Add), y (Integrator).
fn real(frame: &CompletedFrame, index: usize) -> f64 {
    frame.outputs()[index].1.as_real().unwrap()
}

fn out_of_range(error: &OcError, now: f64) -> bool {
    matches!(error, OcError::TimeOutOfRange { now: n, limit }
        if n.to_bits() == now.to_bits() && limit.to_bits() == MAX_FRAME_TIME_MAGNITUDE.to_bits())
}

#[test]
fn the_limit_is_exactly_a_quarter_of_max_and_spans_exactly_half() {
    assert_eq!(MAX_FRAME_TIME_MAGNITUDE.to_bits(), 0x7fcf_ffff_ffff_ffff);
    assert_eq!(MAX_FRAME_TIME_MAGNITUDE * 4.0, f64::MAX);
    // The widest single difference is exactly MAX / 2, leaving a factor of two of headroom.
    assert_eq!(
        (MAX_FRAME_TIME_MAGNITUDE - -MAX_FRAME_TIME_MAGNITUDE).to_bits(),
        (f64::MAX / 2.0).to_bits()
    );
}

#[test]
fn an_upward_drifting_accumulation_over_the_widest_span_stays_finite() {
    // The reviewer's schedule against the former f64::MAX / 2 limit, halved exactly (every value's
    // exponent drops by one): -L, then three interior times, then L three times. All are in
    // range and nondecreasing. The integrator (u = 1, k = 1, trigger false) accumulates the
    // rounded steps: hand-computed in binary64, x = 0 + 0 (first tick), then the four step sums
    // round up to 2^1023 exactly (0x7fe0_0000_0000_0000), one ulp above the exact span
    // 2L = 0x7fdf_ffff_ffff_ffff. At the former limit the same schedule doubled to `inf`; here
    // the headroom keeps it finite. The value is emitted one frame after the last update.
    let schedule = [
        0xffcf_ffff_ffff_ffff_u64,
        0xff9b_d156_12e7_bdd0,
        0x7c50_0000_0000_0000,
        0x7d80_bd24_0000_0000,
        0x7fcf_ffff_ffff_ffff,
        0x7fcf_ffff_ffff_ffff,
        0x7fcf_ffff_ffff_ffff,
    ];
    let mut engine = loaded(MODEL);
    let mut last = None;
    for bits in schedule {
        let time = f64::from_bits(bits);
        assert!(time.abs() <= MAX_FRAME_TIME_MAGNITUDE);
        last = Some(run(&mut engine, time, false));
    }
    let y = real(&last.unwrap(), 2);
    assert_eq!(
        y.to_bits(),
        0x7fe0_0000_0000_0000,
        "drifted one ulp above 2L, finite"
    );
    assert!(
        y > MAX_FRAME_TIME_MAGNITUDE * 2.0,
        "the drift is real, not hypothetical"
    );
    // Doubling the schedule (the former limit) is now refused before anything accumulates.
    let engine = loaded(MODEL);
    let doubled = f64::from_bits(schedule[0]) * 2.0;
    assert!(out_of_range(
        &engine.prepare_frame(doubled, &frame(false)).unwrap_err(),
        doubled
    ));
}

#[test]
fn times_beyond_the_limit_are_refused_for_every_model_and_nothing_moves() {
    for bytes in [MODEL, SAMPLED] {
        for time in [
            just_above(),
            -just_above(),
            1.0e308,
            -1.0e308,
            f64::MAX,
            f64::MIN,
        ] {
            let engine = loaded(bytes);
            let entries = if bytes == MODEL {
                frame(false)
            } else {
                sampled_frame()
            };
            let before = engine.state_snapshot().unwrap();
            let error = engine.prepare_frame(time, &entries).unwrap_err();
            assert!(out_of_range(&error, time), "{time:e}: {error:?}");
            assert_eq!(
                error.to_string(),
                format!(
                    "frame time out of range: |t_now={time:e}| exceeds {MAX_FRAME_TIME_MAGNITUDE:e}"
                )
            );
            assert_eq!(
                engine.state_snapshot().unwrap().as_bytes(),
                before.as_bytes()
            );
            // Model independence: before the limit the sampled model already refused these as
            // unrepresentable; now both models give the same, earlier, model-independent cause.
        }
    }
}

#[test]
fn the_magnitude_check_sits_between_non_finite_time_and_regression() {
    let mut engine = loaded(MODEL);
    run(&mut engine, 0.0, false);
    assert!(matches!(
        engine.prepare_frame(f64::NAN, &frame(false)),
        Err(OcError::NonFiniteTime { .. })
    ));
    // -1e308 both regresses and is out of range: the range refusal is reported.
    assert!(out_of_range(
        &engine.prepare_frame(-1.0e308, &frame(false)).unwrap_err(),
        -1.0e308
    ));
    // In range but regressing.
    assert!(matches!(
        engine.prepare_frame(-1.0, &frame(false)),
        Err(OcError::TimeRegression { .. })
    ));
    // Input-key refusals come after every time refusal.
    assert!(out_of_range(
        &engine
            .prepare_frame(f64::MAX, &[("unknown", Value::Real(0.0))])
            .unwrap_err(),
        f64::MAX
    ));
}

#[test]
fn the_widest_accepted_span_is_exactly_half_of_max_for_a_single_difference() {
    // Timer and integrator both started at -L, then L reached in two exact steps of L each.
    // The integrator (u = 1) emits its prior accumulator: 0 (reset at the first rising trigger),
    // then L after the -L -> 0 step; after the final step it holds L + L = MAX / 2. The Timer's
    // single difference reports L - (-L) = MAX / 2. Without a limit this span reached `inf`.
    let limit = MAX_FRAME_TIME_MAGNITUDE;
    let mut engine = loaded(MODEL);
    let first = run(&mut engine, -limit, true);
    assert_eq!(real(&first, 0).to_bits(), 0.0_f64.to_bits());
    let middle = run(&mut engine, 0.0, true);
    assert_eq!(real(&middle, 0).to_bits(), limit.to_bits());
    let last = run(&mut engine, limit, true);
    assert_eq!(
        real(&last, 0).to_bits(),
        (f64::MAX / 2.0).to_bits(),
        "timer span"
    );
    assert_eq!(
        real(&last, 2).to_bits(),
        limit.to_bits(),
        "integrator emits prior x"
    );
    let after = run(&mut engine, limit, true);
    assert_eq!(
        real(&after, 2).to_bits(),
        (f64::MAX / 2.0).to_bits(),
        "x = L + 1 * L"
    );
    // And straight from -L to L in one step: dt = MAX / 2.
    let mut engine = loaded(MODEL);
    run(&mut engine, -limit, false);
    assert_eq!(run(&mut engine, limit, false).sequence(), 2);
}

#[test]
fn equal_and_large_finite_steps_are_accepted() {
    for time in [-MAX_FRAME_TIME_MAGNITUDE, MAX_FRAME_TIME_MAGNITUDE, -0.0] {
        let mut engine = loaded(MODEL);
        run(&mut engine, time, false);
        assert_eq!(run(&mut engine, time, false).sequence(), 2);
    }
    // A host clock jump from -1e9 to 1e18 is large but finite and is accepted: the integrator
    // (u = 1, k = 1) takes one forward-Euler step of exactly 1e18 + 1e9.
    let mut engine = loaded(MODEL);
    assert_eq!(
        real(&run(&mut engine, -1.0e9, false), 2).to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(
        real(&run(&mut engine, 1.0e18, false), 2).to_bits(),
        0.0_f64.to_bits()
    );
    let after = run(&mut engine, 1.0e18, false);
    assert_eq!(real(&after, 2).to_bits(), 1.000_000_001e18_f64.to_bits());
    // UNIX epoch nanoseconds (2026 is ~1.8e18) are far inside the limit.
    let mut engine = loaded(MODEL);
    run(&mut engine, 1.8e18, false);
}

#[test]
fn sampled_blocks_keep_their_own_stricter_representability() {
    let mut engine = loaded(SAMPLED);
    let plan = engine.prepare_frame(-1.0e9, &sampled_frame()).unwrap();
    engine.execute_frame(plan).unwrap();
    let plan = engine.prepare_frame(1.0e18, &sampled_frame()).unwrap();
    engine.execute_frame(plan).unwrap();
    // In range for the facade, still beyond what a 1 s sampled clock can index.
    assert!(matches!(
        engine.prepare_frame(1.0e300, &sampled_frame()),
        Err(OcError::ModelTimeUnrepresentable { now }) if now.to_bits() == 1.0e300_f64.to_bits()
    ));
}
