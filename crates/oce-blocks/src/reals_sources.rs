//! Stateless `CDL.Reals.Sources` blocks whose outputs are derived from model time or parameters.
//!
//! These are source roots: they have no input connectors, carry no per-tick state, and read time
//! only from the scheduler-provided [`Ctx`]. They intentionally do not consult wall-clock or
//! daylight-saving APIs.

use oce_model::{Value, ZeroTime};

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, emit_real};

/// Minimum valid `CDL.Reals.Sources.Ramp.duration`, from `CDL.Constants.small`.
pub(crate) const MIN_SOURCE_RAMP_DURATION: f64 = 1e-37;

const FIRST_CALENDAR_YEAR: i32 = 2010;
const LAST_CALENDAR_YEAR: i32 = 2050;
const LAST_CUSTOM_YEAR: i32 = 2031;
const SECONDS_PER_HOUR: f64 = 3_600.0;
const SECONDS_PER_DAY: f64 = 86_400.0;
const MINUTES_PER_DAY: f64 = 1_440.0;

const NEW_YEAR_TIMESTAMPS: [f64; 42] = [
    1_262_304_000.0,
    1_293_840_000.0,
    1_325_376_000.0,
    1_356_998_400.0,
    1_388_534_400.0,
    1_420_070_400.0,
    1_451_606_400.0,
    1_483_228_800.0,
    1_514_764_800.0,
    1_546_300_800.0,
    1_577_836_800.0,
    1_609_459_200.0,
    1_640_995_200.0,
    1_672_531_200.0,
    1_704_067_200.0,
    1_735_689_600.0,
    1_767_225_600.0,
    1_798_761_600.0,
    1_830_297_600.0,
    1_861_920_000.0,
    1_893_456_000.0,
    1_924_992_000.0,
    1_956_528_000.0,
    1_988_150_400.0,
    2_019_686_400.0,
    2_051_222_400.0,
    2_082_758_400.0,
    2_114_380_800.0,
    2_145_916_800.0,
    2_177_452_800.0,
    2_208_988_800.0,
    2_240_611_200.0,
    2_272_147_200.0,
    2_303_683_200.0,
    2_335_219_200.0,
    2_366_841_600.0,
    2_398_377_600.0,
    2_429_913_600.0,
    2_461_449_600.0,
    2_493_072_000.0,
    2_524_608_000.0,
    2_556_144_000.0,
];
const LEAP_YEARS: [bool; 41] = [
    false, false, true, false, false, false, true, false, false, false, true, false, false, false,
    true, false, false, false, true, false, false, false, true, false, false, false, true, false,
    false, false, true, false, false, false, true, false, false, false, true, false, false,
];
const DAYS_IN_MONTH: [i32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// Source-verified `CDL.Types.ZeroTime` members in ordinal order.
pub(crate) const ZERO_TIME_MEMBERS: &[&str] = &[
    "UnixTimeStamp",
    "UnixTimeStampGMT",
    "Custom",
    "NY2010",
    "NY2011",
    "NY2012",
    "NY2013",
    "NY2014",
    "NY2015",
    "NY2016",
    "NY2017",
    "NY2018",
    "NY2019",
    "NY2020",
    "NY2021",
    "NY2022",
    "NY2023",
    "NY2024",
    "NY2025",
    "NY2026",
    "NY2027",
    "NY2028",
    "NY2029",
    "NY2030",
    "NY2031",
    "NY2032",
    "NY2033",
    "NY2034",
    "NY2035",
    "NY2036",
    "NY2037",
    "NY2038",
    "NY2039",
    "NY2040",
    "NY2041",
    "NY2042",
    "NY2043",
    "NY2044",
    "NY2045",
    "NY2046",
    "NY2047",
    "NY2048",
    "NY2049",
    "NY2050",
];

/// `CDL.Reals.Sources.CivilTime` - emit the host scheduler's model time in seconds.
///
/// The block has no parameters, inputs, state, panics, or wall-clock dependency. Negative
/// simulation starts are preserved because the CDL source equation is exactly `y = time`.
#[derive(Clone, Copy, Debug, Default)]
pub struct CivilTime;

impl Block for CivilTime {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Sources.CivilTime",
            inputs: &[],
            outputs: &[PortKind::Real],
            stateful: false,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn step_algebraic(&self, ctx: &Ctx<'_>, _inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_real(0, ctx.t(), emit);
    }
}

/// `CDL.Reals.Sources.CalendarTime` - decompose model time into local calendar fields.
///
/// The source delegates to `Buildings.Utilities.Time.CalendarTime` with `timZon=0` and
/// `outputUnixTimeStamp=false`, so the native block emits only `year`, `month`, `day`, `hour`,
/// `minute`, and `weekDay` for `unixTimeStampLocal = time + offset + timOff`. The implementation
/// computes these fields directly from the pinned timestamp table instead of carrying the helper's
/// event-state optimization; it never reads wall-clock time, locale, daylight-saving rules, or host
/// timezone. Invalid direct-construction parameters or out-of-range model times warn and emit
/// `0,0,0,0,NaN,0`.
#[derive(Clone, Copy, Debug)]
pub struct CalendarTime {
    /// Reference epoch selection.
    pub(crate) zer_tim: ZeroTime,
    /// Custom reference year used when `zerTim=Custom`.
    pub(crate) year_ref: i64,
    /// Offset added to model time in seconds.
    pub(crate) offset: f64,
}

impl Default for CalendarTime {
    fn default() -> Self {
        Self {
            zer_tim: ZeroTime::NewYear(2016),
            year_ref: 2016,
            offset: 0.0,
        }
    }
}

impl CalendarTime {
    fn output(self, t: f64) -> Option<CalendarFields> {
        if !t.is_finite() || !self.offset.is_finite() {
            return None;
        }
        let unix = t + self.offset + self.time_offset()?;
        calendar_fields(unix)
    }

    fn time_offset(self) -> Option<f64> {
        match self.zer_tim {
            ZeroTime::UnixTimeStamp | ZeroTime::UnixTimeStampGmt => Some(0.0),
            ZeroTime::Custom => {
                let year = i32::try_from(self.year_ref).ok()?;
                if !(FIRST_CALENDAR_YEAR..=LAST_CUSTOM_YEAR).contains(&year) {
                    return None;
                }
                Some(NEW_YEAR_TIMESTAMPS[source_year_index(year)?])
            }
            ZeroTime::NewYear(year) => Some(NEW_YEAR_TIMESTAMPS[source_year_index(year)?]),
        }
    }
}

impl Block for CalendarTime {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Sources.CalendarTime",
            inputs: &[],
            outputs: &[
                PortKind::Integer,
                PortKind::Integer,
                PortKind::Integer,
                PortKind::Integer,
                PortKind::Real,
                PortKind::Integer,
            ],
            stateful: false,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn step_algebraic(&self, ctx: &Ctx<'_>, _inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let fields = self.output(ctx.t());
        if fields.is_none() {
            ctx.warn(
                self.signature().class_path,
                "CalendarTime: invalid reference time or timestamp outside 2010-01-01..2051-01-01",
            );
        }
        let fields = fields.unwrap_or(CalendarFields {
            year: 0,
            month: 0,
            day: 0,
            hour: 0,
            minute: f64::NAN,
            week_day: 0,
        });
        emit(0, Value::Integer(i64::from(fields.year)));
        emit(1, Value::Integer(i64::from(fields.month)));
        emit(2, Value::Integer(i64::from(fields.day)));
        emit(3, Value::Integer(i64::from(fields.hour)));
        emit_real(4, fields.minute, emit);
        emit(5, Value::Integer(i64::from(fields.week_day)));
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CalendarFields {
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: f64,
    week_day: i32,
}

fn source_year_index(year: i32) -> Option<usize> {
    match year {
        2010..=2023 => usize::try_from(year - FIRST_CALENDAR_YEAR).ok(),
        // Source compatibility: the pinned helper maps NY2024 and Custom yearRef=2024 to the
        // same timestamp slot as NY2023, and later years one slot early.
        2024..=LAST_CALENDAR_YEAR => usize::try_from(year - FIRST_CALENDAR_YEAR - 1).ok(),
        _ => None,
    }
}

fn calendar_fields(unix: f64) -> Option<CalendarFields> {
    if !unix.is_finite()
        || unix < NEW_YEAR_TIMESTAMPS[0]
        || unix >= NEW_YEAR_TIMESTAMPS[NEW_YEAR_TIMESTAMPS.len() - 1]
    {
        return None;
    }
    let year_index = NEW_YEAR_TIMESTAMPS
        .windows(2)
        .position(|pair| unix >= pair[0] && unix < pair[1])?;
    let year = FIRST_CALENDAR_YEAR + i32::try_from(year_index).ok()?;
    let day_of_year = ((unix - NEW_YEAR_TIMESTAMPS[year_index]) / SECONDS_PER_DAY).floor() as i32;
    let (month, day) = month_day(year_index, day_of_year)?;
    let hour = ((unix % SECONDS_PER_DAY) / SECONDS_PER_HOUR).floor() as i32;
    let days_since_epoch = (unix / SECONDS_PER_DAY).floor() as i64;
    let week_day = ((4 + days_since_epoch - 1).rem_euclid(7) + 1) as i32;
    let minute = unix / 60.0 - days_since_epoch as f64 * MINUTES_PER_DAY - f64::from(hour) * 60.0;
    Some(CalendarFields {
        year,
        month,
        day,
        hour,
        minute,
        week_day,
    })
}

fn month_day(year_index: usize, day_of_year: i32) -> Option<(i32, i32)> {
    let mut remaining = day_of_year;
    for (month_index, base_days) in DAYS_IN_MONTH.iter().copied().enumerate() {
        let days = base_days + i32::from(month_index == 1 && LEAP_YEARS[year_index]);
        if remaining < days {
            return Some((i32::try_from(month_index + 1).ok()?, remaining + 1));
        }
        remaining -= days;
    }
    None
}

/// `CDL.Reals.Sources.Ramp` - parameterized time ramp source.
///
/// The CDL equation is `offset` before `startTime`, then a linear interpolation to
/// `offset + height` over `duration`, and finally `offset + height`. Validation requires
/// `duration >= CDL.Constants.small`; direct construction falls back to `1.0 s` when given an
/// invalid duration so the tick path remains total and never divides by zero.
#[derive(Clone, Copy, Debug)]
pub struct SourceRamp {
    /// Ramp height added after the duration elapses.
    pub(crate) height: f64,
    /// Ramp duration in seconds.
    pub(crate) duration: f64,
    /// Output before the ramp starts.
    pub(crate) offset: f64,
    /// Model time at which the ramp begins.
    pub(crate) start_time: f64,
}

impl Default for SourceRamp {
    fn default() -> Self {
        Self {
            height: 1.0,
            duration: 1.0,
            offset: 0.0,
            start_time: 0.0,
        }
    }
}

impl SourceRamp {
    fn duration_eff(self) -> f64 {
        if self.duration.is_finite() && self.duration >= MIN_SOURCE_RAMP_DURATION {
            self.duration
        } else {
            1.0
        }
    }

    fn output(self, t: f64) -> f64 {
        let duration = self.duration_eff();
        self.offset
            + if t < self.start_time {
                0.0
            } else if t < self.start_time + duration {
                (t - self.start_time) * self.height / duration
            } else {
                self.height
            }
    }
}

impl Block for SourceRamp {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Sources.Ramp",
            inputs: &[],
            outputs: &[PortKind::Real],
            stateful: false,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn step_algebraic(&self, ctx: &Ctx<'_>, _inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_real(0, self.output(ctx.t()), emit);
    }
}

/// `CDL.Reals.Sources.Sin` - parameterized sine-wave source.
///
/// The source has no inputs or state. It emits `offset` before `startTime`; at and after
/// `startTime`, it emits `offset + amplitude*sin(2*pi*freqHz*(time-startTime)+phase)`.
/// Parameters are not asserted by the Buildings source, so non-finite values are evaluated through
/// deterministic `libm` math and emitted with canonical NaN bits where applicable. The block never
/// panics and reads only scheduler-provided model time.
#[derive(Clone, Copy, Debug)]
pub struct SourceSin {
    /// Sine-wave amplitude.
    pub(crate) amplitude: f64,
    /// Sine-wave frequency in hertz.
    pub(crate) freq_hz: f64,
    /// Phase offset in radians.
    pub(crate) phase: f64,
    /// Output offset before and during the waveform.
    pub(crate) offset: f64,
    /// Model time at which the sine waveform begins.
    pub(crate) start_time: f64,
}

impl Default for SourceSin {
    fn default() -> Self {
        Self {
            amplitude: 1.0,
            freq_hz: 1.0,
            phase: 0.0,
            offset: 0.0,
            start_time: 0.0,
        }
    }
}

impl SourceSin {
    fn output(self, t: f64) -> f64 {
        self.offset
            + if t < self.start_time {
                0.0
            } else {
                self.amplitude
                    * libm::sin(
                        2.0 * std::f64::consts::PI * self.freq_hz * (t - self.start_time)
                            + self.phase,
                    )
            }
    }
}

impl Block for SourceSin {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Sources.Sin",
            inputs: &[],
            outputs: &[PortKind::Real],
            stateful: false,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn step_algebraic(&self, ctx: &Ctx<'_>, _inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_real(0, self.output(ctx.t()), emit);
    }
}
