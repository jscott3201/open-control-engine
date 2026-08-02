//! G36 CoolingOnly.Controller Tier-2 mixed-kind determinism golden.
//!
//! This is an engine self-output snapshot, not the independent Tier-A correctness oracle.

use std::collections::HashMap;
use std::sync::OnceLock;

use oce_api::Value;
use oce_conformance::drive_trace_with_options;

#[allow(dead_code)]
#[path = "g36_determinism/support.rs"]
mod support;

use support::{
    PointSpec, SequenceSpec, assert_exact_comparisons_pass, assert_output_table_shape,
    assert_provenance_matches_outputs, bless_enabled, bless_sequence, captured_output_table,
    config_for, driver_reference_from_output_golden, options_for, pair, read_output_golden,
};

const FIXTURE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/cooling_only_controller.jsonld");
const REFERENCE_CSV: &str =
    include_str!("../../../tools/golden-gen/goldens/G36/cooling_only_controller/reference.csv");
const MODEL: &str = "http://example.org#g36.source.cooling_only_controller";

const INPUTS: &[PointSpec] = &[
    PointSpec::real("http://example.org#g36.source.cooling_only_controller.TZon"),
    PointSpec::real("http://example.org#g36.source.cooling_only_controller.TCooSet"),
    PointSpec::real("http://example.org#g36.source.cooling_only_controller.THeaSet"),
    PointSpec::boolean("http://example.org#g36.source.cooling_only_controller.u1Win"),
    PointSpec::boolean("http://example.org#g36.source.cooling_only_controller.u1Occ"),
    PointSpec::integer("http://example.org#g36.source.cooling_only_controller.uOpeMod"),
    PointSpec::real("http://example.org#g36.source.cooling_only_controller.ppmCO2Set"),
    PointSpec::real("http://example.org#g36.source.cooling_only_controller.ppmCO2"),
    PointSpec::real("http://example.org#g36.source.cooling_only_controller.TDis"),
    PointSpec::real("http://example.org#g36.source.cooling_only_controller.TSup"),
    PointSpec::real("http://example.org#g36.source.cooling_only_controller.VDis_flow"),
    PointSpec::integer("http://example.org#g36.source.cooling_only_controller.oveFloSet"),
    PointSpec::integer("http://example.org#g36.source.cooling_only_controller.oveDamPos"),
    PointSpec::boolean("http://example.org#g36.source.cooling_only_controller.u1Fan"),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(
        "http://example.org#g36.source.cooling_only_controller.VSet_flow",
        "http://example.org#g36.source.cooling_only_controller.dam.swi1.y",
    ),
    PointSpec::real_alias(
        "http://example.org#g36.source.cooling_only_controller.yDam",
        "http://example.org#g36.source.cooling_only_controller.dam.swi2.y",
    ),
    PointSpec::real_alias(
        "http://example.org#g36.source.cooling_only_controller.VAdjPopBreZon_flow",
        "http://example.org#g36.source.cooling_only_controller.setPoi.modPopBreAir.y",
    ),
    PointSpec::real_alias(
        "http://example.org#g36.source.cooling_only_controller.VAdjAreBreZon_flow",
        "http://example.org#g36.source.cooling_only_controller.setPoi.modAreBreAir.y",
    ),
    PointSpec::real_alias(
        "http://example.org#g36.source.cooling_only_controller.VMinOA_flow",
        "http://example.org#g36.source.cooling_only_controller.setPoi.minOA.y",
    ),
    PointSpec::integer_alias(
        "http://example.org#g36.source.cooling_only_controller.yZonTemResReq",
        "http://example.org#g36.source.cooling_only_controller.sysReq.intSwi.y",
    ),
    PointSpec::integer_alias(
        "http://example.org#g36.source.cooling_only_controller.yZonPreResReq",
        "http://example.org#g36.source.cooling_only_controller.sysReq.swi4.y",
    ),
    PointSpec::integer_alias(
        "http://example.org#g36.source.cooling_only_controller.yLowFloAla",
        "http://example.org#g36.source.cooling_only_controller.ala.proInt.y",
    ),
    PointSpec::integer_alias(
        "http://example.org#g36.source.cooling_only_controller.yFloSenAla",
        "http://example.org#g36.source.cooling_only_controller.ala.booToInt2.y",
    ),
    PointSpec::integer_alias(
        "http://example.org#g36.source.cooling_only_controller.yLeaDamAla",
        "http://example.org#g36.source.cooling_only_controller.ala.booToInt3.y",
    ),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "cooling_only_controller",
    cxf: FIXTURE,
    t_stop: 1_440,
    sample_step: 60.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: controller_inputs,
};

