#![forbid(unsafe_code)]
//! Verification over vectors for Open Control Engine sequences (design note
//! `docs/design/vectors.md`, issues #519 to #521).
//!
//! Status: three of the four parts of the verification product (M3): the one vector format,
//! `oce/vectors/v1`, the run that checks a set against a sequence and issues the receipt, and
//! mutant mode. The `oce verify` verb builds on it.
//!
//! A **vector set** is a strict-JSON document of scenarios, each a schedule of inputs over one
//! clock and expectations over windows of it, to be checked against one loaded sequence, the
//! **subject**. This crate holds the format:
//!
//! - **The reader** ([`VectorSet::from_json_bytes`]): strict and bounded. The document is one
//!   UTF-8 JSON value under the authoring wire form's profile ([`MAX_DEPTH`], [`MAX_LIST_LEN`],
//!   [`MAX_STRING_BYTES`], no repeated key, no control character, no non-finite number) and the
//!   lowerable [`VerifyLimits`]; every object is closed, so a key the format does not define
//!   (`within`, `of` and `when` among them, until the format gains them) is refused, never
//!   ignored. It takes bytes, opens nothing and never panics.
//! - **The rules** a shape cannot say, checked at load: the clock's cadence, decided by
//!   multiplication ([`Clock`]); step lists from `t = 0`, strictly increasing, never past the
//!   horizon; windows inside the clock that hold at least one tick; a diagnostic instant on a
//!   tick.
//! - **The canonical form** ([`VectorSet::to_canonical_json`]) and the set's identity over it
//!   ([`VectorSet::spec_id`], `spec:fnv1a128:<32 hex>`).
//! - **Typed refusals** ([`VectorError`], [`VectorErrorKind`]): one closed kind per refusal,
//!   each with a stable kebab-case code listed in [`VECTOR_ERROR_CODES`].
//! - **The published schema** ([`VECTORS_JSON_SCHEMA`]), derived from the reader's own shapes.
//!
//! and the run ([`run_vectors`]):
//!
//! - **Binding**, once per set against a throwaway engine, before any scenario runs: the subject's
//!   bytes loaded, every input, output and parameter name resolved by replay's one name rule
//!   ([`oce_replay::resolve_path`]), every value typed by the port or parameter it lands on, an
//!   Enum or String boundary port refused, the four work budgets checked ([`VerifyLimits`]:
//!   frames, compared instants, bytes loaded and byte-frames) before every other load, and each
//!   distinct parameter-edit list tried once on a fresh engine.
//! - **The run**: each scenario on a freshly loaded engine, its parameter edits applied in passes
//!   inside one halt, then one replay on the set's grid, compared exactly (a Real by its bits).
//! - **The receipt** ([`Receipt`]): the header shared with the commissioning verifier and the
//!   vector body, written canonically ([`Receipt::to_canonical_json`]) and named by `receipt_id`.
//! - **Mutant mode**: the deliberately broken copies of the subject a set lists ([`Mutant`]),
//!   handed in by the caller ([`VectorInputs::with_mutants`]), validated at bind and, when the
//!   subject passes, scored in the receipt as `caught` of `total` ([`MutantSection`]).
//! - **Typed refusals** ([`VerifyError`], [`VerifyErrorKind`], [`VERIFY_ERROR_CODES`]): every bind
//!   and run refusal is an error with no receipt; a failed expectation is a failed scenario
//!   inside one.
//!
//! ```
//! use oce_verify::{Expectation, Literal, VectorSet, VerifyLimits};
//!
//! let document = br#"{
//!   "schema": "oce/vectors/v1",
//!   "clock": { "step_s": 60, "horizon_s": 1800 },
//!   "scenarios": [
//!     {
//!       "name": "both_open",
//!       "inputs": { "htg_vlv_cmd": 20.0, "clg_vlv_cmd": 30.0 },
//!       "expect": [
//!         { "output": "yFault", "from_s": 960, "to_s": 1800, "equals": true }
//!       ]
//!     }
//!   ]
//! }"#;
//! let set = VectorSet::from_json_bytes(document, &VerifyLimits::default())?;
//! assert_eq!(set.clock().tick_count(), 31);
//! let Expectation::Window(window) = &set.scenarios()[0].expectations()[0] else {
//!     unreachable!("the scenario's one expectation is a window");
//! };
//! assert_eq!((window.first_tick(), window.last_tick()), (16, 30));
//! assert_eq!(window.equals(), Literal::Boolean(true));
//! assert!(set.spec_id().starts_with("spec:fnv1a128:"));
//!
//! // A key the format does not define is refused by its code, never ignored.
//! let refused = VectorSet::from_json_bytes(
//!     br#"{"schema": "oce/vectors/v1", "within": []}"#,
//!     &VerifyLimits::default(),
//! );
//! assert_eq!(refused.unwrap_err().code(), "unknown-key");
//! # Ok::<(), oce_verify::VectorError>(())
//! ```
//!
//! A run, against a subject of one `CDL.Logical.Not`:
//!
//! ```
//! use oce_verify::{
//!     ReceiptBody, ScenarioStatus, VectorInputs, VectorSet, VerifyLimits, VerifyOptions,
//!     run_vectors,
//! };
//!
//! let subject = br#"{"@context": {"S231": "http://data.ashrae.org/S231#"}, "@graph": [
//!   {"@id": "urn:doc#top", "@type": "S231:Block",
//!    "S231:containsBlock": [{"@id": "urn:doc#top.not"}],
//!    "S231:hasInput": [{"@id": "urn:doc#top.u"}], "S231:hasOutput": [{"@id": "urn:doc#top.y"}]},
//!   {"@id": "urn:doc#top.not", "@type": "CDL.Logical.Not",
//!    "S231:hasInput": {"@id": "urn:doc#top.not.u"}, "S231:hasOutput": {"@id": "urn:doc#top.not.y"}},
//!   {"@id": "urn:doc#top.not.u", "@type": "S231:BooleanInput",
//!    "S231:isOfDataType": {"@id": "S231:Boolean"}},
//!   {"@id": "urn:doc#top.not.y", "@type": "S231:BooleanOutput",
//!    "S231:isOfDataType": {"@id": "S231:Boolean"}, "S231:isConnectedTo": {"@id": "urn:doc#top.y"}},
//!   {"@id": "urn:doc#top.u", "@type": "S231:BooleanInput",
//!    "S231:isOfDataType": {"@id": "S231:Boolean"}, "S231:isConnectedTo": {"@id": "urn:doc#top.not.u"}},
//!   {"@id": "urn:doc#top.y", "@type": "S231:BooleanOutput",
//!    "S231:isOfDataType": {"@id": "S231:Boolean"}}
//! ]}"#;
//! let set = VectorSet::from_json_bytes(
//!     br#"{"schema": "oce/vectors/v1", "clock": {"step_s": 1, "horizon_s": 2},
//!         "scenarios": [{"name": "inverts",
//!           "inputs": {"u": [{"t": 0, "value": false}, {"t": 1, "value": true}]},
//!           "expect": [{"output": "y", "from_s": 0, "to_s": 0, "equals": true},
//!                      {"output": "y", "from_s": 1, "to_s": 2, "equals": false}]}]}"#,
//!     &VerifyLimits::default(),
//! )?;
//! let run = run_vectors(&set, &VectorInputs::new(subject), &VerifyOptions::default())?;
//! assert!(run.receipt.header.passed);
//! let ReceiptBody::Vectors(body) = &run.receipt.body else {
//!     unreachable!("a vector run issues a vector receipt");
//! };
//! assert_eq!(body.scenarios[0].status, ScenarioStatus::Pass);
//! assert_eq!(body.totals.instants.checked, 3);
//! assert!(run.receipt.header.receipt_id.starts_with("receipt:fnv1a128:"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod bind;
mod budget;
mod canonical;
mod clock;
mod compare;
mod decode;
mod dto;
mod edit_lists;
mod error;
mod fnv;
mod limits;
mod model;
mod mutants;
mod number;
mod pairing;
mod parameters;
mod receipt;
mod receipt_json;
mod refusal_classes;
mod run;
mod schema;
mod strict;
mod typing;
mod validate;
mod vector_set;
mod verify_error;

