//! Class descriptions, the settable flag and the parameter-template spelling (GitHub #440).
//!
//! The per-class values are pinned byte for byte by the `class_ports` golden in `tests/`; these
//! tests assert the properties that golden cannot show on its own: every class is covered exactly
//! once and in registry order, the settable flag follows the reservation and nothing else, every
//! parameter is typed by agreeing evidence, every
//! array a rule names maps to its catalog template, and the template expansion is the import's
//! row-major underscore spelling at its edges.

use alloc::string::String;
use alloc::vec::Vec;

use crate::class_descriptions::CLASS_DESCRIPTIONS;
use oce_model::{EnumClassId, ValueType};

use crate::class_metadata::{parameter_type_evidence, rule_parameters};
use crate::registry::{all_entries, param_defaults};
use crate::{
    DefaultSource, PARAM_COL_PLACEHOLDER, PARAM_INDEX_PLACEHOLDER, PARAM_ROW_PLACEHOLDER,
    ParamNameTemplate, ParamRule, class_description, class_parameters, parameter_value_type,
};

#[test]
fn every_registered_class_has_exactly_one_description_in_registry_order() {
    let registry: Vec<&str> = all_entries().map(|entry| entry.class_path).collect();
    let described: Vec<&str> = CLASS_DESCRIPTIONS.iter().map(|(path, _)| *path).collect();
    assert_eq!(described, registry);
    for (path, description) in CLASS_DESCRIPTIONS {
        assert_eq!(class_description(path), Some(*description));
        assert!(!description.trim().is_empty(), "{path}: empty description");
        assert_eq!(
            description.trim(),
            *description,
            "{path}: padded description"
        );
        assert!(
            !description.contains('\n'),
            "{path}: multi-line description"
        );
    }
}

#[test]
fn unregistered_paths_have_no_metadata() {
    for path in [
        "",
        "CDL.Reals.NoSuchBlock",
        "Buildings.Controls.OBC.CDL.Reals.Add",
        "cdl.reals.add",
        "CDL.Reals.Add ",
    ] {
        assert_eq!(class_description(path), None, "{path:?}");
        assert_eq!(class_parameters(path), None, "{path:?}");
    }
}

#[test]
fn defaults_come_first_in_order_with_their_source_and_settable_follows_reservation_only() {
    for entry in all_entries() {
        let reserved = entry.class_path.starts_with("urn:oce:lowering#");
        let parameters = class_parameters(entry.class_path).expect("registered");
        let defaults = param_defaults(entry.class_path);
        // Defaults come first, in declaration order, one parameter each, carrying their source.
        assert!(parameters.len() >= defaults.len(), "{}", entry.class_path);
        for (parameter, default) in parameters.iter().zip(defaults) {
            assert_eq!(parameter.name, default.name);
            assert_eq!(parameter.default, default.default);
        }
        // Rule-only parameters have no default: the author must assign them.
        for parameter in &parameters[defaults.len()..] {
            assert_eq!(
                parameter.default,
                DefaultSource::Required,
                "{}",
                parameter.name
            );
        }
        // Settable is the reservation and nothing else (GitHub #440 amendment, 2026-11-08).
        for parameter in &parameters {
            assert_eq!(
                parameter.settable, !reserved,
                "{}.{}",
                entry.class_path, parameter.name
            );
        }
        let mut names: Vec<&str> = parameters.iter().map(|p| p.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            parameters.len(),
            "{}: duplicate",
            entry.class_path
        );
    }
}

#[test]
fn every_derived_default_is_settable_and_only_reserved_identities_are_not() {
    let mut derived: Vec<String> = Vec::new();
    for entry in all_entries() {
        for parameter in class_parameters(entry.class_path).expect("registered") {
            if matches!(parameter.default, DefaultSource::Derived { .. }) {
                assert!(
                    parameter.settable,
                    "{}.{}",
                    entry.class_path, parameter.name
                );
                derived.push(alloc::format!("{}.{}", entry.class_path, parameter.name));
            }
            if !parameter.settable {
                assert!(
                    entry.class_path.starts_with("urn:oce:lowering#"),
                    "{}.{} is unsettable but not reserved",
                    entry.class_path,
                    parameter.name
                );
            }
        }
    }
    // The twelve derived defaults in the registry today, every one assignable upstream; a new one
    // is a conscious change here.
    assert_eq!(
        derived,
        [
            "CDL.Reals.MatrixGain.K_<row>_<col>",
            "CDL.Reals.LimitSlewRate.fallingSlewRate",
            "CDL.Reals.LimitSlewRate.Td",
            "CDL.Reals.Ramp.fallingSlewRate",
            "CDL.Reals.Ramp.Td",
            "CDL.Reals.PIDWithReset.y_reset",
            "CDL.Logical.VariablePulse.minTruFalHol",
            "CDL.Logical.TrueFalseHold.falseHoldDuration",
            "CDL.Integers.Stage.h",
            "CDL.Routing.BooleanExtractSignal.extract_<i>",
            "CDL.Routing.IntegerExtractSignal.extract_<i>",
            "CDL.Routing.RealExtractSignal.extract_<i>",
        ]
        .map(String::from)
        .to_vec()
    );
    // The reserved identities declare no parameters today, so `settable == false` is vacuous for
    // them; the flag is still computed from the reservation, which the test above pins.
    for entry in all_entries().filter(|e| e.class_path.starts_with("urn:oce:lowering#")) {
        assert!(
            class_parameters(entry.class_path)
                .expect("registered")
                .is_empty()
        );
    }
}

