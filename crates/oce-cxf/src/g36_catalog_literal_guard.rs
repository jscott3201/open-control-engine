//! Literal and conditional-expression checks for the ASHRAE G36 catalog guard.

use std::collections::{BTreeMap, BTreeSet};

use crate::CxfValue;
use crate::g36_catalog_guard_data::FixtureSource;
use crate::g36_catalog_guard_helpers::{jsonld_fragment, parameter_names, parse_cxf};

const G36_TYPES_PREFIX: &str = "Buildings.Controls.OBC.ASHRAE.G36.Types.";

pub(super) fn validate_g36_literals_in_fixture(
    fixture: &FixtureSource,
    enum_literals: &BTreeMap<String, BTreeSet<String>>,
    constant_packages: &BTreeMap<String, BTreeMap<String, i64>>,
    errors: &mut Vec<String>,
) {
    let document = parse_cxf(fixture, errors);
    for node in &document.graph {
        if let Some(type_id) = node.is_of_data_type.as_ref().map(|iri| iri.id.as_str()) {
            validate_type_id(type_id, fixture, enum_literals, constant_packages, errors);
        }
        if let Some(value) = &node.value {
            validate_value(value, fixture, enum_literals, constant_packages, errors);
        }
        if let Some(value) = &node.min {
            validate_value(value, fixture, enum_literals, constant_packages, errors);
        }
        if let Some(value) = &node.max {
            validate_value(value, fixture, enum_literals, constant_packages, errors);
        }
    }
}

fn validate_type_id(
    type_id: &str,
    fixture: &FixtureSource,
    enum_literals: &BTreeMap<String, BTreeSet<String>>,
    constant_packages: &BTreeMap<String, BTreeMap<String, i64>>,
    errors: &mut Vec<String>,
) {
    let type_id = jsonld_fragment(type_id);
    if !type_id.starts_with(G36_TYPES_PREFIX) {
        return;
    }
    if !enum_literals.contains_key(type_id) && !constant_packages.contains_key(type_id) {
        errors.push(format!("unknown-g36-enum-type: {}:{type_id}", fixture.name));
    }
}

fn validate_value(
    value: &CxfValue,
    fixture: &FixtureSource,
    enum_literals: &BTreeMap<String, BTreeSet<String>>,
    constant_packages: &BTreeMap<String, BTreeMap<String, i64>>,
    errors: &mut Vec<String>,
) {
    match value {
        CxfValue::Expr(expr) => {
            if expr.starts_with(G36_TYPES_PREFIX) {
                validate_g36_literal(expr, fixture, enum_literals, constant_packages, errors);
            }
        }
        CxfValue::List(values) => {
            for value in values {
                validate_value(value, fixture, enum_literals, constant_packages, errors);
            }
        }
        CxfValue::Bool(_) | CxfValue::Int(_) | CxfValue::Float(_) | CxfValue::Typed { .. } => {}
    }
}

pub(super) fn validate_conditional_guards(
    fixture: &FixtureSource,
    enum_literals: &BTreeMap<String, BTreeSet<String>>,
    constant_packages: &BTreeMap<String, BTreeMap<String, i64>>,
    errors: &mut Vec<String>,
) {
    let document = parse_cxf(fixture, errors);
    let params = parameter_names(&document);
    for node in document
        .graph
        .iter()
        .filter(|node| node.is_conditional == Some(true))
    {
        let Some(expr) = node.cond_expr.as_deref() else {
            errors.push(format!(
                "conditional-guard-missing: {}:{}",
                fixture.name, node.id
            ));
            continue;
        };
        validate_guard_expr(
            fixture,
            &node.id,
            expr,
            &params,
            enum_literals,
            constant_packages,
            errors,
        );
    }
}

fn validate_guard_expr(
    fixture: &FixtureSource,
    node_id: &str,
    expr: &str,
    params: &BTreeSet<String>,
    enum_literals: &BTreeMap<String, BTreeSet<String>>,
    constant_packages: &BTreeMap<String, BTreeMap<String, i64>>,
    errors: &mut Vec<String>,
) {
    if expr.contains("time") || expr.contains("sin(") || expr.contains("max(") || expr.contains('+')
    {
        errors.push(format!(
            "unsupported-conditional-guard: {}:{node_id}",
            fixture.name
        ));
        return;
    }
    let terms = expr
        .replace('(', " ( ")
        .replace(')', " ) ")
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if terms.is_empty() {
        errors.push(format!(
            "unsupported-conditional-guard: {}:{node_id}",
            fixture.name
        ));
        return;
    }
    let mut saw_expression = false;
    for (index, term) in terms.iter().enumerate() {
        if term == "==" || term == "!=" {
            saw_expression = true;
            let left = terms
                .get(index.wrapping_sub(1))
                .map(String::as_str)
                .unwrap_or("");
            let right = terms.get(index + 1).map(String::as_str).unwrap_or("");
            if !params.contains(left) {
                errors.push(format!(
                    "conditional-guard-unknown-parameter: {}:{left}",
                    fixture.name
                ));
            }
            validate_g36_literal(right, fixture, enum_literals, constant_packages, errors);
        }
    }
    if !saw_expression {
        let bare = expr.trim().trim_start_matches('!').trim();
        if !params.contains(bare) {
            errors.push(format!(
                "conditional-guard-unknown-parameter: {}:{bare}",
                fixture.name
            ));
        }
    }
}

fn validate_g36_literal(
    expr: &str,
    fixture: &FixtureSource,
    enum_literals: &BTreeMap<String, BTreeSet<String>>,
    constant_packages: &BTreeMap<String, BTreeMap<String, i64>>,
    errors: &mut Vec<String>,
) {
    let Some((type_path, literal)) = expr.rsplit_once('.') else {
        errors.push(format!("unknown-g36-enum-literal: {}:{expr}", fixture.name));
        return;
    };
    if let Some(literals) = enum_literals.get(type_path) {
        if !literals.contains(literal) {
            errors.push(format!("unknown-g36-enum-literal: {}:{expr}", fixture.name));
        }
        return;
    }
    if let Some(constants) = constant_packages.get(type_path) {
        if !constants.contains_key(literal) {
            errors.push(format!("unknown-g36-enum-literal: {}:{expr}", fixture.name));
        }
        return;
    }
    errors.push(format!(
        "unknown-g36-enum-type: {}:{type_path}",
        fixture.name
    ));
}
