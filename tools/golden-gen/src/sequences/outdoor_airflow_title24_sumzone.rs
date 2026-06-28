//! G36 MultiZone VAV OutdoorAirFlow Title 24 SumZone sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{OUTDOOR_AIRFLOW_TITLE24_SUMZONE, input_i, input_r, r, sequence_golden};

pub(super) fn goldens() -> Vec<Golden> {
    let time: Vec<f64> = (0..6).map(f64::from).collect();
    let operation_mode_1 = [1, 1, 4, 7, 1, 3];
    let operation_mode_2 = [1, 7, 1, 6, 1, 1];
    let absolute_min_flow_1 = [1.0, 2.0, 2.5, 10.0, 0.0, 1.25];
    let absolute_min_flow_2 = [2.0, 4.0, 0.5, 20.0, 0.0, 2.5];
    let absolute_min_flow_3 = [3.0, 8.0, 1.5, 30.0, 0.0, 5.0];
    let design_min_flow_1 = [4.0, 1.0, 6.0, 3.0, 0.0, 8.0];
    let design_min_flow_2 = [5.0, 3.0, 2.0, 2.0, 1.0, 13.0];
    let design_min_flow_3 = [6.0, 5.0, 1.0, 1.0, 0.0, 21.0];
    let co2_1 = [0.1, -0.5, 0.9, 0.0, -1.0, 1.2];
    let co2_2 = [0.6, 0.0, 0.3, 0.0, -2.0, 1.2];
    let co2_3 = [0.2, 0.2, 0.7, 0.0, -3.0, 1.1];

    let mut summed_absolute = Vec::with_capacity(time.len());
    let mut summed_design = Vec::with_capacity(time.len());
    let mut maximum_co2 = Vec::with_capacity(time.len());

    for row in 0..time.len() {
        let outputs = outdoor_airflow_title24_sumzone_outputs(
            operation_mode_1[row],
            operation_mode_2[row],
            [
                absolute_min_flow_1[row],
                absolute_min_flow_2[row],
                absolute_min_flow_3[row],
            ],
            [
                design_min_flow_1[row],
                design_min_flow_2[row],
                design_min_flow_3[row],
            ],
            [co2_1[row], co2_2[row], co2_3[row]],
        );
        summed_absolute.push(outputs.summed_absolute);
        summed_design.push(outputs.summed_design);
        maximum_co2.push(outputs.maximum_co2);
    }

    let inputs = outdoor_airflow_title24_sumzone_inputs(Title24SumZoneInputColumns {
        operation_mode_1: &operation_mode_1,
        operation_mode_2: &operation_mode_2,
        absolute_min_flow_1: &absolute_min_flow_1,
        absolute_min_flow_2: &absolute_min_flow_2,
        absolute_min_flow_3: &absolute_min_flow_3,
        design_min_flow_1: &design_min_flow_1,
        design_min_flow_2: &design_min_flow_2,
        design_min_flow_3: &design_min_flow_3,
        co2_1: &co2_1,
        co2_2: &co2_2,
        co2_3: &co2_3,
    });

    vec![
        sequence_golden(
            OUTDOOR_AIRFLOW_TITLE24_SUMZONE,
            "summed_absolute_minimum_outdoor_airflow",
            ValueKind::Real,
            time.clone(),
            summed_absolute.into_iter().map(r).collect(),
            "OutdoorAirFlow.Title24.SumZone nGro=2/nZon=3: overlapping zone-group matrix covers both occupied, one occupied, none occupied, and zero-flow rows",
            "Pinned Title24/SumZone.mo: groFlo = zonGroMat * VZonAbsMin_flow with zonGroMat=[1,1,0;0,1,1]; occupied groups pass through via BooleanToReal, then MultiSum",
            inputs.clone(),
        ),
        sequence_golden(
            OUTDOOR_AIRFLOW_TITLE24_SUMZONE,
            "summed_design_minimum_outdoor_airflow",
            ValueKind::Real,
            time.clone(),
            summed_design.into_iter().map(r).collect(),
            "OutdoorAirFlow.Title24.SumZone nGro=2/nZon=3: design minimum aggregation follows the same occupancy-gated group matrix",
            "Pinned Title24/SumZone.mo: groFlo1 = zonGroMat * VZonDesMin_flow; occupied operation mode is G36 OperationModes.occupied=1; unoccupied groups contribute zero",
            inputs.clone(),
        ),
        sequence_golden(
            OUTDOOR_AIRFLOW_TITLE24_SUMZONE,
            "maximum_co2_loop",
            ValueKind::Real,
            time,
            maximum_co2.into_iter().map(r).collect(),
            "OutdoorAirFlow.Title24.SumZone have_CO2Sen=true: max CO2 covers positive, negative, equal maxima, and zero rows",
            "Pinned Title24/SumZone.mo conditional CO2 branch: yMaxCO2 = max(uCO2[1], uCO2[2], uCO2[3]) with no occupancy gate",
            inputs,
        ),
    ]
}

