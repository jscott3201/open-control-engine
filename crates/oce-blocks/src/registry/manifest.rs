//! Deterministic generator for the machine-readable block-registry manifest.
//!
//! The checked-in artifact `tools/reference-catalog/oce-blocks.registry-manifest.json` is the
//! published, machine-readable inventory of the native block registry: one JSON object per
//! [`RegistryEntry`], in [`CATALOG`] slice order, carrying the class path, the typed positional
//! ports of the `ParamTable::default()`-resolved instance, a width-driven flag, and the complete
//! class-level parameter-rule list with every embedded guard field. `manifest_tests.rs`
//! regenerates the manifest and compares it byte-for-byte against the checked-in artifact, so any
//! registry change that is not consciously re-blessed fails the test suite.
//!
//! Drift-guard mechanism: [`param_rule_json`] matches [`ParamRule`] exhaustively with **no
//! wildcard arm**, so adding a rule variant fails compilation here until the serializer — and
//! therefore the published schema — is extended. This module must stay inside `oce-blocks`:
//! `ParamRule` is `#[non_exhaustive]`, so a cross-crate match would be forced to add a wildcard
//! arm and the compile-forced guard would silently vanish.
//!
//! Determinism contract: entry order is `CATALOG` slice order then per-slice declaration order;
//! rule order is each rule slice's declaration order; object fields are written in Rust field
//! declaration order (never map iteration); every `f64` guard field serializes through
//! `serde_json` (Ryu shortest round-trip), never `format!`/`Debug`. Repeated generation is
//! byte-identical, and the checked-in artifact is the byte golden.

use crate::{
    CatalogEntry, DefaultLiteral, DefaultSource, ParamRule, PortKind, PortNaming, TimeTableValues,
    catalog,
};

/// Filesystem path of the checked-in manifest artifact, used by `UPDATE_EXPECT` re-blessing.
pub(super) const MANIFEST_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/reference-catalog/oce-blocks.registry-manifest.json"
);

/// Checked-in manifest bytes — the byte golden the regenerate-and-diff test compares against.
pub(super) const MANIFEST_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/reference-catalog/oce-blocks.registry-manifest.json"
));

/// Render the manifest for the live registry as a top-level ordered JSON array with a trailing
/// newline. Panics only on a registry defect the tests exist to catch (a constructor panic at
/// default parameters or a non-finite `f64` guard field).
pub(super) fn render_manifest() -> String {
    render_entries(catalog())
}

pub(crate) fn render_entries(entries: &[CatalogEntry]) -> String {
    let mut out = String::from("[\n");
    let mut first = true;
    for entry in entries {
        if !first {
            out.push_str(",\n");
        }
        first = false;
        push_entry(&mut out, entry);
    }
    out.push_str("\n]\n");
    out
}

/// Append one manifest entry object. Ports come from `resolved_signature()` on the
/// `ParamTable::default()`-constructed instance — NOT the static class `signature()` — so
/// variadic blocks record their true default arity (empty for `nin`-defaulted vectors).
fn push_entry(out: &mut String, entry: &CatalogEntry) {
    let input_kinds: Vec<_> = entry.inputs.iter().map(|port| port.kind).collect();
    let output_kinds: Vec<_> = entry.outputs.iter().map(|port| port.kind).collect();
    out.push_str("  {\n    \"class_path\": ");
    out.push_str(&json_str(entry.class_path));
    out.push_str(",\n    \"inputs\": ");
    out.push_str(&port_kind_list(&input_kinds));
    out.push_str(",\n    \"outputs\": ");
    out.push_str(&port_kind_list(&output_kinds));
    out.push_str(",\n    \"width_driven\": ");
    out.push_str(if entry.width_driven { "true" } else { "false" });
    if entry.param_rules.is_empty() {
        out.push_str(",\n    \"param_rules\": []");
    } else {
        out.push_str(",\n    \"param_rules\": [\n");
        for (index, rule) in entry.param_rules.iter().enumerate() {
            out.push_str("      ");
            out.push_str(&param_rule_json(rule));
            out.push_str(if index + 1 == entry.param_rules.len() {
                "\n"
            } else {
                ",\n"
            });
        }
        out.push_str("    ]");
    }
    out.push_str(",\n    \"port_naming\": ");
    out.push_str(&json_str(match entry.naming {
        PortNaming::Named => "named",
        PortNaming::Positional => "positional",
        PortNaming::WidthDriven => "width_driven",
    }));
    if entry.naming == PortNaming::Named {
        let inputs: Vec<_> = entry.inputs.iter().filter_map(|port| port.name).collect();
        let outputs: Vec<_> = entry.outputs.iter().filter_map(|port| port.name).collect();
        out.push_str(",\n    \"input_names\": ");
        out.push_str(&json_str_list(&inputs));
        out.push_str(",\n    \"output_names\": ");
        out.push_str(&json_str_list(&outputs));
    }
    out.push_str(",\n    \"stateful\": ");
    out.push_str(if entry.stateful { "true" } else { "false" });
    out.push_str(",\n    \"reserved\": ");
    out.push_str(if entry.reserved { "true" } else { "false" });
    out.push_str(",\n    \"param_defaults\": [");
    if !entry.param_defaults.is_empty() {
        out.push('\n');
        for (index, default) in entry.param_defaults.iter().enumerate() {
            out.push_str("      ");
            out.push_str(&param_default_json(default));
            out.push_str(if index + 1 == entry.param_defaults.len() {
                "\n"
            } else {
                ",\n"
            });
        }
        out.push_str("    ");
    }
    out.push_str("]\n");
    out.push_str("  }");
}

