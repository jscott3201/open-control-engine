//! Lowering the authoring model into the diff's own terms: the level-neutral [`Side`] and the
//! data's [`Literal`], and the text-form spelling of a value as written.
//!
//! The authoring model's enums are `#[non_exhaustive]`. A kind this crate does not know yet is
//! carried as the text of its debug spelling, so a difference in it is still reported; nothing
//! here refuses or panics.

use oce_author::{Bound, Endpoint, ParamValue, Sequence};

use crate::data::{Assignment, Literal};
use crate::wiring::{End, Node, Side};
use crate::work::visited;

/// The data literal of an authored literal: a Real by its bits.
pub(super) fn literal(value: &oce_author::Literal) -> Literal {
    match value {
        oce_author::Literal::Integer(integer) => Literal::Integer { integer: *integer },
        oce_author::Literal::Real(real) => Literal::real(*real),
        oce_author::Literal::Boolean(boolean) => Literal::Boolean { boolean: *boolean },
        oce_author::Literal::Text(text) => Literal::Text { text: text.clone() },
        oce_author::Literal::Member(member) => Literal::Member {
            member: member.spelling(),
        },
        other => Literal::Text {
            text: format!("{other:?}"),
        },
    }
}

/// The data literal of an assignment's value: a literal, or a reference by name.
pub(super) fn value(value: &ParamValue) -> Literal {
    match value {
        ParamValue::Literal(inner) => literal(inner),
        ParamValue::Ref(name) => Literal::Ref {
            name: name.as_str().to_owned(),
        },
        other => Literal::Text {
            text: format!("{other:?}"),
        },
    }
}

/// The data literal of a boundary bound.
pub(super) fn bound(value: Bound) -> Literal {
    match value {
        Bound::Integer(integer) => Literal::Integer { integer },
        Bound::Real(real) => Literal::real(real),
        other => Literal::Text {
            text: format!("{other:?}"),
        },
    }
}

/// The Real an Integer literal means where a Real is expected, when the conversion is exact:
/// magnitudes up to 2^53, as the validator admits. `0` is `+0.0`, never `-0.0`.
pub(crate) fn exact_real(integer: i64) -> Option<f64> {
    (integer.unsigned_abs() <= 1 << 53).then_some(integer as f64)
}

/// Whether an Integer literal and a Real literal are one value where a Real is expected.
pub(super) fn same_real(a: &oce_author::Literal, b: &oce_author::Literal) -> bool {
    let (integer, real) = match (a, b) {
        (oce_author::Literal::Integer(integer), oce_author::Literal::Real(real))
        | (oce_author::Literal::Real(real), oce_author::Literal::Integer(integer)) => {
            (*integer, *real)
        }
        _ => return false,
    };
    exact_real(integer).is_some_and(|exact| exact.to_bits() == real.to_bits())
}

/// A value as the text form writes it: the number written before a scaled unit suffix when there
/// is one, then the suffix after one space.
pub(super) fn spelled(
    value: &Literal,
    scaled_from: Option<&oce_author::Literal>,
    suffix: Option<&str>,
) -> String {
    let written = scaled_from.map(literal);
    let number = crate::render::literal_text(written.as_ref().unwrap_or(value));
    match suffix {
        Some(suffix) => format!("{number} {suffix}"),
        None => number,
    }
}

fn end(endpoint: &Endpoint) -> End<'_> {
    match endpoint {
        Endpoint::Boundary(name) => End::Boundary(name.as_str()),
        Endpoint::Port { block, port } => End::Port {
            block: block.as_str(),
            port: port.as_str(),
        },
        // A kind of endpoint this crate does not know names no boundary and no block.
        _ => End::Boundary(""),
    }
}

/// The part of `sequence` that identity and wiring are decided on.
pub(super) fn side(sequence: &Sequence) -> Side<'_> {
    visited(sequence.blocks.len() + sequence.connections.len());
    Side {
        nodes: sequence
            .blocks
            .iter()
            .map(|block| {
                visited(block.assignments.len());
                Node {
                    name: block.name.as_str(),
                    class: &block.class_path,
                    assignments: block
                        .assignments
                        .iter()
                        .map(|assignment| Assignment {
                            name: assignment.name.as_str().to_owned(),
                            value: value(&assignment.value),
                        })
                        .collect(),
                }
            })
            .collect(),
        boundaries: sequence
            .inputs
            .iter()
            .chain(&sequence.outputs)
            .map(|boundary| boundary.name.as_str())
            .collect(),
        links: sequence
            .connections
            .iter()
            .map(|connection| (end(&connection.source), end(&connection.target)))
            .collect(),
    }
}
