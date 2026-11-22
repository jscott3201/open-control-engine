//! Reading CSV text through `CsvSource` bound to a map of no inputs: the time column and text
//! columns only, so a test sees each cell exactly as the reader hands it over, with no engine.

#![allow(dead_code)]

use std::io::Read;

use oce_replay::{
    ColumnMap, CsvFormat, CsvSource, FrameSource, InputMap, MissingPolicy, ReplayLimits,
    SourceError,
};

/// Rows as `(time, text cells)`.
pub type TextRows = Vec<(f64, Vec<Option<String>>)>;

/// The map of no inputs.
pub fn empty_map() -> InputMap {
    InputMap::resolve(&[], &[], &ColumnMap::default(), &MissingPolicy::Refuse, &[]).unwrap()
}

/// A comma-separated format in seconds with time column `time` and `texts` as text columns.
pub fn text_format(time: &str, texts: &[&str]) -> CsvFormat {
    CsvFormat {
        text_columns: texts.iter().map(|&name| name.to_owned()).collect(),
        ..CsvFormat::seconds(time)
    }
}

/// Every row of `reader` under `format` and `limits`, or the first error.
pub fn read_text_with<R: Read>(
    reader: R,
    format: CsvFormat,
    limits: &ReplayLimits,
) -> Result<TextRows, SourceError> {
    let map = empty_map();
    let mut source = CsvSource::new(&map, reader, format, limits)?;
    let mut rows = Vec::new();
    loop {
        let t = match source.next_row()? {
            Some((t, _)) => t,
            None => return Ok(rows),
        };
        let texts = source.text_cells().map(|c| c.map(str::to_owned)).collect();
        rows.push((t, texts));
    }
}

/// Every row of `bytes` with time column `t` and `texts` as text columns, at the default caps.
pub fn read_text(bytes: &[u8], texts: &[&str]) -> Result<TextRows, SourceError> {
    read_text_with(bytes, text_format("t", texts), &ReplayLimits::default())
}

/// The text of one cell, for building expected rows.
pub fn text(cell: &str) -> Option<String> {
    Some(cell.to_owned())
}

/// A reader that hands `bytes` over `piece` bytes per call (after the first, which the source
/// fills to four bytes itself), so every chunk boundary a test needs can be placed.
pub struct Pieces {
    /// The bytes still to hand over.
    pub bytes: Vec<u8>,
    /// Where the next call starts.
    pub at: usize,
    /// Bytes per call.
    pub piece: usize,
}

impl Pieces {
    /// `bytes`, `piece` bytes per call.
    pub fn new(bytes: &[u8], piece: usize) -> Self {
        Self {
            bytes: bytes.to_vec(),
            at: 0,
            piece,
        }
    }
}

impl Read for Pieces {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = buf.len().min(self.piece).min(self.bytes.len() - self.at);
        buf[..n].copy_from_slice(&self.bytes[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}
