#![forbid(unsafe_code)]
//! `oce-diag` — the **shared diagnostic vocabulary** for the Open Control Engine's ingest path.
//!
//! Both `oce-cxf` (the CXF Layer-A→B resolver, which raises resolve-time structural diagnostics)
//! and `oce-validate` (the authoritative deep load-conformance gate) report problems as
//! [`Diagnostic`]s drawn from one [`Severity`]/[`DiagCode`] vocabulary. Sharing the vocabulary is
//! deliberate: `oce-api`'s `LoadReport.warnings` and the resolver's report are then the **same**
//! shape, so a host never juggles two diagnostic
//! dialects, and CXF resolve diagnostics flow into the validator's output without translation.
//!
//! This is a **Group A** leaf crate with **zero dependencies** (std only): a [`Diagnostic`] is
//! plain reportable data, not an `Error` type — the owning crates wrap `Vec<Diagnostic>` in their
//! own error/report enums. Diagnostics carry an optional `subject` IRI (the connector/instance a
//! problem concerns) so a host can navigate back to the offending CXF node.
//!
//! **Ordering is the producer's responsibility, not this type's.** `oce-diag` imposes no order on
//! a `Vec<Diagnostic>`; a deterministic order (e.g. `oce-validate` emitting in `ConnectorId.0`
//! ascending order per the plan §2 determinism rule) is achieved by the producing crate iterating
//! its own arena in order — `Diagnostic` deliberately carries only a string `subject`, never a
//! structured connector id, so this crate stays a zero-dependency leaf (no `oce-model` dep).

use std::fmt;
use std::sync::Arc;

/// How severe a [`Diagnostic`] is. Only [`Severity::Error`] fails a load; warnings and info are
/// advisory (e.g. `displayUnit` divergence is a should-warning per §7.10, never a hard error).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Severity {
    /// A hard `shall`-level violation: the load fails.
    Error,
    /// A `should`-level advisory: the load succeeds, the host is informed.
    Warning,
    /// Purely informational provenance (e.g. a tolerated bridge/normalization).
    Info,
}

impl Severity {
    /// Whether this severity fails a load (only [`Severity::Error`] does).
    #[must_use]
    pub fn is_error(self) -> bool {
        matches!(self, Severity::Error)
    }

    /// A stable lowercase label (`"error"`/`"warning"`/`"info"`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A stable, machine-checkable diagnostic code shared across the ingest path. Each maps to a
/// stable kebab-case string ([`DiagCode::as_str`]) for tests, logs, and host display.
///
/// `#[non_exhaustive]`: more codes are added as later ingest passes land; matching from outside
/// this crate must include a wildcard arm.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum DiagCode {
    // --- Resolve-time `shall`-errors (oce-cxf, doc 04 §9.1) ---
    /// Two `@graph` nodes share an `@id` (§9.1.1).
    DuplicateId,
    /// A connector node in a CXF document has no authored `@id`.
    MissingConnectorId,
    /// An `@id` referenced by an edge / `hasInstance` / `containsBlock` / `isConnectedTo` /
    /// `isOfDataType` is absent from both the document and the libraries (§9.1.2).
    UnresolvedReference,
    /// An instance's class IRI did not resolve to a registered block class, and it is not an
    /// `ExtensionBlock` (§9.1.3).
    ClassNotFound,
    /// An overlay node's trailing `.member` did not match a member of the owning instance (§9.1.4).
    OverlayTargetNotFound,
    /// A flattened array name (e.g. `A_1`) collided with an existing instance name (§9.1.7).
    ArrayFlattenCollision,
    /// A parameter binding could not be ground to a literal in `Ground` mode (§9.1.8) — e.g. an
    /// `oce-expr` evaluation failure or an unresolved symbol.
    GroundingFailed,
    /// A declared enumeration type is outside the source-pinned closed-world registry.
    UnknownEnumType,
    /// A declared enumeration literal is not a member of its source-pinned type.
    UnknownEnumLiteral,
    /// A profile supplied a numeric stand-in where a canonical enum literal is required.
    EnumIntegerStandin,
    /// A conditional guard references a parameter not available in the load-time scope.
    ConditionalGuardUnknownParameter,
    /// A conditional guard uses syntax outside the load-time specialization subset.
    ConditionalGuardUnsupported,
    /// An inactive conditional connector/component was still supplied to active graph structure.
    InactiveConditionalNode,
    /// A connector/component required by the active conditional variant is missing.
    MissingActiveConditionalNode,
    /// `isReplaceable=true` or an unevaluated `conditionalExpression` survived into `Ground` mode
    /// (unresolved polymorphism; §9.1.9).
    UnresolvedPolymorphism,
    /// A construct outside the CDL elementary subset appeared (rejected pre-build; exit #2).
    NonSubsetConstruct,
    /// A block's port list named some of its class's declared ports and not the rest, so it can be
    /// read neither by name nor by position. Ordering is a renderer's choice — a document that
    /// consistently uses declared port names binds by name, and one that uses none binds by
    /// position; only the mixture is unreadable.
    PortNameMismatch,
    /// The JSON-LD document was structurally malformed for CXF (e.g. missing `@graph`, bad shape).
    MalformedDocument,
    /// An identity token — a node `@id` or a followed structural reference — is a relative IRI
    /// reference (no scheme and no declared `@context` prefix), and the document declares no
    /// `@base` to resolve it against, so the token cannot be expanded to the canonical absolute
    /// IRI that keys the model (doc 04 R-3). The no-`@base` clause holds by construction:
    /// context-shape validation runs before slot expansion, and a document declaring `@base`
    /// is refused there as [`DiagCode::NonSubsetConstruct`] before this code can fire. Typing
    /// tokens (`@type`, `isOfDataType`) are never refused with this code: their no-match paths
    /// ([`DiagCode::ClassNotFound`], [`DiagCode::UnresolvedReference`]) own junk there.
    RelativeIri,

