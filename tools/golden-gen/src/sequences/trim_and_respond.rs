//! G36 Generic.TrimAndRespond: the shared reset-chain oracle and its standalone reference.
//!
//! [`TrimAndRespondChain`] is the one golden-gen model of `Generic/TrimAndRespond.mo`
//! (`have_hol=false`). Every sequence oracle that nests a trim-and-respond reset (the standalone
//! block, SupplyFan `staPreSetRes`, SupplyTemperature `maxSupTemRes`) drives this chain, so the
//! two output switches are modeled once:
//!
//! - `swi` publishes `iniSet` until the `TrueDelay(delTim + samplePeriod, delayOnInit=true)` on
//!   the device status has elapsed, even when the `UnitDelay` still holds an older value;
//! - `swi2` feeds `iniSet` (not the clamped reset) into the `UnitDelay` while the device is off,
//!   so a sample instant inside an off window reinitializes the chain.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{
    TRIM_AND_RESPOND_HAVE_HOL_FALSE, clamp, initial_sample_time, input_b, input_i, r, sample_due,
    sample_index, sampler_output, sequence_golden, true_delay_output,
};

/// Parameters of one `Generic/TrimAndRespond.mo` instance with `have_hol=false`.
///
/// Units follow the reset variable (`iniSet`, `minSet`, `maxSet`, `triAmo`, `resAmo`, `maxRes`)
/// and seconds (`delTim`, `samplePeriod`). The `.mo` asserts, and every caller here keeps,
/// `triAmo` and `resAmo` of opposite sign and `resAmo`, `maxRes` of the same sign.
#[derive(Clone, Copy, Debug)]
pub(super) struct TrimAndRespondParameters {
    /// `iniSet`: initial setpoint, `UnitDelay.y_start`, and the `swi`/`swi2` fallback.
    pub ini_set: f64,
    /// `minSet`: lower clamp (`maxInp`).
    pub min_set: f64,
    /// `maxSet`: upper clamp (`min1`).
    pub max_set: f64,
    /// `delTim`, s: the TrueDelay waits `delTim + samplePeriod`.
    pub del_tim: f64,
    /// `samplePeriod`, s: Sampler and UnitDelay period.
    pub sample_period: f64,
    /// `numIgnReq`: ignored requests.
    pub num_ign_req: f64,
    /// `triAmo`: trim amount.
    pub tri_amo: f64,
    /// `resAmo`: respond amount per request above `numIgnReq`.
    pub res_amo: f64,
    /// `maxRes`: maximum response per sample period.
    pub max_res: f64,
}

/// Discrete state of one `TrimAndRespond` instance, advanced one host tick at a time.
///
/// `step` must be called with strictly increasing `t`. The TrueDelay, Sampler and UnitDelay share
/// the first tick as their start time, exactly as a single CDL instance does.
pub(super) struct TrimAndRespondChain {
    params: TrimAndRespondParameters,
    true_delay_timer: f64,
    true_delay_prev_time: Option<f64>,
    true_delay_prev_u: bool,
    true_delay_held: bool,
    sampler_held: f64,
    sampler_t0: f64,
    sampler_last_index: i64,
    sampler_initialized: bool,
    // Upstream Buildings `Discrete/UnitDelay.mo` pair: `y = pre(u_internal)` (held across the
    // interval) and the staged `u_internal` (sampled at the most recent instant). Both initialize
    // to y_start = iniSet.
    unit_delay_y: f64,
    unit_delay_u_internal: f64,
    unit_delay_t0: f64,
    unit_delay_last_index: i64,
    unit_delay_initialized: bool,
}

impl TrimAndRespondChain {
    /// A fresh instance: TrueDelay false, UnitDelay holding `iniSet`.
    pub(super) fn new(params: TrimAndRespondParameters) -> Self {
        Self {
            params,
            true_delay_timer: 0.0,
            true_delay_prev_time: None,
            true_delay_prev_u: false,
            true_delay_held: false,
            sampler_held: 0.0,
            sampler_t0: 0.0,
            sampler_last_index: -1,
            sampler_initialized: false,
            unit_delay_y: params.ini_set,
            unit_delay_u_internal: params.ini_set,
            unit_delay_t0: 0.0,
            unit_delay_last_index: -1,
            unit_delay_initialized: false,
        }
    }

