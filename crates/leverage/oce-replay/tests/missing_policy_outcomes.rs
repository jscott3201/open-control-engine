//! Every missing-sample policy and refusal policy, with the exact status column, substitution
//! mask, refusal rows and retained inputs (design note, test plan: missing policy; decisions 2,
//! 11, 13 and 20).
//!
//! The pairwise scenario runs `generic_time_suppression` (inputs `TSet`, slot 0, and `TZon`,
//! slot 1, both unbounded Real) over six frames one second apart with `TZon` missing at frames 2
//! and 3, for each policy on `TZon` × each `OnRefusal`. The `every_status` scenario
//! (`support/scenarios.rs`) mixes all four policies and an engine refusal in one run.

#[path = "support/controller.rs"]
mod controller;
#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/scenarios.rs"]
mod scenarios;

use fixtures::{Frames, columnar_with_missing, load, path_specs};
use oce_api::Value;
use oce_replay::{
    ColumnMap, FrameStatus, InputMap, MissingPolicy, OnRefusal, Progress, RefusalCause, RefusalRow,
    Replay, ReplayOptions, TimeBase, Trace, TraceColumn,
};

const FIXTURE: &str = "generic_time_suppression";
const TZON: &str = "http://example.org#g36.source.generic_time_suppression.TZon";

fn frames() -> Frames {
    (0..6)
        .map(|k| {
            let t = f64::from(k);
            (t, vec![Value::Real(295.0), Value::Real(290.0 + t)])
        })
        .collect()
}

fn run_pair(policy: MissingPolicy, on_refusal: OnRefusal) -> (Progress, Trace) {
    let engine = load(FIXTURE);
    let specs = path_specs(&engine);
    let map = InputMap::resolve(
        &engine.input_definitions().unwrap(),
        &specs,
        &ColumnMap::by_name(&specs),
        &MissingPolicy::Refuse,
        &[("TZon".to_owned(), policy)],
    )
    .unwrap();
    let source = columnar_with_missing(&map, &frames(), &[(2, 1), (3, 1)]);
    let mut options = ReplayOptions::new(TimeBase::Recorded);
    options.on_refusal = on_refusal;
    options.trace.inputs = true;
    let mut replay = Replay::new(engine, map, source, options).unwrap();
    let progress = replay.advance(100).unwrap();
    (progress, replay.finish().1)
}

fn reals(trace: &Trace, slot: usize) -> Vec<f64> {
    match &trace.inputs().unwrap()[slot].1 {
        TraceColumn::Real(v) => v.clone(),
        other => panic!("{other:?}"),
    }
}

fn missing_tzon(frame: u64) -> RefusalRow {
    RefusalRow {
        frame,
        cause: RefusalCause::MissingInput {
            slot: 1,
            path: TZON.to_owned(),
        },
    }
}

#[test]
fn refuse_stops_on_the_first_missing_frame() {
    let (progress, trace) = run_pair(MissingPolicy::Refuse, OnRefusal::Stop);
    assert_eq!(
        progress,
        Progress::Stopped {
            frame: 2,
            cause: missing_tzon(2).cause
        }
    );
    assert_eq!(trace.status(), [0, 0, 3]);
    assert_eq!(trace.refusals(), [missing_tzon(2)]);
    assert!(
        trace.substitution_mask().is_none(),
        "no policy can substitute"
    );
    assert_eq!(reals(&trace, 1), [290.0, 291.0, 0.0]);
    assert!(!trace.is_clean());
}

#[test]
fn refuse_and_continue_records_every_refused_frame() {
    let (progress, trace) = run_pair(MissingPolicy::Refuse, OnRefusal::RecordAndContinue);
    assert_eq!(progress, Progress::Done { frames: 6 });
    assert_eq!(trace.status(), [0, 0, 3, 3, 0, 0]);
    assert_eq!(trace.refusals(), [missing_tzon(2), missing_tzon(3)]);
    // Refused rows carry zero bits in the output columns.
    let TraceColumn::Boolean(out) = &trace.outputs()[0].1 else {
        panic!("generic_time_suppression's output is a Boolean");
    };
    assert_eq!(out[2], 0);
    assert_eq!(out[3], 0);
}

