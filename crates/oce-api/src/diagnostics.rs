//! Immutable producer-stage evidence, independent of mutable legacy reports.

use std::cmp::Ordering;

use crate::{ExportReport, LoadReport, OcError};

/// Revision of receipt fields and the message-independent machine ordering.
pub const DIAGNOSTIC_SCHEMA_REVISION: u32 = 1;

/// Producer operation at which evidence was emitted or a terminal failure occurred.
///
/// These are facade pipeline boundaries; CXF import includes the resolver's internal passes.
/// Ranks are explicit and do not derive from code strings or diagnostic prose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagnosticStage {
    /// CXF parse and resolution.
    Import,
    /// Flattening the resolved graph.
    Flatten,
    /// Connector attribute unification.
    AttributeUnification,
    /// Structural/type/parameter validation.
    Validation,
    /// Native block instantiation.
    Instantiation,
    /// Graph schedule construction.
    Schedule,
    /// Effective semantic metadata resolution.
    Semantics,
    /// Durable model projection.
    Projection,
    /// Store recovery before saving the model.
    StoreRecovery,
    /// Saving the resolved model through the store port.
    StoreSave,
    /// Resolving store input handles.
    StoreInputs,
    /// CXF emitted-document export.
    Export,
}

impl DiagnosticStage {
    /// Stable revision-1 rank used by receipt machine ordering.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Import => 0,
            Self::Flatten => 1,
            Self::AttributeUnification => 2,
            Self::Validation => 3,
            Self::Instantiation => 4,
            Self::Schedule => 5,
            Self::Semantics => 6,
            Self::Projection => 7,
            Self::StoreRecovery => 8,
            Self::StoreSave => 9,
            Self::StoreInputs => 10,
            Self::Export => 11,
        }
    }
}

/// Facade-owned diagnostic severity; codes remain extensible strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    /// A shall-level error.
    Error,
    /// An advisory warning.
    Warning,
    /// Informational evidence.
    Info,
}

impl DiagnosticSeverity {
    /// Stable revision-1 rank: error, warning, info.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Warning => 1,
            Self::Info => 2,
        }
    }
}

/// Truthful subject presence/category at current producer seams.
///
/// A present string can identify authored or synthetic content, including class-level or
/// positional subjects. No provenance is guessed from its spelling, code, or message. Hosts
/// own authored-target mapping. Text is preserved exactly, with no URI/Unicode normalization.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DiagnosticSubject {
    /// The producer supplied no subject. Sorts before every present subject.
    Absent,
    /// Complete opaque producer text, including a possibly empty string.
    Opaque(String),
}

/// Message-independent diagnostic identity/order fields; equal keys retain multiplicity.
///
/// Ordering is stage rank, subject category/presence and exact UTF-8 text, code string, severity
/// rank. Equality is not a uniqueness guarantee and excludes human display text.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DiagnosticKey {
    stage: DiagnosticStage,
    subject: DiagnosticSubject,
    code: String,
    severity: DiagnosticSeverity,
}

impl DiagnosticKey {
    /// Producer boundary captured during the operation.
    #[must_use]
    pub fn stage(&self) -> DiagnosticStage {
        self.stage
    }
    /// Complete subject presence and opaque text.
    #[must_use]
    pub fn subject(&self) -> &DiagnosticSubject {
        &self.subject
    }
    /// Extensible machine code string, without inventing codes for other errors.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }
    /// Severity of this diagnostic, independent of operation outcome.
    #[must_use]
    pub fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }
}

impl Ord for DiagnosticKey {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.stage.rank(),
            &self.subject,
            &self.code,
            self.severity.rank(),
        )
            .cmp(&(
                other.stage.rank(),
                &other.subject,
                &other.code,
                other.severity.rank(),
            ))
    }
}

impl PartialOrd for DiagnosticKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// One immutable machine record and its separate human display text.
#[derive(Clone, Debug)]
pub struct DiagnosticRecord {
    key: DiagnosticKey,
    message: String,
}

