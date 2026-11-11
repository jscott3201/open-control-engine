//! The bounds a vector set is read and run under (vectors note, "Bounds"; decision 35).
//!
//! Three are fixed by the strict profile and shared with the authoring wire form: nesting
//! ([`MAX_DEPTH`]), list length ([`MAX_LIST_LEN`]) and string length ([`MAX_STRING_BYTES`]); one
//! more is fixed by the receipt ([`MAX_LOAD_WARNINGS`]). The rest are [`VerifyLimits`]: each field
//! is a default and a documented maximum at once. A caller may lower any of them, to zero
//! included; a value above the maximum is read as the maximum, so no caller can raise one. Every
//! bound is inclusive: a set *at* a bound loads or runs, one unit past it is refused with its own
//! code, never truncated. The two record bounds are caps on a list, not refusals: past them the
//! receipt keeps the total beside the capped list.

use oce_replay::ReplayLimits;

/// The deepest nesting of objects and arrays. The deepest well-formed set nests six levels (set,
/// scenarios, scenario, inputs, step list, step); two more are allowed so that a misplaced value
/// is reported as a wrong type or an unknown key, not as depth.
pub const MAX_DEPTH: usize = 8;

/// The longest list: scenarios, expectations or one input's steps.
pub const MAX_LIST_LEN: usize = 65_536;

/// The longest string token, in bytes as written between its quotes (an escape counts as the
/// bytes it is written with).
pub const MAX_STRING_BYTES: usize = 4_096;

/// The largest magnitude an integer token may have where the format reads a time (`step_s`,
/// `horizon_s`, a step's `t`, `from_s`, `to_s`, `at_s`): 2^53. A time is a binary64, and up to
/// this magnitude every integer is one exactly, so an integer token never names a time other than
/// the one it spells. A larger time is written as a real (`1e16`), which rounds to the nearest
/// binary64 as every real does.
pub const MAX_TIME_INTEGER: i64 = 1 << 53;

/// The most load-warning codes a receipt lists (`load_warnings`); `load_warnings_total` keeps the
/// count before the cap.
pub const MAX_LOAD_WARNINGS: usize = 64;

/// The bounds on one vector set and its run that the strict profile does not fix.
///
/// `Default` is the documented maxima ([`VerifyLimits::MAX`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifyLimits {
    /// Bytes in the document. Maximum 4 MiB; past it the refusal is `input-too-large`, decided
    /// from the length before a byte is examined.
    pub max_bytes: u64,
    /// Ticks of the set's clock, `horizon_s / step_s + 1`. Maximum 16,777,216; past it the
    /// refusal is `too-many-ticks`, decided from the quotient before any tick is computed.
    pub max_ticks: u64,
    /// Scenarios in the set. Maximum 4,096; past it the refusal is `too-many-scenarios`.
    pub max_scenarios: u32,
    /// Expectations in one scenario. Maximum 4,096; past it the refusal is
    /// `too-many-expectations`.
    pub max_expectations: u32,
    /// Entries in one `params` map, the set's or a scenario's. Maximum 256; past it the refusal
    /// is `too-many-parameters`.
    pub max_parameters: u32,
    /// Mutants the set lists. Maximum 256; past it the refusal is `too-many-mutants`, at load.
    pub max_mutants: u32,
    /// Bytes of every mutant supplied to a run, together. Maximum 64 MiB; past it the refusal is
    /// `too-many-mutant-bytes`, at bind, before any mutant is loaded. Each mutant is also held to
    /// the engine's own CXF byte limit (`oce_api::MAX_CXF_BYTES`), as the subject is.
    pub max_mutant_bytes: u64,
    /// Failure records a scenario's receipt lists. Maximum 16; past it the list stops and
    /// `failures_total` keeps the count.
    pub max_recorded_failures: u32,
    /// Unmatched run-time warnings a scenario's receipt lists. Maximum 16; past it the list stops
    /// and `warnings_total` keeps the count.
    pub max_recorded_warnings: u32,
    /// Frames one run may execute, `(N + 1) * scenarios`. Maximum 16,777,216; past it the refusal
    /// is `too-many-frames`, decided at bind before any frame runs.
    pub max_total_frames: u64,
    /// The limits each scenario's replay runs under ([`oce_replay::ReplayLimits`]); each field is
    /// lowered to that crate's documented maximum, never raised. A scenario whose `N + 1` ticks
    /// pass its `max_frames` is `too-many-frames` at bind; a replay stopped by another of them is
    /// `replay-failed`.
    pub replay: ReplayLimits,
}

impl VerifyLimits {
    /// The documented maxima, which are also the defaults.
    pub const MAX: Self = Self {
        max_bytes: 4 * 1024 * 1024,
        max_ticks: 16_777_216,
        max_scenarios: 4_096,
        max_expectations: 4_096,
        max_parameters: 256,
        max_mutants: 256,
        max_mutant_bytes: 64 * 1024 * 1024,
        max_recorded_failures: 16,
        max_recorded_warnings: 16,
        max_total_frames: 16_777_216,
        replay: ReplayLimits::MAX,
    };