#[test]
fn hold_last_holds_within_max_age_and_refuses_beyond_it() {
    for on_refusal in [OnRefusal::Stop, OnRefusal::RecordAndContinue] {
        let (progress, trace) = run_pair(MissingPolicy::HoldLast { max_age: 1.5 }, on_refusal);
        // Frame 2 holds frame 1's sample (age 1); frame 3 would hold it at age 2 > 1.5. A hold of
        // frame 2's *substituted* value would be age 1 and pass: the refusal proves it is not.
        match on_refusal {
            OnRefusal::Stop => {
                assert_eq!(
                    progress,
                    Progress::Stopped {
                        frame: 3,
                        cause: missing_tzon(3).cause
                    }
                );
                assert_eq!(trace.status(), [0, 0, 1, 3]);
            }
            OnRefusal::RecordAndContinue => {
                assert_eq!(progress, Progress::Done { frames: 6 });
                assert_eq!(trace.status(), [0, 0, 1, 3, 0, 0]);
                assert_eq!(reals(&trace, 1), [290.0, 291.0, 291.0, 0.0, 294.0, 295.0]);
            }
        }
        assert_eq!(trace.refusals(), [missing_tzon(3)]);
        assert_eq!(trace.mask_width(), 1);
        let mask = trace.substitution_mask().unwrap();
        assert_eq!(&mask[..4], [0, 0, 0b10, 0]);
        assert!(trace.substituted(2, 1) && !trace.substituted(2, 0) && !trace.substituted(3, 1));
    }
}

#[test]
fn hold_last_at_exactly_max_age_is_held_and_one_ulp_beyond_is_refused() {
    for (max_age, expected) in [(2.0, 1_u8), (f64::from_bits(2.0_f64.to_bits() - 1), 3)] {
        // Frame 3 is two seconds after the last sample (frame 1): held iff 2 <= max_age.
        let (_, trace) = run_pair(
            MissingPolicy::HoldLast { max_age },
            OnRefusal::RecordAndContinue,
        );
        assert_eq!(trace.status()[3], expected, "max_age {max_age:e}");
    }
    // A non-integer age: the last sample at t = 1, the missing frame at t = 1.1. The age is
    // `t - t_sample` in f64 (here exact, by Sterbenz), held iff it is <= max_age.
    let t: f64 = 1.1;
    let age = t - 1.0;
    let next_up = |x: f64| f64::from_bits(x.to_bits() + 1);
    for (t, max_age, expected) in [
        (t, age, 1_u8),
        (next_up(t), age, 3),
        (t, f64::from_bits(age.to_bits() - 1), 3),
    ] {
        let frames: Frames = [0.0, 1.0, t]
            .iter()
            .map(|&t| (t, vec![Value::Real(295.0), Value::Real(290.0)]))
            .collect();
        let engine = load(FIXTURE);
        let specs = path_specs(&engine);
        let map = InputMap::resolve(
            &engine.input_definitions().unwrap(),
            &specs,
            &ColumnMap::by_name(&specs),
            &MissingPolicy::HoldLast { max_age },
            &[],
        )
        .unwrap();
        let source = columnar_with_missing(&map, &frames, &[(2, 1)]);
        let mut options = ReplayOptions::new(TimeBase::Recorded);
        options.on_refusal = OnRefusal::RecordAndContinue;
        let (_, trace) = Replay::new(engine, map, source, options)
            .unwrap()
            .run()
            .unwrap();
        assert_eq!(trace.status()[2], expected, "t {t:e} max_age {max_age:e}");
    }
}

#[test]
fn fallback_substitutes_the_declared_value_and_never_refuses() {
    for on_refusal in [OnRefusal::Stop, OnRefusal::RecordAndContinue] {
        let (progress, trace) = run_pair(MissingPolicy::Fallback(Value::Real(21.0)), on_refusal);
        assert_eq!(progress, Progress::Done { frames: 6 });
        assert_eq!(trace.status(), [0, 0, 1, 1, 0, 0]);
        assert!(trace.refusals().is_empty());
        assert_eq!(trace.substitution_mask().unwrap(), [0, 0, 0b10, 0b10, 0, 0]);
        assert_eq!(reals(&trace, 1), [290.0, 291.0, 21.0, 21.0, 294.0, 295.0]);
    }
}

