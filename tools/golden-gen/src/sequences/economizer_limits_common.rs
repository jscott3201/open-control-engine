//! G36 MultiZone VAV Economizers.Subsequences.Limits.Common sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{
    ECONOMIZER_LIMITS_COMMON, b, buildings_line, clamp, input_b, input_i, input_r, r,
    sequence_golden, unit_ticks,
};

pub(super) fn goldens() -> Vec<Golden> {
    let time = unit_ticks(8);
    let outdoor_airflow_normalized = [0.0; 8];
    let minimum_outdoor_airflow_setpoint_normalized =
        [1.0, 1.0, 1.0, 12.0, 24.0, 8.0, 8.0, 8.0];
    let operation_mode = [1, 1, 1, 1, 1, 0, 1, 1];
    let supply_fan_status = [false, true, true, true, true, true, false, true];

    let trace = economizer_limits_common_trace(
        &time,
        &outdoor_airflow_normalized,
        &minimum_outdoor_airflow_setpoint_normalized,
        &operation_mode,
        &supply_fan_status,
    );
    let inputs = economizer_limits_common_inputs(
        &outdoor_airflow_normalized,
        &minimum_outdoor_airflow_setpoint_normalized,
        &operation_mode,
        &supply_fan_status,
    );

    vec![
        sequence_golden(
            ECONOMIZER_LIMITS_COMMON,
            "outdoor_damper_min_limit",
            ValueKind::Real,
            time.clone(),
            trace.outdoor_damper_min_limit.iter().copied().map(r).collect(),
            "Economizer Limits.Common: fan-off, reset, enabled modulation, saturation, unoccupied, fan-off disable, and second reset rows",
            "Pinned Limits/Common.mo source-default PI branch: yOutDam_min = Line(x1=0,f1=outDamPhy_min,x2=uRetDam_min,f2=yOutDam_max,u=damLimCon.y,limitBelow=true,limitAbove=true)",
            inputs.clone(),
        ),
        sequence_golden(
            ECONOMIZER_LIMITS_COMMON,
            "outdoor_damper_max_limit",
            ValueKind::Real,
            time.clone(),
            trace.outdoor_damper_max_limit.iter().copied().map(r).collect(),
            "Economizer Limits.Common: outdoor maximum limit is disabled when fan/mode gating disables minimum outdoor air control",
            "Pinned Limits/Common.mo: outDamPosMaxSwitch selects outDamPhy_min when !yEnaMinOut else outDamPhy_max",
            inputs.clone(),
        ),
        sequence_golden(
            ECONOMIZER_LIMITS_COMMON,
            "return_damper_min_limit",
            ValueKind::Real,
            time.clone(),
            trace.return_damper_min_limit.iter().copied().map(r).collect(),
            "Economizer Limits.Common: return minimum limit is forced to the physical maximum when the minimum outdoor air loop is disabled",
            "Pinned Limits/Common.mo: retDamPosMinSwitch selects retDamPhy_max when !yEnaMinOut else retDamPhy_min",
            inputs.clone(),
        ),
        sequence_golden(
            ECONOMIZER_LIMITS_COMMON,
            "return_damper_max_limit",
            ValueKind::Real,
            time.clone(),
            trace.return_damper_max_limit.iter().copied().map(r).collect(),
            "Economizer Limits.Common: return maximum limit stays high below uRetDam_min and then decreases as the PI loop signal rises",
            "Pinned Limits/Common.mo source-default PI branch: yRetDam_max = Line(x1=uRetDam_min,f1=retDamPhy_max,x2=1,f2=yRetDam_min,u=damLimCon.y,limitBelow=true,limitAbove=true)",
            inputs.clone(),
        ),
        sequence_golden(
            ECONOMIZER_LIMITS_COMMON,
            "return_damper_physical_max_limit",
            ValueKind::Real,
            time.clone(),
            trace
                .return_damper_physical_max_limit
                .iter()
                .copied()
                .map(r)
                .collect(),
            "Economizer Limits.Common: physical return damper maximum is a source constant exported for economizer enable/disable logic",
            "Pinned Limits/Common.mo connects retDamPhyPosMaxSig.y (retDamPhy_max=1) directly to yRetDamPhy_max",
            inputs.clone(),
        ),
        sequence_golden(
            ECONOMIZER_LIMITS_COMMON,
            "minimum_outdoor_air_loop_enabled",
            ValueKind::Boolean,
            time,
            trace
                .minimum_outdoor_air_loop_enabled
                .iter()
                .copied()
                .map(b)
                .collect(),
            "Economizer Limits.Common: operation_mode=occupied is required together with supply fan proof",
            "Pinned Limits/Common.mo yEnaMinOut = u1SupFan AND (uOpeMod == OperationModes.occupied)",
            inputs,
        ),
    ]
}

