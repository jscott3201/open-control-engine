//! The boundary comparison behind `mutant-boundary-differs`, over constructed boundaries: which
//! port is named first, for a path only one side has and for a path with two types.

use oce_api::ValueType;

use super::first_difference;

fn ports(list: &[(&str, ValueType)]) -> Vec<(String, ValueType)> {
    list.iter()
        .map(|(path, value_type)| ((*path).to_owned(), *value_type))
        .collect()
}

#[test]
fn equal_boundaries_have_no_difference() {
    let subject = ports(&[("a.u", ValueType::Real), ("a.v", ValueType::Boolean)]);
    assert_eq!(first_difference("input", &subject, &subject), None);
    assert_eq!(first_difference("input", &[], &[]), None);
}

#[test]
fn a_path_only_the_subject_has_is_named() {
    let subject = ports(&[("a.u", ValueType::Real), ("a.v", ValueType::Real)]);
    let mutant = ports(&[("a.u", ValueType::Real)]);
    let (path, detail) = first_difference("input", &subject, &mutant).expect("a difference");
    assert_eq!(path, "a.v");
    assert!(detail.contains("subject's input a.v"), "{detail}");
    // A renamed port: the subject's name sorts first, so it is the one named.
    let renamed = ports(&[("a.u", ValueType::Real), ("a.w", ValueType::Real)]);
    let (path, _) = first_difference("input", &subject, &renamed).expect("a difference");
    assert_eq!(path, "a.v");
}

#[test]
fn a_path_only_the_mutant_has_is_named() {
    let subject = ports(&[("a.u", ValueType::Real)]);
    let mutant = ports(&[("a.u", ValueType::Real), ("a.z", ValueType::Integer)]);
    let (path, detail) = first_difference("output", &subject, &mutant).expect("a difference");
    assert_eq!(path, "a.z");
    assert!(detail.contains("mutant's output a.z"), "{detail}");
    // A renamed port whose new name sorts first is named by its new name.
    let renamed = ports(&[("a.a", ValueType::Real)]);
    let (path, _) = first_difference("output", &subject, &renamed).expect("a difference");
    assert_eq!(path, "a.a");
}

#[test]
fn one_path_with_two_types_is_named_with_both() {
    let subject = ports(&[("a.y", ValueType::Boolean)]);
    let mutant = ports(&[("a.y", ValueType::Real)]);
    let (path, detail) = first_difference("output", &subject, &mutant).expect("a difference");
    assert_eq!(path, "a.y");
    assert!(
        detail.contains("a Boolean in the subject and a Real in the mutant"),
        "{detail}"
    );
}
