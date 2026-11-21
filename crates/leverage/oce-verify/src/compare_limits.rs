//! The bounds one envelope comparison runs under (commissioning note, "Caps"; decision 22).
//!
//! Each field of [`CompareLimits`] is a default and a documented maximum at once, as
//! [`crate::VerifyLimits`] is: a caller may lower any of them, to zero included, and a value
//! above the maximum is read as the maximum, so no caller can raise one. Every bound is
//! inclusive: an input *at* a bound is compared, one past it is refused with a typed
//! [`crate::CompareError`], never truncated. None of them is a wall-clock time.

/// The bounds on one comparison of a test series against a reference.
///
/// `Default` is the documented maxima ([`CompareLimits::MAX`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompareLimits {
    /// Points in the reference, and in the test, for [`crate::funnel()`]. Maximum 1,048,576; past
    /// it the refusal is `too-many-points`, decided from the lengths before a sample is read.
    /// The funnel's memory and its work outside loop removal are linear in these.
    pub max_funnel_points: usize,
    /// Points in the reference, and in the test, for [`crate::aligned_grid()`]. Maximum 32,768
    /// (one day at 5 s is 17,281); past it the refusal is `too-many-points`, decided from the
    /// lengths. The limit is this low because that envelope tests every grid point against
    /// every rectangle: its work is quadratic in the reference's points.
    pub max_aligned_grid_points: usize,
    /// Work units [`crate::funnel()`] may spend removing loops from its two bounds together: one
    /// unit for each corner a search step examines and each corner moved or dropped. Maximum
    /// 2^28 = 268,435,456; past it the refusal is `work-limit-exceeded`. It is a count, never
    /// a wall-clock time, and one budget for the whole comparison, whatever its length.
    ///
    /// The cost per reference point grows with the width of the `x` tolerance **in samples**,
    /// because every loop's search walks the corners a rectangle reaches back over. Measured
    /// with this counter on a random walk sampled once per unit of `x`: 9 units per point for
    /// an `x` tolerance one sample wide, 18 at 10 samples, 28 at 35, 43 at 100, 70 at 300, 123
    /// at 1,000 and 205 at 3,000; white noise costs 11 to 43 over the same widths and a
    /// regular sawtooth 3 to 4. So a day at 5 s (17,281 points) uses under 1 % of the maximum
    /// at any of these widths, while a series at the point cap has 256 units per point: the
    /// random walk of 1,048,576 points uses 27 % at 300 samples, 48 % at 1,000 and 92 % at
    /// 4,000, and is refused at 5,000. Other signals reach the bound sooner (the review of
    /// GitHub #687 measured 133 to 177 units per point at 1,000 samples on its own, and
    /// refusal from about 1,500). A long trend compared under a very wide `x` tolerance is therefore refused, not
    /// ground through; resample it, or split it.
    ///
    /// Apart from width, the searches can be quadratic in the points themselves: a rising
    /// sawtooth under a local `x` tolerance costs about 0.7 units per point squared, and the
    /// maximum refuses it from about 19,400 points.
    pub max_loop_removal_work: u64,
}

impl CompareLimits {
    /// The documented maxima, which are also the defaults.
    pub const MAX: Self = Self {
        max_funnel_points: 1_048_576,
        max_aligned_grid_points: 32_768,
        max_loop_removal_work: 1 << 28,
    };

    /// These limits with every field above its documented maximum lowered to that maximum: the
    /// limits a comparison actually applies.
    #[must_use]
    pub fn effective(&self) -> Self {
        let max = Self::MAX;
        Self {
            max_funnel_points: self.max_funnel_points.min(max.max_funnel_points),
            max_aligned_grid_points: self
                .max_aligned_grid_points
                .min(max.max_aligned_grid_points),
            max_loop_removal_work: self.max_loop_removal_work.min(max.max_loop_removal_work),
        }
    }
}

impl Default for CompareLimits {
    fn default() -> Self {
        Self::MAX
    }
}

#[cfg(test)]
mod tests {
    use super::CompareLimits;

    #[test]
    fn the_default_is_the_documented_maxima() {
        let limits = CompareLimits::default();
        assert_eq!(limits, CompareLimits::MAX);
        assert_eq!(limits.max_funnel_points, 1_048_576);
        assert_eq!(limits.max_aligned_grid_points, 32_768);
        assert_eq!(limits.max_loop_removal_work, 268_435_456);
    }

    #[test]
    fn a_limit_can_be_lowered_and_never_raised() {
        let raised = CompareLimits {
            max_funnel_points: usize::MAX,
            max_aligned_grid_points: usize::MAX,
            max_loop_removal_work: u64::MAX,
        };
        assert_eq!(raised.effective(), CompareLimits::MAX);

        let one_past = CompareLimits {
            max_funnel_points: CompareLimits::MAX.max_funnel_points + 1,
            max_aligned_grid_points: CompareLimits::MAX.max_aligned_grid_points + 1,
            max_loop_removal_work: CompareLimits::MAX.max_loop_removal_work + 1,
        };
        assert_eq!(one_past.effective(), CompareLimits::MAX);

        let lowered = CompareLimits {
            max_funnel_points: 0,
            max_aligned_grid_points: 7,
            max_loop_removal_work: 0,
        };
        assert_eq!(lowered.effective(), lowered);
    }
}
