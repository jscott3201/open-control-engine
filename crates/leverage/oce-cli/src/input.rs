//! Reading one input: an authored sequence's form, the bounded read every verb shares, and the
//! parse.
//!
//! An authored sequence's form comes from `--from` or the extension (`.oce` text, `.json` wire);
//! stdin (`-`) needs `--from`. Every read is bounded by the caller's limit: an authored form's
//! own (`oce_author::MAX_TEXT_BYTES`, `oce_author::MAX_WIRE_BYTES`), or, for the CXF document
//! `id` and `import` read through [`read_named`], `oce_api::MAX_CXF_BYTES`. A file's size is
//! checked from its metadata **before** a byte is
//! read, and the read itself stops one byte past the limit, so neither an over-size file nor an
//! endless stdin is ever held in memory. Every refusal is a typed error code, never a panic, and
//! the input is named only as it was given.

use std::ffi::OsStr;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;

use oce_author::{
    MAX_TEXT_BYTES, MAX_WIRE_BYTES, NOT_UTF8, Sequence, SourceMap, WireError, from_wire_bytes,
    parse_text_bytes, parse_text_with_spans,
};

use crate::codes;
use crate::report::Report;
use crate::{Form, Source};

/// The name a path is reported under: exactly as given, lossily decoded, `-` for stdin.
pub(crate) fn display_name(path: &OsStr) -> String {
    path.to_string_lossy().into_owned()
}

/// Whether `path` names stdin.
pub(crate) fn is_stdin(path: &OsStr) -> bool {
    path == "-"
}

/// The bytes of one input, in its form.
pub(crate) struct Loaded {
    /// The form the bytes are read in.
    pub(crate) form: Form,
    /// The bytes, at most the form's limit.
    pub(crate) bytes: Vec<u8>,
}

/// The size limit of a form.
pub(crate) fn limit(form: Form) -> usize {
    match form {
        Form::Text => MAX_TEXT_BYTES,
        Form::Wire => MAX_WIRE_BYTES,
    }
}

/// Decide the form and read the bytes, or record why not in `report`.
pub(crate) fn load(source: &Source, stdin: &mut dyn Read, report: &mut Report) -> Option<Loaded> {
    let form = match form_of(source) {
        Ok(form) => form,
        Err((code, detail)) => {
            report.error(code, detail, None);
            return None;
        }
    };
    match read_named(&source.file, limit(form), stdin) {
        Ok(bytes) => Some(Loaded { form, bytes }),
        Err((code, detail)) => {
            report.error(code, detail, None);
            None
        }
    }
}

/// The form from `--from`, else the extension.
fn form_of(source: &Source) -> Result<Form, (&'static str, String)> {
    if let Some(form) = source.from {
        return Ok(form);
    }
    if is_stdin(&source.file) {
        return Err((
            codes::MISSING_FROM,
            "reading stdin needs --from text or --from wire".to_owned(),
        ));
    }
    match Path::new(&source.file).extension().and_then(OsStr::to_str) {
        Some("oce") => Ok(Form::Text),
        Some("json") => Ok(Form::Wire),
        _ => Err((
            codes::UNKNOWN_FORM,
            "the file extension is neither .oce (text) nor .json (wire); pass --from text or \
             --from wire"
                .to_owned(),
        )),
    }
}

/// Read the input `file` names, stdin for `-` and a file otherwise, at most `bound` bytes.
pub(crate) fn read_named(
    file: &OsStr,
    bound: usize,
    stdin: &mut dyn Read,
) -> Result<Vec<u8>, (&'static str, String)> {
    if is_stdin(file) {
        read_bounded(stdin, bound)
    } else {
        read_file(Path::new(file), bound)
    }
}

/// Read a file, refusing a missing path, a non-file and an over-size file before reading.
pub(crate) fn read_file(path: &Path, bound: usize) -> Result<Vec<u8>, (&'static str, String)> {
    let metadata = std::fs::metadata(path).map_err(io_refusal)?;
    if !metadata.is_file() {
        return Err((
            codes::NOT_A_FILE,
            "the path is not a regular file".to_owned(),
        ));
    }
    if metadata.len() > bound as u64 {
        return Err(too_large(metadata.len(), bound));
    }
    let file = File::open(path).map_err(io_refusal)?;
    read_bounded(file, bound)
}

/// Read at most `bound` bytes; one byte more is an over-size refusal.
pub(crate) fn read_bounded(
    reader: impl Read,
    bound: usize,
) -> Result<Vec<u8>, (&'static str, String)> {
    let mut bytes = Vec::new();
    let cap = u64::try_from(bound).unwrap_or(u64::MAX).saturating_add(1);
    reader
        .take(cap)
        .read_to_end(&mut bytes)
        .map_err(io_refusal)?;
    if bytes.len() > bound {
        return Err((
            codes::INPUT_TOO_LARGE,
            format!("the input is larger than {bound} bytes"),
        ));
    }
    Ok(bytes)
}

pub(crate) fn too_large(size: u64, bound: usize) -> (&'static str, String) {
    (
        codes::INPUT_TOO_LARGE,
        format!("the input is {size} bytes; at most {bound} are read"),
    )
}

