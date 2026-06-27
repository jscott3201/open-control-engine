//! Test-only helpers for the checked-in ASHRAE G36 sequence catalog guard.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::dto::CxfValue;
use crate::{OneOrMany, bridge};

pub(super) use crate::g36_catalog_guard_data::*;
pub(super) use crate::g36_catalog_guard_helpers::{
    array_field, catalog_fingerprint, parse, remove_path_entry, remove_string, str_field,
    string_set,
};
use crate::g36_catalog_guard_helpers::{
    assert_usize_field, bool_field, constant_packages, enum_literals, jsonld_fragment,
    package_order, parameter_names, parse_cxf, string_vec,
};
const G36_TYPES_PREFIX: &str = "Buildings.Controls.OBC.ASHRAE.G36.Types.";

pub(super) fn validate_g36_catalog(
    catalog: &Value,
    prov: &Value,
    runtime_fixtures: &[FixtureSource],
    profile_fixtures: &[FixtureSource],
) -> Vec<String> {
    let mut errors = validate_provenance(prov);
    validate_package_orders(catalog, &mut errors);
    validate_top_level_packages(catalog, &mut errors);
    validate_runtime_sequences(catalog, &mut errors);
    validate_initial_paths(catalog, &mut errors);
    validate_type_registry(catalog, &mut errors);
    validate_fixture_records(catalog, runtime_fixtures, &mut errors);
    validate_profile_fixture_records(catalog, profile_fixtures, &mut errors);

    let enum_literals = enum_literals(catalog);
    let constant_packages = constant_packages(catalog);
    for fixture in runtime_fixtures {
        validate_runtime_fixture_shape(catalog, fixture, &mut errors);
        validate_fixture_cdl_types(fixture, &mut errors);
        validate_g36_literals_in_fixture(fixture, &enum_literals, &constant_packages, &mut errors);
    }
    for fixture in profile_fixtures {
        validate_fixture_cdl_types(fixture, &mut errors);
        validate_g36_literals_in_fixture(fixture, &enum_literals, &constant_packages, &mut errors);
        validate_conditional_guards(fixture, &enum_literals, &constant_packages, &mut errors);
    }

    errors
}

fn validate_provenance(prov: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    if str_field(prov, "repository") != "https://github.com/lbl-srg/modelica-buildings" {
        errors.push("invalid-provenance-repository".to_owned());
    }
    if str_field(prov, "branch") != "master" {
        errors.push("invalid-provenance-branch".to_owned());
    }
    if str_field(prov, "commit") != EXPECTED_REFERENCE_COMMIT {
        errors.push("invalid-provenance-commit".to_owned());
    }
    if str_field(prov, "fetched_at") != "2026-06-27" {
        errors.push("invalid-provenance-date".to_owned());
    }
    if str_field(prov, "catalog_fingerprint") != EXPECTED_CATALOG_FINGERPRINT
        || catalog_fingerprint(CATALOG_JSON.as_bytes()) != EXPECTED_CATALOG_FINGERPRINT
    {
        errors.push("invalid-provenance-fingerprint".to_owned());
    }

    assert_sources(
        prov,
        "package_order_files",
        EXPECTED_PACKAGE_ORDER_FILES,
        "missing-package-order-source",
        &mut errors,
    );
    assert_sources(
        prov,
        "sequence_source_files",
        EXPECTED_SEQUENCE_SOURCE_FILES,
        "missing-sequence-source",
        &mut errors,
    );
    assert_sources(
        prov,
        "type_source_files",
        EXPECTED_TYPE_SOURCE_FILES,
        "missing-type-source",
        &mut errors,
    );
    errors
}

fn assert_sources(
    prov: &Value,
    field: &str,
    expected: &[&str],
    tag: &str,
    errors: &mut Vec<String>,
) {
    let actual = string_set(array_field(prov, field));
    for source in expected {
        if !actual.contains(*source) {
            errors.push(format!("{tag}: {source}"));
        }
    }
}

