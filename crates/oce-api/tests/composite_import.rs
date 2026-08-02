//! Full-pipeline smoke tests for nested CXF composite import.

use std::collections::BTreeSet;
use std::fs;

use oce_api::{CollectSpec, Engine, InputSource, PointDirection, SimSpec, Value};
use oce_store::ModelStore;

const FANOUT: &str = include_str!("../../oce-cxf/tests/fixtures/boundary_fanout.jsonld");
const NESTED: &str = include_str!("../../oce-cxf/tests/fixtures/nested_composite.jsonld");
const G36_TRIM_AND_RESPOND: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/trim_and_respond_have_hol_false.jsonld");
const G36_SUPPLY_TEMPERATURE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_supply_temperature.jsonld");
const G36_SUPPLY_FAN: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_supply_fan.jsonld");
const G36_SUPPLY_SIGNALS: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_supply_signals.jsonld");
const FANOUT_INPUT: &str = "http://example.org#g36.profile.boundary_fanout.u";
const INPUT: &str = "http://example.org#g36.profile.nested_composite.u";
const G36_TRIM_MODEL: &str = "http://example.org#g36.source.trim_and_respond_have_hol_false";
const G36_SUPPLY_TEMPERATURE_MODEL: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature";
const G36_SUPPLY_FAN_MODEL: &str = "http://example.org#g36.source.multizone_vav_supply_fan";
const G36_SUPPLY_SIGNALS_MODEL: &str = "http://example.org#g36.source.multizone_vav_supply_signals";
const DEVELOPMENT_NAMED_POINTS: &str = include_str!("fixtures/development_named_points.tsv");

#[test]
fn authored_connector_iris_do_not_move_corpus_host_point_inventory() {
    let expected = DEVELOPMENT_NAMED_POINTS
        .lines()
        .map(|line| {
            let mut fields = line.split('\t');
            let row = (
                fields.next().expect("baseline fixture"),
                fields.next().expect("baseline direction"),
                fields.next().expect("baseline point path"),
            );
            assert!(fields.next().is_none(), "unexpected baseline field: {line}");
            row
        })
        .collect::<BTreeSet<_>>();
    let fixture_dir = format!(
        "{}/../oce-cxf/tests/fixtures/g36",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut fixtures = fs::read_dir(fixture_dir)
        .expect("read G36 fixture corpus")
        .map(|entry| entry.expect("fixture directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonld")
        })
        .collect::<Vec<_>>();
    fixtures.sort();

    let mut actual = BTreeSet::new();
    let mut total_points = 0;
    for fixture in fixtures {
        let fixture_name = fixture
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("UTF-8 fixture stem");
        let bytes = fs::read(&fixture).expect("read G36 fixture");
        let mut engine = Engine::in_memory();
        engine
            .load_cxf(&bytes)
            .unwrap_or_else(|error| panic!("{fixture_name} loads: {error:?}"));
        for point in engine
            .point_list(None)
            .expect("point inventory is available")
        {
            total_points += 1;
            actual.insert((
                fixture_name.to_owned(),
                match point.direction {
                    PointDirection::In => "In",
                    PointDirection::Out => "Out",
                },
                point.path,
            ));
        }
    }

    let expected = expected
        .into_iter()
        .map(|(fixture, direction, path)| (fixture.to_owned(), direction, path.to_owned()))
        .collect::<BTreeSet<_>>();
    assert_eq!(total_points, 2_895, "development corpus point count moved");
    assert_eq!(actual.len(), 2_895, "authored point inventory count moved");
    assert_eq!(
        actual, expected,
        "host-visible authored points must remain byte-identical to the checked-in inventory"
    );
}

