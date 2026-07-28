//! Per-class dispatch for authored parameter defaults.

use crate::ParamDefault;

use super::{
    conversions, discrete, integers, logical, logical_proof, logical_timing,
    logical_variable_pulse, pid, psychrometrics, reals_defaults, reals_filters, reals_integrator,
    reals_ramp, routing, utilities,
};

pub(crate) fn param_defaults(class_path: &str) -> &'static [ParamDefault] {
    match class_path {
        "CDL.Conversions.BooleanToInteger" => conversions::BOOLEAN_TO_INTEGER_PARAM_DEFAULTS,
        "CDL.Conversions.BooleanToReal" => conversions::BOOLEAN_TO_REAL_PARAM_DEFAULTS,
        "CDL.Discrete.FirstOrderHold" | "CDL.Discrete.Sampler" | "CDL.Discrete.ZeroOrderHold" => {
            discrete::SAMPLED_PARAM_DEFAULTS
        }
        "CDL.Discrete.TriggeredMovingMean" => discrete::TRIGGERED_MOVING_MEAN_PARAM_DEFAULTS,
        "CDL.Discrete.TriggeredSampler" => discrete::TRIGGERED_SAMPLER_PARAM_DEFAULTS,
        "CDL.Discrete.UnitDelay" => discrete::UNIT_DELAY_PARAM_DEFAULTS,
        "CDL.Integers.Sources.Constant" => integers::INTEGER_CONSTANT_PARAM_DEFAULTS,
        "CDL.Integers.Sources.Pulse" => integers::INTEGER_PULSE_PARAM_DEFAULTS,
        "CDL.Integers.Sources.TimeTable" => integers::INTEGER_TIME_TABLE_PARAM_DEFAULTS,
        "CDL.Integers.AddParameter" => integers::INTEGER_ADD_PARAMETER_PARAM_DEFAULTS,
        "CDL.Integers.MultiSum" => integers::INTEGER_MULTI_SUM_PARAM_DEFAULTS,
        "CDL.Integers.GreaterThreshold"
        | "CDL.Integers.GreaterEqualThreshold"
        | "CDL.Integers.LessThreshold"
        | "CDL.Integers.LessEqualThreshold" => integers::INTEGER_THRESHOLD_PARAM_DEFAULTS,
        "CDL.Integers.OnCounter" => integers::INTEGER_ON_COUNTER_PARAM_DEFAULTS,
        "CDL.Integers.Change" => integers::INTEGER_CHANGE_PARAM_DEFAULTS,
        "CDL.Integers.Stage" => integers::INTEGER_STAGE_PARAM_DEFAULTS,
        "CDL.Logical.Sources.Constant" => logical::LOGICAL_CONSTANT_PARAM_DEFAULTS,
        "CDL.Logical.Sources.Pulse" => logical::LOGICAL_PULSE_PARAM_DEFAULTS,
        "CDL.Logical.Sources.TimeTable" => logical::LOGICAL_TIME_TABLE_PARAM_DEFAULTS,
        "CDL.Logical.MultiAnd" | "CDL.Logical.MultiOr" => logical::MULTI_LOGICAL_PARAM_DEFAULTS,
        "CDL.Logical.Pre" | "CDL.Logical.Edge" => logical::PRE_PARAM_DEFAULTS,
        "CDL.Logical.Sources.SampleTrigger" => logical::SAMPLE_TRIGGER_PARAM_DEFAULTS,
        "CDL.Logical.Proof" => logical_proof::PROOF_PARAM_DEFAULTS,
        "CDL.Logical.VariablePulse" => logical_variable_pulse::VARIABLE_PULSE_PARAM_DEFAULTS,
        "CDL.Logical.FallingEdge" | "CDL.Logical.Change" => logical_timing::EDGE_PARAM_DEFAULTS,
        "CDL.Logical.Timer" | "CDL.Logical.TimerAccumulating" => {
            logical_timing::TIMER_PARAM_DEFAULTS
        }
        "CDL.Logical.TrueDelay" => logical_timing::TRUE_DELAY_PARAM_DEFAULTS,
        "CDL.Logical.TrueFalseHold" => logical_timing::TRUE_FALSE_HOLD_PARAM_DEFAULTS,
        "CDL.Logical.TrueHoldWithReset" => logical_timing::TRUE_HOLD_WITH_RESET_PARAM_DEFAULTS,
        "CDL.Reals.PID" => pid::PID_PARAM_DEFAULTS,
        "CDL.Reals.PIDWithReset" => pid::PID_WITH_RESET_PARAM_DEFAULTS,
        "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi" => {
            psychrometrics::SPECIFIC_ENTHALPY_PARAM_DEFAULTS
        }
        "CDL.Reals.Sources.Constant" => reals_defaults::CONSTANT_PARAM_DEFAULTS,
        "CDL.Reals.Sources.Pulse" => reals_defaults::PULSE_PARAM_DEFAULTS,
        "CDL.Reals.Sources.Ramp" => reals_defaults::SOURCE_RAMP_PARAM_DEFAULTS,
        "CDL.Reals.Sources.Sin" => reals_defaults::SOURCE_SIN_PARAM_DEFAULTS,
        "CDL.Reals.Sources.CalendarTime" => reals_defaults::CALENDAR_TIME_PARAM_DEFAULTS,
        "CDL.Reals.Round" => reals_defaults::ROUND_PARAM_DEFAULTS,
        "CDL.Reals.AddParameter" => reals_defaults::ADD_PARAMETER_PARAM_DEFAULTS,
        "CDL.Reals.MultiplyByParameter" => reals_defaults::MULTIPLY_BY_PARAMETER_PARAM_DEFAULTS,
        "CDL.Reals.MultiMax" | "CDL.Reals.MultiMin" => reals_defaults::MULTI_REAL_PARAM_DEFAULTS,
        "CDL.Reals.MultiSum" => reals_defaults::MULTI_SUM_PARAM_DEFAULTS,
        "CDL.Reals.MatrixGain" => reals_defaults::MATRIX_GAIN_PARAM_DEFAULTS,
        "CDL.Reals.MatrixMax" => reals_defaults::MATRIX_MAX_PARAM_DEFAULTS,
        "CDL.Reals.MatrixMin" => reals_defaults::MATRIX_MIN_PARAM_DEFAULTS,
        "CDL.Reals.Sort" => reals_defaults::SORT_PARAM_DEFAULTS,
        "CDL.Reals.Limiter" => reals_defaults::LIMITER_PARAM_DEFAULTS,
        "CDL.Reals.Line" => reals_defaults::LINE_PARAM_DEFAULTS,
        "CDL.Reals.Greater" | "CDL.Reals.Less" => reals_defaults::COMPARATOR_PARAM_DEFAULTS,
        "CDL.Reals.GreaterThreshold" | "CDL.Reals.LessThreshold" => {
            reals_defaults::THRESHOLD_COMPARATOR_PARAM_DEFAULTS
        }
        "CDL.Reals.Hysteresis" => reals_defaults::HYSTERESIS_PARAM_DEFAULTS,
        "CDL.Reals.Derivative" => reals_filters::DERIVATIVE_PARAM_DEFAULTS,
        "CDL.Reals.LimitSlewRate" => reals_filters::LIMIT_SLEW_RATE_PARAM_DEFAULTS,
        "CDL.Reals.MovingAverage" => reals_filters::MOVING_AVERAGE_PARAM_DEFAULTS,
        "CDL.Reals.IntegratorWithReset" => reals_integrator::INTEGRATOR_WITH_RESET_PARAM_DEFAULTS,
        "CDL.Reals.Ramp" => reals_ramp::RAMP_PARAM_DEFAULTS,
        "CDL.Routing.BooleanExtractSignal"
        | "CDL.Routing.IntegerExtractSignal"
        | "CDL.Routing.RealExtractSignal" => routing::EXTRACT_SIGNAL_PARAM_DEFAULTS,
        "CDL.Routing.BooleanExtractor"
        | "CDL.Routing.IntegerExtractor"
        | "CDL.Routing.RealExtractor" => routing::EXTRACTOR_PARAM_DEFAULTS,
        "CDL.Routing.BooleanScalarReplicator"
        | "CDL.Routing.IntegerScalarReplicator"
        | "CDL.Routing.RealScalarReplicator" => routing::SCALAR_REPLICATOR_PARAM_DEFAULTS,
        "CDL.Routing.BooleanVectorFilter"
        | "CDL.Routing.IntegerVectorFilter"
        | "CDL.Routing.RealVectorFilter" => routing::VECTOR_FILTER_PARAM_DEFAULTS,
        "CDL.Routing.BooleanVectorReplicator"
        | "CDL.Routing.IntegerVectorReplicator"
        | "CDL.Routing.RealVectorReplicator" => routing::VECTOR_REPLICATOR_PARAM_DEFAULTS,
        "CDL.Utilities.Assert" => utilities::ASSERT_PARAM_DEFAULTS,
        "CDL.Utilities.SunRiseSet" => utilities::SUN_RISE_SET_PARAM_DEFAULTS,
        _ => &[],
    }
}
