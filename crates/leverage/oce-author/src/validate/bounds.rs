//! Boundary bounds (importer note, "Bounds on boundary declarations" and decision 6; issue #492).
//!
//! A bound is a number literal on a Real or Integer boundary. Stage 1 refuses, per bound: a
//! non-finite Real (`non-finite-bound`, the engine's code); a bound on a Boolean boundary, a Real
//! literal on an Integer boundary, or an Integer literal on a Real boundary above 2^53 in magnitude
//! (no exact Real) (`invalid-bound`, author-only, pre-empting the engine's `malformed-document`).
//! An Integer bound on an Integer boundary outside the engine's `i32` Integer domain is refused
//! as `invalid-bound` too: the engine narrows such a bound to `i32` without a warning, so the
//! loaded domain would differ from the declared one (decision 6, amended 2026-11-09, #493).
//! Then, when both bounds pass, a crossed pair, `min > max` (`crossed-bounds`, the engine's
//! code). `bound-mismatch` against a connected port stays a stage-2 engine code.

use crate::model::{Bound, Boundary, SignalType};

use super::names::{MAX_EXACT_INTEGER, exact_as_real};
use super::{CROSSED_BOUNDS, INVALID_BOUND, NON_FINITE_BOUND};

/// What is wrong with one bound of `boundary`, if anything.
fn bound_problem(boundary: &Boundary, side: &str, bound: Bound) -> Option<(&'static str, String)> {
    let name = &boundary.name;
    let kind = boundary.value_type.as_str();
    match (boundary.value_type, bound) {
        (_, Bound::Real(value)) if !value.is_finite() => Some((
            NON_FINITE_BOUND,
            format!("`{side}` of `{name}` is {value}; a bound is finite"),
        )),
        (SignalType::Real, Bound::Integer(value)) if !exact_as_real(value) => Some((
            INVALID_BOUND,
            format!(
                "`{side}` of the Real boundary `{name}`: the Integer {value} exceeds 2^53 ({MAX_EXACT_INTEGER}) in magnitude, so its Real may round; write the Real you mean"
            ),
        )),
        (SignalType::Integer, Bound::Integer(value)) if i32::try_from(value).is_err() => Some((
            INVALID_BOUND,
            format!(
                "`{side}` of the Integer boundary `{name}` is {value}, outside the engine's Integer range {}..={}; the engine would narrow it",
                i32::MIN,
                i32::MAX
            ),
        )),
        (SignalType::Real, _) | (SignalType::Integer, Bound::Integer(_)) => None,
        (SignalType::Integer, Bound::Real(value)) => Some((
            INVALID_BOUND,
            format!(
                "`{side}` of the Integer boundary `{name}` is the Real {value:?}; an Integer boundary takes an Integer bound"
            ),
        )),
        _ => Some((
            INVALID_BOUND,
            format!("`{name}` is a {kind} boundary, which takes no `{side}`"),
        )),
    }
}

/// Whether `min` exceeds `max`, compared as the engine compares them (Integers exactly, Reals
/// as `f64`).
fn crossed(value_type: SignalType, min: Bound, max: Bound) -> bool {
    match (value_type, min, max) {
        (SignalType::Integer, Bound::Integer(low), Bound::Integer(high)) => low > high,
        _ => min.as_f64() > max.as_f64(),
    }
}

fn spelled(bound: Bound) -> String {
    match bound {
        Bound::Integer(value) => value.to_string(),
        Bound::Real(value) => format!("{value:?}"),
    }
}

/// Every bound problem of one boundary as `(code, detail)`, `min` before `max`, then a crossed
/// pair.
pub(crate) fn boundary_bound_problems(boundary: &Boundary) -> Vec<(&'static str, String)> {
    let sides = [("min", boundary.min), ("max", boundary.max)];
    let problems: Vec<(&'static str, String)> = sides
        .iter()
        .filter_map(|&(side, bound)| bound.and_then(|b| bound_problem(boundary, side, b)))
        .collect();
    if problems.is_empty()
        && let (Some(min), Some(max)) = (boundary.min, boundary.max)
        && crossed(boundary.value_type, min, max)
    {
        return vec![(
            CROSSED_BOUNDS,
            format!(
                "`{}` declares `min` {} above `max` {}; no value lies between them",
                boundary.name,
                spelled(min),
                spelled(max)
            ),
        )];
    }
    problems
}