#[test]
fn skip_frame_skips_and_continues_under_either_refusal_policy() {
    for on_refusal in [OnRefusal::Stop, OnRefusal::RecordAndContinue] {
        let (progress, trace) = run_pair(MissingPolicy::SkipFrame, on_refusal);
        assert_eq!(progress, Progress::Done { frames: 6 });
        assert_eq!(trace.status(), [0, 0, 2, 2, 0, 0]);
        assert!(trace.refusals().is_empty());
        assert!(
            trace.substitution_mask().is_none(),
            "SkipFrame never substitutes"
        );
        assert_eq!(reals(&trace, 1), [290.0, 291.0, 0.0, 0.0, 294.0, 295.0]);
        assert_eq!(
            reals(&trace, 0)[2],
            295.0,
            "a skipped row keeps its resolved inputs"
        );
    }
}

#[test]
fn every_status_has_the_exact_status_column_mask_and_refusal_rows() {
    let (_, trace) = scenarios::every_status(|_| {}).run().unwrap();
    let expected: [u8; 24] = [
        0, 0, 1, 1, 2, 0, 3, 0, 1, 4, 0, 1, 3, 3, 0, 0, 3, 0, 0, 0, 1, 3, 0, 0,
    ];
    assert_eq!(trace.status(), expected);
    let mask: Vec<u8> = trace.substitution_mask().unwrap().to_vec();
    let mut expected_mask = vec![0_u8; 24];
    for (row, bits) in [
        (2, 0b1),
        (3, 0b1_0000),
        (8, 0b1),
        (11, 0b1),
        (13, 0b1_0000),
        (20, 0b1_0000),
    ] {
        expected_mask[row] = bits;
    }
    assert_eq!(mask, expected_mask);
    let path = |slot: usize| scenarios::every_status_map().inputs()[slot].path.clone();
    let refusals: Vec<(u64, Option<usize>)> = trace
        .refusals()
        .iter()
        .map(|r| match &r.cause {
            RefusalCause::MissingInput { slot, path: p } => {
                assert_eq!(p, &path(*slot));
                (r.frame, Some(*slot))
            }
            RefusalCause::Engine { message, .. } => {
                assert!(message.contains("uRetDam_max"), "{message}");
                (r.frame, None)
            }
        })
        .collect();
    assert_eq!(
        refusals,
        [
            (6, Some(2)),
            (9, None),
            (12, Some(0)),
            (13, Some(0)),
            (16, Some(3)),
            (21, Some(0))
        ]
    );
    // Frame 9's engine refusal still records the values it submitted.
    let TraceColumn::Real(retained) = &trace.inputs().unwrap()[2].1 else {
        panic!()
    };
    assert_eq!(retained[9], 2.0);
    assert_eq!(
        FrameStatus::from_byte(trace.status()[9]),
        Some(FrameStatus::RefusedEngine)
    );
}

