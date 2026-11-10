//! The bounds a vector set is read under (vectors note, "Bounds"; decision 35).
//!
//! Three are fixed by the strict profile and shared with the authoring wire form: nesting
//! ([`MAX_DEPTH`]), list length ([`MAX_LIST_LEN`]) and string length ([`MAX_STRING_BYTES`]). The
//! rest are [`VerifyLimits`]: each field is a default and a documented maximum at once. A caller
//! may lower any of them, to zero included; a value above the maximum is read as the maximum, so
//! no caller can raise one. Every bound is inclusive: a set *at* a bound loads, one unit past it
//! is refused with its own code, never truncated.

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

/// The bounds on one vector set that the strict profile does not fix.
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
}

impl VerifyLimits {
    /// The documented maxima, which are also the defaults.
    pub const MAX: Self = Self {
        max_bytes: 4 * 1024 * 1024,
        max_ticks: 16_777_216,
        max_scenarios: 4_096,
        max_expectations: 4_096,
        max_parameters: 256,
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
        }
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
        };
        assert_eq!(lowered.effective(), lowered);
        let raised = VerifyLimits {
            max_bytes: u64::MAX,
            max_ticks: u64::MAX,
            max_scenarios: u32::MAX,
            max_expectations: u32::MAX,
            max_parameters: u32::MAX,
        };
        assert_eq!(raised.effective(), VerifyLimits::MAX);
        // One past each maximum is still the maximum.
        let one_past = VerifyLimits {
            max_bytes: VerifyLimits::MAX.max_bytes + 1,
            max_ticks: VerifyLimits::MAX.max_ticks + 1,
            max_scenarios: VerifyLimits::MAX.max_scenarios + 1,
            max_expectations: VerifyLimits::MAX.max_expectations + 1,
            max_parameters: VerifyLimits::MAX.max_parameters + 1,
        };
        assert_eq!(one_past.effective(), VerifyLimits::MAX);
    }
}
