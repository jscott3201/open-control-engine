//! G36 MultiZone VAV ReturnFanAirflowTracking sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{RETURN_FAN_AIRFLOW, clamp, input_b, input_r, r, sequence_golden, unit_ticks};

pub(super) fn goldens() -> Vec<Golden> {
    let time = unit_ticks(8);
    let supply_airflow = [5.0, 5.25, 5.0, 4.75, 5.5, 5.0, 4.5, 4.5];
    let return_airflow = [4.0; 8];
    let supply_fan_status = [false, true, true, true, false, true, true, true];
    let (return_fan_speed, return_fan_status) =
        return_fan_airflow_trace(&time, &supply_airflow, &return_airflow, &supply_fan_status);
    let inputs = return_fan_airflow_inputs(&supply_airflow, &return_airflow, &supply_fan_status);

    vec![
        sequence_golden(
            RETURN_FAN_AIRFLOW,
            "return_fan_speed",
            ValueKind::Real,
            time.clone(),
            return_fan_speed.into_iter().map(r).collect(),
            "ReturnFanAirflowTracking: PI airflow tracking with fan-off rows at t=0 and t=4, unsaturated rows, upper clamp, and lower clamp",
            "Pinned ReturnFanAirflowTracking.mo: conErr=VAirSup_flow-difFloSet, conP is default PI reverseActing=true with y=clamp(conErr-VAirRet_flow + xI, minSpe, maxSpe), and Reals.Switch emits zero when u1SupFan=false",
            inputs.clone(),
        ),
        sequence_golden(
            RETURN_FAN_AIRFLOW,
            "return_fan_status",
            ValueKind::Boolean,
            time,
            return_fan_status.into_iter().map(crate::oracle::Sample::Boolean).collect(),
            "ReturnFanAirflowTracking: y1RetFan is the source boundary alias of u1SupFan",
            "Pinned ReturnFanAirflowTracking.mo connects u1SupFan directly to y1RetFan; the checked-in explicit-CXF fixture uses a pure Boolean identity bridge because OCE exposes boundary outputs through block output connectors",
            inputs,
        ),
    ]
}

fn return_fan_airflow_trace(
    time: &[f64],
    supply_airflow: &[f64],
    return_airflow: &[f64],
    supply_fan_status: &[bool],
) -> (Vec<f64>, Vec<bool>) {
    const DIF_FLOW_SET: f64 = 1.0;
    const K: f64 = 1.0;
    const TI: f64 = 0.5;
    const NI: f64 = 0.9;
    const Y_MIN: f64 = 0.0;
    const Y_MAX: f64 = 1.0;

    let mut integral = 0.0;
    let mut previous_time: Option<f64> = None;
    let mut speed = Vec::with_capacity(time.len());
    let mut status = Vec::with_capacity(time.len());

    for (((&t, &supply), &ret), &fan_on) in time
        .iter()
        .zip(supply_airflow)
        .zip(return_airflow)
        .zip(supply_fan_status)
    {
        let error = (supply - DIF_FLOW_SET) - ret;
        let proportional = K * error;
        let unlimited = proportional + integral;
        let pid = clamp(unlimited, Y_MIN, Y_MAX);
        speed.push(if fan_on { pid } else { 0.0 });
        status.push(fan_on);

        let dt = previous_time.map_or(0.0, |previous| (t - previous).max(0.0));
        let anti_windup_gain = (unlimited - pid) / (K * NI);
        let corrected_error = error - anti_windup_gain;
        integral += (K / TI) * corrected_error * dt;
        previous_time = Some(t);
    }

    (speed, status)
}

fn return_fan_airflow_inputs(
    supply_airflow: &[f64],
    return_airflow: &[f64],
    supply_fan_status: &[bool],
) -> Vec<InputSeries> {
    vec![
        input_r("supply_airflow", supply_airflow.iter().copied()),
        input_r("return_airflow", return_airflow.iter().copied()),
        input_b("supply_fan_status", supply_fan_status.iter().copied()),
    ]
}
