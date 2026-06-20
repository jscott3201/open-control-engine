#![forbid(unsafe_code)]
//! `oce-docs` — sequence-spec and point-list document export for the Open Control Engine
//! (`05`/`06`; CDL Ch. 9 §9.3).
//!
//! Produces the Ch. 9 Word/HTML sequence-specification document and the §7.7.5 Table 7.3
//! point list from the resolved model + effective metadata read through the `oce-store` seam.
//! Word/HTML export and CXF export are two separate output paths from the same model.
//!
//! The public document-export seam is reserved; exporters are deferred.

use oce_store::PointListRow;

/// Render a §7.7.5 Table 7.3 point list (System/Equipment, Name, Type, Hardwired?, Trend, Description)
/// to HTML.
#[must_use]
pub fn point_list_html(_rows: &[PointListRow]) -> String {
    unimplemented!("oce-docs::point_list_html — deferred to M4 exporters")
}
