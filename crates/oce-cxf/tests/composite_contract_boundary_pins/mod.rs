//! Pin-table data for the contract corpus's untagged boundary-interface advisories — the root
//! `hasOutput` / `hasInput` declarations that load but enter no point surface — consumed by
//! `composite_contract_corpus.rs`'s assembled warned table, which the guards there compare
//! against the disk as one table.

use oce_diag::{DiagCode, Diagnostic};

/// The warned boundary-interface fixtures with their exact, complete warning vectors: an
/// undriven declared output and an unread declared input, each its declaration's only
/// representation.
pub(crate) fn warnings() -> Vec<(&'static str, Vec<Diagnostic>)> {
    [
        (
            "warned/undriven_boundary_output.jsonld",
            DiagCode::UndrivenBoundaryOutput,
            "http://example.org#M.y",
            "declared boundary output has no internal driver",
        ),
        (
            "warned/unused_boundary_input.jsonld",
            DiagCode::UnusedBoundaryInput,
            "http://example.org#M.v",
            "declared boundary input is read by no connection",
        ),
    ]
    .into_iter()
    .map(|(fixture, code, subject, message)| {
        (
            fixture,
            vec![Diagnostic::warning(code, message).with_subject(subject.to_owned())],
        )
    })
    .collect()
}
