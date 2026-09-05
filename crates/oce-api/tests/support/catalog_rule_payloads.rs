//! Each rule payload field independently participates in the catalog contract.

use oce_api::{CatalogPortKind, CatalogRule, CatalogValueKind, catalog, catalog_content_id};

fn distinguish(original: &CatalogRule, changed: CatalogRule) {
    let mut entry = catalog()[0].clone();
    entry.param_rules = vec![original.clone()];
    let original_id = catalog_content_id(&[entry.clone()]);
    entry.param_rules = vec![changed];
    assert_ne!(
        original_id,
        catalog_content_id(&[entry]),
        "lost payload in {original:?}"
    );
}

macro_rules! payloads {
    ($variant:ident { $($field:ident : $first:expr => $second:expr),* $(,)? }) => {{
        let original = CatalogRule::$variant { $($field: $first),* };
        $(
            let mut changed = original.clone();
            if let CatalogRule::$variant { $field, .. } = &mut changed { *$field = $second; }
            distinguish(&original, changed);
        )*
    }};
}

#[test]
fn every_rule_payload_field_changes_the_catalog_tag() {
    payloads!(Required {
        name: "before" => "after",
        kind: CatalogValueKind::Real => CatalogValueKind::Enum { class_path: "CDL.Types.SimpleController", members: &["P", "PI", "PD", "PID"] },
    });
    payloads!(Structural {
        name: "before" => "after",
    });
    payloads!(StructuralArrayElements {
        base: "before" => "after",
    });
    payloads!(Boolean {
        name: "before" => "after",
    });
    payloads!(Real {
        name: "before" => "after",
    });
    payloads!(RealFinite {
        name: "before" => "after",
    });
    payloads!(RealGreaterThan {
        name: "before" => "after",
        min: 0.0 => -0.0,
    });
    payloads!(RealFiniteGreaterThan {
        name: "before" => "after",
        min: 0.0 => -0.0,
    });
    payloads!(IntegerGreaterOrEqual {
        name: "before" => "after",
        min: i64::MIN => i64::MAX,
    });
    payloads!(IntegerLessOrEqualConstant {
        name: "before" => "after",
        max: i64::MIN => i64::MAX,
    });
    payloads!(IntegerArrayElements {
        base: "before" => "after",
        len: "before" => "after",
    });
    payloads!(IntegerArrayElementsInRange {
        base: "before" => "after",
        len: "before" => "after",
        len_default: i64::MIN => i64::MAX,
        min: i64::MIN => i64::MAX,
        max: "before" => "after",
        max_default: i64::MIN => i64::MAX,
        default_to_index: false => true,
    });
    payloads!(RealArrayElements {
        base: "before" => "after",
        len: "before" => "after",
    });
    payloads!(RealMatrixElements {
        base: "before" => "after",
        rows: "before" => "after",
        default_rows: i64::MIN => i64::MAX,
        cols: "before" => "after",
        default_cols: i64::MIN => i64::MAX,
    });
    payloads!(TimeTableMatrix {
        base: "before" => "after",
        values: CatalogPortKind::Real => CatalogPortKind::Boolean,
        time_scale: "before" => "after",
        period: None => Some("after"),
        extrapolation: None => Some("after"),
    });
    payloads!(TimeTableOffset {
        base: "before" => "after",
        table: "before" => "after",
    });
    payloads!(BooleanArrayElements {
        base: "before" => "after",
        len: "before" => "after",
    });
    payloads!(BooleanArrayTrueCountEquals {
        base: "before" => "after",
        len: "before" => "after",
        count: "before" => "after",
        default: false => true,
    });
    payloads!(EnumMembers {
        name: "before" => "after",
        members: &["A", "B"] => &["B", "A"],
    });
    payloads!(RealGreaterOrEqual {
        name: "before" => "after",
        min: 0.0 => -0.0,
    });
    payloads!(RealLessOrEqualConstant {
        name: "before" => "after",
        max: 0.0 => -0.0,
    });
    payloads!(RealTimesIntegerInclusiveRange {
        real: "before" => "after",
        integer: "before" => "after",
        min: 0.0 => -0.0,
        max: 0.0 => -0.0,
    });
    payloads!(IntegerProductLessOrEqualConstant {
        left: "before" => "after",
        right: "before" => "after",
        max: i64::MIN => i64::MAX,
    });
    payloads!(RealLessOrEqual {
        lower: "before" => "after",
        upper: "before" => "after",
    });
    payloads!(RealLessOrEqualWarning {
        lower: "before" => "after",
        upper: "before" => "after",
    });
    payloads!(RealGreaterOrEqualScaledWarning {
        left: "before" => "after",
        right: "before" => "after",
        factor: 0.0 => -0.0,
    });
    payloads!(RealEqualWarning {
        left: "before" => "after",
        right: "before" => "after",
    });
}
