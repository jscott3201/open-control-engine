//! The source and window readers on their own cursor.

use super::*;

fn reader(text: &str) -> Reader<'_> {
    Reader::reporting(text)
}

#[test]
fn a_time_column_is_an_index_from_one_or_a_name() {
    assert_eq!(
        time_column(&mut reader("1")),
        Ok(SourceTimeColumn::Index(1))
    );
    assert_eq!(
        time_column(&mut reader("4294967295")),
        Ok(SourceTimeColumn::Index(u32::MAX))
    );
    assert_eq!(
        time_column(&mut reader("\"t\"")),
        Ok(SourceTimeColumn::Name("t".to_owned()))
    );
    for (written, kind, detail) in [
        (
            "0",
            SpecErrorKind::IntegerOutOfRange,
            "a column position is counted from 1 and is at most 4294967295",
        ),
        (
            "-1",
            SpecErrorKind::IntegerOutOfRange,
            "a column position is counted from 1 and is at most 4294967295",
        ),
        (
            "4294967296",
            SpecErrorKind::IntegerOutOfRange,
            "a column position is counted from 1 and is at most 4294967295",
        ),
        (
            "1.0",
            SpecErrorKind::WrongType,
            "expected the time column, a header name (a string) or a position counted from 1 (an integer), found a real number",
        ),
        (
            "null",
            SpecErrorKind::WrongType,
            "expected the time column, a header name (a string) or a position counted from 1 (an integer), found null",
        ),
        (
            "\"\"",
            SpecErrorKind::EmptyName,
            "the time column name must not be empty",
        ),
    ] {
        let refusal = time_column(&mut reader(written)).expect_err("refused");
        assert_eq!(
            (refusal.kind, refusal.detail.as_str()),
            (kind, detail),
            "{written}"
        );
    }
}

#[test]
fn a_window_is_two_ends_in_order_whichever_is_written_first() {
    let read = |text: &str| window(&mut reader(text));
    let forward = read(r#"{"start": 1, "stop": 2.5}"#).expect("read");
    assert_eq!(
        (forward.start, forward.stop),
        (Number::Integer(1), Number::Real(2.5))
    );
    assert_eq!(read(r#"{"stop": 2.5, "start": 1}"#), Ok(forward));
    assert_eq!((forward.start_s(), forward.stop_s()), (1.0, 2.5));
    // Equal ends are a window of one instant; the order is judged on the values, not the kinds.
    assert!(read(r#"{"start": 2, "stop": 2.0}"#).is_ok());
    assert!(read(r#"{"start": 0, "stop": -0.0}"#).is_ok());
    let refusal = read(r#" {"start": 2.0000000000000004, "stop": 2}"#).expect_err("out of order");
    assert_eq!(refusal.kind, SpecErrorKind::InvalidWindow);
    assert_eq!(
        (refusal.at.line, refusal.at.column),
        (1, 2),
        "the window, not an end"
    );
    // A repeated end is refused here too, without the strict pass before it.
    let refusal = read(r#"{"start": 0, "start": 0, "stop": 1}"#).expect_err("a repeat");
    assert_eq!(refusal.kind, SpecErrorKind::DuplicateKey);
    assert_eq!((refusal.at.line, refusal.at.column), (1, 14));
    // A start alone, a stop alone: each end is named when it is missing.
    assert_eq!(
        read(r#"{"start": 0}"#).expect_err("refused").detail,
        "missing key `stop`"
    );
    assert_eq!(
        read(r#"{"stop": 0}"#).expect_err("refused").detail,
        "missing key `start`"
    );
}

#[test]
fn a_hold_is_its_max_age_and_columns_are_sorted_by_their_bytes() {
    assert_eq!(
        hold(&mut reader(r#"{"maxAge": 10}"#)),
        Ok(Number::Integer(10))
    );
    assert_eq!(
        hold(&mut reader(r#"{"maxAge": 0.0}"#)),
        Ok(Number::Real(0.0))
    );
    let refusal = hold(&mut reader(r#"{"maxAge": -0.5}"#)).expect_err("negative");
    assert_eq!(refusal.kind, SpecErrorKind::InvalidHold);
    assert_eq!((refusal.at.line, refusal.at.column), (1, 12));
    let limits = SpecLimits::MAX;
    let read = columns(
        &mut reader(r#"{"b": "y", "10": "x", "B": "z", "2": "w"}"#),
        &limits,
    )
    .expect("read");
    let order: Vec<(&str, &str)> = read
        .iter()
        .map(|each| (each.column(), each.point()))
        .collect();
    assert_eq!(order, [("10", "x"), ("2", "w"), ("B", "z"), ("b", "y")]);
    // Two columns may hold one point's name here: what that means is the source reader's.
    assert!(columns(&mut reader(r#"{"a": "p", "b": "p"}"#), &limits).is_ok());
}

#[test]
fn a_source_list_is_read_in_file_order() {
    let limits = SpecLimits::MAX;
    let text = r#"[{"path": "b.csv", "format": "csv", "timeColumn": "t", "columns": {}},
                   {"path": "a.mos", "format": "combiTimeTable", "timeColumn": 2, "columns": {},
                    "table": "tab1", "hold": {"maxAge": 5}}]"#;
    let read = sources(&mut reader(text), "the inputs", &limits).expect("read");
    let paths: Vec<&str> = read.iter().map(TrendSource::path).collect();
    assert_eq!(paths, ["b.csv", "a.mos"], "file order, not sorted");
    assert_eq!(read[0].format(), TrendFormat::Csv);
    assert_eq!(read[1].format(), TrendFormat::CombiTimeTable);
    assert_eq!(
        (read[1].table(), read[1].hold_max_age_s()),
        (Some("tab1"), Some(5.0))
    );
    assert_eq!((read[0].table(), read[0].hold_max_age_s()), (None, None));
    let refusal = sources(&mut reader("{}"), "the inputs", &limits).expect_err("refused");
    assert_eq!(
        refusal.detail,
        "expected the inputs, a list, found an object"
    );
}
