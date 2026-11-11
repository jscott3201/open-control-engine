//! The validator (authoring note, "Validator"; decision 4).
//!
//! Validation runs in two stages. **Stage 1** ([`Stage::Author`], [`validate_author_side`]) judges
//! the model against the catalog alone: names, the sequence shape, connection syntax and
//! endpoints, class and parameter lookup, parameter kinds and catalog rules, references, arity
//! and port names, duplicates, enumeration types and members, boundary bounds, `SemanticPoint` and
//! `derived_from` syntax. **Stage 2**
//! ([`Stage::Engine`], the `engine` module) runs only when stage 1 found no error: emit, load in a
//! throwaway engine and map each engine diagnostic back onto the model, every engine warning
//! becoming an error finding (decision 14). [`validate()`] runs both stages.
//!
//! Every finding carries an exact code string. [`STAGE_ONE_CODES`] lists the twenty-five stage-1 codes;
//! a code the engine also uses has the engine's [`oce_api::DiagCode`] spelling. The returned list is
//! sorted by the subject's authored position, then code, then detail, and deduplicated, so it is
//! a pure function of the model.

mod blocks;
mod bounds;
pub(crate) mod catalog;
mod connections;
mod engine;
mod index;
pub(crate) mod members;
mod names;
mod semantic;

pub use engine::{DOCUMENT_TOO_LARGE, LOAD_FAILED};
pub use names::MAX_EXACT_INTEGER;
pub(crate) use semantic::boundary_problems as boundary_syntax_problems;
pub use semantic::{MAX_HAYSTACK_TAGS, MAX_SEMANTIC_FIELD_BYTES};

use std::fmt;

use oce_api::ClassPorts;

use crate::location::Location;
use crate::model::Sequence;

pub use codes::*;

/// The finding code strings.
mod codes {
    /// No blocks: a sequence must contain at least one (pre-empts the engine's
    /// `malformed-document`).
    pub const EMPTY_SEQUENCE: &str = "empty-sequence";
    /// A declared name is not an identifier, or is a reserved word or text-form keyword.
    pub const INVALID_NAME: &str = "invalid-name";
    /// A name is declared twice across parameters, inputs, outputs and blocks.
    pub const DUPLICATE_NAME: &str = "duplicate-name";
    /// A block's class path is not a registered class (the engine's code).
    pub const CLASS_NOT_FOUND: &str = "class-not-found";
    /// A block's class is an engine-reserved lowering identity.
    pub const CLASS_NOT_AUTHORABLE: &str = "class-not-authorable";
    /// An assignment names no parameter of the block's class.
    pub const UNKNOWN_PARAMETER: &str = "unknown-parameter";
    /// An assignment names a parameter no author may set (reserved classes only).
    pub const PARAMETER_NOT_SETTABLE: &str = "parameter-not-settable";
    /// A block assigns one parameter twice (the class rules' code; in an emitted document the engine
    /// sees the repeated node as `duplicate-id`).
    pub const DUPLICATE_PARAMETER: &str = "duplicate-parameter";
    /// A required parameter is not assigned (the engine's code).
    pub const MISSING_REQUIRED_PARAMETER: &str = "missing-required-parameter";
    /// A value's kind does not fit the parameter (the engine's code).
    pub const PARAMETER_KIND_MISMATCH: &str = "parameter-kind-mismatch";
    /// A value breaks a catalog rule, widths included (the engine's code).
    pub const PARAMETER_OUT_OF_RANGE: &str = "parameter-out-of-range";
    /// A Real value is NaN or infinite (the engine's code).
    pub const NON_FINITE_PARAMETER: &str = "non-finite-parameter";
    /// An enumeration member is not a member of the parameter's type (the engine refuses the same
    /// value as `grounding-failed`).
    pub const UNKNOWN_ENUM_LITERAL: &str = "unknown-enum-literal";
    /// A sequence parameter's type names no enumeration, or names a constant package, which is
    /// never a parameter type (the engine's code, raised for an unknown G36 type).
    pub const UNKNOWN_ENUM_TYPE: &str = "unknown-enum-type";
    /// A reference names no sequence parameter.
    pub const UNKNOWN_PARAMETER_REFERENCE: &str = "unknown-parameter-reference";
    /// A connection endpoint is not `name` or `block.port` with identifier parts.
    pub const MALFORMED_CONNECTION: &str = "malformed-connection";
    /// A connection source is neither a boundary input nor a block's output.
    pub const UNKNOWN_SOURCE: &str = "unknown-source";
    /// A connection target is neither a boundary output nor a block's input.
    pub const UNKNOWN_TARGET: &str = "unknown-target";
    /// A connection names a port the block, as configured, does not have on that side.
    pub const UNKNOWN_PORT: &str = "unknown-port";
    /// A `SemanticPoint`, `quantity` or `display_unit` field is malformed.
    pub const INVALID_SEMANTIC_POINT: &str = "invalid-semantic-point";
    /// A boundary bound the boundary cannot take: a bound on a Boolean boundary, a Real literal on
    /// an Integer boundary, or an Integer above 2^53 in magnitude on a Real boundary (pre-empts the
    /// engine's `malformed-document`).
    pub const INVALID_BOUND: &str = "invalid-bound";
    /// A boundary bound is NaN or infinite (the engine's code).
    pub const NON_FINITE_BOUND: &str = "non-finite-bound";
    /// A boundary's `min` exceeds its `max` (the engine's code).
    pub const CROSSED_BOUNDS: &str = "crossed-bounds";
    /// The `derived_from` provenance id is malformed.
    pub const INVALID_PROVENANCE_ID: &str = "invalid-provenance-id";
    /// Two connections drive one block input or boundary output. Stage 1 refuses a target the
    /// connection list repeats; the engine's own `single-assignment` covers the rest in stage 2
    /// (it refuses every double-driver shape since GitHub #477).
    pub const SINGLE_ASSIGNMENT: &str = "single-assignment";

