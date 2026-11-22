//! A CSV column read as text (issue #728): `CsvFormat::text_columns` names it, `CsvSource`
//! resolves it once from the header and hands each row's cell over unparsed through
//! `CsvSource::text_cells`, held to the same caps, the same padding rule and the same missing
//! rule as every cell. No BACnet word, or any other spelling, means anything to the reader.

#[path = "support/csv_text.rs"]
mod csv_text;
#[path = "support/fixtures.rs"]
mod fixtures;

use allocation_counter::measure;
use csv_text::{empty_map, read_text, read_text_with, text, text_format};
use oce_replay::{
    CsvFormat, CsvSource, FrameSource, MissingPolicy, Replay, ReplayLimits, ReplayOptions,
    SourceError, TimeBase,
};

#[test]
fn a_text_column_hands_its_cells_over_unparsed() {
    let rows = read_text(
        b"t,state,other\n0,active,x\n1,inactive,x\n2,ON,x\n3,3.5e0,x\n4,\"a, \"\"b\"\"\",x\n",
        &["state"],
    )
    .unwrap();
    let cells: Vec<Option<String>> = rows.into_iter().flat_map(|(_, c)| c).collect();
    assert_eq!(
        cells,
        ["active", "inactive", "ON", "3.5e0", "a, \"b\""].map(text)
    );
}

#[test]
fn a_missing_text_cell_is_none_and_the_tokens_are_the_format_s() {
    let bytes = b"t,a\n0,\n1,NA\n2,NaN\n3, \t \n4,nan\n5,\"\"\n";
    let cells = |format: CsvFormat| -> Vec<Option<String>> {
        read_text_with(&bytes[..], format, &ReplayLimits::MAX)
            .unwrap()
            .into_iter()
            .flat_map(|(_, c)| c)
            .collect()
    };
    // `nan` is no token: only a Real cell that parses to NaN is missing for it.
    assert_eq!(
        cells(text_format("t", &["a"])),
        [None, None, None, None, text("nan"), None]
    );
    let no_tokens = CsvFormat {
        missing_tokens: Vec::new(),
        ..text_format("t", &["a"])
    };
    assert_eq!(
        cells(no_tokens),
        [None, text("NA"), text("NaN"), None, text("nan"), None]
    );
}

#[test]
fn text_cells_follow_the_format_order_and_may_repeat_or_name_the_time_or_a_mapped_column() {
    let rows = read_text(b"t,a,b\n0,1,2\n", &["b", "a", "b", "t"]).unwrap();
    assert_eq!(rows, [(0.0, ["2", "1", "2", "0"].map(text).to_vec())]);
    // A mapped column read as text too: the slot gets its parsed value, the text its spelling.
    let engine = fixtures::load("generic_time_suppression");
    let map = fixtures::path_map(&engine, &MissingPolicy::Refuse);
    let (tset, tzon) = (map.column(0).to_owned(), map.column(1).to_owned());
    let bytes = format!("t,{tset},{tzon}\n0, 295.150 ,1e2\n");
    let format = CsvFormat {
        text_columns: vec![tset],
        ..CsvFormat::seconds("t")
    };
    let mut source =
        CsvSource::new(&map, bytes.as_bytes(), format, &ReplayLimits::default()).unwrap();
    let (_, cells) = source.next_row().unwrap().unwrap();
    assert!(matches!(cells[0], Some(oce_api::Value::Real(x)) if x == 295.15));
    assert_eq!(source.text_cells().collect::<Vec<_>>(), [Some("295.150")]);
}

#[test]
fn an_absent_text_column_is_a_missing_column_after_the_time_and_the_mapped_ones() {
    let map = empty_map();
    let new =
        |header: &'static [u8], format| CsvSource::new(&map, header, format, &ReplayLimits::MAX);
    assert_eq!(
        new(b"t,a\n", text_format("t", &["a", "z", "y"])).unwrap_err(),
        SourceError::MissingColumn {
            column: "z".to_owned()
        }
    );
    assert_eq!(
        new(b"a\n", text_format("t", &["z"])).unwrap_err(),
        SourceError::MissingColumn {
            column: "t".to_owned()
        },
        "the time column first"
    );
    let engine = fixtures::load("generic_time_suppression");
    let map = fixtures::path_map(&engine, &MissingPolicy::Refuse);
    let tset = map.column(0).to_owned();
    assert_eq!(
        CsvSource::new(
            &map,
            &b"t\n"[..],
            text_format("t", &["z"]),
            &ReplayLimits::MAX
        )
        .unwrap_err(),
        SourceError::MissingColumn { column: tset },
        "then the mapped columns in slot order"
    );
}

#[test]
fn a_text_column_stays_among_the_ignored_columns() {
    let map = empty_map();
    let source = CsvSource::new(
        &map,
        &b"t,a,b\n"[..],
        text_format("t", &["a"]),
        &ReplayLimits::MAX,
    )
    .unwrap();
    assert_eq!(source.ignored_columns(), ["a", "b"]);
}