#[test]
fn nan_blank_and_cleared_validity_are_missing_and_infinity_is_a_value_under_both_policies() {
    use oce_api::NonFiniteInputPolicy::{Admit, Refuse};
    for non_finite in [Admit, Refuse] {
        // NaN (Real) and a cleared validity flag (Integer, Boolean) on the controller.
        let reference = controller::reference();
        let engine = load(controller::FIXTURE);
        let map = controller::map(&engine, &reference, &MissingPolicy::Refuse);
        let mut columns = controller::columns(&reference);
        let mut set_missing = |name: &str, row: usize| {
            let (_, column) = columns.iter_mut().find(|(n, _)| n == name).unwrap();
            match column {
                oce_replay::Column::Real(v) => v[row] = f64::NAN,
                oce_replay::Column::Integer { valid, .. }
                | oce_replay::Column::Boolean { valid, .. } => {
                    let mut flags = vec![true; 1_441];
                    flags[row] = false;
                    *valid = Some(flags);
                }
                oce_replay::Column::Enum { .. } => unreachable!(),
            }
        };
        set_missing("zone_temperature", 1);
        set_missing("operating_mode", 2);
        set_missing("supply_fan_status", 3);
        let source = oce_replay::ColumnarSource::new(
            &map,
            oce_replay::TimeColumn::Seconds(reference.column("time")),
            columns,
            &oce_replay::ReplayLimits::default(),
        )
        .unwrap();
        let mut options = ReplayOptions::new(TimeBase::Recorded);
        options.on_refusal = OnRefusal::RecordAndContinue;
        options.non_finite = non_finite;
        let mut replay = Replay::new(engine, map, source, options).unwrap();
        replay.advance(5).unwrap();
        let trace = replay.trace();
        assert_eq!(trace.meta().non_finite, non_finite);
        assert_eq!(trace.status(), [0, 3, 3, 3, 0], "{non_finite:?}");
        let slots: Vec<usize> = trace
            .refusals()
            .iter()
            .map(|r| match &r.cause {
                RefusalCause::MissingInput { slot, .. } => *slot,
                RefusalCause::Engine { .. } => panic!("NaN reached the engine"),
            })
            .collect();
        let slot_of = |suffix: &str| {
            trace
                .meta()
                .inputs
                .iter()
                .position(|i| i.path.ends_with(suffix))
                .unwrap()
        };
        assert_eq!(
            slots,
            [slot_of(".TZon"), slot_of(".uOpeMod"), slot_of(".u1Fan")]
        );

        // +∞ and -∞ are values: the engine sees them and decides.
        for infinity in [f64::INFINITY, f64::NEG_INFINITY] {
            let mut frames = frames();
            frames[1].1[1] = Value::Real(infinity);
            let engine = load(FIXTURE);
            let map = fixtures::path_map(&engine, &MissingPolicy::Refuse);
            let source = columnar_with_missing(&map, &frames, &[]);
            let mut options = ReplayOptions::new(TimeBase::Recorded);
            options.on_refusal = OnRefusal::RecordAndContinue;
            options.non_finite = non_finite;
            let (_, trace) = Replay::new(engine, map, source, options)
                .unwrap()
                .run()
                .unwrap();
            let expected = if non_finite == Refuse { 4 } else { 0 };
            assert_eq!(trace.status()[1], expected, "{non_finite:?} {infinity}");
            if non_finite == Refuse {
                let RefusalCause::Engine { message, .. } = &trace.refusals()[0].cause else {
                    panic!("an infinity is not missing");
                };
                assert!(message.contains("TZon"), "{message}");
            }
        }
    }
}

#[test]
fn the_default_non_finite_policy_is_refuse_and_is_recorded() {
    let options = ReplayOptions::new(TimeBase::Recorded);
    assert_eq!(options.non_finite, oce_api::NonFiniteInputPolicy::Refuse);
    assert_eq!(options.on_refusal, OnRefusal::Stop);
    assert!(matches!(MissingPolicy::default(), MissingPolicy::Refuse));
    let (_, trace) = run_pair(MissingPolicy::Refuse, OnRefusal::Stop);
    assert_eq!(
        trace.meta().non_finite,
        oce_api::NonFiniteInputPolicy::Refuse
    );
}

#[test]
fn blank_na_and_nan_csv_cells_are_missing_under_both_policies_and_inf_is_a_value() {
    use oce_api::NonFiniteInputPolicy::{Admit, Refuse};
    for non_finite in [Admit, Refuse] {
        let engine = load(FIXTURE);
        let map = fixtures::path_map(&engine, &MissingPolicy::Refuse);
        let text = format!(
            "t,{},{}\n0,295,293\n1,295,\n2,295,NA\n3,295,NaN\n4,295,nan\n5,295,inf\n6,295,293\n",
            map.column(0),
            map.column(1)
        );
        let source = oce_replay::CsvSource::new(
            &map,
            text.as_bytes(),
            oce_replay::CsvFormat::seconds("t"),
            &oce_replay::ReplayLimits::default(),
        )
        .unwrap();
        let mut options = ReplayOptions::new(TimeBase::Recorded);
        options.on_refusal = OnRefusal::RecordAndContinue;
        options.non_finite = non_finite;
        let (_, trace) = Replay::new(engine, map, source, options)
            .unwrap()
            .run()
            .unwrap();
        let inf = if non_finite == Refuse { 4 } else { 0 };
        assert_eq!(trace.status(), [0, 3, 3, 3, 3, inf, 0], "{non_finite:?}");
        assert!(
            trace.refusals()[..4]
                .iter()
                .all(|r| r.cause == missing_tzon(r.frame).cause)
        );
    }
}

