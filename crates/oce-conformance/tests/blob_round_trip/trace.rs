//! One fixture's run as a comparable record, taken through the facade or through the blob entry,
//! and the bit-for-bit comparison that names the first difference.
//!
//! A [`Trace`] holds what a host can observe of a run: the boundary output paths in slot order,
//! the raw state words after load, and per accepted frame its time bits, sequence, every boundary
//! output, the warnings it emitted in order and the raw state words after it. The facade's words
//! come from `Engine::state_words`, the entry's from `Executable::state_words`; both are the
//! executable's own layout (GitHub #441), not the portable snapshot. [`compare`] walks two traces
//! in a fixed order and returns the first [`Mismatch`], which names the fixture, the frame, the
//! slot or word index, and both bit patterns.

use std::cell::RefCell;
use std::fmt;

use oce_api::{Engine, InputDefinition, Value};
use oce_blocks::Diagnostics;
use oce_graph::blob::{BlobInput, Executable};

use crate::frame_schedule::{RunDigest, TICKS, input_value, time_at};

/// One warning a frame emitted, in emission order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    /// The producer's diagnostic source.
    pub source: String,
    /// The producer's message text.
    pub message: String,
    /// The model time the warning carries, as IEEE-754 bits.
    pub time: u64,
}

/// One accepted frame.
#[derive(Clone, Debug)]
pub struct FrameRecord {
    /// The frame's model time, as IEEE-754 bits.
    pub time: u64,
    /// The accepted-frame sequence number the frame returned.
    pub sequence: u64,
    /// Every boundary output after the frame, in slot order.
    pub outputs: Vec<Value>,
    /// Every warning the frame emitted, in order.
    pub warnings: Vec<Warning>,
    /// The raw state words after the frame.
    pub words: Vec<u64>,
}

/// One fixture's run.
#[derive(Clone, Debug, Default)]
pub struct Trace {
    /// The boundary output paths, in slot order.
    pub outputs: Vec<String>,
    /// The raw state words after load, before any frame.
    pub initial_words: Vec<u64>,
    /// The accepted frames, in order.
    pub frames: Vec<FrameRecord>,
}

/// The labels of the two sides a comparison reads, as the report prints them.
pub type Sides = (&'static str, &'static str);

/// The facade on this target against the blob entry on this target.
pub const FACADE_VS_ENTRY: Sides = ("facade", "entry");
/// A recorded artifact against this target's run.
pub const RECORDED_VS_CURRENT: Sides = ("recorded", "current");
/// The checked-in golden against a freshly encoded or recorded blob.
pub const GOLDEN_VS_ENCODED: Sides = ("golden", "encoded");

/// Where two runs first differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// The blob bytes, at a byte offset (the shorter length when one is a prefix).
    BlobByte { offset: usize },
    /// The number of boundary outputs.
    OutputCount,
    /// A boundary output's path.
    OutputPath { slot: usize },
    /// The number of state words after load.
    InitialWordCount,
    /// A state word after load.
    InitialWord { index: usize },
    /// The number of accepted frames.
    FrameCount,
    /// A frame's model time.
    Time { frame: usize },
    /// A frame's accepted-frame sequence.
    Sequence { frame: usize },
    /// A frame's boundary output count.
    FrameOutputCount { frame: usize },
    /// A boundary output after a frame.
    Output { frame: usize, slot: usize },
    /// The number of warnings a frame emitted.
    WarningCount { frame: usize },
    /// A warning a frame emitted.
    Warning { frame: usize, index: usize },
    /// The number of state words after a frame.
    WordCount { frame: usize },
    /// A state word after a frame.
    Word { frame: usize, index: usize },
}

