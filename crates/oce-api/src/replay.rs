//! One canonical accepted-frame record; ordering, envelopes and execution remain host-owned.

use std::fmt;

use crate::{
    AssertEvent, CompatibilityDescriptor, CompatibilityMismatch, CompletedFrame, StatePortability,
    Value,
};

/// Inclusive canonical per-record byte limit: 64 MiB. Hosts also bound transport/envelope buffering.
pub const MAX_REPLAY_BYTES: usize = 64 * 1024 * 1024;

/// Closed replay comparison policy. This is not a conformance tolerance or platform qualification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayExactness {
    /// Every native value and diagnostic field agrees exactly; Real values compare raw bits.
    ExactBits,
}

/// FNV-1a-128 over every canonical record byte, INCLUDING its integrity trailer.
///
/// Noncryptographic content identity only, distinct from catalog, export, executable, deployment
/// and build identity. Authenticate exact bytes in the host envelope, never this collision-prone
/// tag. Display is `replay:1:fnv1a128:` followed by 32 lowercase hexadecimal digits.
///
/// ```compile_fail,E0308
/// fn confuse(id: oce_api::ReplayContentId) -> oce_api::CatalogContentId { id }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayContentId(u128);

impl fmt::Display for ReplayContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "replay:1:fnv1a128:{:032x}", self.0)
    }
}