    /// Every stage-1 code, in the order of the authoring note's table.
    pub const STAGE_ONE_CODES: &[&str] = &[
        EMPTY_SEQUENCE,
        INVALID_NAME,
        DUPLICATE_NAME,
        CLASS_NOT_FOUND,
        CLASS_NOT_AUTHORABLE,
        UNKNOWN_PARAMETER,
        PARAMETER_NOT_SETTABLE,
        DUPLICATE_PARAMETER,
        MISSING_REQUIRED_PARAMETER,
        PARAMETER_KIND_MISMATCH,
        PARAMETER_OUT_OF_RANGE,
        NON_FINITE_PARAMETER,
        UNKNOWN_ENUM_LITERAL,
        UNKNOWN_ENUM_TYPE,
        UNKNOWN_PARAMETER_REFERENCE,
        MALFORMED_CONNECTION,
        UNKNOWN_SOURCE,
        UNKNOWN_TARGET,
        UNKNOWN_PORT,
        INVALID_SEMANTIC_POINT,
        INVALID_BOUND,
        NON_FINITE_BOUND,
        CROSSED_BOUNDS,
        INVALID_PROVENANCE_ID,
        SINGLE_ASSIGNMENT,
    ];
}

/// The two validation stages (decision 4).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// Stage 1: the model and the catalog alone ([`validate_author_side`]).
    Author,
    /// Stage 2: emit, load in a throwaway engine and map the diagnostics back
    /// ([`validate()`]; findings with [`Origin::Engine`]).
    Engine,
}

/// How serious a finding is. Every stage-1 finding is an error; stage 2 turns every engine
/// warning into an error too (decision 14).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// The sequence is wrong; it must not be emitted.
    Error,
    /// Advice.
    Warning,
}

/// Who found it.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Origin {
    /// The authoring layer (stage 1).
    Author,
    /// The engine (stage 2).
    Engine,
}

