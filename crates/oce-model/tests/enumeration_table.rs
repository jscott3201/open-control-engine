//! The enumeration and constant-package table (issue #468): its invariants, its agreement with the
//! vendored Buildings source, and a differential test against the hand-written `match` arms it
//! replaced, over every class path and member the table knows plus hostile spellings.

#[path = "support/match_arm_oracle.rs"]
mod match_arm_oracle;

use std::collections::BTreeSet;
use std::path::Path;

use oce_model::{
    EnumClassId, EnumerationClass, EnumerationKind, MemberValue, OBC_PACKAGE, enum_class_id,
    enum_descriptor, enum_descriptor_by_path, enum_member_ordinal, enumeration_class,
    enumeration_class_by_id, enumeration_class_path, enumeration_classes, enumeration_member_value,
    g36_enum_class_id, g36_enum_literals, g36_integer_constant, g36_integer_constant_in_package,
    is_g36_integer_constant_package, is_g36_type_path,
};

const CDL_ENUMERATIONS: [&str; 4] = [
    "CDL.Types.SimpleController",
    "CDL.Types.Smoothness",
    "CDL.Types.Extrapolation",
    "CDL.Types.ZeroTime",
];

const G36_ENUMERATIONS: [&str; 10] = [
    "ASHRAEClimateZone",
    "ControlEconomizer",
    "CoolingCoil",
    "EnergyStandard",
    "FreezeStat",
    "HeatingCoil",
    "OutdoorAirSection",
    "PressureControl",
    "Title24ClimateZone",
    "VentilationStandard",
];

const G36_CONSTANT_PACKAGES: [&str; 4] = [
    "DemandLimitLevels",
    "FreezeProtectionStages",
    "OperationModes",
    "ZoneStates",
];

fn rows() -> &'static [EnumerationClass] {
    enumeration_classes()
}

