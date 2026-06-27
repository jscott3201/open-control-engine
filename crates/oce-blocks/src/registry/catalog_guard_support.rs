//! Shared helpers for the checked-in Buildings CDL catalog guard.

use std::collections::BTreeSet;

use serde_json::Value;

use super::CATALOG as REGISTRY_CATALOG;

pub(super) const CATALOG_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/reference-catalog/Buildings.Controls.OBC.CDL.catalog.json"
));
pub(super) const PROV_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/reference-catalog/Buildings.Controls.OBC.CDL.prov.json"
));
pub(super) const GOLDEN_MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/golden-gen/goldens/MANIFEST.txt"
));

pub(super) const EXPECTED_REFERENCE_COMMIT: &str = "a131864e4c4df22ebcd52bb8da439de0087ac365";
pub(super) const EXPECTED_CATALOG_FINGERPRINT: &str = "cbe3ba4d306be9b5";
pub(super) const EXPECTED_PACKAGE_ORDER_FILES: &[&str] = &[
    "Buildings/Controls/OBC/CDL/package.order",
    "Buildings/Controls/OBC/CDL/Conversions/package.order",
    "Buildings/Controls/OBC/CDL/Discrete/package.order",
    "Buildings/Controls/OBC/CDL/Integers/package.order",
    "Buildings/Controls/OBC/CDL/Integers/Sources/package.order",
    "Buildings/Controls/OBC/CDL/Logical/package.order",
    "Buildings/Controls/OBC/CDL/Logical/Sources/package.order",
    "Buildings/Controls/OBC/CDL/Psychrometrics/package.order",
    "Buildings/Controls/OBC/CDL/Reals/package.order",
    "Buildings/Controls/OBC/CDL/Reals/Sources/package.order",
    "Buildings/Controls/OBC/CDL/Routing/package.order",
    "Buildings/Controls/OBC/CDL/Utilities/package.order",
    "Buildings/Controls/OBC/CDL/Types/package.order",
    "Buildings/Controls/OBC/CDL/Interfaces/package.order",
];
pub(super) const EXPECTED_STRUCTURAL_SOURCE_FILES: &[&str] = &[
    "Buildings/Controls/OBC/CDL/Constants.mo",
    "Buildings/Controls/OBC/CDL/Types/SimpleController.mo",
    "Buildings/Controls/OBC/CDL/Types/Extrapolation.mo",
    "Buildings/Controls/OBC/CDL/Types/Smoothness.mo",
    "Buildings/Controls/OBC/CDL/Types/ZeroTime.mo",
    "Buildings/Controls/OBC/CDL/Interfaces/BooleanInput.mo",
    "Buildings/Controls/OBC/CDL/Interfaces/BooleanOutput.mo",
    "Buildings/Controls/OBC/CDL/Interfaces/IntegerInput.mo",
    "Buildings/Controls/OBC/CDL/Interfaces/IntegerOutput.mo",
    "Buildings/Controls/OBC/CDL/Interfaces/RealInput.mo",
    "Buildings/Controls/OBC/CDL/Interfaces/RealOutput.mo",
];
pub(super) const EXPECTED_DRIFT_SOURCE_FILES: &[&str] =
    &["Buildings/Resources/Scripts/Conversion/ConvertBuildings_from_10_to_11.0.0.mos"];
pub(super) const EXPECTED_EXTERNAL_SOURCE_FILES: &[&str] = &[
    "Buildings/package.mo",
    "Modelica/Constants.mo",
    "ModelicaServices/package.mo",
];
const EXPECTED_MODELICA_COMMIT: &str = "8ae3d35c24e519cb2996cab20f3b13daf2b0c50a";

#[derive(Debug)]
struct RuntimeClass {
    class_path: String,
    status: String,
    evidence: String,
}