#[test]
fn nested_composite_loads_builds_and_ticks_through_frozen_facade() {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(NESTED.as_bytes())
        .expect("nested CXF composite loads");
    assert_eq!(report.block_count, 3);
    assert_eq!(report.stateful_blocks, 0);
    assert!(
        report.warnings.is_empty(),
        "fixture should not warn: {:?}",
        report.warnings
    );
    let output = engine
        .io()
        .iter()
        .filter(|point| point.direction == PointDirection::Out)
        .last()
        .expect("post gain output is visible")
        .path
        .clone();

    let metrics = engine
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop: 2.0,
            step: 1.0,
            inputs: InputSource::Closure(Box::new(|_| vec![(INPUT.to_owned(), Value::Real(8.0))])),
            collect: CollectSpec::Named {
                points: vec![output.clone()],
                stride: 1,
            },
        })
        .expect("nested CXF composite simulates");

    assert_eq!(metrics.trace.columns(), &[output]);
    for (row, value) in metrics
        .trace
        .column(0)
        .expect("output column")
        .iter()
        .enumerate()
    {
        assert!(
            value.bit_eq(&Value::Real(8.0)),
            "row {row} should carry top input through 0.5 then 2.0 gains, got {value:?}"
        );
    }
}

#[test]
fn boundary_input_fanout_loads_as_one_point_and_stages_every_target() {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(FANOUT.as_bytes())
        .expect("boundary fanout CXF composite loads");
    assert_eq!(report.block_count, 3);
    assert_eq!(report.stateful_blocks, 0);
    assert!(
        report.warnings.is_empty(),
        "fixture should not warn: {:?}",
        report.warnings
    );

    let inventory_paths: Vec<String> = engine.io().iter().map(|point| point.path).collect();
    assert_eq!(
        inventory_paths
            .iter()
            .filter(|path| path.as_str() == FANOUT_INPUT)
            .count(),
        1,
        "fanout input should be exposed once as a logical host point"
    );
    let resolved = engine
        .store()
        .load_model(&report.model_id)
        .expect("load stores resolved model");
    assert_eq!(
        resolved
            .points
            .iter()
            .filter(|point| point.key.as_str() == FANOUT_INPUT)
            .count(),
        1,
        "durable point projection should also coalesce the fanout key"
    );
    let output = engine
        .io()
        .iter()
        .filter(|point| point.direction == PointDirection::Out)
        .last()
        .expect("fanout output is visible")
        .path
        .clone();

    engine
        .set_input(FANOUT_INPUT, Value::Real(4.0))
        .expect("fanout input stages");
    engine.tick(0.0).expect("fanout model ticks");
    assert!(
        engine
            .get_output(&output)
            .expect("fanout output exists")
            .bit_eq(&Value::Real(20.0)),
        "fanout output should compute 2*u + 3*u"
    );
}

#[test]
fn source_verified_g36_trim_and_respond_fixture_loads_through_frozen_facade() {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(G36_TRIM_AND_RESPOND.as_bytes())
        .expect("source-verified G36 TrimAndRespond CXF fixture loads");
    assert_eq!(report.block_count, 44);
    assert_eq!(report.stateful_blocks, 5);
    assert!(
        report.warnings.is_empty(),
        "fixture should not warn: {:?}",
        report.warnings
    );

    let paths: Vec<String> = engine.io().iter().map(|point| point.path).collect();
    let num_of_req = format!("{G36_TRIM_MODEL}.numOfReq");
    let device_status = format!("{G36_TRIM_MODEL}.uDevSta");
    let hold = format!("{G36_TRIM_MODEL}.uHol");
    assert!(paths.contains(&num_of_req));
    assert!(paths.contains(&device_status));
    assert!(
        !paths.contains(&hold),
        "inactive have_hol=false optional input should not survive facade IO export"
    );
}

