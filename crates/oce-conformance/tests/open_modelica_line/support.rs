//! Private canonical views, facade execution, and independent Line bit tables.

#![allow(dead_code)]

use crate::block_harness::{
    BlockCase, Param, ParamValue, Port, R, case, drive_case_with_external_reference_at_instants,
};
use oce_conformance::{CombiTimeTable, ComparisonResult, DriveMode};

const INPUTS: &[Port] = &[
    Port {
        name: "x1",
        kind: R,
    },
    Port {
        name: "f1",
        kind: R,
    },
    Port {
        name: "x2",
        kind: R,
    },
    Port {
        name: "f2",
        kind: R,
    },
    Port { name: "u", kind: R },
];
const OUTPUTS: &[Port] = &[Port { name: "y", kind: R }];
const BOTH_PARAMS: &[Param] = &[
    Param {
        name: "limitBelow",
        value: ParamValue::Boolean("true"),
    },
    Param {
        name: "limitAbove",
        value: ParamValue::Boolean("true"),
    },
];
const BELOW_PARAMS: &[Param] = &[
    Param {
        name: "limitBelow",
        value: ParamValue::Boolean("true"),
    },
    Param {
        name: "limitAbove",
        value: ParamValue::Boolean("false"),
    },
];
const ABOVE_PARAMS: &[Param] = &[
    Param {
        name: "limitBelow",
        value: ParamValue::Boolean("false"),
    },
    Param {
        name: "limitAbove",
        value: ParamValue::Boolean("true"),
    },
];
const UNLIMITED_PARAMS: &[Param] = &[
    Param {
        name: "limitBelow",
        value: ParamValue::Boolean("false"),
    },
    Param {
        name: "limitAbove",
        value: ParamValue::Boolean("false"),
    },
];
const CASES: &[BlockCase] = &[
    case(
        "openmodelica_reals_line_both",
        "CDL.Reals.Line",
        "external",
        INPUTS,
        BOTH_PARAMS,
        OUTPUTS,
    ),
    case(
        "openmodelica_reals_line_below",
        "CDL.Reals.Line",
        "external",
        INPUTS,
        BELOW_PARAMS,
        OUTPUTS,
    ),
    case(
        "openmodelica_reals_line_above",
        "CDL.Reals.Line",
        "external",
        INPUTS,
        ABOVE_PARAMS,
        OUTPUTS,
    ),
    case(
        "openmodelica_reals_line_unlimited",
        "CDL.Reals.Line",
        "external",
        INPUTS,
        UNLIMITED_PARAMS,
        OUTPUTS,
    ),
];

pub(crate) const SCENARIO: &str = "four_limit_modes_five_dyadic_regions";
pub(crate) const TIME_BITS: &[u64] = &[
    0x0000_0000_0000_0000,
    0x404e_0000_0000_0000,
    0x404e_0000_0000_0eff,
    0x405e_0000_0000_0000,
    0x405e_0000_0000_0781,
    0x4066_8000_0000_0000,
    0x4066_8000_0000_03c1,
    0x406e_0000_0000_0000,
    0x406e_0000_0000_03c1,
    0x4072_c000_0000_0000,
];
pub(crate) const EXPECTED: [[u64; 10]; 4] = [
    [
        0x3ff4_0000_0000_0000,
        0x3ff4_0000_0000_0000,
        0x3ff4_0000_0000_0000,
        0x3ff4_0000_0000_0000,
        0x4002_0000_0000_0000,
        0x4002_0000_0000_0000,
        0x400a_0000_0000_0000,
        0x400a_0000_0000_0000,
        0x400a_0000_0000_0000,
        0x400a_0000_0000_0000,
    ],
    [
        0x3ff4_0000_0000_0000,
        0x3ff4_0000_0000_0000,
        0x3ff4_0000_0000_0000,
        0x3ff4_0000_0000_0000,
        0x4002_0000_0000_0000,
        0x4002_0000_0000_0000,
        0x400a_0000_0000_0000,
        0x400a_0000_0000_0000,
        0x4011_0000_0000_0000,
        0x4011_0000_0000_0000,
    ],
    [
        0x3fd0_0000_0000_0000,
        0x3fd0_0000_0000_0000,
        0x3ff4_0000_0000_0000,
        0x3ff4_0000_0000_0000,
        0x4002_0000_0000_0000,
        0x4002_0000_0000_0000,
        0x400a_0000_0000_0000,
        0x400a_0000_0000_0000,
        0x400a_0000_0000_0000,
        0x400a_0000_0000_0000,
    ],
    [
        0x3fd0_0000_0000_0000,
        0x3fd0_0000_0000_0000,
        0x3ff4_0000_0000_0000,
        0x3ff4_0000_0000_0000,
        0x4002_0000_0000_0000,
        0x4002_0000_0000_0000,
        0x400a_0000_0000_0000,
        0x400a_0000_0000_0000,
        0x4011_0000_0000_0000,
        0x4011_0000_0000_0000,
    ],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Both,
    Below,
    Above,
    Unlimited,
}

