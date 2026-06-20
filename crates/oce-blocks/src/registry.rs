//! The static class-path → constructor registry (`03` R-IMPL-2). `oce-cxf`/`oce-api` resolve a
//! flattened block instance's `class_iri` to a native [`Block`] impl via [`lookup`], then construct
//! it from the instance's resolved [`ParamTable`]. The catalog holds the **canonical** v9+
//! `Reals`/`Integers`/`Logical` class paths; resolving deprecated `Continuous.*` aliases to these
//! is `oce-validate`/`oce-cxf`'s job (M1, `03` §4), not a second alias table here.

use oce_model::{ParamTable, SimpleController, Value};

use crate::pid::ControllerConfig;
use crate::{
    Abs, Add, AddParameter, And, Block, BooleanToInteger, BooleanToReal, Constant, Derivative,
    Divide, Edge, Greater, GreaterThreshold, Hysteresis, IntegerAbs, IntegerAdd,
    IntegerAddParameter, IntegerConstant, IntegerGreater, IntegerGreaterEqual,
    IntegerGreaterEqualThreshold, IntegerGreaterThreshold, IntegerLess, IntegerLessEqual,
    IntegerLessEqualThreshold, IntegerLessThreshold, IntegerMax, IntegerMin, IntegerMultiply,
    IntegerMultiplyByParameter, IntegerSubtract, IntegerSwitch, IntegerToReal, IntegratorWithReset,
    Less, LessThreshold, LimitSlewRate, Limiter, Line, LogicalConstant, LogicalSwitch, Max, Min,
    MovingAverage, Multiply, MultiplyByParameter, Nand, Nor, Not, Or, Pid, PidWithReset, Pre,
    RealToInteger, RegistryEntry, SampleTrigger, Subtract, Switch, UnitDelay, Xor,
};

/// Look up an elementary-block constructor by canonical class path. Unknown paths return `None`
/// (an unresolved external / extension block — never a panic; R-IMPL-2).
#[must_use]
pub fn lookup(class_path: &str) -> Option<&'static RegistryEntry> {
    CATALOG.iter().find(|e| e.class_path == class_path)
}

/// The native catalog registered so far (`03` §7 Phase-1 core plus M2 Lane A additions).
static CATALOG: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Reals.Sources.Constant",
        make: make_constant,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Add",
        make: make_add,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Subtract",
        make: make_subtract,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Multiply",
        make: make_multiply,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Divide",
        make: make_divide,
    },
    RegistryEntry {
        class_path: "CDL.Reals.AddParameter",
        make: make_add_parameter,
    },
    RegistryEntry {
        class_path: "CDL.Reals.MultiplyByParameter",
        make: make_multiply_by_parameter,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Abs",
        make: make_abs,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Min",
        make: make_min,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Max",
        make: make_max,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Limiter",
        make: make_limiter,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Line",
        make: make_line,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Greater",
        make: make_greater,
    },
    RegistryEntry {
        class_path: "CDL.Reals.GreaterThreshold",
        make: make_greater_threshold,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Hysteresis",
        make: make_hysteresis,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Less",
        make: make_less,
    },
    RegistryEntry {
        class_path: "CDL.Reals.LessThreshold",
        make: make_less_threshold,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Switch",
        make: make_switch,
    },
    RegistryEntry {
        class_path: "CDL.Reals.IntegratorWithReset",
        make: make_integrator_with_reset,
    },
    RegistryEntry {
        class_path: "CDL.Reals.Derivative",
        make: make_derivative,
    },
    RegistryEntry {
        class_path: "CDL.Reals.LimitSlewRate",
        make: make_limit_slew_rate,
    },
    RegistryEntry {
        class_path: "CDL.Reals.MovingAverage",
        make: make_moving_average,
    },
    RegistryEntry {
        class_path: "CDL.Reals.PID",
        make: make_pid,
    },
    RegistryEntry {
        class_path: "CDL.Reals.PIDWithReset",
        make: make_pid_with_reset,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Sources.Constant",
        make: make_logical_constant,
    },
    RegistryEntry {
        class_path: "CDL.Logical.And",
        make: make_and,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Or",
        make: make_or,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Not",
        make: make_not,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Nand",
        make: make_nand,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Nor",
        make: make_nor,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Xor",
        make: make_xor,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Switch",
        make: make_logical_switch,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Pre",
        make: make_pre,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Edge",
        make: make_edge,
    },
    RegistryEntry {
        class_path: "CDL.Logical.Sources.SampleTrigger",
        make: make_sample_trigger,
    },
    RegistryEntry {
        class_path: "CDL.Conversions.BooleanToInteger",
        make: make_boolean_to_integer,
    },
    RegistryEntry {
        class_path: "CDL.Conversions.BooleanToReal",
        make: make_boolean_to_real,
    },
    RegistryEntry {
        class_path: "CDL.Conversions.IntegerToReal",
        make: make_integer_to_real,
    },
    RegistryEntry {
        class_path: "CDL.Conversions.RealToInteger",
        make: make_real_to_integer,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Sources.Constant",
        make: make_integer_constant,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Abs",
        make: make_integer_abs,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Add",
        make: make_integer_add,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Subtract",
        make: make_integer_subtract,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Multiply",
        make: make_integer_multiply,
    },
    RegistryEntry {
        class_path: "CDL.Integers.AddParameter",
        make: make_integer_add_parameter,
    },
    RegistryEntry {
        class_path: "CDL.Integers.MultiplyByParameter",
        make: make_integer_multiply_by_parameter,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Max",
        make: make_integer_max,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Min",
        make: make_integer_min,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Switch",
        make: make_integer_switch,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Greater",
        make: make_integer_greater,
    },
    RegistryEntry {
        class_path: "CDL.Integers.GreaterThreshold",
        make: make_integer_greater_threshold,
    },
    RegistryEntry {
        class_path: "CDL.Integers.GreaterEqual",
        make: make_integer_greater_equal,
    },
    RegistryEntry {
        class_path: "CDL.Integers.GreaterEqualThreshold",
        make: make_integer_greater_equal_threshold,
    },
    RegistryEntry {
        class_path: "CDL.Integers.Less",
        make: make_integer_less,
    },
    RegistryEntry {
        class_path: "CDL.Integers.LessThreshold",
        make: make_integer_less_threshold,
    },
    RegistryEntry {
        class_path: "CDL.Integers.LessEqual",
        make: make_integer_less_equal,
    },
    RegistryEntry {
        class_path: "CDL.Integers.LessEqualThreshold",
        make: make_integer_less_equal_threshold,
    },
    RegistryEntry {
        class_path: "CDL.Discrete.UnitDelay",
        make: make_unit_delay,
    },
];

