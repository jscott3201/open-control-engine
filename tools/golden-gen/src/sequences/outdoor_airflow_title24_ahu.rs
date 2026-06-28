//! G36 MultiZone VAV OutdoorAirFlow Title 24 AHU sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{OUTDOOR_AIRFLOW_TITLE24_AHU, input_r, r, sequence_golden};

pub(super) fn goldens() -> Vec<Golden> {
    let time: Vec<f64> = (0..6).map(f64::from).collect();
    let absolute_min_flow = [1.0, 4.0, 4.0, 2.5, 0.0, 1.0];
    let design_min_flow = [2.0, 10.0, 10.0, 4.0, 0.0, 5.0];
    let co2_loop_max = [0.0, 0.5, 0.75, 1.4, 1.0, -0.25];
    let measured_outdoor_air = [4.0, 6.0, 0.0, 9.0, 8.0, 1.6];

    let mut effective_absolute = Vec::with_capacity(time.len());
    let mut effective_absolute_normalized = Vec::with_capacity(time.len());
    let mut effective_design = Vec::with_capacity(time.len());
    let mut effective_design_normalized = Vec::with_capacity(time.len());
    let mut effective_outdoor_air_normalized = Vec::with_capacity(time.len());
    let mut measured_normalized = Vec::with_capacity(time.len());

    for row in 0..time.len() {
        let outputs = outdoor_airflow_title24_ahu_outputs(
            absolute_min_flow[row],
            design_min_flow[row],
            co2_loop_max[row],
            measured_outdoor_air[row],
        );
        effective_absolute.push(outputs.effective_absolute);
        effective_absolute_normalized.push(outputs.effective_absolute_normalized);
        effective_design.push(outputs.effective_design);
        effective_design_normalized.push(outputs.effective_design_normalized);
        effective_outdoor_air_normalized.push(outputs.effective_outdoor_air_normalized);
        measured_normalized.push(outputs.measured_normalized);
    }

    let inputs = outdoor_airflow_title24_inputs(
        &absolute_min_flow,
        &design_min_flow,
        &co2_loop_max,
        &measured_outdoor_air,
    );

    vec![
        sequence_golden(
            OUTDOOR_AIRFLOW_TITLE24_AHU,
            "effective_absolute_outdoor_airflow",
            ValueKind::Real,
            time.clone(),
            effective_absolute.into_iter().map(r).collect(),
            "OutdoorAirFlow.Title24.AHU SingleDamper with CO2: absolute-min zone sum covers pass-through, cap, and zero cases",
            "Pinned Title24/AHU.mo: VEffAbsOutAir_flow = min(VAbsOutAir_flow, VSumZonAbsMin_flow) with VAbsOutAir_flow=3",
            inputs.clone(),
        ),
        sequence_golden(
            OUTDOOR_AIRFLOW_TITLE24_AHU,
            "effective_absolute_outdoor_airflow_normalized",
            ValueKind::Real,
            time.clone(),
            effective_absolute_normalized.into_iter().map(r).collect(),
            "OutdoorAirFlow.Title24.AHU SingleDamper with CO2: absolute minimum normalized by guarded design absolute flow",
            "Pinned Title24/AHU.mo: effAbsOutAir_normalized = VEffAbsOutAir_flow / max(VAbsOutAir_flow, 1E-4)",
            inputs.clone(),
        ),
        sequence_golden(
            OUTDOOR_AIRFLOW_TITLE24_AHU,
            "effective_design_outdoor_airflow",
            ValueKind::Real,
            time.clone(),
            effective_design.into_iter().map(r).collect(),
            "OutdoorAirFlow.Title24.AHU SingleDamper with CO2: design-min zone sum covers pass-through, cap, and zero cases",
            "Pinned Title24/AHU.mo: VEffDesOutAir_flow = min(VDesOutAir_flow, VSumZonDesMin_flow) with VDesOutAir_flow=8",
            inputs.clone(),
        ),
        sequence_golden(
            OUTDOOR_AIRFLOW_TITLE24_AHU,
            "effective_design_outdoor_airflow_normalized",
            ValueKind::Real,
            time.clone(),
            effective_design_normalized.into_iter().map(r).collect(),
            "OutdoorAirFlow.Title24.AHU SingleDamper with CO2: design minimum normalized by guarded design flow",
            "Pinned Title24/AHU.mo: effDesOutAir_normalized = VEffDesOutAir_flow / max(VDesOutAir_flow, 1E-4)",
            inputs.clone(),
        ),
        sequence_golden(
            OUTDOOR_AIRFLOW_TITLE24_AHU,
            "effective_outdoor_airflow_normalized",
            ValueKind::Real,
            time.clone(),
            effective_outdoor_air_normalized
                .into_iter()
                .map(r)
                .collect(),
            "OutdoorAirFlow.Title24.AHU CO2 line exercises below-x1 extrapolation, x1, interpolation, above-x2 clamp, and zero-flow cases",
            "Pinned Title24/AHU.mo: effOutAir = Line(uCO2Loo_max, x1=0.5, f1=VEffAbsOutAir_flow, x2=1, f2=VEffDesOutAir_flow, limitBelow=false, limitAbove=true); effOutAir_normalized = effOutAir / max(VDesOutAir_flow, 1E-4)",
            inputs.clone(),
        ),
        sequence_golden(
            OUTDOOR_AIRFLOW_TITLE24_AHU,
            "measured_outdoor_airflow_normalized",
            ValueKind::Real,
            time,
            measured_normalized.into_iter().map(r).collect(),
            "OutdoorAirFlow.Title24.AHU SingleDamper: measured outdoor-air normalization branch is active",
            "Pinned Title24/AHU.mo conditional branch: outAir_normalized = VAirOut_flow / max(VDesOutAir_flow, 1E-4) when minOADes is DedicatedDampersAirflow or SingleDamper",
            inputs,
        ),
    ]
}

