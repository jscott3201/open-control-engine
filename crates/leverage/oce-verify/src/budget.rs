//! The work budgets (the work-budget note, `docs/design/vector-work-budget.md`, rulings 1 to 8):
//! four products that bound what one run may ask of the engine, checked in one step at bind.
//!
//! With `N + 1` the ticks, `s` the scenarios, `m` the listed mutants, `e` the distinct non-empty
//! effective edit lists ([`crate::edit_lists`]), `I_j` the instants scenario `j` compares (the ticks
//! inside each window, plus one per diagnostic expectation) and `D` the subject's bytes and every
//! supplied mutant's together:
//!
//! | Budget | Counts | Refusal |
//! | --- | --- | --- |
//! | [`VerifyLimits::max_total_frames`] | `(N + 1) * s * (1 + m)` | `too-many-frames` |
//! | [`VerifyLimits::max_total_instants`] | `(1 + m) * sum(I_j)` | `too-many-instants` |
//! | [`VerifyLimits::max_total_load_bytes`] | `D * (1 + e + s)` | `too-many-load-bytes` |
//! | [`VerifyLimits::max_total_frame_bytes`] | `(N + 1) * s * D` | `too-many-frame-bytes` |
//!
//! Every count is an upper bound: each mutant is counted as if it ran every scenario, and each
//! window as if no frame were refused, so a run inside its budgets does exactly the work it would
//! do with none. `D * (1 + e + s)` is the subject's and each mutant's bind load, one trial per
//! distinct list and one run per scenario. The budgets are checked in this order, the first one
//! exceeded winning, with a scenario's `N + 1` held to the replay limits' `max_frames` first, as
//! part of the frame budget. They are arithmetic on what binding already knows after the
//! subject's first load and its names, so the check runs before every other load (ruling 6).
//!
//! Each product is taken in `u128` with checked multiplication, and each sum saturates. A
//! product that does not fit is past every limit, so a hostile count refuses: it never wraps and
//! never panics.

use crate::bind::{BoundExpectation, BoundScenario};
use crate::limits::VerifyLimits;
use crate::vector_set::VectorSet;
use crate::verify_error::{VerifyError, VerifyErrorKind as Kind};

/// What one run may ask for, counted before any load but the subject's first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Work {
    /// `N + 1`: the frames each scenario runs.
    pub(crate) ticks: u64,
    /// `s`: the scenarios.
    pub(crate) scenarios: u128,
    /// `m`: the mutants the set lists.
    pub(crate) mutants: u128,
    /// `sum(I_j)`: the instants the scenarios compare on one document.
    pub(crate) instants: u128,
    /// `e`: the distinct non-empty effective edit lists.
    pub(crate) edit_lists: u128,
    /// `D`: the subject's bytes and every supplied mutant's.
    pub(crate) document_bytes: u128,
}

/// A `usize` count as `u128`, which holds every `usize`.
fn wide(count: usize) -> u128 {
    u128::try_from(count).unwrap_or(u128::MAX)
}

/// The product of `factors`, or `None` when it does not fit in `u128`.
fn product(factors: &[u128]) -> Option<u128> {
    factors
        .iter()
        .try_fold(1_u128, |total, factor| total.checked_mul(*factor))
}

/// A product as a sentence's number.
fn shown(total: Option<u128>) -> String {
    total.map_or_else(|| "more than 2^128".to_owned(), |total| total.to_string())
}

impl Work {
    /// The work of `set`, bound as `scenarios`, with `edit_lists` distinct lists, against the
    /// subject's bytes and every supplied mutant's.
    pub(crate) fn of(
        set: &VectorSet,
        scenarios: &[BoundScenario],
        edit_lists: usize,
        subject: &[u8],
        supplied: &[(&str, &[u8])],
    ) -> Self {
        let instants = scenarios
            .iter()
            .flat_map(|scenario| &scenario.expectations)
            .map(|expectation| match expectation {
                BoundExpectation::Window(window) => {
                    u128::from(window.last_tick.saturating_sub(window.first_tick)) + 1
                }
                BoundExpectation::Diagnostic(_) => 1,
            })
            .fold(0_u128, u128::saturating_add);
        let document_bytes = supplied
            .iter()
            .map(|(_, bytes)| wide(bytes.len()))
            .fold(wide(subject.len()), u128::saturating_add);
        Self {
            ticks: set.clock().tick_count(),
            scenarios: wide(set.scenarios().len()),
            mutants: wide(set.mutants().map_or(0, <[_]>::len)),
            instants,
            edit_lists: wide(edit_lists),
            document_bytes,
        }
    }

