//! End-to-end M2-4 check: assemble a populated Tier 0–4 `ConformanceReport` from a real G36
//! `multizone_vav_economizer_enable` run.
//!
//! Tier 4 drives the funnel band against the Tier-A reference oracle (the same run the funnel suite
//! exercises); Tier 2 drives the bit-exact regression against the committed self-output golden (the
//! same run the determinism suite exercises). Tiers 1 and 3 are `Skipped` for a single-sequence run.
//! The negatives prove the report actually fails when a tier should fail — it is not a tautology.

use oce_api::Value;
use oce_conformance::{
    CombiTimeTable, ComparisonMode, ConformanceRun, ConformanceTier, DriveCadence,
    DriverInputReplay, DriverOptions, PointEnd, PointMapEntry, ReferenceSpec, RunInputs,
    Tier2Source, TierStatus, ValueKind, VerifyConfig, assemble_report,
};

#[path = "g36_funnel_band/policy.rs"]
mod funnel_band_policy;

#[allow(dead_code)]
#[path = "g36_determinism/support.rs"]
mod support;

use support::{
    PointSpec, SequenceSpec, config_for, driver_reference_from_output_golden, options_for, pair,
    read_output_golden,
};

const ECONOMIZER_ENABLE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_enable.jsonld");
const SEQUENCE: &str = "multizone_vav_economizer_enable";
const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";
const T_STOP: u32 = 23;
const SAMPLE_STEP: f64 = 60.0;

// Source URIs (input connectors), shared by the Tier-4 funnel mapping and the Tier-2 determinism run.
const OUTDOOR_AIR_TEMPERATURE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.TOut";
const OUTDOOR_AIR_CUTOFF: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.TOutCut";
const OUTDOOR_DAMPER_MIN: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uOutDam_min";
const OUTDOOR_DAMPER_MAX: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uOutDam_max";
const RETURN_DAMPER_MAX: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uRetDam_max";
const RETURN_DAMPER_MIN: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uRetDam_min";
const RETURN_DAMPER_PHYSICAL_MAX: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uRetDamPhy_max";
const SUPPLY_FAN_ON: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.u1SupFan";
const FREEZE_PROTECTION_STAGE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uFreProSta";

// Tier-2 golden output identities: source IRIs (golden columns) aliased to runtime connector ids.
const OUTDOOR_DAMPER_MAX_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.yOutDam_max";
const RETURN_DAMPER_MAX_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.yRetDam_max";
const RETURN_DAMPER_MIN_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.yRetDam_min";
const OUTDOOR_DAMPER_MAX_PATH: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.outDamSwitch.y";
const RETURN_DAMPER_MAX_PATH: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.maxRetDamSwitch.y";
const RETURN_DAMPER_MIN_PATH: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.minRetDamSwitch.y";

// ---- Tier 2 (determinism / bit-exact regression) spec -------------------------------------------

const DETERMINISM_INPUTS: &[PointSpec] = &[
    PointSpec::real(OUTDOOR_AIR_TEMPERATURE),
    PointSpec::real(OUTDOOR_AIR_CUTOFF),
    PointSpec::real(OUTDOOR_DAMPER_MIN),
    PointSpec::real(OUTDOOR_DAMPER_MAX),
    PointSpec::real(RETURN_DAMPER_MAX),
    PointSpec::real(RETURN_DAMPER_MIN),
    PointSpec::real(RETURN_DAMPER_PHYSICAL_MAX),
    PointSpec::boolean(SUPPLY_FAN_ON),
    PointSpec::integer(FREEZE_PROTECTION_STAGE),
];
const DETERMINISM_OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(OUTDOOR_DAMPER_MAX_SOURCE, OUTDOOR_DAMPER_MAX_PATH),
    PointSpec::real_alias(RETURN_DAMPER_MAX_SOURCE, RETURN_DAMPER_MAX_PATH),
    PointSpec::real_alias(RETURN_DAMPER_MIN_SOURCE, RETURN_DAMPER_MIN_PATH),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: SEQUENCE,
    cxf: ECONOMIZER_ENABLE,
    t_stop: T_STOP,
    sample_step: SAMPLE_STEP,
    inputs: DETERMINISM_INPUTS,
    outputs: DETERMINISM_OUTPUTS,
    input_fn: economizer_enable_inputs,
};

