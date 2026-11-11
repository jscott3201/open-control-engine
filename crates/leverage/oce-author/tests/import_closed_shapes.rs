//! The importers read no list as an object (GitHub #544). Unlike the wire reader they never fill a
//! `serde`-derived struct: both read the document as a `serde_json::Value` and match each node's
//! JSON type by hand (`import::cxf`, `import::nodes`, `import::values`, `import::v0`), so the
//! positional read the derive makes of a list cannot happen. These sweeps hold that: an object of
//! a document they read, replaced by the list of its values, is refused with a typed error, and
//! the same document as written imports.
//!
//! One exception, by design: the CXF importer drops a parameter node's descriptive attributes
//! (`S231:unit`, `quantity`, `displayUnit`, `min`, `max`; importer note, decision 9) without
//! reading them and reports each value as written, so a list there imports and changes nothing the
//! model carries. The sweep holds exactly that, and nothing else, for those positions.

use std::collections::BTreeSet;

use crate::support;

use oce_author::{ImportError, ImportOptions, Imported, import_cxf, import_graph_v0};

use support::cxf::{IMPORTED, corpus_fixture};
use support::fixture;
use support::spans::{Kind, Site, Step, sites};

/// An importer under sweep.
type Importer = fn(&[u8], &ImportOptions) -> Result<Imported, ImportError>;

/// The most objects swept in one document beyond its root and its `@context`: every object of a
/// document with fewer, otherwise an even stride, so the corpus stays inside the per-test time
/// bound in a debug build.
const MAX_OBJECTS: usize = 150;

/// The parameter attributes the CXF importer drops unread (decision 9).
const DROPPED_ATTRIBUTES: [&str; 5] = [
    "S231:unit",
    "S231:quantity",
    "S231:displayUnit",
    "S231:min",
    "S231:max",
];

/// What part of a CXF document an object is, for the coverage check.
fn role(text: &str, site: &Site) -> &'static str {
    let object = &text[site.span.clone()];
    match site.steps.as_slice() {
        [] => "document",
        [Step::Key(key)] if key == "@context" => "context",
        [Step::Key(key), Step::Index(_)] if key == "@graph" => "node",
        _ if object.contains("\"@value\"") => "literal",
        _ if object.contains("\"@id\"") => "reference",
        _ => "other",
    }
}

/// The objects of `text` a sweep replaces: the root and `@context`, then the rest, every one or
/// an even stride of at most [`MAX_OBJECTS`].
fn swept_objects(text: &str) -> Vec<Site> {
    let objects: Vec<Site> = sites(text)
        .into_iter()
        .filter(|s| s.kind == Kind::Object)
        .collect();
    let (top, rest): (Vec<Site>, Vec<Site>) = objects.into_iter().partition(|s| s.steps.len() <= 1);
    let stride = rest.len().div_ceil(MAX_OBJECTS).max(1);
    top.into_iter()
        .chain(rest.into_iter().step_by(stride))
        .collect()
}

/// Whether `site` is a descriptive attribute the importer drops and `imported` is what it must make
/// of the list there: the model of the document as written, with the list reported as written.
fn dropped_as_written(site: &Site, text: &str, imported: &Imported, original: &Imported) -> bool {
    let Some(Step::Key(key)) = site.steps.last() else {
        return false;
    };
    if !DROPPED_ATTRIBUTES.contains(&key.as_str()) {
        return false;
    }
    let list: serde_json::Value =
        serde_json::from_str(&site.list_of_values(text)).expect("the list is JSON");
    let written = list.to_string();
    imported.sequence == original.sequence
        && imported
            .report
            .dropped
            .iter()
            .any(|dropped| dropped.value == written)
}

/// Sweep `documents` with `importer`: every swept object replaced by the list of its values must
/// be refused (or, for a dropped attribute, carry nothing into the model). Returns the count
/// swept, the roles met and what broke the rule.
fn sweep(
    documents: &[(String, Vec<u8>)],
    importer: Importer,
) -> (usize, BTreeSet<&'static str>, Vec<String>) {
    let options = ImportOptions::new();
    let mut swept = 0usize;
    let mut roles = BTreeSet::new();
    let mut broken = Vec::new();
    for (name, bytes) in documents {
        let text = std::str::from_utf8(bytes).expect("UTF-8");
        let original = importer(bytes, &options).unwrap_or_else(|e| panic!("{name}: {e}"));
        for site in swept_objects(text) {
            swept += 1;
            roles.insert(role(text, &site));
            let mutant = site.as_list_of_values(text);
            match importer(mutant.as_bytes(), &options) {
                Err(_) => {}
                Ok(imported) if dropped_as_written(&site, text, &imported, &original) => {}
                Ok(_) => broken.push(format!("{name} {}: imported", site.path())),
            }
        }
    }
    (swept, roles, broken)
}

