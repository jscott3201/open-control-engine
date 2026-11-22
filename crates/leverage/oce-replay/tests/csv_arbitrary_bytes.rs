//! Two seeded properties of `CsvSource` with padding and text columns (issues #728, #733):
//! arbitrary bytes never panic and read the same however the reader splits them, every outcome a
//! set of rows or a typed error; and cells written by an independent model (a value, padding
//! around it, quotes or none) read back as the model says, which is the oracle for the trimming
//! rule: the expected text is the value the model chose, never the reader's own trimming.

#[path = "support/csv_text.rs"]
mod csv_text;

use csv_text::{Pieces, TextRows, read_text_with, text_format};
use oce_api::{InputDefinition, Value, ValueType};
use oce_replay::{
    ColumnKind, ColumnMap, ColumnSpec, CsvFormat, CsvSource, FrameSource, InputMap, MissingPolicy,
    ReplayLimits, SourceError,
};

/// A small deterministic generator, so the corpus is the same on every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(self.0 >> 33).expect("fits") % below
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.next(items.len())]
    }
}

/// The byte runs inputs are drawn from: structure, padding and the whitespace that is not
/// padding, byte-order marks, digits, tokens, words and bytes that are not UTF-8.
const RUNS: &[&[u8]] = &[
    b",",
    b",",
    b"\"",
    b"\"",
    b" ",
    b" ",
    b"\t",
    b"\r",
    b"\n",
    b"\n",
    b"\r\n",
    b"\xef\xbb\xbf",
    b"\xc2\xa0",
    b"\xe2\x80\x8b",
    b"\x0b",
    b"\x0c",
    b"0",
    b"1",
    b"7",
    b"-",
    b".",
    b"e",
    b"inf",
    b"NA",
    b"NaN",
    b"true",
    b"active",
    b"t",
    b"a",
    b"b",
    b"x",
    b"\xff",
    b"\xc3",
];

/// Limits a generated input is read under: the maxima, or caps small enough to be reached.
fn limits(rng: &mut Lcg) -> ReplayLimits {
    if rng.next(4) != 0 {
        return ReplayLimits::MAX;
    }
    ReplayLimits {
        max_rows: rng.next(6) as u64,
        max_columns: 1 + rng.next(5) as u32,
        max_cell_bytes: rng.next(9) as u32,
        max_source_bytes: rng.next(400) as u64,
        ..ReplayLimits::MAX
    }
}

/// Whole cells: values, tokens, quoted cells with padding inside, a quote after padding.
const CELLS: &[&[u8]] = &[
    b"1",
    b"-2.5",
    b"NA",
    b"active",
    b"x y",
    b"\" q \"",
    b"\"\"",
    b"\"a,b\"",
    b"\"say \"\"x\"\"\"",
    b"\" \t\"",
    b"\"1\" ",
    b"\xef\xbb\xbf1",
];

/// Padding, and the whitespace that is not.
const PAD_RUNS: &[&[u8]] = &[b"", b"", b" ", b"\t", b"  \t", b"\xc2\xa0", b"\x0c"];

