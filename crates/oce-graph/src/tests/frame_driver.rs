//! The HostTick v1 frame driver on hand-built executables: the refusal order, that every refusal
//! precedes mutation, fan-out staging, equal-time transitions, the sequence and time limits, and
//! the domain rule's edge cases. The facade-level and blob-level equivalence over the G36 corpus
//! lives in `oce-api` (`blob_dual_path_tests.rs`) and `tests/blob_execution.rs`.

use super::*;
use crate::frame::{
    FrameDriver, FrameInputs, FrameRefusal, InputSlot, MAX_FRAME_TIME_MAGNITUDE,
    NonFiniteInputPolicy, accepts_domain, accepts_type, check_time, time_is_representable,
};
use oce_model::EnumClassId;

/// An executable with a sampled delay (so time representability bites) and an adder, its slot
/// table and its clock.
struct Rig {
    model: Model,
    schedule: Schedule,
    blocks: Vec<Box<dyn Block>>,
    state: RunState,
    prev_t: Option<f64>,
    sequence: u64,
    slots: Vec<(Vec<ConnectorId>, Option<Value>, Option<Value>)>,
    delay_out: ConnectorId,
    sum_out: ConnectorId,
}

/// Slot 0 is a Real in `[-10, 10]` fanned out to the delay and the adder's first input; slot 1 is
/// an unbounded Real into the adder's second input.
fn rig() -> Rig {
    let mut b = ModelBuilder::default();
    let (_, delay_in, delay_out) = b.block_real(make(
        "CDL.Discrete.UnitDelay",
        &[("samplePeriod", Value::Real(1.0))],
    ));
    let (_, add_in, add_out) = b.block_real(make("CDL.Reals.Add", &[]));
    b.block_real(Box::new(WarningSource));
    let schedule = compile(&b.model, &b.blocks).unwrap();
    let state = allocate_state(&b.model, &b.blocks).unwrap();
    Rig {
        model: b.model,
        schedule,
        blocks: b.blocks,
        state,
        prev_t: None,
        sequence: 0,
        slots: vec![
            (
                vec![delay_in[0], add_in[0]],
                Some(Value::Real(-10.0)),
                Some(Value::Real(10.0)),
            ),
            (vec![add_in[1]], None, None),
        ],
        delay_out: delay_out[0],
        sum_out: add_out[0],
    }
}

struct Frame<'a> {
    slots: &'a [(Vec<ConnectorId>, Option<Value>, Option<Value>)],
    values: &'a [Value],
}

impl FrameInputs for Frame<'_> {
    fn slot_count(&self) -> usize {
        self.slots.len()
    }
    fn value_count(&self) -> usize {
        self.values.len()
    }
    fn slot(&self, slot: usize) -> InputSlot<'_> {
        let (targets, min, max) = &self.slots[slot];
        InputSlot {
            targets,
            min: min.as_ref(),
            max: max.as_ref(),
        }
    }
    fn value(&self, slot: usize) -> &Value {
        &self.values[slot]
    }
}

impl Rig {
    fn execute(
        &mut self,
        time: f64,
        values: &[Value],
        policy: NonFiniteInputPolicy,
        diagnostics: &dyn Diagnostics,
    ) -> Result<u64, FrameRefusal> {
        FrameDriver {
            model: &self.model,
            schedule: &self.schedule,
            blocks: &self.blocks,
            state: &mut self.state,
            prev_t: &mut self.prev_t,
            sequence: &mut self.sequence,
        }
        .execute(
            time,
            &Frame {
                slots: &self.slots,
                values,
            },
            policy,
            diagnostics,
        )
    }

    /// Every bit the driver may write: connector values, words, time, clock and sequence.
    fn image(&self) -> String {
        let values: Vec<String> = self
            .state
            .values
            .iter()
            .map(|value| match value {
                Value::Real(x) => format!("R{:016x}", x.to_bits()),
                other => format!("{other:?}"),
            })
            .collect();
        format!(
            "{values:?}|{:?}|{:016x}|{:?}|{}",
            self.state.words,
            self.state.t.to_bits(),
            self.prev_t.map(f64::to_bits),
            self.sequence
        )
    }
}

fn real(x: f64) -> Value {
    Value::Real(x)
}

const ADMIT: NonFiniteInputPolicy = NonFiniteInputPolicy::Admit;
const REFUSE: NonFiniteInputPolicy = NonFiniteInputPolicy::Refuse;

