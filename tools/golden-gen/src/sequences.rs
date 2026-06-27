//! G36 whole-sequence Tier-A references for the derivable sequence outputs.
//!
//! These references are hand-derived from the G36 fixture topology plus CDL / Buildings block
//! semantics. They intentionally do not depend on, import, or inspect any `oce-*` crate.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

const SAT: &str = "ahu_supply_air_temp_reset";
const ECON: &str = "ahu_economizer";
const VAV: &str = "vav_single_zone";
const SOURCE_COMMIT: &str = "a131864e4c4df22ebcd52bb8da439de0087ac365";

/// A generated provenance-only marker for deferred correctness-oracle coverage.
pub struct DeferredProvenance {
    /// Path under `tools/golden-gen/goldens`.
    pub relative_path: &'static str,
    /// JSON payload.
    pub contents: String,
    /// Manifest line describing the payload.
    pub manifest_line: &'static str,
}

/// Build all sequence-level G36 Tier-A goldens.
pub fn goldens() -> Vec<Golden> {
    let mut out = Vec::new();
    out.extend(sat_reset());
    out.extend(economizer());
    out.extend(vav_single_zone());
    out
}

/// Provenance-only records for intentionally deferred sequence correctness coverage.
pub fn deferred_provenance(generator_version: &str) -> Vec<DeferredProvenance> {
    vec![DeferredProvenance {
        relative_path: "G36/vav_single_zone/pid_cone_deferred.prov.json",
        contents: format!(
            concat!(
                "{{\n",
                "  \"sequence\": \"vav_single_zone\",\n",
                "  \"tier\": \"B-deferred\",\n",
                "  \"source\": \"no in-repo correctness oracle yet; deferred to the ",
                "OpenModelica-Docker Tier-B spike and tracked PID derivative-term ",
                "oracle follow-up\",\n",
                "  \"depends_on_oce_blocks\": false,\n",
                "  \"deferred_signals\": [\n",
                "    \"cooling_signal\",\n",
                "    \"cooling-branch airflow_setpoint\",\n",
                "    \"cooling-branch damper_command\",\n",
                "    \"internal coolingPid.y\"\n",
                "  ],\n",
                "  \"covered_here\": [\n",
                "    \"heating_enabled\",\n",
                "    \"heating-branch airflow_setpoint masked by heating_enabled\",\n",
                "    \"heating-branch damper_command masked by heating_enabled\"\n",
                "  ],\n",
                "  \"fp_residue_note\": \"E2 Tier-A G36 references exercise no add-rounding; ",
                "the dyadic/add-rounding residue concern remains B4 follow-up scope.\",\n",
                "  \"generator\": \"{}\"\n",
                "}}\n"
            ),
            generator_version
        ),
        manifest_line: "G36/vav_single_zone PID cone deferred -> goldens/G36/vav_single_zone/pid_cone_deferred.prov.json",
    }]
}

fn sat_reset() -> Vec<Golden> {
    let time = unit_ticks(5);
    let zone_temp = [22.0, 24.0, 24.5, 25.5, 25.5];
    let cooling_setpoint = [24.0; 5];

    let cooling_demand: Vec<f64> = zone_temp
        .iter()
        .zip(cooling_setpoint)
        .map(|(&zone, setpoint)| clamp(zone - setpoint, 0.0, 1.0))
        .collect();
    let sat_setpoint: Vec<f64> = cooling_demand
        .iter()
        .map(|&demand| buildings_line(0.0, 16.0, 1.0, 12.0, demand))
        .collect();
    let inputs = sat_inputs(&zone_temp, &cooling_setpoint);

    vec![
        sequence_golden(
            SAT,
            "sat_setpoint",
            ValueKind::Real,
            time.clone(),
            sat_setpoint.into_iter().map(r).collect(),
            "SAT reset: zone_temp=[22,24,24.5,25.5,25.5], cooling_setpoint=24; dyadic trace",
            "zoneMinusSet=zone_temp-cooling_setpoint; demandLimiter=max(0,min(diff,1)); \
             satLine uses Buildings Reals/Line.mo xLim, b=(f2-f1)/(x2-x1), a=f2-b*x2, y=a+b*xLim",
            inputs.clone(),
        ),
        sequence_golden(
            SAT,
            "cooling_demand",
            ValueKind::Real,
            time,
            cooling_demand.into_iter().map(r).collect(),
            "SAT reset: same inputs; output is demandLimiter.y",
            "cooling_demand=max(0,min(zone_temp-cooling_setpoint,1)); dyadic operands, no add-rounding coverage",
            inputs,
        ),
    ]
}

