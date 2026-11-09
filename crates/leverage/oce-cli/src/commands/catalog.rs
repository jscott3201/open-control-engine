//! `oce catalog`: the block classes, their parameters and their resolved ports.
//!
//! Without a class: every class in catalog order (`oce_api::class_metadata`), each with its ports
//! at the default parameter table where they resolve (`null` where a required parameter has no
//! default); exit `0`. With a class: that class, its parameters (type, default, settable) and its
//! ports resolved at the default table or at the `--param NAME=VALUE` values
//! (`oce_api::class_ports`). The class rules' refusal of the table is a finding (exit `1`), and so
//! is an unknown class; a malformed `--param` is an error (exit `2`).
//!
//! An enumeration parameter's `--param` value is a member name, bare (`PI`) or qualified by the
//! type (`SimpleController.PI`), resolved through the facade's enumeration table
//! (`oce_api::enumeration_by_id`, GitHub #488); its JSON `enumeration` is the type's authoring class
//! path (`oce_api::enumeration_class_path`), `null` for every other type.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use oce_api::{
    CatalogDefault, CatalogPortKind, ClassMetadata, ClassParameter, ClassPort, ClassPortsError,
    MemberValue, PARAM_COL_PLACEHOLDER, PARAM_INDEX_PLACEHOLDER, PARAM_ROW_PLACEHOLDER,
    Value as EngineValue, ValueType, catalog, class_metadata, class_ports, enumeration_by_id,
    enumeration_class_path, param_element_name,
};
use serde_json::{Value, json};

use super::Outcome;
use crate::report::Report;
use crate::{Exit, codes};

/// Run `catalog`.
pub(crate) fn run(class: Option<&str>, json: bool, params: &[String]) -> Outcome {
    let mut report = Report::new("catalog", None);
    match class {
        None => list(report, json),
        Some(class) => {
            report.set("class", Value::Null);
            report.set("params", params_echo(params));
            describe(report, class, json, params)
        }
    }
}

/// Every class.
fn list(mut report: Report, json: bool) -> Outcome {
    let metadata = class_metadata();
    let classes: Vec<Value> = metadata
        .iter()
        .map(|meta| {
            let ports = class_ports(meta.class_path, &[]).ok();
            class_json(
                meta,
                ports.as_ref().map(|p| (&p.inputs[..], &p.outputs[..])),
            )
        })
        .collect();
    report.set("classes", Value::Array(classes));
    report.verdict(format!("{} classes", metadata.len()));
    if json {
        return Outcome::with_exit(Exit::Yes, report);
    }
    let width = metadata
        .iter()
        .map(|m| m.class_path.len())
        .max()
        .unwrap_or(0);
    let mut text = String::new();
    for meta in metadata {
        let _ = writeln!(text, "{:<width$}  {}", meta.class_path, meta.description);
    }
    Outcome::document(report, text.into_bytes())
}

/// One class at a parameter table.
fn describe(mut report: Report, class: &str, json: bool, params: &[String]) -> Outcome {
    let Some(meta) = class_metadata().iter().find(|m| m.class_path == class) else {
        report.finding_raw(
            oce_author::CLASS_NOT_FOUND,
            class,
            &format!("`{class}` is not a registered class"),
            "engine",
        );
        report.verdict("no such class");
        return finish(report, json, None);
    };
    let table = match parse_params(meta, params) {
        Ok(table) => table,
        Err((code, detail)) => {
            report.error(code, detail, None);
            report.verdict("refused: malformed --param");
            return finish(report, json, None);
        }
    };
    let borrowed: Vec<(&str, EngineValue)> = table
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect();
    let resolved = class_ports(meta.class_path, &borrowed);
    let ports = match &resolved {
        Ok(ports) => Some((&ports.inputs[..], &ports.outputs[..])),
        Err(ClassPortsError::InvalidParameters { diagnostics, .. }) => {
            for diagnostic in diagnostics {
                report.finding_raw(
                    diagnostic.code.as_str(),
                    meta.class_path,
                    &diagnostic.message,
                    "engine",
                );
            }
            None
        }
        Err(error) => {
            report.error(codes::PORTS_UNRESOLVABLE, error.to_string(), None);
            None
        }
    };
    report.set("class", class_json(meta, ports));
    report.verdict(if ports.is_some() {
        "ports resolved"
    } else {
        "ports do not resolve at this parameter table"
    });
    let text = ports.map(|ports| render_class(meta, ports));
    finish(report, json, text)
}

