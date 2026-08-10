//! Runtime-state revisions and structural validation owned by the block library.

use oce_model::determinism::CANONICAL_NAN_BITS;

use crate::BlockKind;
use crate::state_contract_registry::{self, Validator};

const PREV_T_UNSET: u64 = u64::MAX;

pub(crate) fn revision(class_path: &str, kind: BlockKind) -> u32 {
    if kind == BlockKind::Stateful && state_contract_registry::validator(class_path).is_some() {
        1
    } else {
        0
    }
}

pub(crate) fn validate(
    class_path: &str,
    region: &[u64],
    state_t: f64,
    prev_t: f64,
) -> Result<(), String> {
    let validator = state_contract_registry::validator(class_path)
        .ok_or_else(|| "stateful class has no registered state contract".to_string())?;
    match validator {
        Validator::TriggeredSampler => {
            expect_len(region, 2)?;
            bool_word(region, 1)
        }
        Validator::TriggeredMax => {
            expect_len(region, 3)?;
            bool_word(region, 1)?;
            exact_word(region, 2, 1, "has_history")
        }
        Validator::TriggeredMovingMean => validate_triggered_moving_mean(region),
        Validator::IntegerChange => expect_len(region, 1),
        Validator::OnCounter => {
            expect_len(region, 4)?;
            bool_word(region, 1)?;
            bool_word(region, 2)?;
            exact_word(region, 3, 1, "has_history")
        }
        Validator::IntegerStage => validate_integer_stage(region, prev_t),
        Validator::LogicalEdge => {
            expect_len(region, 1)?;
            bool_word(region, 0)
        }
        Validator::LogicalLatch => {
            expect_len(region, 2)?;
            bool_word(region, 0)?;
            bool_word(region, 1)
        }
        Validator::Proof => validate_proof(region, state_t, prev_t),
        Validator::Timer => validate_timer(region, state_t, prev_t),
        Validator::TimerAccumulating => {
            expect_len(region, 5)?;
            prev_word(region, 1, prev_t)?;
            bool_word(region, 2)?;
            bool_word(region, 3)?;
            bool_word(region, 4)
        }
        Validator::TrueDelay => {
            expect_len(region, 4)?;
            prev_word(region, 1, prev_t)?;
            bool_word(region, 2)?;
            bool_word(region, 3)
        }
        Validator::TrueHold => {
            expect_len(region, 3)?;
            bool_word(region, 0)?;
            prev_word(region, 2, prev_t)
        }
        Validator::VariablePulse => {
            expect_len(region, 5)?;
            finite_not_after(region, 1, state_t, "cycle_t0")?;
            bool_word(region, 2)?;
            prev_word(region, 4, prev_t)
        }
        Validator::Comparator => {
            expect_len(region, 1)?;
            bool_word(region, 0)
        }
        Validator::Integrator => {
            expect_len(region, 3)?;
            prev_word(region, 1, prev_t)?;
            bool_word(region, 2)
        }
        Validator::Derivative => {
            expect_len(region, 3)?;
            prev_word(region, 1, prev_t)?;
            bool_word(region, 2)
        }
        Validator::LimitSlewRate => {
            expect_len(region, 2)?;
            prev_word(region, 1, prev_t)
        }
        Validator::MovingAverage => validate_moving_average(region, state_t, prev_t),
        Validator::Ramp => {
            expect_len(region, 3)?;
            bool_word(region, 1)?;
            prev_word(region, 2, prev_t)
        }
        Validator::SunRiseSet => validate_sun_rise_set(region),
        Validator::Sampled | Validator::SampleTrigger | Validator::Pid => {
            Err("state validator requires resolved block data".into())
        }
    }
}

