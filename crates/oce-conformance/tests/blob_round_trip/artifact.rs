//! The blob round trip's recorded artifact: its writer and its bounded, typed reader.
//!
//! The format (version 1) is documented in the suite's module header and in
//! `docs/compiled-schedule-blob.md`. The reader treats the artifact as hostile: it checks a file's
//! length against a limit before reading it, reads at most that many bytes, requires UTF-8 and the
//! exact line grammar, checks every declared count against the lines that follow, and refuses
//! anything else with an [`ArtifactError`]. It never panics, and what it holds is linear in the
//! bytes it read.

use std::fmt;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use oce_api::Value;
use oce_model::EnumClassId;

use crate::trace::{FrameRecord, Trace, Warning};

/// The first line of every trace file.
pub const MAGIC: &str = "oce-blob-round-trip";
/// The format version this reader accepts, and the only one.
pub const VERSION: u32 = 1;
/// The largest trace file the reader opens.
pub const MAX_TRACE_BYTES: u64 = 32 << 20;
/// The largest blob file the reader opens (a G36 golden is at most 17,001 bytes).
pub const MAX_BLOB_BYTES: u64 = 1 << 20;
/// The most boundary outputs a trace may declare.
pub const MAX_OUTPUTS: usize = 1 << 16;
/// The most state words a trace may declare (the engine's own state budget).
pub const MAX_WORDS: usize = 4_194_304;
/// The most frames a trace may hold.
pub const MAX_FRAMES: usize = 1 << 16;

/// Why a recorded artifact was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactError {
    /// A file could not be opened or read.
    Io { path: String, detail: String },
    /// A file the build's fixture list requires is absent.
    Missing { path: String },
    /// The directory holds a file no fixture of this build names.
    Unexpected { name: String },
    /// A file is longer than its limit; it was not read.
    TooLarge { path: String, limit: u64 },
    /// A trace file breaks the grammar at a line (1-based).
    Malformed {
        path: String,
        line: usize,
        reason: String,
    },
    /// The blob file's length or FNV-1a-64 differs from what its trace header records.
    BlobHeader {
        fixture: String,
        recorded: String,
        actual: String,
    },
}

impl fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, detail } => write!(f, "{path}: {detail}"),
            Self::Missing { path } => write!(f, "{path}: missing from the recorded artifact"),
            Self::Unexpected { name } => {
                write!(f, "{name}: not a file of any fixture in this build")
            }
            Self::TooLarge { path, limit } => write!(f, "{path}: longer than {limit} bytes"),
            Self::Malformed { path, line, reason } => write!(f, "{path}:{line}: {reason}"),
            Self::BlobHeader {
                fixture,
                recorded,
                actual,
            } => write!(
                f,
                "{fixture}: the trace header records blob {recorded}, the blob file is {actual}"
            ),
        }
    }
}

/// One fixture's recorded blob and trace.
#[derive(Clone, Debug)]
pub struct Recorded {
    /// The fixture name.
    pub fixture: String,
    /// The blob bytes the recording target encoded.
    pub blob: Vec<u8>,
    /// The run the recording target observed.
    pub trace: Trace,
}

/// FNV-1a-64 over `bytes`, as sixteen hex digits.
pub fn fnv1a64(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The blob file name of `fixture`.
pub fn blob_name(fixture: &str) -> String {
    format!("{fixture}.oceb")
}

/// The trace file name of `fixture`.
pub fn trace_name(fixture: &str) -> String {
    format!("{fixture}.trace.tsv")
}

fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
}

fn render_value(value: &Value, out: &mut String) {
    match value {
        Value::Real(x) => out.push_str(&format!("r:{:016x}", x.to_bits())),
        Value::Integer(n) => out.push_str(&format!("i:{n}")),
        Value::Boolean(b) => out.push_str(if *b { "b:1" } else { "b:0" }),
        Value::String(s) => {
            out.push_str("s:");
            escape(s, out);
        }
        Value::Enum { class, ordinal } => out.push_str(&format!("e:{}:{ordinal}", class.0)),
    }
}

fn render_words(tag: &str, words: &[u64], out: &mut String) {
    out.push_str(tag);
    for word in words {
        out.push_str(&format!("\t{word:016x}"));
    }
    out.push('\n');
}

