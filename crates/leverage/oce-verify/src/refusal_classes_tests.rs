use oce_api::OcError;
use oce_replay::{EngineRefusal, LimitError, ReplayError, SourceError};

use super::*;

/// Every `EngineRefusal`, in declaration (tag) order. The match in `tag_of` is exhaustive, so a
/// new class does not compile until it is listed here, in `engine_refusal_class` and in
/// `ENGINE_REFUSAL_CLASSES`.
const EVERY_REFUSAL: [EngineRefusal; 9] = [
    EngineRefusal::InputType,
    EngineRefusal::InputNonFinite,
    EngineRefusal::InputDomain,
    EngineRefusal::NonFiniteTime,
    EngineRefusal::TimeOutOfRange,
    EngineRefusal::TimeRegression,
    EngineRefusal::NonFiniteTimeStep,
    EngineRefusal::ModelTimeUnrepresentable,
    EngineRefusal::Run,
];

fn tag_of(refusal: EngineRefusal) -> u8 {
    match refusal {
        EngineRefusal::InputType => 0,
        EngineRefusal::InputNonFinite => 1,
        EngineRefusal::InputDomain => 2,
        EngineRefusal::NonFiniteTime => 3,
        EngineRefusal::TimeOutOfRange => 4,
        EngineRefusal::TimeRegression => 5,
        EngineRefusal::NonFiniteTimeStep => 6,
        EngineRefusal::ModelTimeUnrepresentable => 7,
        EngineRefusal::Run => 8,
    }
}

#[test]
fn every_engine_refusal_has_its_class_in_tag_order() {
    assert_eq!(ENGINE_REFUSAL_CLASSES.len(), EVERY_REFUSAL.len());
    for (index, refusal) in EVERY_REFUSAL.into_iter().enumerate() {
        assert_eq!(usize::from(tag_of(refusal)), index);
        assert_eq!(refusal.tag(), tag_of(refusal), "replay's own tag agrees");
        assert_eq!(engine_refusal_class(refusal), ENGINE_REFUSAL_CLASSES[index]);
    }
    assert_eq!(
        engine_refusal_class(EngineRefusal::InputDomain),
        "input-domain"
    );
    assert_eq!(engine_refusal_class(EngineRefusal::Run), "run");
}

#[test]
fn each_replay_error_has_the_cause_the_table_names() {
    let named = [
        (ReplayError::TraceTooLarge { limit: 1 }, "trace-too-large"),
        (
            ReplayError::OutputSetChanged { frame: 3 },
            "output-set-changed",
        ),
        (
            ReplayError::GridNotIncreasing { tick: 9 },
            "grid-not-increasing",
        ),
        (
            ReplayError::Source(SourceError::TooManyRows { limit: 2 }),
            "source",
        ),
        (
            ReplayError::Source(SourceError::BadStep {
                column: "u".to_owned(),
                step: 0,
            }),
            "source",
        ),
    ];
    for (error, cause) in &named {
        assert_eq!(replay_failure_cause(error), *cause, "{error:?}");
    }
    // Everything a replay refuses while being set up, and every error the table does not name,
    // is `engine-setup`.
    let setup = [
        ReplayError::Limit(LimitError::AboveMaximum {
            limit: "max_rows",
            requested: 2,
            maximum: 1,
        }),
        ReplayError::Engine(OcError::StalePreparedFrame),
        ReplayError::StaleMap,
        ReplayError::SourceNotBound,
        ReplayError::InvalidTimeBase,
        ReplayError::InvalidTraceOptions,
        ReplayError::OriginConflict,
        ReplayError::TooManyFrames { limit: 4 },
        ReplayError::UnsupportedOutputType {
            path: "y".to_owned(),
        },
        ReplayError::UnsupportedInputType {
            path: "u".to_owned(),
        },
        ReplayError::InvalidWindow { index: 0 },
        ReplayError::Failed,
    ];
    for error in &setup {
        assert_eq!(replay_failure_cause(error), "engine-setup", "{error:?}");
    }
    for cause in named
        .iter()
        .map(|(_, cause)| *cause)
        .chain(["engine-setup"])
    {
        assert!(crate::REPLAY_FAILURE_CAUSES.contains(&cause), "{cause}");
    }
}

#[test]
fn a_parameter_refusal_has_a_class_only_when_it_is_about_the_value() {
    let path = || "urn:x#top.hys.uLow".to_owned();
    let classes: Vec<Option<&str>> = [
        OcError::ParamRange { path: path() },
        OcError::ParamType { path: path() },
        OcError::ParamStructural { path: path() },
    ]
    .iter()
    .map(param_refusal_class)
    .collect();
    assert_eq!(
        classes,
        [
            Some("param-range"),
            Some("param-type"),
            Some("param-structural")
        ]
    );
    let flattened: Vec<&str> = classes.into_iter().flatten().collect();
    assert_eq!(flattened, crate::PARAM_REFUSAL_CLASSES);
    for other in [
        OcError::ParamWhileRunning { path: path() },
        OcError::UnknownPoint(path()),
        OcError::StalePreparedFrame,
    ] {
        assert_eq!(param_refusal_class(&other), None, "{other:?}");
    }
}