pub(crate) fn validate_sampled(
    region: &[u64],
    state_t: f64,
    period: f64,
    first_order: bool,
    unit_delay: bool,
) -> Result<(), String> {
    let (initialized, t0, last, sample_time) = if first_order {
        expect_len(region, 7)?;
        (0, 1, 2, Some(3))
    } else if unit_delay {
        expect_len(region, 5)?;
        (4, 2, 3, None)
    } else {
        expect_len(region, 4)?;
        (3, 1, 2, None)
    };
    exact_word(region, initialized, 1, "initialized")?;
    let t0_value = finite(region, t0, "sample origin")?;
    if !rounded_grid_origin(t0_value, period) {
        return Err("sample origin is not on the rounded period grid".into());
    }
    if t0_value > state_t {
        let quotient = (state_t / period).floor();
        let product = quotient * period;
        if !quotient.is_finite()
            || !product.is_finite()
            || round_six(product).to_bits() != region[t0]
        {
            return Err("sample origin is later than its rounded grid origin".into());
        }
    }
    let expected = sampled_index(state_t, t0_value, period)
        .ok_or_else(|| "sample index is not representable".to_string())?;
    let stored = region[last].cast_signed();
    if stored != expected {
        return Err(format!(
            "sample index {stored} does not match model time ({expected})"
        ));
    }
    if let Some(sample_time) = sample_time {
        let sample = finite(region, sample_time, "sample time")?;
        if region[sample_time] == region[t0] {
        } else if sampled_index(sample, t0_value, period) != Some(stored) {
            return Err("sample time does not belong to the stored sample index".into());
        }
        if sample > state_t && region[sample_time] != region[t0] {
            return Err("sample time is later than model time".into());
        }
    }
    Ok(())
}

pub(crate) fn validate_sample_trigger(
    region: &[u64],
    state_t: f64,
    period: f64,
    shift: f64,
) -> Result<(), String> {
    expect_len(region, 1)?;
    let expected = sample_trigger_index(state_t, period, shift)
        .ok_or_else(|| "sample index is not representable".to_string())?
        .max(-1);
    if region[0].cast_signed() != expected {
        return Err(format!(
            "sample index {} does not match model time ({expected})",
            region[0].cast_signed()
        ));
    }
    Ok(())
}

pub(crate) fn validate_pid(
    region: &[u64],
    prev_t: f64,
    prev_t_index: Option<usize>,
    trigger_index: Option<usize>,
) -> Result<(), String> {
    let Some(prev_t_index) = prev_t_index else {
        return if region.is_empty() {
            Ok(())
        } else {
            Err("algebraic controller carries state words".into())
        };
    };
    prev_word(region, prev_t_index, prev_t)?;
    if let Some(trigger_index) = trigger_index {
        bool_word(region, trigger_index)?;
    }
    Ok(())
}

pub(crate) fn sampled_time_representable(
    t_now: f64,
    region: &[u64],
    period: f64,
    initialized_word: usize,
    t0_word: usize,
) -> bool {
    let Some(&initialized) = region.get(initialized_word) else {
        return false;
    };
    if initialized > 1 || !period.is_finite() || period <= 0.0 {
        return false;
    }
    let t0 = if initialized == 0 {
        let q = (t_now / period).floor();
        let r = q * period;
        let rounded = round_six(r);
        if !q.is_finite() || !r.is_finite() || !rounded.is_finite() {
            return false;
        }
        rounded
    } else {
        let Some(&bits) = region.get(t0_word) else {
            return false;
        };
        let value = f64::from_bits(bits);
        if !value.is_finite() {
            return false;
        }
        value
    };
    sampled_index(t_now, t0, period).is_some()
}

pub(crate) fn sampled_horizon_representable(first: f64, last: f64, period: f64) -> bool {
    if !period.is_finite() || period <= 0.0 {
        return false;
    }
    let quotient = (first / period).floor();
    let product = quotient * period;
    let t0 = round_six(product);
    quotient.is_finite()
        && product.is_finite()
        && t0.is_finite()
        && sampled_index(first, t0, period).is_some()
        && sampled_index(last, t0, period).is_some()
}

pub(crate) fn sample_trigger_time_representable(t_now: f64, period: f64, shift: f64) -> bool {
    sample_trigger_index(t_now, period, shift).is_some()
}

pub(crate) fn sample_trigger_horizon_representable(
    first: f64,
    last: f64,
    period: f64,
    shift: f64,
) -> bool {
    sample_trigger_index(first, period, shift).is_some()
        && sample_trigger_index(last, period, shift).is_some()
}

fn sampled_index(t_now: f64, t0: f64, period: f64) -> Option<i64> {
    if !period.is_finite() || period <= 0.0 || !t0.is_finite() {
        return None;
    }
    checked_index(((t_now - t0) / period + 1e-9).floor())
}

