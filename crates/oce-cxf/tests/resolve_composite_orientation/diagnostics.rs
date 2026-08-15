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

/// A target-side relation with an invalid source is diagnosed after productive expansion.
#[test]
fn productive_target_side_unknown_reports_only_unresolved_source() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
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
    set_absolute_targets(&mut document, ".subA.gain", &[&format!("{model}.subB.u")]);
    let diagnostics = import(&document).expect_err("non-connector source must reject once");
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(DiagCode::UnresolvedReference, "connection source not found",)
                .with_subject(format!("{model}.subA.gain"))
        ]
    );
}

/// An empty target boundary has no later validation step to own an underivable relation.
#[test]
fn empty_target_side_unknown_keeps_deferred_direction_error() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
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
    clear_targets(&mut document, ".subB.u");
    set_absolute_targets(&mut document, ".subA.gain", &[&format!("{model}.subB.u")]);
    let diagnostics = import(&document).expect_err("erased unknown relation must reject");
    assert_eq!(
        diagnostics,
        vec![Diagnostic::error(
            DiagCode::DirectionMismatch,
            "boundary connection direction cannot be derived",
        )]
    );
}

/// A missing terminal retained by boundary expansion owns the unresolved-reference diagnostic.
#[test]
fn reached_missing_target_reports_only_unresolved() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    let missing = format!("{model}.subB.y");
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
    clear_targets(&mut document, ".subB.gain.y");
    set_absolute_targets(&mut document, ".subA.y", &[&missing]);
    document["@graph"]
        .as_array_mut()
        .expect("@graph")
        .retain(|node| node["@id"].as_str() != Some(missing.as_str()));
    let diagnostics = import(&document).expect_err("missing expanded target must reject once");
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(DiagCode::UnresolvedReference, "connection target not found",)
                .with_subject(missing)
        ]
    );
}
