//! Facade-owned serialized CXF admission policy, independent of model/run state.

use oce_store::Store;

use crate::{Engine, OcError};

/// Default and maximum supported serialized CXF size, in bytes (exactly 8 MiB).
///
/// This pre-deserialization bound is not by itself a memory, CPU, or sandbox guarantee. Within
/// it, the ingest pipeline bounds these input-derived amplifications, each with a typed refusal
/// rather than a panic or allocation abort (issue #365): lowering visits each `containsBlock`
/// composite and leaf at most once (`shared-instance`); inherited parameter scopes are shared,
/// not copied per leaf; array parameters total at most 1,048,576 elements per document and run
/// state at most 4,194,304 words (`resource-limit-exceeded`); and binding evaluation spends at
/// most 2^24 work units per expression and 2^26 per document (`grounding-failed`). These bound
/// growth, not absolute footprint or time: a maximal admissible document can still need
/// hundreds of MiB and a noticeable load. Hosts still own transport limits, trust isolation of
/// untrusted programs, and equipment safety policy.
pub const MAX_CXF_BYTES: usize = 8 * 1024 * 1024;

impl<S: Store> Engine<S> {
    /// Effective serialized CXF byte limit for both load entry points.
    ///
    /// Defaults to [`MAX_CXF_BYTES`]; persists across successful and failed reloads.
    #[must_use]
    pub fn cxf_byte_limit(&self) -> usize {
        self.cxf_byte_limit
    }

    /// Set a host-specific serialized CXF byte limit no larger than [`MAX_CXF_BYTES`].
    ///
    /// The inclusive range is `0..=MAX_CXF_BYTES`. Zero refuses every nonempty input;
    /// an empty input still reaches normal JSON validation. Changes affect subsequent
    /// loads only, not the current executable/run image, Store, or snapshot bytes.
    /// The limit may be changed again within this range. Never panics.
    ///
    /// # Errors
    /// [`OcError::CxfByteLimitTooLarge`] for a value above the supported maximum,
    /// leaving the effective limit and all engine/store state unchanged.
    pub fn set_cxf_byte_limit(&mut self, limit_bytes: usize) -> Result<(), OcError> {
        if limit_bytes > MAX_CXF_BYTES {
            return Err(OcError::CxfByteLimitTooLarge {
                actual_bytes: limit_bytes,
                limit_bytes: MAX_CXF_BYTES,
            });
        }
        self.cxf_byte_limit = limit_bytes;
        Ok(())
    }
}
