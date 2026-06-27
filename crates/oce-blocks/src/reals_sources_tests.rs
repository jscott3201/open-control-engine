use std::cell::RefCell;

use oce_model::determinism::CANONICAL_NAN_BITS;
use oce_model::{EnumClassId, ParamTable, Value, ZeroTime};

use super::{
    Block, BlockKind, CalendarTime, CivilTime, Ctx, Diagnostics, NoopDiagnostics, ParamRule,
    SourceRamp, SourceSin, Time, lookup,
};

fn out_at(block: &dyn Block, t: Time) -> Value {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let mut out = None;
    block.step_algebraic(&cx, &[], &mut |idx, val| {
        assert_eq!(idx, 0);
        out = Some(val);
    });
    out.expect("single-output source emits one value")
}

fn assert_source_trace(block: &dyn Block, cases: &[(Time, f64)]) {
    for &(t, want) in cases {
        let got = out_at(block, t);
        assert!(
            got.bit_eq(&Value::Real(want)),
            "t={t}: got {got:?}, want {want:?}"
        );
    }
}

fn outs_at(block: &dyn Block, t: Time) -> Vec<Value> {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let mut out = Vec::new();
    block.step_algebraic(&cx, &[], &mut |idx, val| {
        assert_eq!(idx, out.len());
        out.push(val);
    });
    out
}

#[derive(Default)]
struct CapturingDiagnostics {
    events: RefCell<Vec<(String, String, Time)>>,
}

impl Diagnostics for CapturingDiagnostics {
    fn warn(&self, source: &str, message: &str, t: Time) {
        self.events
            .borrow_mut()
            .push((source.to_owned(), message.to_owned(), t));
    }
}

fn assert_calendar_fields(
    got: &[Value],
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: f64,
    week_day: i64,
) {
    assert_eq!(got.len(), 6);
    assert!(got[0].bit_eq(&Value::Integer(year)), "{got:?}");
    assert!(got[1].bit_eq(&Value::Integer(month)), "{got:?}");
    assert!(got[2].bit_eq(&Value::Integer(day)), "{got:?}");
    assert!(got[3].bit_eq(&Value::Integer(hour)), "{got:?}");
    match &got[4] {
        Value::Real(actual) => assert!(
            (*actual - minute).abs() <= 1.0e-9,
            "minute {actual:?} != {minute:?}; full output {got:?}"
        ),
        other => panic!("minute output must be Real, got {other:?}"),
    }
    assert!(got[5].bit_eq(&Value::Integer(week_day)), "{got:?}");
}

#[test]
fn civil_time_emits_scheduler_time_without_state() {
    let block = CivilTime;
    assert_eq!(block.kind(), BlockKind::Algebraic);
    assert_eq!(block.state_len(), 0);
    assert!(block.signature().inputs.is_empty());
    assert_eq!(block.signature().outputs, &[crate::PortKind::Real]);
    assert_source_trace(&block, &[(-1.0, -1.0), (0.0, 0.0), (1.25, 1.25)]);
}

#[test]
fn calendar_time_signature_and_validation_month_boundaries_match_buildings() {
    let block = CalendarTime {
        zer_tim: ZeroTime::NewYear(2017),
        year_ref: 2016,
        offset: 0.0,
    };
    assert_eq!(block.kind(), BlockKind::Algebraic);
    assert_eq!(block.state_len(), 0);
    assert!(block.signature().inputs.is_empty());
    assert_eq!(
        block.signature().outputs,
        &[
            crate::PortKind::Integer,
            crate::PortKind::Integer,
            crate::PortKind::Integer,
            crate::PortKind::Integer,
            crate::PortKind::Real,
            crate::PortKind::Integer,
        ]
    );
    assert_calendar_fields(
        &outs_at(&block, 172_799.0),
        2017,
        1,
        2,
        23,
        59.0 + 59.0 / 60.0,
        1,
    );
    assert_calendar_fields(&outs_at(&block, 172_800.0), 2017, 1, 3, 0, 0.0, 2);
    assert_calendar_fields(&outs_at(&block, 345_600.0), 2017, 1, 5, 0, 0.0, 4);
}

