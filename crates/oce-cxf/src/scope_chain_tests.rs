//! Name-resolution and sharing properties of the indexed scope chain (`scope_chain.rs`).
//!
//! The view tests are ports of the slice-scan `ParamScope` tests the chain replaced; each also
//! cross-checks the indexed lookup against an independent reverse scan of the same bindings laid
//! out as one flat slice (the pre-#365 representation), so a resolution-order drift fails here.

use std::sync::Arc;

use oce_expr::{EvalResult, Scope};
use oce_model::Value;

use crate::dto::CxfValue;
use crate::ground::ground_value;
use crate::scope_chain::{IndexedEntries, ParamScope, ScopeFrame};

fn int(v: i64) -> EvalResult {
    EvalResult::Scalar(Value::Integer(v))
}

fn entries(bindings: &[(&str, i64)]) -> IndexedEntries {
    bindings
        .iter()
        .map(|&(name, value)| (Arc::from(name), int(value)))
        .collect()
}

fn read(scope: &dyn Scope, name: &str) -> Option<i64> {
    match scope.lookup(name) {
        Some(EvalResult::Scalar(Value::Integer(v))) => Some(*v),
        Some(other) => panic!("unexpected binding {other:?}"),
        None => None,
    }
}

/// The pre-#365 slice scan: `enclosing` then `siblings` laid out flat; the split view scans the
/// enclosing region (reverse) then the siblings (reverse), the undivided view the whole slice
/// in reverse.
fn reference_scan(
    enclosing: &[(&str, i64)],
    siblings: &[(&str, i64)],
    name: &str,
    split: bool,
) -> Option<i64> {
    let find = |region: &[(&str, i64)]| {
        region
            .iter()
            .rev()
            .find(|(n, _)| *n == name)
            .map(|&(_, v)| v)
    };
    if split {
        find(enclosing).or_else(|| find(siblings))
    } else {
        find(siblings).or_else(|| find(enclosing))
    }
}

#[test]
fn split_view_reads_enclosing_binding_over_same_named_sibling() {
    // Issue #239: on the split view the enclosing region wins on a name bound in both regions,
    // while a sibling-only name keeps resolving against the sibling region.
    let root = ScopeFrame::root();
    let enclosing = ScopeFrame::extend(&root, entries(&[("a", 10)]));
    let siblings = entries(&[("a", 99), ("b", 7)]);
    let split = ParamScope::with_enclosing(&enclosing, &siblings);
    let both = ground_value(&CxfValue::Expr("a".to_owned()), &split).unwrap();
    assert!(both.bit_eq(&Value::Integer(10)), "got {both:?}");
    let sibling_only = ground_value(&CxfValue::Expr("b".to_owned()), &split).unwrap();
    assert!(
        sibling_only.bit_eq(&Value::Integer(7)),
        "got {sibling_only:?}"
    );
}

#[test]
fn innermost_enclosing_frame_wins_over_outer_frames() {
    // Two enclosing composites binding one name: the innermost (last-chained) frame wins, and a
    // name bound only by the outer frame still resolves through the chain.
    let root = ScopeFrame::root();
    let outer = ScopeFrame::extend(&root, entries(&[("a", 1), ("outer_only", 5)]));
    let inner = ScopeFrame::extend(&outer, entries(&[("a", 2)]));
    let siblings = entries(&[("a", 99)]);
    let split = ParamScope::with_enclosing(&inner, &siblings);
    assert_eq!(read(&split, "a"), Some(2));
    assert_eq!(read(&split, "outer_only"), Some(5));
    assert_eq!(read(&*outer, "a"), Some(1), "the outer frame is unchanged");
}

#[test]
fn undivided_view_keeps_latest_wins_on_a_duplicate_name() {
    let root = ScopeFrame::root();
    let enclosing = ScopeFrame::extend(&root, entries(&[("a", 10)]));
    let siblings = entries(&[("a", 99)]);
    assert_eq!(
        read(&ParamScope::undivided(&enclosing, &siblings), "a"),
        Some(99)
    );
    // Within one region a re-pushed name is latest-wins on both views.
    let repeated = entries(&[("b", 1), ("b", 2)]);
    assert_eq!(
        read(&ParamScope::with_enclosing(&root, &repeated), "b"),
        Some(2)
    );
    assert_eq!(read(&ParamScope::undivided(&root, &repeated), "b"), Some(2));
}

#[test]
fn indexed_views_agree_with_the_flat_reverse_scan_on_every_name() {
    let outer_bindings = [("a", 1), ("b", 2), ("c", 3), ("a", 4)];
    let inner_bindings = [("b", 20), ("d", 40)];
    let sibling_bindings = [("c", 300), ("e", 500), ("c", 301), ("a", 100)];
    let root = ScopeFrame::root();
    let outer = ScopeFrame::extend(&root, entries(&outer_bindings));
    let inner = ScopeFrame::extend(&outer, entries(&inner_bindings));
    let siblings = entries(&sibling_bindings);
    let flat_enclosing: Vec<(&str, i64)> = outer_bindings
        .iter()
        .chain(inner_bindings.iter())
        .copied()
        .collect();
    for name in ["a", "b", "c", "d", "e", "missing"] {
        assert_eq!(
            read(&ParamScope::with_enclosing(&inner, &siblings), name),
            reference_scan(&flat_enclosing, &sibling_bindings, name, true),
            "split view, name {name}"
        );
        assert_eq!(
            read(&ParamScope::undivided(&inner, &siblings), name),
            reference_scan(&flat_enclosing, &sibling_bindings, name, false),
            "undivided view, name {name}"
        );
    }
}

#[test]
fn empty_extension_reuses_the_parent_frame_and_empty_scope_binds_nothing() {
    let root = ScopeFrame::root();
    let parent = ScopeFrame::extend(&root, entries(&[("a", 1)]));
    let same = ScopeFrame::extend(&parent, IndexedEntries::default());
    assert!(
        Arc::ptr_eq(&parent, &same),
        "a binding-free composite adds no frame (chain depth stays bounded by bindings)"
    );
    assert_eq!(read(&ParamScope::empty(), "a"), None);
    assert_eq!(read(&*root, "a"), None);
}

#[test]
fn many_leaves_share_one_frame_without_copying_its_bindings() {
    // The P×L shape of issue #365: one composite frame of 4096 bindings referenced by 4096
    // leaves. Sharing is by reference count, so the frame's strong count — not a byte count —
    // is the deterministic evidence that no leaf holds a copy.
    let root = ScopeFrame::root();
    let bindings: IndexedEntries = (0..4096)
        .map(|i| (Arc::from(format!("p{i}").as_str()), int(i)))
        .collect();
    let frame = ScopeFrame::extend(&root, bindings);
    let leaves: Vec<Arc<ScopeFrame>> = (0..4096).map(|_| Arc::clone(&frame)).collect();
    assert_eq!(Arc::strong_count(&frame), 4097);
    let siblings = IndexedEntries::default();
    assert_eq!(
        read(
            &ParamScope::with_enclosing(&leaves[4095], &siblings),
            "p4095"
        ),
        Some(4095)
    );
}
