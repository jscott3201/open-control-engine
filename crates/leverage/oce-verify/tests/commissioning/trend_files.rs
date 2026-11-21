//! The files a run is handed against the paths a reference names, the mapping, and the order in
//! which a refusal that needs no row is found before any row is read.

use oce_verify::{SOURCE_ERROR_CODES, SourceRole, SpecInputs, TrendLimits, TrendSources};

use super::{loaded, point, read_trends, trend_spec_with};

fn temperature() -> String {
    format!(
        "[{}]",
        point(("t", Some("K"), "float"), ("T", Some("degF"), "float"))
    )
}

/// A spec whose reference reads `a.csv` as inputs and expected, maps through `m.json` and reads
/// `i.csv` as an indicator source.
fn three_roles() -> oce_verify::CommissioningSpec {
    let source = |path: &str| {
        format!(
            r#"{{"path": "{path}", "format": "csv", "timeColumn": "time", "columns": {{"T": "T"}}}}"#
        )
    };
    loaded(&format!(
        r#"{{"references": [{{"model": "M", "sequence": "s", "pointNameMapping": "m.json",
            "inputs": [{}], "expected": [{}], "indicatorSources": [{}]}}]}}"#,
        source("a.csv"),
        source("a.csv"),
        source("i.csv")
    ))
}

const CSV: &[u8] = b"time,T\n0,32\n";

#[test]
fn one_file_may_serve_several_roles_and_is_handed_in_once() {
    let mapping = temperature();
    let read = read_trends(
        &three_roles(),
        &[
            ("i.csv", CSV),
            ("m.json", mapping.as_bytes()),
            ("a.csv", CSV),
        ],
    )
    .expect("reads");
    let roles: Vec<(SourceRole, &str)> = read
        .sources
        .iter()
        .map(|s| (s.role, s.path.as_str()))
        .collect();
    assert_eq!(
        roles,
        [
            (SourceRole::Inputs, "a.csv"),
            (SourceRole::Expected, "a.csv"),
            (SourceRole::Indicators, "i.csv")
        ]
    );
    assert_eq!(read.sources[0].digest, read.sources[1].digest);
    let record = read.mapping.expect("a mapping file is recorded");
    assert_eq!(record.path, "m.json");
    assert!(record.digest.starts_with("fnv1a128:") && record.digest.len() == 41);
    // One row per point, though three sources read it.
    assert_eq!(read.unit_columns.len(), 1);
    assert_eq!(read.unit_columns[0].from, "degF");
    // The same point in two roles: an input column, an expected and an indicator series.
    assert_eq!(read.inputs.columns().len(), 1);
    assert_eq!((read.expected.len(), read.indicators.len()), (1, 1));
}

#[test]
fn a_named_file_with_no_bytes_an_unnamed_file_and_a_repeated_file_are_refused() {
    let mapping = temperature();
    let m = mapping.as_bytes();
    let spec = three_roles();
    let refused = |files: &[(&str, &[u8])]| {
        let error = read_trends(&spec, files).expect_err("refused");
        (error.code(), error.file().map(str::to_owned))
    };
    assert_eq!(
        refused(&[("a.csv", CSV), ("i.csv", CSV)]),
        ("file-missing", Some("m.json".to_owned()))
    );
    assert_eq!(
        refused(&[("m.json", m), ("a.csv", CSV)]),
        ("file-missing", Some("i.csv".to_owned()))
    );
    assert_eq!(
        refused(&[
            ("m.json", m),
            ("a.csv", CSV),
            ("i.csv", CSV),
            ("b.csv", CSV)
        ]),
        ("file-unlisted", Some("b.csv".to_owned()))
    );
    assert_eq!(
        refused(&[
            ("m.json", m),
            ("a.csv", CSV),
            ("a.csv", CSV),
            ("i.csv", CSV)
        ]),
        ("duplicate-file", Some("a.csv".to_owned()))
    );
    // Paths compare byte for byte, as the spec writes them.
    assert_eq!(
        refused(&[("m.json", m), ("./a.csv", CSV), ("i.csv", CSV)]),
        ("file-unlisted", Some("./a.csv".to_owned()))
    );
    // The CXF is `cxf`, never a file entry.
    let only_mapping = loaded(
        r#"{"references": [{"model": "M", "sequence": "s", "cxf": "c.json",
            "pointNameMapping": "m.json"}]}"#,
    );
    let mut inputs = SpecInputs {
        cxf: b"{}".to_vec(),
        files: vec![("m.json".to_owned(), mapping.clone().into_bytes())],
    };
    assert!(TrendSources::read(&only_mapping, 0, &inputs, &TrendLimits::default()).is_ok());
    inputs.files.push(("c.json".to_owned(), b"{}".to_vec()));
    let error = TrendSources::read(&only_mapping, 0, &inputs, &TrendLimits::default())
        .expect_err("refused");
    assert_eq!(error.code(), "file-unlisted");
}

