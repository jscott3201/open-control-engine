//! Spaces and tabs around an unquoted CSV cell or header name are not part of it (issue #733):
//! `CsvSource` trims those two bytes, and only those, before the missing-token comparison, the
//! parse and the header match; a quoted cell is kept as `csv-core` unquotes it; the cell cap
//! counts the bytes before trimming; and an unpadded file reads exactly as it always has (the
//! canonical-trace goldens and `csv_hostile_input.rs` pin that).

#[path = "support/csv_text.rs"]
mod csv_text;

use csv_text::{Pieces, read_text, read_text_with, text, text_format};
use oce_api::{InputDefinition, Value, ValueType};
use oce_replay::{
    ColumnKind, ColumnMap, ColumnSpec, CsvFormat, CsvSource, CsvTime, FrameSource, InputMap,
    MissingPolicy, ReplayLimits, SourceError, TimeOrigin,
};

/// OBC §12.7.3's `reference_input.csv`, as the specification prints it: the `<pre>` block of
/// <https://obc.lbl.gov/specification/verification.html>, read 2026-11-21, its markup removed.
const REFERENCE_INPUT: &str = "time, uHea, uCoo, TZonSet,    TZon,    TOut, uFan
   0,    1,    0,  293.15,  292.15,  283.15,    1
  60,  0.5,    0,  293.15,  292.15,  283.15,    1
 120,    0,  0.5,  293.15,  292.15,  283.15,    1
 180,    0,    1,  293.15,  292.15,  283.15,    1
3600,    0,    1,  293.15,  292.15,  283.15,    1
";

/// The same file with no padding.
const REFERENCE_INPUT_UNPADDED: &str = "time,uHea,uCoo,TZonSet,TZon,TOut,uFan
0,1,0,293.15,292.15,283.15,1
60,0.5,0,293.15,292.15,283.15,1
120,0,0.5,293.15,292.15,283.15,1
180,0,1,293.15,292.15,283.15,1
3600,0,1,293.15,292.15,283.15,1
";

/// OBC §12.7.3's `indicator.csv`, from the same page.
const INDICATOR: &str = "time, TSupCoo\n0, 1\n120, 0\n600, 1\n";

/// Synthetic inputs of each parsed kind: `b` Boolean, `i` Integer, `x` Real, mapped by name.
fn typed_map() -> InputMap {
    let definitions: Vec<InputDefinition> = [
        ("m.b", ValueType::Boolean),
        ("m.i", ValueType::Integer),
        ("m.x", ValueType::Real),
    ]
    .into_iter()
    .map(|(path, value_type)| InputDefinition {
        path: path.to_owned(),
        value_type,
        min: None,
        max: None,
    })
    .collect();
    let specs = [
        ColumnSpec::new("b", ColumnKind::Boolean),
        ColumnSpec::new("i", ColumnKind::Integer),
        ColumnSpec::new("x", ColumnKind::Real),
    ];
    InputMap::resolve(
        &definitions,
        &specs,
        &ColumnMap::by_name(&specs),
        &MissingPolicy::Refuse,
        &[],
    )
    .unwrap()
}

/// A cell as text a test can compare: `-` missing, `b:`, `i:` or `x:` and the value (a Real by
/// its bits).
fn show(cell: &Option<Value>) -> String {
    match cell {
        None => "-".to_owned(),
        Some(Value::Boolean(b)) => format!("b:{b}"),
        Some(Value::Integer(i)) => format!("i:{i}"),
        Some(Value::Real(x)) => format!("x:{:016x}", x.to_bits()),
        Some(other) => panic!("{other:?}"),
    }
}

/// Every row of `bytes` through the typed map, with time column `t`, as `(time, [b, i, x])`.
fn read_typed(bytes: &[u8], format: CsvFormat) -> Result<Vec<(f64, Vec<String>)>, SourceError> {
    let map = typed_map();
    let mut source = CsvSource::new(&map, bytes, format, &ReplayLimits::default())?;
    let mut rows = Vec::new();
    while let Some((t, cells)) = source.next_row()? {
        rows.push((t, cells.iter().map(show).collect()));
    }
    Ok(rows)
}

fn typed(bytes: &str) -> Result<Vec<(f64, Vec<String>)>, SourceError> {
    read_typed(bytes.as_bytes(), CsvFormat::seconds("t"))
}

fn x(value: f64) -> String {
    format!("x:{:016x}", value.to_bits())
}

