//! Vendored-corpus characterization capture — the `_spec/19` §5 delta pin.
//!
//! The 44 vendored modelica-json translations under
//! `third_party/modelica-buildings-cdl/cxf/` (pin `85721b82`) cannot load while the
//! cross-document class library and the upstream guard corruption stay open, so no load pin is
//! possible; what this harness pins instead is the exact per-document diagnostic surface of the
//! production `import_cxf` path — per-`DiagCode` counts split by severity, plus the number of
//! duplicate `(code, subject, message)` triples — against the checked-in expectation table in
//! `tests/vendored_corpus_expectations/mod.rs`. Any movement in either direction is a red until
//! the table is deliberately re-blessed, which is what turns every future resolver change on
//! this corpus into a declared delta instead of a silent drift.
//!
//! Discovery is a recursive walk (the documents sit ~7 directories deep) and the harness
//! asserts it found **exactly 44** documents before comparing anything — an empty or partial
//! capture is a red, never a vacuous green. Deterministic and offline: files are read from the
//! vendored tree, never fetched.
//!
//! The pinned post-state carries the `_spec/19` interface derivation's declared movement, both
//! directions stated in advance (an undeclared increase is a defect, not noise):
//! - `malformed-document` 944 → 10: 904 arity mismatches removed by derived interfaces, 30
//!   replaced by `composite/vector-port-instance` (the 10 `composite/root-count` refusals on
//!   the enumeration-only `Types/*.jsonld` documents remain);
//! - `non-subset-construct` 81 → 111: +30 `composite/vector-port-instance`, the scalar-only
//!   refusal on the three documents whose classes publish no port names;
//! - `unresolved-reference` 2,816 → 214: repeated missing endpoints contribute once per endpoint;
//!   every surviving subject sits on the 18 `class-not-found` or 30 vector-port instances, which
//!   derive no interface;
//! - `single-assignment` 0 → 57 (declared INCREASE): 31 undriven + 8 multiply-driven derived
//!   inputs, plus 18 multiply-driven declared boundary outputs — the `_spec/18` R19-16
//!   migration off the wrong `unresolved-reference` code, `refuse_multiply_driven` assessing
//!   this dialect for the first time;
//! - `grounding-failed` 62 → 204 (declared INCREASE): member values and member connector
//!   bounds ground for the first time, so the class-translation corpus's valueless-parameter
//!   references, its upstream-corrupt `undefined` value strings, and its out-of-subset value
//!   expressions now fail loudly at the member level instead of never being read;
//! - `undriven-boundary-output` warnings 131 → 27: derived interfaces supply the drivers;
//! - `inactive-conditional-node` stays 44 (the R19-15 traversal's own movement, taken at the
//!   traversal-only capture) and `class-not-found` / `conditional-guard-*` stay put.
//!
//! Regenerate the expectation table after an intentional resolver change (the writer emits
//! mechanical layout, so follow it with `cargo fmt --all`):
//! ```text
//! OCE_BLESS=1 cargo test -p oce-cxf --test vendored_corpus_delta
//! ```

mod bless;
mod vendored_corpus_expectations;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::Diagnostic;

use vendored_corpus_expectations::{AGGREGATE, EXPECTED, VENDORED_DOCUMENT_COUNT};

/// Root of the vendored corpus, resolved from the crate manifest.
fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../third_party/modelica-buildings-cdl/cxf")
}

