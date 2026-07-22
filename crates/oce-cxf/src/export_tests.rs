//! Tests for the staged [`export`](crate::export) floor: every model — freshly imported or
//! hand-built — is rejected with a single typed `ExportUnsupported` error diagnostic (subject
//! `None`, pinned message), never a panic, and the whole rejection is identical across calls
//! and inputs.

use oce_diag::{DiagCode, Diagnostic, Severity};
use oce_model::ModelGraph;

use super::{CxfError, ResolveOptions, export, import_cxf};

const FIXTURE: &str = include_str!("../tests/fixtures/minimal_loop.jsonld");

/// The pinned, host-visible rejection message. Changing it is a breaking stability event.
const EXPECTED_MESSAGE: &str =
    "CXF export is not yet implemented; every model is rejected until the exporter lands.";

/// Unwrap the staged rejection: `export` must return `Err(CxfError::Validation(_))` carrying
/// exactly one diagnostic. Panics (failing the calling test) on `Ok` or any other error shape.
fn sole_rejection(model: &ModelGraph) -> Diagnostic {
    match export(model) {
        Err(CxfError::Validation(diags)) => {
            assert_eq!(
                diags.len(),
                1,
                "the export floor must emit exactly one diagnostic, got {diags:?}"
            );
            diags.into_iter().next().expect("length checked above")
        }
        Ok(bytes) => panic!(
            "the export floor must reject every model, but got Ok with {} byte(s)",
            bytes.len()
        ),
        Err(other) => panic!("expected CxfError::Validation, got {other:?}"),
    }
}

#[test]
fn resolved_import_is_rejected_with_a_single_export_unsupported_error() {
    let (graph, _report) =
        import_cxf(FIXTURE.as_bytes(), &ResolveOptions::default()).expect("minimal_loop resolves");
    let diag = sole_rejection(&graph);
    assert_eq!(diag.code, DiagCode::ExportUnsupported);
    assert_eq!(diag.severity, Severity::Error);
    assert_eq!(
        diag.subject, None,
        "a whole-operation deferral must not blame any node"
    );
    assert_eq!(diag.message, EXPECTED_MESSAGE);
}

#[test]
fn rejection_is_identical_across_repeated_calls() {
    let (graph, _report) =
        import_cxf(FIXTURE.as_bytes(), &ResolveOptions::default()).expect("minimal_loop resolves");
    let first = sole_rejection(&graph);
    for _ in 0..3 {
        let again = sole_rejection(&graph);
        // Diagnostic equality covers the full (code, subject, message) triple plus severity.
        assert_eq!(again, first, "the rejection must be stable across calls");
    }
}

#[test]
fn hand_built_empty_graph_gets_the_same_rejection_as_an_imported_one() {
    let (imported, _report) =
        import_cxf(FIXTURE.as_bytes(), &ResolveOptions::default()).expect("minimal_loop resolves");
    let from_import = sole_rejection(&imported);

    let from_empty = sole_rejection(&ModelGraph::new());
    assert_eq!(
        from_empty, from_import,
        "the floor is input-independent: every model gets the identical rejection"
    );
    assert_eq!(from_empty.code, DiagCode::ExportUnsupported);
    assert_eq!(from_empty.severity, Severity::Error);
    assert_eq!(from_empty.subject, None);
    assert_eq!(from_empty.message, EXPECTED_MESSAGE);
}
