#![forbid(unsafe_code)]
//! Verification over vectors for Open Control Engine sequences (design note
//! `docs/design/vectors.md`, issue #519).
//!
//! Status: the first part of the verification product (M3). It holds the one vector format,
//! `oce/vectors/v1`, and nothing that runs: it loads no engine, replays nothing and writes no
//! receipt. Binding a set to a sequence, the run, receipts and mutants build on it.
//!
//! A **vector set** is a strict-JSON document of scenarios, each a schedule of inputs over one
//! clock and expectations over windows of it, to be checked against one loaded sequence. This
//! crate holds five things about it:
//!
//! - **The reader** ([`VectorSet::from_json_bytes`]): strict and bounded. The document is one
//!   UTF-8 JSON value under the authoring wire form's profile ([`MAX_DEPTH`], [`MAX_LIST_LEN`],
//!   [`MAX_STRING_BYTES`], no repeated key, no control character, no non-finite number) and the
//!   lowerable [`VerifyLimits`]; every object is closed, so a key the format does not define
//!   (`within`, `of`, `when` and `mutants` among them, until the format gains them) is refused,
//!   never ignored. It takes bytes, opens nothing and never panics.
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
//!     br#"{"schema": "oce/vectors/v1", "mutants": []}"#,
//!     &VerifyLimits::default(),
//! );
//! assert_eq!(refused.unwrap_err().code(), "unknown-key");
//! # Ok::<(), oce_verify::VectorError>(())
//! ```

mod canonical;
mod clock;
mod decode;
mod dto;
mod error;
mod fnv;
mod limits;
mod model;
mod number;
mod schema;
mod strict;
mod validate;
mod vector_set;

pub use clock::Clock;
pub use error::{Location, VECTOR_ERROR_CODES, VectorError, VectorErrorKind};
pub use limits::{MAX_DEPTH, MAX_LIST_LEN, MAX_STRING_BYTES, MAX_TIME_INTEGER, VerifyLimits};
pub use model::{DiagnosticInstant, Expectation, Input, Parameter, Scenario, Step, Window};
pub use number::Literal;
pub use schema::{SCHEMA_PROFILE_KEYWORDS, VECTORS_JSON_SCHEMA, vectors_json_schema};
pub use vector_set::VectorSet;

/// The format tag a vector set carries in its `schema` key. Any other tag is refused as
/// `unknown-schema`.
pub const VECTORS_SCHEMA: &str = "oce/vectors/v1";
