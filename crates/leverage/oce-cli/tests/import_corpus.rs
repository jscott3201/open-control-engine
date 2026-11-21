//! The round trip over every tracked CXF document (GitHub issue #514): each `.jsonld` and
//! `.cxf.json` file git tracks under `crates/` and `third_party/` goes through `oce import`, and
//! every one the importer accepts through `oce emit` and `oce id`. The outcome of each is one
//! row of `tests/fixtures/import_corpus.tsv`, and the number of documents in each class is
//! pinned below, so a fixture added anywhere in the repository has to be classified here.
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
//!   names the prefix. This class and the next are the only two where the ids differ;
//! - `same-id-real-literals`: the same once the original's Integer literals on Real parameters
//!   are written as Reals. The importer keeps such a literal as written, the emitter writes a
//!   Real parameter's value as a Real, and the engine's id tells the two apart;
//! - `warned`: it imports, the original loads with warnings and so has an id, and `oce emit`
//!   answers no, because the validator counts an engine warning as an error finding. The id of
//!   what the emitter alone writes (`oce_author::emit`, no engine stage) is still the original's;
//! - `no-id`: it imports, but the original does not load in an engine, so there is no id to
//!   keep; `oce emit` answers no with the findings the row lists.
//!
//! For every imported document the test also holds the text and the wire output to the
//! sequence `oce_author::import_cxf` returns, each canonical in its form (`oce fmt --check`),
//! `oce emit` to one answer and the same bytes for both, and the report's `dropped` and
//! `normalised` to the importer's lists.
//!
//! The corpus is `git ls-files`, as in `oce-cxf`'s `parameter_grounding_corpus`, so an untracked
//! local file can neither join it nor move a pinned count.

mod support;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use oce_api::ValueType;
use oce_author::{EmitOptions, ImportOptions};
use serde_json::{Value, json};
use support::cxf_edit::{XSD_DOUBLE, XSD_INTEGER};
use support::{Run, compare_or_bless, crate_dir, oce, scratch};

/// The repository root as seen from the crate directory every run starts in.
const REPO: &str = "../../..";