#[test]
fn an_accepted_frame_stages_every_target_evaluates_once_and_advances_clock_and_sequence() {
    let mut rig = rig();
    let sink = CapturingDiagnostics::default();
    assert_eq!(
        rig.execute(2.0, &[real(3.0), real(-0.5)], ADMIT, &sink),
        Ok(1)
    );
    for target in &rig.slots[0].0 {
        assert!(rig.state.values[target.0 as usize].bit_eq(&real(3.0)));
    }
    assert!(rig.state.values[rig.sum_out.0 as usize].bit_eq(&real(2.5)));
    assert_eq!(rig.prev_t.map(f64::to_bits), Some(2.0_f64.to_bits()));
    assert_eq!(rig.state.t.to_bits(), 2.0_f64.to_bits());
    // The sink is the caller's: one warning per evaluation, stamped with the frame time.
    assert_eq!(sink.events.borrow().len(), 1);
    assert_eq!(sink.events.borrow()[0].2.to_bits(), 2.0_f64.to_bits());
    // The delay emits its seed on the first sample, then the latched input on the next one.
    assert!(rig.state.values[rig.delay_out.0 as usize].bit_eq(&real(0.0)));
    assert_eq!(
        rig.execute(3.0, &[real(4.0), real(0.0)], ADMIT, &sink),
        Ok(2)
    );
    assert!(rig.state.values[rig.delay_out.0 as usize].bit_eq(&real(3.0)));
    assert_eq!(sink.events.borrow().len(), 2);
}

#[test]
fn an_equal_timestamp_is_another_transition() {
    let mut rig = rig();
    let sink = NoopDiagnostics;
    rig.execute(5.0, &[real(1.0), real(0.0)], ADMIT, &sink)
        .unwrap();
    assert!(rig.state.values[rig.sum_out.0 as usize].bit_eq(&real(1.0)));
    // Same timestamp, new inputs: staged and evaluated again, and the sequence advances.
    assert_eq!(
        rig.execute(5.0, &[real(2.0), real(0.5)], ADMIT, &sink),
        Ok(2)
    );
    assert!(rig.state.values[rig.sum_out.0 as usize].bit_eq(&real(2.5)));
    assert_eq!(
        rig.execute(5.0, &[real(2.0), real(0.5)], ADMIT, &sink),
        Ok(3)
    );
    assert_eq!(rig.prev_t.map(f64::to_bits), Some(5.0_f64.to_bits()));
    assert_eq!(rig.state.t.to_bits(), 5.0_f64.to_bits());
}

/// One refusal case: how to set the rig up, the frame, the policy and the expected refusal.
struct Case {
    name: &'static str,
    setup: fn(&mut Rig),
    time: f64,
    values: Vec<Value>,
    policy: NonFiniteInputPolicy,
    expected: FrameRefusal,
}

fn advanced(rig: &mut Rig) {
    rig.execute(4.0, &[real(1.0), real(1.0)], ADMIT, &NoopDiagnostics)
        .unwrap();
}