/// Renders one fixture's trace file: deterministic, one record per line, fields separated by a
/// tab, every line ending in a newline.
pub fn render(fixture: &str, blob: &[u8], trace: &Trace) -> String {
    let mut out = format!("{MAGIC}\t{VERSION}\nfixture\t");
    escape(fixture, &mut out);
    out.push_str(&format!(
        "\nblob\t{}\t{}\noutputs\t{}\n",
        blob.len(),
        fnv1a64(blob),
        trace.outputs.len()
    ));
    for (slot, path) in trace.outputs.iter().enumerate() {
        out.push_str(&format!("output\t{slot}\t"));
        escape(path, &mut out);
        out.push('\n');
    }
    out.push_str(&format!("words\t{}\n", trace.initial_words.len()));
    render_words("load", &trace.initial_words, &mut out);
    for (k, frame) in trace.frames.iter().enumerate() {
        out.push_str(&format!(
            "frame\t{k}\t{:016x}\t{}\nout",
            frame.time, frame.sequence
        ));
        for value in &frame.outputs {
            out.push('\t');
            render_value(value, &mut out);
        }
        out.push('\n');
        for warning in &frame.warnings {
            out.push_str("warn\t");
            escape(&warning.source, &mut out);
            out.push('\t');
            escape(&warning.message, &mut out);
            out.push_str(&format!("\t{:016x}\n", warning.time));
        }
        render_words("state", &frame.words, &mut out);
    }
    out.push_str(&format!("end\t{}\n", trace.frames.len()));
    out
}

/// A parse failure before it is given its file: the 1-based line and the reason.
type LineError = (usize, String);

fn unescape(field: &str) -> Result<String, String> {
    let mut out = String::with_capacity(field.len());
    let mut chars = field.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            if (c as u32) < 0x20 || c == '\u{7f}' {
                return Err("an unescaped control character".into());
            }
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                let code = u8::from_str_radix(&hex, 16)
                    .ok()
                    .filter(|_| hex.len() == 2 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
                    .ok_or("a malformed \\x escape")?;
                out.push(char::from(code));
            }
            _ => return Err("an unknown escape".into()),
        }
    }
    Ok(out)
}

/// Exactly sixteen lowercase hex digits.
fn hex64(field: &str) -> Result<u64, String> {
    if field.len() != 16
        || !field
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(format!("{field:?} is not sixteen lowercase hex digits"));
    }
    u64::from_str_radix(field, 16).map_err(|error| error.to_string())
}

/// A canonical unsigned decimal: digits only, no sign, no leading zero.
fn decimal<T: std::str::FromStr>(field: &str) -> Result<T, String> {
    let canonical = !field.is_empty()
        && field.bytes().all(|b| b.is_ascii_digit())
        && (field == "0" || !field.starts_with('0'));
    if !canonical {
        return Err(format!("{field:?} is not a canonical unsigned decimal"));
    }
    field
        .parse()
        .map_err(|_| format!("{field:?} is out of range"))
}

fn parse_value(field: &str) -> Result<Value, String> {
    let (tag, body) = field
        .split_once(':')
        .ok_or_else(|| format!("{field:?} has no type tag"))?;
    match tag {
        "r" => Ok(Value::Real(f64::from_bits(hex64(body)?))),
        "i" => {
            let (negative, digits) = body
                .strip_prefix('-')
                .map_or((false, body), |rest| (true, rest));
            let magnitude: u64 = decimal(digits)?;
            let value = if negative {
                0_i64
                    .checked_sub_unsigned(magnitude)
                    .filter(|_| magnitude != 0)
            } else {
                i64::try_from(magnitude).ok()
            };
            value
                .map(Value::Integer)
                .ok_or_else(|| format!("{field:?} is not a canonical i64"))
        }
        "b" => match body {
            "0" => Ok(Value::Boolean(false)),
            "1" => Ok(Value::Boolean(true)),
            _ => Err(format!("{field:?} is not b:0 or b:1")),
        },
        "s" => Ok(Value::String(unescape(body)?.into())),
        "e" => {
            let (class, ordinal) = body
                .split_once(':')
                .ok_or_else(|| format!("{field:?} is not e:<class>:<ordinal>"))?;
            Ok(Value::Enum {
                class: EnumClassId(decimal(class)?),
                ordinal: decimal(ordinal)?,
            })
        }
        _ => Err(format!("{field:?} has an unknown type tag")),
    }
}

/// The lines of a trace file, each split into its tag and fields, with a 1-based line number.
struct Lines<'t> {
    lines: std::iter::Enumerate<std::str::Split<'t, char>>,
    last: usize,
}

impl<'t> Lines<'t> {
    fn new(text: &'t str) -> Result<Self, LineError> {
        let body = text
            .strip_suffix('\n')
            .ok_or((1, "the file does not end in a newline".to_owned()))?;
        Ok(Self {
            lines: body.split('\n').enumerate(),
            last: 0,
        })
    }

