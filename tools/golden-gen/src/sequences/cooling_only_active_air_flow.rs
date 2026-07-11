//! G36 CoolingOnly ActiveAirFlow source-verified sequence oracle.

use crate::oracle::{Golden, ValueKind};

use super::{COOLING_ONLY_ACTIVE_AIR_FLOW, input_i, input_r, r, sequence_golden};

const DESIGN_COOLING_MAXIMUM_FLOW: f64 = 0.94;
const OCCUPIED_MINIMUM_DESIGN_FLOW: f64 = 0.31;
const OPERATING_MODES: [i64; 14] = [1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7];
const OCCUPIED_MINIMUM_FLOW: [f64; 14] = [
    0.0,
    OCCUPIED_MINIMUM_DESIGN_FLOW,
    OCCUPIED_MINIMUM_DESIGN_FLOW,
    0.0,
    0.18,
    OCCUPIED_MINIMUM_DESIGN_FLOW,
    0.0,
    OCCUPIED_MINIMUM_DESIGN_FLOW,
    0.12,
    0.0,
    OCCUPIED_MINIMUM_DESIGN_FLOW,
    0.0,
    0.22,
    OCCUPIED_MINIMUM_DESIGN_FLOW,
];

/// Build the independent Tier-A goldens for all three active airflow setpoints.
///
/// The operating-mode input visits every upstream G36 OperationModes integer at least twice. Flow
/// inputs and outputs use `m3/s`; the schedule deliberately presents the same nonzero occupied
/// minimum flow in occupied and non-occupied modes so the occupied gate is falsifiable.
pub(super) fn goldens() -> Vec<Golden> {
    assert_falsifiable_schedule();
    let time: Vec<f64> = (0..OPERATING_MODES.len())
        .map(|tick| tick as f64)
        .collect();
    let active_cooling_maximum: Vec<f64> = OPERATING_MODES
        .iter()
        .map(|&mode| {
            if matches!(mode, 1..=3) {
                DESIGN_COOLING_MAXIMUM_FLOW
            } else {
                0.0
            }
        })
        .collect();
    let active_minimum: Vec<f64> = OPERATING_MODES
        .iter()
        .zip(OCCUPIED_MINIMUM_FLOW)
        .map(|(&mode, minimum)| if mode == 1 { minimum } else { 0.0 })
        .collect();
    let inputs = vec![
        input_i("operating_mode", OPERATING_MODES),
        input_r("occupied_minimum_airflow", OCCUPIED_MINIMUM_FLOW),
    ];

    vec![
        sequence_golden(
            COOLING_ONLY_ACTIVE_AIR_FLOW,
            "active_cooling_maximum_airflow",
            ValueKind::Real,
            time.clone(),
            active_cooling_maximum.into_iter().map(r).collect(),
            "ActiveAirFlow: all seven OperationModes appear twice; VCooMax_flow=0.94 m3/s",
            "ActiveAirFlow.mo: occMod/cooDowMod/setUpMod equality gates feed or3/or2, then actCooMax maps true to VCooMax_flow and false to zero",
            inputs.clone(),
        ),
        sequence_golden(
            COOLING_ONLY_ACTIVE_AIR_FLOW,
            "active_minimum_airflow",
            ValueKind::Real,
            time.clone(),
            active_minimum.iter().copied().map(r).collect(),
            "ActiveAirFlow: occupied and non-occupied samples share 0.31 m3/s VOccMin_flow controls",
            "ActiveAirFlow.mo: intEqu(occupied) converts to 1 or 0 and actMin multiplies that gate by VOccMin_flow",
            inputs.clone(),
        ),
        sequence_golden(
            COOLING_ONLY_ACTIVE_AIR_FLOW,
            "active_heating_maximum_airflow",
            ValueKind::Real,
            time,
            active_minimum.into_iter().map(r).collect(),
            "ActiveAirFlow: occupied and non-occupied samples share 0.31 m3/s VOccMin_flow controls",
            "ActiveAirFlow.mo: VActHeaMax_flow and VActMin_flow are both connected directly from actMin.y and are identical by source construction",
            inputs,
        ),
    ]
}

fn assert_falsifiable_schedule() {
    for mode in 1..=7 {
        assert!(
            OPERATING_MODES
                .iter()
                .filter(|&&candidate| candidate == mode)
                .count()
                >= 2,
            "OperationModes value {mode} must appear at least twice"
        );
    }
    assert!(OPERATING_MODES
        .iter()
        .zip(OCCUPIED_MINIMUM_FLOW)
        .any(|(&mode, minimum)| mode == 1 && minimum == OCCUPIED_MINIMUM_DESIGN_FLOW));
    assert!(OPERATING_MODES
        .iter()
        .zip(OCCUPIED_MINIMUM_FLOW)
        .any(|(&mode, minimum)| mode != 1 && minimum == OCCUPIED_MINIMUM_DESIGN_FLOW));
}
