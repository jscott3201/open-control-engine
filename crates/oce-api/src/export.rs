//! CXF export and its completeness check.

use oce_cxf::ExportReport as CxfExportReport;

use crate::diagnostics::{DiagnosticCapture, DiagnosticStage, ExportReceipt, OperationFailure};
use crate::engine::Engine;
use crate::error::OcError;

/// A successful CXF export and any diagnostics for content deferred from the emitted document.
#[derive(Debug)]
#[non_exhaustive]
pub struct ExportReport {
    /// Deterministic UTF-8 JSON-LD bytes for the emitted CXF document.
    pub bytes: Vec<u8>,
    /// Diagnostics for blocks omitted from the survivor cone.
    pub warnings: Vec<oce_diag::Diagnostic>,
}

/// A partial export: [`ExportReport::complete`] found deferral warnings, so the emitted document
/// omits one or more blocks of the loaded model.
#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
#[error("the CXF export is partial: {warning_count} deferral warning(s)")]
#[non_exhaustive]
pub struct ExportIncomplete {
    /// Number of deferral warnings carried by the export report.
    pub warning_count: usize,
}

impl ExportReport {
    /// Check that the emitted document is the whole loaded model: no block was deferred.
    ///
    /// This is a completeness check only; the export carries no identity. The canonical content
    /// id of the loaded model comes from [`Engine::content_id`], which is defined whether or not
    /// the export is complete. A host that wants "the id of a completely exportable model" calls
    /// both. The check reads [`Self::warnings`] as it is now: the report is plain host-owned data,
    /// so a host that edits it is answered about the edited report.
    ///
    /// # Errors
    /// [`ExportIncomplete`] with the warning count when any block was deferred. Never panics.
    pub fn complete(&self) -> Result<(), ExportIncomplete> {
        if self.warnings.is_empty() {
            Ok(())
        } else {
            Err(ExportIncomplete {
                warning_count: self.warnings.len(),
            })
        }
    }
}

impl Engine {
    /// Export the current resolved model as deterministic CXF bytes.
    ///
    /// The document's root node carries the translation-software label, `S231:translationSoftware`
    /// (`open-control-engine`) and `S231:translationSoftwareVersion` (this engine's version), so
    /// the bytes of one model differ between engine versions. The canonical content id
    /// ([`Engine::content_id`]) does not: no importer reads the label back.
    ///
    /// # Errors
    /// Returns [`OcError::Cxf`] when the model cannot be exported, including an unloaded engine.
    /// Never panics for an unloaded engine.
    pub fn export_cxf(&self) -> Result<ExportReport, OcError> {
        self.export_cxf_pipeline(&mut DiagnosticCapture::new(false, DiagnosticStage::Export))
    }

    /// Export through the existing pipeline with immutable, machine-ordered producer evidence.
    ///
    /// Partial exports remain successful with warnings; [`ExportReport::complete`] tells them
    /// apart. No engine or store mutation occurs.
    ///
    /// # Errors
    /// Returns [`OperationFailure`] with the export stage and original error chain, even when the
    /// failure carries no diagnostics. An unloaded engine retains its export-unsupported evidence.
    pub fn export_cxf_with_receipt(&self) -> Result<ExportReceipt, OperationFailure> {
        let mut capture = DiagnosticCapture::new(true, DiagnosticStage::Export);
        match self.export_cxf_pipeline(&mut capture) {
            Ok(report) => Ok(ExportReceipt::new(report, capture)),
            Err(error) => Err(OperationFailure::new(error, capture)),
        }
    }

    fn export_cxf_pipeline(
        &self,
        capture: &mut DiagnosticCapture,
    ) -> Result<ExportReport, OcError> {
        let report: CxfExportReport = oce_cxf::export_with_report(&self.model)?;
        capture.record(&report.warnings);
        Ok(ExportReport {
            bytes: report.bytes,
            warnings: report.warnings,
        })
    }
}
