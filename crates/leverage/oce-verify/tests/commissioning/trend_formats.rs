//! One trend source at a time: what each format's cells and times become, and every cell and
//! time a source refuses, named by file, row and column.

use oce_replay::Column;
use oce_verify::{SeriesValues, TrendSources};

use super::{point, read_trends, trend_spec_with};

/// A mapping of a Real temperature (`degF` to `K`), a Boolean, an Integer, a whole-number Real,
/// a number read as a Boolean and a pressure (`kPa` to `Pa`), by device names `T`, `B`, `N`,
/// `W`, `F` and `P`.
fn mapping() -> String {
    format!(
        "[{}, {}, {}, {}, {}, {}]",
        point(("t", Some("K"), "float"), ("T", Some("degF"), "float")),
        point(("b", None, "bool"), ("B", None, "bool")),
        point(("n", None, "int"), ("N", None, "float")),
        point(("w", None, "float"), ("W", None, "int")),
        point(("f", None, "bool"), ("F", None, "float")),
        point(("p", Some("Pa"), "float"), ("P", Some("kPa"), "float")),
    )
}

/// Read one `role` source of `format` from `bytes`, with `extra` source members.
fn read_one(
    role: &str,
    format: &str,
    time: &str,
    columns: &str,
    bytes: &[u8],
    origin: Option<&str>,
) -> Result<TrendSources, oce_verify::SourceError> {
    let spec = trend_spec_with(
        origin,
        &mapping(),
        &format!(
            r#""{role}": [{{"path": "d/f", "format": "{format}", "timeColumn": {time},
                "columns": {{{columns}}}}}]"#
        ),
    );
    read_trends(&spec, &[("d/f", bytes)])
}

fn csv(columns: &str, text: &str) -> Result<TrendSources, oce_verify::SourceError> {
    read_one(
        "expected",
        "csv",
        r#""time""#,
        columns,
        text.as_bytes(),
        None,
    )
}

fn combi(columns: &str, text: &str) -> Result<TrendSources, oce_verify::SourceError> {
    read_one(
        "expected",
        "combiTimeTable",
        "1",
        columns,
        text.as_bytes(),
        None,
    )
}

/// Code, file, row and column of a refusal.
fn place(result: Result<TrendSources, oce_verify::SourceError>) -> (String, String, u64, String) {
    let error = result.expect_err("refused");
    (
        error.code().to_owned(),
        error.file().unwrap_or("-").to_owned(),
        error.row().unwrap_or(0),
        error.column().unwrap_or("-").to_owned(),
    )
}

fn refusal(code: &str, row: u64, column: &str) -> (String, String, u64, String) {
    (code.to_owned(), "d/f".to_owned(), row, column.to_owned())
}

fn reals(read: &TrendSources, index: usize) -> (Vec<f64>, Vec<u64>) {
    let series = &read.expected[index];
    let SeriesValues::Real(values) = &series.values else {
        panic!("{series:?}")
    };
    (
        series.times.clone(),
        values.iter().map(|x| x.to_bits()).collect(),
    )
}

#[test]
fn a_csv_temperature_is_converted_once_and_a_missing_cell_is_no_sample() {
    let read = csv(
        r#""T": "T""#,
        "time,T\n0,32\n5,\n10,NA\n15,NaN\n20,nan\n25,212\n",
    )
    .expect("reads");
    let (times, values) = reals(&read, 0);
    assert_eq!(times, [0.0, 25.0]);
    assert_eq!(values, [273.15_f64.to_bits(), 373.15_f64.to_bits()]);
    assert_eq!(read.sources[0].rows, 6);
    assert_eq!(read.expected[0].name, "t");
    assert_eq!(read.expected[0].device, "T");
}