fn param_default_json(default: &crate::ParamDefault) -> String {
    let name = json_str(default.name);
    match default.default {
        DefaultSource::Literal(literal) => {
            let (kind, key, value) = match literal {
                DefaultLiteral::Real(value) => ("literal", "real", json_f64(value)),
                DefaultLiteral::Integer(value) => ("literal", "integer", value.to_string()),
                DefaultLiteral::Boolean(value) => ("literal", "boolean", value.to_string()),
                DefaultLiteral::Str(value) => ("literal", "string", json_str(value)),
                DefaultLiteral::EnumMember(value) => ("literal", "enum", json_str(value)),
            };
            format!("{{ \"name\": {name}, \"kind\": \"{kind}\", \"{key}\": {value} }}")
        }
        DefaultSource::Derived { formula } => format!(
            "{{ \"name\": {name}, \"kind\": \"derived\", \"formula\": {} }}",
            json_str(formula)
        ),
        DefaultSource::Required => format!("{{ \"name\": {name}, \"kind\": \"required\" }}"),
    }
}

/// Serialize one [`ParamRule`] as a single-line JSON object: the variant name under `"rule"`,
/// then every embedded field by its Rust field name, in declaration order.
///
/// The match is deliberately exhaustive with no wildcard arm — the compile-forced drift guard.
fn param_rule_json(rule: &ParamRule) -> String {
    match rule {
        ParamRule::Required { name, .. } => object("Required", &[("name", json_str(name))]),
        ParamRule::Structural { name } => object("Structural", &[("name", json_str(name))]),
        ParamRule::StructuralArrayElements { base } => {
            object("StructuralArrayElements", &[("base", json_str(base))])
        }
        ParamRule::Boolean { name } => object("Boolean", &[("name", json_str(name))]),
        ParamRule::Real { name } => object("Real", &[("name", json_str(name))]),
        ParamRule::RealFinite { name } => object("RealFinite", &[("name", json_str(name))]),
        ParamRule::RealGreaterThan { name, min } => object(
            "RealGreaterThan",
            &[("name", json_str(name)), ("min", json_f64(*min))],
        ),
        ParamRule::RealFiniteGreaterThan { name, min } => object(
            "RealFiniteGreaterThan",
            &[("name", json_str(name)), ("min", json_f64(*min))],
        ),
        ParamRule::IntegerGreaterOrEqual { name, min } => object(
            "IntegerGreaterOrEqual",
            &[("name", json_str(name)), ("min", min.to_string())],
        ),
        ParamRule::IntegerLessOrEqualConstant { name, max } => object(
            "IntegerLessOrEqualConstant",
            &[("name", json_str(name)), ("max", max.to_string())],
        ),
        ParamRule::IntegerArrayElements { base, len } => object(
            "IntegerArrayElements",
            &[("base", json_str(base)), ("len", json_str(len))],
        ),
        ParamRule::IntegerArrayElementsInRange {
            base,
            len,
            len_default,
            min,
            max,
            max_default,
            default_to_index,
        } => object(
            "IntegerArrayElementsInRange",
            &[
                ("base", json_str(base)),
                ("len", json_str(len)),
                ("len_default", len_default.to_string()),
                ("min", min.to_string()),
                ("max", json_str(max)),
                ("max_default", max_default.to_string()),
                ("default_to_index", default_to_index.to_string()),
            ],
        ),
        ParamRule::RealArrayElements { base, len } => object(
            "RealArrayElements",
            &[("base", json_str(base)), ("len", json_str(len))],
        ),
        ParamRule::RealMatrixElements {
            base,
            rows,
            default_rows,
            cols,
            default_cols,
        } => object(
            "RealMatrixElements",
            &[
                ("base", json_str(base)),
                ("rows", json_str(rows)),
                ("default_rows", default_rows.to_string()),
                ("cols", json_str(cols)),
                ("default_cols", default_cols.to_string()),
            ],
        ),
        ParamRule::TimeTableMatrix {
            base,
            values,
            time_scale,
            period,
            extrapolation,
        } => object(
            "TimeTableMatrix",
            &[
                ("base", json_str(base)),
                ("values", json_str(time_table_values_name(*values))),
                ("time_scale", json_str(time_scale)),
                ("period", json_opt_str(*period)),
                ("extrapolation", json_opt_str(*extrapolation)),
            ],
        ),
        ParamRule::TimeTableOffset { base, table } => object(
            "TimeTableOffset",
            &[("base", json_str(base)), ("table", json_str(table))],
        ),
        ParamRule::BooleanArrayElements { base, len } => object(
            "BooleanArrayElements",
            &[("base", json_str(base)), ("len", json_str(len))],
        ),
        ParamRule::BooleanArrayTrueCountEquals {
            base,
            len,
            count,
            default,
        } => object(
            "BooleanArrayTrueCountEquals",
            &[
                ("base", json_str(base)),
                ("len", json_str(len)),
                ("count", json_str(count)),
                ("default", default.to_string()),
            ],
        ),
        ParamRule::EnumMembers { name, members } => object(
            "EnumMembers",
            &[
                ("name", json_str(name)),
                ("members", json_str_list(members)),
            ],
        ),
        ParamRule::RealGreaterOrEqual { name, min } => object(
            "RealGreaterOrEqual",
            &[("name", json_str(name)), ("min", json_f64(*min))],
        ),
        ParamRule::RealLessOrEqualConstant { name, max } => object(
            "RealLessOrEqualConstant",
            &[("name", json_str(name)), ("max", json_f64(*max))],
        ),
        ParamRule::RealTimesIntegerInclusiveRange {
            real,
            integer,
            min,
            max,
        } => object(
            "RealTimesIntegerInclusiveRange",
            &[
                ("real", json_str(real)),
                ("integer", json_str(integer)),
                ("min", json_f64(*min)),
                ("max", json_f64(*max)),
            ],
        ),
        ParamRule::IntegerProductLessOrEqualConstant { left, right, max } => object(
            "IntegerProductLessOrEqualConstant",
            &[
                ("left", json_str(left)),
                ("right", json_str(right)),
                ("max", max.to_string()),
            ],
        ),
        ParamRule::RealLessOrEqual { lower, upper } => object(
            "RealLessOrEqual",
            &[("lower", json_str(lower)), ("upper", json_str(upper))],
        ),
        ParamRule::RealLessOrEqualWarning { lower, upper } => object(
            "RealLessOrEqualWarning",
            &[("lower", json_str(lower)), ("upper", json_str(upper))],
        ),
        ParamRule::RealGreaterOrEqualScaledWarning {
            left,
            right,
            factor,
        } => object(
            "RealGreaterOrEqualScaledWarning",
            &[
                ("left", json_str(left)),
                ("right", json_str(right)),
                ("factor", json_f64(*factor)),
            ],
        ),
        ParamRule::RealEqualWarning { left, right } => object(
            "RealEqualWarning",
            &[("left", json_str(left)), ("right", json_str(right))],
        ),
    }
}

