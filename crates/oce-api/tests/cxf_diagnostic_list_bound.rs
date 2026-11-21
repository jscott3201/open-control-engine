//! What a host sees when a CXF read's diagnostics pass the reader's budget of text (GitHub issue
//! #695), through `Engine::load_cxf` and `Engine::load_cxf_with_receipt`.
//!
//! The reader lists diagnostics while their messages and subjects fit 8 MiB, a subject counted
//! for every diagnostic that carries it. The first diagnostic that does not fit ends the list,
//! and one `resource-limit-exceeded` error closes it: it counts what is not listed and names the
//! first of those by its code and an excerpt of its subject. These tests pin the three things a
//! host can observe beyond a long refusal list:
//!
//! - **Warnings alone can refuse a load.** A document whose warnings fit the budget loads with
//!   every one of them; one more warning is a refusal, never a load with a warning left out.
//! - **One diagnostic larger than the whole budget** leaves the closing error alone, which still
//!   says what was refused and where.
//! - **Where the closing error is.** It is the last diagnostic of the error's list, and the first
//!   import-stage record of a receipt, whose records are sorted by key and where a record with
//!   no subject sorts before every record with one.
//!
//! What the bound costs is measured in `cxf_diagnostic_list_cost.rs`.

use oce_api::{DiagnosticSeverity, DiagnosticStage, DiagnosticSubject, Engine};
use oce_diag::{DiagCode, Diagnostic, Severity};

const MODEL: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

/// The reader's budget of message and subject text (`oce-cxf`'s `MAX_DIAGNOSTIC_TEXT_BYTES`),
/// restated so this test fails if the budget moves without review.
const BUDGET: usize = 8 * 1024 * 1024;

/// [`MODEL`] with `terms` added to its `@context` and `nodes` appended to its `@graph`.
fn with_nodes(model: &str, terms: &str, nodes: &str) -> Vec<u8> {
    let context = "\"base\": \"http://example.org#\"";
    let last = "\"S231:isOfDataType\": { \"@id\": \"S231:Boolean\" } }\n  ]";
    assert_eq!(model.matches(context).count(), 1);
    assert_eq!(model.matches(last).count(), 1);
    model
        .replacen(context, &format!("{context}{terms}"), 1)
        .replacen(
            last,
            &format!("\"S231:isOfDataType\": {{ \"@id\": \"S231:Boolean\" }} }}{nodes}\n  ]"),
            1,
        )
        .into_bytes()
}

/// The closing error of a list that left `unlisted` diagnostics out, the first of them described
/// as `first`, stated by hand.
fn closing(unlisted: usize, first: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::ResourceLimitExceeded,
        format!(
            "{unlisted} more diagnostic(s) are not listed, the first of them {first}: a \
             diagnostic list holds at most 8388608 bytes of message and subject text"
        ),
    )
}

/// The message and subject text of `diagnostics`, a subject for every diagnostic that carries it.
fn text(diagnostics: &[Diagnostic]) -> usize {
    diagnostics
        .iter()
        .map(|diagnostic| {
            diagnostic.message.len() + diagnostic.subject.as_deref().map_or(0, str::len)
        })
        .sum()
}

/// The namespace IRI the unused inputs are declared under: 64 KiB.
fn long_namespace() -> String {
    let stem = "urn:long:";
    format!("{stem}{}#", "x".repeat(64 * 1024 - stem.len() - 1))
}

/// [`MODEL`] with `count` more root boundary inputs `p:u<i>` that no connection reads, `p` bound
/// to [`long_namespace`]: each one raises an `unused-boundary-input` warning on its expanded
/// `@id`, 64 KiB and four characters long.
fn unused_inputs(count: usize) -> Vec<u8> {
    let root = "\"S231:hasInput\": { \"@id\": \"http://example.org#MinLoop.uSet\" },";
    assert_eq!(MODEL.matches(root).count(), 1);
    let references: String = (0..count)
        .map(|i| format!(", {{ \"@id\": \"p:u{i:03}\" }}"))
        .collect();
    let nodes: String = (0..count)
        .map(|i| {
            format!(
                ",\n    {{ \"@id\": \"p:u{i:03}\", \"@type\": \"S231:RealInput\", \
                 \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }} }}"
            )
        })
        .collect();
    let model = MODEL.replacen(
        root,
        &format!(
            "\"S231:hasInput\": [ {{ \"@id\": \"http://example.org#MinLoop.uSet\" }}{references} ],"
        ),
        1,
    );
    with_nodes(
        &model,
        &format!(", \"p\": \"{}\"", long_namespace()),
        &nodes,
    )
}

/// The warning an unused boundary input `p:u<index>` raises, stated by hand.
fn unused_input_warning(index: usize) -> Diagnostic {
    Diagnostic::warning(
        DiagCode::UnusedBoundaryInput,
        "declared boundary input is read by no connection",
    )
    .with_subject(format!("{}u{index:03}", long_namespace()))
}