#[test]
fn obc_12_7_3_padded_reference_input_reads_as_its_unpadded_twin() {
    let names = ["uHea", "uCoo", "TZonSet", "TZon", "TOut", "uFan"];
    let format = text_format("time", &names);
    let padded = read_text_with(
        REFERENCE_INPUT.as_bytes(),
        format.clone(),
        &ReplayLimits::MAX,
    )
    .unwrap();
    let unpadded = read_text_with(
        REFERENCE_INPUT_UNPADDED.as_bytes(),
        format,
        &ReplayLimits::MAX,
    )
    .unwrap();
    assert_eq!(padded, unpadded);
    let times: Vec<f64> = padded.iter().map(|(t, _)| *t).collect();
    assert_eq!(times, [0.0, 60.0, 120.0, 180.0, 3600.0]);
    assert_eq!(
        padded[1].1,
        ["0.5", "0", "293.15", "292.15", "283.15", "1"].map(text)
    );
    let indicator = read_text_with(
        INDICATOR.as_bytes(),
        text_format("time", &["TSupCoo"]),
        &ReplayLimits::MAX,
    )
    .unwrap();
    assert_eq!(
        indicator,
        [
            (0.0, vec![text("1")]),
            (120.0, vec![text("0")]),
            (600.0, vec![text("1")])
        ]
    );
}

#[test]
fn a_padded_cell_of_every_kind_reads_as_its_trimmed_text() {
    let rows = typed("t,b,i,x\n  0 , true ,\t-3\t, 1.5\n\t1\t,\t0 ,  +7,inf  \n").unwrap();
    assert_eq!(
        rows,
        [
            (0.0, vec!["b:true".into(), "i:-3".into(), x(1.5)]),
            (1.0, vec!["b:false".into(), "i:7".into(), x(f64::INFINITY)]),
        ]
    );
    // Padded instants.
    let origin = TimeOrigin::parse("2026-11-08T00:00:00Z").unwrap();
    let format = CsvFormat {
        time: CsvTime::Instant { origin },
        ..CsvFormat::seconds("t")
    };
    let rows = read_typed(
        b"t,b,i,x\n 2026-11-08T00:00:05Z\t,1,1,1\n\t2026-11-08T01:00:00+01:00 ,1,1,1\n",
        format,
    )
    .unwrap();
    assert_eq!(rows.iter().map(|r| r.0).collect::<Vec<_>>(), [5.0, 0.0]);
}

#[test]
fn a_cell_of_spaces_and_tabs_only_is_missing() {
    let rows = typed("t,b,i,x\n0,  ,\t, \t \n").unwrap();
    assert_eq!(rows, [(0.0, vec!["-".to_owned(), "-".into(), "-".into()])]);
    assert_eq!(
        read_text(b"t,a\n0, \t\n", &["a"]).unwrap(),
        [(0.0, vec![None])]
    );
    // A time cell of padding alone is a missing time, never a substituted one.
    assert_eq!(
        typed("t,b,i,x\n \t ,1,1,1\n"),
        Err(SourceError::MissingTime { row: 1 })
    );
}

#[test]
fn a_missing_token_is_compared_with_the_trimmed_cell() {
    let rows = typed("t,b,i,x\n0, NA ,\tNaN\t,NA \n").unwrap();
    assert_eq!(rows, [(0.0, vec!["-".to_owned(), "-".into(), "-".into()])]);
    assert_eq!(
        typed("t,b,i,x\n NA ,1,1,1\n"),
        Err(SourceError::MissingTime { row: 1 })
    );
    // A token that is itself padded never matches: every cell it could match is trimmed first.
    let format = CsvFormat {
        missing_tokens: vec![" NA".to_owned()],
        ..CsvFormat::seconds("t")
    };
    assert_eq!(
        read_typed(b"t,b,i,x\n0,1,1, NA\n", format),
        Err(SourceError::BadCell {
            row: 1,
            column: "x".to_owned()
        })
    );
}

#[test]
fn a_quoted_cell_keeps_its_inner_spaces() {
    let rows = read_text(
        b"t,a,b,c,d\n0,\" a \",\"  \",\"\",\"\t1\"\n",
        &["a", "b", "c", "d"],
    );
    assert_eq!(
        rows.unwrap(),
        [(0.0, vec![text(" a "), text("  "), None, text("\t1")])]
    );
    // So a quoted padded number does not parse, and a quoted blank is no missing cell.
    for cell in ["\" 1\"", "\"1 \"", "\" \""] {
        assert_eq!(
            typed(&format!("t,b,i,x\n0,1,1,{cell}\n")),
            Err(SourceError::BadCell {
                row: 1,
                column: "x".to_owned()
            }),
            "{cell}"
        );
    }
    // A quoted header keeps its spaces too: ` x ` is not `x`.
    assert_eq!(
        typed("t,b,i,\" x \"\n0,1,1,1\n"),
        Err(SourceError::MissingColumn {
            column: "x".to_owned()
        })
    );
}