    /// The next line, whose tag must be `tag`; its fields after the tag.
    fn expect(&mut self, tag: &str) -> Result<(usize, Vec<&'t str>), LineError> {
        let (index, line) = self.lines.next().ok_or((
            self.last + 1,
            format!("the file ends before a {tag:?} line"),
        ))?;
        self.last = index + 1;
        let mut fields = line.split('\t');
        let found = fields.next().unwrap_or_default();
        if found != tag {
            return Err((
                self.last,
                format!("expected a {tag:?} line, found {found:?}"),
            ));
        }
        Ok((self.last, fields.collect()))
    }

    /// The tag of the next line, without consuming it.
    fn peek_tag(&self) -> Option<&'t str> {
        self.lines
            .clone()
            .next()
            .map(|(_, line)| line.split('\t').next().unwrap_or_default())
    }

    /// Like [`Self::expect`], requiring exactly `n` fields.
    fn fields(&mut self, tag: &str, n: usize) -> Result<(usize, Vec<&'t str>), LineError> {
        let (line, fields) = self.expect(tag)?;
        if fields.len() != n {
            return Err((
                line,
                format!("a {tag:?} line has {} fields, not {n}", fields.len()),
            ));
        }
        Ok((line, fields))
    }
}

fn at<T>(line: usize, result: Result<T, String>) -> Result<T, LineError> {
    result.map_err(|reason| (line, reason))
}

fn bounded(line: usize, what: &str, n: usize, max: usize) -> Result<usize, LineError> {
    if n > max {
        return Err((line, format!("{n} {what} exceeds the limit {max}")));
    }
    Ok(n)
}

fn words(line: usize, fields: &[&str], n: usize) -> Result<Vec<u64>, LineError> {
    if fields.len() != n {
        return Err((line, format!("{} state words, not {n}", fields.len())));
    }
    fields.iter().map(|field| at(line, hex64(field))).collect()
}

/// Parses one trace file. `fixture` is the name its file belongs to; the header must agree.
///
/// # Errors
/// The first line that breaks the grammar, with the reason.
fn parse_lines(fixture: &str, text: &str) -> Result<(usize, String, Trace), LineError> {
    let mut lines = Lines::new(text)?;
    let (line, header) = lines.fields(MAGIC, 1)?;
    if header[0] != VERSION.to_string() {
        return Err((
            line,
            format!("format version {:?}, not {VERSION}", header[0]),
        ));
    }
    let (line, name) = lines.fields("fixture", 1)?;
    if at(line, unescape(name[0]))? != fixture {
        return Err((
            line,
            format!("the header names {:?}, not {fixture:?}", name[0]),
        ));
    }
    let (line, blob) = lines.fields("blob", 2)?;
    let blob_len: usize = at(line, decimal(blob[0]))?;
    let blob_fnv = format!("{:016x}", at(line, hex64(blob[1]))?);
    let (line, count) = lines.fields("outputs", 1)?;
    let n_outputs = bounded(line, "outputs", at(line, decimal(count[0]))?, MAX_OUTPUTS)?;
    let mut trace = Trace::default();
    for slot in 0..n_outputs {
        let (line, output) = lines.fields("output", 2)?;
        if at(line, decimal::<usize>(output[0]))? != slot {
            return Err((
                line,
                format!("output slot {:?}, expected {slot}", output[0]),
            ));
        }
        trace.outputs.push(at(line, unescape(output[1]))?);
    }
    let (line, count) = lines.fields("words", 1)?;
    let n_words = bounded(line, "state words", at(line, decimal(count[0]))?, MAX_WORDS)?;
    let (line, load) = lines.expect("load")?;
    trace.initial_words = words(line, &load, n_words)?;
    while lines.peek_tag() == Some("frame") {
        let (line, frame) = lines.fields("frame", 3)?;
        let k = trace.frames.len();
        bounded(line, "frames", k + 1, MAX_FRAMES)?;
        if at(line, decimal::<usize>(frame[0]))? != k {
            return Err((line, format!("frame {:?}, expected {k}", frame[0])));
        }
        let time = at(line, hex64(frame[1]))?;
        let sequence = at(line, decimal(frame[2]))?;
        let (line, out) = lines.expect("out")?;
        if out.len() != n_outputs {
            return Err((line, format!("{} outputs, not {n_outputs}", out.len())));
        }
        let outputs = out
            .iter()
            .map(|field| at(line, parse_value(field)))
            .collect::<Result<_, _>>()?;
        let mut warnings = Vec::new();
        while lines.peek_tag() == Some("warn") {
            let (line, warn) = lines.fields("warn", 3)?;
            warnings.push(Warning {
                source: at(line, unescape(warn[0]))?,
                message: at(line, unescape(warn[1]))?,
                time: at(line, hex64(warn[2]))?,
            });
        }
        let (line, state) = lines.expect("state")?;
        trace.frames.push(FrameRecord {
            time,
            sequence,
            outputs,
            warnings,
            words: words(line, &state, n_words)?,
        });
    }
    let (line, end) = lines.fields("end", 1)?;
    if at(line, decimal::<usize>(end[0]))? != trace.frames.len() {
        return Err((
            line,
            format!("end {:?} after {} frames", end[0], trace.frames.len()),
        ));
    }
    if lines.peek_tag().is_some() {
        return Err((line + 1, "a line after the end line".into()));
    }
    Ok((blob_len, blob_fnv, trace))
}

