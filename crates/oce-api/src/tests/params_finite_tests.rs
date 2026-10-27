//! Registry-wide census of the engine-wide Real-parameter finiteness gate (issue #362).
//!
//! For **every** registered block class and **every** Real parameter it declares — authored
//! defaults, required parameters, derived defaults, rule-referenced names, and the first element of
//! every Real array / matrix / time-table parameter — a one-block model is loaded with a valid
//! finite value, then:
//!
//! - each of `NaN`, `-NaN`, `+inf` and `-inf` is refused by `set_param` with
//!   [`OcError::ParamRange`] and leaves the parameter table and dirty flag untouched;
//! - re-submitting the loaded finite value is accepted (the gate never refuses a valid value);
//! - the same model carrying the non-finite value instead is refused at load with exactly one
//!   `non-finite-parameter` diagnostic naming that parameter.
//!
//! The enumeration is derived from the public catalog, never from a hand list, and its size is
//! pinned so a class or parameter that silently drops out of the census fails here.

use std::collections::BTreeSet;

use oce_blocks::{CatalogEntry, DefaultLiteral, DefaultSource, ParamRule, PortKind, catalog};
use oce_diag::DiagCode;

use super::common::*;

const NON_FINITE: [f64; 4] = [f64::NAN, -f64::NAN, f64::INFINITY, f64::NEG_INFINITY];

fn value_type(kind: PortKind) -> ValueType {
    match kind {
        PortKind::Real => ValueType::Real,
        PortKind::Integer => ValueType::Integer,
        PortKind::Boolean => ValueType::Boolean,
    }
}

/// Expand a width-indexed catalog template to its first element (`k_<i>` → `k_1`).
fn first_element(template: &str) -> String {
    template
        .replace("<i>", "1")
        .replace("<row>", "1")
        .replace("<col>", "1")
}

/// Every Real parameter name the class declares, first array element standing in for each array.
fn real_param_names(entry: &CatalogEntry) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let required_real = |name: &str| {
        entry.param_rules.iter().any(|rule| {
            matches!(rule, ParamRule::Required { name: n, kind: ValueType::Real } if *n == name)
        })
    };
    for default in entry.param_defaults {
        let real = match default.default {
            DefaultSource::Literal(DefaultLiteral::Real(_)) => true,
            DefaultSource::Required => required_real(default.name),
            // `extract_<i>` derives the Integer index `i`; every other derived default is Real.
            DefaultSource::Derived { formula } => formula != "i",
            DefaultSource::Literal(_) => false,
        };
        if real {
            names.insert(first_element(default.name));
        }
    }
    for rule in entry.param_rules {
        match *rule {
            ParamRule::Real { name }
            | ParamRule::RealFinite { name }
            | ParamRule::RealGreaterThan { name, .. }
            | ParamRule::RealFiniteGreaterThan { name, .. }
            | ParamRule::RealGreaterOrEqual { name, .. }
            | ParamRule::RealLessOrEqualConstant { name, .. } => {
                names.insert(name.to_owned());
            }
            ParamRule::RealLessOrEqual { lower, upper }
            | ParamRule::RealLessOrEqualWarning { lower, upper } => {
                names.insert(lower.to_owned());
                names.insert(upper.to_owned());
            }
            ParamRule::RealTimesIntegerInclusiveRange { real, .. } => {
                names.insert(real.to_owned());
            }
            ParamRule::RealArrayElements { base, .. } | ParamRule::TimeTableOffset { base, .. } => {
                names.insert(format!("{base}_1"));
            }
            ParamRule::RealMatrixElements { base, .. }
            | ParamRule::TimeTableMatrix { base, .. } => {
                names.insert(format!("{base}_1_1"));
            }
            _ => {}
        }
    }
    names
}

fn representative(kind: ValueType) -> Value {
    match kind {
        ValueType::Real => Value::Real(1.0),
        ValueType::Integer => Value::Integer(1),
        ValueType::Boolean => Value::Boolean(true),
        ValueType::String => Value::String(Arc::from("census")),
        ValueType::Enum(class) => Value::Enum { class, ordinal: 1 },
    }
}

