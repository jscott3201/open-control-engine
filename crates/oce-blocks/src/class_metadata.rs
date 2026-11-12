//! Per-class authoring metadata kept beside the revision-1 catalog (GitHub #440).
//!
//! Two facts an authoring tool needs and the catalog does not carry: a one-sentence description
//! of each class, and, for each parameter, its value type, its default and whether an author may
//! assign it. All of it is read from static tables and the registry's own defaults and rules; none
//! of it enters [`crate::catalog()`]'s entries, the `oce-api` canonical catalog JSON, or
//! [`crate::class_signature_hash`] (descriptions are not behaviour, and settability is a property
//! of the class's reservation, not of its computation).
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
//! # Value type
//!
//! A literal default gives the type directly. Otherwise the type comes from the class's
//! parameter rules (a `Required` rule's kind, an `Integer…` or `Real…` bound, an array rule's
//! element kind, an `EnumMembers` rule's member list matched to its CDL enumeration). Three
//! derived defaults no rule names are typed by a short table here, transcribed from the vendored
//! sources (all `Real`). A test requires every parameter of every class to be typed, and every
//! source of evidence for one parameter to agree.
//!
//! # Settable
//!
//! `settable` means "an author may assign this parameter" (GitHub #440, amendment of 2026-11-08):
//! false only for the parameters of the three engine-reserved lowering identities, which no
//! author can spell. A derived default (`MatrixGain.K`, `PIDWithReset.y_reset`, …) is a fact
//! about the default, carried in [`ClassParameter::default`], not about assignability: upstream
//! CDL declares every such parameter as a plain, overridable `parameter`, and the G36 corpus
//! binds three of them.

use alloc::vec::Vec;

use oce_model::{EnumClassId, ValueType, enum_descriptor};

use crate::class_descriptions::CLASS_DESCRIPTIONS;
use crate::class_signature::is_reserved;
use crate::{DefaultLiteral, DefaultSource, ParamRule, RegistryEntry, registry};

/// One parameter of a class, as an author sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClassParameter {
    /// CDL name, or the array template (`k_<i>`, `K_<row>_<col>`) for an array parameter.
    pub name: &'static str,
    /// The value type an assignment must have; for an array template, the element type.
    pub value_type: ValueType,
    /// The authored default: a literal, a derived formula, or [`DefaultSource::Required`] (no
    /// default; the author must assign it). The rule-only `table_<row>_<col>` is `Required`.
    pub default: DefaultSource,
    /// Whether an author may assign this parameter: false only for the parameters of an
    /// engine-reserved lowering identity. A derived default is still settable.
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

/// Every parameter of a registered class with its type, default and settable flag, or `None` for
/// an unregistered path. Allocates the returned list; never panics.
#[must_use]
pub fn class_parameters(class_path: &str) -> Option<Vec<ClassParameter>> {
    let entry = registry::lookup(class_path)?;
    let settable = !is_reserved(entry.class_path);
    let mut parameters: Vec<ClassParameter> = registry::param_defaults(entry.class_path)
        .iter()
        .map(|param| ClassParameter {
            name: param.name,
            value_type: parameter_type(entry, param.name, Some(param.default)),
            default: param.default,
            settable,
        })
        .collect();
    for rule in entry.param_rules() {
        for (name, _) in rule_parameters(rule).into_iter().flatten() {
            if !parameters.iter().any(|param| param.name == name) {
                parameters.push(ClassParameter {
                    name,
                    value_type: parameter_type(entry, name, None),
                    default: DefaultSource::Required,
                    settable,
                });
            }
        }
    }
    Some(parameters)
}

/// The value type of one class parameter (see the module documentation), or `Real` for a
/// parameter no evidence types. The `every_parameter_has_one_agreed_type` test requires
/// [`parameter_type_evidence`] to type every registered parameter, so the fallback is unreachable
/// for the registry as shipped.
fn parameter_type(entry: &RegistryEntry, name: &str, default: Option<DefaultSource>) -> ValueType {
    parameter_type_evidence(entry, name, default)
        .into_iter()
        .next()
        .unwrap_or(ValueType::Real)
}