fn economizer_enable_inputs(t: f64) -> Vec<(String, Value)> {
    let outdoor_air_temperature = match t as u32 {
        0 => 294.0,
        60..=900 => 296.0,
        960..=1380 => 293.0,
        _ => unreachable!("unexpected test instant {t}"),
    };
    let supply_fan_on = !matches!(t as u32, 660..=840);
    let freeze_protection_stage = if (900.0..960.0).contains(&t) { 1 } else { 0 };
    vec![
        pair(
            OUTDOOR_AIR_TEMPERATURE,
            Value::Real(outdoor_air_temperature),
        ),
        pair(OUTDOOR_AIR_CUTOFF, Value::Real(295.0)),
        pair(OUTDOOR_DAMPER_MIN, Value::Real(0.2)),
        pair(OUTDOOR_DAMPER_MAX, Value::Real(0.9)),
        pair(RETURN_DAMPER_MAX, Value::Real(0.8)),
        pair(RETURN_DAMPER_MIN, Value::Real(0.1)),
        pair(RETURN_DAMPER_PHYSICAL_MAX, Value::Real(1.0)),
        pair(SUPPLY_FAN_ON, Value::Boolean(supply_fan_on)),
        pair(
            FREEZE_PROTECTION_STAGE,
            Value::Integer(freeze_protection_stage),
        ),
    ]
}

// ---- Tier 4 (funnel vs Tier-A reference oracle) mapping -----------------------------------------

/// (reference-column device name, engine connector/URI, kind). Inputs are replayed; the three Real
/// outputs route through the near-ULP funnel band.
const FUNNEL_INPUTS: &[(&str, &str, ValueKind)] = &[
    (
        "outdoor_air_temperature",
        OUTDOOR_AIR_TEMPERATURE,
        ValueKind::Real,
    ),
    ("outdoor_air_cutoff", OUTDOOR_AIR_CUTOFF, ValueKind::Real),
    ("outdoor_damper_min", OUTDOOR_DAMPER_MIN, ValueKind::Real),
    ("outdoor_damper_max", OUTDOOR_DAMPER_MAX, ValueKind::Real),
    ("return_damper_max", RETURN_DAMPER_MAX, ValueKind::Real),
    ("return_damper_min", RETURN_DAMPER_MIN, ValueKind::Real),
    (
        "return_damper_physical_max",
        RETURN_DAMPER_PHYSICAL_MAX,
        ValueKind::Real,
    ),
    ("supply_fan_on", SUPPLY_FAN_ON, ValueKind::Boolean),
    (
        "freeze_protection_stage",
        FREEZE_PROTECTION_STAGE,
        ValueKind::Integer,
    ),
];
const FUNNEL_OUTPUTS: &[(&str, &str)] = &[
    ("outdoor_damper_max_limit", OUTDOOR_DAMPER_MAX_PATH),
    ("return_damper_max_limit", RETURN_DAMPER_MAX_PATH),
    ("return_damper_min_limit", RETURN_DAMPER_MIN_PATH),
];

fn kind_str(kind: ValueKind) -> &'static str {
    match kind {
        ValueKind::Real => "Real",
        ValueKind::Integer => "Integer",
        ValueKind::Boolean => "Boolean",
    }
}

fn point_end(name: &str, kind: ValueKind) -> PointEnd {
    PointEnd {
        name: name.to_string(),
        unit: None,
        kind: Some(kind_str(kind).to_string()),
    }
}

