//! Checked-in Buildings CDL catalog guard tests.

use std::collections::BTreeSet;

use serde_json::Value;

use super::catalog_guard_support::*;

#[test]
fn catalog_provenance_pins_source_snapshot() {
    let prov = parse(PROV_JSON);
    assert_eq!(
        str_field(&prov, "repository"),
        "https://github.com/lbl-srg/modelica-buildings"
    );
    assert_eq!(str_field(&prov, "branch"), "master");
    assert_eq!(str_field(&prov, "commit"), EXPECTED_REFERENCE_COMMIT);
    assert_eq!(str_field(&prov, "fetched_at"), "2026-06-26");
    assert_eq!(
        str_field(&prov, "catalog_fingerprint"),
        EXPECTED_CATALOG_FINGERPRINT
    );
    assert_eq!(
        catalog_fingerprint(CATALOG_JSON.as_bytes()),
        EXPECTED_CATALOG_FINGERPRINT
    );

    let files = string_set(array_field(&prov, "package_order_files"));
    let expected = EXPECTED_PACKAGE_ORDER_FILES
        .iter()
        .copied()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    assert_eq!(files, expected);

    let structural = string_set(array_field(&prov, "structural_source_files"));
    assert_eq!(
        structural,
        EXPECTED_STRUCTURAL_SOURCE_FILES
            .iter()
            .copied()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    );
    let drift = string_set(array_field(&prov, "drift_source_files"));
    assert_eq!(
        drift,
        EXPECTED_DRIFT_SOURCE_FILES
            .iter()
            .copied()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    );
    let external = source_paths(array_field(&prov, "external_source_files"));
    assert_eq!(
        external,
        EXPECTED_EXTERNAL_SOURCE_FILES
            .iter()
            .copied()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    );
}

#[test]
fn catalog_classifications_are_closed_against_registry_and_goldens() {
    let catalog = parse(CATALOG_JSON);
    let prov = parse(PROV_JSON);
    let errors = validate_catalog(&catalog, &prov, GOLDEN_MANIFEST, &registry_class_paths());
    assert!(
        errors.is_empty(),
        "catalog guard failures:\n{}",
        errors.join("\n")
    );
}

