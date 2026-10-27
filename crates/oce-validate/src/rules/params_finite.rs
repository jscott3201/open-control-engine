//! The engine-wide Real-parameter finiteness gate (issue #362).
//!
//! Every Real parameter value on every block — whatever its class, and including each element of
//! a Real array parameter, which arrives here already flattened to scalar `base_i` / `base_i_j`
//! entries — must be finite. `NaN`, `+inf` and `-inf` are refused with
//! [`DiagCode::NonFiniteParameter`]. No registered CDL class takes a non-finite parameter (the
//! `Modelica.Constants.inf` alias grounds to `f64::MAX`, which is finite), so there is no per-class
//! opt-out.
//!
//! The predicate itself is [`crate::parameter_value_is_finite`], shared with `oce-api`'s
//! tune-at-rest `set_param` so the load gate and the edit gate cannot disagree.
//!
//! For CXF loads the resolver refuses earlier and more broadly: strict-finite parameter grounding
//! in `oce-cxf` rejects non-finite values at every composite level and any non-finite expression
//! intermediate, under the same code, so no public load path reaches this gate with a non-finite
//! value today. It is the backstop on the flattened graph: it re-checks every block parameter
//! value, whatever produced it, against any future ingest path or grounding regression.

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{BlockInstance, Value};

use super::block_subject_of;

/// Emit one [`DiagCode::NonFiniteParameter`] error per non-finite Real parameter on `blk`, in
/// parameter declaration order, and report whether any was found.
///
/// Runs for every block, including blocks whose class is not registered, so the refusal does not
/// depend on registry coverage. Total and panic-free.
pub(super) fn check_finite_params(blk: &BlockInstance, diags: &mut Vec<Diagnostic>) -> bool {
    let mut found = false;
    for (name, value) in &blk.params.values {
        // Only a Real can be non-finite; the shared predicate decides which Reals are refused.
        let Value::Real(v) = value else {
            continue;
        };
        if crate::parameter_value_is_finite(value) {
            continue;
        }
        found = true;
        diags.push(
            Diagnostic::error(
                DiagCode::NonFiniteParameter,
                format!(
                    "parameter `{name}` on block `{}` must be finite; got {v}",
                    blk.class_iri
                ),
            )
            .with_subject(block_subject_of(blk)),
        );
    }
    found
}
