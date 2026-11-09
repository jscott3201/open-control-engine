//! The `cooling_only_controller` fixture driven from its golden-gen reference table, the same
//! files the conformance crate's Tier-2 determinism golden reads
//! (`tools/golden-gen/goldens/G36/cooling_only_controller/reference.csv` for the inputs,
//! `crates/oce-conformance/tests/fixtures/golden/g36_traces/cooling_only_controller.csv` for the
//! outputs), read in place.
//!
//! The reference columns map to the fixture's inputs by *suffix* (`zone_temperature` → `TZon`),
//! exactly as the conformance test feeds them: Booleans are `cell != 0.0`, Integers `cell as i64`.
//! The reference's output columns are declared too, so the map lists them as ignored.

#![allow(dead_code)]

use oce_api::{Engine, Value};
use oce_replay::{
    Column, ColumnKind, ColumnMap, ColumnSpec, ColumnarSource, InputMap, MissingPolicy,
    ReplayLimits, StepSource, TimeColumn,
};

use crate::fixtures::workspace_root;

/// The fixture name.
pub const FIXTURE: &str = "cooling_only_controller";

/// Reference column → input suffix and kind (`R`eal, `I`nteger, `B`oolean).
pub const MAPPING: [(&str, &str, char); 14] = [
    ("zone_temperature", "TZon", 'R'),
    ("cooling_setpoint", "TCooSet", 'R'),
    ("heating_setpoint", "THeaSet", 'R'),
    ("window_status", "u1Win", 'B'),
    ("occupancy_status", "u1Occ", 'B'),
    ("operating_mode", "uOpeMod", 'I'),
    ("co2_setpoint", "ppmCO2Set", 'R'),
    ("co2_concentration", "ppmCO2", 'R'),
    ("discharge_air_temperature", "TDis", 'R'),
    ("supply_air_temperature", "TSup", 'R'),
    ("discharge_airflow", "VDis_flow", 'R'),
    ("airflow_override_index", "oveFloSet", 'I'),
    ("damper_override_index", "oveDamPos", 'I'),
    ("supply_fan_status", "u1Fan", 'B'),
];

/// A whitespace-separated CombiTimeTable text: `# columns:` names and numeric rows.
pub struct Table {
    /// Column names, `time` first.
    pub names: Vec<String>,
    /// Rows of cells.
    pub rows: Vec<Vec<f64>>,
}

impl Table {
    fn read(relative: &str) -> Self {
        let path = workspace_root().join(relative);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        let names = text
            .lines()
            .find_map(|line| line.strip_prefix("# columns: "))
            .expect("columns line")
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let rows = text
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("double "))
            .map(|l| l.split_whitespace().map(|c| c.parse().unwrap()).collect())
            .collect();
        Self { names, rows }
    }

    /// The column called `name`.
    pub fn column(&self, name: &str) -> Vec<f64> {
        let index = self.names.iter().position(|n| n == name).unwrap();
        self.rows.iter().map(|r| r[index]).collect()
    }
}

/// The golden-gen reference table (inputs and reference outputs, one row per minute).
pub fn reference() -> Table {
    Table::read("tools/golden-gen/goldens/G36/cooling_only_controller/reference.csv")
}

/// The conformance crate's Tier-2 determinism golden (engine outputs, one row per minute).
pub fn golden() -> Table {
    Table::read(
        "crates/oce-conformance/tests/fixtures/golden/g36_traces/cooling_only_controller.csv",
    )
}

fn kind(code: char) -> ColumnKind {
    match code {
        'R' => ColumnKind::Real,
        'I' => ColumnKind::Integer,
        _ => ColumnKind::Boolean,
    }
}

fn value(code: char, cell: f64) -> Value {
    match code {
        'R' => Value::Real(cell),
        #[allow(clippy::cast_possible_truncation)]
        'I' => Value::Integer(cell as i64),
        _ => Value::Boolean(cell != 0.0),
    }
}

/// The suffix map over every reference column but time; unmapped ones are ignored.
pub fn map(engine: &Engine, reference: &Table, missing: &MissingPolicy) -> InputMap {
    let specs: Vec<ColumnSpec> = reference.names[1..]
        .iter()
        .map(|name| {
            let code = MAPPING.iter().find(|m| m.0 == name).map_or('R', |m| m.2);
            ColumnSpec::new(name.clone(), kind(code))
        })
        .collect();
    let entries = MAPPING
        .iter()
        .map(|(column, suffix, _)| ((*column).to_owned(), (*suffix).to_owned()))
        .collect();
    InputMap::resolve(
        &engine.input_definitions().unwrap(),
        &specs,
        &ColumnMap { entries },
        missing,
        &[],
    )
    .unwrap()
}

/// The mapped reference columns as owned typed columns.
pub fn columns(reference: &Table) -> Vec<(String, Column)> {
    MAPPING
        .iter()
        .map(|(name, _, code)| {
            let cells = reference.column(name);
            let column = match code {
                'R' => Column::Real(cells),
                #[allow(clippy::cast_possible_truncation)]
                'I' => Column::Integer {
                    values: cells.iter().map(|c| *c as i64).collect(),
                    valid: None,
                },
                _ => Column::Boolean {
                    values: cells.iter().map(|c| *c != 0.0).collect(),
                    valid: None,
                },
            };
            ((*name).to_owned(), column)
        })
        .collect()
}

/// A columnar source over the whole reference table.
pub fn columnar(map: &InputMap, reference: &Table) -> ColumnarSource {
    let time = TimeColumn::Seconds(reference.column("time"));
    ColumnarSource::new(map, time, columns(reference), &ReplayLimits::default()).unwrap()
}

/// A step source over the reference table: change-only lists, every row time an instant.
pub fn steps(map: &InputMap, reference: &Table) -> StepSource {
    let times = reference.column("time");
    let lists = MAPPING
        .iter()
        .map(|(name, _, code)| {
            let mut list: Vec<(f64, Value)> = Vec::new();
            for (t, cell) in times.iter().zip(reference.column(name)) {
                let v = value(*code, cell);
                if list
                    .last()
                    .is_none_or(|(_, last)| !crate::fixtures::bits_eq(last, &v))
                {
                    list.push((*t, v));
                }
            }
            ((*name).to_owned(), list)
        })
        .collect();
    StepSource::with_instants(map, lists, times, &ReplayLimits::default()).unwrap()
}