/// A parameter set that loads for `entry`: every required parameter at a representative value,
/// plus a well-formed two-row table for the three time-table sources.
fn base_params(entry: &CatalogEntry) -> Vec<(Arc<str>, Value)> {
    let mut params: Vec<(Arc<str>, Value)> = entry
        .param_rules
        .iter()
        .filter_map(|rule| match rule {
            ParamRule::Required { name, kind } => Some((Arc::from(*name), representative(*kind))),
            _ => None,
        })
        .collect();
    if entry.class_path.ends_with("Sources.TimeTable") {
        params.retain(|(name, _)| name.as_ref() != "period");
        for (name, value) in [
            ("table_1_1", 0.0),
            ("table_1_2", 0.0),
            ("table_2_1", 1.0),
            ("table_2_2", 1.0),
        ] {
            params.push(rp(name, value));
        }
        if !entry.class_path.starts_with("CDL.Reals") {
            params.push(rp("period", 2.0));
        }
    }
    params
}

fn with_param(params: &[(Arc<str>, Value)], name: &str, value: Value) -> Vec<(Arc<str>, Value)> {
    let mut params = params.to_vec();
    match params.iter_mut().find(|(n, _)| n.as_ref() == name) {
        Some((_, slot)) => *slot = value,
        None => params.push((Arc::from(name), value)),
    }
    params
}

fn one_block_model(class: &str, params: Vec<(Arc<str>, Value)>) -> ModelGraph {
    let lookup = oce_blocks::lookup(class).expect("catalog class is registered");
    let block = (lookup.make)(&ParamTable {
        values: params.clone(),
    });
    let signature = block.resolved_signature();
    let ins: Vec<_> = signature.inputs.iter().copied().map(value_type).collect();
    let outs: Vec<_> = signature.outputs.iter().copied().map(value_type).collect();
    let mut mb = Mb::new();
    let (_, inputs, _) = mb.block(class, &ins, &outs, params);
    let mut model = mb.finish();
    model.external_inputs = inputs;
    model
}

/// Load the class with `name` at the first finite candidate that the class accepts.
fn load_with_finite(entry: &CatalogEntry, name: &str) -> Option<(Engine, Value)> {
    let base = base_params(entry);
    let current = base
        .iter()
        .find(|(n, _)| n.as_ref() == name)
        .map(|(_, v)| v.clone());
    let default = entry.param_defaults.iter().find_map(|d| match d.default {
        DefaultSource::Literal(DefaultLiteral::Real(v)) if first_element(d.name) == name => {
            Some(Value::Real(v))
        }
        _ => None,
    });
    let candidates = current.into_iter().chain(default).chain(
        [1.0, 0.5, 0.0, -1.0, 2.0, 1.0e-3]
            .into_iter()
            .map(Value::Real),
    );
    for candidate in candidates {
        let mut engine = Engine::in_memory();
        let model = one_block_model(entry.class_path, with_param(&base, name, candidate.clone()));
        if engine.build_model_in_memory(model, None).is_ok() {
            return Some((engine, candidate));
        }
    }
    None
}

#[test]
fn every_registered_real_parameter_refuses_non_finite_values_at_load_and_at_rest() {
    let mut exercised = Vec::new();
    let mut unloadable = Vec::new();
    for entry in catalog() {
        for name in real_param_names(entry) {
            let case = format!("{}.{name}", entry.class_path);
            let Some((mut engine, finite)) = load_with_finite(entry, &name) else {
                unloadable.push(case);
                continue;
            };
            let path = format!("b0.{name}");
            engine.halt().unwrap();
            for bad in NON_FINITE {
                let error = engine
                    .set_param(&path, Value::Real(bad))
                    .expect_err(&format!("{case}={bad}: set_param must refuse"));
                assert!(
                    matches!(&error, OcError::ParamRange { path: p } if *p == path),
                    "{case}={bad}: {error:?}"
                );
                assert!(engine.get_param(&path).unwrap().bit_eq(&finite), "{case}");
                assert!(!engine.params_dirty, "{case}: refused edit must not dirty");

                let model = one_block_model(
                    entry.class_path,
                    with_param(&base_params(entry), &name, Value::Real(bad)),
                );
                let error = Engine::in_memory()
                    .build_model_in_memory(model, None)
                    .expect_err(&format!("{case}={bad}: load must refuse"));
                let OcError::Validate(error) = error else {
                    panic!("{case}={bad}: {error:?}")
                };
                let codes: Vec<_> = error.diagnostics.iter().map(|d| d.code).collect();
                assert_eq!(codes, [DiagCode::NonFiniteParameter], "{case}={bad}");
                assert!(
                    error.diagnostics[0].message.starts_with(&format!(
                        "parameter `{name}` on block `{}`",
                        entry.class_path
                    )),
                    "{case}: {}",
                    error.diagnostics[0].message
                );
            }
            engine
                .set_param(&path, finite.clone())
                .unwrap_or_else(|error| panic!("{case}: finite {finite:?} refused: {error:?}"));
            exercised.push(case);
        }
    }
    assert_eq!(unloadable, Vec::<String>::new());
    // Pinned census size: a class or Real parameter that silently leaves the enumeration fails.
    assert_eq!(exercised.len(), 103, "{exercised:#?}");
    // The classes the issue reproduced with no class rule at all, plus each array shape.
    for required in [
        "CDL.Reals.MultiplyByParameter.k",
        "CDL.Reals.AddParameter.p",
        "CDL.Logical.Timer.t",
        "CDL.Reals.IntegratorWithReset.k",
        "CDL.Reals.IntegratorWithReset.y_start",
        "CDL.Reals.MultiSum.k_1",
        "CDL.Reals.MatrixGain.K_1_1",
        "CDL.Reals.Sources.TimeTable.table_1_1",
        "CDL.Reals.Sources.TimeTable.offset_1",
    ] {
        assert!(exercised.iter().any(|case| case == required), "{required}");
    }
}

