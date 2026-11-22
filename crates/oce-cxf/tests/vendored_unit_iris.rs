//! Every unit and display-unit IRI the vendored upstream documents write is one the closed unit
//! table maps (issue #631; design note `docs/design/unit-spellings.md`, "Measurements").
//!
//! The corpus is the 45 modelica-json translations under
//! `third_party/modelica-buildings-cdl/cxf/` and `cxf-core/CXF-Core.jsonld`, read as data. The
//! walk visits every JSON object at any depth, so it sees CXF-Core's class declarations and the
//! translations' parameters as well as the connectors a load reads. A key is matched the way the
//! resolver matches it, by its IRI after `@context` expansion, with a small expansion written here
//! and not shared with the crate; the lookup is the crate's ([`oce_cxf::unit_of_iri`]).
//!
//! The expected counts are the note's, stated by hand from its "Where each spelling occurs" and
//! "The IRIs in use" tables. A count that moves means the vendored tree moved, which is a pin
//! advance, or the table lost an entry the corpus uses.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use oce_cxf::{QUDT_SCHEMA_NAMESPACES, unit_of_iri};
use serde_json::{Map, Value as Json};

const S231: &str = "http://data.ashrae.org/S231#";

fn vendored_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../third_party/modelica-buildings-cdl")
}

/// Every `.jsonld` file under `dir`, sorted.
fn documents(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("read vendored directory") {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "jsonld") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// `token` expanded against the document's prefix map: a compact IRI through a bound prefix, or
/// the token as written.
fn expanded(token: &str, context: &Map<String, Json>) -> String {
    match token.split_once(':') {
        Some((prefix, suffix)) if !suffix.starts_with("//") => {
            match context.get(prefix).and_then(Json::as_str) {
                Some(namespace) => format!("{namespace}{suffix}"),
                None => token.to_owned(),
            }
        }
        _ => token.to_owned(),
    }
}

/// Which property an expanded key names: `unit`, `displayUnit`, or neither.
fn property(key_iri: &str) -> Option<&'static str> {
    if QUDT_SCHEMA_NAMESPACES
        .iter()
        .any(|namespace| key_iri.strip_prefix(namespace) == Some("hasUnit"))
    {
        return Some("unit");
    }
    match key_iri.strip_prefix(S231)? {
        "unit" | "hasUnit" => Some("unit"),
        "displayUnit" | "hasDisplayUnit" => Some("displayUnit"),
        _ => None,
    }
}

/// What one corpus (the translations, or CXF-Core) writes.
#[derive(Debug, Default, PartialEq, Eq)]
struct Census {
    /// Documents read.
    documents: usize,
    /// Documents that carry at least one unit or display unit.
    carrying: usize,
    /// The authored key of each unit, counted.
    unit_keys: BTreeMap<String, usize>,
    /// The authored key of each display unit, counted.
    display_unit_keys: BTreeMap<String, usize>,
    /// The unit string each unit reads as, counted.
    units: BTreeMap<String, usize>,
    /// The unit string each display unit reads as, counted.
    display_units: BTreeMap<String, usize>,
}

/// Visit every JSON object under `value`, tallying each unit and display unit it carries.
/// Panics, naming the file and the IRI, on a value that is not an IRI node, an IRI the table does
/// not map, or an object that spells one property twice.
fn visit(value: &Json, context: &Map<String, Json>, file: &Path, census: &mut Census) -> usize {
    let mut found = 0;
    match value {
        Json::Array(items) => {
            for item in items {
                found += visit(item, context, file, census);
            }
        }
        Json::Object(object) => {
            let mut spelled: BTreeMap<&'static str, &str> = BTreeMap::new();
            for (key, value) in object {
                if key != "@context" {
                    found += visit(value, context, file, census);
                }
                let Some(term) = property(&expanded(key, context)) else {
                    continue;
                };
                if let Some(first) = spelled.insert(term, key) {
                    panic!(
                        "{}: one node spells `{term}` twice, as `{first}` and as `{key}`",
                        file.display()
                    );
                }
                let id = match value.as_object() {
                    Some(node) if node.len() == 1 => node.get("@id").and_then(Json::as_str),
                    _ => None,
                }
                .unwrap_or_else(|| panic!("{}: `{key}` is not an IRI node", file.display()));
                let iri = expanded(id, context);
                let unit = unit_of_iri(&iri).unwrap_or_else(|| {
                    panic!(
                        "{}: `{key}` names `{iri}`, which the table does not map",
                        file.display()
                    )
                });
                let (keys, units) = if term == "unit" {
                    (&mut census.unit_keys, &mut census.units)
                } else {
                    (&mut census.display_unit_keys, &mut census.display_units)
                };
                *keys.entry(key.clone()).or_default() += 1;
                *units.entry(unit.to_owned()).or_default() += 1;
                found += 1;
            }
        }
        _ => {}
    }
    found
}