/// Recursively collect every `*.jsonld` under `dir`, keyed by its `/`-joined path relative to
/// the corpus root, sorted by that key.
fn discover() -> Vec<(String, PathBuf)> {
    let root = corpus_root();
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| {
            panic!(
                "vendored corpus dir {} must be readable: {e}",
                dir.display()
            )
        });
        for entry in entries {
            let path = entry.expect("readable directory entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "jsonld") {
                let rel = path
                    .strip_prefix(&root)
                    .expect("walked path lies under the corpus root")
                    .components()
                    .map(|c| c.as_os_str().to_str().expect("UTF-8 corpus path"))
                    .collect::<Vec<_>>()
                    .join("/");
                found.push((rel, path));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// One document's captured diagnostic surface.
struct DocCapture {
    /// `(code, severity)` → count, in sorted key order.
    counts: Vec<(String, String, usize)>,
    /// Occurrences beyond the first of each exact `(code, subject, message)` triple.
    duplicate_triples: usize,
}

/// Import one vendored document on the production path and characterize its diagnostics.
/// A load is characterized by its report's diagnostics; a refusal by its validation vector.
fn capture(path: &Path) -> (Vec<Diagnostic>, DocCapture) {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|e| panic!("vendored document {} must be readable: {e}", path.display()));
    let diags = match import_cxf(&bytes, &ResolveOptions::default()) {
        Ok((_, report)) => report.diagnostics,
        Err(CxfError::Validation(diags)) => diags,
        Err(other) => panic!(
            "{}: non-validation import failure: {other:?}",
            path.display()
        ),
    };
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut triples: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    for d in &diags {
        *counts
            .entry((d.code.as_str().to_owned(), d.severity.as_str().to_owned()))
            .or_insert(0) += 1;
        *triples
            .entry((
                d.code.as_str().to_owned(),
                d.subject.as_deref().unwrap_or("").to_owned(),
                d.message.clone(),
            ))
            .or_insert(0) += 1;
    }
    let duplicate_triples = triples.values().filter(|&&n| n > 1).map(|&n| n - 1).sum();
    (
        diags,
        DocCapture {
            counts: counts
                .into_iter()
                .map(|((code, severity), n)| (code, severity, n))
                .collect(),
            duplicate_triples,
        },
    )
}

/// Corpus-wide aggregate of the same capture: per-`(code, severity)` totals, exact
/// message-class splits for the codes whose movement `_spec/19` §5 declares, and duplicate
/// triples per code.
struct Aggregate {
    counts: BTreeMap<(String, String), usize>,
    unresolved_reference_messages: BTreeMap<String, usize>,
    inactive_conditional_messages: BTreeMap<String, usize>,
    duplicates_by_code: BTreeMap<String, usize>,
    duplicates_total: usize,
}

fn aggregate(all: &[(String, Vec<Diagnostic>, DocCapture)]) -> Aggregate {
    let mut agg = Aggregate {
        counts: BTreeMap::new(),
        unresolved_reference_messages: BTreeMap::new(),
        inactive_conditional_messages: BTreeMap::new(),
        duplicates_by_code: BTreeMap::new(),
        duplicates_total: 0,
    };
    for (_, diags, doc) in all {
        for (code, severity, n) in &doc.counts {
            *agg.counts
                .entry((code.clone(), severity.clone()))
                .or_insert(0) += n;
        }
        agg.duplicates_total += doc.duplicate_triples;
        let mut triples: BTreeMap<(String, String, String), usize> = BTreeMap::new();
        for d in diags {
            *triples
                .entry((
                    d.code.as_str().to_owned(),
                    d.subject.as_deref().unwrap_or("").to_owned(),
                    d.message.clone(),
                ))
                .or_insert(0) += 1;
            match d.code.as_str() {
                "unresolved-reference" => {
                    *agg.unresolved_reference_messages
                        .entry(d.message.clone())
                        .or_insert(0) += 1;
                }
                "inactive-conditional-node" => {
                    *agg.inactive_conditional_messages
                        .entry(d.message.clone())
                        .or_insert(0) += 1;
                }
                _ => {}
            }
        }
        for ((code, _, _), n) in triples {
            if n > 1 {
                *agg.duplicates_by_code.entry(code).or_insert(0) += n - 1;
            }
        }
    }
    agg
}

/// Render the expectation module source from a live capture (the `OCE_BLESS` writer).
fn render_expectations(all: &[(String, Vec<Diagnostic>, DocCapture)], agg: &Aggregate) -> String {
    let mut out = String::from(
        "//! GENERATED expectation table for `vendored_corpus_delta` — regenerate with\n\
         //! `OCE_BLESS=1 cargo test -p oce-cxf --test vendored_corpus_delta`, then review the\n\
         //! diff: every movement here is a declared consequence of a resolver change, never\n\
         //! noise. Counts are per document and per `(DiagCode, severity)`;\n\
         //! `duplicate_triples` counts occurrences beyond the first of each exact\n\
         //! `(code, subject, message)` triple.\n\n",
    );
    let _ = writeln!(
        out,
        "/// Exactly how many vendored documents the recursive walk must find.\n\
         pub(crate) const VENDORED_DOCUMENT_COUNT: usize = {};\n",
        all.len()
    );
    out.push_str(
        "/// One document's pinned diagnostic surface.\n\
         pub(crate) struct DocExpectation {\n\
         \x20   /// Path relative to the vendored `cxf/` root, `/`-joined.\n\
         \x20   pub(crate) rel: &'static str,\n\
         \x20   /// `(diag-code, severity, count)` rows, sorted by code then severity.\n\
         \x20   pub(crate) counts: &'static [(&'static str, &'static str, usize)],\n\
         \x20   /// Duplicate `(code, subject, message)` occurrences beyond the first.\n\
         \x20   pub(crate) duplicate_triples: usize,\n\
         }\n\n\
         /// Corpus-wide totals over the same capture.\n\
         pub(crate) struct CorpusAggregate {\n\
         \x20   /// `(diag-code, severity, corpus-wide count)` rows.\n\
         \x20   pub(crate) counts: &'static [(&'static str, &'static str, usize)],\n\
         \x20   /// Exact message-class split of `unresolved-reference`.\n\
         \x20   pub(crate) unresolved_reference_messages: &'static [(&'static str, usize)],\n\
         \x20   /// Exact message-class split of `inactive-conditional-node`.\n\
         \x20   pub(crate) inactive_conditional_messages: &'static [(&'static str, usize)],\n\
         \x20   /// Duplicate-triple occurrences beyond the first, per diag-code.\n\
         \x20   pub(crate) duplicates_by_code: &'static [(&'static str, usize)],\n\
         \x20   /// Duplicate-triple occurrences beyond the first, corpus-wide.\n\
         \x20   pub(crate) duplicates_total: usize,\n\
         }\n\n",
    );
    out.push_str("pub(crate) const EXPECTED: &[DocExpectation] = &[\n");
    for (rel, _, doc) in all {
        let _ = writeln!(out, "    DocExpectation {{");
        let _ = writeln!(out, "        rel: {rel:?},");
        let _ = writeln!(out, "        counts: &[");
        for (code, severity, n) in &doc.counts {
            let _ = writeln!(out, "            ({code:?}, {severity:?}, {n}),");
        }
        let _ = writeln!(out, "        ],");
        let _ = writeln!(out, "        duplicate_triples: {},", doc.duplicate_triples);
        let _ = writeln!(out, "    }},");
    }
    out.push_str("];\n\n");
    out.push_str(
        "pub(crate) const AGGREGATE: CorpusAggregate = CorpusAggregate {\n    counts: &[\n",
    );
    for ((code, severity), n) in &agg.counts {
        let _ = writeln!(out, "        ({code:?}, {severity:?}, {n}),");
    }
    out.push_str("    ],\n    unresolved_reference_messages: &[\n");
    for (message, n) in &agg.unresolved_reference_messages {
        let _ = writeln!(out, "        ({message:?}, {n}),");
    }
    out.push_str("    ],\n    inactive_conditional_messages: &[\n");
    for (message, n) in &agg.inactive_conditional_messages {
        let _ = writeln!(out, "        ({message:?}, {n}),");
    }
    out.push_str("    ],\n    duplicates_by_code: &[\n");
    for (code, n) in &agg.duplicates_by_code {
        let _ = writeln!(out, "        ({code:?}, {n}),");
    }
    let _ = writeln!(
        out,
        "    ],\n    duplicates_total: {},\n}};",
        agg.duplicates_total
    );
    out
}

/// Filesystem path of the generated expectation module.
fn expectations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/vendored_corpus_expectations/mod.rs")
}

