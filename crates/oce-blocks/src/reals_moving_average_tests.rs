//! Tests for `CDL.Reals.MovingAverage`: the pinned short traces, the window-length guarantee with
//! far more ticks per window than the former fixed ring held, the coalescing bound for ticks
//! denser than the history resolution, ring sizing and the resolved state validator.
//!
//! Two references are used. `held_backward_mean` is an independent exact oracle: it integrates
//! the documented reconstruction (each tick's `u` held back over its gap) piece by piece and
//! never interpolates `mu`. `unbounded_history` replays the engine's documented recurrence with
//! an unbounded `Vec` history, so whenever no tick coalesces the engine must match it bit for bit.

use std::cell::RefCell;

use oce_model::{ParamTable, Value};

use super::{Block, Ctx, Diagnostics, MovingAverage, NoopDiagnostics};

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

const COALESCED_WARNING: &str = "MovingAverage: ticks arrived closer than the window history resolution; \
     checkpoints coalesced (window still spans delta)";
const HEAD_WORD: usize = 3;
const LEN_WORD: usize = 4;
const COALESCED_WORD: usize = 5;
const META_WORDS: usize = 6;

fn init_region(block: &dyn Block) -> Vec<u64> {
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    region
}

fn real(value: &Value) -> f64 {
    match value {
        Value::Real(x) => *x,
        other => panic!("MovingAverage must emit a Real, got {other:?}"),
    }
}

fn emit_real(block: &dyn Block, region: &[u64], t: f64, u: f64) -> Value {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let mut out = None;
    block.emit_from_state(&cx, &[Value::Real(u)], region, &mut |idx, val| {
        assert_eq!(idx, 0);
        out = Some(val);
    });
    out.expect("MovingAverage must emit one output")
}

fn tick_with_diag(
    block: &dyn Block,
    region: &mut [u64],
    t: f64,
    u: f64,
    diag: &dyn Diagnostics,
) -> Value {
    let cx = Ctx::new(t, diag);
    let mut out = None;
    block.emit_from_state(&cx, &[Value::Real(u)], region, &mut |idx, val| {
        assert_eq!(idx, 0);
        out = Some(val);
    });
    block.update_state(&cx, &[Value::Real(u)], region);
    out.expect("MovingAverage must emit one output")
}

fn tick(block: &dyn Block, region: &mut [u64], t: f64, u: f64) -> Value {
    tick_with_diag(block, region, t, u, &NoopDiagnostics)
}

fn drive(block: &dyn Block, steps: &[(f64, f64)]) -> (Vec<Value>, Vec<u64>) {
    let mut region = init_region(block);
    let trace = steps
        .iter()
        .map(|&(t, u)| tick(block, &mut region, t, u))
        .collect();
    (trace, region)
}

fn assert_trace_bits(got: &[Value], want: &[u64]) {
    assert_eq!(got.len(), want.len());
    for (idx, (got, want)) in got.iter().zip(want).enumerate() {
        let want = Value::Real(f64::from_bits(*want));
        assert!(got.bit_eq(&want), "trace[{idx}] got {got:?}, want {want:?}");
    }
}

/// Independent exact oracle: the mean over `[t_k - delta, t_k]` of the input held backward over
/// each tick gap (`u_j` on `(t_{j-1}, t_j]`), or the upstream warm-up quotient before the first
/// full window. Integrates piece by piece; never interpolates an accumulated integral.
fn held_backward_mean(delta: f64, steps: &[(f64, f64)], k: usize) -> f64 {
    let t0 = steps[0].0;
    let t = steps[k].0;
    if t < t0 + delta {
        let integral = (1..=k)
            .map(|j| steps[j].1 * (steps[j].0 - steps[j - 1].0))
            .fold(0.0, |acc, x| acc + x);
        return integral / (t - t0 + 1e-3);
    }
    let lo = t - delta;
    let integral = (1..=k)
        .map(|j| {
            let start = steps[j - 1].0.max(lo);
            let end = steps[j].0;
            if end > start {
                steps[j].1 * (end - start)
            } else {
                0.0
            }
        })
        .fold(0.0, |acc, x| acc + x);
    integral / delta
}