pub(super) fn validate_catalog(
    catalog: &Value,
    prov: &Value,
    manifest: &str,
    registry_paths: &BTreeSet<String>,
) -> Vec<String> {
    let mut errors = validate_provenance(prov);
    let runtime = runtime_classes(catalog, &mut errors);
    let runtime_paths = runtime
        .iter()
        .map(|entry| entry.class_path.clone())
        .collect::<BTreeSet<_>>();
    let implemented = runtime
        .iter()
        .filter(|entry| entry.status == "implemented")
        .map(|entry| entry.class_path.clone())
        .collect::<BTreeSet<_>>();
    let deferred_oracles = deferred_oracle_paths(catalog);
    let extensions = extension_paths(catalog);

    if runtime.len() != 132 {
        errors.push(format!("runtime-count: {}", runtime.len()));
    }
    let missing_count = runtime
        .iter()
        .filter(|entry| entry.status == "missing")
        .count();
    if implemented.len() != 125 {
        errors.push(format!(
            "implemented-reference-count: {}",
            implemented.len()
        ));
    }
    if missing_count != 7 {
        errors.push(format!("missing-reference-count: {missing_count}"));
    }

    for path in registry_paths {
        if !implemented.contains(path) && !extensions.contains(path) {
            errors.push(format!("registry-class-not-in-catalog: {path}"));
        }
    }
    for path in &implemented {
        if !registry_paths.contains(path) {
            errors.push(format!("implemented-class-without-registry-entry: {path}"));
        }
    }

    for entry in runtime.iter().filter(|entry| entry.status == "implemented") {
        match entry.evidence.as_str() {
            "golden_manifest" => {
                if !manifest_mentions(manifest, &entry.class_path) {
                    errors.push(format!("stale-generated-manifest: {}", entry.class_path));
                }
            }
            "legacy_evidence_exemption" => {}
            _ => errors.push(format!(
                "implemented-class-without-golden-or-exemption: {}",
                entry.class_path
            )),
        }
    }

    let structural_manifests = ["CDL.Constants".to_owned(), "CDL.Types".to_owned()]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let classified_manifest_classes = implemented
        .union(&extensions)
        .cloned()
        .collect::<BTreeSet<_>>()
        .union(&deferred_oracles)
        .cloned()
        .collect::<BTreeSet<_>>()
        .union(&structural_manifests)
        .cloned()
        .collect::<BTreeSet<_>>();
    for class_path in manifest_class_paths(manifest) {
        if !classified_manifest_classes.contains(&class_path) {
            errors.push(format!(
                "generated-manifest-class-without-catalog-classification: {class_path}"
            ));
        }
    }

    for path in &deferred_oracles {
        let is_missing_runtime = runtime
            .iter()
            .any(|entry| entry.class_path == *path && entry.status == "missing");
        if !is_missing_runtime {
            errors.push(format!("deferred-oracle-without-missing-runtime: {path}"));
        }
        if registry_paths.contains(path) {
            errors.push(format!("deferred-oracle-has-registry-entry: {path}"));
        }
        if !manifest_mentions(manifest, path) {
            errors.push(format!("deferred-oracle-missing-generated-golden: {path}"));
        }
    }

    if !extensions.contains("CDL.Logical.TrueHoldWithReset") {
        errors.push("missing-extension-classification: CDL.Logical.TrueHoldWithReset".to_owned());
    }
    if runtime_paths.contains("CDL.Logical.TrueHoldWithReset") {
        errors.push(
            "extension-present-in-reference-runtime: CDL.Logical.TrueHoldWithReset".to_owned(),
        );
    }
    if validation_packages(catalog).len() != 11 {
        errors.push(format!(
            "validation-package-count: {}",
            validation_packages(catalog).len()
        ));
    }

    validate_structural_catalog(catalog, &mut errors);
    errors
}

fn validate_provenance(prov: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    if str_field(prov, "commit") != EXPECTED_REFERENCE_COMMIT {
        errors.push("invalid-provenance-commit".to_owned());
    }
    let date = str_field(prov, "fetched_at");
    if date.len() != 10 || date.matches('-').count() != 2 {
        errors.push("invalid-provenance-date".to_owned());
    }
    let fingerprint = str_field(prov, "catalog_fingerprint");
    if fingerprint.len() != 16
        || !fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
        || fingerprint != catalog_fingerprint(CATALOG_JSON.as_bytes())
    {
        errors.push("invalid-provenance-fingerprint".to_owned());
    }
    let package_order = string_set(array_field(prov, "package_order_files"));
    for expected in EXPECTED_PACKAGE_ORDER_FILES {
        if !package_order.contains(*expected) {
            errors.push(format!("missing-package-order-source: {expected}"));
        }
    }
    let structural = string_set(array_field(prov, "structural_source_files"));
    for expected in EXPECTED_STRUCTURAL_SOURCE_FILES {
        if !structural.contains(*expected) {
            errors.push(format!("missing-structural-source: {expected}"));
        }
    }
    let drift = string_set(array_field(prov, "drift_source_files"));
    for expected in EXPECTED_DRIFT_SOURCE_FILES {
        if !drift.contains(*expected) {
            errors.push(format!("missing-drift-source: {expected}"));
        }
    }
    let external_sources = array_field(prov, "external_source_files");
    let external_paths = source_paths(external_sources);
    for expected in EXPECTED_EXTERNAL_SOURCE_FILES {
        if !external_paths.contains(*expected) {
            errors.push(format!("missing-external-source: {expected}"));
        }
    }
    validate_external_sources(external_sources, &mut errors);
    errors
}

