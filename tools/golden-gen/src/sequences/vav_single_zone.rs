//! G36 single-zone VAV fixture oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{VAV, b, clamp, input_r, less_hysteretic, r, sequence_golden, unit_ticks};

pub(super) fn goldens() -> Vec<Golden> {
    let time = unit_ticks(6);
    let zone_temp = [22.0, 27.0, 27.5, 19.0, 19.3, 21.0];
    let cooling_setpoint = [24.0; 6];
    let heating_setpoint = [20.0; 6];

    let trace = vav_trace(&time, &zone_temp, &cooling_setpoint, &heating_setpoint);
    let inputs = vav_inputs(&zone_temp, &cooling_setpoint, &heating_setpoint);

    vec![
        sequence_golden(
            VAV,
            "damper_command",
            ValueKind::Real,
            time.clone(),
            trace.damper_command.iter().copied().map(r).collect(),
            "VAV: zone_temp=[22,27,27.5,19,19.3,21], cooling_setpoint=24, heating_setpoint=20; fixture-local PI cooling loop plus heating branch",
            "damperLimiter.y = max(0.2,min(airflowSwitch.y,1)); airflowSwitch selects heatingFlow=0.6 while heating_enabled, else max(coolingPid.y, minimumFlow=0.25)",
            inputs.clone(),
        ),
        sequence_golden(
            VAV,
            "airflow_setpoint",
            ValueKind::Real,
            time.clone(),
            trace.airflow_setpoint.iter().copied().map(r).collect(),
            "VAV: same fixture inputs; airflow_setpoint is the direct airflowSwitch.y output",
            "airflowSwitch selects heatingFlow=0.6 while heating_enabled, else coolingFlow=max(coolingPid.y, minimumFlow=0.25)",
            inputs.clone(),
        ),
        sequence_golden(
            VAV,
            "cooling_signal",
            ValueKind::Real,
            time.clone(),
            trace.cooling_signal.iter().copied().map(r).collect(),
            "VAV: coolingPid has k=0.25, Ti=20, Ni=0.9 default, yMin=0, yMax=1, xi_start=0, reverseActing=false",
            "cooling_signal aliases internal coolingPid.y. At each tick: e=zone_temp-cooling_setpoint; y=clamp(k*e+xI,0,1); then xI += (k/Ti)*(e-(yU-y)/(k*Ni))*dt with first dt=0.",
            inputs.clone(),
        ),
        sequence_golden(
            VAV,
            "heating_enabled",
            ValueKind::Boolean,
            time,
            trace.heating_enabled.into_iter().map(b).collect(),
            "VAV: same fixture inputs; heatingNeed is Less(h=0.5, pre_y_start=false)",
            "Buildings Reals/Less hysteresis: set when zone_temp<heating_setpoint; hold while previous true and zone_temp<heating_setpoint+h",
            inputs,
        ),
    ]
}

struct VavTrace {
    heating_enabled: Vec<bool>,
    cooling_signal: Vec<f64>,
    airflow_setpoint: Vec<f64>,
    damper_command: Vec<f64>,
}

fn vav_trace(
    time: &[f64],
    zone_temp: &[f64],
    cooling_setpoint: &[f64],
    heating_setpoint: &[f64],
) -> VavTrace {
    const K: f64 = 0.25;
    const TI: f64 = 20.0;
    const NI: f64 = 0.9;
    const MINIMUM_FLOW: f64 = 0.25;
    const HEATING_FLOW: f64 = 0.6;
    const DAMPER_MIN: f64 = 0.2;
    const DAMPER_MAX: f64 = 1.0;

    let heating_enabled = less_hysteretic(zone_temp, heating_setpoint, 0.5, false);
    let mut cooling_signal = Vec::with_capacity(time.len());
    let mut airflow_setpoint = Vec::with_capacity(time.len());
    let mut damper_command = Vec::with_capacity(time.len());
    let mut x_i = 0.0;
    let mut prev_t: Option<f64> = None;

    for (((&t, &zone), &cooling), &heating) in time
        .iter()
        .zip(zone_temp)
        .zip(cooling_setpoint)
        .zip(&heating_enabled)
    {
        let error = zone - cooling;
        let y_p = K * error;
        let y_u = y_p + x_i;
        let y = clamp(y_u, 0.0, 1.0);
        cooling_signal.push(y);

        let cooling_flow = y.max(MINIMUM_FLOW);
        let airflow = if heating { HEATING_FLOW } else { cooling_flow };
        airflow_setpoint.push(airflow);
        damper_command.push(clamp(airflow, DAMPER_MIN, DAMPER_MAX));

        let dt = prev_t.map_or(0.0, |previous| t - previous);
        let anti_windup = (y_u - y) / (K * NI);
        let corrected_error = error - anti_windup;
        x_i += (K / TI) * corrected_error * dt;
        prev_t = Some(t);
    }

    VavTrace {
        heating_enabled,
        cooling_signal,
        airflow_setpoint,
        damper_command,
    }
}

fn vav_inputs(
    zone_temp: &[f64],
    cooling_setpoint: &[f64],
    heating_setpoint: &[f64],
) -> Vec<InputSeries> {
    vec![
        input_r("zone_temp", zone_temp.iter().copied()),
        input_r("cooling_setpoint", cooling_setpoint.iter().copied()),
        input_r("heating_setpoint", heating_setpoint.iter().copied()),
    ]
}