struct ReferenceTable {
    column_by_name: HashMap<String, usize>,
    rows: Vec<Vec<f64>>,
}

impl ReferenceTable {
    fn parse() -> Self {
        let columns = REFERENCE_CSV
            .lines()
            .find_map(|line| line.strip_prefix("# columns: "))
            .expect("reference columns")
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let column_by_name = columns
            .iter()
            .enumerate()
            .map(|(index, name)| (name.clone(), index))
            .collect();
        let rows = REFERENCE_CSV
            .lines()
            .filter(|line| {
                !line.is_empty() && !line.starts_with('#') && !line.starts_with("double ")
            })
            .map(|line| {
                line.split_whitespace()
                    .map(|cell| cell.parse::<f64>().expect("reference cell"))
                    .collect()
            })
            .collect::<Vec<Vec<f64>>>();
        assert_eq!(rows.len(), 1_441);
        Self {
            column_by_name,
            rows,
        }
    }

    fn value(&self, row: usize, name: &str) -> f64 {
        self.rows[row][self.column_by_name[name]]
    }
}

fn reference() -> &'static ReferenceTable {
    static REFERENCE: OnceLock<ReferenceTable> = OnceLock::new();
    REFERENCE.get_or_init(ReferenceTable::parse)
}

#[test]
fn controller_mixed_outputs_match_determinism_golden() {
    if bless_enabled() {
        bless_sequence(&SPEC);
    }

    let golden = read_output_golden(&SPEC);
    assert_provenance_matches_outputs(&SPEC, &golden);
    let reference = driver_reference_from_output_golden(&SPEC, &golden);
    let run = drive_trace_with_options(
        SPEC.cxf.as_bytes(),
        &config_for(&SPEC),
        &reference,
        &options_for(&SPEC),
    )
    .unwrap_or_else(|error| panic!("{} driver run failed: {error}", SPEC.name));

    assert_output_table_shape(&SPEC, &golden);
    assert_eq!(
        captured_output_table(&SPEC, &run),
        golden,
        "{} captured table drifted from committed golden",
        SPEC.name
    );
    assert_exact_comparisons_pass(&SPEC, golden.n_rows, &run.comparisons);
}

fn controller_inputs(t: f64) -> Vec<(String, Value)> {
    let row = (t / 60.0).round() as usize;
    assert!(row < 1_441, "unexpected input time {t}");
    let reference = reference();
    vec![
        pair(
            &format!("{MODEL}.TZon"),
            Value::Real(reference.value(row, "zone_temperature")),
        ),
        pair(
            &format!("{MODEL}.TCooSet"),
            Value::Real(reference.value(row, "cooling_setpoint")),
        ),
        pair(
            &format!("{MODEL}.THeaSet"),
            Value::Real(reference.value(row, "heating_setpoint")),
        ),
        pair(
            &format!("{MODEL}.u1Win"),
            Value::Boolean(reference.value(row, "window_status") != 0.0),
        ),
        pair(
            &format!("{MODEL}.u1Occ"),
            Value::Boolean(reference.value(row, "occupancy_status") != 0.0),
        ),
        pair(
            &format!("{MODEL}.uOpeMod"),
            Value::Integer(reference.value(row, "operating_mode") as i64),
        ),
        pair(
            &format!("{MODEL}.ppmCO2Set"),
            Value::Real(reference.value(row, "co2_setpoint")),
        ),
        pair(
            &format!("{MODEL}.ppmCO2"),
            Value::Real(reference.value(row, "co2_concentration")),
        ),
        pair(
            &format!("{MODEL}.TDis"),
            Value::Real(reference.value(row, "discharge_air_temperature")),
        ),
        pair(
            &format!("{MODEL}.TSup"),
            Value::Real(reference.value(row, "supply_air_temperature")),
        ),
        pair(
            &format!("{MODEL}.VDis_flow"),
            Value::Real(reference.value(row, "discharge_airflow")),
        ),
        pair(
            &format!("{MODEL}.oveFloSet"),
            Value::Integer(reference.value(row, "airflow_override_index") as i64),
        ),
        pair(
            &format!("{MODEL}.oveDamPos"),
            Value::Integer(reference.value(row, "damper_override_index") as i64),
        ),
        pair(
            &format!("{MODEL}.u1Fan"),
            Value::Boolean(reference.value(row, "supply_fan_status") != 0.0),
        ),
    ]
}