fn tier4_config() -> VerifyConfig {
    let mut mapping: Vec<PointMapEntry> = FUNNEL_INPUTS
        .iter()
        .map(|(device, cdl, kind)| PointMapEntry {
            cdl: point_end(cdl, *kind),
            device: point_end(device, *kind),
        })
        .collect();
    mapping.extend(FUNNEL_OUTPUTS.iter().map(|(device, cdl)| PointMapEntry {
        cdl: point_end(cdl, ValueKind::Real),
        device: point_end(device, ValueKind::Real),
    }));
    VerifyConfig {
        references: vec![ReferenceSpec {
            model: "g36".to_string(),
            sequence: SEQUENCE.to_string(),
            point_name_mapping: mapping,
        }],
        tolerances: funnel_band_policy::zero_base(),
        outputs: FUNNEL_OUTPUTS
            .iter()
            .map(|(_, cdl)| funnel_band_policy::near_ulp_override(cdl))
            .collect(),
        indicators: Vec::new(),
        sampling: Some(SAMPLE_STEP),
        run_controller: true,
    }
}

fn tier4_options() -> DriverOptions {
    DriverOptions {
        cadence: DriveCadence::EventAligned {
            instants: (0..=T_STOP)
                .map(|tick| f64::from(tick) * SAMPLE_STEP)
                .collect(),
        },
        input_replay: DriverInputReplay::ReferenceTable,
        comparison: ComparisonMode::Funnel,
    }
}

fn tier_a_reference() -> CombiTimeTable {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(GOLDEN_DIR)
        .join(SEQUENCE)
        .join("reference.csv");
    CombiTimeTable::read(&path).expect("Tier-A reference read")
}

/// Add an out-of-band offset to one output column of a reference table, so a funnel/exact comparison
/// against it must fail — proving the tier is falsifiable, not decorative.
fn perturb_output_column(table: &mut CombiTimeTable, column: &str) {
    let idx = table
        .col_names
        .as_ref()
        .expect("columns")
        .iter()
        .position(|name| name == column)
        .unwrap_or_else(|| panic!("column {column} not found"));
    for row in 0..table.n_rows {
        table.data[row * table.n_cols + idx] += 10.0;
    }
}

// ---- Tests --------------------------------------------------------------------------------------

#[test]
fn conformance_report_populates_all_five_tiers_for_a_passing_run() {
    let tier4_ref = tier_a_reference();
    let golden = read_output_golden(&SPEC);
    let tier2_ref = driver_reference_from_output_golden(&SPEC, &golden);
    let tier2_config = config_for(&SPEC);
    let tier2_options = options_for(&SPEC);

    let report = assemble_report(&ConformanceRun {
        cxf: ECONOMIZER_ENABLE.as_bytes(),
        tier4: RunInputs {
            config: &tier4_config(),
            reference: &tier4_ref,
            options: &tier4_options(),
        },
        tier2: Tier2Source::Golden(RunInputs {
            config: &tier2_config,
            reference: &tier2_ref,
            options: &tier2_options,
        }),
    })
    .expect("assemble report");

    // Exactly five tiers, ordered Tier 0..Tier 4.
    let order: Vec<ConformanceTier> = report.tiers.iter().map(|tier| tier.tier).collect();
    assert_eq!(
        order,
        vec![
            ConformanceTier::Tier0,
            ConformanceTier::Tier1,
            ConformanceTier::Tier2,
            ConformanceTier::Tier3,
            ConformanceTier::Tier4,
        ]
    );

    assert!(
        matches!(
            status(&report, ConformanceTier::Tier0),
            TierStatus::Passed | TierStatus::Advisory
        ),
        "Tier 0 (static load) should pass or be advisory"
    );
    assert_eq!(status(&report, ConformanceTier::Tier1), TierStatus::Skipped);
    assert_eq!(status(&report, ConformanceTier::Tier2), TierStatus::Passed);
    assert_eq!(status(&report, ConformanceTier::Tier3), TierStatus::Skipped);
    assert_eq!(status(&report, ConformanceTier::Tier4), TierStatus::Passed);
    assert!(report.passed(), "a clean run must pass the report");
}