fn rounded_grid_origin(t0: f64, period: f64) -> bool {
    if !period.is_finite() || period <= 0.0 || !t0.is_finite() {
        return false;
    }
    let quotient = (t0 / period).round();
    let product = quotient * period;
    quotient.is_finite() && product.is_finite() && round_six(product).to_bits() == t0.to_bits()
}

fn sample_trigger_index(t_now: f64, period: f64, shift: f64) -> Option<i64> {
    if !period.is_finite() || period <= 0.0 || !shift.is_finite() {
        return None;
    }
    let quotient = (shift / period).floor();
    let product = quotient * period;
    let phase = shift - product;
    if !quotient.is_finite() || !product.is_finite() || !phase.is_finite() {
        return None;
    }
    checked_index(((t_now - phase) / period + 1e-9).floor())
}

fn checked_index(value: f64) -> Option<i64> {
    const I64_MIN_F64: f64 = -9_223_372_036_854_775_808.0;
    const I64_END_F64: f64 = 9_223_372_036_854_775_808.0;
    (value.is_finite() && (I64_MIN_F64..I64_END_F64).contains(&value)).then_some(value as i64)
}

fn round_six(value: f64) -> f64 {
    const FACTOR: f64 = 1_000_000.0;
    if value > 0.0 {
        (value * FACTOR + 0.5).floor() / FACTOR
    } else {
        (value * FACTOR - 0.5).ceil() / FACTOR
    }
}

fn validate_triggered_moving_mean(region: &[u64]) -> Result<(), String> {
    if region.len() < 5 {
        return Err("state length is smaller than 4+n with n >= 1".into());
    }
    bool_word(region, 1)?;
    let n = region.len() - 4;
    let count = usize::try_from(region[2]).map_err(|_| "sample count exceeds usize".to_string())?;
    let next = usize::try_from(region[3]).map_err(|_| "next index exceeds usize".to_string())?;
    if !(1..=n).contains(&count) || next >= n {
        return Err(format!(
            "ring count/index out of range: count={count}, next={next}, n={n}"
        ));
    }
    if count < n {
        if next != count {
            return Err("partially filled ring must append at count".into());
        }
        if region[4 + count..]
            .iter()
            .any(|word| *word != 0.0f64.to_bits())
        {
            return Err("unoccupied sample word is not positive zero".into());
        }
    }
    Ok(())
}

fn validate_integer_stage(region: &[u64], prev_t: f64) -> Result<(), String> {
    expect_len(region, 8)?;
    let t_next = f64::from_bits(region[1]);
    if t_next.is_nan() || (t_next.is_infinite() && t_next.is_sign_negative()) {
        return Err("next-event time must be finite or positive infinity".into());
    }
    finite(region, 2, "upper threshold")?;
    finite(region, 3, "lower threshold")?;
    bool_word(region, 4)?;
    bool_word(region, 5)?;
    bool_word(region, 6)?;
    prev_word(region, 7, prev_t)
}

fn validate_timer(region: &[u64], state_t: f64, prev_t: f64) -> Result<(), String> {
    expect_len(region, 4)?;
    let entry = finite(region, 0, "entry time")?;
    prev_word(region, 1, prev_t)?;
    bool_word(region, 2)?;
    bool_word(region, 3)?;
    if region[2] == 1 && entry > state_t {
        return Err("active timer entry time is later than model time".into());
    }
    Ok(())
}

fn validate_proof(region: &[u64], state_t: f64, prev_t: f64) -> Result<(), String> {
    expect_len(region, 24)?;
    for base in [0, 4, 8, 12] {
        prev_word(region, base + 1, prev_t)?;
        bool_word(region, base + 2)?;
        bool_word(region, base + 3)?;
    }
    let entry = finite(region, 16, "proof timer entry")?;
    prev_word(region, 17, prev_t)?;
    bool_word(region, 18)?;
    if region[18] == 1 && entry > state_t {
        return Err("active proof timer entry is later than model time".into());
    }
    for index in 19..24 {
        bool_word(region, index)?;
    }
    Ok(())
}

