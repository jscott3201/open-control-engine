//! Pins of the S231 vocabulary against its two sources: the vendored `CXF-Core.jsonld` and the
//! block registry (GitHub #620).

use std::collections::BTreeSet;

use super::*;

/// The vendored CXF-Core, read as the vocabulary's source.
fn cxf_core() -> serde_json::Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../third_party/modelica-buildings-cdl/cxf-core/CXF-Core.jsonld"
    );
    let text = std::fs::read_to_string(path).expect("the vendored CXF-Core.jsonld");
    serde_json::from_str(&text).expect("CXF-Core.jsonld parses")
}

/// The `S231:` terms of a node's `@type` (a string or a list).
fn s231_types(node: &serde_json::Value) -> Vec<&str> {
    let values = match &node["@type"] {
        serde_json::Value::Array(values) => values.iter().collect(),
        value => vec![value],
    };
    values
        .into_iter()
        .filter_map(serde_json::Value::as_str)
        .filter_map(|token| token.strip_prefix("S231:"))
        .collect()
}

fn names(cxf_core: CxfCore) -> BTreeSet<&'static str> {
    TERMS
        .iter()
        .filter(|term| term.cxf_core == cxf_core)
        .map(|term| term.name)
        .collect()
}

#[test]
fn the_table_holds_exactly_what_cxf_core_declares_and_uses() {
    let core = cxf_core();
    let graph = core["@graph"].as_array().expect("@graph");
    let mut declared = BTreeSet::new();
    let mut used = BTreeSet::new();
    for node in graph {
        let types = &node["@type"];
        let is_class = match types {
            serde_json::Value::Array(values) => values.iter().any(|t| t == "rdfs:Class"),
            value => value == "rdfs:Class",
        };
        if is_class && let Some(id) = node["@id"].as_str().and_then(|id| id.strip_prefix("S231:")) {
            declared.insert(id.to_owned());
        }
        used.extend(s231_types(node).into_iter().map(str::to_owned));
        if let Some(datatype) = node["S231:isOfDataType"]["@id"]
            .as_str()
            .and_then(|id| id.strip_prefix("S231:"))
        {
            used.insert(datatype.to_owned());
        }
    }
    let table: BTreeSet<String> = names(Declared).into_iter().map(str::to_owned).collect();
    assert_eq!(
        declared, table,
        "CXF-Core.jsonld's declared classes changed: update `TERMS` in vocabulary_terms.rs (and \
         the docs that list the vocabulary); until then a newly declared class is refused"
    );
    let (paths, plain): (BTreeSet<String>, BTreeSet<String>) = used
        .difference(&declared)
        .cloned()
        .partition(|term| term.contains('.'));
    let table: BTreeSet<String> = names(UsedUndeclared)
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        plain, table,
        "the terms CXF-Core.jsonld uses without declaring them changed: update `TERMS` in \
         vocabulary_terms.rs; until then such a term is refused"
    );
    // A class path CXF-Core uses is one of the vocabulary's paths: today, its four CDL
    // enumeration types.
    let unknown: Vec<&String> = paths
        .iter()
        .filter(|path| !vocabulary().contains(path.as_str()))
        .collect();
    assert!(
        unknown.is_empty(),
        "CXF-Core uses class paths outside the vocabulary: {unknown:?}"
    );
    assert_eq!(paths.len(), 4, "{paths:?}");
}

#[test]
fn the_table_is_ordered_unique_and_names_the_engine_only_terms() {
    let all: Vec<&str> = TERMS.iter().map(|term| term.name).collect();
    let unique: BTreeSet<&str> = all.iter().copied().collect();
    assert_eq!(unique.len(), all.len(), "a term listed twice");
    for cxf_core in [Declared, UsedUndeclared, Absent] {
        let names: Vec<&str> = TERMS
            .iter()
            .filter(|t| t.cxf_core == cxf_core)
            .map(|t| t.name)
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "{cxf_core:?} terms in name order");
    }
    assert_eq!(names(Declared).len(), 23);
    assert_eq!(names(UsedUndeclared).len(), 8);
    // A term CXF-Core neither declares nor uses is in the table only because the engine writes it.
    for term in TERMS.iter().filter(|term| term.cxf_core == Absent) {
        assert!(term.engine, "{} is in no source", term.name);
    }
    assert_eq!(
        names(Absent),
        BTreeSet::from(["StringInput", "StringOutput"])
    );
}

#[test]
fn the_vocabulary_holds_every_registered_cdl_class_and_enumeration_row_by_full_path() {
    let classes: Vec<&str> = oce_blocks::catalog()
        .iter()
        .map(|entry| entry.class_path)
        .filter(|path| path.starts_with("CDL."))
        .collect();
    assert_eq!(
        classes.len(),
        133,
        "the block registry's CDL classes changed: the vocabulary follows it, so update this count \
         and the docs that state it"
    );
    let internal: Vec<&str> = oce_blocks::catalog()
        .iter()
        .map(|entry| entry.class_path)
        .filter(|path| !path.starts_with("CDL."))
        .collect();
    assert!(
        internal
            .iter()
            .all(|path| path.starts_with("urn:oce:lowering#")),
        "{internal:?}"
    );
    assert_eq!(internal.len(), 3, "{internal:?}");
    for path in &classes {
        assert!(
            vocabulary().contains(&format!("Buildings.Controls.OBC.{path}")),
            "{path}"
        );
        assert!(
            !vocabulary().contains(*path),
            "a class is held by its full path only: {path}"
        );
    }
    for path in &internal {
        assert!(
            !vocabulary().iter().any(|term| term.ends_with(path)),
            "{path}"
        );
    }
    let rows = oce_model::enumeration_classes();
    assert_eq!(
        rows.len(),
        18,
        "the enumeration rows changed: update this count and the docs"
    );
    for row in rows {
        assert!(
            vocabulary().contains(row.qualified_path),
            "{}",
            row.qualified_path
        );
        let last = row.qualified_path.rsplit('.').next().expect("a segment");
        assert!(
            !vocabulary().contains(last),
            "an enumeration is held by its full path only: {last}"
        );
    }
    assert_eq!(vocabulary().len(), 33 + 133 + 18);
}
