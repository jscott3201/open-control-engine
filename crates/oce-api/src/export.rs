//! CXF export and emitted-document identity.

use oce_cxf::ExportReport as CxfExportReport;
use oce_store::Store;

use crate::engine::Engine;
use crate::error::OcError;
use crate::stable_hash::StableHash;

/// A successful CXF export and any diagnostics for content deferred from the emitted document.
#[derive(Debug)]
#[non_exhaustive]
pub struct ExportReport {
    /// Deterministic UTF-8 JSON-LD bytes for the emitted CXF document.
    pub bytes: Vec<u8>,
    /// Diagnostics for blocks omitted from the survivor cone.
    pub warnings: Vec<oce_diag::Diagnostic>,
}

impl ExportReport {
    /// Return a non-cryptographic integrity tag computed over exactly [`Self::bytes`].
    ///
    /// A host can reproduce it by applying FNV-1a-128 directly to the bytes:
    ///
    /// ```
    /// let bytes = b"abc";
    /// let mut hash = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d_u128;
    /// for byte in bytes {
    ///     hash ^= u128::from(*byte);
    ///     hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    /// }
    /// assert_eq!(format!("cxf:fnv1a128:{hash:032x}").len(), 45);
    /// ```
    ///
    /// This tag is not a security boundary. Hosts requiring a cryptographic digest must hash the
    /// same bytes themselves.
    ///
    /// This is not [`crate::LoadReport::model_id`]: that identity preserves the authored
    /// top-composite `@id`, while export uses a synthetic root, and resumed parameter edits change
    /// exported bytes without recomputing `model_id`.
    ///
    /// When [`Self::warnings`] is non-empty, the id identifies only the partial emitted survivor
    /// document. Hosts minting version identities must require an empty warning list.
    #[must_use]
    pub fn content_id(&self) -> String {
        let mut hash = StableHash::new();
        hash.write_bytes(&self.bytes);
        format!("cxf:fnv1a128:{:032x}", hash.finish())
    }
}

impl<S: Store> Engine<S> {
    /// Export the current resolved model as deterministic CXF bytes.
    ///
    /// # Errors
    /// Returns [`OcError::Cxf`] when the model cannot be exported, including an unloaded engine.
    /// Never panics for an unloaded engine.
    pub fn export_cxf(&self) -> Result<ExportReport, OcError> {
        let report: CxfExportReport = oce_cxf::export_with_report(&self.model)?;
        Ok(ExportReport {
            bytes: report.bytes,
            warnings: report.warnings,
        })
    }
}