/// The documented recurrence over an unbounded checkpoint history.
fn unbounded_history(delta: f64, steps: &[(f64, f64)]) -> Vec<f64> {
    let mut history: Vec<(f64, f64)> = Vec::new();
    let mut mu = 0.0f64;
    let mut out = Vec::with_capacity(steps.len());
    for (k, &(t, u)) in steps.iter().enumerate() {
        let dt = if k == 0 { 0.0 } else { t - steps[k - 1].0 };
        let mu_now = mu + u * dt;
        let target = t - delta;
        let first = history.first().copied().unwrap_or((t, mu_now));
        let mu_del = if target <= first.0 {
            first.1
        } else {
            // First checkpoint at or after the target, closed by the current point.
            let idx = history.partition_point(|p| p.0 < target);
            let next = history.get(idx).copied().unwrap_or((t, mu_now));
            let prev = history[idx - 1];
            let den = next.0 - prev.0;
            if den == 0.0 {
                next.1
            } else {
                prev.1 + (next.1 - prev.1) * ((target - prev.0) / den)
            }
        };
        let t0 = steps[0].0;
        let denom = if t >= t0 + delta {
            delta
        } else {
            t - t0 + 1e-3
        };
        out.push((mu_now - mu_del) / denom);
        history.push((t, mu_now));
        mu = mu_now;
    }
    out
}

/// Deterministic xorshift64 stream mapped to `[0, 1)`.
struct Stream(u64);

