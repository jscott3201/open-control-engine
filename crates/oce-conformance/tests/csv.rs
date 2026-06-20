//! Edge, golden, typed-error, and byte-stability coverage for CombiTimeTable CSV I/O.

use std::fs;

use oce_conformance::{CombiTimeTable, CsvError, ValueKind, format_f64};
use oce_model::{EnumClassId, Value, ValueType};

#[test]
fn reads_all_separators_comments_and_column_names() {
    let text = "\
#1
# arbitrary comment
# columns: time real flag count
double mixed(4,4)
0 1 0 1
1\t2\t1\t2
2,3,0,3
3;4;1;4
";
    let table = CombiTimeTable::parse(text).expect("mixed separators parse");
    assert_eq!(table.name, "mixed");
    assert_eq!(table.n_rows, 4);
    assert_eq!(table.n_cols, 4);
    assert_eq!(
        table.col_names,
        Some(vec![
            "time".into(),
            "real".into(),
            "flag".into(),
            "count".into()
        ])
    );
    assert_eq!(table.data[0], 0.0);
    assert_eq!(table.data[15], 4.0);
}

#[test]
fn golden_write_read_write_is_byte_identical() {
    let golden = include_str!("fixtures/golden/trace.combi.csv");
    let table = CombiTimeTable::parse(golden).expect("golden parses");
    let once = table.to_csv_string().expect("golden writes");
    assert_eq!(once, golden);
    let twice = CombiTimeTable::parse(&once)
        .expect("writer output parses")
        .to_csv_string()
        .expect("writer output rewrites");
    assert_eq!(twice, golden);
}

#[test]
fn path_read_and_write_round_trip() {
    let table = CombiTimeTable::parse(include_str!("fixtures/golden/trace.combi.csv"))
        .expect("parse golden");
    let path = std::env::temp_dir().join(format!(
        "oce-conformance-csv-roundtrip-{}.csv",
        std::process::id()
    ));
    table.write(&path).expect("write temp table");
    let reread = CombiTimeTable::read(&path).expect("read temp table");
    fs::remove_file(&path).ok();
    assert_eq!(
        reread.to_csv_string().unwrap(),
        include_str!("fixtures/golden/trace.combi.csv")
    );
}

#[test]
fn shape_mismatch_is_typed() {
    let err = CombiTimeTable::parse(
        "\
#1
double bad(2,3)
0 1 2
",
    )
    .expect_err("declared two rows but only one present");
    assert!(matches!(
        err,
        CsvError::ShapeMismatch {
            declared: (2, 3),
            actual: (1, 3)
        }
    ));
}

#[test]
fn row_width_mismatch_is_typed() {
    let err = CombiTimeTable::parse(
        "\
#1
double bad(1,3)
0 1
",
    )
    .expect_err("row has too few columns");
    assert!(matches!(
        err,
        CsvError::ShapeMismatch {
            declared: (1, 3),
            actual: (1, 2)
        }
    ));
}

#[test]
fn later_row_width_mismatch_reports_row_index_not_flat_cell_count() {
    let err = CombiTimeTable::parse(
        "\
#1
double bad(3,3)
0 1 2
1 3
2 5 6
",
    )
    .expect_err("second row has too few columns");
    assert!(matches!(
        err,
        CsvError::ShapeMismatch {
            declared: (3, 3),
            actual: (2, 2)
        }
    ));
}

#[test]
fn non_monotonic_time_is_typed() {
    let err = CombiTimeTable::parse(
        "\
#1
double bad(2,2)
1 0
0 0
",
    )
    .expect_err("time decreases");
    assert!(matches!(err, CsvError::NonMonotonicTime { row: 1 }));
}

#[test]
fn invalid_data_cell_is_typed_not_a_bad_header() {
    let err = CombiTimeTable::parse(
        "\
#1
double bad(1,2)
0 nope
",
    )
    .expect_err("data cell is not numeric");
    assert!(matches!(
        err,
        CsvError::InvalidCell {
            line: 3,
            ref text
        } if text == "nope"
    ));
}

#[test]
fn inline_data_comment_is_rejected_as_invalid_cell() {
    let err = CombiTimeTable::parse(
        "\
#1
double bad(1,2)
0 1 # inline comments are not data-row syntax
",
    )
    .expect_err("inline comment on data row fails closed");
    assert!(matches!(
        err,
        CsvError::InvalidCell {
            line: 3,
            ref text
        } if text == "0 1 # inline comments are not data-row syntax"
    ));
}

#[test]
fn non_finite_data_cells_parse_and_round_trip_by_ieee_class() {
    let text = "\
#1
# columns: time pos neg nan
double nonfinite(1,4)
0.0 inf -inf NaN
";
    let table = CombiTimeTable::parse(text).expect("non-finite data cells parse");
    assert_eq!(table.data[1].to_bits(), f64::INFINITY.to_bits());
    assert_eq!(table.data[2].to_bits(), f64::NEG_INFINITY.to_bits());
    assert!(table.data[3].is_nan());
    let serialized = table.to_csv_string().expect("non-finite data writes");
    assert_eq!(serialized, text);
    let reparsed = CombiTimeTable::parse(&serialized).expect("non-finite output reparses");
    assert_eq!(reparsed.data[1].to_bits(), f64::INFINITY.to_bits());
    assert_eq!(reparsed.data[2].to_bits(), f64::NEG_INFINITY.to_bits());
    assert!(reparsed.data[3].is_nan());
}

