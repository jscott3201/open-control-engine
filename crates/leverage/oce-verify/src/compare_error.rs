//! Why an envelope comparison gave no result (commissioning note, "Caps" and decision 22).
//!
//! A [`CompareError`] is one closed refusal with a stable kebab-case [`CompareError::code`] and
//! a sentence for people. The code is the contract: a test, a tool or an agent branches on it,
//! never on the sentence. [`COMPARE_ERROR_CODES`] lists every code in declaration order and a
//! test pins it. A refusal is never a verdict: a test series that leaves the band is a result
//! with an error above zero, not an error.

use std::fmt;

/// Which of the two series a refusal concerns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SeriesRole {
    /// The series the band is built around.
    Reference,
    /// The series checked against the band.
    Test,
}

impl SeriesRole {
    /// `reference` or `test`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Test => "test",
        }
    }
}

/// Which column of a series a refusal concerns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    /// The independent variable.
    X,
    /// The value.
    Y,
}

impl Axis {
    /// `x` or `y`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
        }
    }
}

/// Which end of the two series disagrees in `x`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SeriesEnd {
    /// The first points: the smallest `x` of each series.
    First,
    /// The last points: the largest `x` of each series.
    Last,
}

impl SeriesEnd {
    /// `first` or `last`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Last => "last",
        }
    }
}

/// What a comparison refused. Each variant has one stable code ([`CompareError::code`]);
/// `#[non_exhaustive]`, so consumers branch on the code.
///
/// The first six are decided before any arithmetic, in this order (the tolerances, then the
/// shape of each series, then its samples), so one input has one refusal.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum CompareError {
    /// A tolerance is negative, infinite or NaN; `name` is the first such field, in the order
    /// `atolx`, `atoly`, `rtolx`, `rtoly`, `ltolx`, `ltoly`.
    InvalidTolerance {
        /// The field of [`crate::Tolerances`].
        name: &'static str,
    },
    /// A series' `x` and `y` slices have different lengths.
    LengthMismatch {
        /// The series.
        series: SeriesRole,
        /// The length of its `x`.
        x_len: usize,
        /// The length of its `y`.
        y_len: usize,
    },
    /// A series has no point.
    EmptySeries {
        /// The series.
        series: SeriesRole,
    },
    /// A series has more points than the envelope's limit
    /// ([`crate::CompareLimits::max_funnel_points`] or
    /// [`crate::CompareLimits::max_aligned_grid_points`]), decided from its length.
    TooManyPoints {
        /// The series.
        series: SeriesRole,
        /// The points it has.
        points: usize,
        /// The limit applied, after [`crate::CompareLimits::effective`].
        limit: usize,
    },
    /// A sample is infinite or NaN; the first one of the series, `x` before `y` at one index.
    NonFiniteSample {
        /// The series.
        series: SeriesRole,
        /// The column.
        axis: Axis,
        /// The zero-based index of the sample.
        index: usize,
    },
    /// A series' `x` decreases: the sample at `index` is smaller than the one before it. Equal
    /// values are accepted (they mark a step).
    XNotNonDecreasing {
        /// The series.
        series: SeriesRole,
        /// The zero-based index of the first sample smaller than its predecessor.
        index: usize,
    },
    /// The reference and the test do not start, or do not end, at the same `x` (funnel's
    /// precondition, decided by its own equality: `|a - b| <= 1e-10 * max(1, |a|, |b|)`).
    /// [`crate::funnel()`] only.
    MismatchedEndpoints {
        /// The end that differs; the first is reported when both do.
        end: SeriesEnd,
        /// The reference's `x` there.
        reference: f64,
        /// The test's `x` there.
        test: f64,
    },
    /// Removing the loops of the funnel's bounds needed more than
    /// [`crate::CompareLimits::max_loop_removal_work`] units. [`crate::funnel()`] only.
    WorkLimitExceeded {
        /// The limit applied, after [`crate::CompareLimits::effective`].
        limit: u64,
    },
    /// funnel's loop removal defines no result for this reference: its backward search for the
    /// segment before a loop runs off the start of the corner list, where the C implementation
    /// reads before its array. A tolerance relative to `|x|` above 1 is what reaches it. Two
    /// further checks return this error and are defensive, with no input known to reach them:
    /// the forward search leaving the list, and a bound left with fewer than two corners.
    /// [`crate::funnel()`] only.
    LoopRemovalUndefined,
    /// The envelope's arithmetic left the finite numbers: a bound or an error is infinite or
    /// NaN, which happens only when samples or tolerances are near the largest binary64. A
    /// comparison against such a bound would read as inside the band, so it is refused.
    NonFiniteEnvelope,
}

/// Every [`CompareError::code`], in the declaration order of the variants. A test pins it.
pub const COMPARE_ERROR_CODES: &[&str] = &[
    "invalid-tolerance",
    "length-mismatch",
    "empty-series",
    "too-many-points",
    "non-finite-sample",
    "x-not-non-decreasing",
    "mismatched-endpoints",
    "work-limit-exceeded",
    "loop-removal-undefined",
    "non-finite-envelope",
];