    /// Evaluate the block output `y` at tick `t`, then commit the discrete state for the tick.
    ///
    /// `device_on` is `uDevSta`; `requests` is `numOfReq`. Returns `swi.y`.
    pub(super) fn step(&mut self, t: f64, device_on: bool, requests: i64) -> f64 {
        let p = self.params;
        // `tim`: TrueDelay(delTim + samplePeriod, delayOnInit=true) on uDevSta.
        let (tim, next_timer) = true_delay_output(
            t,
            device_on,
            p.del_tim + p.sample_period,
            true,
            self.true_delay_prev_time,
            self.true_delay_prev_u,
            self.true_delay_held,
            self.true_delay_timer,
        );
        // `sampler` of `intToRea(numOfReq)`, then `difReqIgnReq`.
        let sampled_requests = sampler_output(
            t,
            requests as f64,
            p.sample_period,
            self.sampler_initialized,
            self.sampler_t0,
            self.sampler_last_index,
            self.sampler_held,
        );
        let request_delta = sampled_requests - p.num_ign_req;
        // `pro`, `minInp`, `swi3`: response magnitude capped at |maxRes|, signed like resAmo.
        let response_magnitude = (p.res_amo.abs() * request_delta).min(p.max_res.abs());
        let response = if p.res_amo > 0.0 {
            response_magnitude
        } else {
            -response_magnitude
        };
        // `netRes`: and2 (tim and requests above numIgnReq) selects add2 = triAmo + response;
        // otherwise swi1 = tim ? triAmo : 0.
        let net_reset = if !tim {
            0.0
        } else if request_delta > 0.0 {
            p.tri_amo + response
        } else {
            p.tri_amo
        };
        // UnitDelay output at this tick: at a sample instant the `when` fires before the output
        // is read (y becomes the previously staged sample); between instants y holds.
        let unit_delay_out = if !self.unit_delay_initialized {
            self.unit_delay_y
        } else {
            let (due, _) = sample_due(
                t,
                self.unit_delay_t0,
                p.sample_period,
                self.unit_delay_last_index,
            );
            if due {
                self.unit_delay_u_internal
            } else {
                self.unit_delay_y
            }
        };
        // `add1`, `min1`, `maxInp`.
        let candidate = clamp(unit_delay_out + net_reset, p.min_set, p.max_set);
        // `swi2`: a device that is off reinitializes the chain; this is the UnitDelay input.
        let reset_chain = if device_on { candidate } else { p.ini_set };
        // `swi`: until the TrueDelay elapses the published setpoint is iniSet.
        let y = if tim { reset_chain } else { p.ini_set };

        self.true_delay_timer = next_timer;
        self.true_delay_prev_time = Some(t);
        self.true_delay_prev_u = device_on;
        self.true_delay_held = tim;

        if !self.sampler_initialized {
            self.sampler_t0 = initial_sample_time(t, p.sample_period);
            self.sampler_last_index = sample_index(t, self.sampler_t0, p.sample_period);
            self.sampler_held = requests as f64;
            self.sampler_initialized = true;
        } else {
            let (due, index) =
                sample_due(t, self.sampler_t0, p.sample_period, self.sampler_last_index);
            if due {
                self.sampler_last_index = index;
                self.sampler_held = requests as f64;
            }
        }

        if !self.unit_delay_initialized {
            self.unit_delay_t0 = initial_sample_time(t, p.sample_period);
            self.unit_delay_last_index = sample_index(t, self.unit_delay_t0, p.sample_period);
            // First instant: `y = pre(u_internal)` keeps y_start; the current input is staged.
            self.unit_delay_u_internal = reset_chain;
            self.unit_delay_initialized = true;
        } else {
            let (due, index) = sample_due(
                t,
                self.unit_delay_t0,
                p.sample_period,
                self.unit_delay_last_index,
            );
            if due {
                self.unit_delay_last_index = index;
                self.unit_delay_y = self.unit_delay_u_internal; // y = pre(u_internal)
                self.unit_delay_u_internal = reset_chain; // u_internal = u
            }
        }

        y
    }
}

