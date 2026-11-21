//! A Real parameter holds a Real (issue #683).
//!
//! Grounding is literal-natural: a bare JSON integer, a typed `xsd:integer` literal, an Integer
//! expression and a reference to an Integer-valued sequence parameter all ground to
//! [`Value::Integer`], whatever the parameter they are bound to. CDL widens an Integer bound to a
//! Real parameter, and every block constructor already reads it that way, so the two spellings
//! of one number (`0` and `0.0`) ran identically while the resolved model, the parameter table,
//! the compiled-schedule blob and the canonical content id told them apart.
//!
//! [`hold_real_parameters_as_reals`] closes that once, at the end of an instance's grounding and
//! before anything else reads its table: an Integer bound to a parameter the block catalog types
//! Real ([`oce_blocks::parameter_value_type`]: scalars and flattened array elements alike)
//! becomes the Real of the same value. The conversion is refused, never rounded: an integer above
//! 2^53 in magnitude ([`MAX_EXACT_INTEGER_AS_REAL`]) is not exact as a binary64 in general, so it
//! is [`DiagCode::ParameterOutOfRange`] on the parameter node that bound it, the code the
//! authoring stage uses for the same literal. The bound is the magnitude, not exactness of the
//! one value: `i64::MIN` is a power of two and still refused, so the rule is one comparison a
//! reader can check.
//!
//! Nothing else moves. An Integer on an Integer, enumeration or unregistered parameter stays an
//! Integer; a Real stays the Real it was, `-0.0` included. No Integer spells a negative zero: a
//! typed `xsd:integer` of `-0` is the Integer zero and so `+0.0`, while a bare JSON `-0` is
//! already the float `-0.0` to the JSON reader and was never an Integer. The sibling scope a
//! later binding of the same instance reads keeps the grounded value: an expression is evaluated
//! over what the document wrote.

use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{Value, ValueType};

use crate::diagnostic_list::DiagnosticList;

/// The largest magnitude an Integer bound to a Real parameter may have: 2^53. Every integer up to
/// it is exact as a binary64; 2^53 + 1 is the first that is not.
pub(crate) const MAX_EXACT_INTEGER_AS_REAL: u64 = 1 << 53;

/// The Real of `integer`, or `None` when its magnitude is above
/// [`MAX_EXACT_INTEGER_AS_REAL`]. Total: `i64::MIN` has no `i64` magnitude and is read unsigned.
pub(crate) fn exact_real(integer: i64) -> Option<f64> {
    // The cast is exact for every value the comparison admits.
    (integer.unsigned_abs() <= MAX_EXACT_INTEGER_AS_REAL).then_some(integer as f64)
}

/// Replace every Integer bound to a catalog-Real parameter of `class_path` in `table` with the
/// Real of the same value, in place and in table order. `sources[i]` is the IRI of the parameter
/// node that produced `table[i]` (one node produces every element of a preserved array), and is
/// the subject of a refusal. An entry that cannot be converted exactly is left as grounded and
/// reported; the document is then refused, so the unconverted value reaches nothing.
pub(super) fn hold_real_parameters_as_reals(
    class_path: &str,
    owner: &str,
    table: &mut [(Arc<str>, Value)],
    sources: &[&str],
    diags: &mut DiagnosticList,
) {
    for (index, (name, value)) in table.iter_mut().enumerate() {
        let Value::Integer(integer) = *value else {
            continue;
        };
        if oce_blocks::parameter_value_type(class_path, name) != Some(ValueType::Real) {
            continue;
        }
        match exact_real(integer) {
            Some(real) => *value = Value::Real(real),
            // Raised, not pushed: every element of one array can be refused, and each message
            // names the owner, so nothing is built once the list has ended.
            None => diags.raise(|| {
                Diagnostic::error(
                    DiagCode::ParameterOutOfRange,
                    format!(
                        "parameter `{name}` on `{owner}` binds the Integer {integer} to a Real \
                         parameter of `{class_path}`; an Integer above 2^53 in magnitude is not \
                         exact as a Real"
                    ),
                )
                .with_subject(sources.get(index).copied().unwrap_or(owner).to_owned())
            }),
        }
    }
}
