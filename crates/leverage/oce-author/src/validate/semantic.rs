//! Syntax of a boundary's `SemanticPoint` slot and its `quantity` / `display_unit` attributes
//! (decisions 2 and 21). Syntax only: no vocabulary is consulted before M8 and vocabulary v1.

use crate::model::Boundary;
use crate::wire::MAX_WIRE_LIST_LEN;

/// The longest semantic field, in bytes (the text form's token bound).
pub const MAX_SEMANTIC_FIELD_BYTES: usize = 256;

/// An atom: non-empty printable ASCII with no space or `#`, at most
/// [`MAX_SEMANTIC_FIELD_BYTES`] bytes (the text form's `atom`).
fn atom_problem(value: &str) -> Option<String> {
    if value.is_empty() {
        return Some("is empty; an absent field is left out, not empty".to_owned());
    }
    if value.len() > MAX_SEMANTIC_FIELD_BYTES {
        return Some(format!(
            "is {} bytes; at most {MAX_SEMANTIC_FIELD_BYTES} are allowed",
            value.len()
        ));
    }
    value
        .bytes()
        .position(|b| !(b'!'..=b'~').contains(&b) || b == b'#')
        .map(|at| format!("byte {at} is not printable ASCII without space or `#`"))
}

/// A free-text field (`name`, `label`): non-empty, at most [`MAX_SEMANTIC_FIELD_BYTES`] bytes,
/// no control characters.
fn text_problem(value: &str) -> Option<String> {
    if value.is_empty() {
        return Some("is empty; an absent field is left out, not empty".to_owned());
    }
    if value.len() > MAX_SEMANTIC_FIELD_BYTES {
        return Some(format!(
            "is {} bytes; at most {MAX_SEMANTIC_FIELD_BYTES} are allowed",
            value.len()
        ));
    }
    value
        .chars()
        .any(char::is_control)
        .then(|| "carries a control character".to_owned())
}

/// A field check: the problem with a present value, if any.
type FieldCheck = fn(&str) -> Option<String>;

/// Every syntax problem of one boundary's semantic slot, `quantity` and `display_unit`, as
/// `field: problem` details, in field order.
pub(crate) fn boundary_problems(boundary: &Boundary) -> Vec<String> {
    let s = &boundary.semantic;
    let fields: [(&str, Option<&String>, FieldCheck); 8] = [
        ("quantity", boundary.quantity.as_ref(), atom_problem),
        ("display_unit", boundary.display_unit.as_ref(), atom_problem),
        ("name", s.name.as_ref(), text_problem),
        ("label", s.label.as_ref(), text_problem),
        ("unit", s.unit.as_ref(), atom_problem),
        ("brick", s.brick.as_ref(), atom_problem),
        ("s223", s.s223.as_ref(), atom_problem),
        ("qudt", s.qudt.as_ref(), atom_problem),
    ];
    let mut problems: Vec<String> = fields
        .into_iter()
        .filter_map(|(field, value, check)| {
            value
                .and_then(|v| check(v))
                .map(|problem| format!("`{field}` {problem}"))
        })
        .collect();
    if s.haystack.len() > MAX_WIRE_LIST_LEN {
        problems.push(format!(
            "`haystack` has {} tags; the wire form carries at most {MAX_WIRE_LIST_LEN}",
            s.haystack.len()
        ));
    }
    for (index, tag) in s.haystack.iter().enumerate() {
        if let Some(problem) = atom_problem(tag) {
            problems.push(format!("`haystack[{index}]` {problem}"));
        }
    }
    problems
}
