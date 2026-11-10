//! `oce catalog --param NAME=VALUE`: the parameter table a class's ports are resolved at.
//!
//! Each pair is parsed against the class's parameters (`oce_api::class_metadata`): the name is a
//! scalar parameter or one element of an array template (`k_2` of `k_<i>`), and the value is
//! read by the parameter's type. An enumeration value is a member name, bare (`PI`) or qualified
//! by the type (`SimpleController.PI`), resolved through the facade's enumeration table
//! (`oce_api::enumeration_by_id`, GitHub #488). Every refusal is one of the tool's error codes
//! (exit `2`).

use std::collections::BTreeSet;

use oce_api::{
    ClassMetadata, MemberValue, PARAM_COL_PLACEHOLDER, PARAM_INDEX_PLACEHOLDER,
    PARAM_ROW_PLACEHOLDER, Value as EngineValue, ValueType, enumeration_by_id, param_element_name,
};

use crate::codes;

/// Parse `NAME=VALUE` pairs against the class's parameters.
///
/// # Errors
///
/// The error code and its detail: `malformed-param`, `unknown-param`, `duplicate-param` or
/// `malformed-value`.
pub(super) fn parse_params(
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

/// The type as the report spells it: every enumeration is `Enum`.
pub(super) fn type_name(value_type: ValueType) -> &'static str {
    match value_type {
        ValueType::Real => "Real",
        ValueType::Integer => "Integer",
        ValueType::Boolean => "Boolean",
        ValueType::String => "String",
        ValueType::Enum(_) => "Enum",
    }
}

/// The type as the text listing spells it: an enumeration by its type name (`SimpleController`).
pub(super) fn text_type_name(value_type: ValueType) -> &'static str {
    match value_type {
        ValueType::Enum(id) => enumeration_by_id(id).map_or("Enum", |row| row.type_name()),
        other => type_name(other),
    }
}

#[cfg(test)]
mod tests {
    use oce_api::class_metadata;

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
            assert_eq!(text_type_name(value_type), row.type_name());
            assert_eq!(type_name(value_type), "Enum");
        }
        assert!(checked > 0);
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
