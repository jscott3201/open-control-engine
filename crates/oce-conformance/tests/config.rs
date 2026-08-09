//! VerifyConfig serde and semantic validation coverage.

use oce_conformance::{
    ConfigError, IndicatorPattern, OutputPattern, PartialTolerances, PointEnd, PointMapEntry,
    ReferenceSpec, Tolerances, VerifyConfig, escape_regex,
};

fn valid_config_json() -> &'static str {
    include_str!("fixtures/golden/verify_config.json")
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
        config.indicators,
        vec![IndicatorPattern {
            pattern: "TSupSet.*".to_string(),
            signals: vec!["fanSta.y".to_string()]
        }]
    );
    let overridden = config.tolerance_for_exact_output("TSupSet.*");
    assert_eq!(overridden.atoly.to_bits(), 0.1f64.to_bits());
    assert_eq!(overridden.rtoly.to_bits(), 0.01f64.to_bits());
    assert_eq!(overridden.atolx.to_bits(), 10.0f64.to_bits());
    let regex_overridden = config
        .tolerance_for_output("TSupSet.y")
        .expect("regex pattern compiles");
    assert_eq!(regex_overridden.atoly.to_bits(), 0.1f64.to_bits());
    assert_eq!(regex_overridden.rtoly.to_bits(), 0.01f64.to_bits());

    let text = config.to_json_string_pretty().expect("serialize config");
    let reparsed = VerifyConfig::from_json_str(&text).expect("reparse serialized config");
    assert_eq!(reparsed, config);
}

#[test]
fn verify_config_golden_serializes_byte_identically_across_reruns() {
    let golden = valid_config_json();
    let config = VerifyConfig::from_json_str(golden).expect("golden config parses");
    let once = config.to_json_string_pretty().expect("serialize once");
    let twice = VerifyConfig::from_json_str(&once)
        .expect("serialized config parses")
        .to_json_string_pretty()
        .expect("serialize twice");
    assert_eq!(once, golden.trim_end());
    assert_eq!(twice, once);
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
fn verify_config_rejects_negative_per_output_tolerances() {
    let err = VerifyConfig::from_json_str(
        r#"{
  "references": [
    { "model": "m", "sequence": "s", "pointNameMapping": [] }
  ],
  "outputs": {
    "TSupSet.*": { "atoly": -0.1 }
  }
}"#,
    )
    .expect_err("negative per-output tolerance fails validation");
    assert!(matches!(err, ConfigError::Invalid(ref message)
            if message.contains("outputs[\"TSupSet.*\"].atoly")));
}

#[test]
fn verify_config_rejects_nonfinite_tolerances_through_struct_path() {
    let base = VerifyConfig {
        references: vec![ReferenceSpec {
            model: "m".into(),
            sequence: "s".into(),
            point_name_mapping: vec![],
        }],
        tolerances: Tolerances {
            atoly: f64::INFINITY,
            ..Tolerances::default()
        },
        outputs: Vec::new(),
        indicators: Vec::new(),
        sampling: Some(60.0),
        run_controller: true,
    };
    let err = base
        .validate()
        .expect_err("infinite base tolerance rejected");
    assert!(
        matches!(err, ConfigError::Invalid(ref message) if message.contains("tolerances.atoly"))
    );

    let base = VerifyConfig {
        tolerances: Tolerances {
            atoly: f64::NAN,
            ..Tolerances::default()
        },
        ..base
    };
    let err = base.validate().expect_err("NaN base tolerance rejected");
    assert!(
        matches!(err, ConfigError::Invalid(ref message) if message.contains("tolerances.atoly"))
    );

    let outputs = vec![OutputPattern {
        pattern: "TSupSet.*".to_string(),
        tolerances: PartialTolerances {
            atoly: Some(f64::INFINITY),
            ..PartialTolerances::default()
        },
    }];
    let config = VerifyConfig {
        tolerances: Tolerances::default(),
        outputs,
        ..base
    };
    let err = config
        .validate()
        .expect_err("infinite per-output tolerance rejected");
    assert!(matches!(err, ConfigError::Invalid(ref message)
            if message.contains("outputs[\"TSupSet.*\"].atoly")));

    let outputs = vec![OutputPattern {
        pattern: "TSupSet.*".to_string(),
        tolerances: PartialTolerances {
            atoly: Some(f64::NAN),
            ..PartialTolerances::default()
        },
    }];
    let config = VerifyConfig { outputs, ..config };
    let err = config
        .validate()
        .expect_err("NaN per-output tolerance rejected");
    assert!(matches!(err, ConfigError::Invalid(ref message)
            if message.contains("outputs[\"TSupSet.*\"].atoly")));
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
        outputs: Vec::new(),
        indicators: Vec::new(),
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

#[test]
fn verify_config_preserves_ordered_duplicate_patterns_and_cascades_matches() {
    let config = VerifyConfig::from_json_str(
        r#"{
  "references": [
    { "model": "m", "sequence": "s", "pointNameMapping": [] }
  ],
  "outputs": {
    ".*": { "atoly": 1.0 },
    "TSup.*": { "rtoly": 0.25 },
    "TSup.*": { "atoly": 0.5 }
  },
  "indicators": {
    ".*": ["fanSta.y"],
    "TSup.*": ["mode.y"]
  }
}"#,
    )
    .expect("duplicate JSON object keys are preserved by the ordered visitor");

    assert_eq!(
        config
            .outputs
            .iter()
            .map(|entry| entry.pattern.as_str())
            .collect::<Vec<_>>(),
        [".*", "TSup.*", "TSup.*"]
    );
    let tol = config
        .tolerance_for_output("TSupSet.y")
        .expect("regexes compile");
    assert_eq!(tol.atoly.to_bits(), 0.5f64.to_bits());
    assert_eq!(tol.rtoly.to_bits(), 0.25f64.to_bits());
    assert_eq!(
        config
            .indicator_signals_for_output("TSupSet.y")
            .expect("regexes compile"),
        ["fanSta.y".to_string(), "mode.y".to_string()]
    );
}