#[test]
fn a_reference_out_of_range_or_with_sources_and_no_mapping_is_refused() {
    let spec = loaded(r#"{"references": []}"#);
    let error = TrendSources::read(&spec, 0, &SpecInputs::default(), &TrendLimits::default())
        .expect_err("refused");
    assert_eq!(error.code(), "reference-out-of-range");
    let spec = loaded(
        r#"{"references": [{"model": "M", "sequence": "s", "inputs": [
            {"path": "a.csv", "format": "csv", "timeColumn": "time", "columns": {}}]}]}"#,
    );
    let error = read_trends(&spec, &[("a.csv", CSV)]).expect_err("refused");
    assert_eq!(error.code(), "mapping-required");
    // A reference with no source needs no mapping and reads nothing.
    let bare = loaded(r#"{"references": [{"model": "M", "sequence": "s"}]}"#);
    let read = read_trends(&bare, &[]).expect("reads");
    assert!(read.sources.is_empty() && read.inputs.times().is_empty());
    assert!(read.mapping.is_none());
}

#[test]
fn a_mapping_file_that_does_not_load_carries_its_own_code() {
    let error = read_trends(
        &three_roles(),
        &[("m.json", b"[1,]"), ("a.csv", CSV), ("i.csv", CSV)],
    )
    .expect_err("refused");
    assert_eq!(
        (error.code(), error.cause(), error.file()),
        ("mapping-refused", Some("json-syntax"), Some("m.json"))
    );
    let duplicate = format!(
        "[{}, {}]",
        point(("t", None, "float"), ("T", None, "float")),
        point(("u", None, "float"), ("T", None, "float"))
    );
    let error = read_trends(
        &three_roles(),
        &[
            ("m.json", duplicate.as_bytes()),
            ("a.csv", CSV),
            ("i.csv", CSV),
        ],
    )
    .expect_err("refused");
    assert_eq!(error.cause(), Some("duplicate-point"));
}

#[test]
fn a_unit_or_type_refusal_comes_before_any_row_is_read() {
    // The file is not even a csv; the unknown unit pair is found first, from the spec alone.
    let mapping = format!(
        "[{}]",
        point(("t", Some("K"), "float"), ("T", Some("degR"), "float"))
    );
    let spec = trend_spec_with(
        None,
        &mapping,
        r#""inputs": [{"path": "a.csv", "format": "csv", "timeColumn": "time",
            "columns": {"T": "T"}}]"#,
    );
    let error = read_trends(&spec, &[("a.csv", b"\xff\x00 garbage")]).expect_err("refused");
    assert_eq!(
        (error.code(), error.cause()),
        ("unit-refused", Some("unknown-pair"))
    );
    // A later source's unit refusal is found before an earlier source's bad row.
    let mapping = format!(
        "[{}, {}]",
        point(("t", None, "float"), ("T", None, "float")),
        point(("u", Some("K"), "float"), ("U", None, "float"))
    );
    let spec = trend_spec_with(
        None,
        &mapping,
        r#""inputs": [{"path": "a.csv", "format": "csv", "timeColumn": "time",
               "columns": {"T": "T"}}],
           "expected": [{"path": "b.csv", "format": "csv", "timeColumn": "time",
               "columns": {"U": "U"}}]"#,
    );
    let error =
        read_trends(&spec, &[("a.csv", b"time,T\n0,x\n"), ("b.csv", CSV)]).expect_err("refused");
    assert_eq!(
        (error.code(), error.cause()),
        ("unit-refused", Some("one-sided"))
    );
    // A column naming a point the mapping lacks: a mapped device name no file supplies is not
    // an error here (bind refuses an unmapped input), a column naming an unmapped one is.
    let spec = trend_spec_with(
        None,
        &temperature(),
        r#""inputs": [{"path": "a.csv", "format": "csv", "timeColumn": "time",
            "columns": {"T": "Missing Point"}}]"#,
    );
    let error = read_trends(&spec, &[("a.csv", b"\xff")]).expect_err("refused");
    assert_eq!(
        (error.code(), error.column()),
        ("unmapped-point", Some("T"))
    );
}

#[test]
fn two_columns_of_one_source_naming_one_device_point_are_refused() {
    let spec = trend_spec_with(
        None,
        &temperature(),
        r#""inputs": [{"path": "a.mos", "format": "combiTimeTable", "timeColumn": 1,
            "columns": {"2": "T", "3": "T"}}]"#,
    );
    let error =
        read_trends(&spec, &[("a.mos", b"#1\ndouble t(1,3)\n0 1 1\n")]).expect_err("refused");
    assert_eq!(
        (error.code(), error.column()),
        ("point-read-twice", Some("3"))
    );
}

#[test]
fn the_code_list_is_pinned() {
    assert_eq!(
        SOURCE_ERROR_CODES,
        [
            "reference-out-of-range",
            "too-many-sources",
            "duplicate-file",
            "file-unlisted",
            "file-missing",
            "mapping-required",
            "mapping-refused",
            "column-reference-invalid",
            "unmapped-point",
            "point-read-twice",
            "type-pair-refused",
            "unit-refused",
            "unit-on-discrete-point",
            "format-tag-missing",
            "table-syntax",
            "table-required",
            "table-not-found",
            "duplicate-table",
            "size-mismatch",
            "bad-number",
            "non-finite-number",
            "bad-cell",
            "cell-refused",
            "column-missing",
            "duplicate-column",
            "no-header",
            "no-point-columns",
            "ragged-row",
            "not-utf8",
            "cell-too-long",
            "too-many-rows",
            "too-many-columns",
            "too-many-bytes",
            "time-missing",
            "bad-time",
            "time-origin-required",
            "time-regression",
            "unreadable",
        ]
    );
}
