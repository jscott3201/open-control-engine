//! G36 MultiZone VAV OutdoorAirFlow ASHRAE 62.1 AHU sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{OUTDOOR_AIRFLOW_AHU, input_r, r, sequence_golden};

pub(super) fn goldens() -> Vec<Golden> {
    let time: Vec<f64> = (0..5).map(f64::from).collect();
    let population_flow = [1.0, 4.0, 0.002, 0.0, 5.0];
    let area_flow = [1.0, 5.0, 0.001, 0.0, 5.0];
    let primary_flow = [5.0, 2.0, 0.0, 1.0, 100.0];
    let max_outdoor_air_fraction = [0.2, 0.2, 0.4, 1.5, 0.99];
    let measured_outdoor_air = [4.0, 6.0, 0.1, 9.0, 8.8];

    let mut uncorrected = Vec::with_capacity(time.len());
    let mut effective = Vec::with_capacity(time.len());
    let mut effective_normalized = Vec::with_capacity(time.len());
    let mut measured_normalized = Vec::with_capacity(time.len());
    for row in 0..time.len() {
        let outputs = outdoor_airflow_ahu_outputs(
            population_flow[row],
            area_flow[row],
            primary_flow[row],
            max_outdoor_air_fraction[row],
            measured_outdoor_air[row],
        );
        uncorrected.push(outputs.uncorrected);
        effective.push(outputs.effective);
        effective_normalized.push(outputs.effective_normalized);
        measured_normalized.push(outputs.measured_normalized);
    }

    let inputs = outdoor_airflow_inputs(
        &population_flow,
        &area_flow,
        &primary_flow,
        &max_outdoor_air_fraction,
        &measured_outdoor_air,
    );

    vec![
        sequence_golden(
            OUTDOOR_AIRFLOW_AHU,
            "uncorrected_outdoor_airflow",
            ValueKind::Real,
            time.clone(),
            uncorrected.into_iter().map(r).collect(),
            "OutdoorAirFlow.ASHRAE62_1.AHU SingleDamper: population/area flows exercise nominal, design cap, near-zero, zero, and effective-design-cap cases",
            "Pinned AHU.mo: VUncOutAir_flow = min(VUncDesOutAir_flow, VSumAdjPopBreZon_flow + VSumAdjAreBreZon_flow) with VUncDesOutAir_flow=6",
            inputs.clone(),
        ),
        sequence_golden(
            OUTDOOR_AIRFLOW_AHU,
            "effective_minimum_outdoor_airflow",
            ValueKind::Real,
            time.clone(),
            effective.into_iter().map(r).collect(),
            "OutdoorAirFlow.ASHRAE62_1.AHU SingleDamper: effective setpoint covers primary-flow guard, system-efficiency near-zero guard, and design total cap",
            "Pinned AHU.mo: guardedPrimary=max(VSumZonPri_flow, 6*1E-3); sysVenEff=max(1 + VUncOutAir_flow/guardedPrimary - uOutAirFra_max, 1E-4); VEffAirOut_flow_min=min(VUncOutAir_flow/sysVenEff, 8)",
            inputs.clone(),
        ),
        sequence_golden(
            OUTDOOR_AIRFLOW_AHU,
            "effective_outdoor_airflow_normalized",
            ValueKind::Real,
            time.clone(),
            effective_normalized.into_iter().map(r).collect(),
            "OutdoorAirFlow.ASHRAE62_1.AHU SingleDamper: effective outdoor airflow normalized by design total outdoor airflow",
            "Pinned AHU.mo: effOutAir_normalized = VEffAirOut_flow_min / VDesTotOutAir_flow with VDesTotOutAir_flow=8",
            inputs.clone(),
        ),
        sequence_golden(
            OUTDOOR_AIRFLOW_AHU,
            "measured_outdoor_airflow_normalized",
            ValueKind::Real,
            time,
            measured_normalized.into_iter().map(r).collect(),
            "OutdoorAirFlow.ASHRAE62_1.AHU SingleDamper: measured outdoor-air normalization branch active because minOADes=SingleDamper",
            "Pinned AHU.mo conditional branch: outAir_normalized = VAirOut_flow / VDesTotOutAir_flow when minOADes is DedicatedDampersAirflow or SingleDamper",
            inputs,
        ),
    ]
}

struct OutdoorAirflowAhuOutputs {
    uncorrected: f64,
    effective: f64,
    effective_normalized: f64,
    measured_normalized: f64,
}

fn outdoor_airflow_ahu_outputs(
    population_flow: f64,
    area_flow: f64,
    primary_flow: f64,
    max_outdoor_air_fraction: f64,
    measured_outdoor_air: f64,
) -> OutdoorAirflowAhuOutputs {
    const V_UNC_DES_OUT_AIR_FLOW: f64 = 6.0;
    const V_DES_TOT_OUT_AIR_FLOW: f64 = 8.0;
    const NEAR_ZERO: f64 = 1E-4;

    let uncorrected = V_UNC_DES_OUT_AIR_FLOW.min(population_flow + area_flow);
    let guarded_primary = primary_flow.max(V_UNC_DES_OUT_AIR_FLOW * 1E-3);
    let system_efficiency =
        (1.0 + uncorrected / guarded_primary - max_outdoor_air_fraction).max(NEAR_ZERO);
    let effective = V_DES_TOT_OUT_AIR_FLOW.min(uncorrected / system_efficiency);
    OutdoorAirflowAhuOutputs {
        uncorrected,
        effective,
        effective_normalized: effective / V_DES_TOT_OUT_AIR_FLOW,
        measured_normalized: measured_outdoor_air / V_DES_TOT_OUT_AIR_FLOW,
    }
}

fn outdoor_airflow_inputs(
    population_flow: &[f64],
    area_flow: &[f64],
    primary_flow: &[f64],
    max_outdoor_air_fraction: &[f64],
    measured_outdoor_air: &[f64],
) -> Vec<InputSeries> {
    vec![
        input_r("population_flow", population_flow.iter().copied()),
        input_r("area_flow", area_flow.iter().copied()),
        input_r("primary_flow", primary_flow.iter().copied()),
        input_r(
            "max_outdoor_air_fraction",
            max_outdoor_air_fraction.iter().copied(),
        ),
        input_r("measured_outdoor_air", measured_outdoor_air.iter().copied()),
    ]
}
