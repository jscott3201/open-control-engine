//! The importers read no list as an object (GitHub #544). Unlike the wire reader they never fill a
//! `serde`-derived struct: both read the document as a `serde_json::Value` and match each node's
//! JSON type by hand (`import::cxf`, `import::nodes`, `import::values`, `import::v0`), so the
//! positional read the derive makes of a list cannot happen. These sweeps hold that: an object of
//! a document they read, replaced by the list of its values, is refused with one of the codes in
//! [`REFUSAL_CODES`], and the same document as written imports.
//!
//! There is no exception. Until GitHub issue #559 a parameter node's descriptive attributes
//! (`S231:unit`, `quantity`, `displayUnit`, `min`, `max`) were dropped without any check, so a
//! list there imported; the importer now checks their shape before dropping them (owner ruling of
//! 2026-11-11), and a list there is `wrong-shape` like a list anywhere else.

use std::collections::BTreeSet;

use crate::support;

use oce_author::{
    ImportError, ImportErrorKind, ImportOptions, Imported, SourceForm, import_cxf, import_graph_v0,
};

use support::cxf::{IMPORTED, corpus_fixture};
use support::fixture;
use support::spans::{Kind, Site, Step, sites};

/// An importer under sweep.
type Importer = fn(&[u8], &ImportOptions) -> Result<Imported, ImportError>;

/// The most objects swept in one document beyond its root and its `@context`: every object of a
/// document with fewer, otherwise an even stride, so the corpus stays inside the per-test time
/// bound in a debug build.
const MAX_OBJECTS: usize = 150;

/// The codes the CXF importer refuses an object written as the list of its values with:
/// `wrong-shape` (a document, node, reference or literal of another JSON type), `array-shape` (a
/// list where one value belongs) and `expression-value`, which its fixed precedence (importer
/// note) reaches before the shape: a list of strings under `S231:value` is read as candidate
/// expressions, and a list in place of a `hasParameter` reference hides the sequence parameter a
/// value names.
const REFUSAL_CODES: [&str; 3] = ["wrong-shape", "array-shape", "expression-value"];

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

/// What a CXF sweep met: the objects swept, their roles, the refusal codes and what broke the
/// rule.
#[derive(Default)]
struct Swept {
    count: usize,
    roles: BTreeSet<&'static str>,
    codes: BTreeSet<&'static str>,
    broken: Vec<String>,
}

/// Sweep `documents` with `importer`: every swept object replaced by the list of its values must
/// be refused with a code of [`REFUSAL_CODES`].
fn sweep(documents: &[(String, Vec<u8>)], importer: Importer) -> Swept {
    let options = ImportOptions::new();
    let mut swept = Swept::default();
    for (name, bytes) in documents {
        let text = std::str::from_utf8(bytes).expect("UTF-8");
        importer(bytes, &options).unwrap_or_else(|e| panic!("{name}: {e}"));
        for site in swept_objects(text) {
            swept.count += 1;
            swept.roles.insert(role(text, &site));
            let mutant = site.as_list_of_values(text);
            match importer(mutant.as_bytes(), &options) {
                Err(error) if REFUSAL_CODES.contains(&error.code()) => {
                    swept.codes.insert(error.code());
                }
                Err(error) => swept
                    .broken
                    .push(format!("{name} {}: refused as {error}", site.path())),
                Ok(_) => swept
                    .broken
                    .push(format!("{name} {}: imported", site.path())),
            }
        }
    }
    swept
}

fn read(path: &std::path::Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn assert_swept(what: &str, swept: &Swept) {
    assert!(
        swept.broken.is_empty(),
        "{} of {} {what} objects were not refused by a listed code as the list of their values:\n{}",
        swept.broken.len(),
        swept.count,
        swept.broken.join("\n")
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
    let swept = sweep(&documents, import_cxf);
    assert_swept("emitted CXF", &swept);
    assert!(swept.count > 300, "{}", swept.count);
    for role in ["document", "context", "node", "reference"] {
        assert!(swept.roles.contains(role), "{role}: {:?}", swept.roles);
    }
    assert_eq!(
        swept.codes,
        REFUSAL_CODES.into_iter().collect(),
        "every listed code is met"
    );
}

#[test]
fn every_corpus_cxf_object_refuses_the_list_of_its_values() {
    let documents: Vec<(String, Vec<u8>)> = IMPORTED
        .iter()
        .map(|name| ((*name).to_owned(), read(&corpus_fixture(name))))
        .collect();
    let swept = sweep(&documents, import_cxf);
    assert_swept("corpus CXF", &swept);
    assert!(swept.count > 2_500, "{}", swept.count);
    for role in ["document", "context", "node", "reference", "literal"] {
        assert!(swept.roles.contains(role), "{role}: {:?}", swept.roles);
    }
    assert_eq!(
        swept.codes,
        REFUSAL_CODES.into_iter().collect(),
        "every listed code is met"
    );
}

#[test]
fn a_parameter_bound_written_as_a_list_is_refused_as_wrong_shape() {
    // The exception the sweeps allowed until GitHub issue #559, pinned on its node: the corpus's
    // `dpBuiSet` carries `"S231:max": {"@value": "30", "@type": "…#double"}`. The list of its
    // values imported, dropped unread; it is now refused before the drop.
    let bytes = read(&corpus_fixture("multizone_vav_relief_damper"));
    let text = std::str::from_utf8(&bytes).expect("UTF-8");
    let site = sites(text)
        .into_iter()
        .find(|s| s.path() == "@graph[1].S231:max")
        .expect("the bound");
    let options = ImportOptions::new();
    import_cxf(&bytes, &options).expect("the document as written imports");
    assert_eq!(
        import_cxf(site.as_list_of_values(text).as_bytes(), &options).err(),
        Some(ImportError::new(
            SourceForm::Cxf,
            "http://example.org#g36.source.multizone_vav_relief_damper.dpBuiSet",
            ImportErrorKind::WrongShape {
                key: "S231:max".to_owned(),
            },
        ))
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
