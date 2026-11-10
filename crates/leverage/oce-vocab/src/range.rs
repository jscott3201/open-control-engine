//! A parameter's range, rendered from the catalog's rules.
//!
//! The vocabulary never stores a bound (design note decision 8): the engine's rules are what an
//! author is judged against, so the text is rendered from `oce_api::CatalogEntry::param_rules`
//! every time a view is built. A **range** here is every constraint a rule places on a
//! parameter's *value*: a bound, a relation to another parameter, a relation whose violation
//! only warns, and a bound on the elements of an array parameter.
//!
//! Rules that say only what kind of value a parameter takes, how an array is shaped or that a
//! parameter drives the port structure render nothing: the parameter's type, its default, its
//! template name (`k_<i>`) and the resolved ports already say that. Every [`CatalogRule`] is
//! named in one total `match`, so a new rule kind does not compile until it is given a rendering
//! or listed with those.

use oce_api::{CatalogRule, PARAM_INDEX_PLACEHOLDER};

/// The constraints the catalog's rules place on the value of `parameter`, one line of plain
/// text per rule, in the rules' declaration order.
///
/// `rules` is a class's `oce_api::CatalogEntry::param_rules`; `parameter` is the CDL name
/// (`uLow`; `k` for the catalog's array template `k_<i>`, see
/// [`crate::parameter_base_name`]). A rule that relates two parameters is listed for both
/// (`uLow <= uHigh`). A relation the engine only warns about ends in `warns`
/// (`uLow == uHigh warns`); the authoring validator reports every engine warning as an error.
/// Real bounds are spelled as Rust prints the `f64` (`1e-13`, `0.0`), which reads back to the
/// same bits. Empty when no rule constrains the value. Pure; never panics.
#[must_use]
pub fn parameter_range(rules: &[CatalogRule], parameter: &str) -> Vec<String> {
    rules
        .iter()
        .filter_map(constraint)
        .filter(|constraint| constraint.about.contains(&parameter))
        .map(|constraint| constraint.text)
        .collect()
}

/// One rule as a value constraint: the parameters it constrains and its text.
struct Constraint {
    about: Vec<&'static str>,
    text: String,
}

fn on(name: &'static str, text: String) -> Option<Constraint> {
    Some(Constraint {
        about: vec![name],
        text,
    })
}

fn between(left: &'static str, right: &'static str, text: String) -> Option<Constraint> {
    Some(Constraint {
        about: vec![left, right],
        text,
    })
}

/// The value constraint a rule states, or `None` for a rule about type, shape or structure.
fn constraint(rule: &CatalogRule) -> Option<Constraint> {
    match *rule {
        // What kind of value the parameter takes: its type and default say so.
        CatalogRule::Required { .. }
        | CatalogRule::Boolean { .. }
        | CatalogRule::Real { .. }
        | CatalogRule::EnumMembers { .. }
        // Which parameters drive the ports: the resolved ports show it.
        | CatalogRule::Structural { .. }
        | CatalogRule::StructuralArrayElements { .. }
        // How an array is typed and shaped: the template name and the length parameter say so.
        | CatalogRule::IntegerArrayElements { .. }
        | CatalogRule::RealArrayElements { .. }
        | CatalogRule::BooleanArrayElements { .. }
        | CatalogRule::RealMatrixElements { .. }
        | CatalogRule::TimeTableMatrix { .. }
        | CatalogRule::TimeTableOffset { .. } => None,

        CatalogRule::RealFinite { name } => on(name, format!("{name} is finite")),
        CatalogRule::RealGreaterThan { name, min } => on(name, format!("{name} > {min:?}")),
        CatalogRule::RealFiniteGreaterThan { name, min } => {
            on(name, format!("{name} > {min:?} and finite"))
        }
        CatalogRule::RealGreaterOrEqual { name, min } => on(name, format!("{name} >= {min:?}")),
        CatalogRule::RealLessOrEqualConstant { name, max } => {
            on(name, format!("{name} <= {max:?}"))
        }
        CatalogRule::IntegerGreaterOrEqual { name, min } => on(name, format!("{name} >= {min}")),
        CatalogRule::IntegerLessOrEqualConstant { name, max } => {
            on(name, format!("{name} <= {max}"))
        }

        CatalogRule::RealLessOrEqual { lower, upper } => {
            between(lower, upper, format!("{lower} <= {upper}"))
        }
        CatalogRule::RealTimesIntegerInclusiveRange {
            real,
            integer,
            min,
            max,
        } => between(
            real,
            integer,
            format!("{min:?} <= {real} * {integer} <= {max:?}"),
        ),
        CatalogRule::IntegerProductLessOrEqualConstant { left, right, max } => {
            between(left, right, format!("{left} * {right} <= {max}"))
        }

        // The rule holds when the relation does; the text names the case that warns.
        CatalogRule::RealLessOrEqualWarning { lower, upper } => {
            between(lower, upper, format!("{lower} > {upper} warns"))
        }
        CatalogRule::RealGreaterOrEqualScaledWarning {
            left,
            right,
            factor,
        } => between(left, right, format!("{left} < {factor:?} * {right} warns")),
        CatalogRule::RealEqualWarning { left, right } => {
            between(left, right, format!("{left} == {right} warns"))
        }

        CatalogRule::IntegerArrayElementsInRange { base, min, max, .. } => on(
            base,
            format!("{min} <= {base}_{PARAM_INDEX_PLACEHOLDER} <= {max}"),
        ),
        CatalogRule::BooleanArrayTrueCountEquals { base, count, .. } => between(
            base,
            count,
            format!("the number of true {base}_{PARAM_INDEX_PLACEHOLDER} == {count}"),
        ),
    }
}

#[cfg(test)]
#[path = "range_tests.rs"]
mod tests;