#[test]
fn padding_outside_quotes_is_kept_as_csv_core_reads_it() {
    // A byte after the closing quote joins the cell, and a quote after padding is no opening
    // quote (RFC 4180 has neither): kept untrimmed, so the parse refuses them.
    let rows = read_text(b"t,a,b,c\n0,\"a\" , \"a\",\"a\"b \n", &["a", "b", "c"]).unwrap();
    assert_eq!(rows, [(0.0, vec![text("a "), text("\"a\""), text("ab ")])]);
    for cell in ["\"1\" ", " \"1\""] {
        assert!(
            matches!(
                typed(&format!("t,b,i,x\n0,1,1,{cell}\n")),
                Err(SourceError::BadCell { row: 1, .. })
            ),
            "{cell}"
        );
    }
}

#[test]
fn a_quoted_cell_is_recognised_after_a_blank_line_a_crlf_or_a_second_byte_order_mark() {
    let texts = ["a", "b"];
    let expected = [(0.0, vec![text(" x "), text(" y")])];
    for input in [
        &b"t,a,b\n0,\" x \",\" y\"\n"[..],
        b"t,a,b\r\n\r\n\n0,\" x \",\" y\"\r\n",
        b"t,a,b\r\r0,\" x \",\" y\"",
    ] {
        assert_eq!(read_text(input, &texts).unwrap(), expected, "{input:?}");
    }
    // The first field of a record, quoted.
    let rows = read_text(b"a,t\n\r\n\" x \",0\n", &["a"]).unwrap();
    assert_eq!(rows, [(0.0, vec![text(" x ")])]);
    // `csv-core` strips a second byte-order mark from its first input, so the quote after it
    // still opens the header's first name, which keeps its space.
    let rows = read_text_with(
        &b"\xef\xbb\xbf\xef\xbb\xbf\" a\",t\n1,0\n"[..],
        text_format("t", &[" a"]),
        &ReplayLimits::default(),
    )
    .unwrap();
    assert_eq!(rows, [(0.0, vec![text("1")])]);
}

#[test]
fn other_whitespace_is_never_padding() {
    // A no-break space, a zero-width space, a byte-order mark, a vertical tab, a form feed.
    for pad in ["\u{a0}", "\u{200b}", "\u{feff}", "\u{b}", "\u{c}"] {
        for cell in [format!("{pad}1"), format!("1{pad}"), format!(" {pad}1 ")] {
            assert_eq!(
                typed(&format!("t,b,i,x\n0,1,1,{cell}\n")),
                Err(SourceError::BadCell {
                    row: 1,
                    column: "x".to_owned()
                }),
                "{cell:?}"
            );
            let rows = read_text(format!("t,a\n0,{cell}\n").as_bytes(), &["a"]).unwrap();
            assert_eq!(rows[0].1, [text(cell.trim_matches([' ', '\t']))]);
        }
        // Nor around a header name.
        assert_eq!(
            typed(&format!("t,b,i,{pad}x\n0,1,1,1\n")),
            Err(SourceError::MissingColumn {
                column: "x".to_owned()
            })
        );
    }
}

#[test]
fn padding_inside_a_cell_is_part_of_it() {
    assert_eq!(
        typed("t,b,i,x\n0,1,1, 1 5 \n"),
        Err(SourceError::BadCell {
            row: 1,
            column: "x".to_owned()
        })
    );
    let rows = read_text(b"t,a\n0, a \t b \n", &["a"]).unwrap();
    assert_eq!(rows[0].1, [text("a \t b")]);
}

#[test]
fn header_names_are_trimmed_before_they_are_matched_and_listed() {
    let map = typed_map();
    let source = CsvSource::new(
        &map,
        &b" t ,\tb, i , x\t, note , \n"[..],
        CsvFormat::seconds("t"),
        &ReplayLimits::default(),
    )
    .unwrap();
    assert_eq!(source.ignored_columns(), ["note", ""]);
    // A name asked for with padding matches no trimmed header.
    assert_eq!(
        CsvSource::new(
            &map,
            &b"t,b,i, x\n"[..],
            CsvFormat::seconds(" t"),
            &ReplayLimits::default()
        )
        .unwrap_err(),
        SourceError::MissingColumn {
            column: " t".to_owned()
        }
    );
}

#[test]
fn a_header_that_trims_to_another_header_is_a_duplicate_column() {
    assert_eq!(
        typed("t,b,i,x,a, a\n"),
        Err(SourceError::DuplicateColumn {
            column: "a".to_owned()
        })
    );
    assert_eq!(
        typed("t,b,i,x, ,\t\n"),
        Err(SourceError::DuplicateColumn {
            column: String::new()
        })
    );
    assert_eq!(
        typed("t,b,i,x,\tx\n"),
        Err(SourceError::DuplicateColumn {
            column: "x".to_owned()
        })
    );
}