    // --- Validate-time `shall`-errors (oce-validate, the deep gate) ---
    /// An input's in-degree was not exactly 1 and it is not an external boundary input
    /// (§9.1.5 / §7.10).
    SingleAssignment,
    /// A connection's `from` was not an output, or its `to` not an input (§9.1.6).
    DirectionMismatch,
    /// A connection joined connectors of different value types — no implicit coercion (§9.1.6).
    TypeMismatch,
    /// A connector's declared value type disagrees with its block class's signature port kind
    /// (§7.8 / AD-8) — distinct from [`DiagCode::TypeMismatch`] (which is connection-endpoint
    /// scoped): this is a connector-vs-block-signature mismatch the resolver cannot catch because
    /// it derives the connector type independently of the class.
    PortKindMismatch,
    /// Two connected connectors both declared a unit/quantity and they differ — §7.10 hard error.
    UnitQuantityMismatch,
    /// Two connected connectors both declared a `min`/`max` bound and they differ — §7.10 R13.1
    /// hard error (the bound analogue of [`DiagCode::UnitQuantityMismatch`]).
    BoundMismatch,
    /// A block instance omitted a parameter that the class requires at build time.
    MissingRequiredParameter,
    /// A required block parameter has a scalar kind the block constructor does not consume.
    ParameterKindMismatch,
    /// A block parameter violates a class-level range or ordering rule.
    ParameterOutOfRange,

    // --- Advisory `should`-warnings (doc 04 §9) ---
    /// Connected connectors declared divergent `displayUnit`s — non-computational, warning only
    /// (§7.17).
    DisplayUnitDivergence,
    /// An `S231:Analog*` connector was coerced to `Real` (§8.2 coercion policy) — advisory.
    AnalogCoercedToReal,
    /// An `ExtensionBlock` carried no `hasFmuPath` (§3.7, R-6) — advisory.
    MissingFmuPath,
    /// An unknown `S231:` property key was preserved for forward-compatibility (§9) — advisory.
    UnknownProperty,

    // --- Export-time `shall`-errors (oce-cxf exporter) ---
    /// CXF export was requested but the exporter has not landed; the whole operation is rejected,
    /// so the diagnostic's `subject` is `None` — no individual node is at fault.
    ExportUnsupported,
    /// An export subsetting deferral: an enum-carrying block (and, by cascade, its downstream
    /// consumers) was omitted from the emitted document so the enum-free remainder could still
    /// export. NOT an error — the export succeeds with this diagnostic carried out as a warning
    /// (see `oce_cxf::export_with_report`); `export()` discards it.
    ExportDeferred,
}