impl CompareError {
    /// The refusal's stable kebab-case code, one of [`COMPARE_ERROR_CODES`].
    #[must_use]
    pub const fn code(&self) -> &'static str {
        COMPARE_ERROR_CODES[self.declared_index()]
    }

    /// The position of the variant in the declaration. The match is exhaustive, so a new variant
    /// does not compile until it has a row here, and [`COMPARE_ERROR_CODES`] a code at that
    /// position (a test holds the two the same length).
    const fn declared_index(&self) -> usize {
        match self {
            Self::InvalidTolerance { .. } => 0,
            Self::LengthMismatch { .. } => 1,
            Self::EmptySeries { .. } => 2,
            Self::TooManyPoints { .. } => 3,
            Self::NonFiniteSample { .. } => 4,
            Self::XNotNonDecreasing { .. } => 5,
            Self::MismatchedEndpoints { .. } => 6,
            Self::WorkLimitExceeded { .. } => 7,
            Self::LoopRemovalUndefined => 8,
            Self::NonFiniteEnvelope => 9,
        }
    }
}

impl fmt::Display for CompareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTolerance { name } => {
                write!(f, "tolerance `{name}` is negative or not finite")
            }
            Self::LengthMismatch {
                series,
                x_len,
                y_len,
            } => write!(
                f,
                "the {} series has {x_len} x values and {y_len} y values",
                series.as_str()
            ),
            Self::EmptySeries { series } => {
                write!(f, "the {} series has no point", series.as_str())
            }
            Self::TooManyPoints {
                series,
                points,
                limit,
            } => write!(
                f,
                "the {} series has {points} points, more than the limit of {limit}",
                series.as_str()
            ),
            Self::NonFiniteSample {
                series,
                axis,
                index,
            } => write!(
                f,
                "the {} series' {} at index {index} is not finite",
                series.as_str(),
                axis.as_str()
            ),
            Self::XNotNonDecreasing { series, index } => write!(
                f,
                "the {} series' x at index {index} is smaller than the one before it",
                series.as_str()
            ),
            Self::MismatchedEndpoints {
                end,
                reference,
                test,
            } => write!(
                f,
                "the {} x of the reference ({reference}) and of the test ({test}) differ",
                end.as_str()
            ),
            Self::WorkLimitExceeded { limit } => write!(
                f,
                "removing the loops of the bounds needs more than {limit} work units"
            ),
            Self::LoopRemovalUndefined => f.write_str(
                "funnel's loop removal defines no envelope for this reference and these tolerances",
            ),
            Self::NonFiniteEnvelope => f.write_str(
                "the envelope's arithmetic overflowed: a bound or an error is not finite",
            ),
        }
    }
}

impl std::error::Error for CompareError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// One error of every variant, in declaration order.
    fn one_of_each() -> Vec<CompareError> {
        vec![
            CompareError::InvalidTolerance { name: "atolx" },
            CompareError::LengthMismatch {
                series: SeriesRole::Reference,
                x_len: 2,
                y_len: 1,
            },
            CompareError::EmptySeries {
                series: SeriesRole::Test,
            },
            CompareError::TooManyPoints {
                series: SeriesRole::Test,
                points: 3,
                limit: 2,
            },
            CompareError::NonFiniteSample {
                series: SeriesRole::Reference,
                axis: Axis::Y,
                index: 4,
            },
            CompareError::XNotNonDecreasing {
                series: SeriesRole::Test,
                index: 1,
            },
            CompareError::MismatchedEndpoints {
                end: SeriesEnd::Last,
                reference: 1.0,
                test: 2.0,
            },
            CompareError::WorkLimitExceeded { limit: 9 },
            CompareError::LoopRemovalUndefined,
            CompareError::NonFiniteEnvelope,
        ]
    }

    #[test]
    fn every_variant_has_the_code_at_its_declared_position() {
        let errors = one_of_each();
        assert_eq!(errors.len(), COMPARE_ERROR_CODES.len());
        for (index, error) in errors.iter().enumerate() {
            assert_eq!(error.declared_index(), index, "{error:?}");
            assert_eq!(error.code(), COMPARE_ERROR_CODES[index], "{error:?}");
        }
    }

    #[test]
    fn codes_are_distinct_lowercase_kebab_case() {
        for (index, code) in COMPARE_ERROR_CODES.iter().enumerate() {
            assert!(
                code.split('-')
                    .all(|word| !word.is_empty()
                        && word.bytes().all(|byte| byte.is_ascii_lowercase())),
                "{code}"
            );
            assert!(!COMPARE_ERROR_CODES[..index].contains(code), "{code}");
        }
    }

    #[test]
    fn every_sentence_names_what_was_refused_without_the_code() {
        for error in one_of_each() {
            let sentence = error.to_string();
            assert!(!sentence.is_empty(), "{error:?}");
            assert!(!sentence.contains(error.code()), "{sentence}");
        }
        assert_eq!(
            CompareError::NonFiniteSample {
                series: SeriesRole::Reference,
                axis: Axis::Y,
                index: 4,
            }
            .to_string(),
            "the reference series' y at index 4 is not finite"
        );
        assert_eq!(
            CompareError::MismatchedEndpoints {
                end: SeriesEnd::Last,
                reference: 1.0,
                test: 2.5,
            }
            .to_string(),
            "the last x of the reference (1) and of the test (2.5) differ"
        );
    }
}