pub use clock::Clock;
pub use error::{Location, VECTOR_ERROR_CODES, VectorError, VectorErrorKind};
pub use limits::{
    MAX_DEPTH, MAX_LIST_LEN, MAX_LOAD_WARNINGS, MAX_STRING_BYTES, MAX_TIME_INTEGER, VerifyLimits,
};
pub use model::{DiagnosticInstant, Expectation, Input, Mutant, Parameter, Scenario, Step, Window};
pub use number::Literal;
pub use receipt::{
    Comparison, ComparisonMode, Failure, FrameCounts, InstantCounts, MutantCatch, MutantEntry,
    MutantSection, MutantStatus, ParamEdit, RECEIPT_SCHEMA, Receipt, ReceiptBody, ReceiptHeader,
    ReceiptKind, ReceiptValue, ScenarioCounts, ScenarioReceipt, ScenarioStatus, ScoreStatus,
    Timings, Totals, UnmatchedWarning, VectorBody,
};
pub use refusal_classes::{ENGINE_REFUSAL_CLASSES, engine_refusal_class};
pub use run::{VectorInputs, VectorRun, VerifyOptions, run_vectors};
pub use schema::{SCHEMA_PROFILE_KEYWORDS, VECTORS_JSON_SCHEMA, vectors_json_schema};
pub use vector_set::VectorSet;
pub use verify_error::{
    MUTANT_PARAM_REFUSAL_CAUSES, PARAM_REFUSAL_CLASSES, REPLAY_FAILURE_CAUSES, VERIFY_ERROR_CODES,
    VerifyError, VerifyErrorKind,
};

/// The format tag a vector set carries in its `schema` key. Any other tag is refused as
/// `unknown-schema`.
pub const VECTORS_SCHEMA: &str = "oce/vectors/v1";