impl Stream {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn irregular_steps(seed: u64, n: usize, dt_lo: f64, dt_hi: f64) -> Vec<(f64, f64)> {
    let mut stream = Stream(seed);
    let mut t = 0.0;
    (0..n)
        .map(|k| {
            if k > 0 {
                t += dt_lo + (dt_hi - dt_lo) * stream.next();
            }
            (t, -50.0 + 100.0 * stream.next())
        })
        .collect()
}

#[test]
fn ring_is_sized_from_delta_and_bounded() {
    let words = |delta: f64| MovingAverage { delta }.state_len();
    // clamp(2*ceil(delta), 2048, 131072) slots plus 4 slack, two words each, plus 6 meta words.
    assert_eq!(MovingAverage::default().state_len(), 4110);
    assert_eq!(words(1e-5), 6 + 2 * 2052);
    assert_eq!(words(300.0), 6 + 2 * 2052);
    assert_eq!(words(1024.0), 6 + 2 * 2052);
    assert_eq!(words(1024.5), 6 + 2 * 2054);
    assert_eq!(words(3600.0), 14_414);
    assert_eq!(words(65_536.0), 6 + 2 * 131_076);
    assert_eq!(words(65_535.5), 6 + 2 * 131_076);
    assert_eq!(words(65_534.5), 6 + 2 * 131_074);
    assert_eq!(words(131_072.0), 262_158);
    // Unvalidated direct constructions stay bounded instead of panicking or overflowing.
    for hostile in [1e9, f64::INFINITY, f64::MAX] {
        assert_eq!(words(hostile), 6 + 2 * 131_076, "delta={hostile}");
    }
    for floored in [f64::NAN, 0.0, -5.0, f64::NEG_INFINITY] {
        assert_eq!(words(floored), 6 + 2 * 2052, "delta={floored}");
    }

    let resolution = |delta: f64| MovingAverage { delta }.geometry().resolution;
    assert_eq!(resolution(300.0).to_bits(), (300.0f64 / 2048.0).to_bits());
    for delta in [1024.0, 3600.0, 65_536.0] {
        assert_eq!(
            resolution(delta).to_bits(),
            0.5f64.to_bits(),
            "delta={delta}"
        );
    }
    assert_eq!(resolution(131_072.0).to_bits(), 1.0f64.to_bits());
}

#[test]
fn startup_steady_and_variable_dt_window_are_pinned() {
    let block = MovingAverage { delta: 1.0 };
    let steps = [(0.0, 2.0), (0.5, 2.0), (1.0, 4.0), (1.5, 4.0), (2.25, 1.0)];
    let (trace, region) = drive(&block, &steps);
    assert_trace_bits(
        &trace,
        &[
            0x0000_0000_0000_0000,
            0x3fff_efa6_115f_8d8a,
            0x4008_0000_0000_0000,
            0x4010_0000_0000_0000,
            0x3ffc_0000_0000_0000,
        ],
    );
    assert_eq!(region[0], 5.75f64.to_bits());
}

#[test]
fn non_dyadic_residue_feedthrough_and_determinism_are_pinned() {
    let block = MovingAverage { delta: 0.3 };
    let steps = [
        (0.0, 0.2),
        (0.1, 0.2),
        (0.2, 0.2),
        (0.3, 0.2),
        (0.4, 0.2),
        (0.5, 0.2),
    ];
    let (trace, region) = drive(&block, &steps);
    assert_trace_bits(
        &trace,
        &[
            0x0000_0000_0000_0000,
            0x3fc9_58b6_7ebb_907b,
            0x3fc9_78fe_b9f3_4382,
            0x3fc9_9999_9999_999a,
            0x3fc9_9999_9999_999a,
            0x3fc9_9999_9999_999a,
        ],
    );
    assert_eq!(region[0], 0x3fb9_9999_9999_999a);

    let mut one_tick = init_region(&block);
    tick(&block, &mut one_tick, 0.0, 1.0);
    let low = emit_real(&block, &one_tick, 0.1, 1.0);
    let high = emit_real(&block, &one_tick, 0.1, 3.0);
    assert!(
        !low.bit_eq(&high),
        "MovingAverage output must depend on current u in the same tick"
    );
}

#[test]
fn sub_millisecond_delta_uses_true_full_window_denominator() {
    let block = MovingAverage { delta: 5.0e-4 };
    let dt = 1.0e-4;
    let steps = [
        (0.0, 0.0),
        (dt, 1.0),
        (2.0 * dt, 2.0),
        (3.0 * dt, 3.0),
        (4.0 * dt, 4.0),
        (5.0 * dt, 5.0),
    ];
    let (trace, _region) = drive(&block, &steps);
    // Full-window tick: numerator=(1+2+3+4+5)*0.0001=0.0015, denom=0.0005,
    // so the legal sub-1e-3 horizon reports 3.0. A literal 1e-3 floor reports 1.5.
    assert_trace_bits(
        &trace,
        &[
            0x0000_0000_0000_0000,
            0x3fb7_45d1_745d_1746,
            0x3fd0_0000_0000_0000,
            0x3fdd_89d8_9d89_d89f,
            0x3fe6_db6d_b6db_6db7,
            0x4008_0000_0000_0000,
        ],
    );
}

#[test]
fn step_after_six_hundred_one_second_ticks_averages_over_the_full_window() {
    // The reported reproduction: delta=600 s, 1 s ticks, u steps 0 -> 100 at t=640. The held-
    // backward reconstruction carries u=100 over (639, 640], so at t=700 the window integral is
    // 100*(700-639) = 6100 and y = 6100/600 = 10.1666... The former 64-checkpoint ring divided
    // the same integral by its retained 64 s span and reported 95.3125.
    let block = MovingAverage { delta: 600.0 };
    let diag = CapturingDiagnostics::default();
    let mut region = init_region(&block);
    let steps: Vec<(f64, f64)> = (0..=700)
        .map(|k| (f64::from(k), if k >= 640 { 100.0 } else { 0.0 }))
        .collect();
    let mut trace = Vec::new();
    for &(t, u) in &steps {
        trace.push(real(&tick_with_diag(&block, &mut region, t, u, &diag)));
    }
    assert_eq!(trace[700].to_bits(), (6100.0f64 / 600.0).to_bits());
    assert!((trace[700] - 10.166_666_666_666_666).abs() < 1e-12);
    assert_ne!(trace[700].to_bits(), 95.3125f64.to_bits());
    for k in 640..=700 {
        let want = 100.0 * (f64::from(k) - 639.0) / 600.0;
        assert_eq!(trace[k as usize].to_bits(), want.to_bits(), "t={k}");
    }
    for (k, y) in trace.iter().enumerate().take(640).skip(600) {
        assert_eq!(y.to_bits(), 0.0f64.to_bits(), "t={k}");
    }
    assert!(diag.events.borrow().is_empty(), "1 s ticks never coalesce");
    assert_eq!(
        region[LEN_WORD], 601,
        "one anchor at t-delta plus 600 in-window checkpoints"
    );
}

#[test]
fn long_irregular_run_matches_the_exact_window_mean() {
    // ~200 ticks per 300 s window over ~7 windows, every gap >= r = 300/1024 s.
    let delta = 300.0;
    let steps = irregular_steps(0x9e37_79b9_7f4a_7c15, 1400, 0.3, 2.7);
    let block = MovingAverage { delta };
    let diag = CapturingDiagnostics::default();
    let mut region = init_region(&block);
    let mirror = unbounded_history(delta, &steps);
    let mut full_window_ticks = 0;
    for (k, &(t, u)) in steps.iter().enumerate() {
        let y = real(&tick_with_diag(&block, &mut region, t, u, &diag));
        assert_eq!(y.to_bits(), mirror[k].to_bits(), "k={k} t={t}");
        let exact = held_backward_mean(delta, &steps, k);
        assert!(
            (y - exact).abs() <= 1e-9 * (1.0 + exact.abs()),
            "k={k} t={t}: engine {y} vs exact {exact}"
        );
        if t >= delta {
            full_window_ticks += 1;
        }
        block.validate_state(&region, t, t).unwrap();
    }
    assert!(full_window_ticks > 1000);
    assert!(diag.events.borrow().is_empty());
}

#[test]
fn hour_window_at_one_second_ticks_is_exact_and_never_coalesces() {
    // AHU-0004 uses a 3600 s window; 3600 ticks per window is 56x the former fixed ring.
    let delta = 3600.0;
    let mut stream = Stream(7);
    let steps: Vec<(f64, f64)> = (0..9000)
        .map(|k| (f64::from(k), (20.0 * stream.next()).floor()))
        .collect();
    let block = MovingAverage { delta };
    let diag = CapturingDiagnostics::default();
    let mut region = init_region(&block);
    let mirror = unbounded_history(delta, &steps);
    for (k, &(t, u)) in steps.iter().enumerate() {
        let y = real(&tick_with_diag(&block, &mut region, t, u, &diag));
        assert_eq!(y.to_bits(), mirror[k].to_bits(), "t={t}");
        if t >= delta {
            // Integer-valued inputs at 1 s ticks keep every partial sum exact.
            let integral = steps[k + 1 - 3600..=k]
                .iter()
                .map(|s| s.1)
                .fold(0.0, |acc, x| acc + x);
            assert_eq!(y.to_bits(), (integral / delta).to_bits(), "t={t}");
        }
    }
    assert!(diag.events.borrow().is_empty());
}

#[test]
fn warm_up_and_window_boundary_edges_follow_the_upstream_quotients() {
    let delta = 100.0;
    let block = MovingAverage { delta };
    let mut region = init_region(&block);
    let steps: Vec<(f64, f64)> = (0..=250)
        .map(|k| (f64::from(k), f64::from(k % 7)))
        .collect();
    for (k, &(t, u)) in steps.iter().enumerate() {
        let y = real(&tick(&block, &mut region, t, u));
        let integral = steps[1..=k].iter().map(|s| s.1).fold(0.0, |acc, x| acc + x);
        if t < delta {
            // Warm-up: mu over (t - tStart + 1e-3), including t=99, the last warm-up tick.
            assert_eq!(y.to_bits(), (integral / (t + 1e-3)).to_bits(), "t={t}");
        } else {
            // t=100 is the first full window: its target t-delta=0 is exactly the start
            // checkpoint, and every later target lands exactly on a retained checkpoint.
            let window = steps[k + 1 - 100..=k]
                .iter()
                .map(|s| s.1)
                .fold(0.0, |acc, x| acc + x);
            assert_eq!(y.to_bits(), (window / delta).to_bits(), "t={t}");
        }
    }

    // A target strictly between two checkpoints interpolates linearly in mu.
    let block = MovingAverage { delta: 2.5 };
    let steps = [(0.0, 0.0), (1.0, 4.0), (2.0, 8.0), (3.0, 2.0), (4.0, 6.0)];
    let (trace, _) = drive(&block, &steps);
    // t=3: window [0.5, 3] -> 4*0.5 + 8 + 2 = 12; t=4: window [1.5, 4] -> 8*0.5 + 2 + 6 = 12.
    assert_eq!(real(&trace[3]).to_bits(), (12.0f64 / 2.5).to_bits());
    assert_eq!(real(&trace[4]).to_bits(), (12.0f64 / 2.5).to_bits());

    // A gap longer than the window: the target falls between the previous tick and now.
    let block = MovingAverage { delta: 2.0 };
    let steps = [(0.0, 1.0), (1.0, 1.0), (10.0, 5.0)];
    let (trace, region) = drive(&block, &steps);
    // mu(8) interpolates (1, 1)..(10, 46) to 36, so y = (46 - 36)/2 = 5 up to rounding.
    assert!((real(&trace[2]) - 5.0).abs() <= 1e-12);
    block.validate_state(&region, 10.0, 10.0).unwrap();
}

/// Drive `steps` and require no coalescing, no warning and a bit-exact unbounded-history trace.
fn assert_exact_without_coalescing(delta: f64, steps: &[(f64, f64)]) -> Vec<u64> {
    let block = MovingAverage { delta };
    let diag = CapturingDiagnostics::default();
    let mut region = init_region(&block);
    let mirror = unbounded_history(delta, steps);
    for (k, &(t, u)) in steps.iter().enumerate() {
        let y = real(&tick_with_diag(&block, &mut region, t, u, &diag));
        assert_eq!(y.to_bits(), mirror[k].to_bits(), "delta={delta} t={t}");
    }
    assert!(diag.events.borrow().is_empty(), "delta={delta}");
    assert_eq!(region[COALESCED_WORD], 0, "delta={delta}");
    region
}

/// Nominal 1 s ticks, each displaced by a deterministic pseudo-random offset in `[-jitter, jitter]`.
fn jittered_one_second_steps(seed: u64, n: u32, jitter: f64) -> Vec<(f64, f64)> {
    let mut stream = Stream(seed);
    (0..n)
        .map(|k| {
            let offset = if k == 0 {
                0.0
            } else {
                jitter * (2.0 * stream.next() - 1.0)
            };
            (f64::from(k) + offset, (40.0 * stream.next()).floor() - 20.0)
        })
        .collect()
}

#[test]
fn jittered_one_second_hosts_never_coalesce() {
    // r <= 0.5 s for these windows, so t_k - t_{k-2} >= 2 - 2*jitter >= 2r while jitter <= 0.5 s.
    for (delta, n) in [(300.0, 2000), (3600.0, 9000)] {
        for (seed, jitter) in [(11, 0.005), (12, 0.4), (13, 0.5)] {
            let steps = jittered_one_second_steps(seed, n, jitter);
            assert_exact_without_coalescing(delta, &steps);
        }
    }
}

#[test]
fn gap_pairs_summing_to_exactly_two_r_are_retained() {
    // delta=1024 s -> 2048 slots, r=0.5 s. Gaps alternate 0.25/0.75 s, so every consecutive pair
    // sums to exactly 2r = 1 s; ~2048 ticks fill each window and none coalesce.
    let mut t = 0.0;
    let steps: Vec<(f64, f64)> = (0..6000)
        .map(|k| {
            if k > 0 {
                t += if k % 2 == 1 { 0.25 } else { 0.75 };
            }
            (t, f64::from(k % 13) - 6.0)
        })
        .collect();
    let region = assert_exact_without_coalescing(1024.0, &steps);
    let capacity = (region.len() - META_WORDS) / 2;
    assert!(region[LEN_WORD] as usize >= 2048 && (region[LEN_WORD] as usize) < capacity);
}

#[test]
fn coalescing_starts_at_the_first_gap_pair_shorter_than_two_r() {
    // Same 0.25/0.75 s pattern at delta=1024 (2r = 1 s); the last regular gap is 0.25 s. The
    // next gap is 0.75 s shortened by 1 ns, so that tick's pair sums to just under 2r and it is
    // the first to coalesce.
    let mut steps = Vec::new();
    let mut t = 0.0;
    for k in 0..400 {
        if k > 0 {
            t += if k % 2 == 1 { 0.25 } else { 0.75 };
        }
        steps.push((t, 1.0));
    }
    let short = t + 0.749_999_999;
    steps.push((short, 1.0));
    let block = MovingAverage { delta: 1024.0 };
    let diag = CapturingDiagnostics::default();
    let mut region = init_region(&block);
    for &(time, u) in &steps[..steps.len() - 1] {
        tick_with_diag(&block, &mut region, time, u, &diag);
    }
    assert!(diag.events.borrow().is_empty());
    let len_before = region[LEN_WORD];
    tick_with_diag(&block, &mut region, short, 1.0, &diag);
    assert_eq!(
        region[LEN_WORD], len_before,
        "the short pair coalesces into the newest slot"
    );
    let events = diag.events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].2.to_bits(), short.to_bits());
}