#[test]
fn table_lists_exactly_the_four_cdl_ten_g36_enumerations_and_four_constant_packages() {
    let mut expected: Vec<(String, EnumerationKind)> = CDL_ENUMERATIONS
        .iter()
        .map(|p| ((*p).to_owned(), EnumerationKind::Enumeration))
        .collect();
    expected.extend(G36_ENUMERATIONS.iter().map(|n| {
        (
            format!("ASHRAE.G36.Types.{n}"),
            EnumerationKind::Enumeration,
        )
    }));
    expected.extend(G36_CONSTANT_PACKAGES.iter().map(|n| {
        (
            format!("ASHRAE.G36.Types.{n}"),
            EnumerationKind::ConstantPackage,
        )
    }));
    let actual: Vec<(String, EnumerationKind)> = rows()
        .iter()
        .map(|r| (r.class_path.to_owned(), r.kind))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn every_row_keeps_the_table_invariants() {
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut type_names = BTreeSet::new();
    for row in rows() {
        assert_eq!(
            row.qualified_path,
            format!("{OBC_PACKAGE}{}", row.class_path),
            "{}",
            row.class_path
        );
        assert!(
            !row.class_path.starts_with("Buildings."),
            "{}",
            row.class_path
        );
        assert!(paths.insert(row.class_path), "duplicate {}", row.class_path);
        assert!(
            type_names.insert(row.type_name()),
            "last segment {} is not unique",
            row.type_name()
        );
        assert_eq!(
            Some(row.type_name()),
            row.class_path.rsplit('.').next(),
            "{}",
            row.class_path
        );
        assert!(!row.members.is_empty(), "{}", row.class_path);
        let members: BTreeSet<_> = row.members.iter().collect();
        assert_eq!(members.len(), row.members.len(), "{}", row.class_path);
        for member in row.members {
            assert!(
                !member.is_empty() && !member.contains('.'),
                "{}.{member}",
                row.class_path
            );
        }
        match row.kind {
            EnumerationKind::Enumeration => {
                let id = row.id.expect("an enumeration carries an id");
                assert!(ids.insert(id.0), "duplicate id {}", id.0);
                assert!(row.values.is_empty(), "{}", row.class_path);
            }
            EnumerationKind::ConstantPackage => {
                assert_eq!(row.id, None, "{}", row.class_path);
                assert_eq!(row.values.len(), row.members.len(), "{}", row.class_path);
            }
        }
    }
}

#[test]
fn every_enum_class_id_constant_has_exactly_one_row() {
    let constants = [
        EnumClassId::SIMPLE_CONTROLLER,
        EnumClassId::SMOOTHNESS,
        EnumClassId::EXTRAPOLATION,
        EnumClassId::ZERO_TIME,
        EnumClassId::G36_ASHRAE_CLIMATE_ZONE,
        EnumClassId::G36_CONTROL_ECONOMIZER,
        EnumClassId::G36_COOLING_COIL,
        EnumClassId::G36_ENERGY_STANDARD,
        EnumClassId::G36_FREEZE_STAT,
        EnumClassId::G36_HEATING_COIL,
        EnumClassId::G36_OUTDOOR_AIR_SECTION,
        EnumClassId::G36_PRESSURE_CONTROL,
        EnumClassId::G36_TITLE24_CLIMATE_ZONE,
        EnumClassId::G36_VENTILATION_STANDARD,
    ];
    for id in constants {
        assert_eq!(
            rows().iter().filter(|r| r.id == Some(id)).count(),
            1,
            "{id:?}"
        );
    }
    let enumerations = rows()
        .iter()
        .filter(|r| r.kind == EnumerationKind::Enumeration)
        .count();
    assert_eq!(enumerations, constants.len());
}

#[test]
fn lookups_by_path_id_and_member_agree_with_the_rows() {
    for row in rows() {
        assert_eq!(enumeration_class(row.class_path), Some(row));
        if let Some(id) = row.id {
            assert_eq!(enumeration_class_by_id(id), Some(row));
            assert_eq!(enumeration_class_path(id), Some(row.class_path));
        }
        for (index, member) in row.members.iter().enumerate() {
            let expected = match row.kind {
                EnumerationKind::Enumeration => {
                    MemberValue::Ordinal(u32::try_from(index + 1).expect("small"))
                }
                EnumerationKind::ConstantPackage => MemberValue::Integer(row.values[index]),
            };
            assert_eq!(
                enumeration_member_value(row.class_path, member),
                Some(expected),
                "{}.{member}",
                row.class_path
            );
        }
        assert_eq!(enumeration_member_value(row.class_path, "Bogus"), None);
    }
    assert_eq!(
        enumeration_member_value("ASHRAE.G36.Types.OperationModes", "unoccupied"),
        Some(MemberValue::Integer(7))
    );
    assert_eq!(
        enumeration_member_value("CDL.Types.SimpleController", "PID"),
        Some(MemberValue::Ordinal(4))
    );
    for id in [0, 5, 100, 111, 999, u32::MAX] {
        assert_eq!(enumeration_class_by_id(EnumClassId(id)), None, "{id}");
        assert_eq!(enumeration_class_path(EnumClassId(id)), None, "{id}");
    }
}

/// Hostile spellings of `real`, a real class path or member name.
fn hostile_spellings(real: &str) -> Vec<String> {
    let mut out = vec![
        real.to_lowercase(),
        real.to_uppercase(),
        format!("{real}."),
        format!(".{real}"),
        format!(" {real}"),
        format!("{real} "),
        format!("{real}\0"),
        format!("{OBC_PACKAGE}{real}"),
        format!("http://example.org#{OBC_PACKAGE}{real}"),
        format!("{real}x"),
        String::new(),
        ".".to_owned(),
    ];
    for cut in 1..real.len() {
        if real.is_char_boundary(cut) {
            out.push(real[..cut].to_owned());
        }
    }
    out.retain(|s| s != real);
    out
}

#[test]
fn relative_path_lookup_refuses_every_hostile_spelling() {
    for row in rows() {
        for hostile in hostile_spellings(row.class_path) {
            assert_eq!(enumeration_class(&hostile), None, "{hostile:?}");
            assert_eq!(enumeration_member_value(&hostile, row.members[0]), None);
        }
        for member in row.members {
            for hostile in hostile_spellings(member) {
                assert!(
                    row.member_value(&hostile).is_none() || row.members.contains(&hostile.as_str()),
                    "{}.{hostile:?}",
                    row.class_path
                );
            }
        }
    }
}

/// Every input the differential test feeds both implementations.
fn probe_paths() -> Vec<String> {
    let mut out = Vec::new();
    for row in rows() {
        for base in [row.class_path, row.qualified_path] {
            out.push(base.to_owned());
            out.push(format!("http://example.org#{base}"));
            out.push(format!("https://example.org/{base}"));
            out.extend(hostile_spellings(base));
        }
        out.push(row.type_name().to_owned());
        out.push(format!("Some.Other.Package.{}", row.type_name()));
    }
    out.extend(
        [
            "",
            ".",
            "#",
            "/",
            "Buildings.Controls.OBC.ASHRAE.G36.Types.",
            "Buildings.Controls.OBC.ASHRAE.G36.Types",
            "Buildings.Controls.OBC.CDL.Types.",
            "Buildings.Controls.OBC.ASHRAE.G36.Types.Bogus",
            "Buildings.Controls.OBC.ASHRAE.G36.Types.SimpleController",
            "Buildings.Controls.OBC.CDL.Types.CoolingCoil",
        ]
        .map(str::to_owned),
    );
    out
}

fn assert_constant_lookups_agree(package: &str, member: &str) {
    assert_eq!(
        g36_integer_constant_in_package(package, member),
        match_arm_oracle::g36_integer_constant_in_package(package, member),
        "{package:?} {member:?}"
    );
    let joined = format!("{package}.{member}");
    assert_eq!(
        g36_integer_constant(&joined),
        match_arm_oracle::g36_integer_constant(&joined),
        "{joined:?}"
    );
}

#[test]
fn table_backed_functions_equal_the_match_arms_on_every_input() {
    let mut members: Vec<String> = rows()
        .iter()
        .flat_map(|r| r.members.iter().flat_map(|m| hostile_spellings(m)))
        .collect();
    members.extend(
        rows()
            .iter()
            .flat_map(|r| r.members.iter().map(|m| (*m).to_owned())),
    );
    members.extend(["Bogus", "", "1", "0"].map(str::to_owned));
    let mut constant_members: Vec<String> = rows()
        .iter()
        .filter(|r| r.kind == EnumerationKind::ConstantPackage)
        .flat_map(|r| r.members.iter().map(|m| (*m).to_owned()))
        .collect();
    constant_members.extend(["Bogus", "", "P", "None"].map(str::to_owned));

    for path in probe_paths() {
        let p = path.as_str();
        assert_eq!(
            enum_class_id(p),
            match_arm_oracle::enum_class_id(p),
            "{p:?}"
        );
        assert_eq!(
            g36_enum_class_id(p),
            match_arm_oracle::g36_enum_class_id(p),
            "{p:?}"
        );
        assert_eq!(is_g36_type_path(p), match_arm_oracle::is_g36_type_path(p));
        assert_eq!(
            is_g36_integer_constant_package(p),
            match_arm_oracle::is_g36_integer_constant_package(p),
            "{p:?}"
        );
        assert_eq!(
            enum_descriptor_by_path(p),
            match_arm_oracle::enum_descriptor_by_path(p),
            "{p:?}"
        );
        assert_eq!(
            g36_integer_constant(p),
            match_arm_oracle::g36_integer_constant(p)
        );
        for member in &constant_members {
            assert_constant_lookups_agree(p, member);
        }
    }
    // Every hostile member spelling, under every real constant package path form.
    for row in rows()
        .iter()
        .filter(|r| r.kind == EnumerationKind::ConstantPackage)
    {
        for p in [
            row.class_path.to_owned(),
            row.qualified_path.to_owned(),
            format!("http://example.org#{}", row.qualified_path),
        ] {
            for member in &members {
                assert_constant_lookups_agree(&p, member);
            }
        }
    }

    for id in (0..=120).chain([999, u32::MAX]) {
        let class = EnumClassId(id);
        assert_eq!(
            enum_descriptor(class),
            match_arm_oracle::enum_descriptor(class)
        );
        assert_eq!(
            g36_enum_literals(class),
            match_arm_oracle::g36_enum_literals(class)
        );
        for member in &members {
            assert_eq!(
                enum_member_ordinal(class, member),
                match_arm_oracle::enum_member_ordinal(class, member),
                "{id} {member:?}"
            );
        }
    }
}

#[test]
fn durable_descriptor_paths_stay_frozen() {
    for row in rows()
        .iter()
        .filter(|r| r.kind == EnumerationKind::Enumeration)
    {
        let descriptor = enum_descriptor(row.id.expect("id")).expect("descriptor");
        let expected = if row.class_path.starts_with("CDL.") {
            row.class_path
        } else {
            row.qualified_path
        };
        assert_eq!(descriptor.class_path, expected);
        assert_eq!(descriptor.members, row.members);
        assert_eq!(enum_descriptor_by_path(expected), Some(descriptor));
    }
}

/// The member names between `enumeration(` and its closing `")`, quoted descriptions skipped.
fn vendored_enumeration_members(source: &str) -> Vec<String> {
    let body = source
        .split_once("enumeration(")
        .expect("an enumeration declaration")
        .1;
    let body = &body[..body.find("\")").expect("closing description") + 1];
    let mut unquoted = String::new();
    let mut in_string = false;
    for c in body.chars() {
        if c == '"' {
            in_string = !in_string;
        } else if !in_string {
            unquoted.push(c);
        }
    }
    unquoted
        .split(',')
        .map(|item| item.trim().to_owned())
        .collect()
}

#[test]
fn g36_enumeration_members_match_the_vendored_source_in_order() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/modelica-buildings-cdl/Buildings/Controls/OBC");
    for name in G36_ENUMERATIONS {
        let row = enumeration_class(&format!("ASHRAE.G36.Types.{name}")).expect("row");
        let file = root.join(format!("{}.mo", row.class_path.replace('.', "/")));
        let source = std::fs::read_to_string(&file).expect("vendored source");
        assert!(
            source.starts_with("within Buildings.Controls.OBC.ASHRAE.G36.Types;"),
            "{name}"
        );
        assert_eq!(vendored_enumeration_members(&source), row.members, "{name}");
    }
}

