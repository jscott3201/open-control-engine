//! What an export's diagnostic list costs, in requested bytes and never in elapsed time (issue
//! #753; `TESTING.md`: cost is asserted with deterministic measures).
//!
//! The exporter raises one `export-unsupported` finding per parameter, connector or connection of
//! a block it cannot emit, each copying the block's instance IRI as its subject, and one
//! `export-deferred` warning per deferred block, which copies its IRI twice. Its list was a plain
//! vector outside the 8 MiB budget the reader and the validator hold their lists to, so a
//! hand-built graph whose block carries a long IRI made a list of `items × IRI bytes`: 1,024
//! non-finite parameters on a block with a 64 KiB IRI were a 64 MiB list. The export now raises
//! into the same `DiagnosticList`: the first finding that does not fit ends the list, and one
//! closing `resource-limit-exceeded` error counts the rest, which are never built.
//!
//! No loaded model reaches these lists (validation refuses the shapes first), so the graphs are
//! the minimal loop read by the reader and edited by hand, as a host may build one. Measured with
//! `allocation-counter`, whose counters are local to the measuring thread. The ceilings have
//! headroom over what was measured (in brackets, with what the same graph cost before the
//! change).

use std::sync::Arc;

use allocation_counter::{AllocationInfo, measure};
use oce_cxf::{CxfError, ResolveOptions, export, export_with_report, import_cxf};
use oce_diag::{DiagCode, Diagnostic, Severity};
use oce_model::{BlockId, BlockInstance, EnumClassId, ModelGraph, ParamTable, Value};

const MODEL: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

/// The diagnostic list's budget of message and subject text
/// (`oce_diag::MAX_DIAGNOSTIC_TEXT_BYTES`), restated so this test fails if it moves.
const BUDGET: usize = 8 * 1024 * 1024;

/// The message of a non-finite parameter named `name`.
fn non_finite_message(name: &str) -> String {
    format!(
        "export subset: parameter `{name}` is a non-finite Real, which cannot round-trip \
         through JSON"
    )
}

/// The minimal loop as the reader builds it, with its first block named by an IRI of `iri_bytes`
/// bytes and `count` more parameters, `p0000` …, each a non-finite Real.
fn non_finite_parameters(iri_bytes: usize, count: usize) -> (ModelGraph, Arc<str>) {
    let (mut graph, _) =
        import_cxf(MODEL.as_bytes(), &ResolveOptions::default()).expect("the model reads");
    let stem = "urn:long:";
    let iri: Arc<str> = Arc::from(format!("{stem}{}", "x".repeat(iri_bytes - stem.len())));
    graph.blocks[0].instance_iri = Some(Arc::clone(&iri));
    for i in 0..count {
        graph.blocks[0]
            .params
            .values
            .push((Arc::from(format!("p{i:04}")), Value::Real(f64::NAN)));
    }
    (graph, iri)
}

/// One export of `graph`, measured; a clean export of the unedited model runs first.
fn exported(graph: &ModelGraph) -> (Vec<Diagnostic>, AllocationInfo) {
    let (model, _) = import_cxf(MODEL.as_bytes(), &ResolveOptions::default()).expect("reads");
    export(&model).expect("the model exports");
    let mut result = None;
    let info = measure(|| result = Some(export_with_report(graph)));
    match result.expect("the export ran") {
        Err(CxfError::Validation(diagnostics)) => (diagnostics, info),
        Err(other) => panic!("not a validation refusal: {other}"),
        Ok(_) => panic!("the export succeeded"),
    }
}

/// The text the list counts for `diagnostics`: each one's message and subject.
fn text(diagnostics: &[Diagnostic]) -> usize {
    diagnostics
        .iter()
        .map(|d| d.message.len() + d.subject.as_deref().map_or(0, str::len))
        .sum()
}

#[test]
fn many_findings_about_one_long_iri_hold_the_budget_and_count_the_rest() {
    // 256, 1,024 and 4,096 non-finite parameters on a block with a 64 KiB IRI. The list holds
    // the findings that fit the budget, then one closing error counting the rest; what the export
    // requests is about the budget whatever the number of findings. [measured: peaks of 8,827,679
    // / 8,901,407 / 9,196,319 bytes; before, 17,261,467 at 256 and 67,848,091 at 1,024]
    let (mut peaks, mut totals) = (Vec::new(), Vec::new());
    for count in [256, 1_024, 4_096] {
        let (graph, iri) = non_finite_parameters(64 * 1024, count);
        let (diagnostics, info) = exported(&graph);
        let (closing, listed) = diagnostics.split_last().expect("a list");
        assert_eq!(closing.code, DiagCode::ResourceLimitExceeded);
        assert!(closing.subject.is_none());
        let unlisted = count - listed.len();
        assert!(
            closing.message.starts_with(&format!(
                "{unlisted} more diagnostic(s) are not listed, the first of them \
                 `export-unsupported` on `urn:long:"
            )),
            "{}",
            closing.message
        );
        assert!(
            listed
                .iter()
                .all(|d| d.code == DiagCode::ExportUnsupported
                    && d.subject.as_deref() == Some(&*iri))
        );
        assert!(text(listed) <= BUDGET);
        assert!(
            info.bytes_max <= (BUDGET + 2 * 1024 * 1024) as u64,
            "{count} findings: a peak of {}",
            info.bytes_max
        );
        peaks.push(info.bytes_max);
        totals.push(info.bytes_total);
    }
    // From 1x to 16x the findings, the peak and the total grow by the graph's own parameters,
    // never by a copy of the IRI a finding: a finding past the budget is counted, not built.
    // [measured: totals of 17,320,332 / 17,394,060 / 17,688,972, 96 bytes a further finding]
    assert!(peaks[2] - peaks[0] <= 3_840 * 512, "{peaks:?}");
    assert!(totals[2] - totals[0] <= 3_840 * 512, "{totals:?}");
}

