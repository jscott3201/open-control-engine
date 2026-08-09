//! Nested compact-IRI context-binding refusal and its absolute-IRI control.

use super::*;

#[test]
fn nested_compact_context_bindings_are_refused_before_identity_expansion() {
    for context in [
        Context::Map(
            [
                ("alias".to_owned(), serde_json::json!("ex:A")),
                ("ex".to_owned(), serde_json::json!("http://example.org#")),
            ]
            .into_iter()
            .collect(),
        ),
        Context::List(vec![
            serde_json::json!({ "ex": "http://example.org#" }),
            serde_json::json!({ "alias": "ex:A" }),
        ]),
    ] {
        let mut diags = Vec::new();
        prefix_table(&context, &mut diags);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct);
        assert_eq!(diags[0].subject.as_deref(), Some("alias"));
        assert_eq!(
            diags[0].message,
            "@context term `alias` binds the nested compact IRI `ex:A` through prefix `ex`; \
             nested compact-IRI term definitions are not supported — bind the expanded absolute \
             IRI instead"
        );
    }
}

#[test]
fn declared_scheme_does_not_capture_double_slash_context_values() {
    let context = Context::Map(
        [
            ("http".to_owned(), serde_json::json!("http://trap.example/")),
            (
                "alias".to_owned(),
                serde_json::json!("http://example.org#A"),
            ),
        ]
        .into_iter()
        .collect(),
    );
    let mut diags = Vec::new();
    let table = prefix_table(&context, &mut diags);
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(table["alias"], "http://example.org#A");
}

#[test]
fn large_flat_context_preserves_every_binding() {
    let context = Context::Map(
        (0..50_000)
            .map(|index| (format!("p{index:05}"), serde_json::json!("urn:x")))
            .collect(),
    );
    let mut diags = Vec::new();
    let table = prefix_table(&context, &mut diags);
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(table.len(), 50_000);
    assert_eq!(table["p49999"], "urn:x");
}