#[test]
fn a_text_cell_is_checked_after_the_time_and_the_mapped_cells() {
    // Not UTF-8, with its row and field.
    assert_eq!(
        read_text(b"t,x,a\n0,1,ok\n1,1,\xff\n", &["a"]),
        Err(SourceError::NotUtf8 { row: 2, field: 2 })
    );
    // A bad time is reported before the text cell, as is a bad mapped cell.
    assert_eq!(
        read_text(b"t,a\nnow,\xff\n", &["a"]),
        Err(SourceError::BadCell {
            row: 1,
            column: "t".to_owned()
        })
    );
    let engine = fixtures::load("generic_time_suppression");
    let map = fixtures::path_map(&engine, &MissingPolicy::Refuse);
    let (tset, tzon) = (map.column(0).to_owned(), map.column(1).to_owned());
    let mut bytes = format!("t,{tset},{tzon},a\n0,x,1,").into_bytes();
    bytes.extend_from_slice(b"\xff\n");
    let format = text_format("t", &["a"]);
    let mut source = CsvSource::new(&map, &bytes[..], format, &ReplayLimits::MAX).unwrap();
    assert_eq!(
        source.next_row().unwrap_err(),
        SourceError::BadCell {
            row: 1,
            column: tset
        }
    );
    // A non-UTF-8 cell in a column that is neither mapped nor text is still not read.
    assert_eq!(read_text(b"t,a,b\n0,1,\xff\n", &["a"]).unwrap().len(), 1);
}

#[test]
fn text_cells_are_none_before_the_first_row_after_the_last_and_after_an_error() {
    let map = empty_map();
    let mut source = CsvSource::new(
        &map,
        &b"t,a\n0,x\n1,y\n2\n"[..],
        text_format("t", &["a"]),
        &ReplayLimits::MAX,
    )
    .unwrap();
    assert_eq!(source.text_cells().len(), 1);
    assert_eq!(source.text_cells().collect::<Vec<_>>(), [None]);
    for expected in ["x", "y"] {
        assert!(source.next_row().unwrap().is_some());
        assert_eq!(source.text_cells().collect::<Vec<_>>(), [Some(expected)]);
    }
    assert_eq!(
        source.next_row().unwrap_err(),
        SourceError::Ragged { row: 3 }
    );
    assert_eq!(source.text_cells().collect::<Vec<_>>(), [None]);
    assert_eq!(
        source.next_row().unwrap_err(),
        SourceError::Ragged { row: 3 }
    );
    assert_eq!(source.text_cells().collect::<Vec<_>>(), [None]);

    // An error on the text cell itself, after an earlier text cell of the row was taken.
    let mut source = CsvSource::new(
        &map,
        &b"t,a,b\n0,x,y\n1,x,\xff\n"[..],
        text_format("t", &["a", "b"]),
        &ReplayLimits::MAX,
    )
    .unwrap();
    assert!(source.next_row().unwrap().is_some());
    assert!(source.next_row().is_err());
    assert_eq!(source.text_cells().collect::<Vec<_>>(), [None, None]);

    // At the end.
    let mut source = CsvSource::new(
        &map,
        &b"t,a\n0,x\n"[..],
        text_format("t", &["a"]),
        &ReplayLimits::MAX,
    )
    .unwrap();
    assert!(source.next_row().unwrap().is_some());
    assert!(source.next_row().unwrap().is_none());
    assert_eq!(source.text_cells().collect::<Vec<_>>(), [None]);
}

#[test]
fn a_text_cell_is_held_to_the_cell_cap() {
    let limits = ReplayLimits {
        max_cell_bytes: 8,
        ..ReplayLimits::MAX
    };
    let read = |bytes: &[u8]| read_text_with(bytes, text_format("t", &["a"]), &limits);
    assert_eq!(read(b"t,a\n0,inactive\n").unwrap()[0].1, [text("inactive")]);
    assert_eq!(
        read(b"t,a\n0,inactive!\n"),
        Err(SourceError::CellTooLong {
            row: 1,
            field: 1,
            limit: 8
        })
    );
}

#[test]
fn reading_text_cells_allocates_nothing_once_the_buffers_have_grown() {
    let positive = measure(|| {
        std::hint::black_box(vec![0_u8; 64]);
    });
    assert_eq!(positive.count_total, 1, "the counter observes allocations");
    let mut bytes = String::from("t,a,b,c\n0,active,inactive,longest of all the cells\n");
    for k in 1..200 {
        bytes.push_str(&format!("{k},active,NA,{}\n", "x".repeat(k % 20)));
    }
    let map = empty_map();
    let mut source = CsvSource::new(
        &map,
        bytes.as_bytes(),
        text_format("t", &["a", "b", "c"]),
        &ReplayLimits::MAX,
    )
    .unwrap();
    // The first row grows the record and text buffers to the longest row.
    assert!(source.next_row().unwrap().is_some());
    let mut rows = 1;
    let info = measure(|| {
        while source.next_row().unwrap().is_some() {
            rows += 1;
            std::hint::black_box(source.text_cells().flatten().map(str::len).sum::<usize>());
        }
    });
    assert_eq!(rows, 200);
    assert_eq!((info.count_total, info.bytes_total), (0, 0), "{info:?}");
}

#[test]
fn declaring_a_text_column_changes_no_byte_of_a_replay_trace() {
    let trace = |texts: &[&str]| {
        let engine = fixtures::load("generic_time_suppression");
        let map = fixtures::path_map(&engine, &MissingPolicy::Refuse);
        let bytes = format!(
            "t,{},{},state\n0,295,293,active\n5,296,294,inactive\n",
            map.column(0),
            map.column(1)
        );
        let format = CsvFormat {
            text_columns: texts.iter().map(|&t| t.to_owned()).collect(),
            ..CsvFormat::seconds("t")
        };
        let source = CsvSource::new(
            &map,
            std::io::Cursor::new(bytes),
            format,
            &ReplayLimits::MAX,
        )
        .unwrap();
        let (_, trace) = Replay::new(engine, map, source, ReplayOptions::new(TimeBase::Recorded))
            .unwrap()
            .run()
            .unwrap();
        assert_eq!(trace.rows(), 2);
        trace.canonical_bytes()
    };
    assert_eq!(trace(&["state"]), trace(&[]));
}
