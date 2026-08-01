use std::{collections::HashSet, sync::Arc};

use crate::{
    BlockKind, Ctx, DefaultLiteral, DefaultSource, NoopDiagnostics, ParamRule, PortKind,
    PortNaming, catalog, port_names, registry,
};
use oce_model::{ParamTable, Value};

fn entry(path: &str) -> &'static crate::CatalogEntry {
    catalog()
        .iter()
        .find(|entry| entry.class_path == path)
        .unwrap()
}

fn default(path: &str, name: &str) -> DefaultSource {
    entry(path)
        .param_defaults
        .iter()
        .find(|default| default.name == name)
        .unwrap()
        .default
}

#[test]
fn catalog_partition_and_reserved_identities_are_pinned() {
    assert_eq!(catalog().len(), 136);
    assert_eq!(
        catalog()
            .iter()
            .filter(|e| e.naming == PortNaming::Named)
            .count(),
        108
    );
    assert_eq!(
        catalog()
            .iter()
            .filter(|e| e.naming == PortNaming::Positional)
            .count(),
        3
    );
    assert_eq!(
        catalog()
            .iter()
            .filter(|e| e.naming == PortNaming::WidthDriven)
            .count(),
        25
    );
    let reserved: Vec<_> = catalog()
        .iter()
        .filter(|e| e.reserved)
        .map(|e| e.class_path)
        .collect();
    assert_eq!(
        reserved,
        [
            "urn:oce:lowering#PassThrough.Real",
            "urn:oce:lowering#PassThrough.Integer",
            "urn:oce:lowering#PassThrough.Boolean",
        ]
    );
}

#[test]
fn consumer_palette_metadata_reports_authored_defaults() {
    let limiter = entry("CDL.Reals.Limiter");
    assert_eq!(limiter.inputs[0].name, Some("u"));
    assert_eq!(limiter.outputs[0].name, Some("y"));
    assert_eq!(limiter.inputs[0].kind, PortKind::Real);
    assert_eq!(
        default("CDL.Reals.Limiter", "uMin"),
        DefaultSource::Required
    );
    assert_eq!(
        default("CDL.Reals.Limiter", "uMax"),
        DefaultSource::Required
    );
    assert_eq!(
        default("CDL.Reals.PID", "k"),
        DefaultSource::Literal(DefaultLiteral::Real(1.0))
    );
    assert_eq!(
        default("CDL.Reals.PID", "controllerType"),
        DefaultSource::Literal(DefaultLiteral::EnumMember("PI"))
    );
    assert_eq!(
        default("CDL.Reals.MovingAverage", "delta"),
        DefaultSource::Required
    );
    assert!(matches!(
        default("CDL.Reals.LimitSlewRate", "fallingSlewRate"),
        DefaultSource::Derived { .. }
    ));
    let multi_and = entry("CDL.Logical.MultiAnd");
    assert_eq!(multi_and.naming, PortNaming::WidthDriven);
    assert!(multi_and.inputs.is_empty());
    assert_eq!(multi_and.outputs.len(), 1);
    assert_eq!(multi_and.outputs[0].kind, PortKind::Boolean);
    assert_eq!(
        entry("CDL.Reals.Sources.TimeTable").naming,
        PortNaming::Positional
    );
    assert!(matches!(
        default("CDL.Reals.MatrixGain", "K_<row>_<col>"),
        DefaultSource::Derived { .. }
    ));
    assert!(matches!(
        default("CDL.Routing.RealExtractSignal", "extract_<i>"),
        DefaultSource::Derived { .. }
    ));
}