#[test]
fn every_row_lies_in_a_package_whose_durable_form_is_decided() {
    // `enum_descriptor` picks the durable class-path form by package: relative for
    // `CDL.Types`, fully qualified for `ASHRAE.G36.Types`. A row in any other package would get a
    // form nobody chose, so it must be refused here until one is.
    for row in rows() {
        assert!(
            row.class_path.starts_with("CDL.Types.")
                || row.class_path.starts_with("ASHRAE.G36.Types."),
            "{}: no durable class-path form is decided for this package",
            row.class_path
        );
    }
}

#[test]
fn cdl_rows_agree_with_the_typed_simple_controller_and_zero_time_parsers() {
    use oce_model::{SimpleController, ZeroTime};
    let simple = enumeration_class("CDL.Types.SimpleController").expect("row");
    let typed = [
        SimpleController::P,
        SimpleController::Pi,
        SimpleController::Pd,
        SimpleController::Pid,
    ];
    assert_eq!(simple.members.len(), typed.len());
    for (member, expected) in simple.members.iter().zip(typed) {
        assert_eq!(
            SimpleController::from_qualified(&format!("{}.{member}", simple.qualified_path)),
            Some(expected)
        );
    }
    let zero_time = enumeration_class("CDL.Types.ZeroTime").expect("row");
    assert_eq!(zero_time.members.len(), ZeroTime::MEMBER_COUNT as usize);
    for (index, member) in zero_time.members.iter().enumerate() {
        let ordinal = u32::try_from(index + 1).expect("small");
        assert_eq!(
            ZeroTime::from_member(member).map(ZeroTime::ordinal),
            Some(ordinal),
            "{member}"
        );
    }
}
