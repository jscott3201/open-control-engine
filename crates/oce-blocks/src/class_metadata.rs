//! Per-class authoring metadata kept beside the revision-1 catalog (GitHub #440).
//!
//! Two facts an authoring tool needs and the catalog does not carry: a one-sentence description
//! of each class, and, for each parameter, whether an author may set it. Both are read from
//! static tables and the registry's own defaults and rules; neither enters
//! [`crate::catalog()`]'s entries, the `oce-api` canonical catalog JSON, or
//! [`crate::class_signature_hash`] (descriptions are not behaviour, and settability follows from
//! the already-hashed default source).
//!
//! # Which parameters a class has
//!
//! [`class_parameters`] lists the class's authored parameter defaults in declaration order, then
//! every parameter a parameter rule names that has no default entry, in rule order. Today the
//! second group is exactly the `table_<row>_<col>` matrix of the three `TimeTable` sources, which
//! is required (it has no default) and is named only by its `TimeTableMatrix` rule. Array
//! parameters appear once, under their `<i>` / `<row>_<col>` template (see
//! [`crate::ParamNameTemplate`]).
//!
//! # Settable
//!
//! A parameter is settable unless its default is [`DefaultSource::Derived`] (computed from other
//! parameters) or it belongs to an engine-reserved lowering identity, which no author can spell.
//! This is the owner's recorded rule for M2 (GitHub #440 and decision 18 of the authoring-layer
//! design note), not a claim about Modelica `final`: upstream CDL declares every derived parameter
//! listed here as an ordinary, overridable parameter.

use alloc::vec::Vec;

use crate::class_descriptions::CLASS_DESCRIPTIONS;
use crate::class_signature::is_reserved;
use crate::{DefaultSource, ParamRule, registry};

/// One parameter of a class, as an author sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassParameter {
    /// CDL name, or the array template (`k_<i>`, `K_<row>_<col>`) for an array parameter.
    pub name: &'static str,
    /// Whether an author may assign this parameter: false for a derived default and for every
    /// parameter of an engine-reserved class.
    pub settable: bool,
}

/// The one-sentence description of a registered class, or `None` for an unregistered path.
#[must_use]
pub fn class_description(class_path: &str) -> Option<&'static str> {
    CLASS_DESCRIPTIONS
        .iter()
        .find(|(path, _)| *path == class_path)
        .map(|(_, description)| *description)
}

/// Every parameter of a registered class with its settable flag, or `None` for an unregistered
/// path. Allocates the returned list; never panics.
#[must_use]
pub fn class_parameters(class_path: &str) -> Option<Vec<ClassParameter>> {
    let entry = registry::lookup(class_path)?;
    let reserved = is_reserved(entry.class_path);
    let mut parameters: Vec<ClassParameter> = registry::param_defaults(entry.class_path)
        .iter()
        .map(|param| ClassParameter {
            name: param.name,
            settable: !reserved && !matches!(param.default, DefaultSource::Derived { .. }),
        })
        .collect();
    for rule in entry.param_rules() {
        for name in rule_parameters(rule).into_iter().flatten() {
            if !parameters.iter().any(|param| param.name == name) {
                parameters.push(ClassParameter {
                    name,
                    settable: !reserved,
                });
            }
        }
    }
    Some(parameters)
}

/// The parameters one rule names, as catalog names (array bases as templates). Exhaustive with no
/// wildcard arm, so a new rule variant fails compilation here until it is classified.
pub(crate) fn rule_parameters(rule: &ParamRule) -> [Option<&'static str>; 4] {
    match *rule {
        ParamRule::Required { name, .. }
        | ParamRule::Structural { name }
        | ParamRule::Boolean { name }
        | ParamRule::Real { name }
        | ParamRule::RealFinite { name }
        | ParamRule::RealGreaterThan { name, .. }
        | ParamRule::RealFiniteGreaterThan { name, .. }
        | ParamRule::IntegerGreaterOrEqual { name, .. }
        | ParamRule::IntegerLessOrEqualConstant { name, .. }
        | ParamRule::EnumMembers { name, .. }
        | ParamRule::RealGreaterOrEqual { name, .. }
        | ParamRule::RealLessOrEqualConstant { name, .. } => [Some(name), None, None, None],
        ParamRule::StructuralArrayElements { base } => [vector_template(base), None, None, None],
        ParamRule::IntegerArrayElements { base, len }
        | ParamRule::RealArrayElements { base, len }
        | ParamRule::BooleanArrayElements { base, len } => {
            [vector_template(base), Some(len), None, None]
        }
        ParamRule::IntegerArrayElementsInRange { base, len, max, .. } => {
            [vector_template(base), Some(len), Some(max), None]
        }
        ParamRule::BooleanArrayTrueCountEquals {
            base, len, count, ..
        } => [vector_template(base), Some(len), Some(count), None],
        ParamRule::RealMatrixElements {
            base, rows, cols, ..
        } => [matrix_template(base), Some(rows), Some(cols), None],
        ParamRule::TimeTableMatrix {
            base,
            time_scale,
            period,
            extrapolation,
            ..
        } => [
            matrix_template(base),
            Some(time_scale),
            period,
            extrapolation,
        ],
        ParamRule::TimeTableOffset { base, table } => {
            [vector_template(base), matrix_template(table), None, None]
        }
        ParamRule::RealTimesIntegerInclusiveRange { real, integer, .. } => {
            [Some(real), Some(integer), None, None]
        }
        ParamRule::IntegerProductLessOrEqualConstant { left, right, .. }
        | ParamRule::RealGreaterOrEqualScaledWarning { left, right, .. }
        | ParamRule::RealEqualWarning { left, right } => [Some(left), Some(right), None, None],
        ParamRule::RealLessOrEqual { lower, upper }
        | ParamRule::RealLessOrEqualWarning { lower, upper } => {
            [Some(lower), Some(upper), None, None]
        }
    }
}

/// The catalog template for a vector array base. Every vector base a rule names is listed here, so
/// the template is a `'static` literal; an unlisted base yields `None` and the
/// `every_rule_named_array_has_a_template` test fails.
fn vector_template(base: &str) -> Option<&'static str> {
    Some(match base {
        "k" => "k_<i>",
        "extract" => "extract_<i>",
        "msk" => "msk_<i>",
        "offset" => "offset_<i>",
        _ => return None,
    })
}

/// The catalog template for a matrix array base; see [`vector_template`].
fn matrix_template(base: &str) -> Option<&'static str> {
    Some(match base {
        "K" => "K_<row>_<col>",
        "table" => "table_<row>_<col>",
        _ => return None,
    })
}
