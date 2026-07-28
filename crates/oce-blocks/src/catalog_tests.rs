use std::collections::HashSet;

use crate::{
    DefaultLiteral, DefaultSource, ParamRule, PortKind, PortNaming, catalog, port_names, registry,
};

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
fn catalog_metadata_invariants_cross_check_independent_sources() {
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
            if let ParamRule::Required { name } = rule {
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
        let independent_width = entry.param_rules.iter().any(|rule| {
            matches!(
                rule,
                ParamRule::Structural { .. } | ParamRule::StructuralArrayElements { .. }
            )
        });
        assert_eq!(entry.width_driven, independent_width);
        assert_eq!(
            entry.naming == PortNaming::Named,
            port_names::port_names(entry.class_path).is_some()
        );
        assert_eq!(
            entry.naming == PortNaming::WidthDriven,
            port_names::port_names(entry.class_path).is_none() && independent_width
        );
    }
}

#[test]
fn manifest_data_mutations_are_observable() {
    let control = registry::manifest::render_entries(catalog());
    let mut removed = catalog().to_vec();
    removed.pop();
    assert_ne!(registry::manifest::render_entries(&removed), control);
    assert_ne!(removed.len(), 136);

    let mut stateful = catalog().to_vec();
    stateful[0].stateful = !stateful[0].stateful;
    assert_ne!(registry::manifest::render_entries(&stateful), control);

    let mut literal = catalog().to_vec();
    let index = literal
        .iter()
        .position(|entry| entry.class_path == "CDL.Reals.PID")
        .unwrap();
    literal[index].param_defaults = &[crate::ParamDefault {
        name: "k",
        default: DefaultSource::Literal(DefaultLiteral::Real(2.0)),
    }];
    assert_ne!(registry::manifest::render_entries(&literal), control);
}
