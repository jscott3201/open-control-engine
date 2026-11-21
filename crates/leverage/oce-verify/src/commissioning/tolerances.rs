//! A tolerance object of a commissioning spec, and how several of them combine (commissioning
//! note, "Test-spec file": "key an output name or a glob, value partial tolerances" and
//! "overrides apply in declaration order, later fields winning").
//!
//! A spec writes tolerances in two places: once at the top level, for every output, and once per
//! `outputs` key, for the outputs that key selects. Each object gives any of six names and
//! leaves the rest unsaid, so the type here is partial: a name is either written or absent, and
//! an absent name is not zero until every object that applies to an output has been laid over
//! the one before it.

use crate::envelope::Tolerances;
use crate::number::Number;

/// The six tolerance names, in the order this crate lists them everywhere: sorted by their
/// bytes, which is the order the canonical form and a receipt write them in.
pub const TOLERANCE_NAMES: [&str; 6] = ["atolx", "atoly", "ltolx", "ltoly", "rtolx", "rtoly"];

/// One tolerance object as written: any of `atolx`, `atoly`, `ltolx`, `ltoly`, `rtolx` and
/// `rtoly`, each finite and at least zero.
///
/// `x` is the independent variable, time in seconds, and `y` the compared value in its CDL
/// unit. `atol` is absolute, `rtol` relative to the reference series' range and `ltol` relative
/// to the local value; how the three terms of an axis combine belongs to the envelope, not to
/// the spec. A name the object does not give is `None`, never zero: see
/// [`PartialTolerances::overlaid`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PartialTolerances {
    /// One slot per name of [`TOLERANCE_NAMES`], in that order.
    pub(crate) written: [Option<Number>; 6],
}

impl PartialTolerances {
    /// An object that gives no name: `{}`.
    pub const EMPTY: Self = Self { written: [None; 6] };