/// The first difference between two runs of one fixture, with both bit patterns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
    /// The fixture name.
    pub fixture: String,
    /// The labels of the left and right sides.
    pub sides: Sides,
    /// Where they differ.
    pub place: Place,
    /// The boundary output path, when the place is an output slot.
    pub path: Option<String>,
    /// The left side's bit pattern.
    pub left: String,
    /// The right side's bit pattern.
    pub right: String,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fixture {}: ", self.fixture)?;
        match &self.place {
            Place::BlobByte { offset } => write!(f, "blob byte {offset}")?,
            Place::OutputCount => f.write_str("boundary output count")?,
            Place::OutputPath { slot } => write!(f, "output slot {slot} path")?,
            Place::InitialWordCount => f.write_str("state word count after load")?,
            Place::InitialWord { index } => write!(f, "after load, state word {index}")?,
            Place::FrameCount => f.write_str("frame count")?,
            Place::Time { frame } => write!(f, "frame {frame}, time")?,
            Place::Sequence { frame } => write!(f, "frame {frame}, sequence")?,
            Place::FrameOutputCount { frame } => write!(f, "frame {frame}, output count")?,
            Place::Output { frame, slot } => write!(f, "frame {frame}, output slot {slot}")?,
            Place::WarningCount { frame } => write!(f, "frame {frame}, warning count")?,
            Place::Warning { frame, index } => write!(f, "frame {frame}, warning {index}")?,
            Place::WordCount { frame } => write!(f, "frame {frame}, state word count")?,
            Place::Word { frame, index } => write!(f, "frame {frame}, state word {index}")?,
        }
        if let Some(path) = &self.path {
            write!(f, " ({path})")?;
        }
        write!(
            f,
            ": {} {}, {} {}",
            self.sides.0, self.left, self.sides.1, self.right
        )
    }
}

/// A value's tag and exact bits: Reals and Integers as 64-bit hex, never through float
/// formatting.
pub fn value_bits(value: &Value) -> String {
    match value {
        Value::Real(x) => format!("Real 0x{:016x}", x.to_bits()),
        Value::Integer(n) => format!("Integer 0x{:016x}", n.cast_unsigned()),
        Value::Boolean(b) => format!("Boolean {}", u8::from(*b)),
        Value::String(s) => format!("String {s:?}"),
        Value::Enum { class, ordinal } => format!("Enum class {} ordinal {ordinal}", class.0),
    }
}

/// A 64-bit word as hex.
pub fn word_bits(word: u64) -> String {
    format!("0x{word:016x}")
}

fn warning_bits(warning: &Warning) -> String {
    format!(
        "{:?} {:?} t=0x{:016x}",
        warning.source, warning.message, warning.time
    )
}

/// Compares two runs of `fixture` bit for bit, in a fixed order: the output table, the words after
/// load, the frame count, then per frame its time, sequence, outputs, warnings and words.
///
/// # Errors
/// The first [`Mismatch`].
pub fn compare(
    fixture: &str,
    sides: Sides,
    left: &Trace,
    right: &Trace,
) -> Result<(), Box<Mismatch>> {
    let differ = |place: Place, path: Option<&str>, l: String, r: String| {
        Box::new(Mismatch {
            fixture: fixture.to_owned(),
            sides,
            place,
            path: path.map(str::to_owned),
            left: l,
            right: r,
        })
    };
    let count = |n: usize| n.to_string();
    if left.outputs.len() != right.outputs.len() {
        let (l, r) = (count(left.outputs.len()), count(right.outputs.len()));
        return Err(differ(Place::OutputCount, None, l, r));
    }
    for (slot, (l, r)) in left.outputs.iter().zip(&right.outputs).enumerate() {
        if l != r {
            return Err(differ(
                Place::OutputPath { slot },
                None,
                l.clone(),
                r.clone(),
            ));
        }
    }
    compare_words(&left.initial_words, &right.initial_words)
        .map_err(|(place, l, r)| differ(place.after_load(), None, l, r))?;
    if left.frames.len() != right.frames.len() {
        let (l, r) = (count(left.frames.len()), count(right.frames.len()));
        return Err(differ(Place::FrameCount, None, l, r));
    }
    for (frame, (l, r)) in left.frames.iter().zip(&right.frames).enumerate() {
        if l.time != r.time {
            let (lb, rb) = (word_bits(l.time), word_bits(r.time));
            return Err(differ(Place::Time { frame }, None, lb, rb));
        }
        if l.sequence != r.sequence {
            let (ls, rs) = (count_u64(l.sequence), count_u64(r.sequence));
            return Err(differ(Place::Sequence { frame }, None, ls, rs));
        }
        if l.outputs.len() != r.outputs.len() {
            let (ln, rn) = (count(l.outputs.len()), count(r.outputs.len()));
            return Err(differ(Place::FrameOutputCount { frame }, None, ln, rn));
        }
        for (slot, (lv, rv)) in l.outputs.iter().zip(&r.outputs).enumerate() {
            if !lv.bit_eq(rv) {
                let path = left.outputs.get(slot).map(String::as_str);
                let (lb, rb) = (value_bits(lv), value_bits(rv));
                return Err(differ(Place::Output { frame, slot }, path, lb, rb));
            }
        }
        if l.warnings.len() != r.warnings.len() {
            let (ln, rn) = (count(l.warnings.len()), count(r.warnings.len()));
            return Err(differ(Place::WarningCount { frame }, None, ln, rn));
        }
        for (index, (lw, rw)) in l.warnings.iter().zip(&r.warnings).enumerate() {
            if lw != rw {
                let (lb, rb) = (warning_bits(lw), warning_bits(rw));
                return Err(differ(Place::Warning { frame, index }, None, lb, rb));
            }
        }
        compare_words(&l.words, &r.words)
            .map_err(|(place, lb, rb)| differ(place.at_frame(frame), None, lb, rb))?;
    }
    Ok(())
}