#[derive(Default)]
struct EconomizerLimitsCommonTrace {
    outdoor_damper_min_limit: Vec<f64>,
    outdoor_damper_max_limit: Vec<f64>,
    return_damper_min_limit: Vec<f64>,
    return_damper_max_limit: Vec<f64>,
    return_damper_physical_max_limit: Vec<f64>,
    minimum_outdoor_air_loop_enabled: Vec<bool>,
}

fn economizer_limits_common_trace(
    time: &[f64],
    outdoor_airflow_normalized: &[f64],
    minimum_outdoor_airflow_setpoint_normalized: &[f64],
    operation_mode: &[i64],
    supply_fan_status: &[bool],
) -> EconomizerLimitsCommonTrace {
    const K: f64 = 0.05;
    const TI: f64 = 120.0;
    const NI: f64 = 0.9;
    const Y_MIN: f64 = 0.0;
    const Y_MAX: f64 = 1.0;
    const RESET: f64 = 0.0;
    const U_RET_DAM_MIN: f64 = 0.5;
    const RET_DAM_PHY_MAX: f64 = 1.0;
    const RET_DAM_PHY_MIN: f64 = 0.0;
    const OUT_DAM_PHY_MAX: f64 = 1.0;
    const OUT_DAM_PHY_MIN: f64 = 0.0;
    const OCCUPIED: i64 = 1;

    let mut integral = 0.0;
    let mut previous_time: Option<f64> = None;
    let mut previous_trigger = false;
    let mut trace = EconomizerLimitsCommonTrace::default();

    for ((((&t, &measured), &setpoint), &mode), &fan_on) in time
        .iter()
        .zip(outdoor_airflow_normalized)
        .zip(minimum_outdoor_airflow_setpoint_normalized)
        .zip(operation_mode)
        .zip(supply_fan_status)
    {
        let error = setpoint - measured;
        let proportional = K * error;
        let unlimited = proportional + integral;
        let loop_signal = clamp(unlimited, Y_MIN, Y_MAX);

        let enabled = fan_on && mode == OCCUPIED;
        let disabled = !enabled;
        let outdoor_damper_max = if disabled {
            OUT_DAM_PHY_MIN
        } else {
            OUT_DAM_PHY_MAX
        };
        let return_damper_min = if disabled {
            RET_DAM_PHY_MAX
        } else {
            RET_DAM_PHY_MIN
        };

        trace.outdoor_damper_min_limit.push(buildings_line(
            Y_MIN,
            OUT_DAM_PHY_MIN,
            U_RET_DAM_MIN,
            outdoor_damper_max,
            loop_signal,
        ));
        trace.outdoor_damper_max_limit.push(outdoor_damper_max);
        trace.return_damper_min_limit.push(return_damper_min);
        trace.return_damper_max_limit.push(buildings_line(
            U_RET_DAM_MIN,
            RET_DAM_PHY_MAX,
            Y_MAX,
            return_damper_min,
            loop_signal,
        ));
        trace.return_damper_physical_max_limit.push(RET_DAM_PHY_MAX);
        trace.minimum_outdoor_air_loop_enabled.push(enabled);

        let dt = previous_time.map_or(0.0, |previous| (t - previous).max(0.0));
        if fan_on && !previous_trigger {
            integral = RESET - proportional;
        } else {
            let anti_windup_gain = (unlimited - loop_signal) / (K * NI);
            let corrected_error = error - anti_windup_gain;
            integral += (K / TI) * corrected_error * dt;
        }
        previous_time = Some(t);
        previous_trigger = fan_on;
    }

    trace
}

fn economizer_limits_common_inputs(
    outdoor_airflow_normalized: &[f64],
    minimum_outdoor_airflow_setpoint_normalized: &[f64],
    operation_mode: &[i64],
    supply_fan_status: &[bool],
) -> Vec<InputSeries> {
    vec![
        input_r(
            "outdoor_airflow_normalized",
            outdoor_airflow_normalized.iter().copied(),
        ),
        input_r(
            "minimum_outdoor_airflow_setpoint_normalized",
            minimum_outdoor_airflow_setpoint_normalized.iter().copied(),
        ),
        input_i("operation_mode", operation_mode.iter().copied()),
        input_b("supply_fan_status", supply_fan_status.iter().copied()),
    ]
}