fn runtime_classes(catalog: &Value, errors: &mut Vec<String>) -> Vec<RuntimeClass> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let packages = array_field(catalog, "runtime_packages");
    for package in packages {
        let package_name = str_field(package, "package");
        if str_field(package, "package_order").is_empty() {
            errors.push(format!("missing-package-order: {package_name}"));
        }
        let implemented = package["implemented"].as_object().unwrap();
        for (evidence, classes) in implemented {
            for class in array_strings(classes) {
                let path = format!("{package_name}.{class}");
                if evidence == "unclassified" {
                    errors.push(format!("unclassified-reference-class: {path}"));
                }
                if !seen.insert(path.clone()) {
                    errors.push(format!("duplicate-reference-class: {path}"));
                }
                out.push(RuntimeClass {
                    class_path: path,
                    status: "implemented".to_owned(),
                    evidence: evidence.clone(),
                });
            }
        }
        for class in array_strings(package.get("missing").unwrap()) {
            let path = format!("{package_name}.{class}");
            if !seen.insert(path.clone()) {
                errors.push(format!("duplicate-reference-class: {path}"));
            }
            out.push(RuntimeClass {
                class_path: path,
                status: "missing".to_owned(),
                evidence: "not_required".to_owned(),
            });
        }
    }
    out
}

fn validate_structural_catalog(catalog: &Value, errors: &mut Vec<String>) {
    let constants = &catalog["structural"]["constants"];
    assert_f64_bits(
        constants["values"]["pi"].as_f64(),
        std::f64::consts::PI,
        "pi",
        errors,
    );
    assert_f64_bits(constants["values"]["eps"].as_f64(), 1e-15, "eps", errors);
    assert_f64_bits(
        constants["values"]["small"].as_f64(),
        1e-37,
        "small",
        errors,
    );
    assert_f64_bits(
        constants["extension_aliases"]["inf"]["value"].as_f64(),
        f64::MAX,
        "inf",
        errors,
    );
    if str_field(&constants["extension_aliases"]["inf"], "status") != "modelica_alias" {
        errors.push("missing-constants-inf-extension-classification".to_owned());
    }

    let types = array_field(&catalog["structural"], "types");
    let zero_time = types
        .iter()
        .find(|entry| str_field(entry, "class_path") == "CDL.Types.ZeroTime")
        .expect("ZeroTime structural entry");
    let zero_time_members = array_strings(&zero_time["members"]);
    if zero_time_members.len() != 44
        || zero_time_members.last().map(String::as_str) != Some("NY2050")
    {
        errors.push("stale-type-enum: CDL.Types.ZeroTime".to_owned());
    }
    if array_field(&catalog["structural"], "interfaces").len() != 6 {
        errors.push("stale-interface-catalog".to_owned());
    }
}

fn assert_f64_bits(actual: Option<f64>, expected: f64, name: &str, errors: &mut Vec<String>) {
    if actual.map(f64::to_bits) != Some(expected.to_bits()) {
        errors.push(format!("stale-structural-constant: {name}"));
    }
}

pub(super) fn registry_class_paths() -> BTreeSet<String> {
    REGISTRY_CATALOG
        .iter()
        .flat_map(|entries| entries.iter())
        .map(|entry| entry.class_path.to_owned())
        .collect()
}

fn extension_paths(catalog: &Value) -> BTreeSet<String> {
    array_field(catalog, "extensions")
        .iter()
        .map(|entry| str_field(entry, "class_path").to_owned())
        .collect()
}

fn deferred_oracle_paths(catalog: &Value) -> BTreeSet<String> {
    array_field(catalog, "deferred_oracle_goldens")
        .iter()
        .map(|entry| str_field(entry, "class_path").to_owned())
        .collect()
}