fn census(files: &[PathBuf]) -> Census {
    let mut census = Census::default();
    for file in files {
        let text = std::fs::read_to_string(file).expect("vendored document is readable");
        let document: Json = serde_json::from_str(&text).expect("vendored document is JSON");
        let context = document["@context"]
            .as_object()
            .unwrap_or_else(|| panic!("{}: no inline @context", file.display()));
        census.documents += 1;
        if visit(&document["@graph"], context, file, &mut census) > 0 {
            census.carrying += 1;
        }
    }
    census
}

fn counted(entries: &[(&str, usize)]) -> BTreeMap<String, usize> {
    entries
        .iter()
        .map(|(name, count)| ((*name).to_owned(), *count))
        .collect()
}

#[test]
fn every_unit_iri_of_the_45_translations_maps() {
    let files = documents(&vendored_root().join("cxf"));
    let expected = Census {
        documents: 45,
        // 35 carry a `qudt:hasUnit`; every document with a display unit is among them.
        carrying: 35,
        unit_keys: counted(&[("qudt:hasUnit", 421)]),
        display_unit_keys: counted(&[("S231:hasDisplayUnit", 64)]),
        // `UNITLESS` 183, `M3-PER-SEC` 76, `SEC` 59, `K` 59, `PA` 29, `J-PER-KiloGM` 9, and the
        // S231 fallback `s/K` 2, `1/K` 2 and `1/F` 2. `CoolingCoilValve` (issue #661) brings
        // `1/F` and the second `1/K`: the recorded ALC gains and the derived gain `k`.
        units: counted(&[
            ("1", 183),
            ("m3/s", 76),
            ("s", 59),
            ("K", 59),
            ("Pa", 29),
            ("J/kg", 9),
            ("s/K", 2),
            ("1/K", 2),
            ("1/F", 2),
        ]),
        // `DEG_C` 41, `PA` 14, `K` 6, `M3-PER-SEC` 3.
        display_units: counted(&[("degC", 41), ("Pa", 14), ("K", 6), ("m3/s", 3)]),
    };
    assert_eq!(census(&files), expected);
}

#[test]
fn every_unit_iri_of_cxf_core_maps() {
    let files = vec![vendored_root().join("cxf-core/CXF-Core.jsonld")];
    let expected = Census {
        documents: 1,
        carrying: 1,
        unit_keys: counted(&[("qudt:hasUnit", 73)]),
        display_unit_keys: counted(&[("S231:hasDisplayUnit", 13)]),
        // `SEC` 42, `RAD` 10, `UNITLESS` 9, `K` 5, the S231 fallback `1/s` 4, `HZ` 1,
        // `J-PER-KiloGM` 1, `PA` 1.
        units: counted(&[
            ("s", 42),
            ("rad", 10),
            ("1", 9),
            ("K", 5),
            ("1/s", 4),
            ("Hz", 1),
            ("J/kg", 1),
            ("Pa", 1),
        ]),
        // `DEG` 10, `HR` 3.
        display_units: counted(&[("deg", 10), ("h", 3)]),
    };
    assert_eq!(census(&files), expected);
}

#[test]
fn the_walk_refuses_an_unmapped_iri_a_non_iri_value_and_a_second_spelling() {
    // The census is only evidence if a bad document fails it: each control must panic.
    let context: Map<String, Json> = serde_json::from_str(
        r#"{"S231": "http://data.ashrae.org/S231#", "qudt": "http://qudt.org/schema/qudt#",
            "unit": "http://qudt.org/vocab/unit#"}"#,
    )
    .expect("context");
    let controls = [
        r#"[{"qudt:hasUnit": {"@id": "unit:M3-PER-MIN"}}]"#,
        r#"[{"qudt:hasUnit": {"@id": "unit:FOO"}}]"#,
        r#"[{"S231:hasDisplayUnit": "degC"}]"#,
        r#"[{"qudt:hasUnit": {"@id": "unit:K"}, "S231:hasUnit": {"@id": "unit:K"}}]"#,
        r#"[{"nested": {"deeper": [{"S231:unit": {"@id": "degC"}}]}}]"#,
    ];
    for control in controls {
        let graph: Json = serde_json::from_str(control).expect("control is JSON");
        let outcome = std::panic::catch_unwind(|| {
            visit(
                &graph,
                &context,
                Path::new("control"),
                &mut Census::default(),
            )
        });
        assert!(outcome.is_err(), "the walk accepted {control}");
    }
    let good: Json = serde_json::from_str(
        r#"[{"qudt:hasUnit": {"@id": "unit:K"}, "S231:hasDisplayUnit": {"@id": "unit:DEG_C"},
             "nested": [{"http://qudt.org/schema/qudt/hasUnit": {"@id": "S231:1/s"}}]}]"#,
    )
    .expect("JSON");
    let mut census = Census::default();
    assert_eq!(visit(&good, &context, Path::new("control"), &mut census), 3);
    assert_eq!(census.units, counted(&[("K", 1), ("1/s", 1)]));
    assert_eq!(census.display_units, counted(&[("degC", 1)]));
}