#[test]
fn source_verified_g36_supply_temperature_fixture_loads_through_frozen_facade() {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(G36_SUPPLY_TEMPERATURE.as_bytes())
        .expect("source-verified G36 SupplyTemperature CXF fixture loads");
    assert_eq!(report.block_count, 62);
    assert_eq!(report.stateful_blocks, 5);
    assert!(
        report.warnings.is_empty(),
        "fixture should not warn: {:?}",
        report.warnings
    );

    let paths: Vec<String> = engine.io().iter().map(|point| point.path).collect();
    let outdoor_temp = format!("{G36_SUPPLY_TEMPERATURE_MODEL}.TOut");
    let fan_status = format!("{G36_SUPPLY_TEMPERATURE_MODEL}.u1SupFan");
    let operating_mode = format!("{G36_SUPPLY_TEMPERATURE_MODEL}.uOpeMod");
    let requests = format!("{G36_SUPPLY_TEMPERATURE_MODEL}.uZonTemResReq");
    let hold = format!("{G36_SUPPLY_TEMPERATURE_MODEL}.maxSupTemRes.uHol");
    for required in [&outdoor_temp, &fan_status, &operating_mode, &requests] {
        assert!(paths.contains(required), "missing facade input {required}");
    }
    assert!(
        !paths.contains(&hold),
        "inactive nested have_hol=false optional input should not survive facade IO export"
    );

    let output = &format!("{G36_SUPPLY_TEMPERATURE_MODEL}.swi3.y");
    assert!(
        paths.contains(output),
        "SupplyTemperature output {output} should be visible"
    );

    engine
        .set_input(&outdoor_temp, Value::Real(289.15))
        .expect("outdoor temp stages");
    engine
        .set_input(&fan_status, Value::Boolean(true))
        .expect("fan status stages");
    engine
        .set_input(&operating_mode, Value::Integer(1))
        .expect("operating mode stages");
    engine
        .set_input(&requests, Value::Integer(0))
        .expect("zone request stages");
    engine.tick(0.0).expect("SupplyTemperature model ticks");
    let occupied_output = engine
        .get_output(output)
        .expect("SupplyTemperature output exists");
    match occupied_output {
        Value::Real(value) => assert!(
            (value - 291.15).abs() <= 1e-12,
            "occupied/reset branch should start at TSupCoo_max when TOut is at TOut_min, got {value}"
        ),
        other => panic!("SupplyTemperature output must be Real, got {other:?}"),
    }

    engine
        .set_input(&fan_status, Value::Boolean(false))
        .expect("fan status stages");
    engine.tick(1.0).expect("SupplyTemperature model ticks");
    assert!(
        engine
            .get_output(output)
            .expect("SupplyTemperature output exists")
            .bit_eq(&Value::Real(299.15)),
        "fan-off branch should emit the source deadband temperature"
    );
}

#[test]
fn source_verified_g36_supply_fan_fixture_loads_through_frozen_facade() {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(G36_SUPPLY_FAN.as_bytes())
        .expect("source-verified G36 SupplyFan CXF fixture loads");
    assert_eq!(report.block_count, 65);
    assert_eq!(report.stateful_blocks, 7);
    assert!(
        report.warnings.is_empty(),
        "fixture should not warn: {:?}",
        report.warnings
    );

    let paths: Vec<String> = engine.io().iter().map(|point| point.path).collect();
    let operating_mode = format!("{G36_SUPPLY_FAN_MODEL}.uOpeMod");
    let duct_pressure = format!("{G36_SUPPLY_FAN_MODEL}.dpDuc");
    let pressure_requests = format!("{G36_SUPPLY_FAN_MODEL}.uZonPreResReq");
    let hold = format!("{G36_SUPPLY_FAN_MODEL}.staPreSetRes.uHol");
    for required in [&operating_mode, &duct_pressure, &pressure_requests] {
        assert!(paths.contains(required), "missing facade input {required}");
    }
    assert!(
        !paths.contains(&hold),
        "inactive nested have_hol=false optional input should not survive facade IO export"
    );

    let fan_speed = &format!("{G36_SUPPLY_FAN_MODEL}.swi.y");
    let fan_status = &format!("{G36_SUPPLY_FAN_MODEL}.or1.y");
    for output in [fan_speed, fan_status] {
        assert!(
            paths.contains(output),
            "SupplyFan output {output} should be visible"
        );
    }

    engine
        .set_input(&operating_mode, Value::Integer(1))
        .expect("operating mode stages");
    engine
        .set_input(&duct_pressure, Value::Real(120.0))
        .expect("duct pressure stages");
    engine
        .set_input(&pressure_requests, Value::Integer(0))
        .expect("pressure reset requests stage");
    engine.tick(0.0).expect("SupplyFan model ticks");
    assert!(
        engine
            .get_output(fan_status)
            .expect("SupplyFan status output exists")
            .bit_eq(&Value::Boolean(true)),
        "occupied/setup/cooldown modes should enable the fan in the default no-perimeter variant"
    );
    match engine
        .get_output(fan_speed)
        .expect("SupplyFan speed output exists")
    {
        Value::Real(value) => assert!(
            (0.1..=1.0).contains(&value),
            "enabled fan speed should stay within [minSpe,maxSpe], got {value}"
        ),
        other => panic!("SupplyFan speed output must be Real, got {other:?}"),
    }

    engine
        .set_input(&operating_mode, Value::Integer(4))
        .expect("operating mode stages");
    engine.tick(1.0).expect("SupplyFan model ticks");
    assert!(
        engine
            .get_output(fan_status)
            .expect("SupplyFan status output exists")
            .bit_eq(&Value::Boolean(false)),
        "warm-up mode should leave the default no-perimeter fan disabled"
    );
    assert!(
        engine
            .get_output(fan_speed)
            .expect("SupplyFan speed output exists")
            .bit_eq(&Value::Real(0.0)),
        "fan-off branch should force zero fan speed"
    );
}

