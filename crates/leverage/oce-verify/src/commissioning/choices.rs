//! The closed choices a commissioning spec and a point mapping spell as strings (commissioning
//! note, "Test-spec file" and "Point mapping"): the funnel's reference series, a trend source's
//! format and a point's type. The fourth, the envelope, is [`crate::Envelope`], the type the
//! comparison itself takes, read here by its own spelling ([`crate::Envelope::from_name`]).
//!
//! Each is a closed enum with one spelling per variant, compared byte for byte: `Trend` is no
//! reference and `CSV` no format. A string outside the list is `unknown-value` at load, never a
//! default.

use std::fmt;

/// Which series a funnel is built around (commissioning note, decision 11). The envelope is not
/// symmetric, so the choice is part of the comparison and a receipt records it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FunnelReference {
    /// The engine's series: the CDL sequence is the specification. The default, and what a
    /// reference that writes no `funnelReference` asks for. Spelled `engine`.
    #[default]
    Engine,
    /// The trended series, as the funnel tool's own example has it. Spelled `trend`.
    Trend,
}

impl FunnelReference {
    /// Both roles, in declaration order.
    pub const ALL: [Self; 2] = [Self::Engine, Self::Trend];

    /// The role's spelling in a spec and a receipt: `engine` or `trend`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::Trend => "trend",
        }
    }

    /// The role spelled `name`, exactly; `None` for any other string.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.as_str() == name)
    }
}

impl fmt::Display for FunnelReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How a trend source's file is written (commissioning note, "A trend source").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrendFormat {
    /// Comma-separated values under a header row. Spelled `csv`.
    Csv,
    /// OBC §12.5.1's `#1` text, the Modelica Buildings library's trend format: one or more
    /// named matrices of numbers. Spelled `combiTimeTable`.
    CombiTimeTable,
}

impl TrendFormat {
    /// Both formats, in declaration order.
    pub const ALL: [Self; 2] = [Self::Csv, Self::CombiTimeTable];

    /// The format's spelling in a spec: `csv` or `combiTimeTable`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::CombiTimeTable => "combiTimeTable",
        }
    }

    /// The format spelled `name`, exactly; `None` for any other string.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|format| format.as_str() == name)
    }
}

impl fmt::Display for TrendFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The data type of one side of a mapped point (OBC §12.7 Listing 12.2: "the type refers to the
/// data type of the variable in the specific context, i.e., in CDL or in the actual
/// controller").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointType {
    /// A real number. Spelled `float`.
    Float,
    /// An integer. Spelled `int`.
    Int,
    /// A Boolean. Spelled `bool`.
    Bool,
}

impl PointType {
    /// The three types, in declaration order.
    pub const ALL: [Self; 3] = [Self::Float, Self::Int, Self::Bool];

    /// The type's spelling in a mapping: `float`, `int` or `bool`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Float => "float",
            Self::Int => "int",
            Self::Bool => "bool",
        }
    }

    /// The type spelled `name`, exactly; `None` for any other string.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }
}

impl fmt::Display for PointType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant's spelling reads back to the variant, and nothing near it does.
    fn spellings_round_trip<T: Copy + PartialEq + fmt::Debug + fmt::Display>(
        all: &[T],
        expected: &[&str],
        as_str: fn(T) -> &'static str,
        from_name: fn(&str) -> Option<T>,
    ) {
        let spelled: Vec<&str> = all.iter().map(|each| as_str(*each)).collect();
        assert_eq!(spelled, expected);
        for each in all {
            let name = as_str(*each);
            assert_eq!(from_name(name), Some(*each));
            assert_eq!(each.to_string(), name);
            for near in [
                name.to_uppercase(),
                format!("{name} "),
                format!(" {name}"),
                name[..name.len() - 1].to_owned(),
                format!("{name}s"),
            ] {
                assert_eq!(from_name(&near), None, "{near:?}");
            }
        }
        assert_eq!(from_name(""), None);
    }

    #[test]
    fn each_choice_has_one_spelling_compared_byte_for_byte() {
        // The envelope is the comparison's own type; a spec spells it the same way.
        spellings_round_trip(
            &crate::Envelope::ALL,
            &["funnel", "aligned-grid"],
            crate::Envelope::as_str,
            crate::Envelope::from_name,
        );
        spellings_round_trip(
            &FunnelReference::ALL,
            &["engine", "trend"],
            FunnelReference::as_str,
            FunnelReference::from_name,
        );
        spellings_round_trip(
            &TrendFormat::ALL,
            &["csv", "combiTimeTable"],
            TrendFormat::as_str,
            TrendFormat::from_name,
        );
        spellings_round_trip(
            &PointType::ALL,
            &["float", "int", "bool"],
            PointType::as_str,
            PointType::from_name,
        );
    }

    #[test]
    fn the_spellings_other_tools_use_are_not_accepted() {
        // The underscore form of the grid envelope, a capitalised format and the long type
        // names are each one edit from a spelling this format has.
        assert_eq!(crate::Envelope::from_name("aligned_grid"), None);
        assert_eq!(crate::Envelope::from_name("alignedGrid"), None);
        assert_eq!(TrendFormat::from_name("CombiTimeTable"), None);
        assert_eq!(TrendFormat::from_name("combitimetable"), None);
        assert_eq!(PointType::from_name("boolean"), None);
        assert_eq!(PointType::from_name("integer"), None);
        assert_eq!(PointType::from_name("real"), None);
        assert_eq!(FunnelReference::from_name("reference"), None);
    }

    #[test]
    fn the_engine_series_is_the_default_funnel_reference() {
        assert_eq!(FunnelReference::default(), FunnelReference::Engine);
    }
}