/// An input: a header, sometimes padded or quoted, then either runs at random or records of
/// about three fields, each a few runs between padding, with any line end.
fn generated(rng: &mut Lcg) -> Vec<u8> {
    let mut bytes = Vec::new();
    match rng.next(4) {
        0 => bytes.extend_from_slice(b"t,a,b\n"),
        1 => bytes.extend_from_slice(b" t ,\t\"a\" , b\r\n"),
        2 => bytes.extend_from_slice(b"\xef\xbb\xbft, a\t,\"b\"\n"),
        _ => {}
    }
    if rng.next(4) == 0 {
        for _ in 0..rng.next(120) {
            bytes.extend_from_slice(rng.pick(RUNS));
        }
        return bytes;
    }
    for record in 0..rng.next(6) {
        let fields = if rng.next(5) == 0 { 2 + rng.next(3) } else { 3 };
        for field in 0..fields {
            if field > 0 {
                bytes.push(b',');
            }
            // A time is padded with padding only, so most records reach their text cells.
            let pads = if field == 0 { &PAD_RUNS[..5] } else { PAD_RUNS };
            bytes.extend_from_slice(rng.pick(pads));
            if field == 0 && rng.next(4) != 0 {
                bytes.extend_from_slice(record.to_string().as_bytes());
            } else if rng.next(5) != 0 {
                bytes.extend_from_slice(rng.pick(CELLS));
            } else {
                for _ in 0..rng.next(3) {
                    bytes.extend_from_slice(rng.pick(RUNS));
                }
            }
            bytes.extend_from_slice(rng.pick(pads));
        }
        bytes.extend_from_slice(rng.pick(&[&b"\n"[..], b"\r\n", b"\r"]));
    }
    bytes
}

/// Each well-formed outcome's own invariants: a text cell is never empty (an empty one is
/// missing) and never longer than the cap.
fn check_invariants(rows: &Result<TextRows, SourceError>, limits: &ReplayLimits) {
    if let Ok(rows) = rows {
        for text in rows.iter().flat_map(|(_, cells)| cells).flatten() {
            assert!(!text.is_empty());
            assert!(text.len() <= limits.max_cell_bytes as usize, "{text:?}");
        }
    }
}

#[test]
fn arbitrary_bytes_never_panic_and_read_the_same_however_the_reader_splits_them() {
    let mut rng = Lcg(0x0ce7_2873_3c5f);
    let mut outcomes = [0_usize; 2];
    let mut texts = 0;
    for case in 0..4_000 {
        let bytes = generated(&mut rng);
        let limits = limits(&mut rng);
        let format = text_format("t", &["a", "b"]);
        let whole = read_text_with(&bytes[..], format.clone(), &limits);
        check_invariants(&whole, &limits);
        for piece in [1, 2, 3, 5, 8] {
            let split = read_text_with(Pieces::new(&bytes, piece), format.clone(), &limits);
            if bytes.len() as u64 > limits.max_source_bytes {
                // Over the byte cap the input is refused however it is split, but which refusal
                // comes first depends on how far ahead the reader's calls buffered: a whole
                // slice crosses the cap on the first fill, a byte at a time only where the
                // parse reaches it. That was so before padding was trimmed.
                assert!(
                    split.is_err() && whole.is_err(),
                    "case {case}, piece {piece}: {bytes:?}"
                );
            } else {
                assert_eq!(split, whole, "case {case}, piece {piece}: {bytes:?}");
            }
        }
        outcomes[usize::from(whole.is_ok())] += 1;
        if let Ok(rows) = &whole {
            texts += rows.iter().flat_map(|(_, c)| c).flatten().count();
        }
    }
    // The corpus reaches both sides, rows read and typed refusals, and hands text cells over.
    assert!(outcomes[0] > 1_000 && outcomes[1] > 300, "{outcomes:?}");
    assert!(texts > 500, "{texts} text cells");
}

/// One generated cell: what the model writes, and what it says the reader reads.
struct ModelCell {
    written: String,
    text: Option<String>,
}

/// Values a cell may hold: none starts or ends with a space or a tab, and only the quoted form
/// may hold a delimiter, a quote or a line end.
const VALUES: &[&str] = &[
    "0", "1", "-2.5", "1e3", "inf", "295.15", "NA", "NaN", "", "active", "a b", "x\ty",
];
const QUOTED_ONLY: &[&str] = &["x,y", "say \"hi\"", "two\nlines", " lead", "trail\t", "  "];
const PADS: &[&str] = &["", " ", "\t", "   ", " \t ", "\t\t"];

