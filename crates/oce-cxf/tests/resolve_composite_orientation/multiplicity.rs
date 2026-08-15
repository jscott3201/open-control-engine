//! Assignment cardinality after boundary elision.

use super::*;

/// Same-direction copies through one boundary input remain separate assignments.
#[test]
fn repeated_boundary_input_target_is_multiply_driven() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    set_absolute_targets(
        &mut document,
        ".u",
        &[&format!("{model}.subA.u"), &format!("{model}.subB.gain.u")],
    );
    let target = format!("{model}.subA.gain.u");
    set_absolute_targets(&mut document, ".subA.u", &[&target, &target]);
    let diagnostics = import(&document).expect_err("repeated assignment must reject");
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(
                DiagCode::SingleAssignment,
                "input is multiply driven (in-degree 2)",
            )
            .with_subject(target)
        ]
    );
}

/// Assertions from both endpoints describe one root-boundary assignment.
#[test]
fn opposite_endpoint_spellings_collapse_once() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    let target = format!("{model}.subA.gain.u");
    set_absolute_targets(
        &mut document,
        ".u",
        &[&target, &format!("{model}.subB.gain.u")],
    );
    set_absolute_targets(&mut document, ".subA.gain.u", &[&format!("{model}.u")]);
    let graph = import(&document).expect("opposite spellings describe one assignment");
    assert_eq!(graph.external_inputs.len(), 2);
}

/// Ordinary leaf restatements remain duplicate assignments rather than boundary closure.
#[test]
fn opposite_leaf_spellings_remain_multiply_driven() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    let source = format!("{model}.subA.gain.y");
    let target = format!("{model}.subB.gain.u");
    set_absolute_targets(&mut document, ".subA.gain.y", &[&target]);
    set_absolute_targets(&mut document, ".subB.gain.u", &[&source]);
    let diagnostics = import(&document).expect_err("ordinary restatement must reject");
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(
                DiagCode::SingleAssignment,
                "input is multiply driven (in-degree 2)",
            )
            .with_subject(target)
        ]
    );
}

/// A direct reverse relation stays distinct from a boundary path with the same flat endpoints.
#[test]
fn direct_relation_does_not_collapse_expanded_boundary_path() {
    let mut document = sibling_document();
    let model = "http://example.org#siblings";
    node_mut(&mut document, "#siblings")
        .as_object_mut()
        .expect("root node")
        .remove("S231:hasOutput");
    let target = format!("{model}.subA.gain.u");
    set_absolute_targets(
        &mut document,
        ".u",
        &[&format!("{model}.subA.u"), &format!("{model}.subB.gain.u")],
    );
    set_absolute_targets(&mut document, ".subA.gain.u", &[&format!("{model}.u")]);
    let diagnostics = import(&document).expect_err("distinct boundary paths must remain visible");
    assert_eq!(
        diagnostics,
        vec![
            Diagnostic::error(
                DiagCode::SingleAssignment,
                "input is multiply driven (in-degree 2)",
            )
            .with_subject(target)
        ]
    );
}
