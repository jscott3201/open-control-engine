//! Sequence-level checks: the sequence shape, declared names and their uniqueness, `extends`,
//! sequence parameter values and boundary semantic slots.

use std::collections::BTreeMap;

use crate::model::{Literal, Sequence, SignalType};

use super::semantic::boundary_problems;
use super::{
    Anchor, DUPLICATE_NAME, EMPTY_SEQUENCE, Findings, INVALID_NAME, INVALID_PROVENANCE_ID,
    INVALID_SEMANTIC_POINT, NON_FINITE_PARAMETER, PARAMETER_KIND_MISMATCH, PARAMETER_OUT_OF_RANGE,
};

/// The largest magnitude up to which every Integer has an exact Real: 2^53. Above it some do not
/// (2^53 + 1 rounds to 2^53), so an Integer literal on a Real parameter may load as a value the
/// author never wrote; stage 1 refuses every one above the bound
/// (`parameter-out-of-range`).
pub const MAX_EXACT_INTEGER: i64 = 1 << 53;

/// Whether an Integer literal converts to a Real without rounding (|v| <= 2^53).
pub(crate) fn exact_as_real(value: i64) -> bool {
    value.unsigned_abs() <= MAX_EXACT_INTEGER.unsigned_abs()
}

/// The detail of an inexact Integer-to-Real conversion.
pub(crate) fn inexact_detail(what: &str, value: i64) -> String {
    format!(
        "{what}: the Integer {value} on a Real exceeds 2^53 ({MAX_EXACT_INTEGER}) in magnitude, \
         so its Real may round; write the Real you mean"
    )
}

/// Whether a literal fits a sequence parameter's declared type. A Real accepts an Integer
/// literal (CDL widens it); nothing else crosses kinds.
pub(crate) fn literal_fits(value_type: SignalType, literal: &Literal) -> bool {
    matches!(
        (value_type, literal),
        (SignalType::Real, Literal::Real(_) | Literal::Integer(_))
            | (SignalType::Integer, Literal::Integer(_))
            | (SignalType::Boolean, Literal::Boolean(_))
    )
}

pub(super) fn check(sequence: &Sequence, findings: &mut Findings) {
    if let Err(error) = sequence.name.check() {
        findings.error(
            Anchor::Sequence,
            INVALID_NAME,
            "sequence",
            format!("sequence name `{}`: {error}", sequence.name),
        );
    }
    if sequence.blocks.is_empty() {
        findings.error(
            Anchor::Sequence,
            EMPTY_SEQUENCE,
            "sequence",
            "the sequence has no blocks; it must contain at least one",
        );
    }
    if let Some(id) = &sequence.extends
        && let Err(error) = id.check()
    {
        findings.error(
            Anchor::Extends,
            INVALID_PROVENANCE_ID,
            "extends",
            format!("`{id}`: {error}"),
        );
    }

    let declared = sequence
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| (Anchor::Param(i), "parameter", &p.name))
        .chain(
            sequence
                .inputs
                .iter()
                .enumerate()
                .map(|(i, b)| (Anchor::Input(i), "input", &b.name)),
        )
        .chain(
            sequence
                .outputs
                .iter()
                .enumerate()
                .map(|(i, b)| (Anchor::Output(i), "output", &b.name)),
        )
        .chain(
            sequence
                .blocks
                .iter()
                .enumerate()
                .map(|(i, b)| (Anchor::Block(i, None), "block", &b.name)),
        );
    let mut first_seen: BTreeMap<&str, &str> = BTreeMap::new();
    for (position, kind, name) in declared {
        if let Err(error) = name.check() {
            findings.error(
                position,
                INVALID_NAME,
                name.as_str(),
                format!("{kind} name `{name}`: {error}"),
            );
        }
        if let Some(earlier) = first_seen.get(name.as_str()) {
            findings.error(
                position,
                DUPLICATE_NAME,
                name.as_str(),
                format!(
                    "{kind} `{name}` repeats the name of an earlier {earlier}; names are unique across parameters, inputs, outputs and blocks"
                ),
            );
        } else {
            first_seen.insert(name.as_str(), kind);
        }
    }

    for (index, param) in sequence.params.iter().enumerate() {
        let position = Anchor::Param(index);
        if !literal_fits(param.value_type, &param.value) {
            findings.error(
                position,
                PARAMETER_KIND_MISMATCH,
                param.name.as_str(),
                format!(
                    "sequence parameter `{}` is declared {} but its value is {}",
                    param.name,
                    param.value_type.as_str(),
                    param.value.kind_name()
                ),
            );
        } else if let Literal::Real(value) = param.value
            && !value.is_finite()
        {
            findings.error(
                position,
                NON_FINITE_PARAMETER,
                param.name.as_str(),
                format!("sequence parameter `{}` is {value}", param.name),
            );
        } else if let (SignalType::Real, Literal::Integer(value)) = (param.value_type, &param.value)
            && !exact_as_real(*value)
        {
            findings.error(
                position,
                PARAMETER_OUT_OF_RANGE,
                param.name.as_str(),
                inexact_detail(&format!("sequence parameter `{}`", param.name), *value),
            );
        }
    }

    let boundaries = sequence
        .inputs
        .iter()
        .enumerate()
        .map(|(i, b)| (Anchor::Input(i), b))
        .chain(
            sequence
                .outputs
                .iter()
                .enumerate()
                .map(|(i, b)| (Anchor::Output(i), b)),
        );
    for (position, boundary) in boundaries {
        for problem in boundary_problems(boundary) {
            findings.error(
                position,
                INVALID_SEMANTIC_POINT,
                boundary.name.as_str(),
                problem,
            );
        }
    }
}