impl DiagCode {
    /// A stable kebab-case identifier for this code.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            DiagCode::DuplicateId => "duplicate-id",
            DiagCode::MissingConnectorId => "missing-connector-id",
            DiagCode::UnresolvedReference => "unresolved-reference",
            DiagCode::ClassNotFound => "class-not-found",
            DiagCode::OverlayTargetNotFound => "overlay-target-not-found",
            DiagCode::ArrayFlattenCollision => "array-flatten-collision",
            DiagCode::GroundingFailed => "grounding-failed",
            DiagCode::UnknownEnumType => "unknown-enum-type",
            DiagCode::UnknownEnumLiteral => "unknown-enum-literal",
            DiagCode::EnumIntegerStandin => "enum-integer-standin",
            DiagCode::ConditionalGuardUnknownParameter => "conditional-guard-unknown-parameter",
            DiagCode::ConditionalGuardUnsupported => "conditional-guard-unsupported",
            DiagCode::InactiveConditionalNode => "inactive-conditional-node",
            DiagCode::MissingActiveConditionalNode => "missing-active-conditional-node",
            DiagCode::UnresolvedPolymorphism => "unresolved-polymorphism",
            DiagCode::NonSubsetConstruct => "non-subset-construct",
            DiagCode::MalformedDocument => "malformed-document",
            DiagCode::RelativeIri => "relative-iri",
            DiagCode::SingleAssignment => "single-assignment",
            DiagCode::DirectionMismatch => "direction-mismatch",
            DiagCode::TypeMismatch => "type-mismatch",
            DiagCode::PortNameMismatch => "port-name-mismatch",
            DiagCode::PortKindMismatch => "port-kind-mismatch",
            DiagCode::UnitQuantityMismatch => "unit-quantity-mismatch",
            DiagCode::BoundMismatch => "bound-mismatch",
            DiagCode::MissingRequiredParameter => "missing-required-parameter",
            DiagCode::ParameterKindMismatch => "parameter-kind-mismatch",
            DiagCode::ParameterOutOfRange => "parameter-out-of-range",
            DiagCode::DisplayUnitDivergence => "display-unit-divergence",
            DiagCode::AnalogCoercedToReal => "analog-coerced-to-real",
            DiagCode::MissingFmuPath => "missing-fmu-path",
            DiagCode::UnknownProperty => "unknown-property",
            DiagCode::ExportUnsupported => "export-unsupported",
            DiagCode::ExportDeferred => "export-deferred",
        }
    }
}

impl fmt::Display for DiagCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One diagnostic: a [`Severity`], a stable [`DiagCode`], a human-readable `message`, and an
/// optional `subject` — the IRI of the CXF node/connector the diagnostic concerns, so a host can
/// navigate back to the source.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Diagnostic {
    /// How severe this diagnostic is.
    pub severity: Severity,
    /// The stable code classifying this diagnostic.
    pub code: DiagCode,
    /// A human-readable explanation.
    pub message: String,
    /// The IRI of the offending node/connector, if known.
    pub subject: Option<Arc<str>>,
}

impl Diagnostic {
    /// Construct a diagnostic with an explicit severity.
    #[must_use]
    pub fn new(severity: Severity, code: DiagCode, message: impl Into<String>) -> Self {
        Self {
            severity,
            code,
            message: message.into(),
            subject: None,
        }
    }

    /// Construct an [`Severity::Error`] diagnostic.
    #[must_use]
    pub fn error(code: DiagCode, message: impl Into<String>) -> Self {
        Self::new(Severity::Error, code, message)
    }

    /// Construct a [`Severity::Warning`] diagnostic.
    #[must_use]
    pub fn warning(code: DiagCode, message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, code, message)
    }

    /// Construct a [`Severity::Info`] diagnostic.
    #[must_use]
    pub fn info(code: DiagCode, message: impl Into<String>) -> Self {
        Self::new(Severity::Info, code, message)
    }

    /// Attach the IRI of the offending node/connector (builder style).
    #[must_use]
    pub fn with_subject(mut self, subject: impl Into<Arc<str>>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// Whether this diagnostic is an error (fails a load).
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.severity.is_error()
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.severity, self.code, self.message)?;
        if let Some(subject) = &self.subject {
            write!(f, " (at {subject})")?;
        }
        Ok(())
    }
}