/// Render `{ "rule": "<name>", "<field>": <value>, ... }` with fields in the given order.
fn object(rule: &str, fields: &[(&str, String)]) -> String {
    let mut out = String::from("{ \"rule\": ");
    out.push_str(&json_str(rule));
    for (key, value) in fields {
        out.push_str(", ");
        out.push_str(&json_str(key));
        out.push_str(": ");
        out.push_str(value);
    }
    out.push_str(" }");
    out
}

/// Render a [`PortKind`] slice as an inline JSON array of variant-name strings.
fn port_kind_list(kinds: &[PortKind]) -> String {
    let names: Vec<&str> = kinds.iter().map(|kind| port_kind_name(*kind)).collect();
    json_name_array(&names)
}

/// Render a `&str` slice as an inline JSON array of strings.
fn json_str_list(values: &[&str]) -> String {
    json_name_array(values)
}

fn json_name_array(values: &[&str]) -> String {
    let mut out = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&json_str(value));
    }
    out.push(']');
    out
}

/// Variant-name string for a [`PortKind`] (exhaustive; a new kind fails compilation here).
fn port_kind_name(kind: PortKind) -> &'static str {
    match kind {
        PortKind::Real => "Real",
        PortKind::Integer => "Integer",
        PortKind::Boolean => "Boolean",
    }
}

/// Variant-name string for a [`TimeTableValues`] (exhaustive; a new kind fails compilation here).
fn time_table_values_name(values: TimeTableValues) -> &'static str {
    match values {
        TimeTableValues::Real => "Real",
        TimeTableValues::Integer => "Integer",
        TimeTableValues::Boolean => "Boolean",
    }
}

/// JSON-escape a string through `serde_json`.
fn json_str(value: &str) -> String {
    serde_json::to_string(value).expect("string-to-JSON serialization is infallible")
}

/// Serialize `Some(str)` as a JSON string, `None` as `null`.
fn json_opt_str(value: Option<&str>) -> String {
    value.map_or_else(|| String::from("null"), json_str)
}

/// Serialize a finite `f64` through `serde_json` (Ryu shortest round-trip decimal).
///
/// Panics on a non-finite value: `serde_json` would emit `null` for it, silently destroying the
/// guard bound, so a non-finite guard field must fail generation loudly instead.
fn json_f64(value: f64) -> String {
    assert!(
        value.is_finite(),
        "non-finite f64 guard field {value} cannot round-trip through JSON"
    );
    serde_json::to_string(&value).expect("finite f64-to-JSON serialization is infallible")
}
