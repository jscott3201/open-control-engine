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

/// The reason a checked CXF content identity could not be minted.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContentIdError {
    /// The emitted document is partial because one or more blocks were deferred.
    #[error("cannot mint a complete CXF content identity: export has {warning_count} warning(s)")]
    #[non_exhaustive]
    Incomplete {
        /// Number of deferral warnings carried by the export report.
        warning_count: usize,
    },
}

impl ExportReport {
    /// Return an unchecked non-cryptographic integrity tag over exactly [`Self::bytes`].
    ///
    /// This is not [`crate::LoadReport::model_id`]: that identity preserves the authored
    /// top-composite `@id`, while export uses a synthetic root, and resumed parameter edits change
    /// exported bytes without recomputing `model_id`.
    ///
    /// When [`Self::warnings`] is non-empty, the id identifies only the partial emitted survivor
    /// document. Hosts minting version identities must call [`Self::content_id_complete`] instead.
    #[deprecated(
        since = "0.1.0",
        note = "use ExportReport::content_id_complete to reject partial exports"
    )]
    #[must_use]
    pub fn content_id(&self) -> String {
        self.content_id_unchecked()
    }

    /// Return the exported-document integrity tag only when the export is complete.
    ///
    /// The tag is FNV-1a-128 over exactly [`Self::bytes`], formatted as
    /// `cxf:fnv1a128:<32 lowercase hex characters>`. This documents how the returned tag is
    /// computed so hosts can verify it independently; call this method to obtain an identity so
    /// partial exports are refused. The tag is not a security boundary. Hosts requiring a
    /// cryptographic digest must hash the same bytes themselves after this completeness check.
    ///
    /// The FNV computation is reproducible independently:
    ///
    /// ```
    /// let bytes = b"abc";
    /// let mut hash = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d_u128;
    /// for byte in bytes {
    ///     hash ^= u128::from(*byte);
    ///     hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    /// }
    /// assert_eq!(hash, 0xa68d_622c_ec8b_5822_836d_bc79_77af_7f3b);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`ContentIdError::Incomplete`] with the warning count when any blocks were deferred
    /// from the emitted survivor document. It never panics.
    pub fn content_id_complete(&self) -> Result<String, ContentIdError> {
        if self.warnings.is_empty() {
            Ok(self.content_id_unchecked())
        } else {
            Err(ContentIdError::Incomplete {
                warning_count: self.warnings.len(),
            })
        }
    }

    fn content_id_unchecked(&self) -> String {
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
