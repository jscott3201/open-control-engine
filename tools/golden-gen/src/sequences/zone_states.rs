//! G36 ThermalZones.ZoneStates source-verified sequence oracle.
//!
//! This per-tick recurrence is independently derived from pinned `ZoneStates.mo`. Three CDL
//! hysteresis blocks classify heating demand, cooling demand, and the asymmetric
//! `uHea - uCoo` tie-break. The latter has `uLow=-0.01` and `uHigh=0.01`; both source expressions
//! are pre-grounded in the fixture. `ZoneStates.mo` lines 58-62 bind `uHigh=uLow` in the enclosing
//! scope, but the resolver's latest-wins parameter scope would otherwise shadow that top-level
//! `uLow=+0.01` with the earlier sibling `hysU.uLow=-0.01`. Resolver-design follow-up
//! `019f5431-047a` tracks the general representation gap. The combinational priority ladder then
//! selects exactly one Integer-valued zone state on every tick.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{THERMAL_ZONES_ZONE_STATES, hysteresis, i, input_r, sequence_golden};

const DEMAND_LOW: f64 = 0.01;
const DEMAND_HIGH: f64 = 0.05;
const HEATING_STATE: i64 = 1;
const DEADBAND_STATE: i64 = 2;
const COOLING_STATE: i64 = 3;

#[derive(Clone, Copy)]
struct Row {
    heating_control: f64,
    cooling_control: f64,
}

const fn row(heating_control: f64, cooling_control: f64) -> Row {
    Row {
        heating_control,
        cooling_control,
    }
}

const ROWS: [Row; 44] = [
    // Quiescent t=0: every hysteresis starts false, so the NOR selects deadband.
    row(0.0, 0.0), // 0
    // ZS4, heating signal: equality at uHigh does not arm, the band holds after arming,
    // equality at uLow remains on, and a value below uLow releases to deadband.
    row(0.05, 0.0),  // 60
    row(0.051, 0.0), // 120
    row(0.03, 0.0),  // 180
    row(0.01, 0.0),  // 240
    row(0.009, 0.0), // 300
    // ZS4, cooling signal: mirror the strict-arm, band-hold, equality, and release probes.
    row(0.0, 0.05),  // 360
    row(0.0, 0.051), // 420
    row(0.0, 0.03),  // 480
    row(0.0, 0.01),  // 540
    row(0.0, 0.009), // 600
    // ZS1 + ZS2 + ZS3: simultaneous demands first latch heating. Cooling stays armed while
    // heating wins, including an interior -0.009 hold value. Crossing below -0.01 makes a
    // direct 1->3 transition. The positive hold band then preserves a cooling-latched history
    // at +0.009 until a strict +0.01 crossing restores heating. The same neutral pair is
    // exercised under both latch histories.
    row(0.08, 0.06),  // 660
    row(0.07, 0.065), // 720
    row(0.061, 0.07), // 780
    row(0.06, 0.071), // 840
    row(0.07, 0.065), // 900
    row(0.079, 0.07), // 960
    row(0.081, 0.07), // 1020
    row(0.075, 0.075), // 1080
    row(0.07, 0.081),  // 1140
    row(0.075, 0.075), // 1200
    row(0.081, 0.07),  // 1260
    // ZS3 + ZS4: releasing the heating signal exposes the still-latched cooling signal;
    // cooling holds at uLow, then releases. The mirror sequence separately proves a heating
    // band hold and release while the tie-break remains latched.
    row(0.0, 0.03),  // 1320
    row(0.0, 0.01),  // 1380
    row(0.0, 0.009), // 1440
    row(0.03, 0.0),  // 1500
    row(0.06, 0.0),  // 1560
    row(0.03, 0.0),  // 1620
    row(0.009, 0.0), // 1680
    // ZS2 + ZS3: both demand latches arm; heating priority survives while both signals sit in
    // their hold bands. Releasing only heating exposes cooling, then releasing cooling yields
    // the combinational deadband state rather than a third latch.
    row(0.07, 0.06),  // 1740
    row(0.04, 0.03),  // 1800
    row(0.009, 0.03), // 1860
    row(0.009, 0.009), // 1920
    // ZS5: explicit plateaus and transitions cover every enum ordinal and every required pair:
    // 2->1, 1->2, 2->3, 3->2, and the reachable simultaneous-demand 1->3. The final pair also
    // demonstrates the reverse direct 3->1 transition before returning to deadband.
    row(0.0, 0.0),   // 1980
    row(0.06, 0.0),  // 2040
    row(0.06, 0.0),  // 2100
    row(0.0, 0.0),   // 2160
    row(0.0, 0.06),  // 2220
    row(0.0, 0.06),  // 2280
    row(0.0, 0.0),   // 2340
    row(0.06, 0.08), // 2400
    row(0.08, 0.06), // 2460
    row(0.08, 0.06), // 2520
    row(0.0, 0.0),   // 2580
];

