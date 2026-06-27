//! `CDL.Reals.Sources` goldens derived from the Buildings source equations.

use crate::oracle::{Golden, Sample, ValueKind};

fn r(x: f64) -> Sample {
    Sample::Real(x)
}

fn i(x: i64) -> Sample {
    Sample::Integer(x)
}

/// Build stateless `CDL.Reals.Sources` goldens beyond Constant.
pub fn goldens() -> Vec<Golden> {
    let mut out = Vec::new();

    {
        let time = vec![-1.0, 0.0, 1.0];
        let samples = time.iter().copied().map(r).collect();
        out.push(Golden::new(
            "CDL.Reals.Sources.CivilTime",
            "y",
            ValueKind::Real,
            time,
            samples,
            "no inputs or parameters; scheduler/model time samples include negative start",
            "y = time; Buildings Controls OBC CDL Reals Sources CivilTime.mo",
        ));
    }

    {
        let time = vec![0.0, 1.0, 2.5, 4.0, 5.0];
        let height = 2.0;
        let duration = 3.0;
        let offset = 0.5;
        let start_time = 1.0;
        let samples = time
            .iter()
            .copied()
            .map(|t| r(source_ramp_y(t, height, duration, offset, start_time)))
            .collect();
        out.push(Golden::new(
            "CDL.Reals.Sources.Ramp",
            "y",
            ValueKind::Real,
            time,
            samples,
            "height=2, duration=3, offset=0.5, startTime=1; before, start, interior, end, after",
            "y = offset + (if time < startTime then 0 else if time < startTime+duration then (time-startTime)*height/duration else height); Buildings Reals/Sources/Ramp.mo",
        ));
    }

    {
        let time = vec![-2.0, -1.0, 0.0, 1.0];
        let height = -2.0;
        let duration = 2.0;
        let offset = 10.0;
        let start_time = -1.0;
        let samples = time
            .iter()
            .copied()
            .map(|t| r(source_ramp_y(t, height, duration, offset, start_time)))
            .collect();
        out.push(
            Golden::new(
                "CDL.Reals.Sources.Ramp",
                "y",
                ValueKind::Real,
                time,
                samples,
                "scenario=negative_height_start; height=-2, duration=2, offset=10, startTime=-1",
                "same source piecewise equation with negative height and negative simulation start",
            )
            .with_scenario("negative_height_start"),
        );
    }

    {
        let time = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let amplitude = 2.0;
        let freq_hz = 0.25;
        let phase = std::f64::consts::FRAC_PI_2;
        let offset = 0.5;
        let start_time = 1.0;
        let samples = time
            .iter()
            .copied()
            .map(|t| r(source_sin_y(t, amplitude, freq_hz, phase, offset, start_time)))
            .collect();
        out.push(
            Golden::new(
                "CDL.Reals.Sources.Sin",
                "y",
                ValueKind::Real,
                time,
                samples,
                "amplitude=2, freqHz=0.25, phase=pi/2, offset=0.5, startTime=1; before start, observable start boundary, phase plumbing, and period samples",
                "y = offset + (if time < startTime then 0 else amplitude*sin(2*pi*freqHz*(time-startTime)+phase)); Buildings Reals/Sources/Sin.mo, deterministic libm 0.2.16",
            )
            .with_provenance(
                "math_library",
                "libm 0.2.16, default-features=false, pure Rust deterministic math",
            ),
        );
    }

    {
        let time = vec![172_799.0, 172_800.0, 172_801.0, 345_600.0];
        let fields = calendar_time_fields(&time, 2017, 0.0);
        push_calendar_time_goldens(
            &mut out,
            None,
            time,
            &fields,
            "zerTim=NY2017, yearRef=2016 ignored, offset=0; validation-month start, +/-1s, and stop boundary",
            "unixTimeStampLocal = time + offset + timOff(NY2017); decompose with Buildings.Utilities.Time.CalendarTime table and Monday=1 weekDay convention",
        );
    }

    {
        let time = vec![0.0, 86_400.0];
        let fields = calendar_time_fields(&time, 2024, 0.0);
        push_calendar_time_goldens(
            &mut out,
            Some("source_year_anomaly"),
            time,
            &fields,
            "zerTim=NY2024, yearRef=2016 ignored, offset=0; source table indexes NY2024 to the NY2023 timestamp slot",
            "source-compatible helper index: NY2010..NY2023 map directly; NY2024..NY2050 map one slot early in the pinned Buildings helper",
        );
    }

    out
}