#[test]
fn source_verified_g36_supply_signals_fixture_loads_through_frozen_facade() {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(G36_SUPPLY_SIGNALS.as_bytes())
        .expect("source-verified G36 SupplySignals CXF fixture loads");
    assert_eq!(report.block_count, 9);
    assert_eq!(report.stateful_blocks, 1);
    assert!(
        report.warnings.is_empty(),
        "fixture should not warn: {:?}",
        report.warnings
    );

    let paths: Vec<String> = engine.io().iter().map(|point| point.path).collect();
    let measured_temp = format!("{G36_SUPPLY_SIGNALS_MODEL}.TAirSup");
    let setpoint = format!("{G36_SUPPLY_SIGNALS_MODEL}.TAirSupSet");
    let fan_status = format!("{G36_SUPPLY_SIGNALS_MODEL}.u1SupFan");
    for required in [&measured_temp, &setpoint, &fan_status] {
        assert!(paths.contains(required), "missing facade input {required}");
    }

    let u_t_sup = &format!("{G36_SUPPLY_SIGNALS_MODEL}.swi.y");
    let cooling = &format!("{G36_SUPPLY_SIGNALS_MODEL}.conSigCoo.y");
    let heating = &format!("{G36_SUPPLY_SIGNALS_MODEL}.conSigHea.y");
    for output in [u_t_sup, cooling, heating] {
        assert!(
            paths.contains(output),
            "SupplySignals output {output} should be visible"
        );
    }

    engine
        .set_input(&setpoint, Value::Real(295.0))
        .expect("setpoint stages");
    engine
        .set_input(&measured_temp, Value::Real(300.0))
        .expect("measured temperature stages");
    engine
        .set_input(&fan_status, Value::Boolean(false))
        .expect("fan status stages");
    engine.tick(0.0).expect("SupplySignals model ticks");
    assert!(
        engine
            .get_output(u_t_sup)
            .expect("SupplySignals signal output exists")
            .bit_eq(&Value::Real(0.0)),
        "fan-off branch should force zero supply-temperature loop signal"
    );

    engine
        .set_input(&fan_status, Value::Boolean(true))
        .expect("fan status stages");
    engine.tick(1.0).expect("SupplySignals model ticks");
    assert!(
        engine
            .get_output(u_t_sup)
            .expect("SupplySignals signal output exists")
            .bit_eq(&Value::Real(0.25)),
        "rising fan trigger resets after emit, so the first enabled tick sees the current P term"
    );

    engine.tick(2.0).expect("SupplySignals model ticks");
    assert!(
        engine
            .get_output(u_t_sup)
            .expect("SupplySignals signal output exists")
            .bit_eq(&Value::Real(0.0)),
        "PIDWithReset reset target should be visible on the next tick"
    );

    engine
        .set_input(&setpoint, Value::Real(320.0))
        .expect("setpoint stages");
    engine
        .set_input(&measured_temp, Value::Real(295.0))
        .expect("measured temperature stages");
    engine.tick(3.0).expect("SupplySignals model ticks");
    assert!(
        engine
            .get_output(heating)
            .expect("SupplySignals heating output exists")
            .bit_eq(&Value::Real(1.0)),
        "large negative loop signal should fully open the heating coil command"
    );
    match engine
        .get_output(cooling)
        .expect("SupplySignals cooling output exists")
    {
        Value::Real(value) => assert!(
            value.abs() <= 1e-12,
            "heating-side command should leave cooling coil closed, got {value}"
        ),
        other => panic!("SupplySignals cooling output must be Real, got {other:?}"),
    }
}
