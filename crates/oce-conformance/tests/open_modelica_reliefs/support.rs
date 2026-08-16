//! Canonical Reliefs tables and public-facade execution helpers.

use oce_conformance::{
    CombiTimeTable, ComparisonMode, DriveCadence, DriverInputReplay, DriverOptions, DriverRun,
    ExactMismatch, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, VerifyConfig,
    drive_trace_with_options,
};

pub(crate) const CXF: &str = include_str!(
    "../../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_modulations_reliefs.jsonld"
);
const ROOT: &str = "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs";
pub(crate) const ROOT_OUTPUTS: [&str; 2] = [
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.yOutDam",
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.yRetDam",
];
pub(crate) const CHILD_OUTPUTS: [&str; 2] = [
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.min.y",
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.max.y",
];
pub(crate) const MISSING_ROOT: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.yMissing";
const INPUTS: [(&str, &str); 5] = [
    ("uTSup", "uTSup"),
    ("uOutDam_min", "uOutDam_min"),
    ("uOutDam_max", "uOutDam_max"),
    ("uRetDam_min", "uRetDam_min"),
    ("uRetDam_max", "uRetDam_max"),
];
pub(crate) const TIME_BITS: [u64; 7] = [
    0x0000_0000_0000_0000,
    0x404e_0000_0000_0eff,
    0x405e_0000_0000_0781,
    0x4066_8000_0000_03c1,
    0x406e_0000_0000_03c1,
    0x4072_c000_0000_03c1,
    0x4076_8000_0000_03c1,
];
pub(crate) const EXPECTED: [[u64; 7]; 2] = [
    [
        0x3fd0_0000_0000_0000,
        0x3fd0_0000_0000_0000,
        0x3fe2_0000_0000_0000,
        0x3fec_0000_0000_0000,
        0x3fec_0000_0000_0000,
        0x3fec_0000_0000_0000,
        0x3fec_0000_0000_0000,
    ],
    [
        0x3fe8_0000_0000_0000,
        0x3fe8_0000_0000_0000,
        0x3fe8_0000_0000_0000,
        0x3fe8_0000_0000_0000,
        0x3fdc_0000_0000_0000,
        0x3fc0_0000_0000_0000,
        0x3fc0_0000_0000_0000,
    ],
];
pub(crate) const ZERO_TOLERANCE: Tolerances = Tolerances {
    atolx: 0.0,
    atoly: 0.0,
    rtolx: 0.0,
    rtoly: 0.0,
    ltolx: 0.0,
    ltoly: 0.0,
};

pub(crate) fn canonical() -> Result<CombiTimeTable, String> {
    table(include_bytes!(
        "../fixtures/open_modelica/g36_reliefs/arm64/reliefs.canonical.csv"
    ))
}

pub(crate) fn parameter_control() -> Result<CombiTimeTable, String> {
    table(include_bytes!(
        "../fixtures/open_modelica/g36_reliefs/arm64/parameter-control.canonical.csv"
    ))
}

pub(crate) fn final_clamp() -> Result<CombiTimeTable, String> {
    table(include_bytes!(
        "../fixtures/open_modelica/g36_reliefs/arm64/final-clamp.canonical.csv"
    ))
}

fn table(bytes: &[u8]) -> Result<CombiTimeTable, String> {
    CombiTimeTable::parse(std::str::from_utf8(bytes).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

pub(crate) fn evaluate(
    reference: &CombiTimeTable,
    outputs: [&str; 2],
) -> Result<DriverRun, oce_conformance::DriverError> {
    drive_trace_with_options(
        CXF.as_bytes(),
        &config(outputs),
        reference,
        &DriverOptions {
            cadence: DriveCadence::EventAligned {
                instants: TIME_BITS.into_iter().map(f64::from_bits).collect(),
            },
            input_replay: DriverInputReplay::ReferenceTable,
            comparison: ComparisonMode::Exact,
        },
    )
}

fn config(outputs: [&str; 2]) -> VerifyConfig {
    let mut mapping = INPUTS
        .iter()
        .map(|(column, point)| PointMapEntry {
            cdl: end(&format!("{ROOT}.{point}")),
            device: end(column),
        })
        .collect::<Vec<_>>();
    mapping.extend(
        outputs
            .into_iter()
            .zip(["yOutDam", "yRetDam"])
            .map(|(point, column)| PointMapEntry {
                cdl: end(point),
                device: end(column),
            }),
    );
    VerifyConfig {
        references: vec![ReferenceSpec {
            model: "g36".into(),
            sequence: "source_default_dyadic_regions".into(),
            point_name_mapping: mapping,
        }],
        tolerances: ZERO_TOLERANCE,
        outputs: Vec::new(),
        indicators: Vec::new(),
        sampling: None,
        run_controller: true,
    }
}

fn end(name: &str) -> PointEnd {
    PointEnd {
        name: name.to_owned(),
        unit: None,
        kind: Some("Real".into()),
    }
}

pub(crate) fn column_bits(table: &CombiTimeTable, name: &str) -> Result<Vec<u64>, String> {
    let column = table
        .col_names
        .as_ref()
        .ok_or("canonical columns missing")?
        .iter()
        .position(|candidate| candidate == name)
        .ok_or_else(|| format!("canonical column {name} missing"))?;
    Ok(table
        .data
        .chunks_exact(table.n_cols)
        .map(|row| row[column].to_bits())
        .collect())
}

pub(crate) fn first_mismatch<'a>(run: &'a DriverRun, column: &str) -> &'a ExactMismatch {
    let comparison = run
        .comparisons
        .iter()
        .find(|comparison| comparison.reference_column == column)
        .unwrap_or_else(|| panic!("missing comparison for {column}"));
    let ComparisonResult::Exact(exact) = &comparison.result else {
        panic!("control comparison must be exact");
    };
    assert!(!exact.passed);
    exact.first_mismatch.as_ref().unwrap()
}

use oce_conformance::ComparisonResult;
