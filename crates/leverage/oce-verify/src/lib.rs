#![forbid(unsafe_code)]
//! Verification over vectors for Open Control Engine sequences (design note
//! `docs/design/vectors.md`, issues #519 to #521), and the envelope arithmetic of its tolerance
//! mode (design note `docs/design/commissioning.md`, issue #655).
//!
//! Status: the four parts of the verification product (M3): the one vector format,
//! `oce/vectors/v1`, the run that checks a set against a sequence and issues the receipt, mutant
//! mode, and tolerance mode: the envelope arithmetic, the tolerance windows a vector file asks
//! for it with (issue #684), and the commissioning verifier, which runs a spec's reference
//! against its sequence and issues a commissioning receipt ([`run_commissioning()`]). The
//! `oce verify` verb builds on it.
//!
//! A **vector set** is a strict-JSON document of scenarios, each a schedule of inputs over one
//! clock and expectations over windows of it, to be checked against one loaded sequence, the
//! **subject**. This crate holds the format:
//!
//! - **The reader** ([`VectorSet::from_json_bytes`]): strict and bounded. The document is one
//!   UTF-8 JSON value under the authoring wire form's profile ([`MAX_DEPTH`], [`MAX_LIST_LEN`],
//!   [`MAX_STRING_BYTES`], no repeated key, no control character, no non-finite number) and the
//!   lowerable [`VerifyLimits`]; every object is closed, so a key the format does not define,
//!   or one of its keys in another object, is refused, never ignored. It takes bytes, opens
//!   nothing and never panics.
//! - **The rules** a shape cannot say, checked at load: the clock's cadence, decided by
//!   multiplication ([`Clock`]); step lists from `t = 0`, strictly increasing, never past the
//!   horizon; windows inside the clock that hold at least one tick; a diagnostic instant on a
//!   tick; a tolerance window's `within` (at least one of the six names, none negative, `ltolx`
//!   at most 1), its `of` (from the window's start, never past its end) and a `when` of 1 to 16
//!   names; the set's `envelope`, `funnel` or `aligned-grid`.
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
//!   inside one halt, then one replay on the set's grid. An exact window is compared exactly (a
//!   Real by its bits); a tolerance window under the set's envelope, built around its expected
//!   signal after that signal is checked to lie inside its own envelope; a `when` masks the ticks
//!   where one of its Boolean signals is false.
//! - **The receipt** ([`Receipt`]): the header shared with the commissioning verifier and the
//!   vector body, written canonically ([`Receipt::to_canonical_json`]) and named by `receipt_id`.
//! - **Mutant mode**: the deliberately broken copies of the subject a set lists ([`Mutant`]),
//!   handed in by the caller ([`VectorInputs::with_mutants`]), validated at bind and, when the
//!   subject passes, scored in the receipt as `caught` of `total` ([`MutantSection`]).
//! - **Typed refusals** ([`VerifyError`], [`VerifyErrorKind`], [`VERIFY_ERROR_CODES`]): every bind
//!   and run refusal is an error with no receipt; a failed expectation is a failed scenario
//!   inside one.
//!
//! and the **envelopes** a tolerance comparison is made with, a library surface of their own: is
//! a test series inside a band built around a reference series?
//!
//! - **[`funnel()`]**: a port of LBNL funnel, the tool OBC §12 cites ([`FUNNEL_VERSION`]), held
//!   to the pinned `pyfunnel` by a differential test: its five curves print, in C's `%.16g`
//!   ([`format_g16`]), as pyfunnel's five files do, byte for byte.
//! - **[`aligned_grid()`]**: the conformance harness's wider grid envelope, copied
//!   ([`Envelope::AlignedGrid`], [`ALIGNED_GRID_VERSION`]).
//! - Both take two [`Series`], six [`Tolerances`] and the lowerable [`CompareLimits`], return a
//!   [`FunnelResult`] or a typed [`CompareError`] (stable codes in [`COMPARE_ERROR_CODES`]), read
//!   no file and no clock, and give the same bits on every target.
//!
//! and the first part of the **commissioning verifier** (OBC §12; design note
//! `docs/design/commissioning.md`, issue #658), the readers of the two files a commissioning run
//! reads:
//!
//! - **The test spec** ([`CommissioningSpec`]), `oce/commissioning-spec/v1`
//!   ([`COMMISSIONING_SPEC_SCHEMA`]): a strict-JSON superset of OBC §12.7 Listing 12.1. `//` and
//!   `/* */` comments outside strings are removed, then the text is read by the strict cursor the
//!   vector reader uses, under the same profile and the lowerable [`SpecLimits`]. It keeps the
//!   LBL fields as written, reports the ones the verifier does not act on as findings
//!   ([`Finding`]), writes a canonical form and names it with a `spec_id`.
//! - **The point mapping** ([`PointMapping`]): Listing 12.2's shape, from a file of its own or
//!   inline in a spec.
//! - **Keys**: an `outputs` or `indicators` key is its reference's `sequence`, a `.`, and an
//!   exact name or a glob. Which outputs each selects, and the tolerances
//!   ([`PartialTolerances`]) and indicators that follow, are decided when a run binds a
//!   reference to a sequence, by rules that do not depend on the order of the file: an exact
//!   name beats a glob, and two keys of one kind that disagree are refused. That code is in the
//!   crate and private until the bind that calls it lands.
//! - **Typed refusals** ([`SpecError`], [`SpecErrorKind`], [`SPEC_ERROR_CODES`]), each with the
//!   line and column of the text as written, comments included.
//!
//! Its fifth and sixth parts (issues #662 and #663) run one reference
//! ([`run_commissioning()`]): bind it to its sequence (names by replay's one rule, every mapped
//! point's type, unit and quantity held to its port, the compared outputs those an `expected`
//! source supplies, the keys, indicators and parameters), run one replay over the merged trends
//! (a frame refused for a missing sample recorded and its stretch uncovered, an engine refusal
//! an error), prepare each compared output's two series by zero-order hold, mask them by their
//! indicators, compare a Real under the reference's envelope at the test's own instants and a
//! Boolean or Integer exactly, and issue the commissioning receipt (`kind: "commissioning"`,
//! [`CommissioningBody`]) with the exceedance columns ([`CommissionRun`]).
//!
//! Its second part (issue #659) is the closed unit table a trend's readings are converted with,
//! device unit to CDL unit, before any comparison: [`convert_unit`] over `units/v1`
//! ([`UNITS_TABLE`]), each row one way, temperature readings and temperature differences kept
//! apart, spellings compared byte for byte, every refusal a typed [`UnitError`] (stable codes in
//! [`UNIT_ERROR_CODES`]) and never a pass-through.
//!
//! Its third part (issue #660) reads a reference's trend sources ([`TrendSources::read`]) from
//! bytes the caller hands in ([`SpecInputs`]): `csv` through replay's own reader and
//! `combiTimeTable` through this crate's reader of OBC §12.5.1's `#1` text, every cell converted
//! once to its CDL point's unit and type, the input sources merged on the union of their times
//! ([`TrendInputs`], a `ColumnarSource` for replay), the expected and indicator series
//! ([`TrendSeries`]) and the receipt's records of the files; bounded by [`TrendLimits`], every
//! refusal a typed [`SourceError`] naming its file, row and column ([`SOURCE_ERROR_CODES`]).
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
//! assert_eq!(window.equals(), Some(Literal::Boolean(true)));
//! assert!(set.spec_id().starts_with("spec:fnv1a128:"));
//!
//! // A tolerance window: `within` and `of` in place of `equals`, an optional `when` mask, and
//! // the set's envelope, `funnel` when the set names none.
//! let tolerant = VectorSet::from_json_bytes(
//!     br#"{"schema": "oce/vectors/v1", "clock": {"step_s": 60, "horizon_s": 1800},
//!         "scenarios": [{"name": "tracks", "inputs": {},
//!           "expect": [{"output": "VDis_flow", "from_s": 300, "to_s": 1800,
//!                       "within": {"atoly": 0.005, "atolx": 60},
//!                       "of": [{"t": 300, "value": 0.0}, {"t": 600, "value": 0.25}],
//!                       "when": ["occupied"]}]}]}"#,
//!     &VerifyLimits::default(),
//! )?;
//! let Expectation::Window(window) = &tolerant.scenarios()[0].expectations()[0] else {
//!     unreachable!("the scenario's one expectation is a window");
//! };
//! let within = window.within().expect("a tolerance window");
//! assert_eq!((within.tolerances().atoly, within.tolerances().atolx), (0.005, 60.0));
//! assert_eq!(window.when(), ["occupied"]);
//! assert_eq!(tolerant.envelope(), oce_verify::Envelope::Funnel);
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
//!
//! A commissioning spec, with a comment and no `schema` key, as an LBL file is written:
//!
//! ```
//! use oce_verify::{CommissioningSpec, SpecLimits};
//!
//! let text = r#"{
//!   // one test of one sequence
//!   "references": [
//!     { "model": "Supply_u", "sequence": "setPoiVAV1", "runController": true,
//!       "outputs": { "setPoiVAV1.TSup*": { "atoly": 0.5 } },
//!       "indicators": { "setPoiVAV1.TSup*": [ "fanSta.y" ] } }
//!   ],
//!   "tolerances": { "rtolx": 0.002, "rtoly": 0.002, "atolx": 10, "atoly": 0 }
//! }"#;
//! let spec = CommissioningSpec::from_json_str(text, &SpecLimits::default())?;
//! let reference = &spec.references()[0];
//! assert_eq!(spec.findings()[0].at(), "references[0].runController");
//!
//! // The key is kept as written; which outputs its glob selects is decided at bind.
//! assert_eq!(reference.outputs()[0].key(), "setPoiVAV1.TSup*");
//! assert_eq!(reference.outputs()[0].tolerances().atoly(), Some(0.5));
//! assert_eq!(spec.tolerances().map(|global| global.resolved().atolx), Some(10.0));
//! assert!(spec.spec_id().starts_with("spec:fnv1a128:"));
//!
//! // A trailing comma is still refused, at its place in the text as written.
//! let trailing = "{\n  \"references\": [], // none\n}";
//! let refused = CommissioningSpec::from_json_str(trailing, &SpecLimits::default()).unwrap_err();
//! assert_eq!((refused.code(), refused.subject()), ("json-syntax", "line 3 column 1"));
//! # Ok::<(), oce_verify::SpecError>(())
//! ```