fn cases() -> Vec<Case> {
    let good = || vec![real(1.0), real(1.0)];
    let nan = f64::from_bits(0xfff8_0000_0000_0001);
    vec![
        Case {
            name: "nan time",
            setup: advanced,
            time: f64::NAN,
            values: good(),
            policy: ADMIT,
            expected: FrameRefusal::NonFiniteTime { now: f64::NAN },
        },
        Case {
            name: "infinite time beats a bad input",
            setup: advanced,
            time: f64::INFINITY,
            values: vec![Value::Boolean(true)],
            policy: REFUSE,
            expected: FrameRefusal::NonFiniteTime { now: f64::INFINITY },
        },
        Case {
            name: "out of range",
            setup: advanced,
            time: -f64::MAX,
            values: good(),
            policy: ADMIT,
            expected: FrameRefusal::TimeOutOfRange {
                now: -f64::MAX,
                limit: MAX_FRAME_TIME_MAGNITUDE,
            },
        },
        Case {
            name: "regression",
            setup: advanced,
            time: 3.5,
            values: good(),
            policy: ADMIT,
            expected: FrameRefusal::TimeRegression {
                now: 3.5,
                prev: 4.0,
            },
        },
        Case {
            name: "overflowing step from a clock set elsewhere",
            setup: |rig| rig.prev_t = Some(-f64::MAX),
            time: MAX_FRAME_TIME_MAGNITUDE,
            values: good(),
            policy: ADMIT,
            expected: FrameRefusal::NonFiniteTimeStep {
                now: MAX_FRAME_TIME_MAGNITUDE,
                prev: -f64::MAX,
            },
        },
        Case {
            name: "unrepresentable by the sampled delay",
            setup: advanced,
            time: 1.0e305,
            values: good(),
            policy: ADMIT,
            expected: FrameRefusal::ModelTimeUnrepresentable { now: 1.0e305 },
        },
        Case {
            name: "short slice",
            setup: advanced,
            time: 5.0,
            values: vec![real(1.0)],
            policy: ADMIT,
            expected: FrameRefusal::InputCount {
                expected: 2,
                found: 1,
            },
        },
        Case {
            name: "long slice",
            setup: advanced,
            time: 5.0,
            values: vec![real(1.0), real(1.0), real(1.0)],
            policy: ADMIT,
            expected: FrameRefusal::InputCount {
                expected: 2,
                found: 3,
            },
        },
        Case {
            name: "type",
            setup: advanced,
            time: 5.0,
            values: vec![real(1.0), Value::Integer(1)],
            policy: ADMIT,
            expected: FrameRefusal::InputType { slot: 1 },
        },
        Case {
            name: "slot order: slot 0's domain before slot 1's type",
            setup: advanced,
            time: 5.0,
            values: vec![real(10.5), Value::Boolean(false)],
            policy: ADMIT,
            expected: FrameRefusal::InputDomain { slot: 0 },
        },
        Case {
            name: "non-finite under Refuse on an unbounded slot",
            setup: advanced,
            time: 5.0,
            values: vec![real(1.0), real(f64::NEG_INFINITY)],
            policy: REFUSE,
            expected: FrameRefusal::InputNonFinite { slot: 1 },
        },
        Case {
            name: "non-finite before domain under Refuse on a bounded slot",
            setup: advanced,
            time: 5.0,
            values: vec![real(nan), real(1.0)],
            policy: REFUSE,
            expected: FrameRefusal::InputNonFinite { slot: 0 },
        },
        Case {
            name: "bounded NaN under Admit is a domain refusal",
            setup: advanced,
            time: 5.0,
            values: vec![real(nan), real(1.0)],
            policy: ADMIT,
            expected: FrameRefusal::InputDomain { slot: 0 },
        },
        Case {
            name: "type before non-finite",
            setup: advanced,
            time: 5.0,
            values: vec![Value::Integer(0), real(f64::NAN)],
            policy: REFUSE,
            expected: FrameRefusal::InputType { slot: 0 },
        },
        Case {
            name: "sequence exhausted",
            setup: |rig| rig.sequence = u64::MAX,
            time: 5.0,
            values: good(),
            policy: ADMIT,
            expected: FrameRefusal::SequenceExhausted,
        },
        Case {
            name: "an input refusal beats exhaustion",
            setup: |rig| rig.sequence = u64::MAX,
            time: 5.0,
            values: vec![real(11.0), real(0.0)],
            policy: ADMIT,
            expected: FrameRefusal::InputDomain { slot: 0 },
        },
    ]
}

/// `FrameRefusal` derives `PartialEq` over `f64`, so a NaN time never equals itself; compare bits.
fn same_refusal(a: &FrameRefusal, b: &FrameRefusal) -> bool {
    match (a, b) {
        (FrameRefusal::NonFiniteTime { now: x }, FrameRefusal::NonFiniteTime { now: y }) => {
            x.to_bits() == y.to_bits()
        }
        _ => a == b,
    }
}

#[test]
fn every_refusal_comes_in_the_documented_order_and_precedes_any_mutation() {
    for case in cases() {
        let mut rig = rig();
        (case.setup)(&mut rig);
        let before = rig.image();
        let sink = CapturingDiagnostics::default();
        let refusal = rig
            .execute(case.time, &case.values, case.policy, &sink)
            .expect_err(case.name);
        assert!(
            same_refusal(&refusal, &case.expected),
            "{}: {refusal:?}",
            case.name
        );
        assert_eq!(rig.image(), before, "{}", case.name);
        assert!(sink.events.borrow().is_empty(), "{}: evaluated", case.name);
    }
}

#[test]
fn the_last_sequence_position_is_accepted_once() {
    let mut rig = rig();
    rig.sequence = u64::MAX - 1;
    let frame = [real(0.0), real(0.0)];
    assert_eq!(
        rig.execute(0.0, &frame, ADMIT, &NoopDiagnostics),
        Ok(u64::MAX)
    );
    assert_eq!(
        rig.execute(0.0, &frame, ADMIT, &NoopDiagnostics),
        Err(FrameRefusal::SequenceExhausted)
    );
}

