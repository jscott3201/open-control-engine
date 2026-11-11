//! Refusal parity between the slot entry and the named path, and refusal atomicity (issue #444).
//!
//! For every G36 catalog fixture, after a few accepted frames, each refusal class is submitted to
//! both entries (`support/dual_entry.rs`): every shorter arity, every other scalar type (String
//! included) in every slot,
//! NaN and both infinities in every Real slot under each non-finite policy, and every bad time
//! (NaN, both infinities, beyond `MAX_FRAME_TIME_MAGNITUDE`, before the previous frame). Both must
//! return the same error (variant, path, time) — derived independently here from the slot's
//! definition, not read back from either entry — and after each class the durable snapshot bytes
//! must equal those taken before it on the slot engine, and equal the named engine's. A good frame
//! after the refusals must then agree too. Lifecycle refusals (no load, pending edits, a map from
//! a superseded or foreign incarnation), precedence between classes, and a hostile oversized slot
//! vector (refused before any value is read, allocating nothing) close the matrix.

// Shared helpers, declared once for this binary in its root `facade_allocation_census.rs`:
// `support/blob_corpus.rs`, `support/dual_entry.rs`
// and `../../oce-graph/tests/support/frame_schedule.rs`.
use crate::blob_corpus;
use crate::dual_entry;
use crate::frame_schedule;

use allocation_counter::measure;
use blob_corpus::{FIXTURES, fixture_bytes};
use dual_entry::{DualEntry, Outcome};
use frame_schedule::{input_value, time_at};
use oce_api::{
    EngineStateError, FrameSlots, InputDefinition, MAX_FRAME_TIME_MAGNITUDE, NonFiniteInputPolicy,
    OcError, Value, ValueType,
};

const DELAY: &[u8] = include_bytes!("fixtures/frame_delay.jsonld");

/// Accepted frames before the refusals, so stateful blocks hold non-initial state.
const WARM_FRAMES: u32 = 9;

fn frame(slots: &FrameSlots, k: u32) -> Vec<Value> {
    slots
        .inputs()
        .iter()
        .enumerate()
        .map(|(slot, d)| input_value(d.value_type, d.min.as_ref(), d.max.as_ref(), slot, k))
        .collect()
}

fn debug(error: &OcError) -> String {
    format!("{error:?}")
}

/// Submit a frame both entries must refuse with `expected`.
fn refuse(dual: &mut DualEntry, when: &str, time: f64, values: &[Value], expected: &OcError) {
    match dual.step(time, values) {
        Ok(Outcome::Refused(error)) => assert_eq!(error, debug(expected), "{when}"),
        Ok(Outcome::Accepted { .. }) => panic!("{when}: accepted"),
        Err(error) => panic!("{when}: {error}"),
    }
}

/// The slot engine's snapshot bytes, the comparison base for a class of refusals.
fn snapshot(dual: &DualEntry) -> Vec<u8> {
    dual.slotted.state_snapshot().unwrap().into_bytes()
}

/// After a class of refusals: the slot engine's state is what it was, and equals the named one's.
fn unchanged(dual: &DualEntry, before: &[u8], when: &str) {
    assert_eq!(
        snapshot(dual),
        before,
        "{when}: the slot engine's state moved"
    );
    dual.compare_state()
        .unwrap_or_else(|error| panic!("{when}: {error}"));
}

/// A value of each other scalar type than `value_type`, String included. The corpus has only
/// Real, Integer and Boolean inputs (no registry block has String or Enum signal ports), so every
/// slot is refused a String too; the Enum-slot and String-slot cases run on detached private
/// probes in `src/frame_slots_tests.rs`.
fn wrong_types(value_type: ValueType) -> Vec<Value> {
    [
        Value::Real(0.0),
        Value::Integer(0),
        Value::Boolean(false),
        Value::String("not a signal".into()),
    ]
    .into_iter()
    .filter(|value| value.value_type() != value_type)
    .collect()
}

/// Whether `definition` declares a bound, so `Admit` still refuses the non-finite `x` as a domain
/// violation: any bound refuses NaN, a finite bound refuses the infinity beyond it.
fn domain_refuses(definition: &InputDefinition, x: f64) -> bool {
    // NaN compares as neither ordering, so it fails any present bound.
    let outside = |bound: &Option<Value>, fails: fn(std::cmp::Ordering) -> bool| match bound {
        Some(Value::Real(bound)) => x.partial_cmp(bound).is_none_or(fails),
        _ => false,
    };
    outside(&definition.min, std::cmp::Ordering::is_lt)
        || outside(&definition.max, std::cmp::Ordering::is_gt)
}