#[test]
fn ticks_denser_than_the_resolution_coalesce_without_shrinking_the_window() {
    // delta=1 s -> r = 1/2048 s; ticks every 1/4096 s put ~4096 ticks in each window.
    let delta = 1.0;
    let r = 1.0 / 2048.0;
    let block = MovingAverage { delta };
    let diag = CapturingDiagnostics::default();
    let mut region = init_region(&block);
    let capacity = (region.len() - META_WORDS) / 2;
    let steps: Vec<(f64, f64)> = (0..20_000)
        .map(|k| {
            let t = f64::from(k) / 4096.0;
            (
                t,
                if (t * 3.0).floor() as i64 % 2 == 0 {
                    40.0
                } else {
                    -60.0
                },
            )
        })
        .collect();
    let mut first_warn = None;
    for (k, &(t, u)) in steps.iter().enumerate() {
        let y = real(&tick_with_diag(&block, &mut region, t, u, &diag));
        if first_warn.is_none() && !diag.events.borrow().is_empty() {
            first_warn = Some(t);
        }
        // Retained checkpoints stay within slots + 3, so the defensive full-ring path is idle.
        assert!((region[LEN_WORD] as usize) < capacity, "t={t}");
        if t >= delta {
            // A coalesced gap g < 2r holds piecewise-constant input spanning at most 100 units;
            // the chord of mu across it deviates by at most g*100/4 < r*100/2, and the divisor is
            // still delta.
            let exact = held_backward_mean(delta, &steps, k);
            assert!(
                (y - exact).abs() <= r * 100.0 / (2.0 * delta) + 1e-9,
                "t={t}: engine {y} vs exact {exact}"
            );
        }
        let oldest = f64::from_bits(region[META_WORDS + region[HEAD_WORD] as usize]);
        assert!(
            oldest <= (t - delta).max(0.0),
            "t={t}: window start uncovered"
        );
        block.validate_state(&region, t, t).unwrap();
    }
    let events = diag.events.borrow();
    assert_eq!(events.len(), 1, "coalescing warns once per instance");
    assert_eq!(events[0].0, "CDL.Reals.MovingAverage");
    assert_eq!(events[0].1, COALESCED_WARNING);
    assert_eq!(region[COALESCED_WORD], 1);
    // The first two ticks are 1/4096 apart; the third lands within 2r of the first.
    assert_eq!(first_warn, Some(2.0 / 4096.0));
}