/// Every git-tracked `.jsonld` and `.cxf.json` under `crates/` and `third_party/`, as sorted
/// repository-relative paths.
fn corpus() -> Vec<String> {
    let output = Command::new("git")
        .args(["ls-files", "-z", "--", "crates", "third_party"])
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

/// The nodes a relation names: one reference or a list of them.
fn references(relation: &Value) -> Vec<String> {
    let one = |reference: &Value| reference["@id"].as_str().map(str::to_owned);
    match relation {
        Value::Array(items) => items.iter().filter_map(one).collect(),
        other => one(other).into_iter().collect(),
    }
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

/// `original` with every Integer literal on a Real parameter written as a Real: a sequence
/// parameter declared `S231:Real`, and a block parameter the catalog types Real. Returns the
/// document and how many literals moved.
fn with_real_literals(original: &[u8]) -> (Vec<u8>, usize) {
    let mut document: Value = serde_json::from_slice(original).expect("JSON");
    let root = root_of(&document);
    let graph = document["@graph"].as_array().expect("a graph").clone();
    let node = |id: &str| graph.iter().find(|node| node["@id"] == id);
    let mut real_parameters: Vec<String> = Vec::new();
    let root_node = node(&root).expect("the root");
    for parameter in references(&root_node["S231:hasParameter"]) {
        if node(&parameter).is_some_and(|n| n["S231:isOfDataType"]["@id"] == "S231:Real") {
            real_parameters.push(parameter);
        }
    }
    for block in references(&root_node["S231:containsBlock"]) {
        let Some(block_node) = node(&block) else {
            continue;
        };
        let class = block_node["@type"]
            .as_str()
            .and_then(|iri| iri.split_once("Buildings.Controls.OBC."))
            .map(|(_, path)| path);
        let Some(meta) = oce_api::class_metadata()
            .iter()
            .find(|meta| Some(meta.class_path) == class)
        else {
            continue;
        };
        for parameter in references(&block_node["S231:hasParameter"]) {
            let name = parameter.strip_prefix(&format!("{block}.")).unwrap_or("");
            if meta
                .parameters
                .iter()
                .any(|p| p.name == name && p.value_type == ValueType::Real)
            {
                real_parameters.push(parameter);
            }
        }
    }
    let mut moved = 0;
    for node in document["@graph"].as_array_mut().expect("a graph") {
        let id = node["@id"].as_str().unwrap_or("").to_owned();
        if !real_parameters.contains(&id) {
            continue;
        }
        let value = &mut node["S231:value"];
        let integer = match &*value {
            Value::Number(number) if !number.is_f64() => number.as_i64(),
            Value::Object(literal) if literal.get("@type") == Some(&json!(XSD_INTEGER)) => literal
                .get("@value")
                .and_then(Value::as_str)
                .and_then(|text| text.parse().ok()),
            _ => None,
        };
        if let Some(integer) = integer {
            *value = json!({ "@value": format!("{integer}.0"), "@type": XSD_DOUBLE });
            moved += 1;
        }
    }
    (serde_json::to_vec(&document).expect("JSON"), moved)
}

/// Why `round_trip` is the id of the document: the class and its detail. Panics, naming the
/// document, when neither the root's name nor an Integer literal on a Real parameter explains
/// a difference.
fn explained(
    path: &str,
    original: &[u8],
    sequence: &str,
    round_trip: &str,
) -> (&'static str, String) {
    let document: Value = serde_json::from_slice(original).expect("JSON");
    let root = root_of(&document);
    let prefix = name_prefix(&root, sequence);
    let original_id = id_of(original);
    if original_id == round_trip {
        // The ids agree exactly where the root's name is the sequence name alone.
        assert!(prefix.is_empty(), "{path}: equal ids under a name prefix");
        return ("same-id", "-".to_owned());
    }
    let mut candidate = original.to_vec();
    if !prefix.is_empty() {
        let text = String::from_utf8(candidate).expect("UTF-8");
        candidate = text
            .replace(&root, &format!("urn:oce:import:{sequence}"))
            .into_bytes();
        if id_of(&candidate) == round_trip {
            return ("same-id-restamped", prefix);
        }
    }
    let (candidate, moved) = with_real_literals(&candidate);
    assert!(
        moved > 0 && id_of(&candidate) == round_trip,
        "{path}: the round trip's id is not the original's, and neither the root's name prefix \
         (`{prefix}`) nor an Integer literal on a Real parameter ({moved} found) explains it"
    );
    assert!(
        prefix.is_empty(),
        "{path}: both explanations; add its class"
    );
    ("same-id-real-literals", moved.to_string())
}

/// Run one document through the verbs and classify it. `dir` is this worker's scratch
/// directory.
fn classify(path: &str, dir: &Path) -> Row {
    let relative = format!("{REPO}/{path}");
    let original = std::fs::read(crate_dir().join(&relative)).expect("a tracked file");
    let out = dir.join("out.oce");
    let _ = std::fs::remove_file(&out);
    let out_arg = out.to_string_lossy().into_owned();
    let run = oce(&["import", &relative, "-o", &out_arg], b"");
    assert!(run.stderr.is_empty(), "{path}: {}", run.transcript());
    let report = run.report();
    let library = oce_author::import_cxf(&original, &ImportOptions::new());
    if run.code == 2 {
        let errors = report["errors"].as_array().expect("errors");
        assert_eq!(errors.len(), 1, "{path}: {}", run.transcript());
        let code = errors[0]["code"].as_str().expect("code");
        assert!(
            oce_author::IMPORT_ERROR_CODES.contains(&code),
            "{path}: `{code}` is not an importer code"
        );
        assert_eq!(library.expect_err("the library refuses too").code(), code);
        assert!(!out.exists(), "{path}: a refused import wrote a file");
        return Row {
            class: "refused",
            detail: code.to_owned(),
            lists: None,
            path: path.to_owned(),
        };
    }
    assert_eq!(run.code, 0, "{path}: {}", run.transcript());
    let imported = library.expect("the library imports what the verb imports");
    let text = std::fs::read(&out).expect("the text output");
    let wire = oce(&["import", &relative, "--wire"], b"");
    assert_eq!(wire.code, 0, "{path}: {}", wire.transcript());
    // The oracle: both outputs are the sequence the library imports, and the lists are its.
    let text_str = std::str::from_utf8(&text).expect("UTF-8");
    assert_eq!(
        oce_author::parse_text(text_str).expect("the text parses"),
        imported.sequence,
        "{path}"
    );
    assert_eq!(
        oce_author::from_wire_bytes(&wire.stdout).expect("the wire form reads"),
        imported.sequence,
        "{path}"
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
        "{path}"
    );
    assert_eq!(
        subjects("normalised"),
        normalised
            .iter()
            .map(|n| n.subject.clone())
            .collect::<Vec<_>>(),
        "{path}"
    );
    let lists = Some((dropped.len(), normalised.len()));
    for (form, bytes) in [("text", &text), ("wire", &wire.stdout)] {
        let check = oce(&["fmt", "-", "--from", form, "--check"], bytes);
        assert_eq!(check.code, 0, "{path} {form}: {}", check.transcript());
    }
    let emitted = oce(&["emit", "-", "--from", "text"], &text);
    let from_wire = oce(&["emit", "-", "--from", "wire"], &wire.stdout);
    assert_eq!(emitted.code, from_wire.code, "{path}: the forms disagree");
    let sequence = imported.sequence.name.as_str();
    let row = |class, detail| Row {
        class,
        detail,
        lists,
        path: path.to_owned(),
    };
    match emitted.code {
        0 => {
            assert_eq!(
                emitted.stdout, from_wire.stdout,
                "{path}: the forms disagree"
            );
            let round_trip = id_of(&emitted.stdout);
            let (class, detail) = explained(path, &original, sequence, &round_trip);
            row(class, detail)
        }
        1 => {
            // The same findings whichever form carried the sequence; only the text has spans.
            let findings = emitted.report();
            assert_eq!(
                finding_codes(&findings),
                finding_codes(&from_wire.report()),
                "{path}: the forms disagree"
            );
            let codes = finding_codes(&findings);
            if oce(&["id", &relative, "--from", "cxf"], b"").code != 0 {
                return row("no-id", codes);
            }
            // The original loads, so only the engine's warnings stand between the sequence
            // and an emitted document; what the emitter alone writes still has its id.
            assert!(
                findings["findings"]
                    .as_array()
                    .expect("findings")
                    .iter()
                    .all(|finding| finding["origin"] == "engine"),
                "{path}: an author-side finding on a document the engine loads"
            );
            let alone = oce_author::emit(&imported.sequence, &EmitOptions::new())
                .expect("stage 1 accepts the sequence");
            let (class, _) = explained(path, &original, sequence, &id_of(&alone));
            row("warned", format!("{codes}; {class}"))
        }
        other => panic!("{path}: oce emit exited {other}: {}", emitted.transcript()),
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
                        .map(|path| classify(path, &dir))
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

fn tally<'a>(items: impl Iterator<Item = &'a str>) -> Vec<(&'a str, usize)> {
    let mut counts = BTreeMap::new();
    for item in items {
        *counts.entry(item).or_insert(0) += 1;
    }
    counts.into_iter().collect()
}

#[test]
fn every_tracked_cxf_document_round_trips_or_is_refused_with_a_stable_code() {
    let rows = rows();
    let mut table = String::from(
        "# Every tracked CXF document through `oce import`, then `oce emit` and `oce id`\n\
         # (tests/import_corpus.rs holds the classes). Regenerate with OCE_BLESS_CLI=1.\n\
         # class<TAB>detail<TAB>dropped<TAB>normalised<TAB>path\n",
    );
    for row in &rows {
        table.push_str(&row.line());
    }
    compare_or_bless("import_corpus.tsv", &table);

    // Pinned, so that a CXF fixture added anywhere in the repository is classified here.
    assert_eq!(
        tally(rows.iter().map(|row| row.class)),
        [
            ("no-id", 12),
            ("refused", 188),
            ("same-id", 17),
            ("same-id-real-literals", 1),
            ("same-id-restamped", 37),
            ("warned", 2),
        ]
    );
    assert_eq!(rows.len(), 257);
    assert_eq!(
        tally(
            rows.iter()
                .filter(|row| row.class == "refused")
                .map(|row| row.detail.as_str())
        ),
        [
            ("array-connector", 1),
            ("array-sequence-parameter", 1),
            ("array-shape", 3),
            ("class-outside-obc", 9),
            ("conditional-component", 16),
            ("constant-member", 1),
            ("datatype-mismatch", 4),
            ("expression-value", 6),
            ("instance-member", 30),
            ("invalid-name", 2),
            ("namespace-mismatch", 2),
            ("nested-composite", 27),
            ("no-root", 2),
            ("port-attribute", 15),
            ("stray-node", 3),
            ("unknown-datatype", 1),
            ("unknown-key", 54),
            ("unresolved-reference", 10),
            ("wrong-shape", 1),
        ]
    );
    // The name prefixes of the re-stamped class.
    assert_eq!(
        tally(
            rows.iter()
                .filter(|row| row.class == "same-id-restamped")
                .map(|row| row.detail.as_str())
        ),
        [("ex.", 4), ("g36.profile.", 2), ("g36.source.", 31)]
    );
    // What the importer read and did not carry, over the 69 documents it accepts.
    let lists = rows.iter().filter_map(|row| row.lists);
    let (documents, dropped, normalised) = lists.fold((0, 0, 0), |sum, (dropped, normalised)| {
        (sum.0 + 1, sum.1 + dropped, sum.2 + normalised)
    });
    assert_eq!((documents, dropped, normalised), (69, 397, 1));

    // The 46 fixtures of the G36 catalog (importer note, "Where the corpus stands"): the 31
    // inside the authoring subset round-trip to the id of the committed fixture re-stamped,
    // and the other 15 are refused under the fixed precedence.
    let catalog: Vec<&Row> = rows
        .iter()
        .filter(|row| row.path.starts_with("crates/oce-cxf/tests/fixtures/g36/"))
        // The interface miniature beside them is not a catalog fixture.
        .filter(|row| !row.path.ends_with("/member_list_interface.jsonld"))
        .collect();
    assert_eq!(catalog.len(), 46);
    assert_eq!(
        tally(catalog.iter().map(|row| {
            if row.class == "refused" {
                row.detail.as_str()
            } else {
                row.class
            }
        })),
        [
            ("conditional-component", 7),
            ("expression-value", 1),
            ("nested-composite", 4),
            ("port-attribute", 3),
            ("same-id-restamped", 31),
        ]
    );
    for row in catalog.iter().filter(|row| row.class != "refused") {
        assert_eq!(row.detail, "g36.source.", "{}", row.path);
    }
}
