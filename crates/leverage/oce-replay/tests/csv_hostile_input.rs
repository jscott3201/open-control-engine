//! A hostile CSV corpus refuses with a typed error or parses exactly as the caps and the `csv`
//! reader define, and never panics (design note, test plan: bounds; decisions 8 and 22;
//! GitHub #381 and #382 are the pattern).

#[path = "support/fixtures.rs"]
mod fixtures;

use fixtures::{load, path_map};
use oce_replay::{CsvFormat, CsvSource, FrameSource, MissingPolicy, ReplayLimits, SourceError};

const FIXTURE: &str = "generic_time_suppression";
const TSET: &str = "http://example.org#g36.source.generic_time_suppression.TSet";
const TZON: &str = "http://example.org#g36.source.generic_time_suppression.TZon";

/// Read every row of `bytes` (with `H` standing for the header names), collecting
/// `(time, [TSet, TZon] as Option<f64>)` or the first error.
type Rows = Vec<(f64, Vec<Option<f64>>)>;

fn read(bytes: &[u8]) -> Result<Rows, SourceError> {
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let header = format!("t,{TSET},{TZON}");
    let text = replace(bytes, b"H", header.as_bytes());
    let mut source = CsvSource::new(
        &map,
        &text[..],
        CsvFormat::seconds("t"),
        &ReplayLimits::default(),
    )?;
    let mut rows = Vec::new();
    while let Some((t, cells)) = source.next_row()? {
        rows.push((
            t,
            cells
                .iter()
                .map(|c| c.as_ref().map(|v| v.as_real().unwrap()))
                .collect(),
        ));
    }
    Ok(rows)
}

fn replace(haystack: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    match haystack.windows(from.len()).position(|w| w == from) {
        Some(at) => [&haystack[..at], to, &haystack[at + from.len()..]].concat(),
        None => haystack.to_vec(),
    }
}

#[test]
fn a_well_formed_file_with_quotes_crlf_and_ignored_columns_parses() {
    let rows =
        read(b"H,\"note, quoted\"\r\n0,295,293,\"a, b\"\r\n1.5,\"296\",inf,\"\"\"\"\r\n").unwrap();
    assert_eq!(
        rows,
        [
            (0.0, vec![Some(295.0), Some(293.0)]),
            (1.5, vec![Some(296.0), Some(f64::INFINITY)]),
        ]
    );
}

#[test]
fn a_utf8_byte_order_mark_is_stripped_from_the_header() {
    let rows = read(b"\xef\xbb\xbfH\n0,1,2\n").unwrap();
    assert_eq!(rows, [(0.0, vec![Some(1.0), Some(2.0)])]);
}

#[test]
fn missing_markers_are_none_and_never_values() {
    let rows = read(b"H\n0,,NA\n1,NaN,nan\n2,-inf,1e308\n").unwrap();
    assert_eq!(rows[0].1, [None, None]);
    assert_eq!(rows[1].1, [None, None]);
    assert_eq!(rows[2].1, [Some(f64::NEG_INFINITY), Some(1e308)]);
}

#[test]
fn an_unterminated_quote_runs_to_the_end_of_input_and_is_refused() {
    // The reader takes everything after the open quote as one field, so the record is short.
    assert_eq!(
        read(b"H\n0,\"295,293\n1,2,3\n"),
        Err(SourceError::Ragged { row: 1 })
    );
    // In the last field, the rest of the input is the cell: not a number.
    let engine_column = TZON.to_owned();
    assert_eq!(
        read(b"H\n0,295,\"293\n"),
        Err(SourceError::BadCell {
            row: 1,
            column: engine_column
        })
    );
}

#[test]
fn an_overlong_cell_is_refused_even_inside_quotes() {
    let mut text = b"H\n0,1,\"".to_vec();
    text.extend(std::iter::repeat_n(b'9', 300));
    text.extend_from_slice(b"\"\n");
    assert_eq!(
        read(&text),
        Err(SourceError::CellTooLong {
            row: 1,
            field: 2,
            limit: 256
        })
    );
}

#[test]
fn ragged_rows_are_refused_with_their_row() {
    assert_eq!(
        read(b"H\n0,1,2\n1,1\n"),
        Err(SourceError::Ragged { row: 2 })
    );
    assert_eq!(read(b"H\n0,1,2,3\n"), Err(SourceError::Ragged { row: 1 }));
}