/// Deterministic first-cause replay refusal. No variant authorizes a build or equipment action.
///
/// Decode checks size, header/version, length, integrity, then body fields in wire order.
/// Compatibility checks descriptor fields in canonical order, then target. Comparison checks
/// compatibility/placement, time, inputs, outputs, then diagnostics (each in stored order).
/// Index-bearing mismatches name the first unequal row, or the common length on count mismatch.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReplayError {
    /// Supplied or encoded bytes exceed the inclusive public bound.
    #[error("replay record exceeds the byte limit")]
    TooLarge {
        /// Supplied or attempted encoded byte count.
        actual_bytes: usize,
        /// Inclusive maximum byte count.
        limit_bytes: usize,
    },
    /// Short fixed header or wrong magic.
    #[error("invalid replay header")]
    Header,
    /// Unknown wire revision; no fallback or migration is attempted.
    #[error("unsupported replay format {revision}")]
    UnsupportedFormat {
        /// Carried wire revision.
        revision: u32,
    },
    /// Declared total/body/field length or count cannot fit the supplied bytes.
    #[error("inconsistent replay length at {offset}")]
    Length {
        /// Byte offset in the complete record.
        offset: usize,
    },
    /// The accidental-corruption trailer does not match. Not an authentication failure.
    #[error("replay integrity mismatch")]
    Integrity,
    /// The charged decoded allocation workspace exceeds 128 MiB, before allocation.
    #[error("replay decoded workspace limit exceeded")]
    WorkspaceLimit,
    /// Unknown exactness tag; tolerance acceptance is never inferred.
    #[error("unsupported replay exactness {tag}")]
    UnsupportedExactness {
        /// Carried tag.
        tag: u8,
    },
    /// Unknown placement tag.
    #[error("unknown replay placement {tag}")]
    UnknownPlacement {
        /// Carried tag.
        tag: u8,
    },
    /// Unknown native value discriminant.
    #[error("unknown replay value {tag}")]
    UnknownValue {
        /// Carried tag.
        tag: u8,
    },
    /// Unknown warning severity; there is no Error escalation.
    #[error("unknown replay severity {tag}")]
    UnknownSeverity {
        /// Carried tag.
        tag: u8,
    },
    /// A length-delimited string is not UTF-8.
    #[error("invalid replay UTF-8 at {offset}")]
    Utf8 {
        /// Start of the string bytes.
        offset: usize,
    },
    /// Duplicate/unordered/empty key, noncanonical Boolean/target label, or trailing bytes.
    #[error("noncanonical replay encoding at {offset}")]
    Noncanonical {
        /// Start of the offending field or unconsumed suffix.
        offset: usize,
    },
    /// The closed canonical public-descriptor grammar is malformed.
    #[error("malformed replay compatibility descriptor")]
    MalformedDescriptor,
    /// Unknown/noncanonical enum class path or ordinal outside its declared domain.
    #[error("invalid replay enum at {offset}")]
    EnumDomain {
        /// Start of the enum class field (zero for an unencodable in-memory value).
        offset: usize,
    },
    /// Accepted model time cannot be NaN or infinite. Signal Real bits remain unrestricted.
    #[error("nonfinite replay model time")]
    NonFiniteTime,
    /// Public descriptor facts disagree; this is not unique-build qualification.
    #[error("replay descriptor: {0}")]
    DescriptorMismatch(CompatibilityMismatch),
    /// Foreign architecture/OS, or recorded placement differs from the completed receipt.
    #[error("replay target placement mismatch")]
    TargetMismatch,
    /// Model time differs by at least one bit, including the sign of zero.
    #[error("replay model time mismatch")]
    TimeMismatch,
    /// First differing accepted input row or count boundary.
    #[error("replay input mismatch at {index}")]
    InputMismatch {
        /// Canonical row index, or common length when counts differ.
        index: usize,
    },
    /// First differing completed output row or count boundary.
    #[error("replay output mismatch at {index}")]
    OutputMismatch {
        /// Canonical row index, or common length when counts differ.
        index: usize,
    },
    /// First differing diagnostic (source, message, time bits, severity) or count boundary.
    #[error("replay diagnostic mismatch at {index}")]
    DiagnosticMismatch {
        /// Emission-order index, or common length when counts differ.
        index: usize,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct ReplayData {
    pub(crate) descriptor: String,
    pub(crate) portability: StatePortability,
    pub(crate) time: f64,
    pub(crate) inputs: Vec<(String, Value)>,
    pub(crate) outputs: Vec<(String, Value)>,
    pub(crate) diagnostics: Vec<AssertEvent>,
}

/// Independently owned, canonical revision-1 evidence for ONE accepted frame, not a replay log.
///
/// No sequence position, snapshots, executable fingerprint, build token or authentication is
/// embedded. Hosts authenticate/qualify exact bytes, executable/parameters and optional start/end
/// snapshot sidecars, and own order, omissions and duplicate policy. Equal times can be distinct
/// transitions. Portable placement is the state class policy, not universal mathematical exactness.
/// No serde format is promised. Use [`Self::from_bytes`] and [`Self::as_bytes`].
///
/// Decode never accesses an Engine or Store. Compatibility/comparison helpers are read-only;
/// the only execution path remains preparation followed by consuming execution. Comparison failure
/// AFTER execution cannot undo it. See `docs/replay-record.md` for the independently decodable wire
/// grammar, resource accounting and the full host workflow.
///
/// ```
/// use oce_api::{CompatibilityDescriptor, CompletedFrame, Engine, ReplayRecord};
/// // Host example, not an Engine method. The caller has authenticated/qualified the exact
/// // bytes, order, executable/build and compatible prior state (optional snapshot sidecar).
/// fn replay_one(isolated: &mut Engine, approved_bytes: &[u8])
///     -> Result<CompletedFrame, Box<dyn std::error::Error>>
/// {
///     let record = ReplayRecord::from_bytes(approved_bytes)?;
///     record.check_compatible(&CompatibilityDescriptor::current(None)?)?;
///     let inputs: Vec<_> = record.inputs().iter()
///         .map(|(path, value)| (path.as_str(), value.clone())).collect();
///     let plan = isolated.prepare_frame(record.time(), &inputs)?;
///     let completed = isolated.execute_frame(plan)?;
///     record.verify(&completed)?; // Failure here does not undo the accepted transition.
///     Ok(completed)
/// }
/// ```
#[derive(Clone, Debug)]
pub struct ReplayRecord {
    pub(crate) bytes: Vec<u8>,
    pub(crate) data: ReplayData,
}

impl CompletedFrame {
    /// Encode this accepted receipt without executing again or consulting current engine state.
    ///
    /// Captures current public descriptor facts with `export:none` (no export is performed).
    /// Retained inputs, outputs, warnings, time and placement belong to this receipt, even after
    /// reload/resume/restore. Sequence is deliberately omitted. Encoding sizes before allocation
    /// and validates its result with the same bounded canonical decoder used by hosts.
    ///
    /// # Errors
    /// Returns [`ReplayError`] for an oversized/unencodable record. Execution already succeeded:
    /// this is a record-capture failure, not a refused frame. No engine/Store mutation occurs.
    pub fn replay_record(&self) -> Result<ReplayRecord, ReplayError> {
        let descriptor = CompatibilityDescriptor::current(None)
            .expect("absent export is always complete")
            .to_string();
        let bytes = crate::replay_codec::encode(self, &descriptor)?;
        ReplayRecord::from_bytes(&bytes)
    }
}

impl ReplayRecord {
    /// Decode authenticated/host-qualified bytes, without Engine/Store access or execution.
    ///
    /// First validates all bytes and charges decoded workspace WITHOUT allocation, then allocates
    /// exact vector capacities and owned strings/bytes. At most 64 MiB canonical bytes plus 128 MiB
    /// charged decoded storage; allocator overhead, caller buffers and clones are separate. Linear
    /// scans, no recursive structures. Malformed input cannot panic; allocation failure is excluded.
    /// Eligibility is separate: call [`Self::check_compatible`] before preparation/execution.
    ///
    /// # Errors
    /// Typed size/header/version/length/integrity and canonical field refusals, in documented order.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ReplayError> {
        crate::replay_codec::decode(bytes)
    }

    /// Exact canonical bytes including header and trailer. No allocation or mutation.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Wire format revision, currently 1; independent of state ABI and descriptor revisions.
    #[must_use]
    pub fn format_revision(&self) -> u32 {
        1
    }

    /// Closed comparison policy, never a tolerance policy.
    #[must_use]
    pub fn exactness(&self) -> ReplayExactness {
        ReplayExactness::ExactBits
    }

    /// Exact canonical public descriptor UTF-8. Inspection is not build authentication.
    #[must_use]
    pub fn descriptor(&self) -> &str {
        &self.data.descriptor
    }

    /// Executable placement restriction captured with this receipt. Not build authority.
    #[must_use]
    pub fn portability(&self) -> &StatePortability {
        &self.data.portability
    }

    /// Finite accepted model seconds with exact binary64 bits, including signed zero.
    #[must_use]
    pub fn time(&self) -> f64 {
        self.data.time
    }

    /// All accepted boundary inputs in strict UTF-8 lexical key order, with native exact values.
    #[must_use]
    pub fn inputs(&self) -> &[(String, Value)] {
        &self.data.inputs
    }

    /// All completed boundary outputs in strict UTF-8 lexical key order, with native exact values.
    #[must_use]
    pub fn outputs(&self) -> &[(String, Value)] {
        &self.data.outputs
    }

    /// Warnings in evaluator emission order. Duplicates are meaningful and are not sorted away.
    #[must_use]
    pub fn diagnostics(&self) -> &[AssertEvent] {
        &self.data.diagnostics
    }

    /// Compute the noncryptographic typed identity over the exact complete canonical bytes.
    /// Linear in byte length, with no allocation or mutation.
    #[must_use]
    pub fn content_id(&self) -> ReplayContentId {
        ReplayContentId(crate::replay_codec::hash(&self.bytes))
    }

    /// Require exact expected public facts and local target placement before engine mutation.
    ///
    /// The caller first qualifies the same build/deployment, executable and prior state externally.
    /// This is not executable identity or authentication. Unknown exactness never decodes in v1.
    /// Allocates the bounded canonical expected descriptor; does not access or execute an engine.
    ///
    /// # Errors
    /// First differing descriptor field, then foreign target architecture/OS.
    pub fn check_compatible(&self, expected: &CompatibilityDescriptor) -> Result<(), ReplayError> {
        crate::replay_descriptor::compare(&self.data.descriptor, &expected.to_string())?;
        if let StatePortability::TargetBound { arch, os } = self.portability()
            && (arch != std::env::consts::ARCH || os != std::env::consts::OS)
        {
            return Err(ReplayError::TargetMismatch);
        }
        Ok(())
    }

    /// Compare one already-completed receipt bit-for-bit, without mutation or re-execution.
    ///
    /// Checks descriptor/placement, exact time, inputs, outputs and each diagnostic field/order.
    /// No sequence-position check or tolerance exists. An error does NOT roll back execution;
    /// use an isolated replay engine and keep equipment authority outside this workflow.
    ///
    /// # Errors
    /// First typed mismatch in the order above. Counts differ at the common-length index.
    pub fn verify(&self, completed: &CompletedFrame) -> Result<(), ReplayError> {
        self.check_compatible(
            &CompatibilityDescriptor::current(None).expect("absent export is always complete"),
        )?;
        if completed.target_bound
            != matches!(self.portability(), StatePortability::TargetBound { .. })
        {
            return Err(ReplayError::TargetMismatch);
        }
        if self.time().to_bits() != completed.time().to_bits() {
            return Err(ReplayError::TimeMismatch);
        }
        if let Some(index) = different_values(self.inputs(), completed.inputs()) {
            return Err(ReplayError::InputMismatch { index });
        }
        if let Some(index) = different_values(self.outputs(), completed.outputs()) {
            return Err(ReplayError::OutputMismatch { index });
        }
        let actual = completed.diagnostics();
        let index = self
            .diagnostics()
            .iter()
            .zip(actual)
            .position(|(a, b)| {
                a.block != b.block
                    || a.message != b.message
                    || a.t.to_bits() != b.t.to_bits()
                    || a.level != b.level
            })
            .or_else(|| {
                (self.diagnostics().len() != actual.len())
                    .then_some(self.diagnostics().len().min(actual.len()))
            });
        index.map_or(Ok(()), |index| {
            Err(ReplayError::DiagnosticMismatch { index })
        })
    }
}

fn different_values(a: &[(String, Value)], b: &[(String, Value)]) -> Option<usize> {
    a.iter()
        .zip(b)
        .position(|(a, b)| a.0 != b.0 || !a.1.bit_eq(&b.1))
        .or_else(|| (a.len() != b.len()).then_some(a.len().min(b.len())))
}