/// Every type the registry's own data assigns to one parameter, literal default first, then each
/// rule in declaration order, then the untyped-derived table.
pub(crate) fn parameter_type_evidence(
    entry: &RegistryEntry,
    name: &str,
    default: Option<DefaultSource>,
) -> Vec<ValueType> {
    let mut evidence = Vec::new();
    if let Some(DefaultSource::Literal(literal)) = default {
        evidence.extend(literal_type(entry, name, literal));
    }
    for rule in entry.param_rules() {
        for (rule_name, value_type) in rule_parameters(rule).into_iter().flatten() {
            if rule_name == name {
                evidence.extend(value_type);
            }
        }
    }
    evidence.extend(
        UNRULED_DERIVED_TYPES
            .iter()
            .filter(|(class, param, _)| *class == entry.class_path && *param == name)
            .map(|(_, _, value_type)| *value_type),
    );
    evidence
}

/// Derived defaults that no parameter rule names, typed from their vendored declarations
/// (`parameter Real fallingSlewRate(...) = -raisingSlewRate`, `parameter Real y_reset =
/// xi_start`, `parameter Real falseHoldDuration(...) = trueHoldDuration`).
const UNRULED_DERIVED_TYPES: &[(&str, &str, ValueType)] = &[
    (
        "CDL.Reals.LimitSlewRate",
        "fallingSlewRate",
        ValueType::Real,
    ),
    ("CDL.Reals.PIDWithReset", "y_reset", ValueType::Real),
    (
        "CDL.Logical.TrueFalseHold",
        "falseHoldDuration",
        ValueType::Real,
    ),
];

/// The CDL enumerations a parameter of a registered class can take.
const CDL_ENUMS: [EnumClassId; 4] = [
    EnumClassId::SIMPLE_CONTROLLER,
    EnumClassId::SMOOTHNESS,
    EnumClassId::EXTRAPOLATION,
    EnumClassId::ZERO_TIME,
];

/// The CDL enumeration whose members are exactly `members`.
fn enum_with_members(members: &[&str]) -> Option<ValueType> {
    CDL_ENUMS
        .into_iter()
        .find(|class| enum_descriptor(*class).is_some_and(|d| d.members == members))
        .map(ValueType::Enum)
}

fn literal_type(entry: &RegistryEntry, name: &str, literal: DefaultLiteral) -> Option<ValueType> {
    Some(match literal {
        DefaultLiteral::Real(_) => ValueType::Real,
        DefaultLiteral::Integer(_) => ValueType::Integer,
        DefaultLiteral::Boolean(_) => ValueType::Boolean,
        DefaultLiteral::Str(_) => ValueType::String,
        // A member token is typed by the class's `EnumMembers` rule for the same parameter, or,
        // where the class has none (`PID.controllerType`), by the one CDL enumeration that
        // declares the member.
        DefaultLiteral::EnumMember(member) => {
            let by_rule = entry.param_rules().iter().find_map(|rule| match *rule {
                ParamRule::EnumMembers {
                    name: rule_name,
                    members,
                } if rule_name == name => enum_with_members(members),
                _ => None,
            });
            let mut declaring = CDL_ENUMS.into_iter().filter(|class| {
                enum_descriptor(*class).is_some_and(|d| d.members.contains(&member))
            });
            return by_rule.or_else(|| match (declaring.next(), declaring.next()) {
                (Some(class), None) => Some(ValueType::Enum(class)),
                _ => None,
            });
        }
    })
}

