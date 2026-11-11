//! Which warning each diagnostic expectation matches: one to one (vectors note decision 12, as
//! ruled by the owner on 2026-11-10 for part 2).
//!
//! A diagnostic expectation matches **one** warning, and a warning is matched by at most one
//! expectation. Every warning no expectation took is unmatched: it is listed in the receipt,
//! counts in `warnings_total`, and is what `--deny-warnings` sees. So one expectation of
//! `CDL.Utilities.Assert` at a tick where two `Assert` instances warn leaves the second warning
//! unmatched, and two expectations there take one warning each.
//!
//! **The pairing order**, the same on every run: tick by tick, within one tick first every
//! expectation that names a `message`, in expectation order, then every expectation that names
//! only a `block`, in expectation order; each takes the earliest warning of its tick, in emission
//! order, that it matches and that no expectation has taken yet. Taking the expectations that
//! name a message first means a block-only expectation never takes the one warning a
//! message-bearing one needed, so the pairing matches as many expectations as any pairing can,
//! and whenever some pairing matches every expectation of a tick, this one does. Where fewer
//! warnings than expectations can be matched, the earlier expectations in each of the two groups
//! take them and the later ones fail (`diagnostic-missing`).
//!
//! Only ticks that ran are paired: an expectation at or after a refused frame takes nothing and is
//! not an instant.

use std::collections::BTreeMap;

use oce_replay::DiagnosticRow;

use crate::bind::{BoundDiagnostic, BoundExpectation};

/// The pairing of one scenario's diagnostic expectations with its warnings.
pub(crate) struct Pairing {
    /// Per expectation index: whether it is a diagnostic expectation that took a warning.
    pub(crate) matched: Vec<bool>,
    /// Per warning row, in the trace's order: whether an expectation took it.
    pub(crate) taken: Vec<bool>,
}

/// Whether `row` matches `diagnostic`'s block and, when the expectation gives one, its message
/// byte for byte. The tick is the caller's: [`pair`] offers each expectation the rows of its own
/// tick only.
pub(crate) fn matches(row: &DiagnosticRow, diagnostic: &BoundDiagnostic) -> bool {
    row.block == diagnostic.block
        && diagnostic
            .message
            .as_ref()
            .is_none_or(|message| *message == row.message)
}

/// Pair `expectations` with `rows` (the trace's warnings, sorted by frame) by the order in the
/// module documentation, for the ticks before `compared`.
pub(crate) fn pair(
    rows: &[DiagnosticRow],
    expectations: &[BoundExpectation],
    compared: u64,
) -> Pairing {
    let mut matched = vec![false; expectations.len()];
    let mut taken = vec![false; rows.len()];
    // The diagnostic expectations of each tick that ran, in expectation order.
    let mut by_tick: BTreeMap<u64, Vec<(usize, &BoundDiagnostic)>> = BTreeMap::new();
    for (index, expectation) in expectations.iter().enumerate() {
        if let BoundExpectation::Diagnostic(diagnostic) = expectation
            && diagnostic.tick < compared
        {
            by_tick
                .entry(diagnostic.tick)
                .or_default()
                .push((index, diagnostic));
        }
    }
    for (tick, diagnostics) in &by_tick {
        let start = rows.partition_point(|row| row.frame < *tick);
        let end = rows.partition_point(|row| row.frame <= *tick);
        let named = diagnostics.iter().filter(|(_, d)| d.message.is_some());
        let block_only = diagnostics.iter().filter(|(_, d)| d.message.is_none());
        for (index, diagnostic) in named.chain(block_only) {
            let free = (start..end).find(|&row| !taken[row] && matches(&rows[row], diagnostic));
            if let Some(row) = free {
                taken[row] = true;
                matched[*index] = true;
            }
        }
    }
    Pairing { matched, taken }
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;