fn push_calendar_time_goldens(
    out: &mut Vec<Golden>,
    scenario: Option<&'static str>,
    time: Vec<f64>,
    fields: &[CalendarFields],
    input_desc: &'static str,
    rule_desc: &'static str,
) {
    let outputs = [
        (
            "year",
            ValueKind::Integer,
            fields.iter().map(|field| i(i64::from(field.year))).collect(),
        ),
        (
            "month",
            ValueKind::Integer,
            fields
                .iter()
                .map(|field| i(i64::from(field.month)))
                .collect(),
        ),
        (
            "day",
            ValueKind::Integer,
            fields.iter().map(|field| i(i64::from(field.day))).collect(),
        ),
        (
            "hour",
            ValueKind::Integer,
            fields.iter().map(|field| i(i64::from(field.hour))).collect(),
        ),
        (
            "minute",
            ValueKind::Real,
            fields.iter().map(|field| r(field.minute)).collect(),
        ),
        (
            "weekDay",
            ValueKind::Integer,
            fields
                .iter()
                .map(|field| i(i64::from(field.week_day)))
                .collect(),
        ),
    ];
    for (signal, kind, samples) in outputs {
        let mut golden = Golden::new(
            "CDL.Reals.Sources.CalendarTime",
            signal,
            kind,
            time.clone(),
            samples,
            input_desc,
            rule_desc,
        )
        .with_provenance(
            "source_snapshot",
            "Buildings.Controls.OBC.CDL.Reals.Sources.CalendarTime.mo plus Buildings.Utilities.Time.CalendarTime.mo at a131864e4c4df22ebcd52bb8da439de0087ac365",
        );
        if let Some(scenario) = scenario {
            golden = golden.with_scenario(scenario);
        }
        out.push(golden);
    }
}

fn source_ramp_y(t: f64, height: f64, duration: f64, offset: f64, start_time: f64) -> f64 {
    offset
        + if t < start_time {
            0.0
        } else if t < start_time + duration {
            (t - start_time) * height / duration
        } else {
            height
        }
}

fn source_sin_y(
    t: f64,
    amplitude: f64,
    freq_hz: f64,
    phase: f64,
    offset: f64,
    start_time: f64,
) -> f64 {
    offset
        + if t < start_time {
            0.0
        } else {
            amplitude * libm::sin(2.0 * std::f64::consts::PI * freq_hz * (t - start_time) + phase)
        }
}

const FIRST_CALENDAR_YEAR: i32 = 2010;
const LAST_CALENDAR_YEAR: i32 = 2050;
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

#[derive(Clone, Copy)]
struct CalendarFields {
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: f64,
    week_day: i32,
}

fn calendar_time_fields(time: &[f64], reference_year: i32, offset: f64) -> Vec<CalendarFields> {
    let tim_off = NEW_YEAR_TIMESTAMPS[source_year_index(reference_year)];
    time.iter()
        .map(|t| {
            calendar_fields(t + offset + tim_off).expect("oracle times stay in helper range")
        })
        .collect()
}

fn source_year_index(year: i32) -> usize {
    match year {
        2010..=2023 => (year - FIRST_CALENDAR_YEAR) as usize,
        2024..=LAST_CALENDAR_YEAR => (year - FIRST_CALENDAR_YEAR - 1) as usize,
        _ => panic!("unsupported CalendarTime reference year {year}"),
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
    let year = FIRST_CALENDAR_YEAR + year_index as i32;
    let day_of_year = ((unix - NEW_YEAR_TIMESTAMPS[year_index]) / SECONDS_PER_DAY).floor() as i32;
    let (month, day) = month_day(year_index, day_of_year)?;
    let hour = ((unix % SECONDS_PER_DAY) / SECONDS_PER_HOUR).floor() as i32;
    let days_since_epoch = (unix / SECONDS_PER_DAY).floor() as i64;
    let week_day = ((days_since_epoch + 3).rem_euclid(7) + 1) as i32;
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
            return Some((month_index as i32 + 1, remaining + 1));
        }
        remaining -= days;
    }
    None
}