#[test]
fn calendar_time_handles_leap_day_unix_epoch_offset_and_source_year_anomaly() {
    let leap = CalendarTime {
        zer_tim: ZeroTime::NewYear(2020),
        year_ref: 2016,
        offset: 0.0,
    };
    assert_calendar_fields(&outs_at(&leap, 59.0 * 86_400.0), 2020, 2, 29, 0, 0.0, 6);
    assert_calendar_fields(&outs_at(&leap, 60.0 * 86_400.0), 2020, 3, 1, 0, 0.0, 7);

    let unix = CalendarTime {
        zer_tim: ZeroTime::UnixTimeStamp,
        year_ref: 2016,
        offset: 0.0,
    };
    assert_calendar_fields(&outs_at(&unix, 1_262_304_000.0), 2010, 1, 1, 0, 0.0, 5);

    let offset = CalendarTime {
        zer_tim: ZeroTime::NewYear(2017),
        year_ref: 2016,
        offset: 3_600.0,
    };
    assert_calendar_fields(&outs_at(&offset, 23.0 * 3_600.0), 2017, 1, 2, 0, 0.0, 1);

    let anomaly = CalendarTime {
        zer_tim: ZeroTime::NewYear(2024),
        year_ref: 2016,
        offset: 0.0,
    };
    assert_calendar_fields(&outs_at(&anomaly, 0.0), 2023, 1, 1, 0, 0.0, 7);

    let custom_anomaly = CalendarTime {
        zer_tim: ZeroTime::Custom,
        year_ref: 2024,
        offset: 0.0,
    };
    assert_calendar_fields(&outs_at(&custom_anomaly, 0.0), 2023, 1, 1, 0, 0.0, 7);
}

#[test]
fn calendar_time_invalid_direct_construction_warns_and_emits_total_fallback() {
    let diag = CapturingDiagnostics::default();
    let cx = Ctx::new(0.0, &diag);
    let block = CalendarTime {
        zer_tim: ZeroTime::UnixTimeStamp,
        year_ref: 2016,
        offset: 0.0,
    };
    let mut out = Vec::new();
    block.step_algebraic(&cx, &[], &mut |idx, val| {
        assert_eq!(idx, out.len());
        out.push(val);
    });
    assert_eq!(out.len(), 6);
    assert!(out[0].bit_eq(&Value::Integer(0)));
    assert!(out[1].bit_eq(&Value::Integer(0)));
    assert!(out[2].bit_eq(&Value::Integer(0)));
    assert!(out[3].bit_eq(&Value::Integer(0)));
    assert!(out[4].bit_eq(&Value::Real(f64::from_bits(CANONICAL_NAN_BITS))));
    assert!(out[5].bit_eq(&Value::Integer(0)));

    let events = diag.events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "CDL.Reals.Sources.CalendarTime");
    assert!(
        events[0]
            .1
            .contains("timestamp outside 2010-01-01..2051-01-01")
    );
    assert_eq!(events[0].2.to_bits(), 0.0f64.to_bits());
}

#[test]
fn source_ramp_matches_piecewise_boundaries() {
    let block = SourceRamp {
        height: 2.0,
        duration: 3.0,
        offset: 0.5,
        start_time: 1.0,
    };
    assert_source_trace(
        &block,
        &[(0.0, 0.5), (1.0, 0.5), (2.5, 1.5), (4.0, 2.5), (5.0, 2.5)],
    );
}

#[test]
fn source_ramp_allows_negative_height_and_start_time() {
    let block = SourceRamp {
        height: -2.0,
        duration: 2.0,
        offset: 10.0,
        start_time: -1.0,
    };
    assert_source_trace(
        &block,
        &[(-2.0, 10.0), (-1.0, 10.0), (0.0, 9.0), (1.0, 8.0)],
    );
}