/// Parses one fixture's trace file bytes and checks `blob` against its header.
///
/// # Errors
/// [`ArtifactError::Malformed`] for a grammar break or invalid UTF-8, and
/// [`ArtifactError::BlobHeader`] when the blob's length or hash differs from the header.
pub fn parse(
    fixture: &str,
    path: &str,
    bytes: &[u8],
    blob: Vec<u8>,
) -> Result<Recorded, ArtifactError> {
    let malformed = |(line, reason): LineError| ArtifactError::Malformed {
        path: path.to_owned(),
        line,
        reason,
    };
    let text = std::str::from_utf8(bytes)
        .map_err(|error| malformed((1, format!("not UTF-8: {error}"))))?;
    let (blob_len, blob_fnv, trace) = parse_lines(fixture, text).map_err(malformed)?;
    let actual = (blob.len(), fnv1a64(&blob));
    if (blob_len, blob_fnv.as_str()) != (actual.0, actual.1.as_str()) {
        return Err(ArtifactError::BlobHeader {
            fixture: fixture.to_owned(),
            recorded: format!("{blob_len} bytes fnv1a64 {blob_fnv}"),
            actual: format!("{} bytes fnv1a64 {}", actual.0, actual.1),
        });
    }
    Ok(Recorded {
        fixture: fixture.to_owned(),
        blob,
        trace,
    })
}

/// Reads a file of at most `limit` bytes, refusing a longer one before reading it.
///
/// # Errors
/// [`ArtifactError::Missing`], [`ArtifactError::TooLarge`] or [`ArtifactError::Io`].
pub fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, ArtifactError> {
    let shown = path.display().to_string();
    let io = |error: std::io::Error| ArtifactError::Io {
        path: shown.clone(),
        detail: error.to_string(),
    };
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ArtifactError::Missing { path: shown });
        }
        Err(error) => return Err(io(error)),
    };
    let too_large = || ArtifactError::TooLarge {
        path: shown.clone(),
        limit,
    };
    if file.metadata().map_err(io)?.len() > limit {
        return Err(too_large());
    }
    // The length can change after the check; never read more than one byte past the limit.
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).map_err(io)?;
    if bytes.len() as u64 > limit {
        return Err(too_large());
    }
    Ok(bytes)
}

/// Reads the recorded artifact in `dir` for every fixture in `fixtures`, refusing any other file.
///
/// # Errors
/// The first [`ArtifactError`].
pub fn read_dir(dir: &Path, fixtures: &[String]) -> Result<Vec<Recorded>, ArtifactError> {
    let shown = dir.display().to_string();
    let io = |error: std::io::Error| ArtifactError::Io {
        path: shown.clone(),
        detail: error.to_string(),
    };
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(io)? {
        let name = entry.map_err(io)?.file_name();
        names.push(name.to_string_lossy().into_owned());
    }
    names.sort_unstable();
    for name in &names {
        let known = fixtures
            .iter()
            .any(|fixture| *name == blob_name(fixture) || *name == trace_name(fixture));
        if !known {
            return Err(ArtifactError::Unexpected { name: name.clone() });
        }
    }
    fixtures
        .iter()
        .map(|fixture| {
            let blob = read_bounded(&dir.join(blob_name(fixture)), MAX_BLOB_BYTES)?;
            let path = dir.join(trace_name(fixture));
            let bytes = read_bounded(&path, MAX_TRACE_BYTES)?;
            parse(fixture, &path.display().to_string(), &bytes, blob)
        })
        .collect()
}

/// Writes one fixture's blob and trace files into `dir`.
///
/// # Errors
/// The I/O error, as text.
pub fn write(dir: &Path, fixture: &str, blob: &[u8], trace: &Trace) -> Result<(), String> {
    let write = |path: PathBuf, bytes: &[u8]| {
        std::fs::write(&path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
    };
    write(dir.join(blob_name(fixture)), blob)?;
    write(
        dir.join(trace_name(fixture)),
        render(fixture, blob, trace).as_bytes(),
    )
}
