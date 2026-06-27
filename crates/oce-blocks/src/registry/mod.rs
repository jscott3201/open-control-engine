//! Static class-path to constructor registry for native CDL block implementations.

use std::sync::Arc;

use oce_model::{ParamTable, SimpleController, Value};

use crate::{ParamRule, RegistryEntry};

mod conversions;
mod discrete;
mod integers;
mod logical;
mod logical_proof;
mod logical_timing;
mod pid;
mod reals;
mod reals_filters;
mod reals_integrator;
mod utilities;

#[cfg(test)]
mod catalog_guard_support;
#[cfg(test)]
mod catalog_tests;

/// Look up an elementary-block constructor by canonical class path. Unknown paths return `None`
/// (an unresolved external / extension block — never a panic; R-IMPL-2).
#[must_use]
pub fn lookup(class_path: &str) -> Option<&'static RegistryEntry> {
    CATALOG
        .iter()
        .flat_map(|entries| entries.iter())
        .find(|e| e.class_path == class_path)
}

pub(crate) fn param_rules(class_path: &str) -> &'static [ParamRule] {
    match class_path {
        "CDL.Logical.Sources.SampleTrigger" => logical::SAMPLE_TRIGGER_PARAM_RULES,
        "CDL.Logical.Proof" => logical_proof::PROOF_PARAM_RULES,
        "CDL.Reals.Limiter" => reals::LIMITER_PARAM_RULES,
        "CDL.Reals.Derivative" => reals_filters::DERIVATIVE_PARAM_RULES,
        "CDL.Reals.LimitSlewRate" => reals_filters::LIMIT_SLEW_RATE_PARAM_RULES,
        "CDL.Reals.MovingAverage" => reals_filters::MOVING_AVERAGE_PARAM_RULES,
        "CDL.Reals.PID" => pid::PID_PARAM_RULES,
        "CDL.Reals.PIDWithReset" => pid::PID_WITH_RESET_PARAM_RULES,
        _ => &[],
    }
}

static CATALOG: &[&[RegistryEntry]] = &[
    reals::ENTRIES,
    reals_integrator::ENTRIES,
    reals_filters::ENTRIES,
    pid::ENTRIES,
    logical::ENTRIES,
    logical_proof::ENTRIES,
    logical_timing::ENTRIES,
    conversions::ENTRIES,
    integers::ENTRIES,
    discrete::ENTRIES,
    utilities::ENTRIES,
];

// ---- parameter readers ----------------------------------------------------------------------

fn find<'a>(params: &'a ParamTable, name: &str) -> Option<&'a Value> {
    params
        .values
        .iter()
        .find(|(n, _)| n.as_ref() == name)
        .map(|(_, v)| v)
}

pub(crate) fn real_param(params: &ParamTable, name: &str, default: f64) -> f64 {
    match find(params, name) {
        Some(Value::Real(x)) => *x,
        // Modelica/CDL Int→Real promotion (§7.3.4): an integer literal bound to a `Real` parameter
        // is its real value. CXF may carry a bare integer (e.g. `y_start: 0`) for a Real parameter
        // when no `isOfDataType` re-types it; WITHOUT this arm such a binding would silently fall
        // through to `default`, discarding the author's value (a safety-critical silent wrong value
        // — e.g. a non-zero `UnitDelay.y_start` initial state). `i64 as f64` is the lossless/CDL
        // promotion for the ±2³¹ Integer domain.
        Some(Value::Integer(i)) => *i as f64,
        _ => default,
    }
}

pub(crate) fn bool_param(params: &ParamTable, name: &str, default: bool) -> bool {
    match find(params, name) {
        Some(Value::Boolean(b)) => *b,
        _ => default,
    }
}

pub(crate) fn int_param(params: &ParamTable, name: &str, default: i64) -> i64 {
    match find(params, name) {
        Some(Value::Integer(n)) => *n,
        _ => default,
    }
}

pub(crate) fn string_param(params: &ParamTable, name: &str, default: &'static str) -> Arc<str> {
    match find(params, name) {
        Some(Value::String(s)) => Arc::clone(s),
        _ => Arc::from(default),
    }
}

pub(super) fn controller_type_param(
    params: &ParamTable,
    name: &str,
    default: SimpleController,
) -> SimpleController {
    match find(params, name) {
        Some(Value::Enum { ordinal: 1, .. }) => SimpleController::P,
        Some(Value::Enum { ordinal: 2, .. }) => SimpleController::Pi,
        Some(Value::Enum { ordinal: 3, .. }) => SimpleController::Pd,
        Some(Value::Enum { .. }) => SimpleController::Pid,
        Some(Value::String(s)) => SimpleController::from_qualified(s).unwrap_or(default),
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use oce_model::{EnumClassId, SimpleController, Value};

    use super::{controller_type_param, int_param, string_param};
    use crate::ParamTable;

    #[test]
    fn int_param_reads_integer_without_real_promotion() {
        let params = ParamTable {
            values: vec![
                (Arc::from("n"), Value::Integer(7)),
                (Arc::from("real"), Value::Real(7.0)),
            ],
        };
        assert_eq!(int_param(&params, "n", -1), 7);
        assert_eq!(
            int_param(&params, "real", -1),
            -1,
            "Integer params must not accept Int->Real promotion in reverse"
        );
        assert_eq!(int_param(&params, "missing", 42), 42);
    }

    #[test]
    fn controller_type_param_reads_grounded_enum_ordinals() {
        let class = EnumClassId(9);
        let cases = [
            (1, SimpleController::P),
            (2, SimpleController::Pi),
            (3, SimpleController::Pd),
            (4, SimpleController::Pid),
            (99, SimpleController::Pid),
        ];
        for (ordinal, want) in cases {
            let params = ParamTable {
                values: vec![(Arc::from("controllerType"), Value::Enum { class, ordinal })],
            };
            assert_eq!(
                controller_type_param(&params, "controllerType", SimpleController::P),
                want
            );
        }
    }

    #[test]
    fn controller_type_param_accepts_qualified_string_fallback() {
        let params = ParamTable {
            values: vec![(
                Arc::from("controllerType"),
                Value::String(Arc::from(
                    "Buildings.Controls.OBC.CDL.Types.SimpleController.PID",
                )),
            )],
        };
        assert_eq!(
            controller_type_param(&params, "controllerType", SimpleController::P),
            SimpleController::Pid
        );

        let unknown = ParamTable {
            values: vec![(
                Arc::from("controllerType"),
                Value::String(Arc::from(
                    "Buildings.Controls.OBC.CDL.Types.SimpleController.X",
                )),
            )],
        };
        assert_eq!(
            controller_type_param(&unknown, "controllerType", SimpleController::Pd),
            SimpleController::Pd
        );
    }

    #[test]
    fn string_param_reads_string_without_coercion() {
        let params = ParamTable {
            values: vec![
                (Arc::from("message"), Value::String(Arc::from("trip"))),
                (Arc::from("other"), Value::Boolean(true)),
            ],
        };
        assert_eq!(string_param(&params, "message", "default").as_ref(), "trip");
        assert_eq!(
            string_param(&params, "other", "default").as_ref(),
            "default",
            "String params must not coerce other value kinds"
        );
        assert_eq!(
            string_param(&params, "missing", "default").as_ref(),
            "default"
        );
    }
}