fn validate_package_orders(catalog: &Value, errors: &mut Vec<String>) {
    let expected: &[(&str, &[&str])] = &[
        (
            "Buildings.Controls.OBC.ASHRAE.G36",
            &[
                "AHUs",
                "FanCoilUnits",
                "Generic",
                "TerminalUnits",
                "ThermalZones",
                "VentilationZones",
                "ZoneGroups",
                "Types",
            ],
        ),
        (
            "Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV",
            &["Controller", "Economizers", "SetPoints", "Validation"],
        ),
        (
            "Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints",
            &[
                "FreezeProtection",
                "PlantRequests",
                "ReliefDamper",
                "ReliefFan",
                "ReliefFanGroup",
                "ReturnFanAirflowTracking",
                "ReturnFanDirectPressure",
                "SupplyFan",
                "SupplySignals",
                "SupplyTemperature",
                "OutdoorAirFlow",
                "Validation",
            ],
        ),
        (
            "Buildings.Controls.OBC.ASHRAE.G36.Types",
            &[
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
                "DemandLimitLevels",
                "FreezeProtectionStages",
                "OperationModes",
                "ZoneStates",
            ],
        ),
    ];

    for (package, entries) in expected {
        let Some(entry) = package_order(catalog, package) else {
            errors.push(format!("missing-package-order: {package}"));
            continue;
        };
        let actual = string_vec(array_field(entry, "entries"));
        let expected = entries.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        if actual != expected {
            errors.push(format!("stale-package-order: {package}"));
        }
    }
}

fn validate_top_level_packages(catalog: &Value, errors: &mut Vec<String>) {
    let entries = array_field(catalog, "top_level_packages");
    let status = |path: &str| {
        entries
            .iter()
            .find(|entry| str_field(entry, "class_path") == path)
            .map(|entry| str_field(entry, "status"))
            .unwrap_or("")
    };
    if status("Buildings.Controls.OBC.ASHRAE.G36.AHUs") != "in-progress" {
        errors.push("invalid-top-level-status: Buildings.Controls.OBC.ASHRAE.G36.AHUs".to_owned());
    }
    if status("Buildings.Controls.OBC.ASHRAE.G36.Types") != "structural-type" {
        errors.push("invalid-top-level-status: Buildings.Controls.OBC.ASHRAE.G36.Types".to_owned());
    }
    for path in [
        "Buildings.Controls.OBC.ASHRAE.G36.FanCoilUnits",
        "Buildings.Controls.OBC.ASHRAE.G36.Generic",
        "Buildings.Controls.OBC.ASHRAE.G36.TerminalUnits",
        "Buildings.Controls.OBC.ASHRAE.G36.ThermalZones",
        "Buildings.Controls.OBC.ASHRAE.G36.VentilationZones",
        "Buildings.Controls.OBC.ASHRAE.G36.ZoneGroups",
    ] {
        if status(path) != "deferred" {
            errors.push(format!("invalid-top-level-status: {path}"));
        }
    }
}

fn validate_runtime_sequences(catalog: &Value, errors: &mut Vec<String>) {
    for entry in array_field(catalog, "runtime_sequences") {
        let class_path = str_field(entry, "class_path");
        if class_path.contains(".Validation") {
            errors.push(format!(
                "validation-path-marked-runtime-supported: {class_path}"
            ));
        }
        if str_field(entry, "status") != "supported-runtime-sequence" {
            errors.push(format!(
                "runtime-sequence-with-invalid-status: {class_path}"
            ));
        }
        for required in [
            "fixture",
            "source",
            "supported_variant",
            "golden_trace",
            "oracle_reference",
        ] {
            if str_field(entry, required).is_empty() {
                errors.push(format!(
                    "runtime-sequence-missing-evidence: {class_path}:{required}"
                ));
            }
        }
    }
}

