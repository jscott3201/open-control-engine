//! Output comparison policies for the facade-bound golden-trace driver.

use crate::{
    AlignedToleranceResult, ExactResult, FunnelResult, Indicator, Mask, Series, VerifyConfig,
    compare, compare_aligned_tolerance, compare_exact, compare_masked,
};

use super::{CapturedTrace, DriverError, Plan, SignalComparison};

/// Output comparison policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonMode {
    /// Funnel L1 tolerance-band comparison, including configured indicator masking.
    Funnel,
    /// Aligned finite-Real tolerance comparison with exact time samples and non-finite class checks.
    AlignedTolerance,
    /// Exact comparison for reference goldens.
    Exact,
}

/// Per-output comparison result.
#[derive(Clone, Debug, PartialEq)]
pub enum ComparisonResult {
    /// Funnel L1 tolerance-band verdict.
    Funnel(FunnelResult),
    /// Aligned tolerant verdict.
    AlignedTolerance(AlignedToleranceResult),
    /// Exact-comparison verdict.
    Exact(ExactResult),
}

impl ComparisonResult {
    /// Whether the comparison passed.
    #[must_use]
    pub fn passed(&self) -> bool {
        match self {
            Self::Funnel(result) => result.passed,
            Self::AlignedTolerance(result) => result.passed,
            Self::Exact(result) => result.passed,
        }
    }

    /// Number of points in the comparison domain.
    #[must_use]
    pub fn compared_points(&self) -> usize {
        match self {
            Self::Funnel(result) => result.compared_points,
            Self::AlignedTolerance(result) => result.compared_points,
            Self::Exact(result) => result.compared_points,
        }
    }
}

pub(super) fn compare_outputs(
    config: &VerifyConfig,
    plan: &Plan,
    trace: &CapturedTrace,
    mode: ComparisonMode,
) -> Result<Vec<SignalComparison>, DriverError> {
    let mut comparisons = Vec::with_capacity(plan.outputs.len());
    for output in &plan.outputs {
        let captured = trace
            .column(&output.point)
            .ok_or_else(|| DriverError::MissingCapturedColumn(output.point.clone()))?;
        let tolerance = config.tolerance_for_output(&output.point)?;
        let reference = Series {
            x: &plan.times,
            y: &output.reference_values,
        };
        let test = captured.as_series(&trace.times);
        let (masked, result) = match mode {
            ComparisonMode::Funnel => {
                let indicator_signals = config.indicator_signals_for_output(&output.point)?;
                let masked = !indicator_signals.is_empty();
                let result = if !masked {
                    let result = compare(reference, test, &tolerance);
                    if result.compared_points == 0 {
                        return Err(DriverError::NoComparedPoints {
                            output: output.point.clone(),
                        });
                    }
                    result
                } else {
                    let mut indicators = Vec::with_capacity(indicator_signals.len());
                    for signal in indicator_signals {
                        let column = trace
                            .column(&signal)
                            .ok_or_else(|| DriverError::MissingCapturedColumn(signal.clone()))?;
                        indicators.push(Indicator {
                            signal,
                            samples: trace
                                .times
                                .iter()
                                .copied()
                                .zip(column.values.iter().map(|value| *value != 0.0))
                                .collect(),
                        });
                    }
                    compare_masked(reference, test, &tolerance, &Mask { indicators })
                };
                (masked, ComparisonResult::Funnel(result))
            }
            ComparisonMode::AlignedTolerance => {
                let result = compare_aligned_tolerance(reference, test, output.kind, &tolerance);
                if result.compared_points == 0 {
                    return Err(DriverError::NoComparedPoints {
                        output: output.point.clone(),
                    });
                }
                (false, ComparisonResult::AlignedTolerance(result))
            }
            ComparisonMode::Exact => {
                let result = compare_exact(reference, test, output.kind);
                if result.compared_points == 0 {
                    return Err(DriverError::NoComparedPoints {
                        output: output.point.clone(),
                    });
                }
                (false, ComparisonResult::Exact(result))
            }
        };
        if result.compared_points() == 0 && !masked {
            return Err(DriverError::NoComparedPoints {
                output: output.point.clone(),
            });
        }
        comparisons.push(SignalComparison {
            output: output.point.clone(),
            reference_column: output.reference_column.clone(),
            tolerance,
            masked,
            result,
        });
    }
    Ok(comparisons)
}