fn read(path: &std::path::Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn assert_swept(what: &str, swept: usize, broken: &[String]) {
    assert!(
        broken.is_empty(),
        "{} of {swept} {what} objects imported as the list of their values:\n{}",
        broken.len(),
        broken.join("\n")
    );
}

#[test]
fn every_emitted_cxf_object_refuses_the_list_of_its_values() {
    let mut paths: Vec<_> = std::fs::read_dir(fixture("emit"))
        .expect("emit goldens")
        .map(|e| e.expect("entry").path())
        .collect();
    paths.sort();
    let documents: Vec<(String, Vec<u8>)> = paths
        .iter()
        .map(|path| {
            let name = path
                .file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned();
            (name, read(path))
        })
        .collect();
    assert_eq!(documents.len(), 6, "the six emit goldens");
    let (swept, roles, broken) = sweep(&documents, import_cxf);
    assert_swept("emitted CXF", swept, &broken);
    assert!(swept > 300, "{swept}");
    for role in ["document", "context", "node", "reference"] {
        assert!(roles.contains(role), "{role}: {roles:?}");
    }
}

#[test]
fn every_corpus_cxf_object_refuses_the_list_of_its_values() {
    let documents: Vec<(String, Vec<u8>)> = IMPORTED
        .iter()
        .map(|name| ((*name).to_owned(), read(&corpus_fixture(name))))
        .collect();
    let (swept, roles, broken) = sweep(&documents, import_cxf);
    assert_swept("corpus CXF", swept, &broken);
    assert!(swept > 2_500, "{swept}");
    for role in ["document", "context", "node", "reference", "literal"] {
        assert!(roles.contains(role), "{role}: {roles:?}");
    }
}

#[test]
fn a_dropped_parameter_bound_written_as_a_list_is_reported_as_written() {
    // The exception the sweeps allow, pinned on one node: the corpus's `dpBuiSet` carries
    // `"S231:max": {"@value": "30", "@type": "…#double"}`, which the importer drops unread.
    let bytes = read(&corpus_fixture("multizone_vav_relief_damper"));
    let text = std::str::from_utf8(&bytes).expect("UTF-8");
    let site = sites(text)
        .into_iter()
        .find(|s| s.path() == "@graph[1].S231:max")
        .expect("the bound");
    let options = ImportOptions::new();
    let original = import_cxf(&bytes, &options).expect("imports");
    let imported = import_cxf(site.as_list_of_values(text).as_bytes(), &options).expect("imports");
    assert_eq!(imported.sequence, original.sequence, "nothing read from it");
    let subject = "http://example.org#g36.source.multizone_vav_relief_damper.dpBuiSet";
    let dropped = |report: &oce_author::ImportReport| {
        report
            .dropped
            .iter()
            .find(|d| d.subject == subject && d.property == oce_author::DroppedProperty::Max)
            .map(|d| d.value.clone())
    };
    assert_eq!(
        dropped(&original.report).as_deref(),
        Some(r#"{"@type":"http://www.w3.org/2001/XMLSchema#double","@value":"30"}"#)
    );
    assert_eq!(
        dropped(&imported.report).as_deref(),
        Some(r#"["30","http://www.w3.org/2001/XMLSchema#double"]"#)
    );
}

#[test]
fn every_v0_graph_object_refuses_the_list_of_its_values() {
    let documents: Vec<(String, Vec<u8>)> = ["ahu_0005", "ahu_0027", "width_max_min"]
        .iter()
        .map(|name| {
            let path = fixture(&format!("v0/{name}.graph.json"));
            ((*name).to_owned(), read(&path))
        })
        .collect();
    let options = ImportOptions::new();
    let mut swept = 0usize;
    for (name, bytes) in &documents {
        let text = std::str::from_utf8(bytes).expect("UTF-8");
        assert!(import_graph_v0(bytes, &options).is_ok(), "{name}");
        for site in sites(text).iter().filter(|s| s.kind == Kind::Object) {
            swept += 1;
            let mutant = site.as_list_of_values(text);
            match import_graph_v0(mutant.as_bytes(), &options) {
                Err(error) => assert_eq!(error.code(), "wrong-shape", "{name} {}", site.path()),
                Ok(_) => panic!("{name} {} imported as a list", site.path()),
            }
        }
    }
    assert!(swept > 25, "{swept}");
}
