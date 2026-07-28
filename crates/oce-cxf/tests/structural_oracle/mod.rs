//! Shared plumbing for the structural-oracle conformance audit: locating the vendored
//! oracle corpus, loading JSON-LD documents, and reading the fixture manifest.
//!
//! The comparison semantics live in the sibling modules — [`flatten`] (hierarchy
//! expansion and class resolution), [`conditions`] (upstream conditional extraction and
//! evaluation), and [`compare`] (the diff itself, verdicts, and report rendering).

pub mod compare;
pub mod conditions;
pub mod flatten;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Root of the vendored third-party corpus: the pinned Buildings `.mo` sources and the
/// modelica-json CXF translations beside them.
pub fn vendor_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/modelica-buildings-cdl")
}

/// The oracle CXF document for an upstream class, if vendored.
pub fn doc_path(class: &str) -> PathBuf {
    vendor_root().join(format!("cxf/{}.jsonld", class.replace('.', "/")))
}

/// The upstream Modelica source for a class, if vendored.
pub fn mo_path(class: &str) -> PathBuf {
    vendor_root().join(format!("{}.mo", class.replace('.', "/")))
}

/// Load a JSON-LD document's `@graph` array from disk.
pub fn load_graph(path: &Path) -> Vec<Value> {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()));
    graph_of(&doc)
}

/// Extract the `@graph` array from an already-parsed document value.
pub fn graph_of(doc: &Value) -> Vec<Value> {
    doc.get("@graph")
        .and_then(Value::as_array)
        .cloned()
        .expect("document has no @graph array")
}

/// One manifest row: which upstream class a fixture translates, and its root IRI.
#[derive(Debug, Clone)]
pub struct ManifestEntry {
    /// Upstream class path, `None` for our own compositions with no upstream counterpart.
    pub upstream_class: Option<String>,
    /// The fixture's root model IRI (instance ids are `<root>.<path>`).
    pub root: String,
}

/// Fixture path (repo-relative) → manifest entry, in path order.
pub fn load_manifest() -> BTreeMap<String, ManifestEntry> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/g36/structural_oracle_manifest.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let v: Value = serde_json::from_str(&raw).expect("manifest parses");
    v.as_object()
        .expect("manifest is an object")
        .iter()
        .map(|(k, e)| {
            let cls = e
                .get("upstream_class")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let own = e
                .get("own_composition")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            assert_eq!(
                own,
                cls.is_none(),
                "manifest row {k}: own_composition must mirror a null upstream_class"
            );
            let root = e
                .get("root")
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("manifest row {k} has no root"))
                .to_owned();
            (
                k.clone(),
                ManifestEntry {
                    upstream_class: cls,
                    root,
                },
            )
        })
        .collect()
}

/// A fixture document on disk, by its repo-relative manifest path.
pub fn fixture_graph(repo_relative: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        repo_relative
            .strip_prefix("crates/oce-cxf/")
            .expect("manifest paths are repo-relative under crates/oce-cxf/"),
    );
    load_graph(&path)
}
