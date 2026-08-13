//! Private facade execution and independent Toggle recurrence.

use std::io::Read as _;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;

use crate::block_harness::{
    B, BlockCase, Port, case, drive_case_with_external_reference_at_instants,
};
use oce_conformance::{CombiTimeTable, ComparisonResult, DriveMode};

const INPUTS: &[Port] = &[
    Port { name: "u", kind: B },
    Port {
        name: "clr",
        kind: B,
    },
];
const OUTPUTS: &[Port] = &[Port { name: "y", kind: B }];
const CASE: BlockCase = case(
    "openmodelica_logical_toggle",
    "CDL.Logical.Toggle",
    "external",
    INPUTS,
    &[],
    OUTPUTS,
);
pub(crate) const SCENARIO: &str = "repeated_rises_initial_true_and_clear_priority";
const MANIFEST: &str =
    "crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/manifest.json";
pub(crate) const MAX_REFERENCE_BYTES: usize = 1024 * 1024;
#[cfg(any(target_os = "linux", target_os = "android"))]
const SAFE_OPEN_FLAGS: i32 = 0x0002_0000 | 0x0000_0800;
#[cfg(any(target_os = "macos", target_os = "ios"))]
const SAFE_OPEN_FLAGS: i32 = 0x0000_0100 | 0x0000_0004;
// Other Unix targets retain both regular-file checks and the bounded descriptor read.
#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios"
    ))
))]
compile_error!("Toggle evidence file reads support Linux, Android, macOS, and iOS Unix targets");
pub(crate) const TIME_BITS: &[u64] = &[
    0x0000_0000_0000_0000,
    0x403e_0000_0000_1dff,
    0x404e_0000_0000_0000,
    0x4056_8000_0000_0780,
    0x405e_0000_0000_0000,
    0x4062_c000_0000_03c1,
    0x4066_8000_0000_0000,
    0x406a_4000_0000_03c1,
    0x406e_0000_0000_0000,
    0x4070_e000_0000_01e0,
    0x4072_c000_0000_0000,
    0x4073_6000_0000_0320,
    0x4075_e000_0000_02d0,
    0x4076_8000_0000_0000,
    0x4078_6000_0000_03c1,
    0x407a_4000_0000_0000,
    0x407a_e000_0000_0320,
    0x407c_2000_0000_03c1,
    0x407e_0000_0000_0000,
    0x407f_e000_0000_03c1,
    0x4080_e000_0000_0000,
    0x4082_c000_0000_0000,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Party {
    OpenModelica,
    Engine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RecurrenceDiscrepancy {
    pub(crate) row: usize,
    pub(crate) party: Party,
    pub(crate) expected: bool,
    pub(crate) observed: bool,
}

pub(crate) struct ScopedToggleOutcome {
    pub(crate) producer: &'static str,
    pub(crate) class: &'static str,
    pub(crate) scenario: &'static str,
    pub(crate) manifest: &'static str,
    pub(crate) comparison: ComparisonResult,
    pub(crate) drive_mode: DriveMode,
    pub(crate) times: Vec<u64>,
    pub(crate) engine: Vec<bool>,
    pub(crate) load_warning_count: usize,
}

pub(crate) fn evaluate(reference: &CombiTimeTable) -> ScopedToggleOutcome {
    let instants = TIME_BITS.iter().map(|bits| f64::from_bits(*bits)).collect();
    let run = drive_case_with_external_reference_at_instants(&CASE, SCENARIO, reference, instants);
    assert_eq!(run.comparisons.len(), 1);
    let signal = &run.comparisons[0];
    assert_eq!(signal.reference_column, "y");
    assert!(!signal.masked);
    assert_eq!(
        [
            signal.tolerance.atolx,
            signal.tolerance.atoly,
            signal.tolerance.rtolx,
            signal.tolerance.rtoly,
            signal.tolerance.ltolx,
            signal.tolerance.ltoly,
        ],
        [0.0; 6]
    );
    let output = run
        .trace
        .columns
        .iter()
        .find(|column| column.name.ends_with(".block.y"))
        .expect("captured Toggle output");
    assert_eq!(run.trace.times.len(), reference.n_rows);
    assert_eq!(output.values.len(), run.trace.times.len());
    ScopedToggleOutcome {
        producer: "OpenModelica 1.25.1",
        class: "CDL.Logical.Toggle",
        scenario: SCENARIO,
        manifest: MANIFEST,
        comparison: signal.result.clone(),
        drive_mode: run.drive_mode,
        times: run
            .trace
            .times
            .iter()
            .map(|value| value.to_bits())
            .collect(),
        engine: output.values.iter().map(|value| *value == 1.0).collect(),
        load_warning_count: run.load_warnings.len(),
    }
}

pub(crate) fn read_reference(name: &str) -> Result<CombiTimeTable, String> {
    let bytes = match name {
        "toggle.canonical.csv" => {
            include_bytes!("../fixtures/open_modelica/logical_toggle/toggle.canonical.csv")
                .as_slice()
        }
        "latch.canonical.csv" => {
            include_bytes!("../fixtures/open_modelica/logical_toggle/latch.canonical.csv")
                .as_slice()
        }
        _ => return Err("unsupported scoped OpenModelica Toggle fixture".into()),
    };
    parse_reference_bytes(bytes)
}

pub(crate) fn read_reference_path(path: &Path) -> Result<CombiTimeTable, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("canonical reference must be a regular non-symlink file".into());
    }
    if metadata.len() > MAX_REFERENCE_BYTES as u64 {
        return Err(format!(
            "canonical reference exceeds {MAX_REFERENCE_BYTES} bytes"
        ));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(SAFE_OPEN_FLAGS);
    let file = options.open(path).map_err(|error| error.to_string())?;
    let opened = file.metadata().map_err(|error| error.to_string())?;
    if !opened.is_file() {
        return Err("opened canonical reference is not a regular file".into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_REFERENCE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    parse_reference_bytes(&bytes)
}

pub(crate) fn parse_reference_bytes(bytes: &[u8]) -> Result<CombiTimeTable, String> {
    if bytes.len() > MAX_REFERENCE_BYTES {
        return Err(format!(
            "canonical reference exceeds {MAX_REFERENCE_BYTES} bytes"
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    CombiTimeTable::parse(text).map_err(|error| error.to_string())
}

pub(crate) fn recurrence_discrepancies(
    reference: &CombiTimeTable,
    engine: &[bool],
) -> Result<Vec<RecurrenceDiscrepancy>, String> {
    let rows = rows(reference)?;
    if engine.len() != rows.len() {
        return Err("engine and external row counts differ".into());
    }
    let expected = expected_toggle(&rows);
    let mut discrepancies = Vec::new();
    for (row, ((input, expected), engine)) in rows.iter().zip(&expected).zip(engine).enumerate() {
        for (party, observed) in [(Party::OpenModelica, input.y), (Party::Engine, *engine)] {
            if observed != *expected {
                discrepancies.push(RecurrenceDiscrepancy {
                    row,
                    party,
                    expected: *expected,
                    observed,
                });
            }
        }
    }
    Ok(discrepancies)
}

pub(crate) fn initial_false_mismatch_rows(
    reference: &CombiTimeTable,
) -> Result<Vec<usize>, String> {
    let rows = rows(reference)?;
    let expected = expected_toggle(&rows);
    let wrong = recurrence_with(&rows, |_| false, toggle_step);
    Ok(mismatch_rows(&expected, &wrong))
}

pub(crate) fn every_input_change_mismatch_rows(
    reference: &CombiTimeTable,
) -> Result<Vec<usize>, String> {
    let rows = rows(reference)?;
    let expected = expected_toggle(&rows);
    let wrong = recurrence_with(
        &rows,
        |row| !row.clr && row.u,
        |row, prior, held| {
            if row.u != prior.u || row.clr != prior.clr {
                !held
            } else {
                held
            }
        },
    );
    Ok(mismatch_rows(&expected, &wrong))
}

pub(crate) fn ignore_all_clear_mismatch_rows(
    reference: &CombiTimeTable,
) -> Result<Vec<usize>, String> {
    let rows = rows(reference)?;
    let expected = expected_toggle(&rows);
    let wrong = recurrence_with(
        &rows,
        |row| !row.clr && row.u,
        |row, prior, held| {
            if row.u && !prior.u { !held } else { held }
        },
    );
    Ok(mismatch_rows(&expected, &wrong))
}

pub(crate) fn ignore_clear_on_simultaneous_rise_mismatch_rows(
    reference: &CombiTimeTable,
) -> Result<Vec<usize>, String> {
    let rows = rows(reference)?;
    let expected = expected_toggle(&rows);
    let wrong = recurrence_with(
        &rows,
        |row| !row.clr && row.u,
        |row, prior, held| {
            if row.u && !prior.u {
                !held
            } else if row.clr {
                false
            } else {
                held
            }
        },
    );
    Ok(mismatch_rows(&expected, &wrong))
}

#[derive(Clone, Copy)]
struct Row {
    u: bool,
    clr: bool,
    y: bool,
}

fn rows(reference: &CombiTimeTable) -> Result<Vec<Row>, String> {
    if reference.n_rows != TIME_BITS.len() || reference.n_cols != 4 {
        return Err("unexpected canonical Toggle table shape".into());
    }
    let names = reference
        .col_names
        .as_ref()
        .ok_or("canonical columns missing")?;
    if names != &["time", "u", "clr", "y"] {
        return Err("canonical columns are not exactly time u clr y".into());
    }
    reference
        .data
        .chunks_exact(4)
        .enumerate()
        .map(|(index, values)| {
            if values[0].to_bits() != TIME_BITS[index] {
                return Err(format!("canonical time bits drifted at row {index}"));
            }
            Ok(Row {
                u: boolean(values[1], index, "u")?,
                clr: boolean(values[2], index, "clr")?,
                y: boolean(values[3], index, "y")?,
            })
        })
        .collect()
}

fn expected_toggle(rows: &[Row]) -> Vec<bool> {
    recurrence_with(rows, |row| !row.clr && row.u, toggle_step)
}

fn toggle_step(row: Row, prior: Row, held: bool) -> bool {
    if row.clr {
        false
    } else if row.u && !prior.u {
        !held
    } else {
        held
    }
}

fn recurrence_with(
    rows: &[Row],
    first: impl FnOnce(Row) -> bool,
    mut step: impl FnMut(Row, Row, bool) -> bool,
) -> Vec<bool> {
    let mut values = Vec::with_capacity(rows.len());
    let mut held = first(rows[0]);
    values.push(held);
    for index in 1..rows.len() {
        held = step(rows[index], rows[index - 1], held);
        values.push(held);
    }
    values
}

fn mismatch_rows(expected: &[bool], wrong: &[bool]) -> Vec<usize> {
    expected
        .iter()
        .zip(wrong)
        .enumerate()
        .filter_map(|(index, (expected, wrong))| (expected != wrong).then_some(index))
        .collect()
}

fn boolean(value: f64, row: usize, column: &str) -> Result<bool, String> {
    match value {
        0.0 => Ok(false),
        1.0 => Ok(true),
        _ => Err(format!("row {row} column {column} is not Boolean")),
    }
}
