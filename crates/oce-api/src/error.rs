//! The unified, typed facade error (`08` §7). Never panics on host input (R-ERR-1);
//! `#[non_exhaustive]` so variants evolve additively (R-ERR-2). It wraps the Group A pipeline
//! errors and the store's `StoreError`, and — critically — **never wraps a backend-specific error
//! type directly**: any durable-store adapter flattens its native errors to
//! `StoreError::Backend(String)` at the seam (R-ERR-3), so the facade error surface is identical in
//! shape regardless of which `Store` backend is wired.

use oce_diag::Diagnostic;

/// Opaque diagnostics retained across a failed load's stage boundary.
///
/// [`OcError::all_diagnostics`] exposes the complete stream without cloning the terminal error's
/// diagnostic payload. The standard error source is the terminal [`OcError`], and display delegates
/// to that terminal failure.
#[derive(Debug)]
pub struct LoadErrorContext {
    source: Box<OcError>,
    prior_diagnostics: Vec<Diagnostic>,
}

impl std::fmt::Display for LoadErrorContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.source, formatter)
    }
}

impl std::error::Error for LoadErrorContext {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// The single typed error returned by every fallible `oce-api` operation.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum OcError {
    /// A CXF ingest failure.
    #[error("CXF ingest error: {0}")]
    Cxf(#[from] oce_cxf::CxfError),
    /// A loader-conformance failure.
    #[error("validation failed: {0}")]
    Validate(#[from] oce_validate::ValidationError),
    /// A flattening failure.
    #[error("flatten error: {0}")]
    Flatten(#[from] oce_flatten::FlattenError),
    /// A model build/schedule failure (algebraic loop, etc.).
    #[error("model build/schedule error: {0}")]
    Build(#[from] oce_graph::BuildError),
    /// A failed load with diagnostics returned by earlier completed stages.
    ///
    /// The opaque payload exposes the terminal failure through [`std::error::Error::source`].
    /// [`OcError::all_diagnostics`] is the authoritative, stage-ordered facade stream. It yields
    /// prior-stage diagnostics followed once by the terminal error's diagnostics, without retaining
    /// a second copy of the terminal payload.
    #[error(transparent)]
    LoadContext(LoadErrorContext),
    /// A generic load failure (also the typed surface for deferred load paths).
    #[error("load failed: {detail}")]
    Load {
        /// Human-readable detail.
        detail: String,
    },
    /// `set_param` attempted while `Running` (CDL §7.4.2: params never change while advancing).
    #[error("cannot set parameter '{path}' while Running; halt() first (CDL §7.4.2)")]
    ParamWhileRunning {
        /// The dotted instance path of the parameter.
        path: String,
    },
    /// A `set_param` value outside the parameter's declared `[min, max]` (CDL §7.4.1).
    #[error("parameter '{path}' value out of declared [min,max] (CDL §7.4.1)")]
    ParamRange {
        /// The dotted instance path of the parameter.
        path: String,
    },
    /// A `set_param` value whose type does not match the parameter's declared type (no coercion).
    #[error("parameter '{path}' type mismatch")]
    ParamType {
        /// The dotted instance path of the parameter.
        path: String,
    },
    /// A `set_param` on a structural (conditional-instance) parameter — reload required (CDL §7.7.4).
    #[error(
        "parameter '{path}' is structural (conditional-instance); reload required (CDL §7.7.4)"
    )]
    ParamStructural {
        /// The dotted instance path of the parameter.
        path: String,
    },
    /// `t_now` was NaN or infinite (host error; CDL §7.16 time is a finite real).
    #[error("non-finite tick time: t_now={now}")]
    NonFiniteTime {
        /// The supplied (non-finite) time.
        now: f64,
    },
    /// `t_now` went backwards (host error; CDL §7.16 monotonic time).
    #[error("time regression: t_now={now} < previous {prev}")]
    TimeRegression {
        /// The supplied (too-small) time.
        now: f64,
        /// The previous tick's time.
        prev: f64,
    },
    /// A finite model time cannot be encoded by one loaded block's sampled-time state.
    #[error("model time cannot be represented by loaded block state: t_now={now}")]
    ModelTimeUnrepresentable {
        /// Finite time refused before any engine or store mutation.
        now: f64,
    },
    /// A real-time step was requested before the host supplied its wall-clock origin.
    #[error("real-time epoch is not configured; call set_realtime_epoch_unix_nanos first")]
    RealtimeEpochUnset,
    /// The host-supplied epoch and model time do not map exactly into the UNIX-nanosecond range.
    #[error(
        "real-time instant is not exactly representable: epoch_unix_nanos={epoch_unix_nanos}, t_now={t_now}"
    )]
    RealtimeInstantUnrepresentable {
        /// Host-supplied UNIX timestamp corresponding to model time `t = 0`.
        epoch_unix_nanos: u64,
        /// Host-supplied model time in seconds.
        t_now: f64,
    },
    /// A point/connector name that does not resolve for the requested operation: either not
    /// present in the loaded model's IO inventory, or present with the wrong direction
    /// (e.g. `get_output` on an input point, `set_input` on an output point). Also returned by
    /// the parameter surface for a path that is not a parameter.
    #[error("unknown point/connector '{0}'")]
    UnknownPoint(String),
    /// A staged input value whose type does not match the target connector (no coercion; `01` §5).
    #[error("input type mismatch for '{0}'")]
    InputType(String),
    /// A checkpoint, snapshot, or restore failure.
    #[error(transparent)]
    State(#[from] crate::state::EngineStateError),
    /// A store-seam failure — the only path any `Store` backend reaches the error type.
    #[error("store error: {0}")]
    Store(#[from] oce_store::StoreError),
}

impl OcError {
    /// Attach diagnostics returned before this terminal load failure.
    pub(crate) fn with_load_context(self, mut prior: Vec<Diagnostic>) -> Self {
        if prior.is_empty() {
            return self;
        }
        match self {
            Self::LoadContext(context) => {
                prior.extend(context.prior_diagnostics);
                Self::LoadContext(LoadErrorContext {
                    source: context.source,
                    prior_diagnostics: prior,
                })
            }
            source => Self::LoadContext(LoadErrorContext {
                source: Box::new(source),
                prior_diagnostics: prior,
            }),
        }
    }

    /// The structured diagnostics behind this failure, or an empty slice for a failure that carries
    /// none.
    ///
    /// A rejected load already knows why it rejected — [`Diagnostic`] carries a stable
    /// [`code`](Diagnostic::code), a severity, the offending subject, and a message. Two of the
    /// variants here own diagnostics, and until this accessor existed only one of the original two
    /// could be read through this crate: `Validate`'s payload is a struct with a public field, which Rust
    /// reaches through a type the caller cannot name, while `Cxf`'s sits in a tuple variant, which
    /// needs the variant path and therefore a dependency on `oce-cxf`. That asymmetry was an
    /// accident of how the two errors happen to be shaped, and it fell on the wrong side: the
    /// resolver arm is where the composite-contract rejections land, and its `Display` is a bare
    /// count, so two documents refused for unrelated reasons print the same sentence.
    ///
    /// **The seams filter differently, so read severity rather than position.** `Validate`
    /// carries `shall`-level errors only — `oce-validate` drops the warnings before constructing
    /// it. `Cxf` carries the whole finalized stream, warnings included, and the stream is sorted
    /// into its pinned order rather than by severity, so its first element is not necessarily an
    /// error: `composite_contract/rejected/partial_port_declaration.jsonld` leads with a warning
    /// and carries two errors behind it. For `LoadContext`, this method retains the terminal error's
    /// existing semantics; use [`OcError::all_diagnostics`] for the preceding completed-stage
    /// diagnostics followed by these terminal diagnostics. Filter with [`Diagnostic::is_error`]
    /// instead of indexing.
    ///
    /// **Empty means the failure carried no structured diagnostics, not that it passed.** Every
    /// engine-produced diagnostic-bearing variant carries a non-empty vector, so an empty slice
    /// identifies the other failures — malformed JSON, a build failure without prior warnings, a
    /// host misuse — for which the [`Display`](std::fmt::Display) message is the whole description.
    /// A build or store failure after a warning can therefore return an empty slice here even though
    /// [`OcError::all_diagnostics`] yields its prior warnings.
    ///
    /// Codes are compared as strings via [`DiagCode::as_str`](oce_diag::DiagCode::as_str), which
    /// resolves without naming the type. The enum is deliberately not re-exported: it is
    /// `#[non_exhaustive]`, so matching it exhaustively is impossible anyway, and its membership
    /// has moved often enough that publishing it would promise a stability nothing yet guards.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        match self {
            Self::Cxf(oce_cxf::CxfError::Validation(diagnostics)) => diagnostics,
            Self::Validate(error) => &error.diagnostics,
            Self::LoadContext(context) => context.source.diagnostics(),
            _ => &[],
        }
    }

