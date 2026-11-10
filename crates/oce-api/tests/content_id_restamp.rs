//! The canonical content id pin: `tests/fixtures/content_id/restamp.tsv` holds the
//! `Engine::content_id` of every corpus document, one line per document. This test regenerates
//! the whole file and compares it byte for byte; `OCE_BLESS=1` rewrites it. Any change to an id
//! shows in review as a one-line diff.
//!
//! The file holds current ids only: no retired id, and nothing maps one id scheme to another
//! (issue #509).

#[path = "support/content_id_corpus.rs"]
mod corpus;

use std::fmt::Write as _;
use std::path::PathBuf;

use corpus::*;

fn table_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/content_id/restamp.tsv")
}

fn rendered() -> String {
    let mut out = String::from(
        "# Canonical content id pin: <content id> TAB <source path>, sorted by path.\n\
         # The id is Engine::content_id (oce:canon1, SHA-256) of the loaded document.\n",
    );
    writeln!(out, "# Engine version: {}", env!("CARGO_PKG_VERSION")).unwrap();
    out.push_str(
        "# Owner test: crates/oce-api/tests/content_id_restamp.rs (regenerate with OCE_BLESS=1).\n",
    );
    let mut rows: Vec<(String, String)> = corpus()
        .into_iter()
        .map(|name| {
            let path = format!("crates/oce-cxf/tests/fixtures/g36/{name}.jsonld");
            (path, id_of(&fixture_bytes(&name)).to_string())
        })
        .collect();
    rows.sort();
    for (path, id) in rows {
        writeln!(out, "{id}\t{path}").unwrap();
    }
    out
}

#[test]
fn the_pin_matches_every_documents_current_id() {
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
fn the_pin_has_the_documented_shape() {
    let text = std::fs::read_to_string(table_path()).unwrap();
    assert!(text.ends_with('\n') && !text.contains('\r'));
    let rows: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    assert!(
        text.lines().take(4).all(|l| l.starts_with('#')),
        "comments come first"
    );
    assert_eq!(rows.len(), 47);
    let mut ids = std::collections::BTreeSet::new();
    let mut paths = Vec::new();
    for row in &rows {
        let fields: Vec<&str> = row.split('\t').collect();
        assert_eq!(fields.len(), 2, "{row}");
        oce_api::ContentId::parse(fields[0]).unwrap();
        assert!(ids.insert(fields[0]), "every document has its own id");
        assert!(!fields[1].contains('\\') && fields[1].starts_with("crates/"));
        paths.push(fields[1]);
    }
    let mut sorted = paths.clone();
    sorted.sort_unstable();
    assert_eq!(paths, sorted, "rows sort by source path");
}
