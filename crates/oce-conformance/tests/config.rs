//! VerifyConfig serde and semantic validation coverage.

use oce_conformance::{
    ConfigError, PartialTolerances, PointEnd, PointMapEntry, ReferenceSpec, Tolerances,
    VerifyConfig,
};

fn valid_config_json() -> &'static str {
    r#"{
  "references": [
    {
      "model": "Buildings.Controls.OBC.ASHRAE.G36.AHUs.SingleZone.VAV.SetPoints.Supply",
      "sequence": "ahu_supply_air_temp_reset",
      "pointNameMapping": [
        {
          "cdl": { "name": "TSupSet.y", "unit": "K", "type": "Real" },
          "device": { "name": "supply_air_temperature_setpoint", "unit": "degC", "type": "Real" }
        }
      ]
    }
  ],
  "tolerances": {
    "atolx": 10.0,
    "atoly": 0.0,
    "rtolx": 0.002,
    "rtoly": 0.002,
    "ltolx": 0.0,
    "ltoly": 0.0
  },
  "outputs": {
    "TSupSet.*": { "atoly": 0.1, "rtoly": 0.01 }
  },
  "indicators": {
    "TSupSet.*": [ "fanSta.y" ]
  },
  "sampling": 60.0,
  "runController": true
}"#
}

#[test]
fn verify_config_round_trips_with_point_mapping_masks_and_tolerances() {
    let config = VerifyConfig::from_json_str(valid_config_json()).expect("valid config");
    assert_eq!(config.references.len(), 1);
    assert_eq!(
        config.references[0].point_name_mapping[0].cdl.name,
        "TSupSet.y"
    );
    assert_eq!(
        config.indicators.get("TSupSet.*"),
        Some(&vec!["fanSta.y".to_string()])
    );
    let overridden = config.tolerance_for_exact_output("TSupSet.*");
    assert_eq!(overridden.atoly.to_bits(), 0.1f64.to_bits());
    assert_eq!(overridden.rtoly.to_bits(), 0.01f64.to_bits());
    assert_eq!(overridden.atolx.to_bits(), 10.0f64.to_bits());

    let text = config.to_json_string_pretty().expect("serialize config");
    let reparsed = VerifyConfig::from_json_str(&text).expect("reparse serialized config");
    assert_eq!(reparsed, config);
}

#[test]
fn verify_config_defaults_run_controller_to_true() {
    let config = VerifyConfig::from_json_str(
        r#"{
  "references": [
    { "model": "m", "sequence": "s", "pointNameMapping": [] }
  ]
}"#,
    )
    .expect("minimal valid config");
    assert!(config.run_controller);
    assert_eq!(config.tolerances, Tolerances::default());
}

#[test]
fn verify_config_rejects_negative_or_nonfinite_tolerances() {
    let err = VerifyConfig::from_json_str(
        r#"{
  "references": [
    { "model": "m", "sequence": "s", "pointNameMapping": [] }
  ],
  "tolerances": {
    "atolx": 10.0,
    "atoly": -0.1,
    "rtolx": 0.002,
    "rtoly": 0.002,
    "ltolx": 0.0,
    "ltoly": 0.0
  }
}"#,
    )
    .expect_err("negative tolerance fails validation");
    assert!(matches!(err, ConfigError::Invalid(ref message) if message.contains("atoly")));
}

#[test]
fn verify_config_rejects_missing_reference_and_empty_indicator() {
    let err = VerifyConfig::from_json_str(r#"{ "references": [] }"#)
        .expect_err("at least one reference is required");
    assert!(matches!(err, ConfigError::Invalid(ref message) if message.contains("reference")));

    let err = VerifyConfig::from_json_str(
        r#"{
  "references": [
    { "model": "m", "sequence": "s", "pointNameMapping": [] }
  ],
  "indicators": { "y.*": [] }
}"#,
    )
    .expect_err("empty indicator list fails closed");
    assert!(matches!(err, ConfigError::Invalid(ref message) if message.contains("no signals")));
}

#[test]
fn verify_config_rejects_empty_point_names_and_bad_sampling() {
    let config = VerifyConfig {
        references: vec![ReferenceSpec {
            model: "m".into(),
            sequence: "s".into(),
            point_name_mapping: vec![PointMapEntry {
                cdl: PointEnd {
                    name: "".into(),
                    unit: Some("K".into()),
                    kind: Some("Real".into()),
                },
                device: PointEnd {
                    name: "sensor".into(),
                    unit: Some("K".into()),
                    kind: Some("Real".into()),
                },
            }],
        }],
        tolerances: Tolerances::default(),
        outputs: Default::default(),
        indicators: Default::default(),
        sampling: Some(0.0),
        run_controller: true,
    };
    let err = config
        .validate()
        .expect_err("empty point name rejected first");
    assert!(matches!(err, ConfigError::Invalid(ref message) if message.contains("sampling")));

    let config = VerifyConfig {
        sampling: Some(60.0),
        ..config
    };
    let err = config.validate().expect_err("empty point name rejected");
    assert!(matches!(err, ConfigError::Invalid(ref message) if message.contains("cdl name")));
}

#[test]
fn partial_tolerances_apply_only_present_fields() {
    let partial = PartialTolerances {
        atoly: Some(0.25),
        ltoly: Some(0.5),
        ..PartialTolerances::default()
    };
    let got = partial.apply_to(Tolerances::default());
    assert_eq!(got.atolx.to_bits(), 10.0f64.to_bits());
    assert_eq!(got.atoly.to_bits(), 0.25f64.to_bits());
    assert_eq!(got.ltoly.to_bits(), 0.5f64.to_bits());
}
