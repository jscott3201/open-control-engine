//! Indicator and don't-care masking for conformance comparison.
//!
//! CDL verification permits a boolean indicator signal to suspend checking for a variable. Indicator
//! samples use step-hold semantics: the last value at or before `t` is active until a later sample
//! changes it.

use crate::{FunnelResult, Series, Tolerances, compare};

/// A boolean signal used to gate checking of one or more output variables.
#[derive(Clone, Debug, PartialEq)]
pub struct Indicator {
    /// Captured indicator signal name, such as `fanSta.y`.
    pub signal: String,
    /// Step-held `(time, value)` samples. The value holds until the next sample.
    pub samples: Vec<(f64, bool)>,
}

impl Indicator {
    /// Return the step-held value at `t`.
    ///
    /// Before the first sample, or for an empty sample list, the indicator is inactive.
    #[must_use]
    pub fn active_at(&self, t: f64) -> bool {
        let mut active = false;
        for &(sample_t, value) in &self.samples {
            if sample_t > t {
                break;
            }
            active = value;
        }
        active
    }
}

/// A per-variable mask made of one or more ANDed indicators.
#[derive(Clone, Debug, PartialEq)]
pub struct Mask {
    /// Indicators that must all be active for the variable to be compared.
    pub indicators: Vec<Indicator>,
}

impl Mask {
    /// Return true when all indicators are active at `t`.
    ///
    /// A mask with no indicators is a no-op and is active at every timestamp.
    #[must_use]
    pub fn active_at(&self, t: f64) -> bool {
        self.indicators
            .iter()
            .all(|indicator| indicator.active_at(t))
    }

    /// Copy the active subset of `series` into caller-owned scratch storage.
    ///
    /// The same mask should be applied to both the reference and test series before funnel
    /// comparison so don't-care intervals cannot contribute to the error.
    pub fn apply<'a>(
        &self,
        series: Series<'a>,
        scratch_x: &'a mut Vec<f64>,
        scratch_y: &'a mut Vec<f64>,
    ) -> Series<'a> {
        scratch_x.clear();
        scratch_y.clear();
        for (x, y) in series.x.iter().copied().zip(series.y.iter().copied()) {
            if self.active_at(x) {
                scratch_x.push(x);
                scratch_y.push(y);
            }
        }
        Series {
            x: scratch_x,
            y: scratch_y,
        }
    }
}

/// Compare two series after applying the same don't-care mask to both sides.
#[must_use]
pub fn compare_masked(
    reference: Series<'_>,
    test: Series<'_>,
    tol: &Tolerances,
    mask: &Mask,
) -> FunnelResult {
    let mut ref_x = Vec::new();
    let mut ref_y = Vec::new();
    let mut test_x = Vec::new();
    let mut test_y = Vec::new();
    let masked_ref = mask.apply(reference, &mut ref_x, &mut ref_y);
    let masked_test = mask.apply(test, &mut test_x, &mut test_y);
    compare(masked_ref, masked_test, tol)
}