fn economizer() -> Vec<Golden> {
    let time = unit_ticks(6);
    let return_air_temp = [24.0; 6];
    let outdoor_air_temp = [23.0, 19.0, 19.0, 19.0, 24.0, 19.0];
    let operating_mode = [1, 1, 1, 1, 1, 0];

    let oa_temperature_delta: Vec<f64> = return_air_temp
        .iter()
        .zip(outdoor_air_temp)
        .map(|(&ret, outdoor)| ret - outdoor)
        .collect();
    let favorable = hysteresis(&oa_temperature_delta, 1.0, 3.0, false);
    let mode_allowed: Vec<bool> = operating_mode.iter().map(|&mode| mode > 0).collect();
    let candidate: Vec<bool> = favorable
        .iter()
        .zip(&mode_allowed)
        .map(|(&fav, &allowed)| fav && allowed)
        .collect();
    let delayed = true_delay(&time, &candidate, 1.0);
    let not_candidate: Vec<bool> = candidate.iter().map(|&value| !value).collect();
    let economizer_enabled = latch(&delayed, &not_candidate);
    let damper_command: Vec<f64> = economizer_enabled
        .iter()
        .map(|&enabled| if enabled { 1.0 } else { 0.2 })
        .collect();
    let operating_mode_real: Vec<f64> = operating_mode.iter().map(|&mode| mode as f64).collect();
    let inputs = economizer_inputs(&return_air_temp, &outdoor_air_temp, &operating_mode);

    vec![
        sequence_golden(
            ECON,
            "oa_temperature_delta",
            ValueKind::Real,
            time.clone(),
            oa_temperature_delta.into_iter().map(r).collect(),
            "Economizer: return_air_temp=24; outdoor_air_temp=[23,19,19,19,24,19]",
            "returnMinusOutdoor.y = return_air_temp - outdoor_air_temp; dyadic operands, no add-rounding coverage",
            inputs.clone(),
        ),
        sequence_golden(
            ECON,
            "operating_mode_real",
            ValueKind::Real,
            time.clone(),
            operating_mode_real.into_iter().map(r).collect(),
            "Economizer: operating_mode=[1,1,1,1,1,0]",
            "modeIdentity adds integer parameter p=0; modeToReal converts the integer value to Real",
            inputs.clone(),
        ),
        sequence_golden(
            ECON,
            "economizer_enabled",
            ValueKind::Boolean,
            time.clone(),
            economizer_enabled.iter().copied().map(b).collect(),
            "Economizer FSM: Hysteresis(uLow=1,uHigh=3) -> And(mode>0) -> TrueDelay(1s) -> Latch(clear=!candidate)",
            "Buildings semantics: Hysteresis holds inside [uLow,uHigh]; TrueDelay emits true after continuous true duration >= delayTime; Latch is clear-dominant and set by rising delayed input",
            inputs.clone(),
        ),
        sequence_golden(
            ECON,
            "damper_command",
            ValueKind::Real,
            time,
            damper_command.into_iter().map(r).collect(),
            "Economizer damperSwitch: u1=1.0, u2=economizer_enabled, u3=0.2",
            "Reals.Switch selects u1 when enabled else u3; selected constants are compared exactly",
            inputs,
        ),
    ]
}

