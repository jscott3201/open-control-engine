//! The round trip over the repository's own CXF fixtures (GitHub issue #514): each `.jsonld` and
//! `.cxf.json` file git tracks under `crates/` goes through `oce import`, and every one the
//! importer accepts through `oce emit` and `oce id`. The outcome of each is one row of
//! `tests/fixtures/import_corpus.tsv`, a blessed table: a fixture added to any crate is
//! classified by one `OCE_BLESS_CLI=1` run, and the row it gets is what a reviewer reads. The
//! vendored tree under `third_party/` is left out, so a pin advance never touches this test.
//! Nothing is counted by hand but the G36 catalog's split, which the importer note states.
//!
//! A document is in exactly one class:
//!
//! - `refused`: the importer refuses it, exit 2, under one code of
//!   `oce_author::IMPORT_ERROR_CODES`, and writes nothing;
//! - `same-id`: it imports, `oce emit` accepts both the text and the wire output, and the
//!   emitted document's `oce id` is the original's;
//! - `same-id-restamped`: the same once the original's root is re-stamped. The root's name
//!   carries a prefix after its namespace (`http://example.org#g36.source.<name>`); the importer
//!   keeps the trailing identifier as the sequence name, and names are inside the id. The row
//!   names the prefix;
//! - `warned`: it imports, the original loads with warnings and so has an id, and `oce emit`
//!   answers no, because the validator counts an engine warning as an error finding. The id of
//!   what the emitter alone writes (`oce_author::emit`, no engine stage) is still the
//!   original's, under the explanation the row names;
//! - `author-refused`: it imports and the original loads, but stage 1 of the validator refuses
//!   the sequence, so nothing is emitted and there is no second id. No tracked fixture is one
//!   today. (An Integer literal above 2^53 on a Real parameter was, until the engine began to
//!   refuse it too, issue #683: it is `no-id` now);
//! - `no-id`: it imports, but the original does not load in an engine, so there is no id to
//!   keep; `oce emit` answers no with the findings the row lists.
//!
//! The id of a round trip differs from the original's only in the re-stamped class, and for
//! each such document the test proves the named cause is the only difference: the original,
//! with its root re-stamped and nothing else changed, has the round trip's id. A difference
//! that does not explain fails the test; that is a finding, not a row.
//!
//! How a Real parameter's number is spelled is not a cause any more (issue #683). The importer
//! keeps an Integer literal as written and the emitter writes a Real parameter's value as a
//! Real, and the engine holds both as the same Real, so both have one id. The class that
//! recorded the difference, `same-id-real-literals`, is gone with the helper that rewrote the
//! literals.
//!
//! For every imported document the test also holds the text and the wire output to the
//! sequence `oce_author::import_cxf` returns, each canonical in its form (`oce fmt --check`),
//! `oce emit` to one answer and the same bytes for both, and the report's `dropped` and
//! `normalised` to the importer's lists.
//!
//! The corpus is `git ls-files`, as in `oce-cxf`'s `parameter_grounding_corpus`, so an untracked
//! local file cannot join it: `git add` a new fixture before blessing.

mod support;

use std::path::Path;
use std::process::Command;

use oce_author::{EmitOptions, ImportOptions};
use serde_json::{Value, json};
use support::cxf_edit::{edited, node};
use support::{Run, author_fixture, compare_or_bless, crate_dir, oce, scratch};

/// The repository root as seen from the crate directory every run starts in.
const REPO: &str = "../../..";

/// Every git-tracked `.jsonld` and `.cxf.json` under `crates/`, as sorted repository-relative
/// paths. `third_party/` is not walked: the vendored tree moves with a pin advance, which must
/// not move a leverage test.
fn corpus() -> Vec<String> {
    let output = Command::new("git")
        .args(["ls-files", "-z", "--", "crates"])
        .current_dir(crate_dir().join(REPO))
        .output()
        .expect("enumerate the tracked files");
    assert!(output.status.success(), "git ls-files failed");
    let mut paths: Vec<String> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8(path.to_vec()).expect("a tracked path is UTF-8"))
        .filter(|path| path.ends_with(".jsonld") || path.ends_with(".cxf.json"))
        .collect();
    paths.sort();
    paths
}

/// One document's outcome.
struct Row {
    class: &'static str,
    detail: String,
    /// The lengths of the report's `dropped` and `normalised`, for an imported document.
    lists: Option<(usize, usize)>,
    path: String,
}

