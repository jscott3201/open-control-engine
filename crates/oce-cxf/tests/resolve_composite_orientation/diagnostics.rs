//! Diagnostic ownership across erased and surviving boundary relations.

use super::*;

/// A surviving edge owns the diagnostic for an unclassified missing endpoint shared with an
/// erased relation.
#[test]
fn preserved_unclassified_missing_target_reports_only_unresolved() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    let missing = format!("{model}.missing");
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    set_absolute_targets(
        &mut document,
        ".u",
        &[
            &format!("{model}.subA.gain.u"),
            &format!("{model}.subB.gain.u"),
        ],
    );
    set_absolute_targets(&mut document, ".subA.y", &[&missing]);
    set_absolute_targets(&mut document, ".subA.gain.y", &[&missing]);
    let diagnostics = import(&document).expect_err("missing endpoint must reject once");
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(DiagCode::UnresolvedReference, "connection target not found",)
                .with_subject(missing)
        ]
    );
}

/// A productive target boundary leaves direction refusal to the expanded flat edge.
#[test]
fn target_side_contradiction_reports_only_flat_direction_error() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    set_absolute_targets(&mut document, ".subA.gain.u", &[&format!("{model}.subB.u")]);
    let diagnostics = import(&document).expect_err("input-to-input edge must reject once");
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(
                DiagCode::DirectionMismatch,
                "connection is not output→input",
            )
            .with_subject(format!("{model}.u"))
        ]
    );
}