#[test]
fn a_tab_separated_file_trims_its_spaces() {
    let format = CsvFormat {
        delimiter: b'\t',
        ..CsvFormat::seconds("t")
    };
    let rows = read_typed(b" t \t b\t i \tx\n 0 \t 1 \t 2\t 3 \n", format).unwrap();
    assert_eq!(
        rows,
        [(0.0, vec!["b:true".to_owned(), "i:2".into(), x(3.0)])]
    );
}

/// Limits with `max_cell_bytes` lowered to `cap`.
fn cell_cap(cap: u32) -> ReplayLimits {
    ReplayLimits {
        max_cell_bytes: cap,
        ..ReplayLimits::MAX
    }
}

#[test]
fn the_cell_cap_counts_a_cell_before_its_padding_is_trimmed() {
    let read =
        |bytes: &[u8], cap: u32| read_text_with(bytes, text_format("t", &["a"]), &cell_cap(cap));
    // At the cap with padding: read, and trimmed.
    assert_eq!(
        read(b"t,a\n0,  12\n", 4).unwrap(),
        [(0.0, vec![text("12")])]
    );
    assert_eq!(read(b"t,a\n0,1\t\t\t\n", 4).unwrap()[0].1, [text("1")]);
    // One byte past it, though the trimmed cell is shorter than the cap.
    for (bytes, field) in [
        (&b"t,a\n0,  123\n"[..], 1),
        (b"t,a\n0,12   \n", 1),
        (b"t,a\n     ,1\n", 0),
    ] {
        assert_eq!(
            read(bytes, 4),
            Err(SourceError::CellTooLong {
                row: 1,
                field,
                limit: 4
            }),
            "{bytes:?}"
        );
    }
    // A header name likewise.
    assert!(read(b"  t ,a\n0,1\n", 4).is_ok());
    assert_eq!(
        read(b"t,  a  \n0,1\n", 4),
        Err(SourceError::CellTooLong {
            row: 0,
            field: 1,
            limit: 4
        })
    );
    // At the documented maximum: 255 spaces and a digit is read, 256 and a digit refused.
    let at = format!("t,a\n0,{}1\n", " ".repeat(255));
    assert_eq!(read(at.as_bytes(), 256).unwrap()[0].1, [text("1")]);
    let past = format!("t,a\n0,{}1\n", " ".repeat(256));
    assert_eq!(
        read(past.as_bytes(), 256),
        Err(SourceError::CellTooLong {
            row: 1,
            field: 1,
            limit: 256
        })
    );
}

#[test]
fn padding_and_quotes_read_the_same_however_the_reader_splits_the_bytes() {
    // The two byte-order-mark entries read the same split or whole only since the first fill
    // hands `csv-core` four bytes after a skipped mark: before, a reader of one byte per call
    // kept the second mark as a cell's bytes, a whole slice had `csv-core` strip it.
    let corpus: [&[u8]; 8] = [
        b"\xef\xbb\xbf\xef\xbb\xbft,a,b\n0,1,2\n",
        b"\xef\xbb\xbf\xef\xbb\xbf",
        REFERENCE_INPUT.as_bytes(),
        b"\xef\xbb\xbf t ,a,b\r\n\r\n 0 ,\" x \",\t\"\"\r\n1,\"a\"\" b\" ,NA\r\n",
        b"\xef\xbb\xbf\xef\xbb\xbf\" t\",a,b\n\n\n\" 1\",\" a\" , b \n",
        b"t,a,b\r\r\r0,\"\r\n\",  \n",
        b"t,a,b\n0, \"x, y\" ,1\n",
        b"t,a,b\n0,1\n",
    ];
    for bytes in corpus {
        let whole = read_text_with(bytes, text_format("t", &["a", "b"]), &ReplayLimits::MAX);
        let whole_time = read_text_with(bytes, text_format(" t", &["a", "b"]), &ReplayLimits::MAX);
        let quoted_time = read_text_with(
            bytes,
            text_format("time", &["uHea", "uFan"]),
            &ReplayLimits::MAX,
        );
        for piece in 1..=9 {
            let split =
                |format| read_text_with(Pieces::new(bytes, piece), format, &ReplayLimits::MAX);
            assert_eq!(
                split(text_format("t", &["a", "b"])),
                whole,
                "{piece} {bytes:?}"
            );
            assert_eq!(
                split(text_format(" t", &["a", "b"])),
                whole_time,
                "{piece} {bytes:?}"
            );
            assert_eq!(
                split(text_format("time", &["uHea", "uFan"])),
                quoted_time,
                "{piece} {bytes:?}"
            );
        }
    }
}
