//! Sequence-level checks: the sequence shape, declared names and their uniqueness, `extends`,
//! sequence parameter types and values, and boundary semantic slots and bounds.

use std::collections::BTreeMap;

use crate::model::Sequence;

use super::bounds::boundary_bound_problems;
use super::members::parameter_problem;
use super::semantic::boundary_problems;
use super::{
    Anchor, DUPLICATE_NAME, EMPTY_SEQUENCE, Findings, INVALID_NAME, INVALID_PROVENANCE_ID,
    INVALID_SEMANTIC_POINT,
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
        if let Some((code, detail)) = parameter_problem(param) {
            findings.error(Anchor::Param(index), code, param.name.as_str(), detail);
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
        for (code, detail) in boundary_bound_problems(boundary) {
            findings.error(position, code, boundary.name.as_str(), detail);
        }
    }
}