/// Build the independent Tier-A Integer golden for ThermalZones.ZoneStates.
///
/// Time is in seconds and both control signals are dimensionless. The 60-second, 44-row schedule
/// covers ZS1-ZS5: asymmetric tie-break crossings and holds in both directions, simultaneous
/// demands under both latch histories, heating priority, both signal-hysteresis boundaries and
/// releases, all three enum ordinals, and every required output transition pair.
pub(super) fn goldens() -> Vec<Golden> {
    let time: Vec<f64> = (0..ROWS.len()).map(|tick| tick as f64 * 60.0).collect();
    let zone_state = zone_state_trace();

    vec![sequence_golden(
        THERMAL_ZONES_ZONE_STATES,
        "zone_state",
        ValueKind::Integer,
        time,
        zone_state.into_iter().map(i).collect(),
        "ZoneStates: 60-second ZS1-ZS5 schedule covers strict boundaries, signal and asymmetric tie-break holds, simultaneous demands, priority, enum plateaus, and direct transitions",
        "ZoneStates.mo lines 21-87 and 89-123: three Hysteresis blocks classify demand and uHea-uCoo, And/Not/Nor enforce heating priority, and BooleanToInteger plus two Integers.Add blocks emit ZoneStates heating=1, deadband=2, cooling=3",
        zone_state_inputs(),
    )]
}

fn zone_state_trace() -> Vec<i64> {
    let heating_control: Vec<f64> = ROWS.iter().map(|row| row.heating_control).collect();
    let cooling_control: Vec<f64> = ROWS.iter().map(|row| row.cooling_control).collect();
    let heating_signal = hysteresis(&heating_control, DEMAND_LOW, DEMAND_HIGH, false);
    let cooling_signal = hysteresis(&cooling_control, DEMAND_LOW, DEMAND_HIGH, false);
    let control_difference: Vec<f64> = heating_control
        .iter()
        .zip(&cooling_control)
        .map(|(&heating, &cooling)| heating - cooling)
        .collect();
    let heating_wins_tiebreak = hysteresis(
        &control_difference,
        -DEMAND_LOW,
        DEMAND_LOW,
        false,
    );

    let mut zone_state = Vec::with_capacity(ROWS.len());
    for index in 0..ROWS.len() {
        let heating = heating_signal[index] && heating_wins_tiebreak[index];
        let cooling = !heating && cooling_signal[index];
        let deadband = !(heating || cooling);
        assert_eq!(
            usize::from(heating) + usize::from(cooling) + usize::from(deadband),
            1,
            "ZoneStates must select exactly one state at row {index}"
        );
        zone_state.push(if heating {
            HEATING_STATE
        } else if cooling {
            COOLING_STATE
        } else {
            DEADBAND_STATE
        });
    }

    assert_eq!(zone_state[0], DEADBAND_STATE, "quiescent start");
    assert_eq!(zone_state[1], DEADBAND_STATE, "uHigh equality must not arm");
    assert_eq!(zone_state[4], HEATING_STATE, "heating uLow equality must hold");
    assert_eq!(zone_state[5], DEADBAND_STATE, "heating below uLow must release");
    assert_eq!(zone_state[9], COOLING_STATE, "cooling uLow equality must hold");
    assert_eq!(zone_state[10], DEADBAND_STATE, "cooling below uLow must release");
    assert!(cooling_signal[12], "ZS3 cooling signal must be live under heating priority");
    assert_eq!(
        zone_state[13],
        HEATING_STATE,
        "an interior negative tie-break value must hold heating"
    );
    assert_eq!(zone_state[14], COOLING_STATE, "crossing below -0.01 must select cooling");
    assert_eq!(
        zone_state[16],
        COOLING_STATE,
        "an interior positive tie-break value must hold cooling"
    );
    assert_eq!(zone_state[17], HEATING_STATE, "crossing above +0.01 must select heating");

    for (from, to) in [
        (HEATING_STATE, DEADBAND_STATE),
        (DEADBAND_STATE, COOLING_STATE),
        (COOLING_STATE, DEADBAND_STATE),
        (DEADBAND_STATE, HEATING_STATE),
        (HEATING_STATE, COOLING_STATE),
        (COOLING_STATE, HEATING_STATE),
    ] {
        assert!(
            zone_state
                .windows(2)
                .any(|pair| pair == [from, to]),
            "missing ZoneStates transition {from}->{to}"
        );
    }

    zone_state
}

fn zone_state_inputs() -> Vec<InputSeries> {
    vec![
        input_r(
            "heating_control",
            ROWS.iter().map(|row| row.heating_control),
        ),
        input_r(
            "cooling_control",
            ROWS.iter().map(|row| row.cooling_control),
        ),
    ]
}