#[test]
fn time_table_classes_publish_the_upstream_authored_defaults() {
    for (class_path, published) in [
        ("CDL.Reals.Sources.TimeTable", 4),
        ("CDL.Integers.Sources.TimeTable", 2),
        ("CDL.Logical.Sources.TimeTable", 2),
    ] {
        assert_eq!(
            entry(class_path).param_defaults.len(),
            published,
            "{class_path}"
        );
    }
    assert_eq!(
        default("CDL.Reals.Sources.TimeTable", "smoothness"),
        DefaultSource::Literal(DefaultLiteral::EnumMember("LinearSegments"))
    );
    assert_eq!(
        default("CDL.Reals.Sources.TimeTable", "extrapolation"),
        DefaultSource::Literal(DefaultLiteral::EnumMember("Periodic"))
    );
    assert_eq!(
        default("CDL.Reals.Sources.TimeTable", "offset_<i>"),
        DefaultSource::Literal(DefaultLiteral::Real(0.0))
    );
    assert_eq!(
        default("CDL.Reals.Sources.TimeTable", "timeScale"),
        DefaultSource::Literal(DefaultLiteral::Real(1.0))
    );
    assert_eq!(
        default("CDL.Integers.Sources.TimeTable", "timeScale"),
        DefaultSource::Literal(DefaultLiteral::Real(1.0))
    );
    assert_eq!(
        default("CDL.Integers.Sources.TimeTable", "period"),
        DefaultSource::Required
    );
    assert_eq!(
        default("CDL.Logical.Sources.TimeTable", "timeScale"),
        DefaultSource::Literal(DefaultLiteral::Real(1.0))
    );
    assert_eq!(
        default("CDL.Logical.Sources.TimeTable", "period"),
        DefaultSource::Required
    );
}

#[test]
fn catalog_metadata_invariants_cross_check_independent_sources() {
    let mut required_rules = 0;
    let mut required_defaults = 0;
    for entry in catalog() {
        assert!(
            entry
                .inputs
                .iter()
                .chain(&entry.outputs)
                .all(|port| { port.name.is_some() == (entry.naming == PortNaming::Named) })
        );
        let mut names = HashSet::new();
        assert!(
            entry
                .param_defaults
                .iter()
                .all(|default| names.insert(default.name))
        );
        for rule in entry.param_rules {
            if let ParamRule::Required { name, kind } = rule {
                required_rules += 1;
                assert!(
                    matches!(
                        kind,
                        oce_model::ValueType::Real
                            | oce_model::ValueType::Integer
                            | oce_model::ValueType::Boolean
                            | oce_model::ValueType::String
                            | oce_model::ValueType::Enum(_)
                    ),
                    "{}.{} has no supported required kind",
                    entry.class_path,
                    name
                );
                assert_eq!(
                    entry
                        .param_defaults
                        .iter()
                        .find(|default| default.name == *name)
                        .map(|d| d.default),
                    Some(DefaultSource::Required),
                    "{}.{name}",
                    entry.class_path
                );
            }
        }
        for default in entry
            .param_defaults
            .iter()
            .filter(|default| default.default == DefaultSource::Required)
        {
            required_defaults += 1;
            assert!(
                entry.param_rules.iter().any(
                    |rule| matches!(rule, ParamRule::Required { name, .. } if *name == default.name)
                ),
                "{}.{} has a required default but no Required rule",
                entry.class_path,
                default.name
            );
        }
        assert_eq!(
            entry.naming == PortNaming::Named,
            port_names::port_names(entry.class_path).is_some()
        );
        // `port_names_tests::width_driven_classes_are_exactly_the_unnamed_structural_set`
        // independently re-derives the structural predicate; do not duplicate it from catalog data.
    }
    assert_eq!((required_rules, required_defaults), (49, 49));
}

#[test]
fn every_required_catalog_parameter_declares_a_supported_kind() {
    let required: Vec<_> = catalog()
        .iter()
        .flat_map(|entry| {
            entry.param_rules.iter().filter_map(move |rule| match rule {
                ParamRule::Required { name, kind } => Some((entry.class_path, *name, *kind)),
                _ => None,
            })
        })
        .collect();
    assert_eq!(required.len(), 49);
}

