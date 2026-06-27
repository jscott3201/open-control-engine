//! Checked-in ASHRAE G36 catalog/profile guard tests.

use serde_json::Value;

use super::g36_catalog_guard_support::*;

#[test]
fn g36_catalog_provenance_pins_source_snapshot() {
    let catalog = parse(CATALOG_JSON);
    let prov = parse(PROV_JSON);

    assert_eq!(
        str_field(&prov, "repository"),
        "https://github.com/lbl-srg/modelica-buildings"
    );
    assert_eq!(str_field(&prov, "branch"), "master");
    assert_eq!(str_field(&prov, "commit"), EXPECTED_REFERENCE_COMMIT);
    assert_eq!(str_field(&prov, "fetched_at"), "2026-06-27");
    assert_eq!(
        str_field(&prov, "catalog_fingerprint"),
        EXPECTED_CATALOG_FINGERPRINT
    );
    assert_eq!(
        catalog_fingerprint(CATALOG_JSON.as_bytes()),
        EXPECTED_CATALOG_FINGERPRINT
    );

    let sources = string_set(array_field(&prov, "package_order_files"));
    for expected in EXPECTED_PACKAGE_ORDER_FILES {
        assert!(sources.contains(*expected), "missing {expected}");
    }
    let sources = string_set(array_field(&prov, "sequence_source_files"));
    for expected in EXPECTED_SEQUENCE_SOURCE_FILES {
        assert!(sources.contains(*expected), "missing {expected}");
    }
    let sources = string_set(array_field(&prov, "type_source_files"));
    for expected in EXPECTED_TYPE_SOURCE_FILES {
        assert!(sources.contains(*expected), "missing {expected}");
    }

    assert_eq!(
        str_field(&catalog, "schema"),
        "oce-g36-catalog-v1",
        "catalog schema"
    );
}

#[test]
fn g36_catalog_profile_guard_is_closed_against_fixtures_and_types() {
    let catalog = parse(CATALOG_JSON);
    let prov = parse(PROV_JSON);
    let errors = validate_g36_catalog(
        &catalog,
        &prov,
        RUNTIME_FIXTURES,
        COMPOSITE_IMPORT_FIXTURES,
        PROFILE_FIXTURES,
    );
    assert!(
        errors.is_empty(),
        "G36 catalog guard failures:\n{}",
        errors.join("\n")
    );
}

