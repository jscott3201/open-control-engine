//! Complete-frame cadence and domain controls. Pre expectations are hand-derived HostTick
//! recurrences, not Modelica event-iteration evidence; no reference fixture is regenerated.

use oce_api::OcError;
use oce_conformance::{
    CombiTimeTable, ComparisonMode, DriveCadence, DriverError, DriverOptions, OutputPattern,
    PartialTolerances, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, VerifyConfig,
    drive_trace_with_options, escape_regex,
};

const PRE: &[u8] = include_bytes!("../../oce-api/tests/fixtures/frame_pre.jsonld");

fn mapping(name: &str, path: &str, kind: &str) -> PointMapEntry {
    let end = |name: &str| PointEnd {
        name: name.to_owned(),
        unit: None,
        kind: Some(kind.to_owned()),
    };
    PointMapEntry {
        device: end(name),
        cdl: end(path),
    }
}

fn config(mappings: Vec<PointMapEntry>, outputs: &[&str]) -> VerifyConfig {
    VerifyConfig {
        references: vec![ReferenceSpec {
            model: "frame-cadence".into(),
            sequence: "hosttick".into(),
            point_name_mapping: mappings,
        }],
        tolerances: Tolerances {
            atolx: 0.0,
            atoly: 0.0,
            rtolx: 0.0,
            rtoly: 0.0,
            ltolx: 0.0,
            ltoly: 0.0,
        },
        outputs: outputs
            .iter()
            .map(|p| OutputPattern {
                pattern: format!("^{}$", escape_regex(p)),
                tolerances: PartialTolerances::default(),
            })
            .collect(),
        indicators: Vec::new(),
        sampling: None,
        run_controller: true,
    }
}

fn pre_config() -> VerifyConfig {
    config(
        vec![
            mapping("a", "urn:pre:a", "Boolean"),
            mapping("z", "urn:pre:z", "Boolean"),
        ],
        &["urn:pre:a", "urn:pre:z"],
    )
}

fn pre_reference(times: &[f64]) -> CombiTimeTable {
    CombiTimeTable {
        name: "pre_reference".into(),
        n_rows: times.len(),
        n_cols: 3,
        data: times
            .iter()
            .enumerate()
            .flat_map(|(i, &t)| [t, (i % 2) as f64, (i % 2) as f64])
            .collect(),
        col_names: Some(vec!["time".into(), "a".into(), "z".into()]),
    }
}

#[test]
fn rounded_uniform_grid_and_explicit_equal_times_match_the_hand_recurrence() {
    let base = 9_007_199_254_740_992.0;
    // Fresh binary64 multiplication/addition: half-ULP ties round to even at this magnitude.
    let times = [base, base, base, base + 2.0, base + 2.0];
    let reference = pre_reference(&times);
    let mut retained = None;
    for _ in 0..2 {
        for cadence in [
            DriveCadence::Uniform {
                t_start: base,
                t_stop: base + 2.0,
                step: 0.5,
            },
            DriveCadence::EventAligned {
                instants: times.to_vec(),
            },
        ] {
            let run = drive_trace_with_options(
                PRE,
                &pre_config(),
                &reference,
                &DriverOptions {
                    cadence,
                    comparison: ComparisonMode::Exact,
                    ..DriverOptions::default()
                },
            )
            .unwrap();
            assert!(run.comparisons.iter().all(|c| c.result.passed()));
            assert_eq!(
                run.trace
                    .times
                    .iter()
                    .map(|t| t.to_bits())
                    .collect::<Vec<_>>(),
                times.map(f64::to_bits)
            );
            for column in &run.trace.columns {
                assert_eq!(
                    column
                        .values
                        .iter()
                        .map(|v| v.to_bits())
                        .collect::<Vec<_>>(),
                    [0.0_f64, 1.0, 0.0, 1.0, 0.0].map(f64::to_bits)
                );
            }
            let bytes = run.trace.to_table("cadence").to_csv_string().unwrap();
            if let Some(expected) = &retained {
                assert_eq!(&bytes, expected);
            } else {
                retained = Some(bytes);
            }
        }
    }
}