fn count_u64(n: u64) -> String {
    n.to_string()
}

/// A word-list difference before it is placed: the count, or the first differing index.
enum WordPlace {
    Count,
    Index(usize),
}

impl WordPlace {
    fn after_load(self) -> Place {
        match self {
            WordPlace::Count => Place::InitialWordCount,
            WordPlace::Index(index) => Place::InitialWord { index },
        }
    }

    fn at_frame(self, frame: usize) -> Place {
        match self {
            WordPlace::Count => Place::WordCount { frame },
            WordPlace::Index(index) => Place::Word { frame, index },
        }
    }
}

fn compare_words(left: &[u64], right: &[u64]) -> Result<(), (WordPlace, String, String)> {
    if left.len() != right.len() {
        return Err((
            WordPlace::Count,
            left.len().to_string(),
            right.len().to_string(),
        ));
    }
    match left.iter().zip(right).position(|(l, r)| l != r) {
        None => Ok(()),
        Some(index) => Err((
            WordPlace::Index(index),
            word_bits(left[index]),
            word_bits(right[index]),
        )),
    }
}

/// Compares two blobs byte for byte.
///
/// # Errors
/// The first differing byte offset, or the shorter length when one blob is a prefix of the other.
pub fn compare_bytes(
    fixture: &str,
    sides: Sides,
    left: &[u8],
    right: &[u8],
) -> Result<(), Box<Mismatch>> {
    let shown = |bytes: &[u8], offset: usize| {
        bytes.get(offset).map_or_else(
            || format!("end of {} bytes", bytes.len()),
            |b| format!("0x{b:02x}"),
        )
    };
    let offset = left
        .iter()
        .zip(right)
        .position(|(l, r)| l != r)
        .or_else(|| (left.len() != right.len()).then(|| left.len().min(right.len())));
    match offset {
        None => Ok(()),
        Some(offset) => Err(Box::new(Mismatch {
            fixture: fixture.to_owned(),
            sides,
            place: Place::BlobByte { offset },
            path: None,
            left: shown(left, offset),
            right: shown(right, offset),
        })),
    }
}

/// A warning sink that records every warning in order.
#[derive(Default)]
struct RecordingSink(RefCell<Vec<Warning>>);

impl Diagnostics for RecordingSink {
    fn warn(&self, source: &str, message: &str, t: f64) {
        self.0.borrow_mut().push(Warning {
            source: source.to_owned(),
            message: message.to_owned(),
            time: t.to_bits(),
        });
    }
}

