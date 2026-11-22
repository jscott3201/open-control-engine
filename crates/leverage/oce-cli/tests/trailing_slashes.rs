//! A path that ends in `/` or `/.` names a directory, and the system refuses it when the entry is
//! a file. Since issue #771 a file is opened by name from its directory's handle; the name keeps
//! one `/` (`directory::split`), so the refusal is the one the path always earned:
//! `read-failed` "not a directory" for a document or `fmt --write`'s file, `write-failed` for an
//! output, and nothing is read through or written. Without that `/` the name would be the file's
//! own, and `oce fmt --write t.oce/` would rewrite `t.oce` (the review of GitHub #777).

#![cfg(unix)]

mod support;

use std::fs;
use std::path::Path;

use support::commission::run_layout;
use support::{Run, author_fixture, crate_dir, oce_in, scratch};

/// The first error's code and detail.
fn refusal(run: &Run) -> (String, String) {
    assert_eq!(run.code, 2, "{}", run.transcript());
    let error = &run.report()["errors"][0];
    (
        error["code"].as_str().expect("code").to_owned(),
        error["detail"].as_str().expect("detail").to_owned(),
    )
}

fn not_a_directory() -> (String, String) {
    (
        "read-failed".to_owned(),
        "the read failed: not a directory".to_owned(),
    )
}

/// Every name in `dir`, sorted, with its bytes.
fn listing(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut names: Vec<(String, Vec<u8>)> = fs::read_dir(dir)
        .expect("list")
        .map(|entry| {
            let path = entry.expect("entry").path();
            let name = path
                .file_name()
                .expect("a name")
                .to_string_lossy()
                .into_owned();
            (name, fs::read(&path).unwrap_or_default())
        })
        .collect();
    names.sort();
    names
}

#[test]
fn a_spec_or_a_vector_set_named_with_a_trailing_slash_is_not_a_directory() {
    let dir = run_layout("slash_documents", "pass");
    fs::write(dir.join("vectors.json"), b"{}").expect("a set");
    let before = listing(&dir);
    for args in [
        ["verify", "--commission", "spec.json/"].as_slice(),
        ["verify", "--commission", "spec.json/."].as_slice(),
        ["verify", "vectors.json/", "--subject", "plant.cxf.json"].as_slice(),
    ] {
        let run = oce_in(&dir, args, b"");
        assert_eq!(refusal(&run), not_a_directory(), "{args:?}");
    }
    assert_eq!(listing(&dir), before, "nothing changed");
}

#[test]
fn fmt_write_of_a_file_named_with_a_trailing_slash_rewrites_nothing() {
    let dir = scratch("slash_fmt");
    let text =
        fs::read(crate_dir().join(author_fixture("text/fmt/comments.in.oce"))).expect("text");
    fs::write(dir.join("t.oce"), &text).expect("t.oce");
    for name in ["t.oce/", "t.oce/."] {
        let run = oce_in(&dir, &["fmt", "--write", name], b"");
        assert_eq!(refusal(&run), not_a_directory(), "{name}");
    }
    assert_eq!(
        listing(&dir),
        [("t.oce".to_owned(), text)],
        "t.oce untouched"
    );
}

#[test]
fn an_output_named_with_a_trailing_slash_is_never_written() {
    let dir = scratch("slash_output");
    let source = fs::read(crate_dir().join(author_fixture("text/econ_enable.oce"))).expect("text");
    fs::write(dir.join("e.oce"), &source).expect("e.oce");
    // Absent: nothing to be a directory, nothing created. The rename onto `out.cxf.json/` is
    // refused as the system words it: "not found" on macOS, "not a directory" on Linux.
    let run = oce_in(&dir, &["emit", "e.oce", "-o", "out.cxf.json/"], b"");
    let (code, detail) = refusal(&run);
    assert_eq!(code, "write-failed");
    assert!(
        [
            "the output could not be written: entity not found",
            "the output could not be written: not a directory"
        ]
        .contains(&detail.as_str()),
        "{detail}"
    );
    assert_eq!(listing(&dir), [("e.oce".to_owned(), source.clone())]);
    // A regular file: not a directory, and left as it was.
    fs::write(dir.join("out.cxf.json"), b"KEEP").expect("out");
    let run = oce_in(&dir, &["emit", "e.oce", "-o", "out.cxf.json/"], b"");
    assert_eq!(
        refusal(&run),
        (
            "write-failed".to_owned(),
            "the output could not be written: not a directory".to_owned()
        )
    );
    assert_eq!(
        listing(&dir),
        [
            ("e.oce".to_owned(), source),
            ("out.cxf.json".to_owned(), b"KEEP".to_vec())
        ]
    );
}