    /// The value written for `name`, one of [`TOLERANCE_NAMES`]; `None` when the object does
    /// not give it, and for any other name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<f64> {
        let slot = TOLERANCE_NAMES.iter().position(|known| *known == name)?;
        self.written[slot].map(Number::seconds)
    }

    /// Absolute tolerance along `x`, in seconds.
    #[must_use]
    pub fn atolx(&self) -> Option<f64> {
        self.written[0].map(Number::seconds)
    }

    /// Absolute tolerance along `y`, in the output's CDL unit.
    #[must_use]
    pub fn atoly(&self) -> Option<f64> {
        self.written[1].map(Number::seconds)
    }

    /// Tolerance along `x` relative to the local `|x|`: a dimensionless fraction.
    #[must_use]
    pub fn ltolx(&self) -> Option<f64> {
        self.written[2].map(Number::seconds)
    }

    /// Tolerance along `y` relative to the local `|y|`: a dimensionless fraction.
    #[must_use]
    pub fn ltoly(&self) -> Option<f64> {
        self.written[3].map(Number::seconds)
    }

    /// Tolerance along `x` relative to the reference's range in `x`: a dimensionless fraction.
    #[must_use]
    pub fn rtolx(&self) -> Option<f64> {
        self.written[4].map(Number::seconds)
    }

    /// Tolerance along `y` relative to the reference's range in `y`: a dimensionless fraction.
    #[must_use]
    pub fn rtoly(&self) -> Option<f64> {
        self.written[5].map(Number::seconds)
    }

    /// Whether the object gives no name.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.written.iter().all(Option::is_none)
    }

    /// This object with `later` laid over it: every name `later` gives takes `later`'s value,
    /// and every other name keeps this object's. Applying a spec's objects to one output in
    /// declaration order, the top-level `tolerances` first, is "later fields winning".
    #[must_use]
    pub fn overlaid(&self, later: &Self) -> Self {
        let mut written = self.written;
        for (slot, value) in written.iter_mut().zip(later.written) {
            if value.is_some() {
                *slot = value;
            }
        }
        Self { written }
    }

    /// The six tolerances a comparison takes ([`Tolerances`]), a name nothing gave read as
    /// `0.0`. Every field is finite and at least zero, which is what a comparison requires of
    /// them, so a tolerance a spec loads is never one an envelope refuses.
    #[must_use]
    pub fn resolved(&self) -> Tolerances {
        let [atolx, atoly, ltolx, ltoly, rtolx, rtoly] = self.or_zero();
        Tolerances {
            atolx,
            atoly,
            rtolx,
            rtoly,
            ltolx,
            ltoly,
        }
    }

    /// The six values in the order of [`TOLERANCE_NAMES`], a name nothing gave read as `0.0`
    /// (commissioning note, "Report and receipt": "a tolerance name that no tolerance object of
    /// the spec gives is written `0`"). Every value is finite and at least zero.
    #[must_use]
    pub fn or_zero(&self) -> [f64; 6] {
        self.written.map(|value| value.map_or(0.0, Number::seconds))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(pairs: &[(&str, f64)]) -> PartialTolerances {
        let mut written = [None; 6];
        for (name, value) in pairs {
            let slot = TOLERANCE_NAMES
                .iter()
                .position(|known| known == name)
                .expect("a tolerance name");
            written[slot] = Some(Number::Real(*value));
        }
        PartialTolerances { written }
    }

    #[test]
    fn the_names_are_sorted_and_each_accessor_reads_its_own_slot() {
        let mut sorted = TOLERANCE_NAMES;
        sorted.sort_unstable();
        assert_eq!(sorted, TOLERANCE_NAMES);
        let all = of(&[
            ("atolx", 1.0),
            ("atoly", 2.0),
            ("ltolx", 3.0),
            ("ltoly", 4.0),
            ("rtolx", 5.0),
            ("rtoly", 6.0),
        ]);
        assert_eq!(
            [
                all.atolx(),
                all.atoly(),
                all.ltolx(),
                all.ltoly(),
                all.rtolx(),
                all.rtoly()
            ],
            [1.0, 2.0, 3.0, 4.0, 5.0, 6.0].map(Some)
        );
        for (index, name) in TOLERANCE_NAMES.iter().enumerate() {
            let expected = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0][index];
            assert_eq!(all.get(name), Some(expected), "{name}");
        }
        assert_eq!(all.get("atol"), None);
        assert_eq!(all.get(""), None);
        assert_eq!(all.or_zero(), [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn the_resolved_tolerances_carry_each_name_to_the_field_of_that_name() {
        // Six different values: a slot that landed in another field would show.
        let all = of(&[
            ("atolx", 1.0),
            ("atoly", 2.0),
            ("ltolx", 3.0),
            ("ltoly", 4.0),
            ("rtolx", 5.0),
            ("rtoly", 6.0),
        ]);
        assert_eq!(
            all.resolved(),
            Tolerances {
                atolx: 1.0,
                atoly: 2.0,
                rtolx: 5.0,
                rtoly: 6.0,
                ltolx: 3.0,
                ltoly: 4.0,
            }
        );
        // What nothing gave is zero, the comparison's own "every tolerance zero".
        assert_eq!(PartialTolerances::EMPTY.resolved(), Tolerances::ZERO);
        let one = of(&[("rtoly", 0.01)]).resolved();
        assert_eq!(
            one,
            Tolerances {
                rtoly: 0.01,
                ..Tolerances::ZERO
            }
        );
    }

    #[test]
    fn an_absent_name_is_absent_until_it_is_read_as_zero() {
        let empty = PartialTolerances::EMPTY;
        assert!(empty.is_empty());
        assert_eq!(empty, PartialTolerances::default());
        assert_eq!(empty.atoly(), None);
        assert_eq!(empty.or_zero().map(f64::to_bits), [0.0_f64.to_bits(); 6]);
        let one = of(&[("atoly", 0.5)]);
        assert!(!one.is_empty());
        assert_eq!(one.or_zero(), [0.0, 0.5, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn a_later_object_wins_field_by_field_and_leaves_the_rest() {
        let global = of(&[
            ("rtolx", 0.002),
            ("rtoly", 0.002),
            ("atolx", 10.0),
            ("atoly", 0.0),
        ]);
        let first = of(&[("atoly", 0.5)]);
        let second = of(&[("atoly", 0.25), ("ltoly", 0.1)]);
        let merged = global.overlaid(&first).overlaid(&second);
        assert_eq!(merged.atolx(), Some(10.0), "the global value survives");
        assert_eq!(merged.atoly(), Some(0.25), "the later key wins");
        assert_eq!(merged.ltoly(), Some(0.1));
        assert_eq!(merged.rtolx(), Some(0.002));
        assert_eq!(merged.ltolx(), None, "nothing gave it");
        // The order matters: laid the other way, the earlier key's value is the last word.
        assert_eq!(global.overlaid(&second).overlaid(&first).atoly(), Some(0.5));
        // An explicit zero is a value: it overrides, where an absent name does not.
        let zeroed = of(&[("atolx", 0.0)]);
        assert_eq!(global.overlaid(&zeroed).atolx(), Some(0.0));
        assert_eq!(global.overlaid(&PartialTolerances::EMPTY), global);
        assert_eq!(PartialTolerances::EMPTY.overlaid(&global), global);
    }

    #[test]
    fn an_integer_token_and_a_negative_zero_read_as_plain_reals() {
        let mut written = [None; 6];
        written[0] = Some(Number::Integer(10));
        written[1] = Some(Number::Real(-0.0));
        let tolerances = PartialTolerances { written };
        assert_eq!(tolerances.atolx(), Some(10.0));
        // A tolerance carries no sign of its own.
        assert_eq!(
            tolerances.atoly().map(f64::to_bits),
            Some(0.0_f64.to_bits())
        );
    }
}