    /// These limits with every field above its documented maximum lowered to that maximum: the
    /// limits a load actually applies.
    #[must_use]
    pub fn effective(&self) -> Self {
        let max = Self::MAX;
        Self {
            max_bytes: self.max_bytes.min(max.max_bytes),
            max_ticks: self.max_ticks.min(max.max_ticks),
            max_scenarios: self.max_scenarios.min(max.max_scenarios),
            max_expectations: self.max_expectations.min(max.max_expectations),
            max_parameters: self.max_parameters.min(max.max_parameters),
            max_mutants: self.max_mutants.min(max.max_mutants),
            max_mutant_bytes: self.max_mutant_bytes.min(max.max_mutant_bytes),
            max_recorded_failures: self.max_recorded_failures.min(max.max_recorded_failures),
            max_recorded_warnings: self.max_recorded_warnings.min(max.max_recorded_warnings),
            max_total_frames: self.max_total_frames.min(max.max_total_frames),
            replay: lowered(&self.replay),
        }
    }
}

/// `limits` with every field above [`ReplayLimits::MAX`] lowered to it, so the limits a scenario
/// replays under always pass `ReplayLimits::validate`.
fn lowered(limits: &ReplayLimits) -> ReplayLimits {
    let max = ReplayLimits::MAX;
    ReplayLimits {
        max_rows: limits.max_rows.min(max.max_rows),
        max_columns: limits.max_columns.min(max.max_columns),
        max_cell_bytes: limits.max_cell_bytes.min(max.max_cell_bytes),
        max_source_bytes: limits.max_source_bytes.min(max.max_source_bytes),
        max_steps: limits.max_steps.min(max.max_steps),
        max_frames: limits.max_frames.min(max.max_frames),
        max_trace_bytes: limits.max_trace_bytes.min(max.max_trace_bytes),
    }
}

impl Default for VerifyLimits {
    fn default() -> Self {
        Self::MAX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_documented_maxima() {
        let limits = VerifyLimits::default();
        assert_eq!(limits, VerifyLimits::MAX);
        assert_eq!(limits.max_bytes, 4_194_304);
        assert_eq!(limits.max_ticks, 16_777_216);
        assert_eq!(limits.max_scenarios, 4_096);
        assert_eq!(limits.max_expectations, 4_096);
        assert_eq!(limits.max_parameters, 256);
        assert_eq!(limits.max_mutants, 256);
        assert_eq!(limits.max_mutant_bytes, 67_108_864);
        assert_eq!(limits.max_recorded_failures, 16);
        assert_eq!(limits.max_recorded_warnings, 16);
        assert_eq!(limits.max_total_frames, 16_777_216);
        assert_eq!(limits.replay, ReplayLimits::MAX);
        assert_eq!(MAX_LOAD_WARNINGS, 64);
        assert_eq!(MAX_DEPTH, 8);
        assert_eq!(MAX_LIST_LEN, 65_536);
        assert_eq!(MAX_STRING_BYTES, 4_096);
        assert_eq!(MAX_TIME_INTEGER, 9_007_199_254_740_992);
    }

    #[test]
    fn a_limit_can_be_lowered_and_never_raised() {
        let lowered = VerifyLimits {
            max_bytes: 10,
            max_ticks: 0,
            max_scenarios: 1,
            max_expectations: 2,
            max_parameters: 3,
            max_mutants: 4,
            max_mutant_bytes: 6,
            max_recorded_failures: 0,
            max_recorded_warnings: 1,
            max_total_frames: 5,
            replay: ReplayLimits {
                max_rows: 7,
                max_columns: 0,
                max_cell_bytes: 1,
                max_source_bytes: 2,
                max_steps: 3,
                max_frames: 4,
                max_trace_bytes: 5,
            },
        };
        assert_eq!(lowered.effective(), lowered);
        let raised = VerifyLimits {
            max_bytes: u64::MAX,
            max_ticks: u64::MAX,
            max_scenarios: u32::MAX,
            max_expectations: u32::MAX,
            max_parameters: u32::MAX,
            max_mutants: u32::MAX,
            max_mutant_bytes: u64::MAX,
            max_recorded_failures: u32::MAX,
            max_recorded_warnings: u32::MAX,
            max_total_frames: u64::MAX,
            replay: ReplayLimits {
                max_rows: u64::MAX,
                max_columns: u32::MAX,
                max_cell_bytes: u32::MAX,
                max_source_bytes: u64::MAX,
                max_steps: u64::MAX,
                max_frames: u64::MAX,
                max_trace_bytes: u64::MAX,
            },
        };
        assert_eq!(raised.effective(), VerifyLimits::MAX);
        assert!(raised.effective().replay.validate().is_ok());
        // One past each maximum is still the maximum.
        let max = VerifyLimits::MAX;
        let one_past = VerifyLimits {
            max_bytes: max.max_bytes + 1,
            max_ticks: max.max_ticks + 1,
            max_scenarios: max.max_scenarios + 1,
            max_expectations: max.max_expectations + 1,
            max_parameters: max.max_parameters + 1,
            max_mutants: max.max_mutants + 1,
            max_mutant_bytes: max.max_mutant_bytes + 1,
            max_recorded_failures: max.max_recorded_failures + 1,
            max_recorded_warnings: max.max_recorded_warnings + 1,
            max_total_frames: max.max_total_frames + 1,
            replay: ReplayLimits {
                max_rows: max.replay.max_rows + 1,
                max_columns: max.replay.max_columns + 1,
                max_cell_bytes: max.replay.max_cell_bytes + 1,
                max_source_bytes: max.replay.max_source_bytes + 1,
                max_steps: max.replay.max_steps + 1,
                max_frames: max.replay.max_frames + 1,
                max_trace_bytes: max.replay.max_trace_bytes + 1,
            },
        };
        assert_eq!(one_past.effective(), VerifyLimits::MAX);
    }
}
