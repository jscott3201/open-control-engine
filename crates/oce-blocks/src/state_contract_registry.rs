//! Stateful class registry shared by revision lookup and structural validation.

#[derive(Clone, Copy)]
pub(crate) enum Validator {
    Sampled,
    TriggeredSampler,
    TriggeredMax,
    TriggeredMovingMean,
    IntegerChange,
    OnCounter,
    IntegerStage,
    LogicalEdge,
    LogicalLatch,
    Proof,
    SampleTrigger,
    Timer,
    TimerAccumulating,
    TrueDelay,
    TrueHold,
    VariablePulse,
    Comparator,
    Integrator,
    Derivative,
    LimitSlewRate,
    MovingAverage,
    Pid,
    Ramp,
    SunRiseSet,
}

const CONTRACTS: &[(&str, Validator)] = &[
    ("CDL.Discrete.FirstOrderHold", Validator::Sampled),
    ("CDL.Discrete.Sampler", Validator::Sampled),
    ("CDL.Discrete.TriggeredMax", Validator::TriggeredMax),
    (
        "CDL.Discrete.TriggeredMovingMean",
        Validator::TriggeredMovingMean,
    ),
    ("CDL.Discrete.TriggeredSampler", Validator::TriggeredSampler),
    ("CDL.Discrete.UnitDelay", Validator::Sampled),
    ("CDL.Discrete.ZeroOrderHold", Validator::Sampled),
    ("CDL.Integers.Change", Validator::IntegerChange),
    ("CDL.Integers.OnCounter", Validator::OnCounter),
    ("CDL.Integers.Stage", Validator::IntegerStage),
    ("CDL.Logical.Change", Validator::LogicalEdge),
    ("CDL.Logical.Edge", Validator::LogicalEdge),
    ("CDL.Logical.FallingEdge", Validator::LogicalEdge),
    ("CDL.Logical.Latch", Validator::LogicalLatch),
    ("CDL.Logical.Pre", Validator::LogicalEdge),
    ("CDL.Logical.Proof", Validator::Proof),
    (
        "CDL.Logical.Sources.SampleTrigger",
        Validator::SampleTrigger,
    ),
    ("CDL.Logical.Timer", Validator::Timer),
    (
        "CDL.Logical.TimerAccumulating",
        Validator::TimerAccumulating,
    ),
    ("CDL.Logical.Toggle", Validator::LogicalLatch),
    ("CDL.Logical.TrueDelay", Validator::TrueDelay),
    ("CDL.Logical.TrueFalseHold", Validator::TrueHold),
    ("CDL.Logical.TrueHoldWithReset", Validator::TrueHold),
    ("CDL.Logical.VariablePulse", Validator::VariablePulse),
    ("CDL.Reals.Derivative", Validator::Derivative),
    ("CDL.Reals.Greater", Validator::Comparator),
    ("CDL.Reals.GreaterThreshold", Validator::Comparator),
    ("CDL.Reals.Hysteresis", Validator::Comparator),
    ("CDL.Reals.IntegratorWithReset", Validator::Integrator),
    ("CDL.Reals.Less", Validator::Comparator),
    ("CDL.Reals.LessThreshold", Validator::Comparator),
    ("CDL.Reals.LimitSlewRate", Validator::LimitSlewRate),
    ("CDL.Reals.MovingAverage", Validator::MovingAverage),
    ("CDL.Reals.PID", Validator::Pid),
    ("CDL.Reals.PIDWithReset", Validator::Pid),
    ("CDL.Reals.Ramp", Validator::Ramp),
    ("CDL.Utilities.SunRiseSet", Validator::SunRiseSet),
];

pub(crate) fn validator(class_path: &str) -> Option<Validator> {
    CONTRACTS
        .binary_search_by_key(&class_path, |(path, _)| *path)
        .ok()
        .map(|index| CONTRACTS[index].1)
}

#[cfg(test)]
pub(crate) fn class_paths() -> impl Iterator<Item = &'static str> {
    CONTRACTS.iter().map(|(path, _)| *path)
}