struct OutdoorAirflowTitle24AhuOutputs {
    effective_absolute: f64,
    effective_absolute_normalized: f64,
    effective_design: f64,
    effective_design_normalized: f64,
    effective_outdoor_air_normalized: f64,
    measured_normalized: f64,
}

fn outdoor_airflow_title24_ahu_outputs(
    absolute_min_flow: f64,
    design_min_flow: f64,
    co2_loop_max: f64,
    measured_outdoor_air: f64,
) -> OutdoorAirflowTitle24AhuOutputs {
    const V_ABS_OUT_AIR_FLOW: f64 = 3.0;
    const V_DES_OUT_AIR_FLOW: f64 = 8.0;
    const NEAR_ZERO: f64 = 1E-4;

    let effective_absolute = V_ABS_OUT_AIR_FLOW.min(absolute_min_flow);
    let effective_design = V_DES_OUT_AIR_FLOW.min(design_min_flow);
    let guarded_absolute = V_ABS_OUT_AIR_FLOW.max(NEAR_ZERO);
    let guarded_design = V_DES_OUT_AIR_FLOW.max(NEAR_ZERO);
    let effective_outdoor_air = buildings_line(
        0.5,
        effective_absolute,
        1.0,
        effective_design,
        co2_loop_max,
        false,
        true,
    );

    OutdoorAirflowTitle24AhuOutputs {
        effective_absolute,
        effective_absolute_normalized: effective_absolute / guarded_absolute,
        effective_design,
        effective_design_normalized: effective_design / guarded_design,
        effective_outdoor_air_normalized: effective_outdoor_air / guarded_design,
        measured_normalized: measured_outdoor_air / guarded_design,
    }
}

fn buildings_line(
    x1: f64,
    f1: f64,
    x2: f64,
    f2: f64,
    u: f64,
    limit_below: bool,
    limit_above: bool,
) -> f64 {
    let x_lim = match (limit_below, limit_above) {
        (true, true) => u.max(x1).min(x2),
        (true, false) => u.max(x1),
        (false, true) => u.min(x2),
        (false, false) => u,
    };
    let slope = (f2 - f1) / (x2 - x1);
    let intercept = f2 - slope * x2;
    intercept + slope * x_lim
}

fn outdoor_airflow_title24_inputs(
    absolute_min_flow: &[f64],
    design_min_flow: &[f64],
    co2_loop_max: &[f64],
    measured_outdoor_air: &[f64],
) -> Vec<InputSeries> {
    vec![
        input_r("absolute_min_flow", absolute_min_flow.iter().copied()),
        input_r("design_min_flow", design_min_flow.iter().copied()),
        input_r("co2_loop_max", co2_loop_max.iter().copied()),
        input_r(
            "measured_outdoor_air",
            measured_outdoor_air.iter().copied(),
        ),
    ]
}
