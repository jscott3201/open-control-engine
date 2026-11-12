//! Unit tests for the canonical element-name spelling check.

use std::sync::Arc;

use oce_diag::DiagCode;
use oce_model::{BlockId, BlockInstance, ParamTable, Value};

use super::{check_element_name_spelling, is_canonical_index, is_signed_decimal};

fn block(class: &str, names: &[&str]) -> BlockInstance {
    BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from(class),
        inputs: Vec::new(),
        outputs: Vec::new(),
        params: ParamTable {
            values: names
                .iter()
                .map(|name| (Arc::from(*name), Value::Real(1.0)))
                .collect(),
        },
        decl_order: 0,
        instance_iri: Some(Arc::from("urn:test:b")),
    }
}

fn refused(class: &str, names: &[&str]) -> usize {
    let mut diags = Vec::new();
    check_element_name_spelling(&block(class, names), &mut diags);
    assert!(
        diags
            .iter()
            .all(|diag| diag.code == DiagCode::NonCanonicalElementName)
    );
    diags.len()
}

#[test]
fn index_classifiers_accept_exactly_their_spellings() {
    for text in ["1", "9", "10", "123", "+1", "-1", "01", "0", "007", "+0"] {
        assert!(is_signed_decimal(text), "{text}");
    }
    for text in ["", "+", "-", "1a", "a", "1.0", " 1", "١"] {
        assert!(!is_signed_decimal(text), "{text}");
    }
    for text in ["0", "1", "9", "10", "123"] {
        assert!(is_canonical_index(text), "{text}");
    }
    for text in ["", "01", "+1", "-1", "00", "+0", "-0", "1a"] {
        assert!(!is_canonical_index(text), "{text}");
    }
}

#[test]
fn every_aliasing_spelling_of_a_table_cell_or_offset_is_refused() {
    let class = "CDL.Reals.Sources.TimeTable";
    for name in [
        "table_01_2",
        "table_1_02",
        "table_+1_2",
        "table_1_+2",
        "table_-1_2",
        "table_00_1",
    ] {
        assert_eq!(
            refused(class, &["table_1_1", "table_1_2", name]),
            1,
            "{name}"
        );
        assert_eq!(refused(class, &[name]), 1, "{name} alone");
    }
    for name in ["offset_01", "offset_+1", "offset_00"] {
        assert_eq!(refused(class, &[name]), 1, "{name}");
    }
    assert_eq!(
        refused(class, &["table_1_1", "table_1_2", "table_10_2", "offset_1"]),
        0
    ); // A zero index is a canonical spelling; the table's own range rule refuses it.
    assert_eq!(refused(class, &["table_0_1", "offset_0"]), 0);
}

#[test]
fn width_driven_and_matrix_elements_follow_the_same_rule() {
    assert_eq!(refused("CDL.Reals.MultiSum", &["k_01"]), 1);
    assert_eq!(refused("CDL.Reals.MultiSum", &["k_+2"]), 1);
    assert_eq!(refused("CDL.Reals.MultiSum", &["k_1", "k_2", "nin"]), 0);
    assert_eq!(refused("CDL.Reals.MatrixGain", &["K_01_1"]), 1);
    assert_eq!(refused("CDL.Reals.MatrixGain", &["K_1_1", "K_2_1"]), 0);
}

#[test]
fn names_that_are_not_element_spellings_are_left_to_other_rules() {
    // Wrong rank, non-numeric suffixes and other bases are not element spellings of a template.
    let class = "CDL.Reals.Sources.TimeTable";
    assert_eq!(
        refused(class, &["table_1", "table_1_2_3", "table_a_1", "timeScale"]),
        0
    );
    assert_eq!(refused("CDL.Reals.MultiSum", &["k_1_1", "k", "kk_01"]), 0);
    // An unregistered class has no templates and is the loader's typed error, not this one.
    assert_eq!(refused("CDL.Unknown.Class", &["k_01"]), 0);
}