// ---- parameter readers ----------------------------------------------------------------------

fn find<'a>(params: &'a ParamTable, name: &str) -> Option<&'a Value> {
    params
        .values
        .iter()
        .find(|(n, _)| n.as_ref() == name)
        .map(|(_, v)| v)
}

fn real_param(params: &ParamTable, name: &str, default: f64) -> f64 {
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

fn bool_param(params: &ParamTable, name: &str, default: bool) -> bool {
    match find(params, name) {
        Some(Value::Boolean(b)) => *b,
        _ => default,
    }
}

fn int_param(params: &ParamTable, name: &str, default: i64) -> i64 {
    match find(params, name) {
        Some(Value::Integer(n)) => *n,
        _ => default,
    }
}

fn controller_type_param(
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

// ---- constructors (one per catalog entry) ---------------------------------------------------

fn make_constant(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Constant {
        k: real_param(p, "k", 0.0),
    })
}

fn make_add(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Add)
}

fn make_subtract(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Subtract)
}

fn make_multiply(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Multiply)
}

fn make_divide(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Divide)
}

fn make_add_parameter(p: &ParamTable) -> Box<dyn Block> {
    Box::new(AddParameter {
        p: real_param(p, "p", 0.0),
    })
}

fn make_multiply_by_parameter(p: &ParamTable) -> Box<dyn Block> {
    Box::new(MultiplyByParameter {
        k: real_param(p, "k", 1.0),
    })
}

fn make_abs(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Abs)
}

fn make_min(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Min)
}

fn make_max(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Max)
}

fn make_limiter(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Limiter {
        u_min: real_param(p, "uMin", f64::NEG_INFINITY),
        u_max: real_param(p, "uMax", f64::INFINITY),
    })
}

fn make_line(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Line)
}

fn make_greater(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Greater {
        h: real_param(p, "h", 0.0),
        pre_y_start: bool_param(p, "pre_y_start", false),
    })
}

fn make_greater_threshold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(GreaterThreshold {
        t: real_param(p, "t", 0.0),
        h: real_param(p, "h", 0.0),
        pre_y_start: bool_param(p, "pre_y_start", false),
    })
}

fn make_hysteresis(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Hysteresis {
        u_low: real_param(p, "uLow", 0.0),
        u_high: real_param(p, "uHigh", 1.0),
        pre_y_start: bool_param(p, "pre_y_start", false),
    })
}

fn make_less(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Less {
        h: real_param(p, "h", 0.0),
        pre_y_start: bool_param(p, "pre_y_start", false),
    })
}

fn make_less_threshold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(LessThreshold {
        t: real_param(p, "t", 0.0),
        h: real_param(p, "h", 0.0),
        pre_y_start: bool_param(p, "pre_y_start", false),
    })
}

fn make_switch(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Switch)
}

fn make_integrator_with_reset(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegratorWithReset {
        y_start: real_param(p, "y_start", 0.0),
    })
}

fn make_derivative(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Derivative {
        k: real_param(p, "k", 1.0),
        t: real_param(p, "T", 0.1),
        y_start: real_param(p, "y_start", 0.0),
    })
}