#[test]
fn constant_input_is_exact_even_when_coalesced() {
    // mu is linear for a constant input, so coalescing cannot move the mean.
    let block = MovingAverage { delta: 0.5 };
    let mut region = init_region(&block);
    for k in 0..10_000 {
        let t = f64::from(k) * 1.0e-4;
        let y = real(&tick(&block, &mut region, t, 3.0));
        if t >= 0.5 {
            assert!((y - 3.0).abs() <= 1e-9, "t={t}: {y}");
        }
    }
    assert_eq!(region[COALESCED_WORD], 1);
}

#[test]
fn full_ring_coalesces_into_the_newest_checkpoint_instead_of_evicting() {
    // Hand-build a full ring that the coalescing rule itself never produces: 2052 checkpoints
    // 0.0004 s apart, all inside one 1 s window. The next tick is 2r past the second-newest
    // checkpoint, so only the defensive capacity guard can stop the append. It must keep the
    // window anchor.
    let block = MovingAverage { delta: 1.0 };
    let mut region = init_region(&block);
    let capacity = (region.len() - META_WORDS) / 2;
    let times: Vec<f64> = (0..capacity).map(|k| k as f64 * 0.0004).collect();
    for (slot, &time) in times.iter().enumerate() {
        region[META_WORDS + slot] = time.to_bits();
        region[META_WORDS + capacity + slot] = (slot as f64).to_bits();
    }
    let last = *times.last().unwrap();
    region[0] = ((capacity - 1) as f64).to_bits();
    region[1] = 0.0f64.to_bits();
    region[2] = last.to_bits();
    region[HEAD_WORD] = 0;
    region[LEN_WORD] = capacity as u64;
    block.validate_state(&region, last, last).unwrap();

    let diag = CapturingDiagnostics::default();
    let t = times[capacity - 2] + 2.0 / 2048.0 + 1e-6;
    tick_with_diag(&block, &mut region, t, 1.0, &diag);
    assert_eq!(region[LEN_WORD] as usize, capacity);
    assert_eq!(region[HEAD_WORD], 0, "the window anchor is never evicted");
    assert_eq!(region[META_WORDS + capacity - 1], t.to_bits());
    assert_eq!(diag.events.borrow().len(), 1);
    block.validate_state(&region, t, t).unwrap();
}

