//! Every enumeration class, enumeration member and Integer constant the CXF corpus names is a row
//! of the enumeration table (issue #468): the conformance fixtures under `oce-cxf` and the vendored
//! modelica-json translations. A reference the table lacks would load as an unknown type or an
//! ungroundable constant; this test names it instead.

use std::path::{Path, PathBuf};

use oce_model::{
    EnumerationKind, MemberValue, OBC_PACKAGE, enumeration_class, g36_integer_constant,
};

/// The `Types` packages whose classes the table covers.
const TYPES_PACKAGES: [&str; 2] = ["CDL.Types.", "ASHRAE.G36.Types."];

fn corpus_roots() -> [PathBuf; 2] {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    [
        root.join("crates/oce-cxf/tests/fixtures"),
        root.join("third_party/modelica-buildings-cdl/cxf"),
    ]
}

fn json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            json_files(&path, out);
        } else if path
            .extension()
            .is_some_and(|ext| ext == "json" || ext == "jsonld")
        {
            out.push(path);
        }
    }
}

/// Every `Buildings.Controls.OBC.<Types package>.<Name>[.<member>]` token in `text`, as the part
/// after `Buildings.Controls.OBC.`.
fn type_references(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(OBC_PACKAGE) {
        let tail = &rest[at + OBC_PACKAGE.len()..];
        let len = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
            .unwrap_or(tail.len());
        let token = tail[..len].trim_end_matches('.');
        if TYPES_PACKAGES.iter().any(|p| token.starts_with(p)) {
            out.push(token);
        }
        rest = &tail[len..];
    }
    out
}

#[test]
fn every_types_reference_in_the_corpus_is_a_table_row_or_member() {
    let mut files = Vec::new();
    for root in corpus_roots() {
        json_files(&root, &mut files);
    }
    assert!(files.len() > 100, "corpus found: {}", files.len());

    let mut constants_seen = 0usize;
    let mut members_seen = 0usize;
    for file in &files {
        let text = std::fs::read_to_string(file).expect("utf-8 corpus file");
        for token in type_references(&text) {
            let package = TYPES_PACKAGES
                .iter()
                .find(|p| token.starts_with(*p))
                .expect("scanned for a Types package");
            let (class_path, member) = match token[package.len()..].split_once('.') {
                Some((name, member)) => (&token[..package.len() + name.len()], Some(member)),
                None => (token, None),
            };
            let row = enumeration_class(class_path)
                .unwrap_or_else(|| panic!("{}: `{token}` names no table row", file.display()));
            let Some(member) = member else { continue };
            let value = row.member_value(member).unwrap_or_else(|| {
                panic!("{}: `{token}` names no member of the row", file.display())
            });
            members_seen += 1;
            if row.kind == EnumerationKind::ConstantPackage {
                constants_seen += 1;
                let MemberValue::Integer(expected) = value else {
                    panic!("a constant package member is an Integer");
                };
                assert_eq!(
                    g36_integer_constant(&format!("{OBC_PACKAGE}{token}")),
                    Some(expected),
                    "{}: `{token}` grounds to the table's value",
                    file.display()
                );
            }
        }
    }
    assert!(
        members_seen > 0 && constants_seen > 0,
        "{members_seen} {constants_seen}"
    );
}

#[test]
fn reference_scan_reads_class_and_member_tokens() {
    let text = r#"{"v":"Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes.occupied",
        "t":"http://example.org#Buildings.Controls.OBC.CDL.Types.SimpleController",
        "c":"Buildings.Controls.OBC.CDL.Reals.PID"}"#;
    assert_eq!(
        type_references(text),
        [
            "ASHRAE.G36.Types.OperationModes.occupied",
            "CDL.Types.SimpleController"
        ]
    );
}
