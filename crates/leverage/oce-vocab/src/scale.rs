//! The unit scale-factor table (design note `docs/design/vocabulary.md`, "Units" and decision
//! 14).
//!
//! A closed table of unit symbols that are not base units, each an **exact positive integer
//! multiple** of one base unit, with no offset. v1's table is time only: `min`, `h` and `d`
//! against `s`. Affine units (`degC`, `degF`), `%` and prefixed SI units (`kPa`) are not in it,
//! by ruling: an offset is not a scale, and a silent conversion is the error this table exists
//! to prevent.
//!
//! This module only states the table. Scaling a written literal (exact decimal scaling with one
//! rounding to `f64`) and refusing a suffix that contradicts a declared unit belong to the text
//! form that reads the suffix.

/// One row of the scale-factor table: `1 symbol` is exactly `factor base`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScaleFactor {
    /// The scaled unit's symbol, as a unit suffix spells it (`h`).
    pub symbol: &'static str,
    /// The base unit it is a multiple of (`s`), in the spelling the CDL sources and the
    /// vocabulary's `unit` fields use.
    pub base: &'static str,
    /// How many base units one scaled unit is. Exact, positive and never zero.
    pub factor: u32,
}

/// The scale-factor table, in ascending order of factor. Closed: a symbol that is not here has
/// no scale factor.
pub const SCALE_FACTORS: &[ScaleFactor] = &[
    ScaleFactor {
        symbol: "min",
        base: "s",
        factor: 60,
    },
    ScaleFactor {
        symbol: "h",
        base: "s",
        factor: 3_600,
    },
    ScaleFactor {
        symbol: "d",
        base: "s",
        factor: 86_400,
    },
];

/// The row of `symbol`, or `None` when the symbol has no scale factor: a base unit (`s`), an
/// affine or prefixed unit, or anything else. The match is exact and case-sensitive (`H` and
/// `hr` are not `h`). Pure; never panics.
#[must_use]
pub fn scale_factor(symbol: &str) -> Option<&'static ScaleFactor> {
    SCALE_FACTORS.iter().find(|row| row.symbol == symbol)
}

#[cfg(test)]
mod tests {
    use super::{SCALE_FACTORS, ScaleFactor, scale_factor};

    #[test]
    fn the_table_is_time_only_with_the_three_exact_factors() {
        let rows: Vec<(&str, &str, u32)> = SCALE_FACTORS
            .iter()
            .map(|row| (row.symbol, row.base, row.factor))
            .collect();
        assert_eq!(
            rows,
            [("min", "s", 60), ("h", "s", 3_600), ("d", "s", 86_400)]
        );
        // The factors compose exactly: an hour is sixty minutes, a day twenty-four hours.
        let factor = |symbol| scale_factor(symbol).map(|row| row.factor);
        assert_eq!(factor("min").map(|minute| 60 * minute), factor("h"));
        assert_eq!(factor("h").map(|hour| 24 * hour), factor("d"));
    }

    #[test]
    fn every_row_is_a_positive_multiple_of_another_symbol_and_symbols_are_unique() {
        for (index, row) in SCALE_FACTORS.iter().enumerate() {
            assert!(row.factor > 1, "{row:?}");
            assert_ne!(row.symbol, row.base, "{row:?}");
            assert!(
                SCALE_FACTORS.iter().all(|other| other.symbol != row.base),
                "{row:?}: the base is itself scaled"
            );
            assert!(
                SCALE_FACTORS[..index]
                    .iter()
                    .all(|earlier| earlier.symbol != row.symbol && earlier.factor < row.factor),
                "{row:?}: repeated or out of order"
            );
            // Exact in binary64 with room to spare: every factor is far below 2^53.
            assert!(u64::from(row.factor) < (1 << 53));
        }
    }

    #[test]
    fn lookup_is_exact_and_refuses_everything_outside_the_table() {
        assert_eq!(
            scale_factor("h"),
            Some(&ScaleFactor {
                symbol: "h",
                base: "s",
                factor: 3_600
            })
        );
        assert_eq!(scale_factor("min").map(|row| row.factor), Some(60));
        assert_eq!(scale_factor("d").map(|row| row.factor), Some(86_400));
        for outside in [
            "", "s", "H", "hr", "h ", " h", "hh", "ms", "ks", "degC", "degF", "%", "kPa", "K",
            "Min", "day", "wk", "a",
        ] {
            assert_eq!(scale_factor(outside), None, "{outside:?}");
        }
    }
}
