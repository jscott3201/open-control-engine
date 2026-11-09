//! Pin-table data for the contract corpus's untagged boundary-interface machinery — the root
//! `hasOutput` / `hasInput` declarations that load but enter no point surface, and the three
//! double-driver shapes of a child input (§7.10 single assignment) — consumed by
//! `composite_contract_corpus.rs`'s assembled warned and rejected tables, which the guards there
//! compare against the disk as one table each.

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

/// The double-driver fixtures with their exact, complete refusal vectors: every shape of a child
/// input with two drivers — two block outputs, two root boundary inputs, one of each (issue #472)
/// — refuses with `single-assignment` at the driven input's authored IRI, plus the authoring
/// note's `econ_enable` worked example with the issue's extra `TOut -> hys.u` connection.
pub(crate) fn rejections() -> Vec<(&'static str, Vec<Diagnostic>)> {
    [
        (
            "rejected/double_driver_two_block_outputs.jsonld",
            "http://example.org#M.abs.u",
            "input is multiply driven (in-degree 2)",
        ),
        (
            "rejected/double_driver_two_boundary_inputs.jsonld",
            "http://example.org#M.abs.u",
            "input is driven by distinct boundary inputs",
        ),
        (
            "rejected/double_driver_block_output_and_boundary_input.jsonld",
            "http://example.org#M.abs.u",
            "input is driven by a block output and a boundary input",
        ),
        (
            "rejected/double_driver_econ_enable.jsonld",
            "http://example.org#econ_enable.hys.u",
            "input is driven by a block output and a boundary input",
        ),
    ]
    .into_iter()
    .map(|(fixture, subject, message)| {
        (
            fixture,
            vec![
                Diagnostic::error(DiagCode::SingleAssignment, message)
                    .with_subject(subject.to_owned()),
            ],
        )
    })
    .collect()
}