fn validate_moving_average(region: &[u64], state_t: f64, prev_t: f64) -> Result<(), String> {
    expect_len(region, 134)?;
    finite_not_after(region, 1, state_t, "start time")?;
    prev_word(region, 2, prev_t)?;
    let head = usize::try_from(region[3]).map_err(|_| "ring head exceeds usize".to_string())?;
    let len = usize::try_from(region[4]).map_err(|_| "ring length exceeds usize".to_string())?;
    bool_word(region, 5)?;
    if head >= 64 || !(1..=64).contains(&len) {
        return Err(format!(
            "ring head/length out of range: head={head}, len={len}"
        ));
    }
    let mut prior = None;
    for logical in 0..len {
        let physical = (head + logical) % 64;
        let time = finite(region, 6 + physical, "history time")?;
        if time > state_t || prior.is_some_and(|prior| time < prior) {
            return Err("history times must be ordered and not later than model time".into());
        }
        prior = Some(time);
    }
    let last = (head + len - 1) % 64;
    if region[6 + last] != prev_t.to_bits() {
        return Err("last history time differs from previous tick time".into());
    }
    Ok(())
}

fn validate_sun_rise_set(region: &[u64]) -> Result<(), String> {
    expect_len(region, 5)?;
    exact_word(region, 3, 1, "initialized")?;
    bool_word(region, 4)?;
    let rise = f64::from_bits(region[0]);
    let set = f64::from_bits(region[1]);
    let start = f64::from_bits(region[2]);
    if rise.is_finite() && set.is_finite() && start.is_finite() {
        return Ok(());
    }
    if region[0] == CANONICAL_NAN_BITS
        && region[1] == CANONICAL_NAN_BITS
        && region[2] == 0.0f64.to_bits()
        && region[4] == 1
    {
        return Ok(());
    }
    Err("solar event state is neither finite nor the canonical invalid-state tuple".into())
}

fn expect_len(region: &[u64], expected: usize) -> Result<(), String> {
    if region.len() == expected {
        Ok(())
    } else {
        Err(format!(
            "state length is {}, expected {expected}",
            region.len()
        ))
    }
}

fn exact_word(region: &[u64], index: usize, expected: u64, name: &str) -> Result<(), String> {
    match region.get(index) {
        Some(actual) if *actual == expected => Ok(()),
        Some(actual) => Err(format!("{name} word is {actual}, expected {expected}")),
        None => Err(format!("{name} word is missing")),
    }
}

fn bool_word(region: &[u64], index: usize) -> Result<(), String> {
    match region.get(index) {
        Some(0 | 1) => Ok(()),
        Some(value) => Err(format!("word {index} is not a canonical Boolean: {value}")),
        None => Err(format!("Boolean word {index} is missing")),
    }
}

fn finite(region: &[u64], index: usize, name: &str) -> Result<f64, String> {
    let bits = *region
        .get(index)
        .ok_or_else(|| format!("{name} word is missing"))?;
    let value = f64::from_bits(bits);
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("{name} is not finite"))
    }
}

fn finite_not_after(region: &[u64], index: usize, state_t: f64, name: &str) -> Result<(), String> {
    let value = finite(region, index, name)?;
    if value <= state_t {
        Ok(())
    } else {
        Err(format!("{name} is later than model time"))
    }
}