#[test]
fn non_finite_time_cells_are_typed_errors() {
    let err = CombiTimeTable::parse(
        "\
#1
double bad(1,2)
inf 0
",
    )
    .expect_err("infinite timestamp is rejected");
    assert!(matches!(
        err,
        CsvError::NonFinite {
            line: 3,
            column: 0,
            ref text
        } if text == "inf"
    ));

    let err = CombiTimeTable::parse(
        "\
#1
double bad(1,2)
NaN 0
",
    )
    .expect_err("NaN timestamp is rejected");
    assert!(matches!(
        err,
        CsvError::NonFinite {
            line: 3,
            column: 0,
            ref text
        } if text == "NaN"
    ));
}

#[test]
fn writer_accepts_non_finite_in_memory_data_cells_but_rejects_time() {
    let table = CombiTimeTable {
        name: "nonfinite".into(),
        n_rows: 1,
        n_cols: 2,
        data: vec![0.0, f64::INFINITY],
        col_names: None,
    };
    assert_eq!(
        table.to_csv_string().expect("writer accepts data infinity"),
        "#1\ndouble nonfinite(1,2)\n0.0 inf\n"
    );
    assert_eq!(
        ValueKind::Real
            .encode_value(&Value::Real(f64::NEG_INFINITY))
            .expect("Real encoder preserves -inf")
            .to_bits(),
        f64::NEG_INFINITY.to_bits()
    );
    assert!(
        ValueKind::Real
            .encode_value(&Value::Real(f64::NAN))
            .expect("Real encoder preserves NaN class")
            .is_nan()
    );

    let table = CombiTimeTable {
        name: "bad".into(),
        n_rows: 1,
        n_cols: 2,
        data: vec![f64::INFINITY, 1.0],
        col_names: None,
    };
    assert!(matches!(
        table
            .to_csv_string()
            .expect_err("writer rejects non-finite time"),
        CsvError::NonFinite { column: 0, .. }
    ));
}

#[test]
fn empty_file_and_bad_header_are_typed() {
    assert!(matches!(
        CombiTimeTable::parse("").expect_err("empty file fails"),
        CsvError::BadHeader(_)
    ));
    assert!(matches!(
        CombiTimeTable::parse("double missing(1,2)\n0 1\n").expect_err("missing #1 fails"),
        CsvError::BadHeader(_)
    ));
}

#[test]
fn column_owned_returns_series_values() {
    let table = CombiTimeTable::parse(include_str!("fixtures/golden/trace.combi.csv")).unwrap();
    let col = table.column_owned(2).expect("flag column");
    assert_eq!(col.x, vec![0.0, 1.0, 1.5]);
    assert_eq!(col.y, vec![0.0, 1.0, 1.0]);
    assert_eq!(col.as_series().len(), 3);
}

#[test]
fn value_kind_encodes_bool_int_and_real_with_exact_integer_guard() {
    assert_eq!(
        ValueKind::from_value_type(ValueType::Boolean).unwrap(),
        ValueKind::Boolean
    );
    assert_eq!(
        ValueKind::Boolean
            .encode_value(&Value::Boolean(true))
            .unwrap(),
        1.0
    );
    assert_eq!(
        ValueKind::Boolean
            .encode_value(&Value::Boolean(false))
            .unwrap(),
        0.0
    );
    assert_eq!(
        ValueKind::Integer
            .encode_value(&Value::Integer(9_007_199_254_740_992))
            .unwrap(),
        9_007_199_254_740_992.0
    );
    assert!(matches!(
        ValueKind::Integer.encode_value(&Value::Integer(9_007_199_254_740_993)),
        Err(CsvError::IntegerNotExactlyRepresentable(
            9_007_199_254_740_993
        ))
    ));
    assert_eq!(
        ValueKind::Real
            .encode_value(&Value::Real(-0.0))
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits()
    );
    assert!(matches!(
        ValueKind::from_value_type(ValueType::String),
        Err(CsvError::UnsupportedValueType(ValueType::String))
    ));
    assert!(matches!(
        ValueKind::from_value_type(ValueType::Enum(EnumClassId(1))),
        Err(CsvError::UnsupportedValueType(ValueType::Enum(
            EnumClassId(1)
        )))
    ));
}

#[test]
fn ryu_formatting_is_shortest_round_trip_and_byte_stable() {
    let values = [0.0, -0.0, 1.2345678901234567, f64::MIN_POSITIVE, 1.0e300];
    let once: Vec<_> = values.into_iter().map(format_f64).collect();
    let twice: Vec<_> = values.into_iter().map(format_f64).collect();
    assert_eq!(once, twice);
    for (raw, formatted) in values.into_iter().zip(once) {
        let parsed = formatted.parse::<f64>().expect("ryu output parses");
        assert_eq!(
            parsed.to_bits(),
            raw.to_bits(),
            "round-trip mismatch for {formatted}"
        );
    }
}