#[test]
fn an_expired_hold_refuses_the_frame_even_when_another_input_skips_it() {
    // TSet: HoldLast { max_age: 0.5 }; TZon: SkipFrame. Frame 2 lacks both: TSet's hold is one
    // second old (expired), so refusal dominates the skip and names TSet.
    for on_refusal in [OnRefusal::RecordAndContinue, OnRefusal::Stop] {
        let engine = load(FIXTURE);
        let specs = path_specs(&engine);
        let map = InputMap::resolve(
            &engine.input_definitions().unwrap(),
            &specs,
            &ColumnMap::by_name(&specs),
            &MissingPolicy::HoldLast { max_age: 0.5 },
            &[("TZon".to_owned(), MissingPolicy::SkipFrame)],
        )
        .unwrap();
        let source = columnar_with_missing(&map, &frames(), &[(2, 0), (2, 1), (3, 1)]);
        let mut options = ReplayOptions::new(TimeBase::Recorded);
        options.on_refusal = on_refusal;
        let mut replay = Replay::new(engine, map, source, options).unwrap();
        let progress = replay.advance(100).unwrap();
        let (_, trace) = replay.finish();
        let tset = RefusalCause::MissingInput {
            slot: 0,
            path: "http://example.org#g36.source.generic_time_suppression.TSet".to_owned(),
        };
        assert_eq!(
            trace.refusals()[0],
            RefusalRow {
                frame: 2,
                cause: tset.clone()
            }
        );
        match on_refusal {
            OnRefusal::Stop => {
                assert_eq!(
                    progress,
                    Progress::Stopped {
                        frame: 2,
                        cause: tset
                    }
                );
                assert_eq!(trace.status(), [0, 0, 3]);
            }
            OnRefusal::RecordAndContinue => {
                // Frame 3 lacks only TZon: skipped.
                assert_eq!(trace.status(), [0, 0, 3, 2, 0, 0]);
            }
        }
        // The mask is per input and per row whatever the status: no substitution happened here.
        assert!(trace.substitution_mask().unwrap().iter().all(|b| *b == 0));
    }
}

#[test]
fn the_mask_records_a_substitution_on_skipped_and_refused_rows() {
    // TSet: Fallback; TZon: SkipFrame on frame 2, Refuse on frame 3 (run-level).
    for (tzon_policy, expected) in [(MissingPolicy::SkipFrame, 2_u8), (MissingPolicy::Refuse, 3)] {
        let engine = load(FIXTURE);
        let specs = path_specs(&engine);
        let map = InputMap::resolve(
            &engine.input_definitions().unwrap(),
            &specs,
            &ColumnMap::by_name(&specs),
            &MissingPolicy::Refuse,
            &[
                (
                    "TSet".to_owned(),
                    MissingPolicy::Fallback(Value::Real(21.0)),
                ),
                ("TZon".to_owned(), tzon_policy),
            ],
        )
        .unwrap();
        let source = columnar_with_missing(&map, &frames(), &[(2, 0), (2, 1)]);
        let mut options = ReplayOptions::new(TimeBase::Recorded);
        options.on_refusal = OnRefusal::RecordAndContinue;
        let (_, trace) = Replay::new(engine, map, source, options)
            .unwrap()
            .run()
            .unwrap();
        assert_eq!(trace.status()[2], expected);
        assert!(trace.substituted(2, 0) && !trace.substituted(2, 1));
    }
}