#[test]
fn findings_that_fill_the_budget_exactly_are_all_listed_and_one_more_closes_the_list() {
    // 128 findings of exactly 65,536 bytes of text each are the budget: all listed, no closing
    // error. A 129th is counted by the closing error and never built.
    let message = non_finite_message("p0000").len();
    let iri_bytes = 65_536 - message;
    let (graph, iri) = non_finite_parameters(iri_bytes, 128);
    let (diagnostics, _) = exported(&graph);
    assert_eq!(diagnostics.len(), 128);
    assert_eq!(text(&diagnostics), BUDGET);
    assert!(diagnostics.iter().enumerate().all(|(i, d)| {
        d.code == DiagCode::ExportUnsupported
            && d.subject.as_deref() == Some(&*iri)
            && d.message == non_finite_message(&format!("p{i:04}"))
    }));
    let (graph, _) = non_finite_parameters(iri_bytes, 129);
    let (diagnostics, _) = exported(&graph);
    assert_eq!(diagnostics.len(), 129);
    let closing = diagnostics.last().expect("a closing error");
    assert_eq!(closing.code, DiagCode::ResourceLimitExceeded);
    assert!(
        closing.message.starts_with(
            "1 more diagnostic(s) are not listed, the first of them `export-unsupported`"
        ),
        "{}",
        closing.message
    );
}

/// The minimal loop with `count` more blocks, each a `Constant` with an enumeration parameter
/// and no connector, all named by one IRI of `iri_bytes` bytes: each is deferred with a warning
/// that names the IRI twice.
fn deferred_blocks(iri_bytes: usize, count: usize) -> ModelGraph {
    let (mut graph, _) =
        import_cxf(MODEL.as_bytes(), &ResolveOptions::default()).expect("the model reads");
    let iri: Arc<str> = Arc::from(format!("urn:d:{}", "x".repeat(iri_bytes - 6)));
    for _ in 0..count {
        let id = BlockId(graph.blocks.len() as u32);
        graph.blocks.push(BlockInstance {
            id,
            class_iri: Arc::from("Buildings.Controls.OBC.CDL.Reals.Sources.Constant"),
            inputs: Vec::new(),
            outputs: Vec::new(),
            params: ParamTable {
                values: vec![(
                    Arc::from("e"),
                    Value::Enum {
                        class: EnumClassId(0),
                        ordinal: 1,
                    },
                )],
            },
            decl_order: id.0,
            instance_iri: Some(Arc::clone(&iri)),
        });
    }
    graph
}

#[test]
fn deferral_warnings_are_held_to_the_budget_too() {
    // A few deferred blocks export with their warnings. 256 blocks sharing one 64 KiB IRI raise
    // 32 MiB of warnings: the list ends at the budget, and its closing error makes the export a
    // refusal, so an export never succeeds with warnings left out: 63 warnings listed. [measured:
    // a peak of 8,608,769 bytes; before, the export succeeded holding 256 warnings of 128 KiB
    // each, 32 MiB]
    let few = export_with_report(&deferred_blocks(64 * 1024, 4)).expect("a partial export");
    assert_eq!(few.warnings.len(), 4);
    assert!(
        few.warnings
            .iter()
            .all(|w| w.code == DiagCode::ExportDeferred && w.severity == Severity::Warning)
    );
    let (diagnostics, info) = exported(&deferred_blocks(64 * 1024, 256));
    let (closing, listed) = diagnostics.split_last().expect("a list");
    assert_eq!(closing.code, DiagCode::ResourceLimitExceeded);
    assert!(listed.iter().all(|w| w.code == DiagCode::ExportDeferred));
    assert_eq!(listed.len(), 63);
    assert!(text(listed) <= BUDGET);
    assert!(
        info.bytes_max <= (BUDGET + 2 * 1024 * 1024) as u64,
        "a peak of {}",
        info.bytes_max
    );
}
