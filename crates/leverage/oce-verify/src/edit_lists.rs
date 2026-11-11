//! A bound set's parameter edits, and their distinct effective lists (work-budget note, ruling 7;
//! vectors note, part 3's ruling on mutant edits; issue #571).
//!
//! **Held once.** An edit is a row of the subject's parameter table (sorted by path bytes, one row
//! per path, so row order is path order) and its value. The set's own `params` are held once, in
//! [`Edits`], and each scenario holds only its own map ([`crate::bind::BoundScenario::edits`]); a
//! scenario's effective list is the two merged, its own value winning where both edit one
//! parameter (decision 14). A path is copied out only for the list one trial, run or receipt is
//! using, so binding a set of 4,096 scenarios under 256 set-level edits holds 256 edits, not a
//! million.
//!
//! **Distinct lists.** Binding tries each distinct non-empty effective list once: on the subject,
//! before any scenario runs, and on each mutant while validating it. Two lists are one when they
//! name the same parameters in the same order with values of the same kind and bits, so `1.0` and
//! `1` differ, as do `0.0` and `-0.0`. The lists keep the order of the first scenario that has
//! each, so the first scenario in file order holding a refused list is the one a refusal names,
//! as when every scenario was tried. Their count is the budget step's `e`. They are found by
//! sorting the scenarios by their merged lists, compared in place: nothing is copied to compare.

use std::cmp::Ordering;

use crate::bind::BoundScenario;
use crate::receipt::ReceiptValue;

/// One parameter edit: the row of the subject's parameter table, and its value.
pub(crate) type Edit = (usize, ReceiptValue);

/// A bound set's parameter edits: the subject's parameter paths by row, and the set's own edits,
/// which every scenario shares.
pub(crate) struct Edits {
    /// The subject's parameter paths, sorted by their bytes, one per row.
    pub(crate) paths: Vec<String>,
    /// The set's own `params`, by row in ascending order.
    pub(crate) set: Vec<Edit>,
}

/// The identity of a value: its kind and bits.
fn identity(value: ReceiptValue) -> (u8, u64) {
    match value {
        ReceiptValue::Real(x) => (0, x.to_bits()),
        ReceiptValue::Integer(n) => (1, n.cast_unsigned()),
        ReceiptValue::Boolean(b) => (2, u64::from(b)),
    }
}

impl Edits {
    /// The effective edits of a scenario whose own map is `own` (by row, ascending): the set's,
    /// with `own`'s value where both edit one parameter, by row.
    pub(crate) fn effective<'b>(&'b self, own: &'b [Edit]) -> impl Iterator<Item = Edit> + 'b {
        let mut set = self.set.iter().copied().peekable();
        let mut own = own.iter().copied().peekable();
        std::iter::from_fn(move || match (set.peek(), own.peek()) {
            (Some(&(s, _)), Some(&(o, _))) => match s.cmp(&o) {
                Ordering::Less => set.next(),
                Ordering::Greater => own.next(),
                Ordering::Equal => {
                    set.next();
                    own.next()
                }
            },
            (Some(_), None) => set.next(),
            (None, _) => own.next(),
        })
    }

    /// The effective edits of `own` with their canonical paths, sorted by path bytes: the list one
    /// trial, run or receipt uses, built for that use alone.
    pub(crate) fn resolved<'b>(&'b self, own: &'b [Edit]) -> Vec<(&'b str, ReceiptValue)> {
        self.effective(own)
            .map(|(row, value)| (self.paths[row].as_str(), value))
            .collect()
    }

    /// Whether a scenario whose own map is `own` edits nothing.
    pub(crate) fn none(&self, own: &[Edit]) -> bool {
        self.set.is_empty() && own.is_empty()
    }

    /// The two effective lists in a total order, compared edit by edit without copying either.
    fn compare(&self, left: &[Edit], right: &[Edit]) -> Ordering {
        let key = |(row, value): Edit| (row, identity(value));
        self.effective(left)
            .map(key)
            .cmp(self.effective(right).map(key))
    }
}

/// Every distinct non-empty effective edit list of a bound set, each as the first scenario, in
/// file order, that has it, in that order.
pub(crate) struct Lists<'b> {
    edits: &'b Edits,
    scenarios: &'b [BoundScenario],
    firsts: Vec<usize>,
}

impl<'b> Lists<'b> {
    /// How many distinct lists there are: the budget step's `e`.
    pub(crate) fn len(&self) -> usize {
        self.firsts.len()
    }

    /// Each list as its first scenario's index and its edits with their paths, in that order.
    pub(crate) fn resolved(&self) -> impl Iterator<Item = (usize, Vec<(&'b str, ReceiptValue)>)> {
        let edits = self.edits;
        let scenarios = self.scenarios;
        self.firsts
            .iter()
            .map(move |&first| (first, edits.resolved(&scenarios[first].edits)))
    }
}

/// Every distinct non-empty effective edit list of `scenarios` under `edits`.
pub(crate) fn distinct<'b>(edits: &'b Edits, scenarios: &'b [BoundScenario]) -> Lists<'b> {
    let mut order: Vec<usize> = (0..scenarios.len())
        .filter(|&index| !edits.none(&scenarios[index].edits))
        .collect();
    // Equal lists sit together, the first scenario of each first.
    order.sort_by(|&a, &b| {
        edits
            .compare(&scenarios[a].edits, &scenarios[b].edits)
            .then(a.cmp(&b))
    });
    let mut firsts: Vec<usize> = Vec::new();
    for (position, &index) in order.iter().enumerate() {
        let repeat = position > 0 && {
            let previous = order[position - 1];
            edits.compare(&scenarios[previous].edits, &scenarios[index].edits) == Ordering::Equal
        };
        if !repeat {
            firsts.push(index);
        }
    }
    firsts.sort_unstable();
    Lists {
        edits,
        scenarios,
        firsts,
    }
}

#[cfg(test)]
#[path = "edit_lists_tests.rs"]
mod tests;
