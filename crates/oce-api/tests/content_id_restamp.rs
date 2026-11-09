//! The re-stamp mapping table (`docs/design/content-id.md`, "Mapping table for the library
//! re-stamp"): `tests/fixtures/content_id/restamp.tsv` maps every corpus document's retired
//! `cxf:fnv1a128:` export tag (`-` where its export was partial) to its canonical content id. This
//! test regenerates the whole file and compares it byte for byte; `OCE_BLESS=1` rewrites it.
//!
//! The old column is the retired tag of the emitted bytes, read from the export content oracle's
//! pinned digests (`g36_export_content.txt`, unchanged by GitHub #470); the new column is
//! `Engine::content_id`. Any change to a new id shows in review as a one-line diff.

#[path = "support/content_id_corpus.rs"]
mod corpus;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use corpus::*;

fn table_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/content_id/restamp.tsv")
}

/// The retired export tag of every complete export, by fixture stem, from the oracle's file.
fn old_tags() -> BTreeMap<String, Option<String>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../oce-cxf/tests/fixtures/expectations/g36_export_content.txt");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let fields: Vec<&str> = line.split(' ').collect();
            assert_eq!(fields.len(), 3, "{line}");
            let old = (fields[2] == "ok").then(|| format!("cxf:fnv1a128:{}", fields[1]));
            (fields[0].to_owned(), old)
        })
        .collect()
}

fn rendered() -> String {
    let mut out = String::from(
        "# Canonical content id re-stamp table: <old id> TAB <new id> TAB <source path>, sorted by \
         path.\n\
         # old: the retired cxf:fnv1a128 export tag, - where the export was partial; new: \
         Engine::content_id (oce:canon1, SHA-256).\n",
    );
    writeln!(out, "# Engine version: {}", env!("CARGO_PKG_VERSION")).unwrap();
    out.push_str(
        "# Owner test: crates/oce-api/tests/content_id_restamp.rs (regenerate with OCE_BLESS=1).\n",
    );
    let old = old_tags();
    let mut rows: Vec<(String, String, String)> = corpus()
        .into_iter()
        .map(|name| {
            let path = format!("crates/oce-cxf/tests/fixtures/g36/{name}.jsonld");
            let new = id_of(&fixture_bytes(&name)).to_string();
            let old = old[&name].clone().unwrap_or_else(|| "-".to_owned());
            (path, old, new)
        })
        .collect();
    rows.sort();
    for (path, old, new) in rows {
        writeln!(out, "{old}\t{new}\t{path}").unwrap();
    }
    out
}

#[test]
fn the_restamp_table_matches_every_documents_current_id() {
    let rendered = rendered();
    if oce_bless::enabled("OCE_BLESS") {
        std::fs::write(table_path(), &rendered).unwrap();
        return;
    }
    let checked_in = std::fs::read_to_string(table_path()).expect("read restamp.tsv");
    assert_eq!(
        checked_in, rendered,
        "a canonical content id moved; bless deliberately with OCE_BLESS=1"
    );
}

#[test]
fn the_table_has_the_documented_shape() {
    let text = std::fs::read_to_string(table_path()).unwrap();
    assert!(text.ends_with('\n') && !text.contains('\r'));
    let rows: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    assert!(
        text.lines().take(4).all(|l| l.starts_with('#')),
        "comments come first"
    );
    assert_eq!(rows.len(), 47);
    let (mut partial, mut news) = (0, std::collections::BTreeSet::new());
    let mut paths = Vec::new();
    for row in &rows {
        let fields: Vec<&str> = row.split('\t').collect();
        assert_eq!(fields.len(), 3, "{row}");
        if fields[0] == "-" {
            partial += 1;
        } else {
            let hex = fields[0].strip_prefix("cxf:fnv1a128:").unwrap();
            assert!(hex.len() == 32 && hex.bytes().all(|b| b.is_ascii_hexdigit()));
        }
        oce_api::ContentId::parse(fields[1]).unwrap();
        assert!(news.insert(fields[1]), "every document has its own id");
        assert!(!fields[2].contains('\\') && fields[2].starts_with("crates/"));
        paths.push(fields[2]);
    }
    let mut sorted = paths.clone();
    sorted.sort_unstable();
    assert_eq!(paths, sorted, "rows sort by source path");
    assert_eq!(partial, 13, "the 13 partial exports have no old id");
}