/// An I/O failure as a code and a path-free detail.
pub(crate) fn io_refusal(error: std::io::Error) -> (&'static str, String) {
    match error.kind() {
        ErrorKind::NotFound => (codes::NOT_FOUND, "no such file".to_owned()),
        ErrorKind::PermissionDenied => (codes::READ_FAILED, "permission denied".to_owned()),
        ErrorKind::IsADirectory => (
            codes::NOT_A_FILE,
            "the path is not a regular file".to_owned(),
        ),
        kind => (codes::READ_FAILED, format!("the read failed: {kind}")),
    }
}

/// A parsed sequence and, for text, its source map.
pub(crate) struct Parsed {
    /// The model.
    pub(crate) sequence: Sequence,
    /// The source map when the input was text.
    pub(crate) spans: Option<SourceMap>,
}

/// Parse loaded bytes, or record the refusal in `report`.
pub(crate) fn parse(loaded: &Loaded, report: &mut Report) -> Option<Parsed> {
    match loaded.form {
        Form::Text => {
            let text = decode_text(&loaded.bytes, report)?;
            match parse_text_with_spans(text) {
                Ok((sequence, spans)) => Some(Parsed {
                    sequence,
                    spans: Some(spans),
                }),
                Err(error) => {
                    report.parse_error(&error);
                    None
                }
            }
        }
        Form::Wire => match from_wire_bytes(&loaded.bytes) {
            Ok(sequence) => Some(Parsed {
                sequence,
                spans: None,
            }),
            Err(error) => {
                wire_error(&error, report);
                None
            }
        },
    }
}

/// The bytes as text, or the text form's own `not-utf8` refusal recorded in `report`.
pub(crate) fn decode_text<'b>(bytes: &'b [u8], report: &mut Report) -> Option<&'b str> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Some(text),
        Err(utf8) => {
            match parse_text_bytes(bytes) {
                Err(error) => report.parse_error(&error),
                // `parse_text_bytes` refuses exactly what `from_utf8` refuses; stated anyway.
                Ok(_) => report.error(
                    NOT_UTF8,
                    format!("byte {} is not UTF-8", utf8.valid_up_to()),
                    None,
                ),
            }
            None
        }
    }
}

/// Record a wire refusal under the code and location the refusal itself reports
/// (`oce_author::WireError::code` and `location`): the tool matches no variant, so a variant
/// `oce-author` adds is reported under its own code.
pub(crate) fn wire_error(error: &WireError, report: &mut Report) {
    report.error(error.code(), error.to_string(), error.location());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wire_refusal_is_recorded_under_its_own_code_message_and_location() {
        let refusals: [(&[u8], &str, bool); 4] = [
            (b"\xFF", "not-utf8", false),
            (b"{", "json-syntax", true),
            (b"[]", "wrong-type", true),
            (
                br#"{"sequence":"s","derived_from":"","params":[],"inputs":[],"outputs":[],"blocks":[],"connections":["a b"]}"#,
                "malformed-connection",
                false,
            ),
        ];
        for (bytes, code, located) in refusals {
            let error = oce_author::from_wire_bytes(bytes).expect_err("refused");
            let mut report = Report::new("validate", None);
            wire_error(&error, &mut report);
            let json: serde_json::Value =
                serde_json::from_str(&report.to_json(false)).expect("the report is JSON");
            let recorded = &json["errors"][0];
            assert_eq!(recorded["code"], code);
            assert_eq!(recorded["detail"], error.to_string());
            assert_eq!(recorded["span"].is_null(), !located, "{code}");
            if let Some(at) = error.location() {
                assert_eq!(recorded["span"]["line"], at.line);
                assert_eq!(recorded["span"]["column"], at.column);
            }
            assert!(
                crate::codes::WIRE_MEANINGS
                    .iter()
                    .any(|(listed, _)| *listed == code),
                "{code} has no meaning for vocab"
            );
        }
    }

    #[test]
    fn the_form_comes_from_from_then_the_extension() {
        let source = |file: &str, from| Source {
            file: file.into(),
            from,
        };
        assert_eq!(form_of(&source("a.oce", None)), Ok(Form::Text));
        assert_eq!(form_of(&source("a.json", None)), Ok(Form::Wire));
        assert_eq!(form_of(&source("a.json", Some(Form::Text))), Ok(Form::Text));
        assert_eq!(form_of(&source("-", Some(Form::Wire))), Ok(Form::Wire));
        assert_eq!(
            form_of(&source("-", None)).map_err(|e| e.0),
            Err(codes::MISSING_FROM)
        );
        for file in ["a.OCE", "a", "a.txt", ".oce", "dir/"] {
            assert_eq!(
                form_of(&source(file, None)).map_err(|e| e.0),
                Err(codes::UNKNOWN_FORM),
                "{file}"
            );
        }
    }

    #[test]
    fn a_bounded_read_accepts_the_bound_and_refuses_one_more() {
        assert_eq!(read_bounded(&[1u8; 8][..], 8).map(|b| b.len()), Ok(8));
        assert_eq!(
            read_bounded(&[1u8; 9][..], 8).map_err(|e| e.0),
            Err(codes::INPUT_TOO_LARGE)
        );
        assert_eq!(read_bounded(&[][..], 8), Ok(Vec::new()));
    }
}