/// Whether any diagnostic in `diagnostics` is an [`Severity::Error`] — the shared "did the load
/// fail?" predicate, so `oce-cxf` and `oce-validate` agree on pass/fail.
#[must_use]
pub fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(Diagnostic::is_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_error_predicate_and_labels() {
        assert!(Severity::Error.is_error());
        assert!(!Severity::Warning.is_error());
        assert!(!Severity::Info.is_error());
        assert_eq!(Severity::Error.as_str(), "error");
        assert_eq!(Severity::Warning.to_string(), "warning");
    }

    /// Single source of truth for the pin table: expands to both the `PINNED_CODES` list the
    /// test iterates and a wildcard-free `match` over `DiagCode`. In-crate, the match is checked
    /// exhaustively despite `#[non_exhaustive]`, so adding a variant without pinning its string
    /// here — or dropping an entry from this list — fails to compile before any test runs, and a
    /// duplicated entry dies as an unreachable match arm.
    macro_rules! pinned_diag_code_strings {
        ($($variant:ident => $string:literal,)+) => {
            /// Every `DiagCode` variant, derived from the same list as the exhaustive match.
            const PINNED_CODES: &[DiagCode] = &[$(DiagCode::$variant,)+];

            /// The pinned kebab-case string for `code` — the compile-time exhaustiveness guard.
            fn pinned_str(code: DiagCode) -> &'static str {
                match code {
                    $(DiagCode::$variant => $string,)+
                }
            }
        };
    }

    pinned_diag_code_strings! {
        DuplicateId => "duplicate-id",
        MissingConnectorId => "missing-connector-id",
        UnresolvedReference => "unresolved-reference",
        ClassNotFound => "class-not-found",
        OverlayTargetNotFound => "overlay-target-not-found",
        ArrayFlattenCollision => "array-flatten-collision",
        GroundingFailed => "grounding-failed",
        UnknownEnumType => "unknown-enum-type",
        UnknownEnumLiteral => "unknown-enum-literal",
        EnumIntegerStandin => "enum-integer-standin",
        ConditionalGuardUnknownParameter => "conditional-guard-unknown-parameter",
        ConditionalGuardUnsupported => "conditional-guard-unsupported",
        InactiveConditionalNode => "inactive-conditional-node",
        MissingActiveConditionalNode => "missing-active-conditional-node",
        UnresolvedPolymorphism => "unresolved-polymorphism",
        NonSubsetConstruct => "non-subset-construct",
        PortNameMismatch => "port-name-mismatch",
        MalformedDocument => "malformed-document",
        RelativeIri => "relative-iri",
        SingleAssignment => "single-assignment",
        DirectionMismatch => "direction-mismatch",
        TypeMismatch => "type-mismatch",
        PortKindMismatch => "port-kind-mismatch",
        UnitQuantityMismatch => "unit-quantity-mismatch",
        BoundMismatch => "bound-mismatch",
        MissingRequiredParameter => "missing-required-parameter",
        ParameterKindMismatch => "parameter-kind-mismatch",
        ParameterOutOfRange => "parameter-out-of-range",
        DisplayUnitDivergence => "display-unit-divergence",
        AnalogCoercedToReal => "analog-coerced-to-real",
        MissingFmuPath => "missing-fmu-path",
        UnknownProperty => "unknown-property",
        ExportUnsupported => "export-unsupported",
        ExportDeferred => "export-deferred",
    }

    #[test]
    fn diag_codes_have_stable_unique_strings() {
        // Every code emits exactly its pinned kebab-case string, and all strings are distinct.
        let mut seen = Vec::new();
        for &code in PINNED_CODES {
            let s = code.as_str();
            assert_eq!(s, pinned_str(code), "{code:?} must keep its pinned string");
            assert!(!s.is_empty() && s.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-'));
            assert!(!seen.contains(&s), "duplicate code string {s:?}");
            seen.push(s);
        }
    }

    #[test]
    fn diagnostic_builders_and_display() {
        let d = Diagnostic::error(DiagCode::SingleAssignment, "input has in-degree 2")
            .with_subject("http://example.org#Seq.gt.u2");
        assert!(d.is_error());
        assert_eq!(d.code, DiagCode::SingleAssignment);
        assert_eq!(
            d.to_string(),
            "[error] single-assignment: input has in-degree 2 (at http://example.org#Seq.gt.u2)"
        );

        let w = Diagnostic::warning(DiagCode::DisplayUnitDivergence, "degC vs K");
        assert!(!w.is_error());
        assert_eq!(w.subject, None);
        assert_eq!(
            w.to_string(),
            "[warning] display-unit-divergence: degC vs K"
        );
    }

    #[test]
    fn has_errors_predicate() {
        let none: Vec<Diagnostic> = Vec::new();
        assert!(!has_errors(&none));

        let warnings_only = vec![Diagnostic::warning(DiagCode::DisplayUnitDivergence, "x")];
        assert!(!has_errors(&warnings_only));

        let with_error = vec![
            Diagnostic::info(DiagCode::ClassNotFound, "bridged"),
            Diagnostic::error(DiagCode::TypeMismatch, "Real vs Integer"),
        ];
        assert!(has_errors(&with_error));
    }
}