fn model_cell(rng: &mut Lcg) -> ModelCell {
    let missing = |value: &str| value.is_empty() || value == "NA" || value == "NaN";
    if rng.next(3) == 0 {
        // Quoted: the inner text exactly, padding inside the quotes included.
        let inner = if rng.next(2) == 0 {
            format!("{}{}{}", rng.pick(PADS), rng.pick(VALUES), rng.pick(PADS))
        } else {
            (*rng.pick(QUOTED_ONLY)).to_owned()
        };
        ModelCell {
            written: format!("\"{}\"", inner.replace('"', "\"\"")),
            text: (!missing(&inner)).then_some(inner),
        }
    } else {
        let value = *rng.pick(VALUES);
        ModelCell {
            written: format!("{}{value}{}", rng.pick(PADS), rng.pick(PADS)),
            text: (!missing(value)).then(|| value.to_owned()),
        }
    }
}

/// The map of one Real input `x`.
fn real_map() -> InputMap {
    let definitions = [InputDefinition {
        path: "m.x".to_owned(),
        value_type: ValueType::Real,
        min: None,
        max: None,
    }];
    let specs = [ColumnSpec::new("x", ColumnKind::Real)];
    InputMap::resolve(
        &definitions,
        &specs,
        &ColumnMap::by_name(&specs),
        &MissingPolicy::Refuse,
        &[],
    )
    .unwrap()
}

#[test]
fn cells_written_by_an_independent_model_read_back_as_the_model_says() {
    let mut rng = Lcg(0x7330_0728);
    for case in 0..500 {
        let columns = 1 + rng.next(4);
        let names: Vec<String> = (0..columns).map(|c| format!("c{c}")).collect();
        let pad = |rng: &mut Lcg, name: &str| format!("{}{name}{}", rng.pick(PADS), rng.pick(PADS));
        let mut header = vec![pad(&mut rng, "t")];
        header.extend(names.iter().map(|n| pad(&mut rng, n)));
        let mut bytes = header.join(",");
        let mut expected = Vec::new();
        for row in 0..rng.next(6) {
            let cells: Vec<ModelCell> = (0..columns).map(|_| model_cell(&mut rng)).collect();
            let time = pad(&mut rng, &row.to_string());
            let line: Vec<&str> = std::iter::once(time.as_str())
                .chain(cells.iter().map(|c| c.written.as_str()))
                .collect();
            bytes.push_str(if rng.next(2) == 0 { "\n" } else { "\r\n" });
            bytes.push_str(&line.join(","));
            expected.push((row as f64, cells.into_iter().map(|c| c.text).collect()));
        }
        bytes.push('\n');
        let texts: Vec<&str> = names.iter().map(String::as_str).collect();
        let read = read_text_with(
            bytes.as_bytes(),
            text_format("t", &texts),
            &ReplayLimits::MAX,
        );
        assert_eq!(read.as_ref(), Ok(&expected), "case {case}: {bytes:?}");

        // The same file with its first value column read as a Real: what the model calls a
        // number reads as that number, its missing cells are missing, and anything else, a
        // quoted padded number included, is refused.
        let real = bytes.replacen(&header[1], "x", 1);
        let format = CsvFormat::seconds("t");
        let map = real_map();
        let mut source = CsvSource::new(&map, real.as_bytes(), format, &ReplayLimits::MAX).unwrap();
        for (row, (_, cells)) in expected.iter().enumerate() {
            let read = source.next_row();
            let model = cells[0].as_deref();
            match (model.map(str::parse::<f64>), read) {
                (None, Ok(Some((_, [None])))) => {}
                (Some(Ok(x)), Ok(Some((_, [Some(Value::Real(y))])))) => {
                    assert_eq!(x.to_bits(), y.to_bits(), "case {case} row {row}");
                }
                (Some(Err(_)), Err(SourceError::BadCell { column, .. })) => {
                    assert_eq!(column, "x");
                    break;
                }
                (model, read) => panic!("case {case} row {row}: {model:?} vs {read:?}: {real:?}"),
            }
        }
    }
}