#[test]
fn every_parameter_has_one_agreed_type() {
    for entry in all_entries() {
        for parameter in class_parameters(entry.class_path).expect("registered") {
            let defaults = param_defaults(entry.class_path);
            let default = defaults
                .iter()
                .find(|d| d.name == parameter.name)
                .map(|d| d.default);
            let evidence = parameter_type_evidence(entry, parameter.name, default);
            assert!(
                !evidence.is_empty(),
                "{}.{}: no evidence types this parameter",
                entry.class_path,
                parameter.name
            );
            assert!(
                evidence.iter().all(|t| *t == evidence[0]),
                "{}.{}: conflicting types {evidence:?}",
                entry.class_path,
                parameter.name
            );
            assert_eq!(parameter.value_type, evidence[0]);
        }
    }
}

#[test]
fn spot_checked_types_match_the_cdl_declarations() {
    let type_of = |class: &str, name: &str| {
        class_parameters(class)
            .unwrap()
            .into_iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("{class}.{name}"))
            .value_type
    };
    assert_eq!(type_of("CDL.Reals.MultiSum", "nin"), ValueType::Integer);
    assert_eq!(type_of("CDL.Reals.MultiSum", "k_<i>"), ValueType::Real);
    assert_eq!(
        type_of("CDL.Integers.MultiSum", "k_<i>"),
        ValueType::Integer
    );
    assert_eq!(
        type_of("CDL.Reals.MatrixGain", "K_<row>_<col>"),
        ValueType::Real
    );
    assert_eq!(type_of("CDL.Reals.MatrixMax", "rowMax"), ValueType::Boolean);
    assert_eq!(
        type_of("CDL.Routing.RealVectorFilter", "msk_<i>"),
        ValueType::Boolean
    );
    assert_eq!(
        type_of("CDL.Routing.RealExtractSignal", "extract_<i>"),
        ValueType::Integer
    );
    assert_eq!(
        type_of("CDL.Reals.Sources.TimeTable", "table_<row>_<col>"),
        ValueType::Real
    );
    assert_eq!(
        type_of("CDL.Reals.Sources.TimeTable", "extrapolation"),
        ValueType::Enum(EnumClassId::EXTRAPOLATION)
    );
    assert_eq!(
        type_of("CDL.Reals.Sources.TimeTable", "smoothness"),
        ValueType::Enum(EnumClassId::SMOOTHNESS)
    );
    assert_eq!(
        type_of("CDL.Reals.PID", "controllerType"),
        ValueType::Enum(EnumClassId::SIMPLE_CONTROLLER)
    );
    assert_eq!(
        type_of("CDL.Reals.Sources.CalendarTime", "zerTim"),
        ValueType::Enum(EnumClassId::ZERO_TIME)
    );
    assert_eq!(
        type_of("CDL.Utilities.Assert", "message"),
        ValueType::String
    );
    assert_eq!(
        type_of("CDL.Reals.PIDWithReset", "y_reset"),
        ValueType::Real
    );
}

#[test]
fn every_array_a_rule_names_maps_to_its_catalog_template() {
    for entry in all_entries() {
        for rule in entry.param_rules() {
            let bases: &[(&str, usize)] = match *rule {
                ParamRule::StructuralArrayElements { base }
                | ParamRule::IntegerArrayElements { base, .. }
                | ParamRule::IntegerArrayElementsInRange { base, .. }
                | ParamRule::RealArrayElements { base, .. }
                | ParamRule::BooleanArrayElements { base, .. }
                | ParamRule::BooleanArrayTrueCountEquals { base, .. }
                | ParamRule::TimeTableOffset { base, .. } => &[(base, 1)],
                ParamRule::RealMatrixElements { base, .. }
                | ParamRule::TimeTableMatrix { base, .. } => &[(base, 2)],
                _ => &[],
            };
            let named = rule_parameters(rule);
            for (base, rank) in bases {
                let template = named
                    .iter()
                    .flatten()
                    .map(|(name, _)| ParamNameTemplate::parse(name))
                    .find(|template| template.base() == *base && template.rank() == *rank);
                assert!(
                    template.is_some(),
                    "{}: rule array `{base}` has no catalog template",
                    entry.class_path
                );
            }
        }
    }
}

