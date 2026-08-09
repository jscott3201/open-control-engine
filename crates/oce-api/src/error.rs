//! The unified, typed facade error (`08` §7). Never panics on host input (R-ERR-1);
//! `#[non_exhaustive]` so variants evolve additively (R-ERR-2). It wraps the Group A pipeline
//! errors and the store's `StoreError`, and — critically — **never wraps a backend-specific error
//! type directly**: any durable-store adapter flattens its native errors to
//! `StoreError::Backend(String)` at the seam (R-ERR-3), so the facade error surface is identical in
//! shape regardless of which `Store` backend is wired.

use oce_diag::Diagnostic;

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
    /// A store-seam failure — the only path any `Store` backend reaches the error type.
    #[error("store error: {0}")]
    Store(#[from] oce_store::StoreError),
}

impl OcError {
    /// The structured diagnostics behind this failure, or an empty slice for a failure that carries
    /// none.
    ///
    /// A rejected load already knows why it rejected — [`Diagnostic`] carries a stable
    /// [`code`](Diagnostic::code), a severity, the offending subject, and a message. Two of the
    /// variants here own that vector, and until this accessor existed only one of them could be
    /// read through this crate: `Validate`'s payload is a struct with a public field, which Rust
    /// reaches through a type the caller cannot name, while `Cxf`'s sits in a tuple variant, which
    /// needs the variant path and therefore a dependency on `oce-cxf`. That asymmetry was an
    /// accident of how the two errors happen to be shaped, and it fell on the wrong side: the
    /// resolver arm is where the composite-contract rejections land, and its `Display` is a bare
    /// count, so two documents refused for unrelated reasons print the same sentence.
    ///
    /// **The two seams filter differently, so read severity rather than position.** `Validate`
    /// carries `shall`-level errors only — `oce-validate` drops the warnings before constructing
    /// it. `Cxf` carries the whole finalized stream, warnings included, and the stream is sorted
    /// into its pinned order rather than by severity, so its first element is not necessarily an
    /// error: `composite_contract/rejected/partial_port_declaration.jsonld` leads with a warning
    /// and carries two errors behind it. Filter with [`Diagnostic::is_error`] instead of indexing.
    ///
    /// **Empty means the failure carried no structured diagnostics, not that it passed.** Both
    /// diagnostic-bearing variants are constructed only from a non-empty vector, so an empty slice
    /// identifies the other failures — malformed JSON, a build failure, a host misuse — for which
    /// the [`Display`](std::fmt::Display) message is the whole description. The malformed-JSON case
    /// is pinned by `a_refusal_carrying_no_diagnostics_reports_an_empty_slice`; the rest are
    /// unreachable or untested here and rest on the construction sites, not on a test.
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
            _ => &[],
        }
    }
}

/// Convenience result alias for facade operations.
pub type OcResult<T> = Result<T, OcError>;