#[test]
fn source_ramp_direct_invalid_duration_degrades_to_one_second() {
    let block = SourceRamp {
        height: 2.0,
        duration: 0.0,
        offset: 0.0,
        start_time: 0.0,
    };
    assert_source_trace(&block, &[(0.0, 0.0), (0.5, 1.0), (1.0, 2.0), (2.0, 2.0)]);
}

#[test]
fn source_ramp_nan_output_is_canonicalized() {
    let block = SourceRamp {
        height: f64::from_bits(0x7ff0_0000_0000_0001),
        duration: 1.0,
        offset: 0.0,
        start_time: 0.0,
    };
    assert!(out_at(&block, 2.0).bit_eq(&Value::Real(f64::from_bits(CANONICAL_NAN_BITS))));
}

#[test]
fn source_sin_matches_source_equation_boundaries() {
    let block = SourceSin {
        amplitude: 2.0,
        freq_hz: 0.25,
        phase: std::f64::consts::FRAC_PI_2,
        offset: 0.5,
        start_time: 1.0,
    };
    assert_source_trace(
        &block,
        &[
            (0.0, 0.5),
            (1.0, 0.5 + 2.0 * libm::sin(std::f64::consts::FRAC_PI_2)),
            (2.0, 0.5 + 2.0 * libm::sin(std::f64::consts::PI)),
            (3.0, -1.5),
            (4.0, 0.5 + 2.0 * libm::sin(2.0 * std::f64::consts::PI)),
            (
                5.0,
                0.5 + 2.0 * libm::sin(2.0 * std::f64::consts::PI + std::f64::consts::FRAC_PI_2),
            ),
        ],
    );
}

#[test]
fn source_sin_allows_negative_frequency_phase_and_start_time() {
    let block = SourceSin {
        amplitude: 3.0,
        freq_hz: -0.5,
        phase: std::f64::consts::FRAC_PI_2,
        offset: -1.0,
        start_time: -2.0,
    };
    assert_source_trace(
        &block,
        &[
            (-3.0, -1.0),
            (-2.0, 2.0),
            (-1.5, -1.0),
            (-1.0, -4.0),
            (0.0, 2.0),
        ],
    );
}

#[test]
fn source_sin_nan_output_is_canonicalized() {
    let block = SourceSin {
        amplitude: f64::from_bits(0x7ff0_0000_0000_0001),
        ..SourceSin::default()
    };
    assert!(out_at(&block, 0.0).bit_eq(&Value::Real(f64::from_bits(CANONICAL_NAN_BITS))));
}

#[test]
fn source_sin_non_finite_frequency_follows_libm_domain() {
    let block = SourceSin {
        freq_hz: f64::INFINITY,
        ..SourceSin::default()
    };
    assert!(out_at(&block, 1.0).bit_eq(&Value::Real(f64::from_bits(CANONICAL_NAN_BITS))));
}

#[test]
fn source_blocks_are_pure_for_repeated_step() {
    let blocks: [&dyn Block; 3] = [
        &CivilTime,
        &SourceRamp {
            height: 2.0,
            duration: 3.0,
            offset: 0.5,
            start_time: 1.0,
        },
        &SourceSin {
            amplitude: 2.0,
            freq_hz: 0.25,
            phase: std::f64::consts::FRAC_PI_2,
            offset: 0.5,
            start_time: 1.0,
        },
    ];
    for block in blocks {
        let a = out_at(block, 2.5);
        let b = out_at(block, 2.5);
        assert!(a.bit_eq(&b), "{} diverged", block.signature().class_path);
        assert_eq!(block.state_len(), 0);
        assert!(!block.feeds_through(0, 0));
    }

    let calendar = CalendarTime {
        zer_tim: ZeroTime::NewYear(2017),
        year_ref: 2016,
        offset: 0.0,
    };
    let first = outs_at(&calendar, 172_800.0);
    let second = outs_at(&calendar, 172_800.0);
    assert_eq!(first.len(), second.len());
    for (left, right) in first.iter().zip(&second) {
        assert!(left.bit_eq(right));
    }
    assert_eq!(calendar.state_len(), 0);
    assert!(!calendar.feeds_through(0, 0));
}