#[test]
fn negative_times_and_signed_zero_keep_the_selected_cadence_bits() {
    for (times, cadence) in [
        (
            vec![-1.0, -0.5, 0.0],
            DriveCadence::Uniform {
                t_start: -1.0,
                t_stop: 0.0,
                step: 0.5,
            },
        ),
        (
            vec![-0.0],
            DriveCadence::EventAligned {
                instants: vec![-0.0],
            },
        ),
        // The uniform grid retains its historical fresh multiply, including -0 + 0 = +0.
        (
            vec![0.0],
            DriveCadence::Uniform {
                t_start: -0.0,
                t_stop: 0.0,
                step: 1.0,
            },
        ),
    ] {
        let run = drive_trace_with_options(
            PRE,
            &pre_config(),
            &pre_reference(&times),
            &DriverOptions {
                cadence,
                comparison: ComparisonMode::Exact,
                ..DriverOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            run.trace
                .times
                .iter()
                .map(|t| t.to_bits())
                .collect::<Vec<_>>(),
            times.iter().map(|t| t.to_bits()).collect::<Vec<_>>()
        );
        assert!(run.comparisons.iter().all(|c| c.result.passed()));
    }
}

#[test]
fn invalid_uniform_step_and_time_bounds_refuse_before_comparison() {
    let drive = |cadence| {
        drive_trace_with_options(
            PRE,
            &pre_config(),
            &pre_reference(&[0.0]),
            &DriverOptions {
                cadence,
                comparison: ComparisonMode::Exact,
                ..DriverOptions::default()
            },
        )
    };
    for step in [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            drive(DriveCadence::Uniform {
                t_start: 0.0,
                t_stop: 1.0,
                step
            }),
            Err(DriverError::Engine(OcError::Load { .. }))
        ));
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for (t_start, t_stop) in [(bad, 1.0), (0.0, bad)] {
            assert!(
                matches!(drive(DriveCadence::Uniform { t_start, t_stop, step: 1.0 }),
                Err(DriverError::Engine(OcError::NonFiniteTime { now })) if now.to_bits() == bad.to_bits())
            );
        }
    }
    assert!(
        matches!(drive(DriveCadence::Uniform { t_start: 1.0, t_stop: 0.0, step: 1.0 }),
        Err(DriverError::Engine(OcError::TimeRegression { now, prev })) if now.to_bits() == 0 && prev.to_bits() == 1.0_f64.to_bits())
    );
}

#[test]
fn default_integer_domain_is_not_widened_by_either_driver_cadence() {
    const ROOT: &str = "http://example.org#PassThroughMiniature.";
    let integer = format!("{ROOT}integerIn");
    let output = format!("{ROOT}integerOut");
    let settings = config(
        vec![
            mapping("real", &format!("{ROOT}realIn"), "Real"),
            mapping("integer", &integer, "Integer"),
            mapping("boolean", &format!("{ROOT}booleanIn"), "Boolean"),
            mapping("output", &output, "Integer"),
        ],
        &[&output],
    );
    for value in [i64::from(i32::MIN) - 1, i64::from(i32::MAX) + 1] {
        let reference = CombiTimeTable {
            name: "integer_domain".into(),
            n_rows: 1,
            n_cols: 5,
            data: vec![0.0, 0.0, value as f64, 0.0, 0.0],
            col_names: Some(
                ["time", "real", "integer", "boolean", "output"]
                    .map(str::to_owned)
                    .to_vec(),
            ),
        };
        for cadence in [
            DriveCadence::Uniform {
                t_start: 0.0,
                t_stop: 0.0,
                step: 1.0,
            },
            DriveCadence::EventAligned {
                instants: vec![0.0],
            },
        ] {
            let result = drive_trace_with_options(
                include_bytes!("../../oce-cxf/tests/fixtures/pass_through_miniature.jsonld"),
                &settings,
                &reference,
                &DriverOptions {
                    cadence,
                    ..DriverOptions::default()
                },
            );
            assert!(
                matches!(result, Err(DriverError::Engine(OcError::InputDomain(ref path))) if path == &integer),
                "{result:?}"
            );
        }
    }
}