#[test]
fn window_longer_than_the_slot_cap_keeps_its_full_span_at_a_coarser_resolution() {
    // delta = 2*131072 s -> resolution 2 s, so 1 s ticks coalesce (and warn once), but the
    // window is neither shortened nor refused.
    let delta = 262_144.0;
    let block = MovingAverage { delta };
    assert_eq!(block.geometry().resolution.to_bits(), 2.0f64.to_bits());
    let diag = CapturingDiagnostics::default();
    let mut region = init_region(&block);
    let mut y = 0.0;
    for k in 0..=270_000u32 {
        let u = if k <= 131_072 { 10.0 } else { 0.0 };
        y = real(&tick_with_diag(&block, &mut region, f64::from(k), u, &diag));
    }
    // Window [7856, 270000]: u = 10 over (7856, 131072], zero afterwards.
    let exact = 10.0 * (131_072.0 - 7856.0) / delta;
    assert!(
        (y - exact).abs() <= 2.0 * 2.0 * 10.0 / delta,
        "engine {y} vs exact {exact}"
    );
    assert_eq!(diag.events.borrow().len(), 1);
    block.validate_state(&region, 270_000.0, 270_000.0).unwrap();
}

#[test]
fn resolved_validator_refuses_foreign_sizes_and_uncovered_windows() {
    let block = MovingAverage { delta: 10.0 };
    let steps: Vec<(f64, f64)> = (0..50).map(|k| (f64::from(k), 1.0)).collect();
    let (_, region) = drive(&block, &steps);
    block.validate_state(&region, 49.0, 49.0).unwrap();

    let other = MovingAverage { delta: 2000.0 };
    assert!(other.validate_state(&region, 49.0, 49.0).is_err());

    // Advancing the head past the anchor would leave the window start uncovered.
    let mut shrunk = region.clone();
    shrunk[HEAD_WORD] += 1;
    shrunk[LEN_WORD] -= 1;
    assert!(block.validate_state(&shrunk, 49.0, 49.0).is_err());
}