#[test]
fn vendored_corpus_diagnostic_surface_matches_the_pinned_capture() {
    let discovered = discover();
    assert_eq!(
        discovered.len(),
        VENDORED_DOCUMENT_COUNT,
        "the recursive walk must find exactly the pinned vendored document count; \
         an empty or partial capture is a red, never a green"
    );
    let all: Vec<(String, Vec<Diagnostic>, DocCapture)> = discovered
        .into_iter()
        .map(|(rel, path)| {
            let (diags, doc) = capture(&path);
            (rel, diags, doc)
        })
        .collect();
    let agg = aggregate(&all);

    if bless::enabled() {
        std::fs::write(expectations_path(), render_expectations(&all, &agg))
            .expect("write regenerated expectation table");
        return;
    }

    let expected_rels: Vec<&str> = EXPECTED.iter().map(|e| e.rel).collect();
    let actual_rels: Vec<&str> = all.iter().map(|(rel, ..)| rel.as_str()).collect();
    assert_eq!(
        actual_rels, expected_rels,
        "the on-disk vendored corpus and the expectation table must stay one-to-one"
    );
    for ((rel, _, doc), expected) in all.iter().zip(EXPECTED) {
        let actual: Vec<(&str, &str, usize)> = doc
            .counts
            .iter()
            .map(|(code, severity, n)| (code.as_str(), severity.as_str(), *n))
            .collect();
        assert_eq!(
            actual, expected.counts,
            "{rel}: per-code diagnostic counts moved without a re-bless"
        );
        assert_eq!(
            doc.duplicate_triples, expected.duplicate_triples,
            "{rel}: duplicate (code, subject, message) count moved without a re-bless"
        );
    }

    let actual_counts: Vec<(&str, &str, usize)> = agg
        .counts
        .iter()
        .map(|((code, severity), n)| (code.as_str(), severity.as_str(), *n))
        .collect();
    assert_eq!(
        actual_counts, AGGREGATE.counts,
        "corpus-wide per-code totals moved"
    );
    let as_pairs = |m: &BTreeMap<String, usize>| -> Vec<(String, usize)> {
        m.iter().map(|(k, v)| (k.clone(), *v)).collect()
    };
    let expected_pairs = |rows: &[(&str, usize)]| -> Vec<(String, usize)> {
        rows.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
    };
    assert_eq!(
        as_pairs(&agg.unresolved_reference_messages),
        expected_pairs(AGGREGATE.unresolved_reference_messages),
        "unresolved-reference message-class split moved"
    );
    assert_eq!(
        as_pairs(&agg.inactive_conditional_messages),
        expected_pairs(AGGREGATE.inactive_conditional_messages),
        "inactive-conditional-node message-class split moved"
    );
    assert_eq!(
        as_pairs(&agg.duplicates_by_code),
        expected_pairs(AGGREGATE.duplicates_by_code),
        "per-code duplicate-triple totals moved"
    );
    assert_eq!(
        agg.duplicates_total, AGGREGATE.duplicates_total,
        "corpus-wide duplicate-triple total moved"
    );
}