#[test]
fn g36_catalog_guard_mutations_cover_required_failures() {
    let catalog = parse(CATALOG_JSON);
    let prov = parse(PROV_JSON);

    let mut bad_commit = prov.clone();
    bad_commit["commit"] = Value::String(String::new());
    assert_validation_error(&catalog, &bad_commit, "invalid-provenance-commit");

    let mut bad_hash = prov.clone();
    bad_hash["catalog_fingerprint"] = Value::String(String::new());
    assert_validation_error(&catalog, &bad_hash, "invalid-provenance-fingerprint");

    let mut missing_source = prov.clone();
    remove_string(
        missing_source["package_order_files"]
            .as_array_mut()
            .unwrap(),
        "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/package.order",
    );
    assert_validation_error(
        &catalog,
        &missing_source,
        "missing-package-order-source: Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/package.order",
    );

    let mut missing_fixture = catalog.clone();
    remove_path_entry(
        missing_fixture["fixture_only_sequences"]
            .as_array_mut()
            .unwrap(),
        "fixture",
        "crates/oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld",
    );
    assert_validation_error(&missing_fixture, &prov, "fixture-path-set-drift");

    let mut missing_source_review = catalog.clone();
    let sat_fixture = fixture_record_mut(&mut missing_source_review, "ahu_supply_air_temp_reset");
    sat_fixture["source_mapping_status"] = Value::String(String::new());
    assert_validation_error(
        &missing_source_review,
        &prov,
        "fixture-source-mapping-not-reviewed: ahu_supply_air_temp_reset",
    );

    let mut stale_fixture_manifest = catalog.clone();
    let sat_fixture = fixture_record_mut(&mut stale_fixture_manifest, "ahu_supply_air_temp_reset");
    sat_fixture["required_inputs"].as_array_mut().unwrap().pop();
    assert_validation_error(
        &stale_fixture_manifest,
        &prov,
        "fixture-input-manifest-drift: ahu_supply_air_temp_reset",
    );

    let mut unknown_fixture_source = catalog.clone();
    let econ_fixture = fixture_record_mut(&mut unknown_fixture_source, "ahu_economizer");
    econ_fixture["upstream_source_files"]
        .as_array_mut()
        .unwrap()
        .push(Value::String(
            "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Missing.mo"
                .to_owned(),
        ));
    assert_validation_error(
        &unknown_fixture_source,
        &prov,
        "fixture-source-not-in-provenance: ahu_economizer:Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Missing.mo",
    );

    let mut missing_composite_fixture = catalog.clone();
    remove_path_entry(
        missing_composite_fixture["composite_import_fixtures"]
            .as_array_mut()
            .unwrap(),
        "fixture",
        "crates/oce-cxf/tests/fixtures/g36/trim_and_respond_have_hol_false.jsonld",
    );
    assert_validation_error(
        &missing_composite_fixture,
        &prov,
        "composite-import-fixture-path-set-drift",
    );

    let mut runtime_claim_without_evidence = catalog.clone();
    runtime_claim_without_evidence["runtime_sequences"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "class_path": "Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature",
            "status": "supported-runtime-sequence",
            "source": "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyTemperature.mo"
        }));
    assert_validation_error(
        &runtime_claim_without_evidence,
        &prov,
        "runtime-sequence-missing-evidence: Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature:fixture",
    );

    let mut validation_promoted = catalog.clone();
    let validation = validation_promoted["initial_vav_paths"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| {
            str_field(entry, "class_path")
                == "Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.Validation"
        })
        .expect("validation catalog entry");
    validation["status"] = Value::String("supported-runtime-sequence".to_owned());
    validation["runtime_support"] = Value::Bool(true);
    assert_validation_error(
        &validation_promoted,
        &prov,
        "validation-path-marked-runtime-supported: Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.Validation",
    );

    let mut unknown_literal = catalog.clone();
    let ventilation = unknown_literal["g36_types"]["enumerations"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| {
            str_field(entry, "class_path")
                == "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard"
        })
        .expect("VentilationStandard catalog entry");
    let removed = remove_string(
        ventilation["literals"].as_array_mut().unwrap(),
        "California_Title_24",
    );
    assert!(removed);
    assert_validation_error(
        &unknown_literal,
        &prov,
        "unknown-g36-enum-literal: parameter_gated_connector:Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard.California_Title_24",
    );

    let bad_guard = FixtureSource {
        name: "parameter_gated_connector",
        path: "_spec/oce_g36_gap_specs_v1/reference/fixtures/parameter-gated-connector.jsonld",
        text: PARAMETER_GATED_UNKNOWN_PARAMETER,
    };
    let profile_fixtures = [PROFILE_FIXTURES[0], bad_guard];
    let errors = validate_g36_catalog(
        &catalog,
        &prov,
        RUNTIME_FIXTURES,
        COMPOSITE_IMPORT_FIXTURES,
        &profile_fixtures,
    );
    assert!(
        errors
            .iter()
            .any(|err| err
                == "conditional-guard-unknown-parameter: parameter_gated_connector:unknown"),
        "expected unknown guard parameter in errors:\n{}",
        errors.join("\n")
    );
}

fn fixture_record_mut<'a>(catalog: &'a mut Value, name: &str) -> &'a mut Value {
    catalog["fixture_only_sequences"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| str_field(entry, "name") == name)
        .unwrap_or_else(|| panic!("missing fixture record {name}"))
}

const PARAMETER_GATED_UNKNOWN_PARAMETER: &str = r#"{
  "@context": {
    "S231": "http://data.ashrae.org/S231P#",
    "base": "http://example.org#"
  },
  "@graph": [
    {
      "@id": "http://example.org#g36.profile.parameter_gated_connector",
      "@type": "S231:Block",
      "S231:label": "parameter_gated_connector",
      "S231:hasParameter": { "@id": "http://example.org#g36.profile.parameter_gated_connector.venStd" },
      "S231:hasInput": { "@id": "http://example.org#g36.profile.parameter_gated_connector.co2_concentration" }
    },
    {
      "@id": "http://example.org#g36.profile.parameter_gated_connector.venStd",
      "@type": "S231:Parameter",
      "S231:isOfDataType": {
        "@id": "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard"
      },
      "S231:value": "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard.California_Title_24"
    },
    {
      "@id": "http://example.org#g36.profile.parameter_gated_connector.co2_concentration",
      "@type": "S231:RealInput",
      "S231:isOfDataType": { "@id": "S231:Real" },
      "S231:isConditionalComponent": true,
      "S231:conditionalExpression": "unknown == Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard.California_Title_24"
    }
  ]
}"#;