struct Title24SumZoneOutputs {
    summed_absolute: f64,
    summed_design: f64,
    maximum_co2: f64,
}

fn outdoor_airflow_title24_sumzone_outputs(
    operation_mode_1: i64,
    operation_mode_2: i64,
    absolute_min_flow: [f64; 3],
    design_min_flow: [f64; 3],
    co2: [f64; 3],
) -> Title24SumZoneOutputs {
    let occupied = [operation_mode_1 == 1, operation_mode_2 == 1];
    let absolute_group = [
        absolute_min_flow[0] + absolute_min_flow[1],
        absolute_min_flow[1] + absolute_min_flow[2],
    ];
    let design_group = [
        design_min_flow[0] + design_min_flow[1],
        design_min_flow[1] + design_min_flow[2],
    ];

    Title24SumZoneOutputs {
        summed_absolute: gated(occupied[0], absolute_group[0])
            + gated(occupied[1], absolute_group[1]),
        summed_design: gated(occupied[0], design_group[0]) + gated(occupied[1], design_group[1]),
        maximum_co2: co2[0].max(co2[1]).max(co2[2]),
    }
}

fn gated(enabled: bool, value: f64) -> f64 {
    if enabled { value } else { 0.0 }
}

struct Title24SumZoneInputColumns<'a> {
    operation_mode_1: &'a [i64],
    operation_mode_2: &'a [i64],
    absolute_min_flow_1: &'a [f64],
    absolute_min_flow_2: &'a [f64],
    absolute_min_flow_3: &'a [f64],
    design_min_flow_1: &'a [f64],
    design_min_flow_2: &'a [f64],
    design_min_flow_3: &'a [f64],
    co2_1: &'a [f64],
    co2_2: &'a [f64],
    co2_3: &'a [f64],
}

fn outdoor_airflow_title24_sumzone_inputs(
    input: Title24SumZoneInputColumns<'_>,
) -> Vec<InputSeries> {
    vec![
        input_i("operation_mode_1", input.operation_mode_1.iter().copied()),
        input_i("operation_mode_2", input.operation_mode_2.iter().copied()),
        input_r(
            "absolute_min_flow_1",
            input.absolute_min_flow_1.iter().copied(),
        ),
        input_r(
            "absolute_min_flow_2",
            input.absolute_min_flow_2.iter().copied(),
        ),
        input_r(
            "absolute_min_flow_3",
            input.absolute_min_flow_3.iter().copied(),
        ),
        input_r("design_min_flow_1", input.design_min_flow_1.iter().copied()),
        input_r("design_min_flow_2", input.design_min_flow_2.iter().copied()),
        input_r("design_min_flow_3", input.design_min_flow_3.iter().copied()),
        input_r("co2_1", input.co2_1.iter().copied()),
        input_r("co2_2", input.co2_2.iter().copied()),
        input_r("co2_3", input.co2_3.iter().copied()),
    ]
}