#[test]
fn the_time_magnitude_limit_is_inclusive_and_the_next_float_is_refused() {
    let mut b = ModelBuilder::default();
    b.block_real(make("CDL.Reals.Add", &[]));
    let state = allocate_state(&b.model, &b.blocks).unwrap();
    for limit in [MAX_FRAME_TIME_MAGNITUDE, -MAX_FRAME_TIME_MAGNITUDE] {
        assert_eq!(check_time(&b.blocks, &state, None, limit), Ok(()));
        let past = f64::from_bits(limit.to_bits() + 1);
        assert_eq!(
            check_time(&b.blocks, &state, None, past),
            Err(FrameRefusal::TimeOutOfRange {
                now: past,
                limit: MAX_FRAME_TIME_MAGNITUDE
            })
        );
    }
    assert_eq!(
        MAX_FRAME_TIME_MAGNITUDE.to_bits(),
        (f64::MAX / 4.0).to_bits()
    );
    // Equal time, signed zero against signed zero, is not a regression.
    assert_eq!(check_time(&b.blocks, &state, Some(0.0), -0.0), Ok(()));
}

#[test]
fn the_domain_rule_holds_at_its_edges() {
    let zero = real(0.0);
    // Signed zero compares equal to zero, on both bounds.
    assert!(accepts_domain(&real(-0.0), Some(&zero), Some(&zero)));
    // A NaN bound accepts nothing; a NaN value fails any present bound and passes none.
    assert!(!accepts_domain(&real(1.0), Some(&real(f64::NAN)), None));
    assert!(!accepts_domain(&real(f64::NAN), None, Some(&real(1.0))));
    assert!(accepts_domain(&real(f64::NAN), None, None));
    assert!(accepts_domain(&real(f64::INFINITY), None, None));
    assert!(!accepts_domain(
        &real(f64::INFINITY),
        None,
        Some(&real(f64::MAX))
    ));
    // A bound of another type accepts nothing.
    assert!(!accepts_domain(&real(1.0), Some(&Value::Integer(0)), None));
    // Integers compare exactly above 2^53.
    let big = (1_i64 << 53) + 1;
    assert!(!accepts_domain(
        &Value::Integer(big),
        None,
        Some(&Value::Integer(big - 1))
    ));
    assert!(accepts_domain(
        &Value::Integer(big),
        Some(&Value::Integer(big)),
        Some(&Value::Integer(big))
    ));
    // Enum ordinals are 1-based and bounded by the class; an unknown class accepts nothing.
    let class = EnumClassId::SIMPLE_CONTROLLER;
    let members = oce_model::enum_descriptor(class).unwrap().members.len() as u32;
    let ordinal = |ordinal| Value::Enum { class, ordinal };
    assert!(!accepts_domain(&ordinal(0), None, None));
    assert!(accepts_domain(&ordinal(1), None, None));
    assert!(accepts_domain(&ordinal(members), None, None));
    assert!(!accepts_domain(&ordinal(members + 1), None, None));
    let unknown = Value::Enum {
        class: EnumClassId(u32::MAX),
        ordinal: 1,
    };
    assert!(!accepts_domain(&unknown, None, None));
    // Booleans and Strings have no domain.
    assert!(accepts_domain(
        &Value::Boolean(true),
        Some(&zero),
        Some(&zero)
    ));
}

#[test]
fn malformed_slots_and_targets_refuse_instead_of_panicking() {
    let mut rig = rig();
    let outside = ConnectorId(u32::MAX);
    assert!(!accepts_type(&rig.model, &[outside], &real(0.0)));
    assert!(accepts_type(&rig.model, &[], &real(0.0)));
    rig.slots[1].0.push(outside);
    assert_eq!(
        rig.execute(0.0, &[real(0.0), real(0.0)], ADMIT, &NoopDiagnostics),
        Err(FrameRefusal::InputType { slot: 1 })
    );
    let mut state = rig.state.clone();
    state.slots[0].offset = usize::MAX;
    assert!(!time_is_representable(&rig.blocks, &state, 0.0));
    let mut state = rig.state.clone();
    state.slots[0].block = BlockId(u32::MAX);
    assert!(!time_is_representable(&rig.blocks, &state, 0.0));
}
