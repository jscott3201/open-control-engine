//! G36 Reheat Overrides source-verified sequence oracle.
//!
//! The oracle follows the pinned `Overrides.mo` graph without importing engine code. Damper and
//! valve positions are dimensionless fractions. The source makes `cloDam.y` identically zero:
//! both `realTrue` and the CDL-default `realFalse` are zero, so the `cloDam -> add3.u1` wire has an
//! output-unobservable constant contribution. Its parameters remain falsifiable: row A exposes a
//! wrong `realTrue`, row B exposes a wrong `realFalse`, and the resolver golden pins both values.
//! Crossing `pro.u1` and `pro.u2` is a commutative multiplication no-op and therefore cannot be
//! distinguished by any input schedule; this is an upstream source property, not a coverage gap.

use crate::oracle::{Golden, ValueKind};

use super::{REHEAT_OVERRIDES, input_b, input_i, input_r, r, sequence_golden};

#[derive(Clone, Copy)]
struct Row {
    damper_override: i64,
    damper_command: f64,
    heating_off: bool,
    valve_command: f64,
}

const ROWS: [Row; 12] = [
    Row {
        damper_override: 1,
        damper_command: 0.37,
        heating_off: false,
        valve_command: 0.62,
    },
    Row {
        damper_override: 2,
        damper_command: 0.37,
        heating_off: false,
        valve_command: 0.62,
    },
    Row {
        damper_override: 0,
        damper_command: 0.37,
        heating_off: false,
        valve_command: 0.62,
    },
    Row {
        damper_override: 0,
        damper_command: 0.37,
        heating_off: true,
        valve_command: 0.62,
    },
    Row {
        damper_override: 2,
        damper_command: 0.37,
        heating_off: true,
        valve_command: 0.62,
    },
    // Load-bearing: only a non-special positive index kills Equal -> >= comparator mutations.
    Row {
        damper_override: 3,
        damper_command: 0.37,
        heating_off: false,
        valve_command: 0.62,
    },
    Row {
        damper_override: 1,
        damper_command: 0.37,
        heating_off: true,
        valve_command: 0.62,
    },
    Row {
        damper_override: 3,
        damper_command: 0.37,
        heating_off: true,
        valve_command: 0.62,
    },
    Row {
        damper_override: -1,
        damper_command: 0.81,
        heating_off: false,
        valve_command: 0.23,
    },
    Row {
        damper_override: 1,
        damper_command: 0.81,
        heating_off: false,
        valve_command: 0.23,
    },
    Row {
        damper_override: 2,
        damper_command: 0.81,
        heating_off: false,
        valve_command: 0.23,
    },
    Row {
        damper_override: 0,
        damper_command: 0.81,
        heating_off: true,
        valve_command: 0.23,
    },
];

/// Build independent Tier-A goldens for the overridden damper and heating-valve commands.
///
/// Every input and output position is a dimensionless fraction. The schedule covers close, open,
/// passthrough, valve-off, combined overrides, a non-special positive override index, and a second
/// pair of distinct non-binary command values. The two output paths are derived independently.
pub(super) fn goldens() -> Vec<Golden> {
    let time: Vec<f64> = (0..ROWS.len()).map(|tick| tick as f64).collect();
    let inputs = vec![
        input_i(
            "damper_override_index",
            ROWS.iter().map(|row| row.damper_override),
        ),
        input_r(
            "damper_command_input",
            ROWS.iter().map(|row| row.damper_command),
        ),
        input_b("heating_valve_off", ROWS.iter().map(|row| row.heating_off)),
        input_r(
            "heating_valve_command_input",
            ROWS.iter().map(|row| row.valve_command),
        ),
    ];
    let damper: Vec<f64> = ROWS
        .iter()
        .map(|row| match row.damper_override {
            1 => 0.0,
            2 => 1.0,
            _ => row.damper_command,
        })
        .collect();
    let valve: Vec<f64> = ROWS
        .iter()
        .map(|row| {
            if row.heating_off {
                0.0
            } else {
                row.valve_command
            }
        })
        .collect();

    vec![
        sequence_golden(
            REHEAT_OVERRIDES,
            "damper_command",
            ValueKind::Real,
            time.clone(),
            damper.into_iter().map(r).collect(),
            "Overrides: indices 1/2 force 0/1; all other integers pass through distinct 0.37 and 0.81 commands",
            "Overrides.mo: Equal(k=1/k=2) gates feed Or and Switch; add3 sums cloDam=0 with opeDam=1 only for index 2",
            inputs.clone(),
        ),
        sequence_golden(
            REHEAT_OVERRIDES,
            "heating_valve_command",
            ValueKind::Real,
            time,
            valve.into_iter().map(r).collect(),
            "Overrides: uHeaOff toggles across override states with distinct 0.62 and 0.23 valve commands",
            "Overrides.mo: BooleanToReal maps true/false to 0/1 and Multiply gates uVal independently of the damper path",
            inputs,
        ),
    ]
}