fn vav_single_zone() -> Vec<Golden> {
    let time = unit_ticks(6);
    let zone_temp = [22.0, 27.0, 27.5, 19.0, 19.3, 21.0];
    let cooling_setpoint = [24.0; 6];
    let heating_setpoint = [20.0; 6];

    let heating_enabled = less_hysteretic(&zone_temp, &heating_setpoint, 0.5, false);
    let heating_branch = vec![0.6; time.len()];
    let inputs = vav_inputs(&zone_temp, &cooling_setpoint, &heating_setpoint);

    vec![
        sequence_golden(
            VAV,
            "heating_enabled",
            ValueKind::Boolean,
            time.clone(),
            heating_enabled.into_iter().map(b).collect(),
            "VAV: zone_temp=[22,27,27.5,19,19.3,21], heating_setpoint=20, h=0.5",
            "Buildings Reals/Less hysteresis: set when zone_temp<heating_setpoint; hold while previous true and zone_temp<heating_setpoint+h",
            inputs.clone(),
        ),
        sequence_golden(
            VAV,
            "airflow_setpoint",
            ValueKind::Real,
            time.clone(),
            heating_branch.iter().copied().map(r).collect(),
            "VAV airflowSwitch heating branch only: heatingFlow.k=0.6; cooling branch is PID-contaminated and masked out",
            "When heating_enabled is true, Reals.Switch selects heatingFlow.y=0.6 exactly; rows outside the mask are deferred Tier-B territory",
            inputs.clone(),
        ),
        sequence_golden(
            VAV,
            "damper_command",
            ValueKind::Real,
            time,
            heating_branch.into_iter().map(r).collect(),
            "VAV damperLimiter heating branch only: airflowSwitch.y=0.6, uMin=0.2, uMax=1.0",
            "When heating_enabled is true, Limiter clamps 0.6 to 0.6 exactly; cooling-branch rows are masked and deferred",
            inputs,
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn sequence_golden(
    sequence: &'static str,
    signal: &'static str,
    kind: ValueKind,
    time: Vec<f64>,
    samples: Vec<Sample>,
    input_desc: &'static str,
    rule_desc: &'static str,
    inputs: Vec<InputSeries>,
) -> Golden {
    Golden::new("G36", signal, kind, time, samples, input_desc, rule_desc)
        .with_scenario(sequence)
        .with_inputs(inputs)
        .with_provenance("source_commit", SOURCE_COMMIT)
        .with_provenance("source_files", source_files(sequence))
        .with_provenance("fixture_status", "supported-fixture-only source-reviewed fragment")
}

fn source_files(sequence: &str) -> &'static str {
    match sequence {
        SAT => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyTemperature.mo",
        ECON => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Controller.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Enable.mo; Buildings/Controls/OBC/ASHRAE/G36/Generic/AirEconomizerHighLimits.mo",
        VAV => "Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/Controller.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/SetPoints/Supply.mo; Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/SetPoints/SupplyFan.mo",
        _ => unreachable!("unknown G36 sequence {sequence}"),
    }
}

fn unit_ticks(n: usize) -> Vec<f64> {
    (0..n).map(|tick| tick as f64).collect()
}

fn buildings_line(x1: f64, f1: f64, x2: f64, f2: f64, u: f64) -> f64 {
    let x_lim = clamp(u, x1, x2);
    let slope = (f2 - f1) / (x2 - x1);
    let intercept = f2 - slope * x2;
    intercept + slope * x_lim
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    min.max(value.min(max))
}

fn hysteresis(u: &[f64], u_low: f64, u_high: f64, pre_y_start: bool) -> Vec<bool> {
    let mut previous = pre_y_start;
    let mut out = Vec::with_capacity(u.len());
    for &value in u {
        let next = if value > u_high {
            true
        } else if value < u_low {
            false
        } else {
            previous
        };
        out.push(next);
        previous = next;
    }
    out
}

fn true_delay(time: &[f64], u: &[bool], delay_time: f64) -> Vec<bool> {
    let mut entry_time = None;
    let mut previous_u = false;
    let mut out = Vec::with_capacity(time.len());
    for (&t, &input) in time.iter().zip(u) {
        if input && !previous_u {
            entry_time = Some(t);
        }
        out.push(input && entry_time.is_some_and(|entry| t - entry >= delay_time));
        if !input {
            entry_time = None;
        }
        previous_u = input;
    }
    out
}

fn latch(u: &[bool], clear: &[bool]) -> Vec<bool> {
    let mut previous_u = false;
    let mut previous_y = false;
    let mut out = Vec::with_capacity(u.len());
    for (&input, &clr) in u.iter().zip(clear) {
        let rising = input && !previous_u;
        let next = if clr {
            false
        } else if rising {
            true
        } else {
            previous_y
        };
        out.push(next);
        previous_y = next;
        previous_u = input;
    }
    out
}

fn less_hysteretic(u1: &[f64], u2: &[f64], h: f64, pre_y_start: bool) -> Vec<bool> {
    let mut previous = pre_y_start;
    let mut out = Vec::with_capacity(u1.len());
    for (&left, &right) in u1.iter().zip(u2) {
        let next = (!previous && left < right) || (previous && left < right + h);
        out.push(next);
        previous = next;
    }
    out
}

fn sat_inputs(zone_temp: &[f64], cooling_setpoint: &[f64]) -> Vec<InputSeries> {
    vec![
        input_r("zone_temp", zone_temp.iter().copied()),
        input_r("cooling_setpoint", cooling_setpoint.iter().copied()),
    ]
}

fn economizer_inputs(
    return_air_temp: &[f64],
    outdoor_air_temp: &[f64],
    operating_mode: &[i64],
) -> Vec<InputSeries> {
    vec![
        input_r("return_air_temp", return_air_temp.iter().copied()),
        input_r("outdoor_air_temp", outdoor_air_temp.iter().copied()),
        input_i("operating_mode", operating_mode.iter().copied()),
    ]
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

fn input_r(name: &'static str, values: impl IntoIterator<Item = f64>) -> InputSeries {
    InputSeries::new(name, ValueKind::Real, values.into_iter().map(r).collect())
}

fn input_i(name: &'static str, values: impl IntoIterator<Item = i64>) -> InputSeries {
    InputSeries::new(
        name,
        ValueKind::Integer,
        values.into_iter().map(i).collect(),
    )
}

fn r(x: f64) -> Sample {
    Sample::Real(x)
}

fn i(x: i64) -> Sample {
    Sample::Integer(x)
}

fn b(x: bool) -> Sample {
    Sample::Boolean(x)
}