impl Row {
    fn line(&self) -> String {
        let (dropped, normalised) = self.lists.map_or(("-".to_owned(), "-".to_owned()), |l| {
            (l.0.to_string(), l.1.to_string())
        });
        format!(
            "{}\t{}\t{dropped}\t{normalised}\t{}\n",
            self.class, self.detail, self.path
        )
    }
}

/// The id of a clean `oce id` run.
fn printed_id(run: &Run) -> String {
    assert_eq!(run.code, 0, "{}", run.transcript());
    run.stdout_text()
        .strip_suffix('\n')
        .expect("one line")
        .to_owned()
}

/// `oce id` of CXF bytes on stdin.
fn id_of(cxf: &[u8]) -> String {
    printed_id(&oce(&["id", "-", "--from", "cxf"], cxf))
}

/// The error-finding codes of an `oce emit` that answered no, sorted and joined.
fn finding_codes(report: &Value) -> String {
    let mut codes: Vec<&str> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| finding["code"].as_str().expect("code"))
        .collect();
    codes.sort_unstable();
    codes.dedup();
    codes.join(",")
}

/// The root `@id` of a flat document: the node that contains blocks.
fn root_of(document: &Value) -> String {
    document["@graph"]
        .as_array()
        .and_then(|graph| {
            graph
                .iter()
                .find(|node| node.get("S231:containsBlock").is_some())
        })
        .and_then(|node| node["@id"].as_str())
        .expect("an imported document has a root")
        .to_owned()
}

/// What the root's name carries between its namespace (through the last `#`, `/` or `:`) and
/// the sequence name: `g36.source.` in the G36 corpus, empty for an emitted document.
fn name_prefix(root: &str, sequence: &str) -> String {
    let before = root
        .strip_suffix(sequence)
        .expect("the root ends in the sequence name");
    let namespace_end = before.rfind(['#', '/', ':']).map_or(0, |at| at + 1);
    before[namespace_end..].to_owned()
}

