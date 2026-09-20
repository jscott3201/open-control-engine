//! Read-only snapshot inspection and bounded byte admission, separate from restore authority.

use std::sync::Arc;

use crate::{EngineStateError, EngineStateSnapshot};

/// Placement restriction encoded by a canonical state snapshot, not numerical qualification.
///
/// Host build/deployment qualification is a separate mandatory precondition. Neither variant
/// authenticates bytes or proves arbitrary-input, cross-build or cross-platform exactness. The
/// retained 21-signal Linux corpus does not relax the conservative 15-class target-bound policy;
/// it is finite-corpus evidence, not a whole-executable or macOS guarantee.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StatePortability {
    /// No architecture/OS restriction in this execution ABI's class policy. Restore still checks
    /// the entire executable manifest and state; this is not universal mathematical equivalence.
    Portable,
    /// At least one class requires the capture architecture and operating system. These are
    /// Rust's `std::env::consts` labels, not a compiler, ABI triple or build identity.
    TargetBound {
        /// Capture architecture, compared exactly at restore.
        arch: String,
        /// Capture operating system, compared exactly at restore.
        os: String,
    },
}

impl EngineStateSnapshot {
    /// Parse one bounded canonical snapshot after the host has verified its sealed envelope.
    ///
    /// Checks the 64 MiB cap, header/format, corruption checksum, canonical encoding, manifest
    /// self-consistency and fingerprint. It neither authenticates bytes nor admits a build.
    /// Unknown execution ABI revisions can parse; target compatibility and class-specific state
    /// invariants are checked only by [`crate::Engine::restore_state`] before mutation.
    ///
    /// Allocates owned bytes and a decoded image with bounded decoder workspaces. The byte cap
    /// is not a process peak-memory guarantee. Malformed input returns [`EngineStateError`], not
    /// a caller-input panic; allocation failure/process termination are outside the contract.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EngineStateError> {
        let image = crate::state_codec::decode_snapshot(bytes)?;
        Ok(Self {
            bytes: Arc::from(bytes),
            image: Arc::new(image),
        })
    }

    /// Borrow the canonical byte stream without allocation or mutation.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Copy the canonical byte stream to an owned vector, consuming this handle.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes.as_ref().to_vec()
    }

    /// Borrow the encoded placement policy without allocation or panic.
    ///
    /// Inspection is not restore admission, especially for an unknown execution ABI. Always use
    /// [`crate::Engine::restore_state`] on a fresh compatible target after host-envelope approval.
    ///
    /// ```
    /// use oce_api::{EngineStateSnapshot, StatePortability};
    /// fn capture_target(snapshot: &EngineStateSnapshot) -> Option<(&str, &str)> {
    ///     match snapshot.portability() {
    ///         StatePortability::TargetBound { arch, os } => Some((arch, os)),
    ///         _ => None, // Absence of a target restriction is not build approval.
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn portability(&self) -> &StatePortability {
        &self.image.manifest.portability
    }
}