#[test]
fn every_catalog_template_parses_to_an_array_and_every_plain_name_to_a_scalar() {
    for entry in all_entries() {
        for parameter in class_parameters(entry.class_path).expect("registered") {
            let template = ParamNameTemplate::parse(parameter.name);
            let has_placeholder = parameter.name.contains('<');
            assert_eq!(template.rank() > 0, has_placeholder, "{}", parameter.name);
            if !has_placeholder {
                assert_eq!(template, ParamNameTemplate::Scalar(parameter.name));
            }
        }
    }
}

#[test]
fn template_placeholders_are_the_catalog_spellings() {
    assert_eq!(PARAM_INDEX_PLACEHOLDER, "<i>");
    assert_eq!(PARAM_ROW_PLACEHOLDER, "<row>");
    assert_eq!(PARAM_COL_PLACEHOLDER, "<col>");
    let vector = alloc::format!("k_{PARAM_INDEX_PLACEHOLDER}");
    let matrix = alloc::format!("K_{PARAM_ROW_PLACEHOLDER}_{PARAM_COL_PLACEHOLDER}");
    assert_eq!(
        ParamNameTemplate::parse(&vector),
        ParamNameTemplate::Vector { base: "k" }
    );
    assert_eq!(
        ParamNameTemplate::parse(&matrix),
        ParamNameTemplate::Matrix { base: "K" }
    );
}

#[test]
fn template_expansion_is_one_based_row_major_and_refuses_bad_indices() {
    let vector = ParamNameTemplate::parse("k_<i>");
    let matrix = ParamNameTemplate::parse("K_<row>_<col>");
    let scalar = ParamNameTemplate::parse("nin");
    assert_eq!(vector.element_name(&[1]).as_deref(), Some("k_1"));
    assert_eq!(
        vector.element_name(&[1_048_576]).as_deref(),
        Some("k_1048576")
    );
    assert_eq!(
        vector.element_name(&[usize::MAX]),
        Some(alloc::format!("k_{}", usize::MAX))
    );
    assert_eq!(matrix.element_name(&[2, 1]).as_deref(), Some("K_2_1"));
    assert_eq!(matrix.element_name(&[1, 12]).as_deref(), Some("K_1_12"));
    assert_eq!(scalar.element_name(&[]).as_deref(), Some("nin"));
    // Wrong rank and zero indices are refused, never spelled.
    assert_eq!(vector.element_name(&[]), None);
    assert_eq!(vector.element_name(&[1, 1]), None);
    assert_eq!(vector.element_name(&[0]), None);
    assert_eq!(matrix.element_name(&[1]), None);
    assert_eq!(matrix.element_name(&[0, 1]), None);
    assert_eq!(matrix.element_name(&[1, 0]), None);
    assert_eq!(scalar.element_name(&[1]), None);
}

#[test]
fn malformed_templates_are_scalar_names() {
    for name in [
        "_<i>",
        "_<row>_<col>",
        "<i>",
        "k<i>",
        "k_<i>_",
        "k_<j>",
        "K_<col>_<row>",
        "K_<row>",
        "",
    ] {
        assert_eq!(
            ParamNameTemplate::parse(name),
            ParamNameTemplate::Scalar(name),
            "{name:?}"
        );
    }
    // A matrix suffix wins over the vector suffix it does not contain.
    assert_eq!(
        ParamNameTemplate::parse("a_<i>_<row>_<col>"),
        ParamNameTemplate::Matrix { base: "a_<i>" }
    );
}

#[test]
fn a_bound_name_is_typed_as_the_catalog_types_its_parameter_for_every_class() {
    // The oracle is the allocating listing: every scalar under its own name, and every array
    // template under element names at the first index, a multi-digit index and the largest one.
    let mut typed = 0usize;
    for entry in all_entries() {
        for parameter in class_parameters(entry.class_path).expect("registered") {
            let template = ParamNameTemplate::parse(parameter.name);
            let indices: &[&[usize]] = match template.rank() {
                0 => &[&[]],
                1 => &[&[1], &[10], &[usize::MAX]],
                _ => &[&[1, 1], &[2, 37], &[usize::MAX, usize::MAX]],
            };
            for index in indices {
                let name = template.element_name(index).expect("valid indices");
                assert_eq!(
                    parameter_value_type(entry.class_path, &name),
                    Some(parameter.value_type),
                    "{} {name}",
                    entry.class_path
                );
                typed += 1;
            }
            // The template text itself is not a name a model binds.
            if template.rank() > 0 {
                assert_eq!(parameter_value_type(entry.class_path, parameter.name), None);
            }
        }
    }
    assert!(typed > 0);
}

