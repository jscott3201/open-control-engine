//! Closed-form continuous-time CDL PI and a forward-Euler departure bound.
//!
//! Shared by `g36_freeze_protection_heating_coil_continuous_time.rs`; kept in a subdirectory so
//! Cargo does not compile it as its own integration-test binary. It depends on nothing but `std`.

/// Closed-form continuous-time CDL PI (no derivative, `r = 1`, reverse acting).
#[derive(Clone, Copy)]
pub struct ContinuousPi {
    pub k: f64,
    pub ti: f64,
    pub ni: f64,
    pub y_min: f64,
    pub y_max: f64,
}

impl ContinuousPi {
    pub fn output(&self, x: f64, e: f64) -> f64 {
        (self.k * e + x).clamp(self.y_min, self.y_max)
    }

    /// `der(xI)` at state `x` and error `e`.
    pub fn rate(&self, x: f64, e: f64) -> f64 {
        let unlimited = self.k * e + x;
        let limited = unlimited.clamp(self.y_min, self.y_max);
        (self.k / self.ti) * (e - (unlimited - limited) / (self.k * self.ni))
    }

    /// Operating mode at state `x` and error `e`. On a limit, the error's sign decides whether
    /// the loop is entering saturation or moving off it.
    pub fn mode(&self, x: f64, e: f64) -> Mode {
        let unlimited = self.k * e + x;
        if unlimited > self.y_max || (unlimited == self.y_max && e > 0.0) {
            Mode::Saturated(self.y_max)
        } else if unlimited < self.y_min || (unlimited == self.y_min && e < 0.0) {
            Mode::Saturated(self.y_min)
        } else {
            Mode::Unsaturated
        }
    }

    /// Exact integrator state after `duration` seconds of constant error `e`, starting from `x`.
    ///
    /// The mode is classified once from the starting state and then carried across each switch,
    /// so a switching state that rounds back past a limit cannot re-enter the mode it just left.
    pub fn advance(&self, mut x: f64, e: f64, duration: f64) -> f64 {
        let tau = self.ti * self.ni;
        let mut remaining = duration;
        let mut mode = self.mode(x, e);
        // Each pass ends in a mode switch; a constant error crosses at most three modes.
        for _ in 0..4 {
            if remaining <= 0.0 {
                return x;
            }
            match mode {
                Mode::Saturated(limit) => {
                    // der(x) = -(x - target)/tau; the loop leaves this limit when x reaches
                    // `boundary`, which happens only if the target lies beyond it.
                    let target = limit + self.k * e * (self.ni - 1.0);
                    let boundary = limit - self.k * e;
                    let leaves = (x - boundary) * (target - boundary) < 0.0;
                    if leaves {
                        let exit = tau * ((x - target) / (boundary - target)).ln();
                        if exit < remaining {
                            x = boundary;
                            remaining -= exit;
                            mode = Mode::Unsaturated;
                            continue;
                        }
                    }
                    return target + (x - target) * (-remaining / tau).exp();
                }
                Mode::Unsaturated => {
                    let slope = (self.k / self.ti) * e;
                    if slope == 0.0 {
                        return x;
                    }
                    let limit = if slope > 0.0 { self.y_max } else { self.y_min };
                    let reach = ((limit - self.k * e - x) / slope).max(0.0);
                    if reach >= remaining {
                        return x + slope * remaining;
                    }
                    x = limit - self.k * e;
                    remaining -= reach;
                    mode = Mode::Saturated(limit);
                }
            }
        }
        panic!("constant-error PI response switched modes more than three times");
    }
}

/// PI operating mode under a constant error.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Unsaturated,
    /// Output held at this limit; the anti-windup term is active.
    Saturated(f64),
}

/// Forward Euler with the frame convention of the HostTick engine: each frame's output uses the
/// state before the frame, and the frame then integrates its own error over the time since the
/// previous frame (nothing on the first frame).
pub fn forward_euler(pi: &ContinuousPi, errors: &[f64], step: f64) -> Vec<f64> {
    let mut x = 0.0;
    let mut out = Vec::with_capacity(errors.len());
    for (frame, &e) in errors.iter().enumerate() {
        out.push(pi.output(x, e));
        let dt = if frame == 0 { 0.0 } else { step };
        x += pi.rate(x, e) * dt;
    }
    out
}

/// Rounding allowance added to the bound per frame; far below any departure under test.
pub const ROUNDING_PER_FRAME: f64 = 1e-12;

/// Continuous-time PI outputs at each frame instant, and a per-frame bound on how far a
/// forward-Euler trace with the HostTick frame convention can lie from them.
pub struct ClosedForm {
    /// Closed-form PI output at each frame instant.
    pub output: Vec<f64>,
    /// Bound on the forward-Euler output departure at each frame instant.
    pub bound: Vec<f64>,
}

/// Solve the PI in closed form over the frame grid and bound the forward-Euler departure.
///
/// Write `x_n` for the closed-form state at frame `n` and `z_n` for the Euler state, which steps
/// `z_{n+1} = z_n + h_n*rate(z_n, e_n)` with `h_0 = 0` and `h_n` the time since the previous
/// frame. Then `|z_{n+1} - x_{n+1}| <= L_n*|z_n - x_n| + local_n`, where:
///
/// - `local_n = |x_{n+1} - (x_n + h_n*rate(x_n, e_n))|` is Euler's one-frame truncation error
///   started from the exact state, computed here from the closed form itself. It is zero on
///   unsaturated ramps, the full first-frame increment on frame 0, and positive across
///   relaxations and mode corners;
/// - `L_n` is the Lipschitz constant of the Euler map `z -> z + h_n*rate(z, e_n)`. That map is
///   continuous and piecewise linear with slopes `1` (unsaturated) and `1 - h_n/(Ti*Ni)`
///   (saturated), so `L_n <= 1` whenever `h_n <= Ti*Ni`. When the exact state is saturated with a
///   margin larger than the current bound, every state within the bound is saturated at the same
///   limit and `L_n = 1 - h_n/(Ti*Ni)` exactly.
///
/// The output clamps `k*e + x` with the same `e` on both sides, so the output departure at frame
/// `n` is at most the state bound at frame `n`.
pub fn closed_form(pi: &ContinuousPi, time: &[f64], errors: &[f64]) -> ClosedForm {
    let tau = pi.ti * pi.ni;
    let mut x = 0.0;
    let mut bound: f64 = 0.0;
    let mut output = Vec::with_capacity(time.len());
    let mut bounds = Vec::with_capacity(time.len());
    for (row, &e) in errors.iter().enumerate() {
        output.push(pi.output(x, e));
        bounds.push(bound);
        let Some(next) = time.get(row + 1) else {
            break;
        };
        let euler_step = if row == 0 {
            0.0
        } else {
            time[row] - time[row - 1]
        };
        let exact_next = pi.advance(x, e, next - time[row]);
        let local = (exact_next - (x + euler_step * pi.rate(x, e))).abs();
        let unlimited = pi.k * e + x;
        let saturation_margin = (unlimited - pi.y_max).max(pi.y_min - unlimited);
        let contraction = if saturation_margin > bound {
            1.0 - euler_step / tau
        } else {
            1.0
        };
        // Above h = Ti*Ni the Euler map can expand differences, and no bound is claimed.
        bound = if euler_step <= tau {
            contraction * bound + local + ROUNDING_PER_FRAME
        } else {
            f64::INFINITY
        };
        x = exact_next;
    }
    ClosedForm {
        output,
        bound: bounds,
    }
}