/// A described class's outcome: the report, or the text when it resolved and `--json` is off.
fn finish(report: Report, json: bool, text: Option<String>) -> Outcome {
    match text {
        Some(text) if !json => Outcome::document(report, text.into_bytes()),
        _ => Outcome::judged(report),
    }
}

/// The `--param` values as given, for the report.
fn params_echo(params: &[String]) -> Value {
    Value::Array(params.iter().map(|p| Value::from(p.as_str())).collect())
}

/// Parse `NAME=VALUE` pairs against the class's parameters.
fn parse_params(
    meta: &ClassMetadata,
    params: &[String],
) -> Result<Vec<(String, EngineValue)>, (&'static str, String)> {
    let mut seen = BTreeSet::new();
    let mut table = Vec::with_capacity(params.len());
    for param in params {
        let Some((name, value)) = param.split_once('=') else {
            return Err((
                codes::MALFORMED_PARAM,
                format!("`{param}` is not NAME=VALUE"),
            ));
        };
        let Some(parameter) = meta.parameters.iter().find(|p| names_element(p.name, name)) else {
            return Err((
                codes::UNKNOWN_PARAM,
                format!("`{name}` is not a parameter of {}", meta.class_path),
            ));
        };
        if !seen.insert(name.to_owned()) {
            return Err((codes::DUPLICATE_PARAM, format!("`{name}` is given twice")));
        }
        let value = parse_value(parameter.value_type, value).ok_or_else(|| {
            (
                codes::MALFORMED_VALUE,
                format!(
                    "`{value}` is not a {} value for `{name}`",
                    text_type_name(parameter.value_type)
                ),
            )
        })?;
        table.push((name.to_owned(), value));
    }
    Ok(table)
}

/// Whether `name` is the scalar parameter `template` or one element of the array it spells.
fn names_element(template: &str, name: &str) -> bool {
    if template == name {
        return true;
    }
    let mut indices = Vec::new();
    let (mut rest_template, mut rest_name) = (template, name);
    loop {
        let next = [
            PARAM_INDEX_PLACEHOLDER,
            PARAM_ROW_PLACEHOLDER,
            PARAM_COL_PLACEHOLDER,
        ]
        .iter()
        .filter_map(|placeholder| {
            rest_template
                .find(placeholder)
                .map(|at| (at, placeholder.len()))
        })
        .min();
        let Some((at, len)) = next else {
            if rest_template != rest_name || indices.is_empty() {
                return false;
            }
            break;
        };
        let Some(after) = rest_name.strip_prefix(&rest_template[..at]) else {
            return false;
        };
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        let Ok(index) = after[..digits].parse::<usize>() else {
            return false;
        };
        indices.push(index);
        rest_name = &after[digits..];
        rest_template = &rest_template[at + len..];
    }
    param_element_name(template, &indices).as_deref() == Some(name)
}

/// A `--param` value of type `value_type`.
fn parse_value(value_type: ValueType, text: &str) -> Option<EngineValue> {
    match value_type {
        ValueType::Real => text
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .map(EngineValue::Real),
        ValueType::Integer => text.parse::<i64>().ok().map(EngineValue::Integer),
        ValueType::Boolean => match text {
            "true" => Some(EngineValue::Boolean(true)),
            "false" => Some(EngineValue::Boolean(false)),
            _ => None,
        },
        ValueType::String => Some(EngineValue::String(text.into())),
        ValueType::Enum(class) => {
            let row = enumeration_by_id(class)?;
            let member = match text.split_once('.') {
                Some((type_name, member)) if type_name == row.type_name() => member,
                Some(_) => return None,
                None => text,
            };
            match row.member_value(member)? {
                MemberValue::Ordinal(ordinal) => Some(EngineValue::Enum { class, ordinal }),
                MemberValue::Integer(_) => None,
            }
        }
    }
}