#[test]
fn a_combi_time_table_reads_the_same_values_as_a_csv_of_the_same_numbers() {
    let from_csv = csv(r#""T": "T""#, "time,T\n0,50.5\n5,-40\n").expect("reads");
    let from_combi = combi(r#""2": "T""#, "#1\ndouble t(2,2)\n0 50.5\n5 -40\n").expect("reads");
    assert_eq!(reals(&from_csv, 0), reals(&from_combi, 0));
}

#[test]
fn an_infinity_is_refused_in_every_cell_and_time_of_both_formats() {
    assert_eq!(
        place(csv(r#""T": "T""#, "time,T\n0,1\n5,inf\n")),
        refusal("non-finite-number", 2, "T")
    );
    assert_eq!(
        place(csv(r#""T": "T""#, "time,T\n0,-infinity\n")),
        refusal("non-finite-number", 1, "T")
    );
    assert_eq!(
        place(csv(r#""T": "T""#, "time,T\ninf,1\n")),
        refusal("non-finite-number", 1, "time")
    );
    assert_eq!(
        place(combi(r#""2": "T""#, "#1\ndouble t(2,2)\n0 1\n5 1e999\n")).0,
        "non-finite-number"
    );
    // An input source refuses it too, before any frame.
    let input = read_one(
        "inputs",
        "csv",
        r#""time""#,
        r#""T": "T""#,
        b"time,T\n0,inf\n",
        None,
    );
    assert_eq!(place(input), refusal("non-finite-number", 1, "T"));
}

#[test]
fn a_half_in_a_boolean_column_is_refused_with_its_file_row_and_column() {
    // A numeric device column mapped to a CDL Boolean: the type rule refuses it.
    let error = combi(r#""2": "F""#, "#1\ndouble t(3,2)\n0 0\n5 1\n10 0.5\n").unwrap_err();
    assert_eq!(
        (
            error.code(),
            error.cause(),
            error.file(),
            error.row(),
            error.column()
        ),
        (
            "cell-refused",
            Some("not-boolean"),
            Some("d/f"),
            Some(3),
            Some("2")
        )
    );
    let error = csv(r#""F": "F""#, "time,F\n0,1\n5,0.5\n").unwrap_err();
    assert_eq!(
        (error.code(), error.cause()),
        ("cell-refused", Some("not-boolean"))
    );
    assert_eq!((error.row(), error.column()), (Some(2), Some("F")));
    // A `bool` device column in csv is read as text by the Boolean text rule, so `0.5` is no
    // Boolean spelling.
    let error = csv(r#""B": "B""#, "time,B\n0,0.5\n").unwrap_err();
    assert_eq!(
        (
            error.code(),
            error.cause(),
            error.file(),
            error.row(),
            error.column()
        ),
        (
            "cell-refused",
            Some("not-boolean-text"),
            Some("d/f"),
            Some(1),
            Some("B")
        )
    );
    // A combiTimeTable `bool` device column holds numbers: exactly 0 and 1 only.
    let error = combi(r#""2": "B""#, "#1\ndouble t(1,2)\n0 0.5\n").unwrap_err();
    assert_eq!(
        (error.code(), error.cause()),
        ("cell-refused", Some("not-boolean"))
    );
}

#[test]
fn a_csv_boolean_reads_the_type_table_s_six_spellings_and_refuses_every_other_text() {
    // The type table's "CSV text → bool" row: `true`/`false`, `active`/`inactive` (BACnet binary
    // present value), `1`/`0`; replay hands the cell over as text (issue #728), its padding
    // trimmed (issue #733), and a missing cell is no sample.
    let read = csv(
        r#""B": "B""#,
        "time,B\n0,true\n1,false\n2,1\n3,0\n4,\n5,active\n6,inactive\n7, active\t\n8,NA\n9,\" 1\"\n",
    );
    let error = read.expect_err("a quoted padded spelling is refused");
    assert_eq!(
        (error.code(), error.cause(), error.row()),
        ("cell-refused", Some("not-boolean-text"), Some(10))
    );
    let read = csv(
        r#""B": "B""#,
        "time,B\n0,true\n1,false\n2,1\n3,0\n4,\n5,active\n6,inactive\n7, active\t\n8,NA\n",
    )
    .expect("reads");
    assert_eq!(read.expected[0].times, [0.0, 1.0, 2.0, 3.0, 5.0, 6.0, 7.0]);
    assert_eq!(
        read.expected[0].values,
        SeriesValues::Boolean(vec![true, false, true, false, true, false, true])
    );
    // Byte for byte otherwise: no case folding, no number grammar, no other word.
    for word in [
        "True", "TRUE", "ACTIVE", "Inactive", "1.0", "+1", "yes", "on", "nan",
    ] {
        let error = csv(r#""B": "B""#, &format!("time,B\n0,{word}\n")).unwrap_err();
        assert_eq!(
            (error.code(), error.cause(), error.row(), error.column()),
            ("cell-refused", Some("not-boolean-text"), Some(1), Some("B")),
            "{word}"
        );
    }
    // A `bool` device column mapped to a CDL Integer reads the same text, as 0 and 1.
    let spec = trend_spec_with(
        None,
        &format!("[{}]", point(("occ", None, "int"), ("Occ", None, "bool"))),
        r#""expected": [{"path": "d/f", "format": "csv", "timeColumn": "time",
            "columns": {"Occ": "Occ"}}]"#,
    );
    let read =
        super::read_trends(&spec, &[("d/f", b"time,Occ\n0,active\n1,inactive\n")]).expect("reads");
    assert_eq!(read.expected[0].values, SeriesValues::Integer(vec![1, 0]));
}

#[test]
fn of_several_faulty_cells_in_a_row_a_real_cell_replay_cannot_parse_is_reported_first() {
    // Replay parses a row's Real columns before this crate reads its `bool` text cells, and this
    // crate then converts the columns in plan order (here the header names' order, declared in
    // it too). A `bool` cell no longer comes first because replay parsed it: issue #728 moved it
    // to the text side. Header `A` or `C` reads the Real device point `T`.
    let shape = |columns: &str, text: &str| {
        let error = csv(columns, text).unwrap_err();
        (error.code().to_owned(), error.column().map(str::to_owned))
    };
    let at = |code: &str, column: &str| (code.to_owned(), Some(column.to_owned()));
    // A Real cell that does not parse, after a bad `bool` cell: the Real column.
    assert_eq!(
        shape(r#""B": "B", "C": "T""#, "time,B,C\n0,yes,x\n"),
        at("bad-cell", "C")
    );
    // A Real cell refused after the parse, before a bad `bool` cell: the Real column.
    assert_eq!(
        shape(r#""A": "T", "B": "B""#, "time,A,B\n0,inf,yes\n"),
        at("non-finite-number", "A")
    );
    // A bad `bool` cell before a Real cell refused after the parse: the `bool` column.
    assert_eq!(
        shape(r#""B": "B", "C": "T""#, "time,B,C\n0,yes,inf\n"),
        at("cell-refused", "B")
    );
}

#[test]
fn integer_cells_follow_the_type_rules_exactly_once() {
    let read = csv(
        r#""N": "N", "W": "W""#,
        "time,N,W\n0,4.0,7\n1,-2147483648,-0\n",
    )
    .expect("reads");
    let n = read.expected.iter().find(|s| s.name == "n").expect("n");
    assert_eq!(n.values, SeriesValues::Integer(vec![4, i32::MIN]));
    let w = read.expected.iter().find(|s| s.name == "w").expect("w");
    assert_eq!(w.values, SeriesValues::Real(vec![7.0, 0.0]));
    let SeriesValues::Real(w) = &w.values else {
        unreachable!()
    };
    assert_eq!(
        w[1].to_bits(),
        0,
        "-0 read as a whole number is the integer zero"
    );
    let error = csv(r#""N": "N""#, "time,N\n0,4.5\n").unwrap_err();
    assert_eq!(
        (error.code(), error.cause()),
        ("cell-refused", Some("not-integral"))
    );
    let error = csv(r#""N": "N""#, "time,N\n0,2147483648\n").unwrap_err();
    assert_eq!(
        (error.code(), error.cause()),
        ("cell-refused", Some("outside-int32"))
    );
    let error = csv(r#""W": "W""#, "time,W\n0,0.5\n").unwrap_err();
    assert_eq!(
        (error.code(), error.cause()),
        ("cell-refused", Some("not-integral"))
    );
}

#[test]
fn a_reading_whose_conversion_overflows_is_refused_at_its_cell() {
    let error = csv(r#""P": "P""#, "time,P\n0,1\n1,1e306\n").unwrap_err();
    assert_eq!(
        (error.code(), error.cause(), error.row(), error.column()),
        ("unit-refused", Some("overflow"), Some(2), Some("P"))
    );
}

#[test]
fn a_decreasing_time_is_refused_and_equal_times_are_kept() {
    assert_eq!(
        place(csv(r#""T": "T""#, "time,T\n0,1\n5,1\n4.999,1\n")),
        refusal("time-regression", 3, "time")
    );
    assert_eq!(
        place(combi(r#""2": "T""#, "#1\ndouble t(3,2)\n0 1\n5 1\n4 1\n")),
        refusal("time-regression", 3, "1")
    );
    let read = csv(r#""T": "T""#, "time,T\n0,1\n0,2\n-0,3\n1,4\n").expect("reads");
    let (times, _) = reals(&read, 0);
    assert_eq!(times, [0.0, 0.0, 0.0, 1.0]);
    assert!(
        times.iter().all(|t| t.to_bits() != (-0.0_f64).to_bits()),
        "-0 is read as 0"
    );
}

#[test]
fn a_time_of_instants_and_a_time_of_seconds_give_the_same_model_seconds() {
    let origin = Some("2018-06-07T00:00:00-07:00");
    let instants = read_one(
        "expected",
        "csv",
        r#""stamp""#,
        r#""T": "T""#,
        b"stamp,T\n2018-06-07T06:59:58.5Z,1\n2018-06-07T00:00:00-07:00,2\n2018-06-07T09:00:05.25+02:00,3\n2018-06-07T07:01:00Z,4\n",
        origin,
    )
    .expect("reads");
    let seconds = read_one(
        "expected",
        "csv",
        r#""stamp""#,
        r#""T": "T""#,
        b"stamp,T\n-1.5,1\n0,2\n5.25,3\n60,4\n",
        origin,
    )
    .expect("reads");
    assert_eq!(reals(&instants, 0), reals(&seconds, 0));
    assert_eq!(reals(&instants, 0).0, [-1.5, 0.0, 5.25, 60.0]);
}

#[test]
fn the_first_row_fixes_the_time_kind_and_instants_need_an_origin() {
    let origin = Some("2018-06-07T00:00:00Z");
    let read = |text: &str, origin| {
        read_one(
            "expected",
            "csv",
            r#""time""#,
            r#""T": "T""#,
            text.as_bytes(),
            origin,
        )
    };
    assert_eq!(
        place(read("time,T\n0,1\n2018-06-07T00:00:01Z,1\n", origin)),
        refusal("bad-time", 2, "time")
    );
    assert_eq!(
        place(read("time,T\n2018-06-07T00:00:01Z,1\n2,1\n", origin)),
        refusal("bad-time", 2, "time")
    );
    assert_eq!(
        place(read("time,T\n2018-06-07T00:00:01Z,1\n", None)),
        refusal("time-origin-required", 1, "time")
    );
    // The time is read before the cells, so a bad cell beside an instant still asks for the
    // origin first.
    assert_eq!(
        place(read("time,T\n2018-06-07T00:00:01Z,x\n", None)),
        refusal("time-origin-required", 1, "time")
    );
    // Neither kind: refused as a bad time whether or not the spec has an origin.
    for origin in [None, origin] {
        assert_eq!(
            place(read("time,T\nyesterday,1\n", origin)),
            refusal("bad-time", 1, "time")
        );
        assert_eq!(
            place(read("time,T\n2018-06-07 00:00:01Z,1\n", origin)),
            refusal("bad-time", 1, "time")
        );
    }
    assert_eq!(
        place(read("time,T\n,1\n", None)),
        refusal("time-missing", 1, "time")
    );
    assert_eq!(
        place(read("time,T\nNA,1\n", None)),
        refusal("time-missing", 1, "time")
    );
}

#[test]
fn a_column_the_map_names_that_the_file_lacks_is_refused() {
    let error = csv(r#""T": "T""#, "time,U\n0,1\n").unwrap_err();
    assert_eq!(
        (error.code(), error.column()),
        ("column-missing", Some("T"))
    );
    let error = csv(r#""T": "T""#, "stamp,T\n0,1\n").unwrap_err();
    assert_eq!(
        (error.code(), error.column()),
        ("column-missing", Some("time"))
    );
    let error = combi(r#""3": "T""#, "#1\ndouble t(1,2)\n0 1\n").unwrap_err();
    assert_eq!(
        (error.code(), error.column()),
        ("column-missing", Some("3"))
    );
    let error = csv(r#""T": "T""#, "").unwrap_err();
    assert_eq!(error.code(), "column-missing");
    // A `bool` column, read as text, is missed before the time column's kind is asked for.
    let error = csv(r#""B": "B""#, "time,T\n2018-06-07T00:00:01Z,1\n").unwrap_err();
    assert_eq!(
        (error.code(), error.column()),
        ("column-missing", Some("B"))
    );
}

#[test]
fn a_csv_file_is_held_to_its_header() {
    assert_eq!(
        place(csv(r#""T": "T""#, "time,T,T\n0,1,2\n")).0,
        "duplicate-column"
    );
    assert_eq!(
        place(csv(r#""T": "T""#, "time,T\n0,1,2\n")),
        refusal("ragged-row", 1, "-")
    );
    assert_eq!(
        place(csv(r#""T": "T""#, "time,T\n0\n")),
        refusal("ragged-row", 1, "-")
    );
    let error = csv(r#""T": "T""#, "time,T\n0,\u{0}\u{ff}\n");
    assert_eq!(place(error).0, "bad-cell");
    let not_utf8 = read_one(
        "expected",
        "csv",
        r#""time""#,
        r#""T": "T""#,
        b"time,T\n0,\xff\n",
        None,
    );
    assert_eq!(place(not_utf8), refusal("not-utf8", 1, "#2"));
    // Unmapped columns are ignored, whatever they hold.
    let read = csv(r#""T": "T""#, "time,T,note\n0,1,anything at all\n").expect("reads");
    assert_eq!(read.expected.len(), 1);
}

#[test]
fn the_merged_inputs_hold_missing_cells_as_nan_and_cleared_flags() {
    let read = read_one(
        "inputs",
        "csv",
        r#""time""#,
        r#""T": "T", "B": "B", "N": "N""#,
        b"time,T,B,N\n0,,,\n1,32,true,3\n",
        None,
    )
    .expect("reads");
    let columns = read.inputs.columns();
    let names: Vec<&str> = columns.iter().map(|c| c.name()).collect();
    assert_eq!(names, ["b", "n", "t"]);
    match columns[0].values() {
        Column::Boolean { values, valid } => {
            assert_eq!(values, &[false, true]);
            assert_eq!(valid.as_deref(), Some(&[false, true][..]));
        }
        other => panic!("{other:?}"),
    }
    match columns[1].values() {
        Column::Integer { values, valid } => {
            assert_eq!(values, &[0, 3]);
            assert_eq!(valid.as_deref(), Some(&[false, true][..]));
        }
        other => panic!("{other:?}"),
    }
    match columns[2].values() {
        Column::Real(values) => {
            assert!(values[0].is_nan());
            assert_eq!(values[1].to_bits(), 273.15_f64.to_bits());
        }
        other => panic!("{other:?}"),
    }
    assert!(
        read.inputs.overrides().is_empty(),
        "no hold: the run-level Refuse stands"
    );
    assert!(read.expected.is_empty() && read.indicators.is_empty());
}

#[test]
fn an_indicator_series_keeps_its_mapped_name_unresolved() {
    let spec = trend_spec_with(
        None,
        &format!(
            "[{}]",
            point(("fanSta.y", None, "bool"), ("Fan Status", None, "bool"))
        ),
        r#""indicatorSources": [{"path": "ind.csv", "format": "csv", "timeColumn": "time",
            "columns": {"Fan Status": "Fan Status"}}]"#,
    );
    let read =
        super::read_trends(&spec, &[("ind.csv", b"time,Fan Status\n0,1\n60,0\n")]).expect("reads");
    assert_eq!(read.indicators.len(), 1);
    assert_eq!(read.indicators[0].name, "fanSta.y");
    assert_eq!(read.indicators[0].times, [0.0, 60.0]);
    assert_eq!(read.sources[0].role.as_str(), "indicators");
    assert!(read.unit_columns.is_empty());
}
