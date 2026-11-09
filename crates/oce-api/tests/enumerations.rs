//! `enumerations`, `enumeration`, `enumeration_by_id` and `enum_class_path` through the facade
//! (issue #468, and the Enum class path lookup of #465).
//!
//! The table is held complete against everything else the facade publishes about enumerations:
//! every `EnumMembers` rule, every `Required` Enum kind, every enum default and every Enum-typed
//! parameter in `class_metadata()` names a row, and the row's members in order. Every Enum
//! parameter a load of the G36 corpus keeps names a class whose path the lookup returns.

use std::collections::BTreeSet;

use oce_api::{
    CatalogDefault, CatalogRule, CatalogValueKind, EnumClassId, EnumerationKind, MemberValue,
    Value, ValueType, catalog, class_metadata, enum_class_path, enumeration, enumeration_by_id,
    enumerations,
};

#[test]
fn facade_publishes_the_engine_table_itself() {
    assert!(std::ptr::eq(
        enumerations(),
        oce_model::enumeration_classes()
    ));
    assert_eq!(enumerations().len(), 18);
    let kinds: Vec<EnumerationKind> = enumerations().iter().map(|r| r.kind).collect();
    assert_eq!(
        kinds
            .iter()
            .filter(|k| **k == EnumerationKind::Enumeration)
            .count(),
        14
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|k| **k == EnumerationKind::ConstantPackage)
            .count(),
        4
    );
    for row in enumerations() {
        assert_eq!(enumeration(row.class_path), Some(row));
        assert_eq!(enumeration(row.qualified_path), None, "one path form");
        match row.id {
            Some(id) => {
                assert_eq!(enumeration_by_id(id), Some(row));
                assert_eq!(enum_class_path(id), Some(row.class_path));
            }
            None => assert_eq!(row.kind, EnumerationKind::ConstantPackage),
        }
    }
}

/// The row for an Enum-typed value type, or a panic naming `context`.
fn row_of(value_type: ValueType, context: &str) -> &'static oce_api::EnumerationClass {
    let ValueType::Enum(id) = value_type else {
        panic!("{context}: not an Enum type");
    };
    let row = enumeration_by_id(id).unwrap_or_else(|| panic!("{context}: no row for {id:?}"));
    assert_eq!(row.kind, EnumerationKind::Enumeration, "{context}");
    row
}

#[test]
fn every_enum_parameter_rule_and_default_names_a_row_and_its_members_in_order() {
    let mut used = BTreeSet::new();
    let mut checked_rules = 0usize;
    for class in class_metadata() {
        let entry = catalog()
            .iter()
            .find(|e| e.class_path == class.class_path)
            .expect("metadata class is in the catalog");
        for parameter in &class.parameters {
            if !matches!(parameter.value_type, ValueType::Enum(_)) {
                continue;
            }
            let context = format!("{}.{}", class.class_path, parameter.name);
            let row = row_of(parameter.value_type, &context);
            used.insert(row.class_path);
            if let CatalogDefault::EnumMember(member) = parameter.default {
                assert!(
                    matches!(row.member_value(member), Some(MemberValue::Ordinal(_))),
                    "{context}: default {member}"
                );
            }
            for rule in &entry.param_rules {
                match rule {
                    CatalogRule::EnumMembers { name, members } if *name == parameter.name => {
                        assert_eq!(*members, row.members, "{context}");
                        checked_rules += 1;
                    }
                    CatalogRule::Required {
                        name,
                        kind:
                            CatalogValueKind::Enum {
                                class_path,
                                members,
                            },
                    } if *name == parameter.name => {
                        assert_eq!(*class_path, row.class_path, "{context}");
                        assert_eq!(*members, row.members, "{context}");
                        checked_rules += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    assert!(checked_rules > 0);
    // The catalog's enum parameters use every CDL enumeration (`SimpleController` by the PID
    // family, `Smoothness` and `Extrapolation` by `TimeTable`, `ZeroTime` by `CalendarTime`).
    assert_eq!(
        used,
        BTreeSet::from([
            "CDL.Types.Extrapolation",
            "CDL.Types.SimpleController",
            "CDL.Types.Smoothness",
            "CDL.Types.ZeroTime",
        ])
    );
}

#[test]
fn every_catalog_enum_rule_anywhere_matches_some_row() {
    // Rules on parameters `class_metadata` might not list still name a row's member list.
    for entry in catalog() {
        for rule in &entry.param_rules {
            let members = match rule {
                CatalogRule::EnumMembers { members, .. } => *members,
                CatalogRule::Required {
                    kind: CatalogValueKind::Enum { members, .. },
                    ..
                } => *members,
                _ => continue,
            };
            assert!(
                enumerations().iter().any(|r| r.members == members),
                "{}: {members:?}",
                entry.class_path
            );
        }
    }
}

#[test]
fn unknown_ids_have_no_path() {
    for id in [0, 5, 100, 111, u32::MAX] {
        assert_eq!(enum_class_path(EnumClassId(id)), None);
        assert_eq!(enumeration_by_id(EnumClassId(id)), None);
    }
}

#[test]
fn hostile_class_paths_are_refused() {
    for path in [
        "",
        ".",
        "cdl.types.simplecontroller",
        "CDL.Types.SimpleController.",
        "CDL.Types.Simple",
        "SimpleController",
        "Buildings.Controls.OBC.CDL.Types.SimpleController",
        "http://example.org#Buildings.Controls.OBC.CDL.Types.SimpleController",
        "ASHRAE.G36.Types.OperationModes.occupied",
        "ASHRAE.G36.Types.",
    ] {
        assert_eq!(enumeration(path), None, "{path:?}");
    }
}

#[test]
fn every_loaded_enum_parameter_of_the_g36_corpus_names_its_class_through_the_lookup() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oce-cxf/tests/fixtures/g36");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .expect("g36 fixtures")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonld"))
        .collect();
    paths.sort();
    let mut seen = BTreeSet::new();
    for path in paths {
        let bytes = std::fs::read(&path).expect("fixture");
        let mut engine = oce_api::Engine::new();
        if engine.load_cxf(&bytes).is_err() {
            continue;
        }
        for (name, value, _) in engine.params().iter() {
            if let Value::Enum { class, ordinal } = value {
                let row = enumeration_by_id(class)
                    .unwrap_or_else(|| panic!("{}: {name}: no row", path.display()));
                assert_eq!(enum_class_path(class), Some(row.class_path));
                assert!(
                    ordinal >= 1 && ordinal as usize <= row.members.len(),
                    "{}: {name}",
                    path.display()
                );
                seen.insert(row.class_path);
            }
        }
    }
    assert!(!seen.is_empty(), "the corpus loads some Enum parameter");
}
