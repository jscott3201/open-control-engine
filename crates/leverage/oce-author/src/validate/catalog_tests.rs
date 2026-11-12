//! The catalog view: template matching, and the facade's enumeration table (issue #468) checked
//! against everything else `oce-api` publishes about enumerations.

use oce_api::{
    CatalogDefault, CatalogRule, CatalogValueKind, ClassPortsError, DiagCode, EnumerationKind,
    Value, ValueType, catalog, class_metadata, class_ports, enumeration, enumerations,
};

use super::{enum_type, match_template};
use crate::emit::iri::{CLASS_BASE, MODELICA_PACKAGE, class_iri};

#[test]
fn templates_match_only_positive_unpadded_indices() {
    assert_eq!(match_template("k_<i>", "k_2"), Some(vec![2]));
    assert_eq!(match_template("K_<row>_<col>", "K_12_3"), Some(vec![12, 3]));
    assert_eq!(match_template("k_<i>", "k_0"), None);
    assert_eq!(match_template("k_<i>", "k_02"), None);
    assert_eq!(match_template("k_<i>", "k_"), None);
    assert_eq!(match_template("k_<i>", "k_2x"), None);
    assert_eq!(match_template("k_<i>", "kk_2"), None);
    assert_eq!(match_template("nin", "nin"), None);
    assert_eq!(
        match_template("k_<i>", "k_99999999999999999999999999"),
        None
    );
}

fn enum_parameters() -> Vec<(&'static str, &'static str, ValueType, CatalogDefault)> {
    class_metadata()
        .iter()
        .flat_map(|class| {
            class
                .parameters
                .iter()
                .filter(|p| matches!(p.value_type, ValueType::Enum(_)))
                .map(move |p| (class.class_path, p.name, p.value_type, p.default.clone()))
        })
        .collect()
}

#[test]
fn enum_table_agrees_with_the_catalog() {
    let parameters = enum_parameters();
    assert!(!parameters.is_empty());
    for (class, name, value_type, default) in &parameters {
        let table = enum_type(*value_type)
            .unwrap_or_else(|| panic!("{class}.{name}: enumeration not in the table"));
        if let CatalogDefault::EnumMember(member) = default {
            assert!(
                table.members.contains(member),
                "{class}.{name} default {member}"
            );
        }
        let entry = catalog()
            .iter()
            .find(|e| e.class_path == *class)
            .expect("entry");
        for rule in &entry.param_rules {
            match rule {
                CatalogRule::EnumMembers { name: n, members } if n == name => {
                    assert_eq!(table.members, *members, "{class}.{name}");
                }
                CatalogRule::Required {
                    name: n,
                    kind:
                        CatalogValueKind::Enum {
                            class_path,
                            members,
                        },
                } if n == name => {
                    assert_eq!(table.class_path, *class_path, "{class}.{name}");
                    assert_eq!(table.members, *members, "{class}.{name}");
                }
                _ => {}
            }
        }
    }
    for row in enumerations() {
        let Some(id) = row.id else {
            assert_eq!(row.kind, EnumerationKind::ConstantPackage);
            continue;
        };
        assert_eq!(
            enum_type(ValueType::Enum(id)),
            Some(row),
            "{}",
            row.class_path
        );
        if row.class_path.starts_with("CDL.Types.") {
            assert!(
                parameters
                    .iter()
                    .any(|(_, _, t, _)| enum_type(*t).is_some_and(|u| u == row)),
                "{}: some registered parameter uses it",
                row.class_path
            );
        }
    }
}

#[test]
fn every_table_class_path_round_trips_through_the_class_base() {
    for row in enumerations() {
        let iri = class_iri(row.class_path);
        assert!(iri.starts_with(CLASS_BASE), "{iri}");
        // The engine's class bridge reads the fragment after the last `#`.
        let fragment = iri.rsplit('#').next().expect("fragment");
        assert_eq!(fragment, row.qualified_path, "{}", row.class_path);
        let relative = fragment
            .strip_prefix(MODELICA_PACKAGE)
            .expect("under Buildings.Controls.OBC.");
        assert_eq!(enumeration(relative), Some(row));
    }
}

/// A table that satisfies `class`'s other required parameters.
fn base_table(class: &str) -> Vec<(&'static str, Value)> {
    if class == "CDL.Reals.Sources.TimeTable" {
        vec![
            ("table_1_1", Value::Real(0.0)),
            ("table_1_2", Value::Real(1.0)),
            ("table_2_1", Value::Real(1.0)),
            ("table_2_2", Value::Real(2.0)),
        ]
    } else {
        Vec::new()
    }
}

#[test]
fn enum_table_member_counts_match_the_load_gate() {
    for (class, name, value_type, _) in enum_parameters() {
        let ValueType::Enum(id) = value_type else {
            unreachable!("filtered to enums")
        };
        let members = enum_type(value_type).expect("known").members;
        let with = |ordinal: usize| {
            let mut table = base_table(class);
            table.push((
                name,
                Value::Enum {
                    class: id,
                    ordinal: u32::try_from(ordinal).expect("small"),
                },
            ));
            class_ports(class, &table)
        };
        assert!(
            with(members.len()).is_ok(),
            "{class}.{name}: the last member is accepted"
        );
        match with(members.len() + 1) {
            Err(ClassPortsError::InvalidParameters { diagnostics, .. }) => assert!(
                diagnostics
                    .iter()
                    .any(|d| d.code == DiagCode::ParameterOutOfRange),
                "{class}.{name}: {diagnostics:?}"
            ),
            other => panic!("{class}.{name}: one past the last member is refused, got {other:?}"),
        }
    }
}
