//! OBC §12.7.3's example files, padded with spaces as the specification prints them, read as
//! they are (issues #728 and #733): `reference_input.csv` as an input source and, in "the same
//! format", as a `controllerOutput`, and `indicator.csv` as an indicator source. Replay trims the
//! spaces and tabs around each csv cell and header name, and hands a `bool` device column over as
//! text, read by the type table's Boolean spellings.

use oce_replay::Column;
use oce_verify::{SeriesValues, TrendSources};

use super::{loaded, point, read_trends, trend_file, trend_spec_with};

/// The six points of `reference_input.csv`, each mapped to a CDL point of its own name.
fn mapping(device_prefix: &str) -> String {
    let points: Vec<String> = [
        ("uHea", None, "float"),
        ("uCoo", None, "float"),
        ("TZonSet", Some("K"), "float"),
        ("TZon", Some("K"), "float"),
        ("TOut", Some("K"), "float"),
        ("uFan", None, "bool"),
    ]
    .into_iter()
    .map(|(name, unit, kind)| {
        point(
            (name, unit, kind),
            (&format!("{device_prefix}{name}"), unit, kind),
        )
    })
    .collect();
    format!("[{}]", points.join(", "))
}

const COLUMNS: &str = r#""uHea": "uHea", "uCoo": "uCoo", "TZonSet": "TZonSet", "TZon": "TZon",
    "TOut": "TOut", "uFan": "uFan""#;

fn as_input_source(bytes: &[u8]) -> Result<TrendSources, oce_verify::SourceError> {
    let spec = trend_spec_with(
        None,
        &mapping(""),
        &format!(
            r#""inputs": [{{"path": "reference_input.csv", "format": "csv",
                "timeColumn": "time", "columns": {{{COLUMNS}}}}}]"#
        ),
    );
    read_trends(&spec, &[("reference_input.csv", bytes)])
}

fn as_controller_output(
    bytes: &[u8],
    device_prefix: &str,
) -> Result<TrendSources, oce_verify::SourceError> {
    let spec = loaded(&format!(
        r#"{{"references": [{{"model": "M", "sequence": "s", "pointNameMapping": {},
            "controllerOutput": "controller_output.csv"}}]}}"#,
        mapping(device_prefix)
    ));
    read_trends(&spec, &[("controller_output.csv", bytes)])
}

/// Each merged input column by name: its values as bits, a Boolean as 0 or 1.
fn merged(read: &TrendSources) -> Vec<(String, Vec<u64>)> {
    read.inputs
        .columns()
        .iter()
        .map(|column| {
            let bits = match column.values() {
                Column::Real(values) => values.iter().map(|x| x.to_bits()).collect(),
                Column::Boolean { values, valid } => {
                    assert!(valid.as_ref().is_none_or(|v| v.iter().all(|&b| b)));
                    values.iter().map(|&b| u64::from(b)).collect()
                }
                other => panic!("{other:?}"),
            };
            (column.name().to_owned(), bits)
        })
        .collect()
}

fn bits(values: &[f64]) -> Vec<u64> {
    values.iter().map(|x| x.to_bits()).collect()
}

/// The values §12.7.3 prints, by point.
fn printed() -> Vec<(String, Vec<u64>)> {
    let mut columns = vec![
        ("TOut".to_owned(), bits(&[283.15; 5])),
        ("TZon".to_owned(), bits(&[292.15; 5])),
        ("TZonSet".to_owned(), bits(&[293.15; 5])),
        ("uCoo".to_owned(), bits(&[0.0, 0.0, 0.5, 1.0, 1.0])),
        ("uFan".to_owned(), vec![1; 5]),
        ("uHea".to_owned(), bits(&[1.0, 0.5, 0.0, 0.0, 0.0])),
    ];
    columns.sort();
    columns
}

#[test]
fn obc_12_7_3_reference_input_reads_as_an_input_source_as_printed() {
    let bytes = trend_file("obc_reference_input.csv");
    assert_eq!(bytes.len(), 300);
    assert!(bytes.starts_with(b"time, uHea, uCoo, TZonSet,    TZon,    TOut, uFan\n   0,"));
    let read = as_input_source(&bytes).expect("the printed file reads");
    assert_eq!(read.inputs.times(), [0.0, 60.0, 120.0, 180.0, 3600.0]);
    let mut columns = merged(&read);
    columns.sort();
    assert_eq!(columns, printed());
    assert_eq!(read.sources[0].rows, 5);
    // Its unpadded twin reads to the same merged inputs.
    let unpadded: Vec<u8> = bytes.iter().copied().filter(|&b| b != b' ').collect();
    let twin = as_input_source(&unpadded).expect("reads");
    assert_eq!(merged(&twin), merged(&read));
    assert_eq!(twin.inputs.times(), read.inputs.times());
}

#[test]
fn obc_12_7_3_reference_input_reads_as_a_controller_output_as_printed() {
    let read = as_controller_output(&trend_file("obc_reference_input.csv"), "")
        .expect("the printed file reads");
    let mut series: Vec<(String, Vec<u64>)> = read
        .expected
        .iter()
        .map(|series| {
            assert_eq!(series.times, [0.0, 60.0, 120.0, 180.0, 3600.0]);
            let values = match &series.values {
                SeriesValues::Real(values) => bits(values),
                SeriesValues::Boolean(values) => values.iter().map(|&b| u64::from(b)).collect(),
                other @ SeriesValues::Integer(_) => panic!("{other:?}"),
            };
            (series.name.clone(), values)
        })
        .collect();
    series.sort();
    assert_eq!(series, printed());
}

#[test]
fn obc_12_7_3_indicator_reads_as_an_indicator_source_as_printed() {
    let spec = trend_spec_with(
        None,
        &format!(
            "[{}]",
            point(("TSupCoo", None, "bool"), ("TSupCoo", None, "bool"))
        ),
        r#""indicatorSources": [{"path": "indicator.csv", "format": "csv", "timeColumn": "time",
            "columns": {"TSupCoo": "TSupCoo"}}]"#,
    );
    let bytes = trend_file("obc_indicator.csv");
    assert_eq!(bytes, b"time, TSupCoo\n0, 1\n120, 0\n600, 1\n");
    let read = read_trends(&spec, &[("indicator.csv", &bytes)]).expect("the printed file reads");
    assert_eq!(read.indicators.len(), 1);
    assert_eq!(read.indicators[0].name, "TSupCoo");
    assert_eq!(read.indicators[0].times, [0.0, 120.0, 600.0]);
    assert_eq!(
        read.indicators[0].values,
        SeriesValues::Boolean(vec![true, false, true])
    );
}

#[test]
fn a_mapping_that_writes_the_padding_names_no_header_any_more() {
    // Before issue #733 a mapping had to write ` uHea` to reach the printed header; the header
    // is trimmed now, so that spelling is the one that names nothing.
    let spec = trend_spec_with(
        None,
        &format!(
            "[{}]",
            point(("uHea", None, "float"), (" uHea", None, "float"))
        ),
        r#""inputs": [{"path": "f.csv", "format": "csv", "timeColumn": "time",
            "columns": {" uHea": " uHea"}}]"#,
    );
    let error = read_trends(&spec, &[("f.csv", b"time, uHea\n0, 1\n")]).expect_err("refused");
    assert_eq!(
        (error.code(), error.column()),
        ("column-missing", Some(" uHea"))
    );
    let error =
        as_controller_output(&trend_file("obc_reference_input.csv"), " ").expect_err("refused");
    assert_eq!(
        (error.code(), error.column()),
        ("unmapped-point", Some("uHea"))
    );
}