#[test]
fn conformance_report_is_deterministic() {
    let tier4_ref = tier_a_reference();
    let golden = read_output_golden(&SPEC);
    let tier2_ref = driver_reference_from_output_golden(&SPEC, &golden);
    let tier2_config = config_for(&SPEC);
    let tier2_options = options_for(&SPEC);
    let build = || {
        assemble_report(&ConformanceRun {
            cxf: ECONOMIZER_ENABLE.as_bytes(),
            tier4: RunInputs {
                config: &tier4_config(),
                reference: &tier4_ref,
                options: &tier4_options(),
            },
            tier2: Tier2Source::Golden(RunInputs {
                config: &tier2_config,
                reference: &tier2_ref,
                options: &tier2_options,
            }),
        })
        .expect("assemble report")
    };
    assert_eq!(build(), build(), "report assembly must be deterministic");
}

#[test]
fn out_of_band_tier4_oracle_fails_the_report() {
    let mut tier4_ref = tier_a_reference();
    perturb_output_column(&mut tier4_ref, "outdoor_damper_max_limit");
    let golden = read_output_golden(&SPEC);
    let tier2_ref = driver_reference_from_output_golden(&SPEC, &golden);
    let tier2_config = config_for(&SPEC);
    let tier2_options = options_for(&SPEC);

    let report = assemble_report(&ConformanceRun {
        cxf: ECONOMIZER_ENABLE.as_bytes(),
        tier4: RunInputs {
            config: &tier4_config(),
            reference: &tier4_ref,
            options: &tier4_options(),
        },
        tier2: Tier2Source::Golden(RunInputs {
            config: &tier2_config,
            reference: &tier2_ref,
            options: &tier2_options,
        }),
    })
    .expect("assemble report");

    assert_eq!(status(&report, ConformanceTier::Tier4), TierStatus::Failed);
    assert!(
        !report.passed(),
        "an out-of-band Tier 4 must fail the report"
    );
}

#[test]
fn regression_drift_fails_tier2() {
    let tier4_ref = tier_a_reference();
    let golden = read_output_golden(&SPEC);
    let mut tier2_ref = driver_reference_from_output_golden(&SPEC, &golden);
    perturb_output_column(&mut tier2_ref, OUTDOOR_DAMPER_MAX_SOURCE);
    let tier2_config = config_for(&SPEC);
    let tier2_options = options_for(&SPEC);

    let report = assemble_report(&ConformanceRun {
        cxf: ECONOMIZER_ENABLE.as_bytes(),
        tier4: RunInputs {
            config: &tier4_config(),
            reference: &tier4_ref,
            options: &tier4_options(),
        },
        tier2: Tier2Source::Golden(RunInputs {
            config: &tier2_config,
            reference: &tier2_ref,
            options: &tier2_options,
        }),
    })
    .expect("assemble report");

    assert_eq!(status(&report, ConformanceTier::Tier2), TierStatus::Failed);
    assert!(!report.passed(), "a Tier 2 regression must fail the report");
}

#[test]
fn unloadable_model_fails_tier0_and_skips_the_rest() {
    let tier4_ref = tier_a_reference();
    let report = assemble_report(&ConformanceRun {
        cxf: b"{ not a valid CXF document",
        tier4: RunInputs {
            config: &tier4_config(),
            reference: &tier4_ref,
            options: &tier4_options(),
        },
        tier2: Tier2Source::Skipped,
    })
    .expect("assemble report");

    assert_eq!(status(&report, ConformanceTier::Tier0), TierStatus::Failed);
    for tier in [
        ConformanceTier::Tier1,
        ConformanceTier::Tier2,
        ConformanceTier::Tier3,
        ConformanceTier::Tier4,
    ] {
        assert_eq!(
            status(&report, tier),
            TierStatus::Skipped,
            "{tier:?} must be skipped when the model fails to load"
        );
    }
    assert!(!report.passed(), "a load failure must fail the report");
}

fn status(report: &oce_conformance::ConformanceReport, tier: ConformanceTier) -> TierStatus {
    report
        .tier(tier)
        .unwrap_or_else(|| panic!("missing {tier:?}"))
        .status
}