#[test]
fn a_name_the_class_does_not_declare_has_no_catalog_type() {
    assert_eq!(parameter_value_type("CDL.Reals.NoSuchClass", "k"), None);
    assert_eq!(parameter_value_type("", "k"), None);
    for (class_path, name) in [
        ("CDL.Reals.Hysteresis", ""),
        ("CDL.Reals.Hysteresis", "ulow"),
        ("CDL.Reals.Hysteresis", "uLow_1"),
        ("CDL.Reals.Hysteresis", "uLow "),
        ("CDL.Reals.MultiSum", "k"),
        ("CDL.Reals.MultiSum", "k_"),
        ("CDL.Reals.MultiSum", "k_0"),
        ("CDL.Reals.MultiSum", "k_01"),
        ("CDL.Reals.MultiSum", "k_+1"),
        ("CDL.Reals.MultiSum", "k_-1"),
        ("CDL.Reals.MultiSum", "k_1 "),
        ("CDL.Reals.MultiSum", "k_1_1"),
        ("CDL.Reals.MultiSum", "k_\u{0661}"),
        ("CDL.Reals.MatrixGain", "K_1"),
        ("CDL.Reals.MatrixGain", "K_1_"),
        ("CDL.Reals.MatrixGain", "K__1"),
        ("CDL.Reals.MatrixGain", "K_1_0"),
        ("CDL.Reals.MatrixGain", "K_1_1_1"),
        ("CDL.Reals.MatrixGain", "k_1_1"),
    ] {
        assert_eq!(
            parameter_value_type(class_path, name),
            None,
            "{class_path} {name:?}"
        );
    }
}

#[test]
fn the_catalog_type_of_a_bound_name_is_the_parameters_own_not_its_neighbours() {
    use ValueType::{Boolean, Integer, Real};
    for (class_path, name, expected) in [
        ("CDL.Reals.Hysteresis", "uLow", Real),
        ("CDL.Reals.Hysteresis", "pre_y_start", Boolean),
        ("CDL.Reals.MultiSum", "nin", Integer),
        ("CDL.Reals.MultiSum", "k_3", Real),
        ("CDL.Integers.MultiSum", "k_3", Integer),
        ("CDL.Integers.Sources.Constant", "k", Integer),
        ("CDL.Reals.Sources.Constant", "k", Real),
        ("CDL.Reals.MatrixGain", "K_2_1", Real),
        ("CDL.Integers.Sources.TimeTable", "table_1_2", Real),
        ("CDL.Integers.Sources.Pulse", "amplitude", Integer),
        ("CDL.Integers.Sources.Pulse", "period", Real),
        ("CDL.Reals.LimitSlewRate", "fallingSlewRate", Real),
        (
            "CDL.Reals.PID",
            "controllerType",
            ValueType::Enum(EnumClassId::SIMPLE_CONTROLLER),
        ),
    ] {
        assert_eq!(
            parameter_value_type(class_path, name),
            Some(expected),
            "{class_path} {name}"
        );
    } // `MatrixGain`'s counts are `K`'s shape, protected upstream: no catalog type (issue #773).
    for name in ["nin", "nout"] {
        assert_eq!(
            parameter_value_type("CDL.Reals.MatrixGain", name),
            None,
            "{name}"
        );
    }
}

#[test]
fn a_template_covers_exactly_the_canonical_element_names_of_its_array() {
    let scalar = ParamNameTemplate::parse("k");
    assert!(scalar.names("k"));
    assert!(!scalar.names("k_1") && !scalar.names("") && !scalar.names("K"));

    let vector = ParamNameTemplate::parse("k_<i>");
    for name in ["k_1", "k_9", "k_10", "k_18446744073709551616"] {
        assert!(vector.names(name), "{name}");
    }
    for name in [
        "k", "k_", "k_0", "k_01", "k_+1", "k_-1", "k_1_1", "k_1a", "k_ 1", "kk_1", "K_1", "_1", "",
    ] {
        assert!(!vector.names(name), "{name:?}");
    }

    let matrix = ParamNameTemplate::parse("table_<row>_<col>");
    for name in ["table_1_1", "table_12_3", "table_3_12"] {
        assert!(matrix.names(name), "{name}");
    }
    for name in [
        "table",
        "table_1",
        "table_1_",
        "table__1",
        "table_0_1",
        "table_1_0",
        "table_01_1",
        "table_1_1_1",
        "table_1_x",
        "tables_1_1",
    ] {
        assert!(!matrix.names(name), "{name:?}");
    }
}