/// A config whose single per-output override carries `pattern`, consumed through
/// `tolerance_for_output` — the same machinery every point-keyed pattern in the suite feeds.
fn config_with_output_pattern(pattern: String) -> VerifyConfig {
    VerifyConfig {
        references: Vec::new(),
        tolerances: Tolerances::default(),
        outputs: vec![OutputPattern {
            pattern,
            tolerances: PartialTolerances {
                atoly: Some(0.5),
                ..PartialTolerances::default()
            },
        }],
        indicators: Vec::new(),
        sampling: None,
        run_controller: true,
    }
}

#[test]
fn escaped_anchored_pattern_selects_only_the_authored_path_it_names() {
    // The shape every point-keyed call site builds: `^` + escape_regex(path) + `$`.
    let path = "http://example.org#DriverAdd.add.y";
    // Each `.` in the path wildcard-matches the `X`s here when left unescaped.
    let sibling = "http://example.org#DriverAddXaddXy";
    let config = config_with_output_pattern(format!("^{}$", escape_regex(path)));
    let on_path = config.tolerance_for_output(path).expect("pattern compiles");
    assert_eq!(
        on_path.atoly.to_bits(),
        0.5f64.to_bits(),
        "the override must land on the named point"
    );
    let on_sibling = config
        .tolerance_for_output(sibling)
        .expect("pattern compiles");
    assert_eq!(
        on_sibling.atoly.to_bits(),
        Tolerances::default().atoly.to_bits(),
        "escaped dots must not select a sibling point"
    );
}

#[test]
fn unescaped_dots_in_an_anchored_authored_path_select_a_sibling_point() {
    // The control for the test above: the same anchored pattern WITHOUT escaping does match the
    // sibling, so the escape is load-bearing, not decorative.
    let path = "http://example.org#DriverAdd.add.y";
    let sibling = "http://example.org#DriverAddXaddXy";
    let config = config_with_output_pattern(format!("^{path}$"));
    let on_sibling = config
        .tolerance_for_output(sibling)
        .expect("pattern compiles");
    assert_eq!(
        on_sibling.atoly.to_bits(),
        0.5f64.to_bits(),
        "an unescaped authored path selects points it never named"
    );
}

#[test]
fn verify_config_rejects_invalid_regex_patterns() {
    let err = VerifyConfig::from_json_str(
        r#"{
  "references": [
    { "model": "m", "sequence": "s", "pointNameMapping": [] }
  ],
  "outputs": { "[": { "atoly": 1.0 } }
}"#,
    )
    .expect_err("bad regex rejected");
    assert!(matches!(err, ConfigError::Invalid(ref message) if message.contains("regex")));
}
