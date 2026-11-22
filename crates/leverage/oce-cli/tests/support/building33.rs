//! The Building 33 worked example laid out for `oce verify --commission` (issue #755). The spec,
//! the mapping, the authored sequence and its emitted CXF come from `oce-verify`'s fixture
//! (`tests/fixtures/commissioning/building33/`, **authored from the vendored model, not
//! vendored**); the six trends are copied from the vendored tree under `third_party/`, read in
//! place and never edited, into the scratch directory's `data/`, so every path the spec names
//! stays inside the spec's directory as the verb requires.

use std::fs;
use std::path::PathBuf;

use oce_api::{Engine, Value};

use super::commission::VERIFY_FIXTURES;
use super::{crate_dir, scratch};

/// The vendored trend directory, relative to this crate.
const TRENDS: &str = "../../../third_party/modelica-buildings-cdl/Buildings/Resources/Data/\
                      Utilities/Plotters/Examples/ControlsVerification_CoolingCoilValve";

/// The six trend files the spec names under `data/`.
pub const TREND_FILES: [&str; 6] = [
    "Supply_Air_Temp.mos",
    "SA_Clg_Stpt.mos",
    "OA_Temp.mos",
    "VFD_Fan_Feedback.mos",
    "VFD_Fan_Enable.mos",
    "Clg_Coil_Valve.mos",
];

/// The emitted document's root, the prefix of every boundary path.
pub const ROOT: &str = "urn:oce:author:CoolingCoilValve.";

/// The bytes of a file of `oce-verify`'s Building 33 fixture.
pub fn example_file(name: &str) -> Vec<u8> {
    let path = crate_dir()
        .join(VERIFY_FIXTURES)
        .join("commissioning/building33")
        .join(name);
    fs::read(&path).unwrap_or_else(|error| panic!("read {name}: {error}"))
}

/// The bytes of a file of `oce-verify`'s fixtures, by its path under `tests/fixtures/`.
pub fn verify_fixture(relative: &str) -> Vec<u8> {
    let path = crate_dir().join(VERIFY_FIXTURES).join(relative);
    fs::read(&path).unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

/// The bytes of a vendored trend file.
pub fn trend(name: &str) -> Vec<u8> {
    let path = crate_dir().join(TRENDS).join(name);
    fs::read(&path).unwrap_or_else(|error| panic!("read {name}: {error}"))
}

/// A scratch directory holding the worked example: `spec.json`, `b33-points.json`,
/// `cooling_coil_valve.cxf.json` and the six trends under `data/`.
pub fn building_33_layout(name: &str) -> PathBuf {
    let dir = scratch(name);
    for file in [
        "spec.json",
        "b33-points.json",
        "cooling_coil_valve.cxf.json",
    ] {
        fs::write(dir.join(file), example_file(file)).expect("a fixture file");
    }
    fs::create_dir(dir.join("data")).expect("data/");
    for file in TREND_FILES {
        fs::write(dir.join("data").join(file), trend(file)).expect("a trend");
    }
    dir
}

/// One frame of `valve_oracle.txt`: time, `TSup`, `TSupSet`, `TOut`, `uFanFee`, `uFanSta` and
/// the derived valve signal's bits.
pub struct OracleRow {
    /// The frame's time.
    pub t: f64,
    /// `TSup`, `TSupSet`, `TOut`, `uFanFee`.
    pub reals: [f64; 4],
    /// `uFanSta`.
    pub fan_sta: bool,
    /// The derived `yCooVal`.
    pub valve: f64,
}

/// The frames of the independent derivation, `derive_valve.py`'s `valve_oracle.txt`.
pub fn oracle_rows() -> Vec<OracleRow> {
    String::from_utf8(example_file("valve_oracle.txt"))
        .expect("UTF-8")
        .lines()
        .filter(|line| line.starts_with("row\t"))
        .map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            let number = |i: usize| cells[i].parse::<f64>().expect("a number");
            OracleRow {
                t: number(1),
                reals: [number(2), number(3), number(4), number(5)],
                fan_sta: cells[6] == "true",
                valve: f64::from_bits(u64::from_str_radix(cells[7], 16).expect("hex bits")),
            }
        })
        .collect()
}

/// Whether the engine, loaded with `cxf`, gives every oracle frame's valve signal bit for bit.
pub fn matches_oracle(cxf: &[u8]) -> bool {
    let mut engine = Engine::new();
    engine.load_cxf(cxf).expect("the CXF loads");
    let names = ["TSup", "TSupSet", "TOut", "uFanFee", "uFanSta", "yCooVal"]
        .map(|name| format!("{ROOT}{name}"));
    oracle_rows().iter().all(|row| {
        let mut entries: Vec<(&str, Value)> = row
            .reals
            .iter()
            .zip(&names)
            .map(|(x, name)| (name.as_str(), Value::Real(*x)))
            .collect();
        entries.push((names[4].as_str(), Value::Boolean(row.fan_sta)));
        let frame = engine.prepare_frame(row.t, &entries).expect("prepares");
        let done = engine.execute_frame(frame).expect("executes");
        matches!(
            done.outputs().iter().find(|(path, _)| *path == names[5]),
            Some((_, Value::Real(y))) if y.to_bits() == row.valve.to_bits()
        )
    })
}
