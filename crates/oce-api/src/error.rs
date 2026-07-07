//! The unified, typed facade error (`08` §7). Never panics on host input (R-ERR-1);
//! `#[non_exhaustive]` so variants evolve additively (R-ERR-2). It wraps the Group A pipeline
//! errors and the store's `StoreError`, and — critically — **never wraps a backend-specific error
//! type directly**: any durable-store adapter flattens its native errors to
//! `StoreError::Backend(String)` at the seam (R-ERR-3), so the facade error surface is identical in
//! shape regardless of which `Store` backend is wired.

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

/// Convenience result alias for facade operations.
pub type OcResult<T> = Result<T, OcError>;