#[test]
fn required_rules_and_required_defaults_agree_exactly() {
    let mut agree = 0;
    let mut rule_only = Vec::new();
    let mut default_only = Vec::new();
    for entry in catalog() {
        for rule in entry.param_rules {
            if let ParamRule::Required { name, .. } = rule {
                if entry.param_defaults.iter().any(|default| {
                    default.name == *name && default.default == DefaultSource::Required
                }) {
                    agree += 1;
                } else {
                    rule_only.push(format!("{}.{}", entry.class_path, name));
                }
            }
        }
        for default in entry
            .param_defaults
            .iter()
            .filter(|default| default.default == DefaultSource::Required)
        {
            if !entry.param_rules.iter().any(
                |rule| matches!(rule, ParamRule::Required { name, .. } if *name == default.name),
            ) {
                default_only.push(format!("{}.{}", entry.class_path, default.name));
            }
        }
    }
    assert_eq!(agree, 49);
    assert!(rule_only.is_empty(), "rule-only: {rule_only:?}");
    assert!(default_only.is_empty(), "default-only: {default_only:?}");
}

#[test]
fn required_constant_kinds_execute_authored_values_including_real_widening() {
    for (class, authored, expected) in [
        (
            "CDL.Reals.Sources.Constant",
            Value::Real(3.5),
            Value::Real(3.5),
        ),
        (
            "CDL.Integers.Sources.Constant",
            Value::Integer(7),
            Value::Integer(7),
        ),
        (
            "CDL.Logical.Sources.Constant",
            Value::Boolean(true),
            Value::Boolean(true),
        ),
        (
            "CDL.Reals.Sources.Constant",
            Value::Integer(3),
            Value::Real(3.0),
        ),
    ] {
        let params = ParamTable {
            values: vec![(Arc::from("k"), authored)],
        };
        let block = (registry::lookup(class).expect("constant registered").make)(&params);
        let mut emitted = Vec::new();
        block.step_algebraic(&Ctx::new(0.0, &NoopDiagnostics), &[], &mut |port, value| {
            emitted.push((port, value));
        });
        assert_eq!(emitted.len(), 1, "{class}");
        assert_eq!(emitted[0].0, 0, "{class}");
        assert!(emitted[0].1.bit_eq(&expected), "{class}: {emitted:?}");
    }
}

#[test]
fn class_state_hints_diverge_from_default_instances_only_for_zero_hysteresis_comparators() {
    let mut divergent: Vec<_> = catalog()
        .iter()
        .filter_map(|entry| {
            let resolved =
                (registry::lookup(entry.class_path).unwrap().make)(&ParamTable::default());
            (entry.stateful != (resolved.kind() == BlockKind::Stateful)).then_some(entry.class_path)
        })
        .collect();
    divergent.sort_unstable();
    assert_eq!(
        divergent,
        [
            "CDL.Reals.Greater",
            "CDL.Reals.GreaterThreshold",
            "CDL.Reals.Less",
            "CDL.Reals.LessThreshold",
        ]
    );
}

#[test]
fn manifest_data_mutations_are_observable() {
    let control = registry::manifest::render_entries(catalog());
    assert_eq!(
        control,
        include_str!("../../../tools/reference-catalog/oce-blocks.registry-manifest.json")
    );
    let mut removed = catalog().to_vec();
    removed.pop();
    assert_ne!(registry::manifest::render_entries(&removed), control);
    // The source-level 136-entry closure is pinned by `catalog_partition_and_reserved_identities`.

    let mut stateful = catalog().to_vec();
    stateful[0].stateful = !stateful[0].stateful;
    assert_ne!(registry::manifest::render_entries(&stateful), control);

    let mut literal = catalog().to_vec();
    let index = literal
        .iter()
        .position(|entry| entry.class_path == "CDL.Reals.PID")
        .unwrap();
    let mut defaults = literal[index].param_defaults.to_vec();
    let default = defaults
        .iter_mut()
        .find(|default| default.name == "k")
        .unwrap();
    let DefaultSource::Literal(DefaultLiteral::Real(value)) = &mut default.default else {
        panic!("PID.k is a Real literal");
    };
    *value += 1.0;
    literal[index].param_defaults = Box::leak(defaults.into_boxed_slice());
    assert_ne!(registry::manifest::render_entries(&literal), control);
}