fn make_limit_slew_rate(p: &ParamTable) -> Box<dyn Block> {
    let raising_slew_rate = real_param(p, "raisingSlewRate", 1.0);
    Box::new(LimitSlewRate {
        raising_slew_rate,
        falling_slew_rate: real_param(p, "fallingSlewRate", -raising_slew_rate),
        td: real_param(p, "Td", raising_slew_rate * 10.0),
        enable: bool_param(p, "enable", true),
    })
}

fn make_moving_average(p: &ParamTable) -> Box<dyn Block> {
    Box::new(MovingAverage {
        delta: real_param(p, "delta", 1.0),
    })
}

fn pid_config(p: &ParamTable) -> ControllerConfig {
    ControllerConfig {
        controller_type: controller_type_param(p, "controllerType", SimpleController::Pi),
        k: real_param(p, "k", 1.0),
        ti: real_param(p, "Ti", 0.5),
        td: real_param(p, "Td", 0.1),
        r: real_param(p, "r", 1.0),
        y_max: real_param(p, "yMax", 1.0),
        y_min: real_param(p, "yMin", 0.0),
        ni: real_param(p, "Ni", 0.9),
        nd: real_param(p, "Nd", 10.0),
        xi_start: real_param(p, "xi_start", 0.0),
        yd_start: real_param(p, "yd_start", 0.0),
        reverse_acting: bool_param(p, "reverseActing", true),
    }
}

fn make_pid(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Pid {
        config: pid_config(p),
    })
}

fn make_pid_with_reset(p: &ParamTable) -> Box<dyn Block> {
    Box::new(PidWithReset {
        config: pid_config(p),
    })
}

fn make_logical_constant(p: &ParamTable) -> Box<dyn Block> {
    Box::new(LogicalConstant {
        k: bool_param(p, "k", false),
    })
}

fn make_and(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(And)
}

fn make_or(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Or)
}

fn make_not(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Not)
}

fn make_nand(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Nand)
}

fn make_nor(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Nor)
}

fn make_xor(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(Xor)
}

fn make_logical_switch(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(LogicalSwitch)
}

fn make_pre(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Pre {
        y_start: bool_param(p, "pre_u_start", false),
    })
}

fn make_edge(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Edge {
        pre_u_start: bool_param(p, "pre_u_start", false),
    })
}

fn make_sample_trigger(p: &ParamTable) -> Box<dyn Block> {
    // `period` defaults to 1.0 only for a param-less construction; a resolved model carries the
    // author's value (CDL requires `period > 0`; the oce-validate rule enforcing it is pending —
    // SampleTrigger degrades safely until then). `shift` defaults to 0.0.
    Box::new(SampleTrigger {
        period: real_param(p, "period", 1.0),
        shift: real_param(p, "shift", 0.0),
    })
}

fn make_boolean_to_integer(p: &ParamTable) -> Box<dyn Block> {
    Box::new(BooleanToInteger {
        integer_true: int_param(p, "integerTrue", 1),
        integer_false: int_param(p, "integerFalse", 0),
    })
}

fn make_boolean_to_real(p: &ParamTable) -> Box<dyn Block> {
    Box::new(BooleanToReal {
        real_true: real_param(p, "realTrue", 1.0),
        real_false: real_param(p, "realFalse", 0.0),
    })
}

fn make_integer_to_real(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerToReal)
}

fn make_real_to_integer(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(RealToInteger)
}

fn make_integer_constant(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerConstant {
        k: int_param(p, "k", 0),
    })
}

fn make_integer_abs(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerAbs)
}

fn make_integer_add(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerAdd)
}

fn make_integer_subtract(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerSubtract)
}

fn make_integer_multiply(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerMultiply)
}

fn make_integer_add_parameter(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerAddParameter {
        p: int_param(p, "p", 0),
    })
}

fn make_integer_multiply_by_parameter(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerMultiplyByParameter {
        k: int_param(p, "k", 1),
    })
}

fn make_integer_max(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerMax)
}

fn make_integer_min(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerMin)
}

fn make_integer_switch(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerSwitch)
}

fn make_integer_greater(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerGreater)
}

fn make_integer_greater_threshold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerGreaterThreshold {
        t: int_param(p, "t", 0),
    })
}

fn make_integer_greater_equal(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerGreaterEqual)
}

fn make_integer_greater_equal_threshold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerGreaterEqualThreshold {
        t: int_param(p, "t", 0),
    })
}

fn make_integer_less(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerLess)
}

fn make_integer_less_threshold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerLessThreshold {
        t: int_param(p, "t", 0),
    })
}

fn make_integer_less_equal(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerLessEqual)
}

fn make_integer_less_equal_threshold(p: &ParamTable) -> Box<dyn Block> {
    Box::new(IntegerLessEqualThreshold {
        t: int_param(p, "t", 0),
    })
}

fn make_unit_delay(p: &ParamTable) -> Box<dyn Block> {
    Box::new(UnitDelay {
        y_start: real_param(p, "y_start", 0.0),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use oce_model::{EnumClassId, SimpleController, Value};

    use super::{controller_type_param, int_param};
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
}