#[test]
fn runs_and_mid_run_state_copies_continue_bit_identically() {
    let block = MovingAverage { delta: 120.0 };
    let steps = irregular_steps(42, 3000, 0.2, 1.4);
    let (trace_a, region_a) = drive(&block, &steps);
    let (trace_b, region_b) = drive(&block, &steps);
    assert_eq!(region_a, region_b);
    for (idx, (a, b)) in trace_a.iter().zip(&trace_b).enumerate() {
        assert!(a.bit_eq(b), "trace[{idx}]");
    }

    let mut region = init_region(&block);
    for &(t, u) in &steps[..1500] {
        tick(&block, &mut region, t, u);
    }
    let mut copy = region.clone();
    for (idx, &(t, u)) in steps.iter().enumerate().skip(1500) {
        let original = tick(&block, &mut region, t, u);
        let continued = tick(&block, &mut copy, t, u);
        assert!(original.bit_eq(&continued) && original.bit_eq(&trace_a[idx]));
    }
    assert_eq!(region, copy);
}

#[test]
fn hand_built_regions_refuse_bad_geometry_and_uncovered_window_starts() {
    use crate::state_contract::validate_moving_average;
    let (state_t, prev_t) = (2.0f64, 2.0f64);
    // A capacity-8 ring with a 1 s window: checkpoints at the start (0) and at `prev_t`.
    let mut moving = [0; 6 + 2 * 8];
    moving[2] = prev_t.to_bits();
    moving[LEN_WORD] = 2;
    moving[7] = prev_t.to_bits();
    assert!(validate_moving_average(&moving, state_t, prev_t, 8, 1.0).is_ok());
    let mut over_capacity = moving;
    over_capacity[4] = 9;
    assert!(validate_moving_average(&over_capacity, state_t, prev_t, 8, 1.0).is_err());
    let mut bad_head = moving;
    bad_head[3] = 8;
    assert!(validate_moving_average(&bad_head, state_t, prev_t, 8, 1.0).is_err());
    assert!(
        validate_moving_average(&moving, state_t, prev_t, 9, 1.0).is_err(),
        "a region sized for another delta must refuse"
    );
    // Oldest checkpoint after the start but later than `prev_t - delta`: the window start
    // is no longer covered, so continuation would silently shrink the window.
    let mut uncovered = moving;
    uncovered[6] = 1.5f64.to_bits();
    assert!(validate_moving_average(&uncovered, state_t, prev_t, 8, 1.0).is_err());
    // The same oldest checkpoint is legal once it sits at or before `prev_t - delta`.
    let mut covered = moving;
    covered[6] = 0.5f64.to_bits();
    assert!(validate_moving_average(&covered, state_t, prev_t, 8, 1.0).is_ok());
    let mut before_start = moving;
    before_start[1] = 0.25f64.to_bits();
    assert!(validate_moving_average(&before_start, state_t, prev_t, 8, 1.0).is_err());
}