fn validation_packages(catalog: &Value) -> BTreeSet<String> {
    string_set(array_field(catalog, "validation_packages"))
}

fn manifest_class_paths(manifest: &str) -> BTreeSet<String> {
    manifest
        .lines()
        .filter_map(|line| line.strip_prefix("CDL."))
        .filter_map(|suffix| {
            let class = suffix
                .split(" -> ")
                .next()
                .unwrap_or("")
                .split_whitespace()
                .next()
                .unwrap_or("")
                .split('/')
                .next()
                .unwrap_or("");
            if class.is_empty() {
                None
            } else {
                Some(format!("CDL.{class}"))
            }
        })
        .collect()
}

fn manifest_mentions(manifest: &str, class_path: &str) -> bool {
    manifest.contains(&format!("{class_path} "))
        || manifest.contains(&format!("{class_path}/"))
        || manifest.contains(&format!("{class_path} ("))
}

pub(super) fn assert_validation_error(
    catalog: &Value,
    prov: &Value,
    manifest: &str,
    registry: &BTreeSet<String>,
    expected: &str,
) {
    let errors = validate_catalog(catalog, prov, manifest, registry);
    assert!(
        errors.iter().any(|err| err == expected),
        "expected `{expected}` in errors:\n{}",
        errors.join("\n")
    );
}

pub(super) fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("catalog JSON parses")
}

pub(super) fn runtime_package_mut<'a>(catalog: &'a mut Value, package_name: &str) -> &'a mut Value {
    catalog["runtime_packages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|package| str_field(package, "package") == package_name)
        .unwrap_or_else(|| panic!("missing runtime package {package_name}"))
}

pub(super) fn remove_string(values: &mut Vec<Value>, target: &str) -> bool {
    let Some(pos) = values
        .iter()
        .position(|value| value.as_str() == Some(target))
    else {
        return false;
    };
    values.remove(pos);
    true
}

pub(super) fn remove_source_path(values: &mut Vec<Value>, target: &str) -> bool {
    let Some(pos) = values
        .iter()
        .position(|value| str_field(value, "path") == target)
    else {
        return false;
    };
    values.remove(pos);
    true
}

fn validate_external_sources(values: &[Value], errors: &mut Vec<String>) {
    let buildings_package = values
        .iter()
        .find(|value| str_field(value, "path") == "Buildings/package.mo");
    if buildings_package
        .map(|value| str_field(value, "commit") != EXPECTED_REFERENCE_COMMIT)
        .unwrap_or(true)
    {
        errors.push("invalid-external-source-commit: Buildings/package.mo".to_owned());
    }
    if !buildings_package
        .map(|value| str_field(value, "evidence").contains("Modelica(version=\"4.1.0\")"))
        .unwrap_or(false)
    {
        errors.push("invalid-buildings-modelica-version-source".to_owned());
    }

    for path in ["Modelica/Constants.mo", "ModelicaServices/package.mo"] {
        let source = values.iter().find(|value| str_field(value, "path") == path);
        if source
            .map(|value| str_field(value, "commit") != EXPECTED_MODELICA_COMMIT)
            .unwrap_or(true)
        {
            errors.push(format!("invalid-external-source-commit: {path}"));
        }
        if source
            .map(|value| str_field(value, "tag") != "v4.1.0")
            .unwrap_or(true)
        {
            errors.push(format!("invalid-external-source-tag: {path}"));
        }
    }
}

pub(super) fn str_field<'a>(value: &'a Value, name: &str) -> &'a str {
    value.get(name).and_then(Value::as_str).unwrap_or("")
}

pub(super) fn array_field<'a>(value: &'a Value, name: &str) -> &'a Vec<Value> {
    value
        .get(name)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("missing array field `{name}`"))
}

fn array_strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("expected array, got {value:?}"))
        .iter()
        .map(|item| item.as_str().unwrap_or("").to_owned())
        .collect()
}

pub(super) fn string_set(values: &[Value]) -> BTreeSet<String> {
    values
        .iter()
        .map(|item| item.as_str().unwrap_or("").to_owned())
        .collect()
}

pub(super) fn source_paths(values: &[Value]) -> BTreeSet<String> {
    values
        .iter()
        .map(|item| str_field(item, "path").to_owned())
        .collect()
}

pub(super) fn catalog_fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