/// Every refusal class on one fixture, then one good frame.
fn refusal_matrix(name: &str) -> usize {
    let mut dual = DualEntry::load(&fixture_bytes(name));
    for k in 0..WARM_FRAMES {
        let values = frame(&dual.slots, k);
        dual.step(time_at(k), &values)
            .unwrap_or_else(|error| panic!("{name}: warm frame {k}: {error}"));
    }
    let time = time_at(WARM_FRAMES);
    let good = frame(&dual.slots, WARM_FRAMES);
    let definitions = dual.slots.inputs().to_vec();
    let mut refused = 0;
    let mut admitted = 0;

    // Arity: every shorter vector is missing its first absent slot, on both entries.
    let before = snapshot(&dual);
    for len in 0..good.len() {
        let expected = OcError::FrameMissingInput(definitions[len].path.clone());
        refuse(
            &mut dual,
            &format!("{name}: {len} values"),
            time,
            &good[..len],
            &expected,
        );
        refused += 1;
    }
    unchanged(&dual, &before, &format!("{name}: arity"));

    // Type: every other scalar type in every slot.
    for (slot, definition) in definitions.iter().enumerate() {
        for wrong in wrong_types(definition.value_type) {
            let mut values = good.clone();
            values[slot] = wrong;
            let expected = OcError::InputType(definition.path.clone());
            refuse(
                &mut dual,
                &format!("{name}: type of slot {slot}"),
                time,
                &values,
                &expected,
            );
            refused += 1;
        }
    }
    unchanged(&dual, &before, &format!("{name}: types"));

    // Non-finite Reals under Refuse: every Real slot, bounded or not.
    dual.set_policy(NonFiniteInputPolicy::Refuse);
    for (slot, definition) in definitions.iter().enumerate() {
        if definition.value_type != ValueType::Real {
            continue;
        }
        for x in [f64::NAN, -f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut values = good.clone();
            values[slot] = Value::Real(x);
            let expected = OcError::InputNonFinite(definition.path.clone());
            refuse(
                &mut dual,
                &format!("{name}: {x} in slot {slot}"),
                time,
                &values,
                &expected,
            );
            refused += 1;
        }
    }
    unchanged(&dual, &before, &format!("{name}: non-finite under Refuse"));

    // Time: non-finite, out of range, and before the previous accepted frame.
    let previous = time_at(WARM_FRAMES - 1);
    let times = [
        (f64::NAN, OcError::NonFiniteTime { now: f64::NAN }),
        (f64::INFINITY, OcError::NonFiniteTime { now: f64::INFINITY }),
        (
            f64::NEG_INFINITY,
            OcError::NonFiniteTime {
                now: f64::NEG_INFINITY,
            },
        ),
        (
            f64::MAX,
            OcError::TimeOutOfRange {
                now: f64::MAX,
                limit: MAX_FRAME_TIME_MAGNITUDE,
            },
        ),
        (
            -f64::MAX,
            OcError::TimeOutOfRange {
                now: -f64::MAX,
                limit: MAX_FRAME_TIME_MAGNITUDE,
            },
        ),
        (
            previous - 1.0,
            OcError::TimeRegression {
                now: previous - 1.0,
                prev: previous,
            },
        ),
        (
            -0.0,
            OcError::TimeRegression {
                now: -0.0,
                prev: previous,
            },
        ),
    ];
    for (bad, expected) in times {
        refuse(
            &mut dual,
            &format!("{name}: time {bad}"),
            bad,
            &good,
            &expected,
        );
        refused += 1;
    }
    unchanged(&dual, &before, &format!("{name}: times"));

    // Non-finite Reals under Admit: a bound refuses them as a domain violation; an unbounded slot
    // accepts them on both entries alike (the frame then commits, at the same time each).
    dual.set_policy(NonFiniteInputPolicy::Admit);
    for (slot, definition) in definitions.iter().enumerate() {
        if definition.value_type != ValueType::Real {
            continue;
        }
        for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut values = good.clone();
            values[slot] = Value::Real(x);
            let when = format!("{name}: {x} in slot {slot} under Admit");
            if domain_refuses(definition, x) {
                let before = snapshot(&dual);
                let expected = OcError::InputDomain(definition.path.clone());
                refuse(&mut dual, &when, time, &values, &expected);
                unchanged(&dual, &before, &when);
                refused += 1;
            } else {
                match dual.step(time, &values) {
                    Ok(Outcome::Accepted { .. }) => admitted += 1,
                    other => panic!("{when}: {other:?}"),
                }
            }
        }
    }

    // A good frame after all of it agrees on both entries.
    // A good frame after all of it agrees on both entries, and its absolute position counts
    // only accepted frames: the warm-up, the admitted non-finite frames and itself.
    match dual.step(time, &good) {
        Ok(Outcome::Accepted { sequence, .. }) => assert_eq!(
            sequence,
            u64::from(WARM_FRAMES) + admitted + 1,
            "{name}: refusals consumed a sequence position"
        ),
        other => panic!("{name}: the good frame after the refusals: {other:?}"),
    }
    dual.compare_state()
        .unwrap_or_else(|error| panic!("{name}: after the good frame: {error}"));
    refused
}