/// Standalone `trim_and_respond_have_hol_false` parameters (the pinned fixture's literals).
const STANDALONE_PARAMETERS: TrimAndRespondParameters = TrimAndRespondParameters {
    ini_set: 10.0,
    min_set: 0.0,
    max_set: 20.0,
    del_tim: 600.0,
    sample_period: 120.0,
    num_ign_req: 2.0,
    tri_amo: 0.1,
    res_amo: -0.2,
    max_res: -0.6,
};

/// Standalone scenario: 0..=2400 s at 60 s ticks (every other tick is a 120 s sample instant).
///
/// The device runs from 0 s, so the TrueDelay (`delTim + samplePeriod = 720 s`) releases the
/// reset at 720 s. Three responds (720 s, 840 s, 960 s) and two trims (1080 s, 1200 s) move the
/// chain to 9.1. The device is then off for 1260..1379 s, a window holding exactly one sample
/// instant (1320 s): `swi2` stages iniSet into the UnitDelay there, while the UnitDelay output
/// still holds the pre-off 9.1. The restart at 1380 s lands between sample instants, so `swi`
/// must publish iniSet although the held value is 9.1. The re-armed TrueDelay releases at
/// 2100 s and the reset resumes from iniSet: a trim, a respond, and two more trims.
const STANDALONE_TICKS: u32 = 40;
const STANDALONE_TICK_SECONDS: f64 = 60.0;

pub(super) fn goldens() -> Vec<Golden> {
    let time = (0..=STANDALONE_TICKS)
        .map(|tick| f64::from(tick) * STANDALONE_TICK_SECONDS)
        .collect::<Vec<_>>();
    let request_count = time
        .iter()
        .map(|&t| {
            if (720.0..840.0).contains(&t) {
                3
            } else if (840.0..1080.0).contains(&t) {
                6
            } else if (2160.0..2280.0).contains(&t) {
                5
            } else {
                0
            }
        })
        .collect::<Vec<_>>();
    let device_status = time
        .iter()
        .map(|&t| !(1260.0..1380.0).contains(&t))
        .collect::<Vec<_>>();
    let setpoint = trim_and_respond_trace(&time, &request_count, &device_status);
    let inputs = trim_and_respond_inputs(&request_count, &device_status);

    vec![sequence_golden(
        TRIM_AND_RESPOND_HAVE_HOL_FALSE,
        "setpoint",
        ValueKind::Real,
        time,
        setpoint.into_iter().map(r).collect(),
        "TrimAndRespond have_hol=false over 0..2400s at 60s ticks: device proven on except 1260..1379s (one sample instant, 1320s, inside the off window; restart at 1380s between instants); requests 0 -> 3 at 720s -> 6 at 840s -> 0 at 1080s -> 5 at 2160s -> 0 at 2280s",
        "Pinned TrimAndRespond.mo have_hol=false variant: TrueDelay(delTim+samplePeriod=720s, delayOnInit) on uDevSta selects swi between iniSet=10 and the reset chain; swi2 feeds iniSet into UnitDelay(samplePeriod=120s, y_start=iniSet) while the device is off; net reset from Sampler(numOfReq) with triAmo=0.1, resAmo=-0.2, maxRes=-0.6, numIgnReq=2, clamp 0..20; inactive hold path is specialized away. Setpoint: 9.9 at 720s, 9.4 at 840s, 8.9 at 960s, trims to 9.0 and 9.1, iniSet 10 while off and through the re-armed delay (held 9.1 at the 1380s restart), then 10.1 at 2100s, 9.5 at 2160s, 9.6 at 2280s, 9.7 at 2400s",
        inputs,
    )]
}

fn trim_and_respond_trace(
    time: &[f64],
    request_count: &[i64],
    device_status: &[bool],
) -> Vec<f64> {
    let mut chain = TrimAndRespondChain::new(STANDALONE_PARAMETERS);
    time.iter()
        .zip(request_count)
        .zip(device_status)
        .map(|((&t, &requests), &device_on)| chain.step(t, device_on, requests))
        .collect()
}

fn trim_and_respond_inputs(request_count: &[i64], device_status: &[bool]) -> Vec<InputSeries> {
    vec![
        input_i("request_count", request_count.iter().copied()),
        input_b("device_status", device_status.iter().copied()),
    ]
}