#[test]
fn source_ramp_registry_rules_are_pinned() {
    assert!(
        lookup("CDL.Reals.Sources.CivilTime")
            .unwrap()
            .param_rules()
            .is_empty()
    );
    assert_eq!(
        lookup("CDL.Reals.Sources.Ramp").unwrap().param_rules(),
        &[
            ParamRule::Required { name: "duration" },
            ParamRule::RealGreaterOrEqual {
                name: "duration",
                min: 1e-37,
            },
        ]
    );
    assert_eq!(
        lookup("CDL.Reals.Sources.Sin").unwrap().param_rules(),
        &[
            ParamRule::Required { name: "freqHz" },
            ParamRule::Real { name: "amplitude" },
            ParamRule::Real { name: "freqHz" },
            ParamRule::Real { name: "phase" },
            ParamRule::Real { name: "offset" },
            ParamRule::Real { name: "startTime" },
        ]
    );
    assert_eq!(
        lookup("CDL.Reals.Sources.CalendarTime")
            .unwrap()
            .param_rules(),
        &[
            ParamRule::Required { name: "zerTim" },
            ParamRule::EnumMembers {
                name: "zerTim",
                members: &[
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
                ],
            },
            ParamRule::IntegerGreaterOrEqual {
                name: "yearRef",
                min: 2010,
            },
            ParamRule::IntegerLessOrEqualConstant {
                name: "yearRef",
                max: 2031,
            },
            ParamRule::RealFinite { name: "offset" },
        ]
    );
}

#[test]
fn source_registry_constructors_resolve_parameters() {
    let civil_time = (lookup("CDL.Reals.Sources.CivilTime").unwrap().make)(&ParamTable::default());
    assert!(out_at(civil_time.as_ref(), -1.0).bit_eq(&Value::Real(-1.0)));

    let source_ramp = (lookup("CDL.Reals.Sources.Ramp").unwrap().make)(&ParamTable {
        values: vec![
            ("height".into(), Value::Real(2.0)),
            ("duration".into(), Value::Real(3.0)),
            ("offset".into(), Value::Real(0.5)),
            ("startTime".into(), Value::Real(1.0)),
        ],
    });
    assert!(out_at(source_ramp.as_ref(), 2.5).bit_eq(&Value::Real(1.5)));

    let source_sin = (lookup("CDL.Reals.Sources.Sin").unwrap().make)(&ParamTable {
        values: vec![
            ("amplitude".into(), Value::Real(2.0)),
            ("freqHz".into(), Value::Real(0.25)),
            ("phase".into(), Value::Real(std::f64::consts::FRAC_PI_2)),
            ("offset".into(), Value::Real(0.5)),
            ("startTime".into(), Value::Real(1.0)),
        ],
    });
    assert!(out_at(source_sin.as_ref(), 1.0).bit_eq(&Value::Real(
        0.5 + 2.0 * libm::sin(std::f64::consts::FRAC_PI_2)
    )));

    let calendar = (lookup("CDL.Reals.Sources.CalendarTime").unwrap().make)(&ParamTable {
        values: vec![
            (
                "zerTim".into(),
                Value::Enum {
                    class: EnumClassId::ZERO_TIME,
                    ordinal: 11,
                },
            ),
            ("yearRef".into(), Value::Integer(2016)),
            ("offset".into(), Value::Real(0.0)),
        ],
    });
    assert_calendar_fields(
        &outs_at(calendar.as_ref(), 172_800.0),
        2017,
        1,
        3,
        0,
        0.0,
        2,
    );
}