/// Loads `cxf` through the facade, compiles its blob, and runs the shared frame schedule. Returns
/// the trace, the blob bytes and the facade's input definitions (the entry's frames are checked
/// against them).
///
/// # Errors
/// A load, compile or frame refusal, as text.
pub fn run_facade(cxf: &[u8]) -> Result<(Trace, Vec<u8>, Vec<InputDefinition>), String> {
    let mut engine = Engine::new();
    engine
        .load_cxf(cxf)
        .map_err(|error| format!("load: {error}"))?;
    let blob = engine
        .compile_blob()
        .map_err(|error| format!("compile_blob: {error}"))?
        .bytes;
    let definitions = engine
        .input_definitions()
        .map_err(|error| format!("input definitions: {error}"))?;
    let mut trace = Trace {
        initial_words: engine.state_words().to_vec(),
        ..Trace::default()
    };
    for k in 0..TICKS {
        let time = time_at(k);
        let entries: Vec<(&str, Value)> = definitions
            .iter()
            .enumerate()
            .map(|(slot, d)| {
                let value = input_value(d.value_type, d.min.as_ref(), d.max.as_ref(), slot, k);
                (d.path.as_str(), value)
            })
            .collect();
        let completed = engine
            .prepare_frame(time, &entries)
            .and_then(|plan| engine.execute_frame(plan))
            .map_err(|error| format!("frame {k}: facade: {error}"))?;
        let paths: Vec<&str> = completed
            .outputs()
            .iter()
            .map(|(p, _)| p.as_str())
            .collect();
        if k == 0 {
            trace.outputs = paths.iter().map(|p| (*p).to_owned()).collect();
        } else if paths != trace.outputs {
            return Err(format!("frame {k}: the facade's output paths changed"));
        }
        trace.frames.push(FrameRecord {
            time: time.to_bits(),
            sequence: completed.sequence(),
            outputs: completed.outputs().iter().map(|(_, v)| v.clone()).collect(),
            warnings: completed
                .diagnostics()
                .iter()
                .map(|event| Warning {
                    source: event.block.clone(),
                    message: event.message.clone(),
                    time: event.t.to_bits(),
                })
                .collect(),
            words: engine.state_words().to_vec(),
        });
    }
    Ok((trace, blob, definitions))
}

/// Requires the facade's input definitions and the blob's input table to derive the same frames:
/// same paths, types and bounds, slot for slot.
///
/// # Errors
/// The first slot that differs, as text.
pub fn compare_input_tables(
    definitions: &[InputDefinition],
    inputs: &[BlobInput<'_>],
) -> Result<(), String> {
    if definitions.len() != inputs.len() {
        return Err(format!(
            "input count: facade {}, entry {}",
            definitions.len(),
            inputs.len()
        ));
    }
    let same = |a: Option<&Value>, b: Option<&Value>| match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.bit_eq(b),
        _ => false,
    };
    for (slot, (d, i)) in definitions.iter().zip(inputs).enumerate() {
        if d.path != i.path
            || d.value_type != i.value_type
            || !same(d.min.as_ref(), i.min.as_ref())
            || !same(d.max.as_ref(), i.max.as_ref())
        {
            return Err(format!("input slot {slot} ({}) differs", d.path));
        }
    }
    Ok(())
}

/// Decodes `blob` with the `no_std` execution entry and runs the shared frame schedule, deriving
/// each frame from the blob's own input table as a target would, after requiring that table to
/// match the facade's `definitions`. Returns the trace and the run digest (the
/// `blob_frame_digests.tsv` convention: time, sequence, outputs, every connector value and state
/// word after every frame).
///
/// # Errors
/// A decode or frame refusal, or an input table that differs from `definitions`, as text.
pub fn run_entry(blob: &[u8], definitions: &[InputDefinition]) -> Result<(Trace, String), String> {
    let mut executable = Executable::from_blob(blob).map_err(|error| format!("decode: {error}"))?;
    compare_input_tables(definitions, executable.inputs())?;
    let mut trace = Trace {
        outputs: executable
            .outputs()
            .iter()
            .map(|output| output.path.to_owned())
            .collect(),
        initial_words: executable.state_words().to_vec(),
        frames: Vec::new(),
    };
    let mut digest = RunDigest::default();
    for k in 0..TICKS {
        let time = time_at(k);
        let values: Vec<Value> = executable
            .inputs()
            .iter()
            .enumerate()
            .map(|(slot, i)| input_value(i.value_type, i.min.as_ref(), i.max.as_ref(), slot, k))
            .collect();
        let sink = RecordingSink::default();
        let sequence = executable
            .execute_frame(time, &values, &sink)
            .map_err(|refusal| format!("frame {k}: entry: {refusal}"))?;
        let state = &executable.decoded().state;
        digest.frame(
            time,
            sequence,
            executable.output_values(),
            &state.values,
            &state.words,
        );
        trace.frames.push(FrameRecord {
            time: time.to_bits(),
            sequence,
            outputs: executable.output_values().cloned().collect(),
            warnings: sink.0.into_inner(),
            words: executable.state_words().to_vec(),
        });
    }
    Ok((trace, digest.hex()))
}