#[test]
fn warnings_that_fill_the_budget_load_whole_and_one_more_warning_refuses_the_load() {
    // Each warning is its fixed message and its 64 KiB subject, so the budget holds 127 of them.
    let each = text(&[unused_input_warning(0)]);
    let fit = BUDGET / each;
    assert_eq!((each, fit), (65_588, 127));
    let every_warning =
        |count: usize| -> Vec<Diagnostic> { (0..count).map(unused_input_warning).collect() };

    let (report, _) = Engine::new()
        .load_cxf(&unused_inputs(fit))
        .expect("warnings within the budget do not refuse a load");
    assert_eq!(report.warnings, every_warning(fit));

    // One more unused input: the load is refused. The 127 warnings that fit are listed as they
    // are for the document that loads, and the closing error counts the one left out.
    let error = Engine::new()
        .load_cxf(&unused_inputs(fit + 1))
        .expect_err("a load never succeeds with a warning left out");
    let diagnostics = error.diagnostics();
    assert_eq!(diagnostics.len(), fit + 1);
    assert_eq!(&diagnostics[..fit], &every_warning(fit)[..]);
    let subject = format!("urn:long:{}…", "x".repeat(55));
    let last = &diagnostics[fit];
    assert_eq!(
        last,
        &closing(1, &format!("`unused-boundary-input` on `{subject}`"))
    );
    assert_eq!(last.severity, Severity::Error);
}

/// [`MODEL`] with the value of the constant's parameter `k` replaced by a typed literal whose
/// lexical form is `lexical`.
fn with_literal(lexical: &str) -> Vec<u8> {
    let value = "\"S231:value\": { \"@value\": \"2.0\",";
    assert_eq!(MODEL.matches(value).count(), 1);
    MODEL
        .replacen(
            value,
            &format!("\"S231:value\": {{ \"@value\": \"{lexical}\","),
            1,
        )
        .into_bytes()
}

#[test]
fn one_diagnostic_larger_than_the_budget_is_refused_with_a_closing_error_that_names_it() {
    // A literal of 1.5 million DEL characters is no number, and the refusal quotes it with each
    // DEL written as an escape of six bytes: one message of about 9 MB, which no list holds.
    let document = with_literal(&"\u{7f}".repeat(1_500_000));
    let error = Engine::new()
        .load_cxf(&document)
        .expect_err("a literal that is no number is refused");
    assert_eq!(
        error.diagnostics(),
        [closing(
            1,
            "`grounding-failed` on `http://example.org#MinLoop.con.k`"
        )]
    );
}

#[test]
fn an_oversized_refusal_of_the_expansion_pass_is_a_refusal_and_never_an_empty_list() {
    // A relative `@id` is refused where identities are expanded, in a message that quotes it
    // and on a subject that is it: twice 4.3 MiB, more than a list holds. The pass has raised
    // nothing else, and what it returns is a refusal all the same.
    let id = "r".repeat(4_300_000);
    let document = with_nodes(MODEL, "", &format!(",\n    {{ \"@id\": \"{id}\" }}"));
    let error = Engine::new()
        .load_cxf(&document)
        .expect_err("a relative @id is refused");
    assert_eq!(
        error.diagnostics(),
        [closing(
            1,
            &format!("`relative-iri` on `{}…`", "r".repeat(64))
        )]
    );
}

#[test]
fn the_closing_error_is_last_in_the_errors_list_and_the_first_import_record_of_a_receipt() {
    // 200 relative references on one node of 64 KiB: 63 fit the budget.
    let id = format!("urn:long:{}", "x".repeat(64 * 1024));
    let references: Vec<&str> = vec!["{ \"@id\": \"r\" }"; 200];
    let node = format!(
        ",\n    {{ \"@id\": \"{id}\", \"S231:hasInput\": [ {} ] }}",
        references.join(", ")
    );
    let document = with_nodes(MODEL, "", &node);

    let failure = Engine::new()
        .load_cxf_with_receipt(&document)
        .expect_err("relative references are refused");
    let records = failure.diagnostics().records();
    let diagnostics = failure.error().diagnostics();
    let listed = diagnostics.len() - 1;
    assert_eq!(listed, 63);

    // In the error's own list the closing error is last.
    let last = &diagnostics[listed];
    assert_eq!(last.code, DiagCode::ResourceLimitExceeded);
    assert!(
        diagnostics[..listed]
            .iter()
            .all(|diagnostic| diagnostic.code == DiagCode::RelativeIri)
    );

    // A receipt sorts its records by stage, subject, code and severity, and a record with no
    // subject sorts before every record with one: the closing error is record 0.
    assert_eq!(records.len(), diagnostics.len());
    let first = records[0].key();
    assert_eq!(first.stage(), DiagnosticStage::Import);
    assert_eq!(first.subject(), &DiagnosticSubject::Absent);
    assert_eq!(first.code(), "resource-limit-exceeded");
    assert_eq!(first.severity(), DiagnosticSeverity::Error);
    assert_eq!(records[0].message(), last.message);
    assert!(records[1..].iter().all(|record| {
        record.key().code() == "relative-iri"
            && record.key().subject() == &DiagnosticSubject::Opaque(id.clone())
    }));
}