fn validate_initial_paths(catalog: &Value, errors: &mut Vec<String>) {
    let mut validation_count = 0;
    for entry in array_field(catalog, "initial_vav_paths") {
        let class_path = str_field(entry, "class_path");
        let status = str_field(entry, "status");
        if class_path.contains(".Validation") {
            validation_count += 1;
            if status != "validation-only" || bool_field(entry, "runtime_support") {
                errors.push(format!(
                    "validation-path-marked-runtime-supported: {class_path}"
                ));
            }
        }
        if bool_field(entry, "runtime_support") && status != "supported-runtime-sequence" {
            errors.push(format!(
                "runtime-support-without-supported-status: {class_path}"
            ));
        }
        if status == "supported-runtime-sequence" {
            errors.push(format!(
                "unexpected-supported-runtime-sequence: {class_path}"
            ));
        }
    }
    if validation_count < 3 {
        errors.push(format!("validation-path-count: {validation_count}"));
    }
}

fn validate_type_registry(catalog: &Value, errors: &mut Vec<String>) {
    let enums = array_field(&catalog["g36_types"], "enumerations");
    if enums.len() != 10 {
        errors.push(format!("g36-enum-count: {}", enums.len()));
    }
    for expected_enum in [
        (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard",
            &["ASHRAE62_1", "California_Title_24"][..],
        ),
        (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.ControlEconomizer",
            &[
                "FixedDryBulb",
                "DifferentialDryBulb",
                "FixedDryBulbWithDifferentialDryBulb",
                "FixedEnthalpyWithFixedDryBulb",
                "DifferentialEnthalpyWithFixedDryBulb",
            ][..],
        ),
        (
            "Buildings.Controls.OBC.ASHRAE.G36.Types.PressureControl",
            &[
                "BarometricRelief",
                "ReliefDamper",
                "ReliefFan",
                "ReturnFanMeasuredAir",
                "ReturnFanDp",
            ][..],
        ),
    ] {
        let actual = enum_literals(catalog)
            .get(expected_enum.0)
            .cloned()
            .unwrap_or_default();
        let expected = expected_enum
            .1
            .iter()
            .map(|s| (*s).to_owned())
            .collect::<BTreeSet<_>>();
        if actual != expected {
            errors.push(format!("stale-g36-enum: {}", expected_enum.0));
        }
    }
    let constants = constant_packages(catalog);
    for package in [
        "Buildings.Controls.OBC.ASHRAE.G36.Types.DemandLimitLevels",
        "Buildings.Controls.OBC.ASHRAE.G36.Types.FreezeProtectionStages",
        "Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes",
        "Buildings.Controls.OBC.ASHRAE.G36.Types.ZoneStates",
    ] {
        if !constants.contains_key(package) {
            errors.push(format!("missing-g36-constant-package: {package}"));
        }
    }
    if constants
        .get("Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes")
        .and_then(|values| values.get("occupied"))
        != Some(&1)
    {
        errors.push("stale-g36-constant: OperationModes.occupied".to_owned());
    }
}

fn validate_fixture_records(
    catalog: &Value,
    runtime_fixtures: &[FixtureSource],
    errors: &mut Vec<String>,
) {
    let entries = array_field(catalog, "fixture_only_sequences");
    let catalog_paths = entries
        .iter()
        .map(|entry| str_field(entry, "fixture").to_owned())
        .collect::<BTreeSet<_>>();
    let expected_paths = runtime_fixtures
        .iter()
        .map(|fixture| fixture.path.to_owned())
        .collect::<BTreeSet<_>>();
    if catalog_paths != expected_paths {
        errors.push("fixture-path-set-drift".to_owned());
    }

    for fixture in runtime_fixtures {
        let Some(entry) = entries
            .iter()
            .find(|entry| str_field(entry, "fixture") == fixture.path)
        else {
            errors.push(format!("fixture-missing-catalog-record: {}", fixture.path));
            continue;
        };
        if str_field(entry, "status") != "supported-fixture-only" {
            errors.push(format!("fixture-invalid-status: {}", fixture.name));
        }
        if str_field(entry, "canonical_g36_class_path_status") != "unknown-pending-source-review" {
            errors.push(format!(
                "fixture-overclaims-canonical-source: {}",
                fixture.name
            ));
        }
        if str_field(entry, "golden_trace").is_empty()
            || str_field(entry, "oracle_reference").is_empty()
            || str_field(entry, "oracle_test").is_empty()
        {
            errors.push(format!("fixture-missing-evidence: {}", fixture.name));
        }
        if fixture.name == "vav_single_zone" && str_field(entry, "oracle_status") != "partial" {
            errors.push("vav-fixture-oracle-status-not-partial".to_owned());
        }
    }
}