fn type_name(value_type: ValueType) -> &'static str {
    match value_type {
        ValueType::Real => "Real",
        ValueType::Integer => "Integer",
        ValueType::Boolean => "Boolean",
        ValueType::String => "String",
        ValueType::Enum(_) => "Enum",
    }
}

/// The type as the text listing spells it: an enumeration by its type name (`SimpleController`).
fn text_type_name(value_type: ValueType) -> &'static str {
    match value_type {
        ValueType::Enum(id) => enumeration_by_id(id).map_or("Enum", |row| row.type_name()),
        other => type_name(other),
    }
}

fn kind_name(kind: CatalogPortKind) -> &'static str {
    match kind {
        CatalogPortKind::Real => "Real",
        CatalogPortKind::Integer => "Integer",
        CatalogPortKind::Boolean => "Boolean",
    }
}

/// A default as `{kind, value}`.
fn default_json(default: &CatalogDefault) -> Value {
    let (kind, value) = match default {
        CatalogDefault::Real(v) => ("real", json!(v)),
        CatalogDefault::Integer(v) => ("integer", json!(v)),
        CatalogDefault::Boolean(v) => ("boolean", json!(v)),
        CatalogDefault::String(v) => ("string", json!(v)),
        CatalogDefault::EnumMember(v) => ("member", json!(v)),
        CatalogDefault::Derived(v) => ("derived", json!(v)),
        CatalogDefault::Required => ("required", Value::Null),
    };
    json!({ "kind": kind, "value": value })
}

/// A default as the text listing spells it.
fn default_text(default: &CatalogDefault) -> String {
    match default {
        CatalogDefault::Real(v) => format!("= {v:?}"),
        CatalogDefault::Integer(v) => format!("= {v}"),
        CatalogDefault::Boolean(v) => format!("= {v}"),
        CatalogDefault::String(v) => format!("= {v:?}"),
        CatalogDefault::EnumMember(v) => format!("= {v}"),
        CatalogDefault::Derived(v) => format!("(derived: {v})"),
        CatalogDefault::Required => "(required)".to_owned(),
    }
}

fn parameter_json(parameter: &ClassParameter) -> Value {
    json!({
        "default": default_json(&parameter.default),
        "enumeration": enumeration_of(parameter.value_type).map_or(Value::Null, Value::from),
        "name": parameter.name,
        "settable": parameter.settable,
        "type": type_name(parameter.value_type),
    })
}

/// The authoring class path of an enumeration type; `None` for every other type.
fn enumeration_of(value_type: ValueType) -> Option<&'static str> {
    match value_type {
        ValueType::Enum(id) => enumeration_class_path(id),
        _ => None,
    }
}

fn ports_json(ports: &[ClassPort]) -> Value {
    Value::Array(
        ports
            .iter()
            .map(|port| json!({ "kind": kind_name(port.kind), "name": port.name }))
            .collect(),
    )
}

fn class_json(meta: &ClassMetadata, ports: Option<(&[ClassPort], &[ClassPort])>) -> Value {
    let reserved = catalog()
        .iter()
        .any(|entry| entry.class_path == meta.class_path && entry.reserved);
    json!({
        "class_path": meta.class_path,
        "description": meta.description,
        "inputs": ports.map_or(Value::Null, |(inputs, _)| ports_json(inputs)),
        "outputs": ports.map_or(Value::Null, |(_, outputs)| ports_json(outputs)),
        "parameters": meta.parameters.iter().map(parameter_json).collect::<Vec<_>>(),
        "reserved": reserved,
    })
}