/// The parameters one rule names, as catalog names (array bases as templates), each with the
/// value type the rule implies, if it implies one. Exhaustive with no wildcard arm, so a new rule
/// variant fails compilation here until it is classified.
pub(crate) fn rule_parameters(rule: &ParamRule) -> [Option<(&'static str, Option<ValueType>)>; 4] {
    use ValueType::{Boolean, Integer, Real};
    let named = |name: &'static str, value_type: Option<ValueType>| Some((name, value_type));
    let vector =
        |base: &str, value_type: ValueType| vector_template(base).map(|t| (t, Some(value_type)));
    let matrix = |base: &str| matrix_template(base).map(|t| (t, Some(Real)));
    match *rule {
        ParamRule::Required { name, kind } => [named(name, Some(kind)), None, None, None],
        ParamRule::Structural { name } => [named(name, None), None, None, None],
        ParamRule::Boolean { name } => [named(name, Some(Boolean)), None, None, None],
        ParamRule::Real { name }
        | ParamRule::RealFinite { name }
        | ParamRule::RealGreaterThan { name, .. }
        | ParamRule::RealFiniteGreaterThan { name, .. }
        | ParamRule::RealGreaterOrEqual { name, .. }
        | ParamRule::RealLessOrEqualConstant { name, .. } => {
            [named(name, Some(Real)), None, None, None]
        }
        ParamRule::IntegerGreaterOrEqual { name, .. }
        | ParamRule::IntegerLessOrEqualConstant { name, .. } => {
            [named(name, Some(Integer)), None, None, None]
        }
        ParamRule::EnumMembers { name, members } => {
            [named(name, enum_with_members(members)), None, None, None]
        }
        ParamRule::StructuralArrayElements { base } => {
            [vector_template(base).map(|t| (t, None)), None, None, None]
        }
        ParamRule::IntegerArrayElements { base, len } => {
            [vector(base, Integer), named(len, Some(Integer)), None, None]
        }
        ParamRule::RealArrayElements { base, len } => {
            [vector(base, Real), named(len, Some(Integer)), None, None]
        }
        ParamRule::BooleanArrayElements { base, len } => {
            [vector(base, Boolean), named(len, Some(Integer)), None, None]
        }
        ParamRule::IntegerArrayElementsInRange { base, len, max, .. } => [
            vector(base, Integer),
            named(len, Some(Integer)),
            named(max, Some(Integer)),
            None,
        ],
        ParamRule::BooleanArrayTrueCountEquals {
            base, len, count, ..
        } => [
            vector(base, Boolean),
            named(len, Some(Integer)),
            named(count, Some(Integer)),
            None,
        ],
        ParamRule::RealMatrixElements {
            base, rows, cols, ..
        } => [
            matrix(base),
            named(rows, Some(Integer)),
            named(cols, Some(Integer)),
            None,
        ],
        ParamRule::TimeTableMatrix {
            base,
            time_scale,
            period,
            extrapolation,
            ..
        } => [
            matrix(base),
            named(time_scale, Some(Real)),
            period.and_then(|name| named(name, Some(Real))),
            extrapolation
                .and_then(|name| named(name, Some(ValueType::Enum(EnumClassId::EXTRAPOLATION)))),
        ],
        ParamRule::TimeTableOffset { base, table } => {
            [vector(base, Real), matrix(table), None, None]
        }
        ParamRule::RealTimesIntegerInclusiveRange { real, integer, .. } => [
            named(real, Some(Real)),
            named(integer, Some(Integer)),
            None,
            None,
        ],
        ParamRule::IntegerProductLessOrEqualConstant { left, right, .. } => [
            named(left, Some(Integer)),
            named(right, Some(Integer)),
            None,
            None,
        ],
        ParamRule::RealGreaterOrEqualScaledWarning { left, right, .. }
        | ParamRule::RealEqualWarning { left, right } => [
            named(left, Some(Real)),
            named(right, Some(Real)),
            None,
            None,
        ],
        ParamRule::RealLessOrEqual { lower, upper }
        | ParamRule::RealLessOrEqualWarning { lower, upper } => [
            named(lower, Some(Real)),
            named(upper, Some(Real)),
            None,
            None,
        ],
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