#[test]
fn non_utf8_cells_and_headers_are_refused() {
    assert_eq!(
        read(b"H\n0,\xff\xfe,2\n"),
        Err(SourceError::NotUtf8 { row: 1, field: 1 })
    );
    assert_eq!(
        read(b"H,\xc3\x28\n0,1,2,3\n"),
        Err(SourceError::NotUtf8 { row: 0, field: 3 })
    );
    assert_eq!(
        read(b"H\n\xff,1,2\n"),
        Err(SourceError::NotUtf8 { row: 1, field: 0 })
    );
    // A non-UTF-8 cell in an ignored column is not read.
    assert_eq!(read(b"H,x\n0,1,2,\xff\n").unwrap().len(), 1);
}

#[test]
fn malformed_headers_and_cells_are_typed_errors() {
    assert_eq!(
        read(b""),
        Err(SourceError::MissingColumn {
            column: "t".to_owned()
        })
    );
    assert_eq!(
        read(b"x,y\n"),
        Err(SourceError::MissingColumn {
            column: "t".to_owned()
        })
    );
    assert_eq!(
        read(format!("t,{TSET}\n0,1\n").as_bytes()),
        Err(SourceError::MissingColumn {
            column: TZON.to_owned()
        })
    );
    assert_eq!(
        read(b"H,t\n"),
        Err(SourceError::DuplicateColumn {
            column: "t".to_owned()
        })
    );
    assert_eq!(
        read(b"H\n0,1.0.0,2\n"),
        Err(SourceError::BadCell {
            row: 1,
            column: TSET.to_owned()
        })
    );
    assert_eq!(
        read(b"H\n0, 1,2\n"),
        Err(SourceError::BadCell {
            row: 1,
            column: TSET.to_owned()
        }),
        "no trimming"
    );
    assert_eq!(
        read(b"H\nnow,1,2\n"),
        Err(SourceError::BadCell {
            row: 1,
            column: "t".to_owned()
        })
    );
    assert_eq!(
        read(b"H\n").unwrap(),
        [],
        "a header alone is an empty source"
    );
}

#[test]
fn integer_boolean_and_enum_cells_parse_strictly() {
    // multizone_vav_supply_fan: dpDuc Real, uOpeMod Integer, uZonPreResReq Integer.
    let engine = load("multizone_vav_supply_fan");
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let header = format!("t,{},{},{}", map.column(0), map.column(1), map.column(2));
    for (row, ok) in [
        ("0,1,2,3", true),
        ("0,1,2.0,3", false),
        ("0,1,+2,-3", true),
        ("0,1,9223372036854775808,3", false),
    ] {
        let text = format!("{header}\n{row}\n");
        let mut source = CsvSource::new(
            &map,
            text.as_bytes(),
            CsvFormat::seconds("t"),
            &ReplayLimits::default(),
        )
        .unwrap();
        assert_eq!(source.next_row().is_ok(), ok, "{row}");
    }
    // cooling_only_controller's Booleans: true, false, 1, 0 only.
    let engine = load("cooling_only_controller");
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let fan = map
        .slot_of_column(
            &map.inputs()
                .iter()
                .find(|d| d.path.ends_with(".u1Fan"))
                .unwrap()
                .path,
        )
        .unwrap();
    let header: Vec<String> = std::iter::once("t".to_owned())
        .chain((0..map.len()).map(|s| map.column(s).to_owned()))
        .collect();
    for (cell, expected) in [
        ("true", Some(true)),
        ("1", Some(true)),
        ("false", Some(false)),
        ("0", Some(false)),
        ("TRUE", None),
        ("yes", None),
    ] {
        let mut row: Vec<String> = vec!["0".to_owned()];
        row.extend((0..map.len()).map(|s| {
            if s == fan {
                cell.to_owned()
            } else {
                "1".to_owned()
            }
        }));
        let text = format!("{}\n{}\n", header.join(","), row.join(","));
        let mut source = CsvSource::new(
            &map,
            text.as_bytes(),
            CsvFormat::seconds("t"),
            &ReplayLimits::default(),
        )
        .unwrap();
        match (source.next_row(), expected) {
            (Ok(Some((_, cells))), Some(b)) => {
                assert!(
                    matches!(cells[fan], Some(oce_api::Value::Boolean(x)) if x == b),
                    "{cell}"
                );
            }
            (Err(SourceError::BadCell { row: 1, .. }), None) => {}
            (other, _) => panic!("{cell}: {other:?}"),
        }
    }
}

/// A reader that yields `prefix`, then `count` copies of `byte`, without holding them.
struct Flood {
    prefix: Vec<u8>,
    at: usize,
    byte: u8,
    count: u64,
}