#[test]
fn unconstrained_real_parameters_accept_every_finite_boundary_at_load_and_at_rest() {
    // These parameters carry no value-constraining class rule, so only the finiteness gate could refuse them;
    // it must admit every finite IEEE boundary, bit-exactly, including signed zero and subnormals.
    let boundaries = [
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        f64::from_bits(1),
        -f64::from_bits(1),
        0.0,
        -0.0,
    ];
    for (class, name) in [
        ("CDL.Reals.MultiplyByParameter", "k"),
        ("CDL.Reals.AddParameter", "p"),
        ("CDL.Logical.Timer", "t"),
        ("CDL.Reals.IntegratorWithReset", "k"),
        ("CDL.Reals.IntegratorWithReset", "y_start"),
    ] {
        let entry = catalog()
            .iter()
            .find(|entry| entry.class_path == class)
            .expect("class is registered");
        // At most a type rule (`Real` / `Required`): no range, ordering or finiteness rule.
        assert!(
            entry.param_rules.iter().all(|rule| match rule {
                ParamRule::Real { .. } | ParamRule::Required { .. } => true,
                other => !format!("{other:?}").contains(&format!("\"{name}\"")),
            }),
            "{class}.{name} is expected to carry no value-constraining class rule"
        );
        let path = format!("b0.{name}");
        for value in boundaries {
            let mut loaded = Engine::in_memory();
            loaded
                .build_model_in_memory(
                    one_block_model(class, vec![(Arc::from(name), Value::Real(value))]),
                    None,
                )
                .unwrap_or_else(|error| panic!("{class}.{name}={value:e} load: {error:?}"));
            assert!(loaded.get_param(&path).unwrap().bit_eq(&Value::Real(value)));

            let mut edited = Engine::in_memory();
            edited
                .build_model_in_memory(
                    one_block_model(class, vec![(Arc::from(name), Value::Real(1.0))]),
                    None,
                )
                .unwrap();
            edited.halt().unwrap();
            edited
                .set_param(&path, Value::Real(value))
                .unwrap_or_else(|error| panic!("{class}.{name}={value:e} edit: {error:?}"));
            assert!(edited.get_param(&path).unwrap().bit_eq(&Value::Real(value)));
            edited.resume().unwrap();
        }
    }
}

#[test]
fn integer_and_boolean_parameters_are_outside_the_finiteness_gate() {
    // The gate is value-based: Integer extremes (always finite, also when promoted to a Real
    // parameter) and Booleans never trip it.
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(
            one_block_model(
                "CDL.Reals.MultiplyByParameter",
                vec![(Arc::from("k"), Value::Integer(i64::MAX))],
            ),
            None,
        )
        .expect("an Integer-valued Real gain is finite");
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(
            one_block_model(
                "CDL.Integers.AddParameter",
                vec![(Arc::from("p"), Value::Integer(i64::from(i32::MIN)))],
            ),
            None,
        )
        .expect("Integer parameters are never non-finite");
    engine.halt().unwrap();
    engine
        .set_param("b0.p", Value::Integer(i64::from(i32::MAX)))
        .expect("Integer edit is outside the finiteness gate");
}
