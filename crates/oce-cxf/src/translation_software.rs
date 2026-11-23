//! The translation-software label every exported document carries (GitHub #527).
//!
//! The OpenBuildingControl (OBC) specification lets a CXF document name the software that wrote
//! it (OBC §8.4: "shall optionally include the name and the version of the software that
//! generated it"). The two properties are `translationSoftware` and `translationSoftwareVersion`
//! (OBC §8.2, Table 8.2), which modelica-json's `CXF-Core.jsonld` declares as
//! `S231:translationSoftware` and `S231:translationSoftwareVersion` with domain `S231:Block` and
//! range `S231:String`. The exporter writes both, as two JSON strings on the root composite node.
//!
//! The label is provenance, not behaviour. The importer models neither property: both stay in the
//! node's pass-through map, in any spelling, on any node, with any value, and nothing downstream
//! reads them. They never reach [`oce_model::ModelGraph`], so they are in no content id, blob or
//! snapshot, and a document that carries another tool's label, a malformed one or none loads the
//! same model. On export this engine writes its own label, whatever the source document said.
//!
//! The version is this crate's package version, which is the workspace version. Every exported
//! document therefore changes when the workspace version does.

#[cfg(test)]
use serde_json::Value as Json;

#[cfg(test)]
use crate::dto::Node;

/// The property naming the software that wrote a document.
pub(crate) const SOFTWARE_KEY: &str = "S231:translationSoftware";
/// The property naming that software's version.
pub(crate) const VERSION_KEY: &str = "S231:translationSoftwareVersion";
/// The name this engine writes.
pub(crate) const SOFTWARE: &str = "open-control-engine";
/// The version this engine writes: the workspace version.
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The label's two properties and their values, in the order a node's map of other keys writes
/// them (sorted by key): what the exporter writes on the root, after the root's fields.
pub(crate) const LABEL: [(&str, &str); 2] = [(SOFTWARE_KEY, SOFTWARE), (VERSION_KEY, VERSION)];

/// Write the label on `root`, the document's root composite node: the tests' reference for the
/// document built in full, which the exporter writes from [`LABEL`] without building a node.
#[cfg(test)]
pub(crate) fn stamp(root: &mut Node) {
    for (key, value) in LABEL {
        root.other
            .insert(key.to_owned(), Json::String(value.to_owned()));
    }
}
