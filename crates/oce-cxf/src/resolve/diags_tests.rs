//! Diagnostic ordering over shared and separate subject allocations.

use std::collections::HashMap;
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::ConnectorId;

use super::finalize_diags;

/// Diagnostics whose subjects are drawn from `subjects` in a fixed interleaving.
fn diagnostics(subjects: &[Arc<str>]) -> Vec<Diagnostic> {
    let pick = [0, 1, 0, 2, 0, 1, 3, 0, 2, 0];
    pick.iter()
        .enumerate()
        .map(|(index, &subject)| {
            let code = if index % 2 == 0 {
                DiagCode::TypeMismatch
            } else {
                DiagCode::DirectionMismatch
            };
            Diagnostic::error(code, format!("finding {}", index % 3))
                .with_subject(Arc::clone(&subjects[subject]))
        })
        .chain(std::iter::once(Diagnostic::error(
            DiagCode::MalformedDocument,
            "no subject",
        )))
        .collect()
}

/// One shared allocation per subject and a fresh allocation per diagnostic sort into the same
/// order: the allocation-keyed sort-key memo never decides a result (GitHub issue #480).
#[test]
fn shared_and_separate_subject_allocations_sort_identically() {
    let long = format!("urn:leaf:{}.y", "x".repeat(4096));
    let names = [long.as_str(), "urn:leaf:u", "connector#2", "urn:other"];
    let conn_of_iri: HashMap<&str, ConnectorId> = HashMap::from([
        (long.as_str(), ConnectorId(7)),
        ("urn:leaf:u", ConnectorId(1)),
    ]);
    let subjects: Vec<Arc<str>> = names.iter().map(|name| Arc::from(*name)).collect();
    let shared = finalize_diags(diagnostics(&subjects), &conn_of_iri);
    let separate: Vec<Diagnostic> = diagnostics(&subjects)
        .into_iter()
        .map(|mut diagnostic| {
            diagnostic.subject = diagnostic
                .subject
                .map(|subject| Arc::from(subject.to_string()));
            diagnostic
        })
        .collect();
    let separate = finalize_diags(separate, &conn_of_iri);
    assert_eq!(shared, separate);
    let order: Vec<Option<&str>> = shared
        .iter()
        .map(|diagnostic| diagnostic.subject.as_deref())
        .collect();
    let mut expected = vec![Some("urn:leaf:u"); 2];
    expected.push(Some("connector#2"));
    expected.push(Some("connector#2"));
    expected.extend([Some(long.as_str()); 5]);
    expected.push(None);
    expected.push(Some("urn:other"));
    assert_eq!(order, expected);
}