    /// `(N + 1) * s * (1 + m)`.
    pub(crate) fn frames(&self) -> Option<u128> {
        product(&[
            u128::from(self.ticks),
            self.scenarios,
            self.mutants.checked_add(1)?,
        ])
    }

    /// `(1 + m) * sum(I_j)`.
    pub(crate) fn compared_instants(&self) -> Option<u128> {
        product(&[self.mutants.checked_add(1)?, self.instants])
    }

    /// `D * (1 + e + s)`.
    pub(crate) fn load_bytes(&self) -> Option<u128> {
        let loads = self
            .edit_lists
            .checked_add(self.scenarios)?
            .checked_add(1)?;
        product(&[self.document_bytes, loads])
    }

    /// `(N + 1) * s * D`.
    pub(crate) fn frame_bytes(&self) -> Option<u128> {
        product(&[u128::from(self.ticks), self.scenarios, self.document_bytes])
    }
}

/// Whether `total` is past `limit`; a product that did not fit always is.
fn exceeds(total: Option<u128>, limit: u64) -> bool {
    total.is_none_or(|total| total > u128::from(limit))
}

/// Hold `work` to the four budgets of `limits` (already effective), in the documented order.
///
/// # Errors
/// `too-many-frames`, `too-many-instants`, `too-many-load-bytes` or `too-many-frame-bytes`, for
/// the first budget exceeded; `too-many-frames` also for a scenario whose `N + 1` passes the
/// replay limits' `max_frames`.
pub(crate) fn check(work: &Work, limits: &VerifyLimits) -> Result<(), VerifyError> {
    let Work {
        ticks,
        scenarios,
        mutants,
        instants,
        edit_lists,
        document_bytes,
    } = *work;
    if scenarios > 0 && ticks > limits.replay.max_frames {
        return Err(VerifyError::new(
            Kind::TooManyFrames,
            "clock",
            format!(
                "each scenario runs {ticks} frames; the replay limits allow {}",
                limits.replay.max_frames
            ),
        ));
    }
    let frames = work.frames();
    if exceeds(frames, limits.max_total_frames) {
        return Err(VerifyError::new(
            Kind::TooManyFrames,
            "scenarios",
            format!(
                "{scenarios} scenarios of {ticks} frames on the subject and {mutants} mutants are {} frames; at most {} are allowed",
                shown(frames),
                limits.max_total_frames
            ),
        ));
    }
    let compared = work.compared_instants();
    if exceeds(compared, limits.max_total_instants) {
        return Err(VerifyError::new(
            Kind::TooManyInstants,
            "scenarios",
            format!(
                "{scenarios} scenarios compare {instants} instants on the subject and on each of {mutants} mutants, {} in all; at most {} are allowed",
                shown(compared),
                limits.max_total_instants
            ),
        ));
    }
    let loaded = work.load_bytes();
    if exceeds(loaded, limits.max_total_load_bytes) {
        return Err(VerifyError::new(
            Kind::TooManyLoadBytes,
            "scenarios",
            format!(
                "{document_bytes} bytes of subject and mutants, each loaded once, once per each of {edit_lists} distinct edit lists and once per each of {scenarios} scenarios, are {} bytes loaded; at most {} are allowed",
                shown(loaded),
                limits.max_total_load_bytes
            ),
        ));
    }
    let frame_bytes = work.frame_bytes();
    if exceeds(frame_bytes, limits.max_total_frame_bytes) {
        return Err(VerifyError::new(
            Kind::TooManyFrameBytes,
            "scenarios",
            format!(
                "{scenarios} scenarios of {ticks} frames over {document_bytes} bytes of subject and mutants are {} byte-frames; at most {} are allowed",
                shown(frame_bytes),
                limits.max_total_frame_bytes
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "budget_tests.rs"]
mod tests;