#[test]
fn every_refusal_class_refuses_both_entries_alike_and_leaves_state_untouched() {
    let mut refused = 0;
    for name in FIXTURES {
        refused += refusal_matrix(name);
    }
    // Seven time refusals per fixture alone; the per-slot classes add the rest.
    assert!(refused > 46 * 7 * 3, "{refused} refusals across the corpus");
}

#[test]
fn refusal_precedence_is_the_named_path_order_on_both_entries() {
    let mut dual = DualEntry::load(DELAY);
    let [a, b] = ["urn:frame:a", "urn:frame:b"].map(str::to_owned);
    assert_eq!(dual.slots.inputs()[0].path, a);
    dual.step(1.0, &[Value::Real(0.5), Value::Real(2.0)])
        .unwrap();
    let before = snapshot(&dual);
    // Time before arity, arity before type, the lower slot first, type before domain.
    let cases: [(f64, Vec<Value>, OcError); 5] = [
        (
            f64::NAN,
            vec![Value::Boolean(true)],
            OcError::NonFiniteTime { now: f64::NAN },
        ),
        (
            0.5,
            Vec::new(),
            OcError::TimeRegression {
                now: 0.5,
                prev: 1.0,
            },
        ),
        (
            2.0,
            vec![Value::Boolean(true)],
            OcError::FrameMissingInput(b.clone()),
        ),
        (
            2.0,
            vec![Value::Real(5.0), Value::Boolean(true)],
            OcError::InputDomain(a.clone()),
        ),
        (
            2.0,
            vec![Value::Integer(5), Value::Real(f64::NAN)],
            OcError::InputType(a.clone()),
        ),
    ];
    for (time, values, expected) in &cases {
        refuse(&mut dual, &format!("{expected:?}"), *time, values, expected);
    }
    // Under Refuse, the non-finite check on `b` follows `a`'s domain check, and precedes `b`'s.
    dual.set_policy(NonFiniteInputPolicy::Refuse);
    refuse(
        &mut dual,
        "domain before a later slot's non-finite value",
        2.0,
        &[Value::Real(5.0), Value::Real(f64::NAN)],
        &OcError::InputDomain(a.clone()),
    );
    refuse(
        &mut dual,
        "non-finite under Refuse",
        2.0,
        &[Value::Real(0.0), Value::Real(f64::INFINITY)],
        &OcError::InputNonFinite(b),
    );
    unchanged(&dual, &before, "precedence");
}

#[test]
fn an_oversized_slot_vector_is_refused_before_any_value_is_read() {
    let mut dual = DualEntry::load(DELAY);
    dual.step(1.0, &[Value::Real(0.5), Value::Real(2.0)])
        .unwrap();
    let before = snapshot(&dual);
    // A quarter-million cells, every one of the wrong type: only the count is ever examined.
    let hostile = vec![Value::Boolean(true); 1 << 18];
    let mut result = None;
    let census = measure(|| {
        result = Some(
            dual.slotted
                .execute_slots(&dual.slots, 2.0, &hostile)
                .map(|frame| frame.sequence()),
        );
    });
    let error = result.unwrap().unwrap_err();
    assert_eq!(
        debug(&error),
        debug(&OcError::FrameExtraInputs {
            expected: 2,
            found: 1 << 18
        })
    );
    assert_eq!(
        census.count_total, 0,
        "the refusal allocates nothing: {census:?}"
    );
    // One extra cell after two good ones is refused the same way.
    let error = dual
        .slotted
        .execute_slots(
            &dual.slots,
            2.0,
            &[Value::Real(0.5), Value::Real(2.0), Value::Real(0.0)],
        )
        .unwrap_err();
    assert_eq!(
        debug(&error),
        debug(&OcError::FrameExtraInputs {
            expected: 2,
            found: 3
        })
    );
    // A bad time still wins over the count, as it wins over every input check.
    let error = dual
        .slotted
        .execute_slots(&dual.slots, f64::NAN, &hostile)
        .unwrap_err();
    assert!(matches!(error, OcError::NonFiniteTime { .. }), "{error:?}");
    unchanged(&dual, &before, "oversized");
    // An executable with no inputs refuses even one cell.
    let mut engine = oce_api::Engine::new();
    engine
        .load_cxf(include_bytes!("fixtures/frame_pre.jsonld"))
        .unwrap();
    let slots = engine.frame_slots().unwrap();
    assert!(slots.inputs().is_empty());
    let error = engine
        .execute_slots(&slots, 0.0, &[Value::Real(0.0)])
        .unwrap_err();
    assert_eq!(
        debug(&error),
        debug(&OcError::FrameExtraInputs {
            expected: 0,
            found: 1
        })
    );
}

