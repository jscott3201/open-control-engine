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