/// The text rendering of one resolved class, in the text form's own spelling.
fn render_class(meta: &ClassMetadata, (inputs, outputs): (&[ClassPort], &[ClassPort])) -> String {
    let mut text = format!("{}\n  {}\n", meta.class_path, meta.description);
    text.push_str("\nparameters:\n");
    if meta.parameters.is_empty() {
        text.push_str("  (none)\n");
    }
    for parameter in &meta.parameters {
        let _ = writeln!(
            text,
            "  {} : {} {}{}",
            parameter.name,
            text_type_name(parameter.value_type),
            default_text(&parameter.default),
            if parameter.settable {
                ""
            } else {
                " [not settable]"
            }
        );
    }
    for (title, ports) in [("inputs", inputs), ("outputs", outputs)] {
        let _ = writeln!(text, "\n{title}:");
        if ports.is_empty() {
            text.push_str("  (none)\n");
        }
        for port in ports {
            let _ = writeln!(text, "  {} : {}", port.name, kind_name(port.kind));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_matches_a_scalar_or_an_element_of_its_array_template() {
        assert!(names_element("nin", "nin"));
        assert!(!names_element("nin", "nin1"));
        assert!(names_element("k_<i>", "k_1"));
        assert!(names_element("k_<i>", "k_12"));
        assert!(!names_element("k_<i>", "k_0"));
        assert!(!names_element("k_<i>", "k_"));
        assert!(!names_element("k_<i>", "k_01"));
        assert!(!names_element("k_<i>", "k_1x"));
        assert!(!names_element("k_<i>", "k"));
        assert!(names_element("K_<row>_<col>", "K_2_1"));
        assert!(!names_element("K_<row>_<col>", "K_2"));
        assert!(!names_element("K_<row>_<col>", "K_2_1_1"));
        assert!(!names_element("K_<row>_<col>", "k_2_1"));
    }

    /// `Value` has no `PartialEq`; its `Debug` spelling is exact for these cases.
    fn parsed(value_type: ValueType, text: &str) -> String {
        format!("{:?}", parse_value(value_type, text))
    }

    fn some(value: EngineValue) -> String {
        format!("{:?}", Some(value))
    }

    #[test]
    fn values_parse_by_the_parameter_type() {
        assert_eq!(parsed(ValueType::Real, "1"), some(EngineValue::Real(1.0)));
        assert_eq!(
            parsed(ValueType::Real, "-0.5e1"),
            some(EngineValue::Real(-5.0))
        );
        for refused in ["NaN", "inf", "-infinity", "1e999", "", "one"] {
            assert_eq!(parsed(ValueType::Real, refused), "None", "{refused}");
        }
        assert_eq!(
            parsed(ValueType::Integer, "-3"),
            some(EngineValue::Integer(-3))
        );
        assert_eq!(parsed(ValueType::Integer, "1.0"), "None");
        assert_eq!(parsed(ValueType::Integer, "9223372036854775808"), "None");
        assert_eq!(
            parsed(ValueType::Boolean, "true"),
            some(EngineValue::Boolean(true))
        );
        assert_eq!(parsed(ValueType::Boolean, "True"), "None");
        assert_eq!(
            parsed(ValueType::String, "a b"),
            some(EngineValue::String("a b".into()))
        );
    }

    #[test]
    fn an_enumeration_value_is_a_member_name_of_its_own_type() {
        let mut checked = 0;
        for row in oce_api::enumeration_types() {
            let Some(class) = row.id else {
                panic!("{} is an enumeration without an id", row.class_path);
            };
            let value_type = ValueType::Enum(class);
            for (index, member) in row.members.iter().enumerate() {
                let ordinal = u32::try_from(index + 1).unwrap_or(u32::MAX);
                let expected = some(EngineValue::Enum { class, ordinal });
                assert_eq!(parsed(value_type, member), expected, "{member}");
                let qualified = format!("{}.{member}", row.type_name());
                assert_eq!(parsed(value_type, &qualified), expected, "{qualified}");
                assert_eq!(parsed(value_type, &format!("Other.{member}")), "None");
                checked += 1;
            }
            assert_eq!(parsed(value_type, "1"), "None", "ordinals are not names");
            assert_eq!(parsed(value_type, "NoSuchMember"), "None");
            assert_eq!(enumeration_of(value_type), Some(row.class_path));
        }
        assert!(checked > 0);
        assert_eq!(enumeration_of(ValueType::Real), None);
    }

    #[test]
    fn every_class_parameter_name_matches_itself_or_its_first_element() {
        for meta in class_metadata() {
            for parameter in &meta.parameters {
                let first = [vec![1], vec![1, 1]]
                    .iter()
                    .find_map(|indices| param_element_name(parameter.name, indices))
                    .unwrap_or_else(|| parameter.name.to_owned());
                assert!(
                    names_element(parameter.name, &first),
                    "{}.{}",
                    meta.class_path,
                    parameter.name
                );
            }
        }
    }
}