impl Mode {
    fn index(self) -> usize {
        match self {
            Self::Both => 0,
            Self::Below => 1,
            Self::Above => 2,
            Self::Unlimited => 3,
        }
    }
    fn source_column(self) -> &'static str {
        match self {
            Self::Both => "yBoth",
            Self::Below => "yBelow",
            Self::Above => "yAbove",
            Self::Unlimited => "yUnlimited",
        }
    }
}

pub(crate) struct ModeOutcome {
    pub(crate) mode: Mode,
    pub(crate) reference_bits: Vec<u64>,
    pub(crate) engine_bits: Vec<u64>,
    pub(crate) comparison: ComparisonResult,
    pub(crate) drive_mode: DriveMode,
    pub(crate) warning_count: usize,
}

pub(crate) fn canonical() -> Result<CombiTimeTable, String> {
    CombiTimeTable::parse(
        std::str::from_utf8(include_bytes!(
            "../fixtures/open_modelica/reals_line/arm64/line.canonical.csv"
        ))
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn flag_control() -> Result<CombiTimeTable, String> {
    CombiTimeTable::parse(
        std::str::from_utf8(include_bytes!(
            "../fixtures/open_modelica/reals_line/arm64/flag-control.canonical.csv"
        ))
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn views(source: &CombiTimeTable) -> Result<Vec<(Mode, CombiTimeTable)>, String> {
    [Mode::Both, Mode::Below, Mode::Above, Mode::Unlimited]
        .into_iter()
        .map(|mode| Ok((mode, view(source, mode, mode.source_column())?)))
        .collect()
}

pub(crate) fn view(
    source: &CombiTimeTable,
    mode: Mode,
    output_column: &str,
) -> Result<CombiTimeTable, String> {
    if source.n_rows != 10 || source.n_cols != 10 {
        return Err("unexpected canonical Line table shape".into());
    }
    let names = source
        .col_names
        .as_ref()
        .ok_or("canonical Line columns missing")?;
    let required = ["time", "x1", "f1", "x2", "f2", "u", output_column];
    let indices = required
        .iter()
        .map(|name| {
            names
                .iter()
                .position(|candidate| candidate == name)
                .ok_or_else(|| format!("canonical Line column {name} missing"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut data = Vec::with_capacity(source.n_rows * indices.len());
    for row in source.data.chunks_exact(source.n_cols) {
        data.extend(indices.iter().map(|index| row[*index]));
    }
    Ok(CombiTimeTable {
        name: format!("openmodelica_reals_line_{:?}", mode).to_ascii_lowercase(),
        n_rows: source.n_rows,
        n_cols: indices.len(),
        data,
        col_names: Some(
            ["time", "x1", "f1", "x2", "f2", "u", "y"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        ),
    })
}

pub(crate) fn evaluate(mode: Mode, reference: &CombiTimeTable) -> ModeOutcome {
    let instants = TIME_BITS.iter().map(|bits| f64::from_bits(*bits)).collect();
    let run = drive_case_with_external_reference_at_instants(
        &CASES[mode.index()],
        SCENARIO,
        reference,
        instants,
    );
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
        .expect("captured Line output");
    let y = reference
        .col_names
        .as_ref()
        .unwrap()
        .iter()
        .position(|name| name == "y")
        .unwrap();
    ModeOutcome {
        mode,
        reference_bits: reference
            .data
            .chunks_exact(reference.n_cols)
            .map(|row| row[y].to_bits())
            .collect(),
        engine_bits: output.values.iter().map(|value| value.to_bits()).collect(),
        comparison: signal.result.clone(),
        drive_mode: run.drive_mode,
        warning_count: run.load_warnings.len(),
    }
}

pub(crate) fn mutated_reference(
    source: &CombiTimeTable,
    mode: Mode,
    mutant: fn(Mode, f64) -> f64,
) -> Result<CombiTimeTable, String> {
    let mut reference = view(source, mode, mode.source_column())?;
    for row in reference.data.chunks_exact_mut(reference.n_cols) {
        let input = row[5];
        row[6] = mutant(mode, input);
    }
    Ok(reference)
}

fn result_with_x(x: f64, omit_intercept: bool) -> f64 {
    let slope = (3.25 - 1.25) / (2.0 - -2.0);
    if omit_intercept {
        slope * x
    } else {
        3.25 - slope * 2.0 + slope * x
    }
}

pub(crate) fn always_clamp(_: Mode, u: f64) -> f64 {
    result_with_x(u.clamp(-2.0, 2.0), false)
}
pub(crate) fn never_clamp(_: Mode, u: f64) -> f64 {
    result_with_x(u, false)
}
pub(crate) fn swapped_flags(mode: Mode, u: f64) -> f64 {
    let x = match mode {
        Mode::Both => u.clamp(-2.0, 2.0),
        Mode::Below => u.min(2.0),
        Mode::Above => u.max(-2.0),
        Mode::Unlimited => u,
    };
    result_with_x(x, false)
}
pub(crate) fn omitted_intercept(mode: Mode, u: f64) -> f64 {
    let x = match mode {
        Mode::Both => u.clamp(-2.0, 2.0),
        Mode::Below => u.max(-2.0),
        Mode::Above => u.min(2.0),
        Mode::Unlimited => u,
    };
    result_with_x(x, true)
}