impl std::io::Read for Flood {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.at < self.prefix.len() {
            let n = buf.len().min(self.prefix.len() - self.at);
            buf[..n].copy_from_slice(&self.prefix[self.at..self.at + n]);
            self.at += n;
            return Ok(n);
        }
        let n = (buf.len() as u64).min(self.count) as usize;
        buf[..n].fill(self.byte);
        self.count -= n as u64;
        Ok(n)
    }
}

/// Construct and read a CSV source over `flood`, returning the error and the peak bytes allocated.
fn flooded(prefix: &str, byte: u8, count: u64, limits: ReplayLimits) -> (SourceError, u64) {
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let prefix = prefix
        .replacen('H', &format!("t,{TSET},{TZON}"), 1)
        .into_bytes();
    let mut error = None;
    let info = allocation_counter::measure(|| {
        let flood = Flood {
            prefix,
            at: 0,
            byte,
            count,
        };
        error = Some(
            match CsvSource::new(&map, flood, CsvFormat::seconds("t"), &limits) {
                Err(e) => e,
                Ok(mut source) => loop {
                    match source.next_row() {
                        Err(e) => break e,
                        Ok(None) => panic!("the flood parsed to the end"),
                        Ok(Some(_)) => {}
                    }
                },
            },
        );
    });
    (error.unwrap(), info.bytes_max)
}

const MB: u64 = 1 << 20;

#[test]
fn a_flood_of_delimiters_or_an_unterminated_quote_is_refused_in_bounded_memory() {
    let small = ReplayLimits {
        max_columns: 16,
        max_cell_bytes: 64,
        ..ReplayLimits::MAX
    };
    // 64 MB of commas as the header: refused at the 17th field.
    let (error, peak) = flooded("", b',', 64 * MB, small);
    assert_eq!(error, SourceError::TooManyColumns { limit: 16 });
    assert!(peak < 64 * 1024, "peak {peak} bytes");
    // ... and as a data record: refused at the fourth field (the header has three).
    let (error, peak) = flooded("H\n0,1,2\n", b',', 64 * MB, small);
    assert_eq!(error, SourceError::Ragged { row: 2 });
    assert!(peak < 64 * 1024, "peak {peak} bytes");
    // 64 MB inside an unterminated quote, in the header and in a data cell.
    let (error, peak) = flooded("\"", b'x', 64 * MB, small);
    assert_eq!(
        error,
        SourceError::CellTooLong {
            row: 0,
            field: 0,
            limit: 64
        }
    );
    assert!(peak < 64 * 1024, "peak {peak} bytes");
    let (error, peak) = flooded("H\n0,\"", b'9', 64 * MB, small);
    assert_eq!(
        error,
        SourceError::CellTooLong {
            row: 1,
            field: 1,
            limit: 64
        }
    );
    assert!(peak < 64 * 1024, "peak {peak} bytes");
}

#[test]
fn at_the_default_caps_a_100_mb_hostile_line_is_refused_within_the_documented_bound() {
    // The bound: an 8 KiB chunk, one cell and one record of at most 4,096 cells of 256 bytes.
    let bound = 8 * 1024 + 257 + 4 * MB;
    let (error, peak) = flooded("", b',', 100 * MB, ReplayLimits::MAX);
    assert_eq!(error, SourceError::TooManyColumns { limit: 4_096 });
    assert!(peak < bound, "peak {peak} bytes");
    let (error, peak) = flooded("H\n0,", b'7', 100 * MB, ReplayLimits::MAX);
    assert_eq!(
        error,
        SourceError::CellTooLong {
            row: 1,
            field: 1,
            limit: 256
        }
    );
    assert!(peak < bound, "peak {peak} bytes");
}

#[test]
fn an_invalid_delimiter_is_refused() {
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    for delimiter in [b'"', b'\r', b'\n', 0xff] {
        let format = CsvFormat {
            delimiter,
            ..CsvFormat::seconds("t")
        };
        assert_eq!(
            CsvSource::new(&map, &b"t\n"[..], format, &ReplayLimits::default()).unwrap_err(),
            SourceError::BadDelimiter { delimiter }
        );
    }
    let format = CsvFormat {
        delimiter: b';',
        ..CsvFormat::seconds("t")
    };
    let text = format!("t;{TSET};{TZON}\n0;1;2\n");
    assert!(CsvSource::new(&map, text.as_bytes(), format, &ReplayLimits::default()).is_ok());
}
