//! Every parameter a block binds is one its registered class declares (issue #748).
//!
//! A block reads its parameters by name and falls back to a default for a name it does not
//! find, so a binding under a name the class does not declare was read by nothing: a misspelt
//! `amplitud` on `Integers.Sources.Pulse` loaded clean and the block ran on the default amplitude,
//! while the misspelt value still entered the model, the content id and the compiled blob. Each
//! such binding is now [`DiagCode::UnknownParameter`], one finding per binding in parameter order.
//! The subject is the block, as for every class-level parameter finding; the message names the
//! parameter first and, when [`oce_blocks::nearest_declared_parameter`] finds one, the declared
//! parameter it probably meant.
//!
//! What a class declares is [`oce_blocks::declares_parameter`]: its scalar parameters and the
//! canonical elements of its arrays. Two kinds of undeclared name are refused by another rule
//! already, and are left to it so each is reported once:
//!
//! - a name with the shape of an element of one of the class's arrays but a sign, a leading zero
//!   or a zero index ([`oce_blocks::spells_array_element`]): the spelling check's
//!   `non-canonical-element-name` and the extent rules' `parameter-out-of-range`;
//! - any name under the `<base>_` prefix of a matrix (`K`, `table`) or of a `TimeTable`'s
//!   `offset`: the matrix and offset rules judge every such name, and refuse one that is no cell
//!   of their shape (`K_1`, `table_1_2_3`, `offset_x`) as `parameter-out-of-range`.
//!
//! An unregistered class is outside the rule: `oce-api` refuses the class itself.
//!
//! Cost: one pass over the class's declared names per binding, and for an undeclared name the
//! suggestion's pass, which skips every declared name by length alone unless the undeclared name
//! is within a few bytes of its length; each finding is raised through the bounded list, so none
//! is built once the list has ended.

use oce_blocks::{ParamRule, declares_parameter, nearest_declared_parameter, spells_array_element};
use oce_diag::{DiagCode, Diagnostic, DiagnosticList};
use oce_model::BlockInstance;

use super::block_subject_of;

/// Report every parameter `blk` binds under a name its registered class does not declare, in
/// parameter order, except those another of the class's `rules` refuses (see the module
/// documentation). A block of an unregistered class gets nothing here. Total and panic-free.
pub(super) fn check_declared_parameter_names(
    blk: &BlockInstance,
    rules: &[ParamRule],
    diags: &mut DiagnosticList,
) {
    let class = blk.class_iri.as_ref();
    for (name, _) in &blk.params.values {
        match declares_parameter(class, name) {
            None => return,
            Some(true) => continue,
            Some(false)
                if spells_array_element(class, name) || judged_by_a_prefix_rule(rules, name) =>
            {
                continue;
            }
            Some(false) => {}
        }
        diags.raise(|| {
            let message = match nearest_declared_parameter(class, name) {
                Some(nearest) => format!(
                    "parameter `{name}` on block `{class}` is not a parameter of its class; did \
                     you mean `{nearest}`?"
                ),
                None => {
                    format!("parameter `{name}` on block `{class}` is not a parameter of its class")
                }
            };
            Diagnostic::error(DiagCode::UnknownParameter, message)
                .with_subject(block_subject_of(blk))
        });
    }
}

/// Whether a matrix or offset rule of the class judges `name`: those rules read every name under
/// their array's `<base>_` prefix and refuse one that is no cell of their shape.
fn judged_by_a_prefix_rule(rules: &[ParamRule], name: &str) -> bool {
    rules.iter().any(|rule| match *rule {
        ParamRule::RealMatrixElements { base, .. }
        | ParamRule::TimeTableMatrix { base, .. }
        | ParamRule::TimeTableOffset { base, .. } => name
            .strip_prefix(base)
            .is_some_and(|rest| rest.starts_with('_')),
        _ => false,
    })
}

#[cfg(test)]
#[path = "params_declared_tests.rs"]
mod tests;