fn prev_word(region: &[u64], index: usize, prev_t: f64) -> Result<(), String> {
    let word = *region
        .get(index)
        .ok_or_else(|| "previous-time word is missing".to_string())?;
    if word == prev_t.to_bits() {
        Ok(())
    } else if word == PREV_T_UNSET {
        Err("previous-time word is still unset after a completed tick".into())
    } else {
        Err("previous-time word differs from engine previous time".into())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use oce_model::{ParamTable, Value};

    use super::*;
    use crate::{Ctx, NoopDiagnostics, PortKind};

    #[test]
    fn revision_table_is_sorted_unique_complete_and_registry_backed() {
        let class_paths = crate::state_contract_registry::class_paths().collect::<Vec<_>>();
        assert_eq!(class_paths.len(), 37);
        assert!(class_paths.windows(2).all(|pair| pair[0] < pair[1]));
        for class_path in class_paths {
            assert!(crate::lookup(class_path).is_some(), "missing {class_path}");
        }
        for entry in crate::catalog() {
            let block = (crate::lookup(entry.class_path).unwrap().make)(&ParamTable::default());
            if block.kind() == BlockKind::Stateful {
                assert_eq!(
                    block.state_contract_revision(),
                    1,
                    "{} is stateful without a revision",
                    entry.class_path
                );
            } else {
                assert_eq!(block.state_contract_revision(), 0, "{}", entry.class_path);
            }
        }
    }

    #[test]
    fn parameter_resolved_stateful_variants_have_revision_one() {
        let hysteresis = ParamTable {
            values: vec![(Arc::from("h"), Value::Real(0.1))],
        };
        for class_path in [
            "CDL.Reals.Greater",
            "CDL.Reals.GreaterThreshold",
            "CDL.Reals.Less",
            "CDL.Reals.LessThreshold",
        ] {
            let block = (crate::lookup(class_path).unwrap().make)(&hysteresis);
            assert_eq!(block.kind(), BlockKind::Stateful, "{class_path}");
            assert_eq!(block.state_contract_revision(), 1, "{class_path}");
        }

        for controller_type in [2, 3, 4] {
            let params = ParamTable {
                values: vec![(Arc::from("controllerType"), Value::Integer(controller_type))],
            };
            for class_path in ["CDL.Reals.PID", "CDL.Reals.PIDWithReset"] {
                let block = (crate::lookup(class_path).unwrap().make)(&params);
                assert_eq!(block.kind(), BlockKind::Stateful, "{class_path}");
                assert_eq!(block.state_contract_revision(), 1, "{class_path}");
            }
        }
    }

    #[test]
    fn sampled_index_bounds_do_not_saturate() {
        assert_eq!(checked_index(-9_223_372_036_854_775_808.0), Some(i64::MIN));
        assert_eq!(
            checked_index(9_223_372_036_854_773_760.0),
            Some(i64::MAX - 2047)
        );
        assert_eq!(checked_index(9_223_372_036_854_775_808.0), None);
        assert_eq!(checked_index(f64::INFINITY), None);
        assert_eq!(checked_index(f64::NAN), None);
    }

    #[test]
    fn every_default_stateful_registry_instance_validates_after_one_tick() {
        for entry in crate::catalog() {
            let params = ParamTable::default();
            let block = (crate::lookup(entry.class_path).unwrap().make)(&params);
            if block.kind() != BlockKind::Stateful {
                continue;
            }
            let signature = block.resolved_signature();
            let inputs = signature
                .inputs
                .iter()
                .map(|kind| match kind {
                    PortKind::Real => Value::Real(0.0),
                    PortKind::Integer => Value::Integer(0),
                    PortKind::Boolean => Value::Boolean(false),
                })
                .collect::<Vec<_>>();
            let mut region = vec![0; block.state_len()];
            block.init_state(&mut region, &params);
            let context = Ctx::new(0.0, &NoopDiagnostics);
            block.emit_from_state(&context, &inputs, &region, &mut |_, _| {});
            block.update_state(&context, &inputs, &mut region);
            block
                .validate_state(&region, 0.0, 0.0)
                .unwrap_or_else(|error| panic!("{}: {error}", entry.class_path));
        }
    }

    #[test]
    fn structural_state_roles_reject_noncanonical_words() {
        let cases = [
            ("CDL.Discrete.FirstOrderHold", 0, 2),
            ("CDL.Discrete.Sampler", 3, 2),
            ("CDL.Discrete.TriggeredMax", 2, 0),
            ("CDL.Discrete.TriggeredMovingMean", 2, 0),
            ("CDL.Discrete.UnitDelay", 4, 2),
            ("CDL.Integers.OnCounter", 3, 0),
            ("CDL.Integers.Stage", 7, PREV_T_UNSET),
            ("CDL.Logical.Pre", 0, 2),
            ("CDL.Logical.Latch", 0, 2),
            ("CDL.Logical.Proof", 19, 2),
            (
                "CDL.Logical.Sources.SampleTrigger",
                0,
                i64::MAX.cast_unsigned(),
            ),
            ("CDL.Logical.Timer", 1, PREV_T_UNSET),
            ("CDL.Logical.TimerAccumulating", 1, PREV_T_UNSET),
            ("CDL.Logical.TrueDelay", 1, PREV_T_UNSET),
            ("CDL.Logical.TrueFalseHold", 0, 2),
            ("CDL.Logical.VariablePulse", 1, f64::INFINITY.to_bits()),
            ("CDL.Reals.Greater", 0, 2),
            ("CDL.Reals.IntegratorWithReset", 1, PREV_T_UNSET),
            ("CDL.Reals.Derivative", 2, 2),
            ("CDL.Reals.LimitSlewRate", 1, PREV_T_UNSET),
            ("CDL.Reals.MovingAverage", 3, 64),
            ("CDL.Reals.PID", 1, PREV_T_UNSET),
            ("CDL.Reals.PIDWithReset", 1, PREV_T_UNSET),
            ("CDL.Reals.Ramp", 1, 2),
            ("CDL.Utilities.SunRiseSet", 3, 0),
        ];
        for (class_path, word, bad_value) in cases {
            let params = if class_path == "CDL.Reals.Greater" {
                ParamTable {
                    values: vec![(Arc::from("h"), Value::Real(0.1))],
                }
            } else {
                ParamTable::default()
            };
            let block = (crate::lookup(class_path).unwrap().make)(&params);
            assert_eq!(block.kind(), BlockKind::Stateful, "{class_path}");
            let inputs = block
                .resolved_signature()
                .inputs
                .iter()
                .map(|kind| match kind {
                    PortKind::Real => Value::Real(0.0),
                    PortKind::Integer => Value::Integer(0),
                    PortKind::Boolean => Value::Boolean(false),
                })
                .collect::<Vec<_>>();
            let mut region = vec![0; block.state_len()];
            block.init_state(&mut region, &params);
            block.update_state(&Ctx::new(0.0, &NoopDiagnostics), &inputs, &mut region);
            block.validate_state(&region, 0.0, 0.0).unwrap();
            region[word] = bad_value;
            assert!(
                block.validate_state(&region, 0.0, 0.0).is_err(),
                "{class_path} accepted corrupted word {word}"
            );
        }
    }

    #[test]
    fn time_bearing_roles_reject_future_history_and_invalid_event_state() {
        let state_t: f64 = 2.0;
        let prev_t: f64 = 2.0;

        let timer = [1.0f64.to_bits(), prev_t.to_bits(), 1, 0];
        assert!(validate_timer(&timer, state_t, prev_t).is_ok());
        let mut bad_timer = timer;
        bad_timer[0] = 3.0f64.to_bits();
        assert!(validate_timer(&bad_timer, state_t, prev_t).is_err());

        let mut proof = [0; 24];
        for index in [1, 5, 9, 13, 17] {
            proof[index] = prev_t.to_bits();
        }
        proof[16] = 1.0f64.to_bits();
        proof[18] = 1;
        assert!(validate_proof(&proof, state_t, prev_t).is_ok());
        proof[16] = 3.0f64.to_bits();
        assert!(validate_proof(&proof, state_t, prev_t).is_err());

        let mut moving = [0; 134];
        moving[1] = 0.0f64.to_bits();
        moving[2] = prev_t.to_bits();
        moving[4] = 2;
        moving[6] = 1.0f64.to_bits();
        moving[7] = prev_t.to_bits();
        assert!(validate_moving_average(&moving, state_t, prev_t).is_ok());
        let mut bad_order = moving;
        bad_order[2] = 1.0f64.to_bits();
        bad_order[6] = prev_t.to_bits();
        bad_order[7] = 1.0f64.to_bits();
        assert!(validate_moving_average(&bad_order, state_t, 1.0).is_err());
        let mut bad_length = moving;
        bad_length[4] = 0;
        assert!(validate_moving_average(&bad_length, state_t, prev_t).is_err());

        let variable_pulse = [0, 1.0f64.to_bits(), 0, 0, prev_t.to_bits()];
        assert!(
            validate(
                "CDL.Logical.VariablePulse",
                &variable_pulse,
                state_t,
                prev_t
            )
            .is_ok()
        );
        let mut future_cycle = variable_pulse;
        future_cycle[1] = 3.0f64.to_bits();
        assert!(validate("CDL.Logical.VariablePulse", &future_cycle, state_t, prev_t).is_err());

        let stage = [0, f64::INFINITY.to_bits(), 0, 0, 0, 0, 0, prev_t.to_bits()];
        assert!(validate_integer_stage(&stage, prev_t).is_ok());
        let mut bad_stage = stage;
        bad_stage[1] = f64::NEG_INFINITY.to_bits();
        assert!(validate_integer_stage(&bad_stage, prev_t).is_err());

        let sun = [0, 0, 0, 1, 0];
        assert!(validate_sun_rise_set(&sun).is_ok());
        let mut bad_sun = sun;
        bad_sun[0] = CANONICAL_NAN_BITS;
        assert!(validate_sun_rise_set(&bad_sun).is_err());
    }
}