impl DiagnosticRecord {
    /// Stable machine fields. Compare these, not prose or full debug output.
    #[must_use]
    pub fn key(&self) -> &DiagnosticKey {
        &self.key
    }
    /// Human display text, excluded from ordering and identity; not a compatibility promise.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Immutable complete diagnostic stream from one load/export attempt, sorted by machine key.
///
/// Equal machine records keep multiplicity and producer-relative tie order, without using prose
/// as a tie-breaker. There is no truncation, deduplication, or host-target remapping. An empty
/// failure receipt is valid: inspect [`OperationFailure::stage`] and its error/source context.
#[derive(Clone, Debug)]
pub struct DiagnosticReceipt {
    records: Vec<DiagnosticRecord>,
}

impl DiagnosticReceipt {
    /// Borrow records in revision-1 machine order; no allocation.
    #[must_use]
    pub fn records(&self) -> &[DiagnosticRecord] {
        &self.records
    }
}

/// Successful load result with independently captured immutable producer evidence.
#[derive(Clone, Debug)]
pub struct LoadReceipt {
    report: LoadReport,
    diagnostics: DiagnosticReceipt,
}

impl LoadReceipt {
    /// Borrow the legacy result without granting mutable access to receipt evidence.
    #[must_use]
    pub fn report(&self) -> &LoadReport {
        &self.report
    }
    /// Borrow the captured machine-ordered diagnostic stream.
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticReceipt {
        &self.diagnostics
    }
    /// Separate the legacy result and evidence; later report mutation cannot affect the receipt.
    #[must_use]
    pub fn into_parts(self) -> (LoadReport, DiagnosticReceipt) {
        (self.report, self.diagnostics)
    }
    pub(crate) fn new(report: LoadReport, capture: DiagnosticCapture) -> Self {
        Self {
            report,
            diagnostics: capture.finish(),
        }
    }
}

/// Successful export result with independently captured immutable producer evidence.
#[derive(Debug)]
pub struct ExportReceipt {
    report: ExportReport,
    diagnostics: DiagnosticReceipt,
}

impl ExportReceipt {
    /// Borrow the legacy emitted bytes and warnings.
    #[must_use]
    pub fn report(&self) -> &ExportReport {
        &self.report
    }
    /// Borrow machine-ordered evidence, including any partial-export warnings.
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticReceipt {
        &self.diagnostics
    }
    /// Separate mutable legacy data from the immutable evidence snapshot.
    #[must_use]
    pub fn into_parts(self) -> (ExportReport, DiagnosticReceipt) {
        (self.report, self.diagnostics)
    }
    pub(crate) fn new(report: ExportReport, capture: DiagnosticCapture) -> Self {
        Self {
            report,
            diagnostics: capture.finish(),
        }
    }
}

/// A failed load/export with completed-stage evidence, terminal stage, and original error chain.
///
/// This never fabricates a diagnostic code for JSON/build/store errors without diagnostics.
/// Display retains the legacy terminal context; [`std::error::Error::source`] exposes the original
/// facade error and its sources. Failure atomicity and store side effects are unchanged.
#[derive(Debug)]
pub struct OperationFailure {
    stage: DiagnosticStage,
    diagnostics: DiagnosticReceipt,
    source: Box<OcError>,
}

impl std::fmt::Display for OperationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(formatter)
    }
}

impl std::error::Error for OperationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

impl OperationFailure {
    /// Actual producer operation at the terminal failure.
    #[must_use]
    pub fn stage(&self) -> DiagnosticStage {
        self.stage
    }
    /// Prior-stage and terminal diagnostics, each captured once at its producer boundary.
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticReceipt {
        &self.diagnostics
    }
    /// Borrow the original facade error, retaining existing typed variants and legacy order.
    #[must_use]
    pub fn error(&self) -> &OcError {
        &self.source
    }
    /// Recover the original error for legacy handling.
    #[must_use]
    pub fn into_error(self) -> OcError {
        *self.source
    }
    pub(crate) fn new(source: OcError, mut capture: DiagnosticCapture) -> Self {
        capture.record(source.diagnostics());
        Self {
            stage: capture.stage,
            diagnostics: capture.finish(),
            source: Box::new(source),
        }
    }
}

/// A disabled capture avoids cloning/allocating evidence on legacy operation paths.
pub(crate) struct DiagnosticCapture {
    stage: DiagnosticStage,
    records: Option<Vec<DiagnosticRecord>>,
}

impl DiagnosticCapture {
    pub(crate) fn new(enabled: bool, stage: DiagnosticStage) -> Self {
        Self {
            stage,
            records: enabled.then(Vec::new),
        }
    }
    pub(crate) fn enter(&mut self, stage: DiagnosticStage) {
        self.stage = stage;
    }
    pub(crate) fn record(&mut self, diagnostics: &[oce_diag::Diagnostic]) {
        let Some(records) = &mut self.records else {
            return;
        };
        records.extend(diagnostics.iter().map(|diagnostic| {
            DiagnosticRecord {
                key: DiagnosticKey {
                    stage: self.stage,
                    subject: diagnostic
                        .subject
                        .as_ref()
                        .map_or(DiagnosticSubject::Absent, |text| {
                            DiagnosticSubject::Opaque(text.to_string())
                        }),
                    code: diagnostic.code.as_str().to_owned(),
                    severity: match diagnostic.severity {
                        oce_diag::Severity::Error => DiagnosticSeverity::Error,
                        oce_diag::Severity::Warning => DiagnosticSeverity::Warning,
                        oce_diag::Severity::Info => DiagnosticSeverity::Info,
                    },
                },
                message: diagnostic.message.to_string(),
            }
        }));
    }
    fn finish(self) -> DiagnosticReceipt {
        let mut records = self.records.unwrap_or_default();
        records.sort_by(|left, right| left.key.cmp(&right.key));
        DiagnosticReceipt { records }
    }
}

#[cfg(test)]
#[path = "tests/diagnostic_receipts.rs"]
mod tests;