/// `original` with its root re-stamped: every `@id` value under `root` moved under
/// `urn:oce:import:<sequence>`. Only `@id` values are touched, so no other string of the
/// document can be rewritten by accident.
fn restamped(original: &[u8], root: &str, sequence: &str) -> Vec<u8> {
    fn walk(value: &mut Value, root: &str, to: &str) {
        match value {
            Value::Object(map) => {
                for (key, child) in map.iter_mut() {
                    match child {
                        Value::String(id) if key == "@id" => {
                            if let Some(rest) = id.strip_prefix(root)
                                && (rest.is_empty() || rest.starts_with('.'))
                            {
                                *id = format!("{to}{rest}");
                            }
                        }
                        other => walk(other, root, to),
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|item| walk(item, root, to)),
            _ => {}
        }
    }
    let mut document: Value = serde_json::from_slice(original).expect("JSON");
    walk(&mut document, root, &format!("urn:oce:import:{sequence}"));
    serde_json::to_vec(&document).expect("JSON")
}

/// Why `round_trip` is the id of the document: the class and its detail. Panics, naming the
/// document, when the root's name does not explain a difference: that is a finding, not a
/// class.
fn explained(
    label: &str,
    original: &[u8],
    sequence: &str,
    round_trip: &str,
) -> (&'static str, String) {
    let document: Value = serde_json::from_slice(original).expect("JSON");
    let root = root_of(&document);
    let prefix = name_prefix(&root, sequence);
    if id_of(original) == round_trip {
        // The ids agree exactly where the root's name is the sequence name alone.
        assert!(prefix.is_empty(), "{label}: equal ids under a name prefix");
        return ("same-id", "-".to_owned());
    }
    assert!(
        !prefix.is_empty() && id_of(&restamped(original, &root, sequence)) == round_trip,
        "{label}: the round trip's id is not the original's, and the root's name prefix \
         (`{prefix}`) does not explain it"
    );
    ("same-id-restamped", prefix)
}

/// Run one document through the verbs and classify it. `label` names it in the row and in a
/// failure, `file` is where it is, and `dir` is the caller's scratch directory.
fn classify(label: &str, file: &Path, dir: &Path) -> Row {
    let original = std::fs::read(file).expect("the document");
    let file_arg = file.to_string_lossy().into_owned();
    let out = dir.join("out.oce");
    let _ = std::fs::remove_file(&out);
    let out_arg = out.to_string_lossy().into_owned();
    let run = oce(&["import", &file_arg, "-o", &out_arg], b"");
    assert!(run.stderr.is_empty(), "{label}: {}", run.transcript());
    let report = run.report();
    let library = oce_author::import_cxf(&original, &ImportOptions::new());
    if run.code == 2 {
        let errors = report["errors"].as_array().expect("errors");
        assert_eq!(errors.len(), 1, "{label}: {}", run.transcript());
        let code = errors[0]["code"].as_str().expect("code");
        assert!(
            oce_author::IMPORT_ERROR_CODES.contains(&code),
            "{label}: `{code}` is not an importer code"
        );
        assert_eq!(library.expect_err("the library refuses too").code(), code);
        assert!(!out.exists(), "{label}: a refused import wrote a file");
        return Row {
            class: "refused",
            detail: code.to_owned(),
            lists: None,
            path: label.to_owned(),
        };
    }
    assert_eq!(run.code, 0, "{label}: {}", run.transcript());
    let imported = library.expect("the library imports what the verb imports");
    let text = std::fs::read(&out).expect("the text output");
    let wire = oce(&["import", &file_arg, "--wire"], b"");
    assert_eq!(wire.code, 0, "{label}: {}", wire.transcript());
    // The oracle: both outputs are the sequence the library imports, and the lists are its.
    let text_str = std::str::from_utf8(&text).expect("UTF-8");
    assert_eq!(
        oce_author::parse_text(text_str).expect("the text parses"),
        imported.sequence,
        "{label}"
    );
    assert_eq!(
        oce_author::from_wire_bytes(&wire.stdout).expect("the wire form reads"),
        imported.sequence,
        "{label}"
    );
    let subjects = |key: &str| -> Vec<String> {
        report[key]
            .as_array()
            .expect("a list")
            .iter()
            .map(|entry| entry["subject"].as_str().expect("subject").to_owned())
            .collect()
    };
    let (dropped, normalised) = (&imported.report.dropped, &imported.report.normalised);
    assert_eq!(
        subjects("dropped"),
        dropped
            .iter()
            .map(|d| d.subject.clone())
            .collect::<Vec<_>>(),
        "{label}"
    );
    assert_eq!(
        subjects("normalised"),
        normalised
            .iter()
            .map(|n| n.subject.clone())
            .collect::<Vec<_>>(),
        "{label}"
    );
    let lists = Some((dropped.len(), normalised.len()));
    for (form, bytes) in [("text", &text), ("wire", &wire.stdout)] {
        let check = oce(&["fmt", "-", "--from", form, "--check"], bytes);
        assert_eq!(check.code, 0, "{label} {form}: {}", check.transcript());
    }
    let emitted = oce(&["emit", "-", "--from", "text"], &text);
    let from_wire = oce(&["emit", "-", "--from", "wire"], &wire.stdout);
    assert_eq!(emitted.code, from_wire.code, "{label}: the forms disagree");
    let sequence = imported.sequence.name.as_str();
    let row = |class, detail| Row {
        class,
        detail,
        lists,
        path: label.to_owned(),
    };
    match emitted.code {
        0 => {
            assert_eq!(
                emitted.stdout, from_wire.stdout,
                "{label}: the forms disagree"
            );
            let round_trip = id_of(&emitted.stdout);
            let (class, detail) = explained(label, &original, sequence, &round_trip);
            row(class, detail)
        }
        1 => {
            // The same findings whichever form carried the sequence; only the text has spans.
            let findings = emitted.report();
            assert_eq!(
                finding_codes(&findings),
                finding_codes(&from_wire.report()),
                "{label}: the forms disagree"
            );
            let codes = finding_codes(&findings);
            if oce(&["id", &file_arg, "--from", "cxf"], b"").code != 0 {
                return row("no-id", codes);
            }
            // The original loads. Where stage 1 accepts the sequence, only the engine's
            // warnings stand between it and an emitted document, and what the emitter alone
            // writes still has the original's id; where stage 1 refuses it, nothing is emitted.
            match oce_author::emit(&imported.sequence, &EmitOptions::new()) {
                Ok(alone) => {
                    assert!(
                        findings["findings"]
                            .as_array()
                            .expect("findings")
                            .iter()
                            .all(|finding| finding["origin"] == "engine"),
                        "{label}: an author-side finding on a sequence stage 1 accepts"
                    );
                    let (class, _) = explained(label, &original, sequence, &id_of(&alone));
                    row("warned", format!("{codes}; {class}"))
                }
                Err(_) => row("author-refused", codes),
            }
        }
        other => panic!("{label}: oce emit exited {other}: {}", emitted.transcript()),
    }
}

/// Classify the whole corpus, a few documents at a time.
fn rows() -> Vec<Row> {
    const WORKERS: usize = 6;
    let paths = corpus();
    let mut rows: Vec<Row> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..WORKERS)
            .map(|worker| {
                let paths = &paths;
                scope.spawn(move || {
                    let dir = scratch(&format!("import_corpus_{worker}"));
                    paths
                        .iter()
                        .skip(worker)
                        .step_by(WORKERS)
                        .map(|path| {
                            classify(path, &crate_dir().join(format!("{REPO}/{path}")), &dir)
                        })
                        .collect::<Vec<Row>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("a worker panicked; see its message"))
            .collect()
    });
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    rows
}

#[test]
fn every_tracked_cxf_fixture_round_trips_or_is_refused_with_a_stable_code() {
    let rows = rows();
    let mut table = String::from(
        "# Every tracked CXF fixture under crates/ through `oce import`, then `oce emit` and\n\
         # `oce id` (tests/import_corpus.rs holds the classes). A new fixture gets its row from\n\
         # one run with OCE_BLESS_CLI=1; read the row before committing it.\n\
         # class<TAB>detail<TAB>dropped<TAB>normalised<TAB>path\n",
    );
    for row in &rows {
        table.push_str(&row.line());
    }
    compare_or_bless("import_corpus.tsv", &table);
    // A walk that silently shrinks to nothing would bless an empty table.
    assert!(rows.len() >= 200, "only {} documents", rows.len());

    // The one split stated by hand: the 46 fixtures of the G36 catalog (importer note, "Where
    // the corpus stands"). The 31 inside the authoring subset round-trip to the id of the
    // committed fixture re-stamped, and the other 15 are refused under the fixed precedence.
    let catalog: Vec<&Row> = rows
        .iter()
        .filter(|row| row.path.starts_with("crates/oce-cxf/tests/fixtures/g36/"))
        // The interface miniature beside them is not a catalog fixture.
        .filter(|row| !row.path.ends_with("/member_list_interface.jsonld"))
        .collect();
    let (imported, refused): (Vec<&Row>, Vec<&Row>) =
        catalog.iter().partition(|row| row.class != "refused");
    assert_eq!((imported.len(), refused.len()), (31, 15));
    for row in imported {
        assert_eq!(
            (row.class, row.detail.as_str()),
            ("same-id-restamped", "g36.source."),
            "{}",
            row.path
        );
    }
}

#[test]
fn an_integer_literal_on_a_real_parameter_keeps_its_id_and_one_past_two_to_the_53rd_has_none() {
    // The two shapes issue #683 named that no tracked fixture writes, built here and run through
    // the same classifier.
    let dir = scratch("import_corpus_real_literals");
    let element =
        std::fs::read_to_string(crate_dir().join(author_fixture("emit/every_element.cxf.json")))
            .expect("fixture");
    // An Integer element of a Real array: `sum3.k = [1.0, -2, 1e-7]`, the `-2` a bare integer.
    // The importer keeps it, the emitter writes `-2.0`, and the id does not move.
    let mut document: Value = serde_json::from_str(&element).expect("JSON");
    let gains = document["@graph"]
        .as_array_mut()
        .expect("a graph")
        .iter_mut()
        .find(|node| node["@id"] == "urn:oce:author:every_element.sum3.k")
        .expect("the array parameter");
    assert_eq!(gains["S231:value"][1]["@value"], "-2.0");
    gains["S231:value"][1] = json!(-2);
    let array = dir.join("array.cxf.json");
    let respelled = serde_json::to_vec(&document).expect("JSON");
    std::fs::write(&array, &respelled).expect("write");
    let row = classify("an Integer element of a Real array", &array, &dir);
    assert_eq!((row.class, row.detail.as_str()), ("same-id", "-"));
    // And that id is the unedited fixture's: the spelling is not in the id.
    assert_eq!(id_of(&respelled), id_of(element.as_bytes()));

    // An Integer above 2^53 on a Real parameter: the engine refuses it, as stage 1 does and
    // under the same code, so the document has no id.
    let large = dir.join("large.cxf.json");
    std::fs::write(
        &large,
        edited(|document| {
            node(document, ".hys.uLow")["S231:value"] = json!(-9_007_199_254_740_993_i64);
        }),
    )
    .expect("write");
    let row = classify("an Integer above 2^53 on a Real parameter", &large, &dir);
    assert_eq!(
        (row.class, row.detail.as_str()),
        ("no-id", "parameter-out-of-range")
    );
    let id = oce(&["id", &large.to_string_lossy(), "--from", "cxf"], b"");
    assert_eq!(id.code, 2, "{}", id.transcript());
}