/// One validation finding.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// The exact code (`duplicate-name`, …): a stage-1 code ([`STAGE_ONE_CODES`]), an engine
    /// `DiagCode` spelling passed through verbatim by stage 2, or [`LOAD_FAILED`] /
    /// [`DOCUMENT_TOO_LARGE`].
    pub code: &'static str,
    /// Its severity.
    pub severity: Severity,
    /// The model path it concerns: `sequence` (the sequence itself), `derived_from`, a declared
    /// name (`dTHys`, `TOut`, `hys`), an assignment (`hys.uHigh`) or a connection
    /// (`connection[2]`).
    pub subject: String,
    /// The text location, when the input was text. [`crate::validate_text`] fills it from the
    /// subject-to-location map built while parsing ([`crate::SourceMap`]); [`validate()`] on a
    /// model (from the wire form or the builder) leaves it `None`.
    pub span: Option<Location>,
    /// What is wrong, in the author's terms.
    pub detail: String,
    /// Who found it.
    pub origin: Origin,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}: {}", self.code, self.subject, self.detail)
    }
}

/// Where a subject sits in the authored model (crate-private); findings sort by it first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Anchor {
    Sequence,
    DerivedFrom,
    Param(usize),
    Input(usize),
    Output(usize),
    /// A block, then (`Some(j)`) its assignment `j`.
    Block(usize, Option<usize>),
    Connection(usize),
}

/// The findings of one run, before ordering.
#[derive(Default)]
pub(crate) struct Findings {
    items: Vec<(Anchor, Finding)>,
}

impl Findings {
    /// Record an author-side error.
    pub(crate) fn error(
        &mut self,
        position: Anchor,
        code: &'static str,
        subject: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.items.push((
            position,
            Finding {
                code,
                severity: Severity::Error,
                subject: subject.into(),
                span: None,
                detail: detail.into(),
                origin: Origin::Author,
            },
        ));
    }

    /// Record a finding as it is.
    pub(crate) fn push(&mut self, position: Anchor, finding: Finding) {
        self.items.push((position, finding));
    }

    fn finish(mut self) -> Vec<Finding> {
        self.items.sort_by(|(pa, fa), (pb, fb)| {
            (pa, fa.code, &fa.detail, &fa.subject).cmp(&(pb, fb.code, &fb.detail, &fb.subject))
        });
        self.items.dedup();
        self.items.into_iter().map(|(_, finding)| finding).collect()
    }
}

/// Validate a sequence: stage 1 ([`validate_author_side`]), then, only when stage 1 found no
/// error, stage 2 — emit, load in a throwaway engine and map every engine diagnostic back onto the
/// model ([`Stage::Engine`]). Deterministic; the result is sorted by authored position, then code,
/// then detail, and deduplicated. Never panics and never touches an engine the caller owns.
#[must_use]
pub fn validate(sequence: &Sequence) -> Vec<Finding> {
    let (findings, ports) = author_side_with_ports(sequence);
    if findings.iter().any(|f| f.severity == Severity::Error) {
        return findings;
    }
    let mut all = findings;
    all.extend(engine::check(sequence, &ports));
    all
}

/// Stage 1: judge the model against the catalog alone. Pure, deterministic and panic-free; the
/// result is sorted by authored position, then code, then detail, and deduplicated.
#[must_use]
pub fn validate_author_side(sequence: &Sequence) -> Vec<Finding> {
    author_side_with_ports(sequence).0
}

/// Stage 1, also returning each block's resolved ports: `Some` where the class and every
/// assignment passed the author-side checks, `None` otherwise. The emitter names port nodes from
/// these, so it never resolves a block's interface by a second route.
pub(crate) fn author_side_with_ports(
    sequence: &Sequence,
) -> (Vec<Finding>, Vec<Option<ClassPorts>>) {
    let mut findings = Findings::default();
    let declared = index::Declared::new(sequence);
    names::check(sequence, &mut findings);
    let ports = blocks::check(sequence, &declared, &mut findings);
    connections::check(sequence, &declared, &ports, &mut findings);
    let resolved = ports
        .into_iter()
        .map(|p| p.map(|ports| ports.resolved))
        .collect();
    (findings.finish(), resolved)
}