#[test]
fn lifecycle_refusals_match_the_named_path_and_a_map_never_rebinds_to_a_new_incarnation() {
    let good = [Value::Real(0.5), Value::Real(2.0)];
    let entries = [
        ("urn:frame:a", good[0].clone()),
        ("urn:frame:b", good[1].clone()),
    ];
    let mut engine = oce_api::Engine::new();
    engine.load_cxf(DELAY).unwrap();
    let slots = engine.frame_slots().unwrap();

    // Before any load: the empty engine refuses the map and the named path alike.
    let mut empty = oce_api::Engine::new();
    let unloaded = debug(&OcError::State(EngineStateError::NoLoadedModel));
    assert_eq!(debug(&empty.frame_slots().unwrap_err()), unloaded);
    assert_eq!(
        debug(&empty.execute_slots(&slots, 0.0, &good).unwrap_err()),
        unloaded
    );
    assert_eq!(
        debug(&empty.prepare_frame(0.0, &entries).unwrap_err()),
        unloaded
    );

    // A map from another engine of the same bytes is stale, never rebound.
    let mut other = oce_api::Engine::new();
    other.load_cxf(DELAY).unwrap();
    let stale = debug(&OcError::StalePreparedFrame);
    assert_eq!(
        debug(&other.execute_slots(&slots, 0.0, &good).unwrap_err()),
        stale
    );

    // Pending edits refuse both entries and the map; a dirty resume supersedes the map.
    engine.execute_slots(&slots, 1.0, &good).unwrap();
    let plan = engine.prepare_frame(2.0, &entries).unwrap();
    engine.halt().unwrap();
    engine
        .set_param("urn:frame:delay.samplePeriod", Value::Real(2.0))
        .unwrap();
    let pending = debug(&OcError::State(EngineStateError::PendingParameterEdits));
    assert_eq!(debug(&engine.frame_slots().unwrap_err()), pending);
    assert_eq!(
        debug(&engine.execute_slots(&slots, 2.0, &good).unwrap_err()),
        pending
    );
    engine.resume().unwrap();
    assert_eq!(
        debug(&engine.execute_slots(&slots, 2.0, &good).unwrap_err()),
        stale
    );
    assert_eq!(debug(&engine.execute_frame(plan).unwrap_err()), stale);

    // A clean resume and a failed load keep the map; a successful reload, even of identical
    // bytes, supersedes it.
    let slots = engine.frame_slots().unwrap();
    engine.halt().unwrap();
    engine.resume().unwrap();
    assert_eq!(
        engine.execute_slots(&slots, 2.0, &good).unwrap().sequence(),
        2
    );
    assert!(engine.load_cxf(b"not a document").is_err());
    assert_eq!(
        engine.execute_slots(&slots, 3.0, &good).unwrap().sequence(),
        3
    );
    let before = engine.state_snapshot().unwrap();
    engine.load_cxf(DELAY).unwrap();
    let reloaded = engine.state_snapshot().unwrap();
    assert_eq!(
        debug(&engine.execute_slots(&slots, 0.0, &good).unwrap_err()),
        stale
    );
    assert_eq!(
        reloaded.as_bytes(),
        engine.state_snapshot().unwrap().as_bytes()
    );
    assert_ne!(before.as_bytes(), reloaded.as_bytes());
    // The sequence continues across the reload, as on the named path.
    let slots = engine.frame_slots().unwrap();
    assert_eq!(
        engine.execute_slots(&slots, 0.0, &good).unwrap().sequence(),
        4
    );
}

#[test]
fn a_first_frame_at_negative_zero_keeps_its_sign_bit_through_both_entries() {
    let mut dual = DualEntry::load(DELAY);
    let values = [Value::Real(-0.0), Value::Real(2.0)];
    match dual.step(-0.0, &values) {
        Ok(Outcome::Accepted { sequence, .. }) => assert_eq!(sequence, 1),
        other => panic!("{other:?}"),
    }
    let mut fresh = oce_api::Engine::new();
    fresh.load_cxf(DELAY).unwrap();
    let slots = fresh.frame_slots().unwrap();
    let frame = fresh.execute_slots(&slots, -0.0, &values).unwrap();
    assert_eq!(frame.time().to_bits(), (-0.0_f64).to_bits());
    assert_eq!(frame.sequence(), 1);
    // Equal time, positive zero, is not a regression: -0.0 == 0.0.
    let frame = fresh.execute_slots(&slots, 0.0, &values).unwrap();
    assert_eq!(frame.time().to_bits(), 0.0_f64.to_bits());
    dual.compare_state().unwrap();
}