mod aligned_grid;
mod bind;
mod bind_window;
mod budget;
mod canonical;
mod clock;
mod commissioning;
mod compare;
mod compare_error;
mod compare_limits;
mod decode;
mod dto;
mod edit_lists;
mod envelope;
mod error;
mod fnv;
mod funnel;
mod g16;
mod limits;
mod mask;
mod model;
mod mutants;
mod number;
mod pairing;
mod parameters;
mod receipt;
mod receipt_json;
mod refusal_classes;
mod relative_path;
mod run;
mod schema;
mod strict;
mod tolerance;
mod typing;
mod validate;
mod vector_set;
mod verify_error;

pub use aligned_grid::{ALIGNED_GRID_VERSION, aligned_grid};
pub use clock::Clock;
pub use commissioning::{
    COMMISSIONING_SPEC_SCHEMA, CommissioningSpec, ComparisonWindow, Finding, FunnelReference,
    IndicatorRule, LBL_FIELD_IGNORED, MAX_SPEC_DETAIL_BYTES, OutputOverride, PartialTolerances,
    PointMapping, PointNameMapping, PointPair, PointSide, PointType, SPEC_ERROR_CODES,
    SPEC_FINDING_CODES, SourceColumn, SourceTimeColumn, SpecError, SpecErrorKind, SpecLimits,
    SpecReference, TOLERANCE_NAMES, TrendFormat, TrendSource, UNIT_ERROR_CODES, UNITS_TABLE,
    UnitError, UnitSide, convert_unit,
};
pub use commissioning::{
    CommissionOptions, CommissionRun, CommissioningBody, CommissioningParam, Exceedance,
    FindingRecord, MAX_COMPARED_OUTPUTS, MAX_FINDINGS, MAX_INDICATORS, MAX_MASKED_EXCEEDANCE,
    MAX_MASKED_INTERVALS, MAX_RUN_POINTS, MAX_RUN_WARNINGS, MAX_TOTAL_ALIGNED_GRID_WORK,
    MAX_UNIT_COLUMNS, MAX_UNVERIFIED_OUTPUTS, MaskedInterval, OutputBounds, OutputCurves,
    OutputReport, OutputStatus, OutputType, SpecNumber, TimeRecord, TrendData, run_commissioning,
};
pub use commissioning::{
    InputColumn, MappingRecord, SOURCE_ERROR_CODES, SeriesValues, SourceError, SourceErrorKind,
    SourceRecord, SourceRole, SpecInputs, TrendFrameSource, TrendInputs, TrendLimits, TrendSeries,
    TrendSources, UnitColumn,
};
pub use compare_error::{Axis, COMPARE_ERROR_CODES, CompareError, SeriesEnd, SeriesRole};
pub use compare_limits::CompareLimits;
pub use envelope::{Curve, Envelope, FunnelResult, HalfSize, Series, Tolerances};
pub use error::{Location, VECTOR_ERROR_CODES, VectorError, VectorErrorKind};
pub use funnel::{FUNNEL_VERSION, funnel};
pub use g16::format_g16;
pub use limits::{
    MAX_DEPTH, MAX_LIST_LEN, MAX_LOAD_WARNINGS, MAX_STRING_BYTES, MAX_TIME_INTEGER, VerifyLimits,
};
pub use model::{
    DiagnosticInstant, Expectation, Input, Mutant, OfStep, Parameter, Scenario, Step, Window,
    Within,
};
pub use number::Literal;
pub use receipt::{
    Comparison, ComparisonMode, Failure, FrameCounts, InstantCounts, MutantCatch, MutantEntry,
    MutantSection, MutantStatus, ParamEdit, RECEIPT_SCHEMA, Receipt, ReceiptBody, ReceiptHeader,
    ReceiptKind, ReceiptValue, ScenarioCounts, ScenarioReceipt, ScenarioStatus, ScoreStatus,
    Timings, Totals, UnmatchedWarning, VECTOR_FUNNEL_REFERENCE, VectorBody,
};
pub use refusal_classes::{ENGINE_REFUSAL_CLASSES, engine_refusal_class};
pub use run::{VectorInputs, VectorRun, VerifyOptions, run_vectors};
pub use schema::{SCHEMA_PROFILE_KEYWORDS, VECTORS_JSON_SCHEMA, vectors_json_schema};
pub use vector_set::VectorSet;
pub use verify_error::{
    COMPARE_REFUSAL_CAUSES, MUTANT_PARAM_REFUSAL_CAUSES, PARAM_REFUSAL_CLASSES,
    REPLAY_FAILURE_CAUSES, VERIFY_ERROR_CODES, VerifyError, VerifyErrorKind,
};

/// The format tag a vector set carries in its `schema` key. Any other tag is refused as
/// `unknown-schema`.
pub const VECTORS_SCHEMA: &str = "oce/vectors/v1";