#[test]
fn catalog_guard_mutations_cover_required_failures() {
    let catalog = parse(CATALOG_JSON);
    let prov = parse(PROV_JSON);
    let registry = registry_class_paths();

    let mut unclassified = catalog.clone();
    let package = runtime_package_mut(&mut unclassified, "CDL.Reals");
    let removed = remove_string(
        package["implemented"]["golden_manifest"]
            .as_array_mut()
            .unwrap(),
        "Abs",
    );
    assert!(removed);
    package["implemented"]["unclassified"] = Value::Array(vec![Value::String("Abs".to_owned())]);
    assert_validation_error(
        &unclassified,
        &prov,
        GOLDEN_MANIFEST,
        &registry,
        "unclassified-reference-class: CDL.Reals.Abs",
    );

    let mut missing_registry_path = catalog.clone();
    let package = runtime_package_mut(&mut missing_registry_path, "CDL.Reals");
    let removed = remove_string(
        package["implemented"]["golden_manifest"]
            .as_array_mut()
            .unwrap(),
        "Add",
    );
    assert!(removed);
    assert_validation_error(
        &missing_registry_path,
        &prov,
        GOLDEN_MANIFEST,
        &registry,
        "registry-class-not-in-catalog: CDL.Reals.Add",
    );

    let mut implemented_without_registry = catalog.clone();
    let package = runtime_package_mut(&mut implemented_without_registry, "CDL.Reals.Sources");
    let removed = remove_string(package["missing"].as_array_mut().unwrap(), "TimeTable");
    assert!(removed);
    package["implemented"]["golden_manifest"]
        .as_array_mut()
        .unwrap()
        .push(Value::String("TimeTable".to_owned()));
    assert_validation_error(
        &implemented_without_registry,
        &prov,
        GOLDEN_MANIFEST,
        &registry,
        "implemented-class-without-registry-entry: CDL.Reals.Sources.TimeTable",
    );

    let mut stale_manifest = catalog.clone();
    let package = runtime_package_mut(&mut stale_manifest, "CDL.Logical");
    let removed = remove_string(
        package["implemented"]["legacy_evidence_exemption"]
            .as_array_mut()
            .unwrap(),
        "Nand",
    );
    assert!(removed);
    package["implemented"]["golden_manifest"]
        .as_array_mut()
        .unwrap()
        .push(Value::String("Nand".to_owned()));
    assert_validation_error(
        &stale_manifest,
        &prov,
        GOLDEN_MANIFEST,
        &registry,
        "stale-generated-manifest: CDL.Logical.Nand",
    );

    let manifest_with_unclassified_class = format!(
        "{GOLDEN_MANIFEST}CDL.Synthetic.Missing y -> goldens/CDL/Synthetic/Missing/y.csv\n"
    );
    assert_validation_error(
        &catalog,
        &prov,
        &manifest_with_unclassified_class,
        &registry,
        "generated-manifest-class-without-catalog-classification: CDL.Synthetic.Missing",
    );

    let mut missing_evidence = catalog.clone();
    let package = runtime_package_mut(&mut missing_evidence, "CDL.Reals");
    let removed = remove_string(
        package["implemented"]["golden_manifest"]
            .as_array_mut()
            .unwrap(),
        "AddParameter",
    );
    assert!(removed);
    package["implemented"]["not_required"] =
        Value::Array(vec![Value::String("AddParameter".to_owned())]);
    assert_validation_error(
        &missing_evidence,
        &prov,
        GOLDEN_MANIFEST,
        &registry,
        "implemented-class-without-golden-or-exemption: CDL.Reals.AddParameter",
    );

    let mut stale_constant = catalog.clone();
    stale_constant["structural"]["constants"]["values"]["small"] = Value::from(1e-60);
    assert_validation_error(
        &stale_constant,
        &prov,
        GOLDEN_MANIFEST,
        &registry,
        "stale-structural-constant: small",
    );

    let mut bad_hash = prov.clone();
    bad_hash["catalog_fingerprint"] = Value::String(String::new());
    assert_validation_error(
        &catalog,
        &bad_hash,
        GOLDEN_MANIFEST,
        &registry,
        "invalid-provenance-fingerprint",
    );

    let mut bad_date = prov.clone();
    bad_date["fetched_at"] = Value::String(String::new());
    assert_validation_error(
        &catalog,
        &bad_date,
        GOLDEN_MANIFEST,
        &registry,
        "invalid-provenance-date",
    );

    let mut missing_source = prov;
    remove_string(
        missing_source["package_order_files"]
            .as_array_mut()
            .unwrap(),
        "Buildings/Controls/OBC/CDL/Conversions/package.order",
    );
    assert_validation_error(
        &catalog,
        &missing_source,
        GOLDEN_MANIFEST,
        &registry,
        "missing-package-order-source: Buildings/Controls/OBC/CDL/Conversions/package.order",
    );

    let mut missing_structural_source = parse(PROV_JSON);
    remove_string(
        missing_structural_source["structural_source_files"]
            .as_array_mut()
            .unwrap(),
        "Buildings/Controls/OBC/CDL/Interfaces/RealOutput.mo",
    );
    assert_validation_error(
        &catalog,
        &missing_structural_source,
        GOLDEN_MANIFEST,
        &registry,
        "missing-structural-source: Buildings/Controls/OBC/CDL/Interfaces/RealOutput.mo",
    );

    let mut missing_external_source = parse(PROV_JSON);
    remove_source_path(
        missing_external_source["external_source_files"]
            .as_array_mut()
            .unwrap(),
        "ModelicaServices/package.mo",
    );
    assert_validation_error(
        &catalog,
        &missing_external_source,
        GOLDEN_MANIFEST,
        &registry,
        "missing-external-source: ModelicaServices/package.mo",
    );
}
