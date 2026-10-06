//! Facade-owned serialized CXF admission policy, independent of model/run state.

use oce_store::Store;

use crate::{Engine, OcError};

/// Default and maximum supported serialized CXF size, in bytes (exactly 8 MiB).
///
/// This pre-deserialization bound is not a memory, CPU, graph-expansion, or sandbox guarantee.
/// Hosts still own transport limits, trust isolation and equipment safety policy.
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
