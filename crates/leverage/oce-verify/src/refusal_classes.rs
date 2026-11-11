//! The closed kebab-case tables `oce-verify` owns for refusals it did not make (vectors note,
//! decision 25 and "Error codes"): the class of an engine-refused frame, the cause of a replay
//! that ended in an error, and the class of a refused parameter edit.
//!
//! Each table is a function with an exhaustive match where the source type allows one
//! ([`oce_replay::EngineRefusal`] is a closed enum, so a new class does not build until it has a
//! spelling here), and a pinned list constant; the facade's and replay's error types are
//! `#[non_exhaustive]`, so their mappings end in the one documented catch-all. No spelling is a
//! Rust variant name, and none moves when a variant is renamed.

use oce_api::OcError;
use oce_replay::{EngineRefusal, ReplayError};

/// The class spellings of [`EngineRefusal`], in its declaration (tag) order: the `refusal` of a
/// `frame-refused` failure record.
pub const ENGINE_REFUSAL_CLASSES: &[&str] = &[
    "input-type",
    "input-non-finite",
    "input-domain",
    "non-finite-time",
    "time-out-of-range",
    "time-regression",
    "non-finite-time-step",
    "model-time-unrepresentable",
    "run",
];

/// The kebab-case class of an engine refusal (decision 25). Exhaustive: `EngineRefusal` is a
/// closed enum, so this table is complete by construction.
#[must_use]
pub const fn engine_refusal_class(refusal: EngineRefusal) -> &'static str {
    match refusal {
        EngineRefusal::InputType => "input-type",
        EngineRefusal::InputNonFinite => "input-non-finite",
        EngineRefusal::InputDomain => "input-domain",
        EngineRefusal::NonFiniteTime => "non-finite-time",
        EngineRefusal::TimeOutOfRange => "time-out-of-range",
        EngineRefusal::TimeRegression => "time-regression",
        EngineRefusal::NonFiniteTimeStep => "non-finite-time-step",
        EngineRefusal::ModelTimeUnrepresentable => "model-time-unrepresentable",
        EngineRefusal::Run => "run",
    }
}

/// The cause of a replay error, from [`crate::REPLAY_FAILURE_CAUSES`]. A coverage refusal never
/// reaches here: it is `window-uncovered`, its own code.
pub(crate) fn replay_failure_cause(error: &ReplayError) -> &'static str {
    match error {
        ReplayError::TraceTooLarge { .. } => "trace-too-large",
        ReplayError::OutputSetChanged { .. } => "output-set-changed",
        ReplayError::GridNotIncreasing { .. } => "grid-not-increasing",
        ReplayError::Source(_) => "source",
        // Everything `Replay::new` refuses, a facade setup call refused (`Engine`), and every
        // error the table does not name: the run could not be set up or continued.
        _ => "engine-setup",
    }
}

/// The class of a refused parameter edit of the subject, from [`crate::PARAM_REFUSAL_CLASSES`],
/// or `None` for a refusal of `set_param` that is not about the edit's value (it is then
/// `engine-setup`).
pub(crate) fn param_refusal_class(error: &OcError) -> Option<&'static str> {
    match error {
        OcError::ParamRange { .. } => Some("param-range"),
        OcError::ParamType { .. } => Some("param-type"),
        OcError::ParamStructural { .. } => Some("param-structural"),
        _ => None,
    }
}

/// The cause of a mutant's refusal of an edit the subject admits, from
/// [`crate::MUTANT_PARAM_REFUSAL_CAUSES`] (owner ruling of 2026-11-11): the subject's three classes,
/// and `param-path-unknown` for a parameter the mutant does not have, spelled as the bind code for
/// a key that names no parameter. `None` for any other refusal (`engine-setup`).
pub(crate) fn mutant_param_refusal_cause(error: &OcError) -> Option<&'static str> {
    match error {
        OcError::UnknownPoint(_) => Some("param-path-unknown"),
        other => param_refusal_class(other),
    }
}

#[cfg(test)]
#[path = "refusal_classes_tests.rs"]
mod tests;
