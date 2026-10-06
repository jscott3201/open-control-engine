//! Canonical catalog JSON, with lossless binary64 fields and lexical object-key order.

use crate::{CatalogRule, CatalogValueKind};
use serde_json::{Value, json};

pub(super) fn rule_json(rule: &CatalogRule) -> Value {
    match rule {
        CatalogRule::Required { name, kind } => {
            json!({"rule": "Required", "name": name, "kind": value_kind_json(kind)})
        }
        CatalogRule::Structural { name } => json!({"rule": "Structural", "name": name}),
        CatalogRule::StructuralArrayElements { base } => {
            json!({"rule": "StructuralArrayElements", "base": base})
        }
        CatalogRule::Boolean { name } => json!({"rule": "Boolean", "name": name}),
        CatalogRule::Real { name } => json!({"rule": "Real", "name": name}),
        CatalogRule::RealFinite { name } => json!({"rule": "RealFinite", "name": name}),
        CatalogRule::RealGreaterThan { name, min } => {
            json!({"rule": "RealGreaterThan", "name": name, "min": format!("{:016x}", min.to_bits())})
        }
        CatalogRule::RealFiniteGreaterThan { name, min } => {
            json!({"rule": "RealFiniteGreaterThan", "name": name, "min": format!("{:016x}", min.to_bits())})
        }
        CatalogRule::IntegerGreaterOrEqual { name, min } => {
            json!({"rule": "IntegerGreaterOrEqual", "name": name, "min": min})
        }
        CatalogRule::IntegerLessOrEqualConstant { name, max } => {
            json!({"rule": "IntegerLessOrEqualConstant", "name": name, "max": max})
        }
        CatalogRule::IntegerArrayElements { base, len } => {
            json!({"rule": "IntegerArrayElements", "base": base, "len": len})
        }
        CatalogRule::IntegerArrayElementsInRange {
            base,
            len,
            len_default,
            min,
            max,
            max_default,
            default_to_index,
        } => {
            json!({"rule": "IntegerArrayElementsInRange", "base": base, "len": len, "len_default": len_default, "min": min, "max": max, "max_default": max_default, "default_to_index": default_to_index})
        }
        CatalogRule::RealArrayElements { base, len } => {
            json!({"rule": "RealArrayElements", "base": base, "len": len})
        }
        CatalogRule::RealMatrixElements {
            base,
            rows,
            default_rows,
            cols,
            default_cols,
        } => {
            json!({"rule": "RealMatrixElements", "base": base, "rows": rows, "default_rows": default_rows, "cols": cols, "default_cols": default_cols})
        }
        CatalogRule::TimeTableMatrix {
            base,
            values,
            time_scale,
            period,
            extrapolation,
        } => {
            json!({"rule": "TimeTableMatrix", "base": base, "values": values.label(), "time_scale": time_scale, "period": period, "extrapolation": extrapolation})
        }
        CatalogRule::TimeTableOffset { base, table } => {
            json!({"rule": "TimeTableOffset", "base": base, "table": table})
        }
        CatalogRule::BooleanArrayElements { base, len } => {
            json!({"rule": "BooleanArrayElements", "base": base, "len": len})
        }
        CatalogRule::BooleanArrayTrueCountEquals {
            base,
            len,
            count,
            default,
        } => {
            json!({"rule": "BooleanArrayTrueCountEquals", "base": base, "len": len, "count": count, "default": default})
        }
        CatalogRule::EnumMembers { name, members } => {
            json!({"rule": "EnumMembers", "name": name, "members": members})
        }
        CatalogRule::RealGreaterOrEqual { name, min } => {
            json!({"rule": "RealGreaterOrEqual", "name": name, "min": format!("{:016x}", min.to_bits())})
        }
        CatalogRule::RealLessOrEqualConstant { name, max } => {
            json!({"rule": "RealLessOrEqualConstant", "name": name, "max": format!("{:016x}", max.to_bits())})
        }
        CatalogRule::RealTimesIntegerInclusiveRange {
            real,
            integer,
            min,
            max,
        } => {
            json!({"rule": "RealTimesIntegerInclusiveRange", "real": real, "integer": integer, "min": format!("{:016x}", min.to_bits()), "max": format!("{:016x}", max.to_bits())})
        }
        CatalogRule::IntegerProductLessOrEqualConstant { left, right, max } => {
            json!({"rule": "IntegerProductLessOrEqualConstant", "left": left, "right": right, "max": max})
        }
        CatalogRule::RealLessOrEqual { lower, upper } => {
            json!({"rule": "RealLessOrEqual", "lower": lower, "upper": upper})
        }
        CatalogRule::RealLessOrEqualWarning { lower, upper } => {
            json!({"rule": "RealLessOrEqualWarning", "lower": lower, "upper": upper})
        }
        CatalogRule::RealGreaterOrEqualScaledWarning {
            left,
            right,
            factor,
        } => {
            json!({"rule": "RealGreaterOrEqualScaledWarning", "left": left, "right": right, "factor": format!("{:016x}", factor.to_bits())})
        }
        CatalogRule::RealEqualWarning { left, right } => {
            json!({"rule": "RealEqualWarning", "left": left, "right": right})
        }
    }
}

pub(super) fn value_kind_json(kind: &CatalogValueKind) -> Value {
    match kind {
        CatalogValueKind::Real => json!({"kind": "Real"}),
        CatalogValueKind::Integer => json!({"kind": "Integer"}),
        CatalogValueKind::Boolean => json!({"kind": "Boolean"}),
        CatalogValueKind::String => json!({"kind": "String"}),
        CatalogValueKind::Enum {
            class_path,
            members,
        } => json!({"kind": "Enum", "class_path": class_path, "members": members}),
    }
}