    /// All diagnostics available for this failure in facade stage order.
    ///
    /// For an ordinary error this is the same stream as [`OcError::diagnostics`]. For
    /// [`OcError::LoadContext`], diagnostics returned by completed stages come first, followed by
    /// the terminal error's diagnostics. The iterator borrows both vectors and allocates nothing.
    pub fn all_diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        let prior = match self {
            Self::LoadContext(context) => context.prior_diagnostics.as_slice(),
            _ => &[],
        };
        prior.iter().chain(self.diagnostics())
    }
}

/// Convenience result alias for facade operations.
pub type OcResult<T> = Result<T, OcError>;

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use allocation_counter::measure;
    use oce_diag::{DiagCode, Diagnostic};

    use super::OcError;

    fn warning(subject: &str) -> Diagnostic {
        Diagnostic::warning(DiagCode::AnalogCoercedToReal, "prior warning")
            .with_subject(subject.to_owned())
    }

    #[test]
    fn empty_prior_stream_preserves_the_terminal_variant() {
        let error = OcError::Load {
            detail: "terminal".to_owned(),
        }
        .with_load_context(Vec::new());
        assert!(matches!(error, OcError::Load { .. }));
    }

    #[test]
    fn context_delegates_display_and_exposes_the_terminal_source() {
        let terminal = OcError::Load {
            detail: "terminal".to_owned(),
        };
        let terminal_display = terminal.to_string();
        let error = terminal.with_load_context(vec![warning("first")]);
        assert_eq!(error.to_string(), terminal_display);
        let source = error
            .source()
            .and_then(|source| source.downcast_ref::<OcError>())
            .expect("terminal OcError source");
        assert!(matches!(source, OcError::Load { .. }));
    }

    #[test]
    fn context_preserves_the_terminal_lower_level_source_chain() {
        let terminal = OcError::Validate(oce_validate::ValidationError {
            diagnostics: vec![Diagnostic::error(
                DiagCode::UnitQuantityMismatch,
                "terminal error",
            )],
        });
        let error = terminal.with_load_context(vec![warning("first")]);
        let terminal = error
            .source()
            .and_then(|source| source.downcast_ref::<OcError>())
            .expect("terminal OcError source");
        assert!(matches!(terminal, OcError::Validate(_)));
        assert!(terminal.source().is_some());
    }

    #[test]
    fn attaching_context_flattens_the_stream_and_keeps_terminal_diagnostics_once() {
        let terminal = OcError::Validate(oce_validate::ValidationError {
            diagnostics: vec![Diagnostic::error(
                DiagCode::UnitQuantityMismatch,
                "terminal error",
            )],
        });
        let nested = terminal.with_load_context(vec![warning("second")]);
        let error = nested.with_load_context(vec![warning("first")]);
        let OcError::LoadContext(context) = &error else {
            panic!("context must be flattened")
        };
        assert!(matches!(context.source.as_ref(), OcError::Validate(_)));
        assert_eq!(
            error
                .all_diagnostics()
                .map(|diag| diag.code)
                .collect::<Vec<_>>(),
            [
                DiagCode::AnalogCoercedToReal,
                DiagCode::AnalogCoercedToReal,
                DiagCode::UnitQuantityMismatch,
            ]
        );
        assert_eq!(
            error
                .all_diagnostics()
                .filter(|diag| diag.code == DiagCode::UnitQuantityMismatch)
                .count(),
            1
        );
        assert_eq!(context.prior_diagnostics.len(), 2);
        assert_eq!(error.diagnostics().len(), 1);
    }

    #[test]
    fn complete_diagnostic_iteration_allocates_nothing() {
        let terminal = OcError::Validate(oce_validate::ValidationError {
            diagnostics: vec![Diagnostic::error(
                DiagCode::UnitQuantityMismatch,
                "terminal error",
            )],
        });
        let error = terminal.with_load_context(vec![warning("first")]);
        let mut count = 0;

        let allocations = measure(|| {
            for diagnostic in error.all_diagnostics() {
                std::hint::black_box(diagnostic);
                count += 1;
            }
        });

        assert_eq!(count, 2);
        assert_eq!(allocations.count_total, 0, "{allocations:?}");
        assert_eq!(allocations.bytes_total, 0, "{allocations:?}");
    }
}
