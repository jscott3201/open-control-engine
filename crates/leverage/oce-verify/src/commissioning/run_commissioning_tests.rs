use super::*;

#[test]
fn a_source_refusal_keeps_the_readers_word_as_its_cause_and_names_the_file() {
    let refused = source_refused(
        "references[0]",
        &SourceError::new(SourceErrorKind::BadCell, "no number").in_file("a.csv"),
    );
    assert_eq!(refused.code(), "source-refused");
    assert_eq!(refused.cause(), Some("bad-cell"));
    assert_eq!(refused.subject(), "a.csv");
    let refused = source_refused(
        "references[0]",
        &SourceError::new(SourceErrorKind::TooManySources, "257"),
    );
    assert_eq!(
        (refused.code(), refused.cause()),
        ("too-many-sources", None)
    );
    assert_eq!(refused.subject(), "references[0]");
    let refused = source_refused(
        "references[0]",
        &SourceError::new(SourceErrorKind::ReferenceOutOfRange, "none"),
    );
    assert_eq!(refused.code(), "reference-out-of-range");
}
