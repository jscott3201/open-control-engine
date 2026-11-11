//! The distinct effective edit lists of a bound set (work-budget note, ruling 7; vectors note,
//! part 3's ruling on mutant edits).
//!
//! Binding tries each distinct non-empty list once: on the subject, before any scenario runs, and
//! on each mutant while validating it. Two lists are one when they name the same canonical paths
//! in the same order (each list is sorted by path bytes) with values of the same kind and bits,
//! so `1.0` and `1` differ, as do `0.0` and `-0.0`. The lists keep the order of the first scenario
//! that has each, so the first scenario in file order holding a refused list is the one a refusal
//! names, as when every scenario was tried. Their count is the budget step's `e`.
//!
//! Finding them costs one ordered-set insertion per scenario, so a hostile set of 4,096 scenarios
//! with 512 edits each is sorted, never compared pair by pair.

use std::collections::BTreeSet;

use crate::bind::BoundScenario;
use crate::receipt::ReceiptValue;

/// One distinct non-empty effective edit list.
pub(crate) struct EditList<'s> {
    /// The first scenario, in file order, whose effective edits are this list.
    pub(crate) first: usize,
    /// The edits by canonical path, sorted by path bytes.
    pub(crate) edits: &'s [(String, ReceiptValue)],
}

/// The identity of one edit: its path, and its value's kind and bits.
type EditKey<'s> = (&'s str, u8, u64);

fn key(path: &str, value: ReceiptValue) -> EditKey<'_> {
    match value {
        ReceiptValue::Real(x) => (path, 0, x.to_bits()),
        ReceiptValue::Integer(n) => (path, 1, n.cast_unsigned()),
        ReceiptValue::Boolean(b) => (path, 2, u64::from(b)),
    }
}

/// Every distinct non-empty effective edit list of `scenarios`, in the order of the first scenario
/// that has it.
pub(crate) fn distinct(scenarios: &[BoundScenario]) -> Vec<EditList<'_>> {
    let mut seen: BTreeSet<Vec<EditKey<'_>>> = BTreeSet::new();
    let mut lists = Vec::new();
    for (first, scenario) in scenarios.iter().enumerate() {
        if scenario.edits.is_empty() {
            continue;
        }
        let identity: Vec<EditKey<'_>> = scenario
            .edits
            .iter()
            .map(|(path, value)| key(path, *value))
            .collect();
        if seen.insert(identity) {
            lists.push(EditList {
                first,
                edits: &scenario.edits,
            });
        }
    }
    lists
}

#[cfg(test)]
#[path = "edit_lists_tests.rs"]
mod tests;