fn validate_profile_fixture_records(
    catalog: &Value,
    profile_fixtures: &[FixtureSource],
    errors: &mut Vec<String>,
) {
    let entries = array_field(catalog, "profile_fixtures");
    let catalog_paths = entries
        .iter()
        .map(|entry| str_field(entry, "path").to_owned())
        .collect::<BTreeSet<_>>();
    let expected_paths = profile_fixtures
        .iter()
        .map(|fixture| fixture.path.to_owned())
        .collect::<BTreeSet<_>>();
    if catalog_paths != expected_paths {
        errors.push("profile-fixture-path-set-drift".to_owned());
    }
    for entry in entries {
        if bool_field(entry, "runtime_expected") {
            errors.push(format!(
                "profile-fixture-marked-runtime: {}",
                str_field(entry, "name")
            ));
        }
    }
}

fn validate_runtime_fixture_shape(
    catalog: &Value,
    fixture: &FixtureSource,
    errors: &mut Vec<String>,
) {
    let document = parse_cxf(fixture, errors);
    let Some(entry) = array_field(catalog, "fixture_only_sequences")
        .iter()
        .find(|entry| str_field(entry, "fixture") == fixture.path)
    else {
        return;
    };
    let Some(top) = document
        .graph
        .iter()
        .find(|node| node.id == str_field(entry, "top_id"))
    else {
        errors.push(format!("fixture-top-id-missing: {}", fixture.name));
        return;
    };
    assert_usize_field(
        top.contains_block.len(),
        entry,
        "child_block_count",
        fixture,
        errors,
    );
    assert_usize_field(top.has_input.len(), entry, "input_count", fixture, errors);
    assert_usize_field(top.has_output.len(), entry, "output_count", fixture, errors);
}

fn validate_fixture_cdl_types(fixture: &FixtureSource, errors: &mut Vec<String>) {
    let document = parse_cxf(fixture, errors);
    for node in &document.graph {
        for type_iri in node.r#type.as_ref().map(OneOrMany::as_slice).unwrap_or(&[]) {
            if type_iri.starts_with("S231:") || type_iri.starts_with("http://data.ashrae.org/") {
                continue;
            }
            let class_path = bridge::class_path_of(type_iri);
            if class_path.starts_with("CDL.") {
                if oce_blocks::lookup(class_path).is_none() {
                    errors.push(format!(
                        "fixture-cdl-type-unregistered: {}:{class_path}",
                        fixture.name
                    ));
                }
            } else if class_path.starts_with("Buildings.Controls.OBC.ASHRAE.G36.") {
                errors.push(format!(
                    "fixture-contains-canonical-g36-runtime-type: {}:{class_path}",
                    fixture.name
                ));
            } else {
                errors.push(format!(
                    "fixture-unknown-runtime-type: {}:{class_path}",
                    fixture.name
                ));
            }
        }
    }
}

fn validate_g36_literals_in_fixture(
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

fn validate_conditional_guards(
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

pub(super) fn assert_validation_error(catalog: &Value, prov: &Value, expected: &str) {
    let errors = validate_g36_catalog(catalog, prov, RUNTIME_FIXTURES, PROFILE_FIXTURES);
    assert!(
        errors.iter().any(|err| err == expected),
        "expected `{expected}` in errors:\n{}",
        errors.join("\n")
    );
}
