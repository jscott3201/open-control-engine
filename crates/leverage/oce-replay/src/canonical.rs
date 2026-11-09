//! `Trace::canonical_bytes()` (decision 23): a fixed little-endian encoding for goldens, the
//! chunk-size test and hashing. It is not a published interchange format.
//!
//! Layout, every integer little-endian, every count and length a `u64`, every string a `u64`
//! byte length then UTF-8, every `f64` its raw bits as `u64`:
//!
//! ```text
//! magic "OCETRACE", u32 version 1
//! inputs:  count; per input: path, u8 kind, enum class (string, empty when none), policy
//!          policy: u8 tag (0 Refuse, 1 HoldLast, 2 Fallback, 3 SkipFrame), then
//!                  HoldLast: f64 max_age; Fallback: value
//!          value: u8 kind tag (0 Real, 1 Integer, 2 Boolean, 3 Enum), then 8 bytes:
//!                 Real bits, Integer i64, Boolean 0/1 as u64, Enum ordinal as u64
//! outputs: count; per output: path, u8 kind, u8 has enum class index, u32 index (0 when none)
//! options: u8 time base (0 Recorded, 1 Grid), f64 t0, f64 step (0 bits when Recorded),
//!          u8 duplicate (0 Refuse, 1 Allow), u8 on refusal (0 Stop, 1 RecordAndContinue),
//!          u8 non-finite (0 Admit, 1 Refuse, 255 other), u8 retains inputs,
//!          u8 has snapshot_every, u64 snapshot_every (0 when none), snapshot_at count + u64s,
//!          limits: max_rows, max_columns, max_cell_bytes, max_source_bytes, max_steps,
//!                  max_frames, max_trace_bytes, each u64,
//!          u8 has origin, origin string (empty when none), ignored columns count + strings
//! rows:    count; times (f64 each); status (u8 each); per output its cells (width each);
//!          u8 has mask, then mask bytes (rows × ceil(N_in / 8)); u8 has inputs, then per input
//!          its cells
//! diagnostics: count; per row: u64 frame, f64 time, u8 level (0 Warning), block, message
//! refusals:    count; per row: u64 frame, u8 cause (0 MissingInput, 1 Engine), then
//!              MissingInput: u64 slot, path; Engine: message
//! snapshots:   count; per snapshot: u64 frame, u64 length, the snapshot's bytes
//! ```

use oce_api::{AssertLevel, NonFiniteInputPolicy, Value};

use crate::{DuplicateTime, MissingPolicy, OnRefusal, RefusalCause, TimeBase, Trace, TraceColumn};

/// The encoding's magic.
pub const TRACE_MAGIC: &[u8; 8] = b"OCETRACE";
/// The encoding's version.
pub const TRACE_VERSION: u32 = 1;

struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.u64(v.to_bits());
    }
    fn len(&mut self, n: usize) {
        self.u64(n as u64);
    }
    fn str(&mut self, s: &str) {
        self.len(s.len());
        self.0.extend_from_slice(s.as_bytes());
    }
    fn flag(&mut self, b: bool) {
        self.u8(u8::from(b));
    }
    fn value(&mut self, value: &Value) {
        match value {
            Value::Real(x) => {
                self.u8(0);
                self.f64(*x);
            }
            Value::Integer(x) => {
                self.u8(1);
                self.0.extend_from_slice(&x.to_le_bytes());
            }
            Value::Boolean(x) => {
                self.u8(2);
                self.u64(u64::from(*x));
            }
            Value::Enum { ordinal, .. } => {
                self.u8(3);
                self.u64(u64::from(*ordinal));
            }
            // A String is never a fallback (the map refuses it); encode its length as a marker.
            Value::String(s) => {
                self.u8(4);
                self.len(s.len());
            }
        }
    }
    fn column(&mut self, column: &TraceColumn) {
        match column {
            TraceColumn::Real(v) => v.iter().for_each(|x| self.f64(*x)),
            TraceColumn::Integer(v) => v
                .iter()
                .for_each(|x| self.0.extend_from_slice(&x.to_le_bytes())),
            TraceColumn::Boolean(v) => self.0.extend_from_slice(v),
            TraceColumn::Enum(v) => v.iter().for_each(|x| self.u32(*x)),
        }
    }
}

fn non_finite_tag(policy: NonFiniteInputPolicy) -> u8 {
    match policy {
        NonFiniteInputPolicy::Admit => 0,
        NonFiniteInputPolicy::Refuse => 1,
        _ => 255,
    }
}

pub(crate) fn encode(trace: &Trace) -> Vec<u8> {
    let meta = &trace.meta;
    let mut w = Writer(Vec::with_capacity(
        usize::try_from(trace.byte_size()).unwrap_or(0) + 4096,
    ));
    w.0.extend_from_slice(TRACE_MAGIC);
    w.u32(TRACE_VERSION);
    w.len(meta.inputs.len());
    for input in &meta.inputs {
        w.str(&input.path);
        w.u8(input.kind.tag());
        w.str(input.enum_class.as_deref().unwrap_or(""));
        match &input.policy {
            MissingPolicy::Refuse => w.u8(0),
            MissingPolicy::HoldLast { max_age } => {
                w.u8(1);
                w.f64(*max_age);
            }
            MissingPolicy::Fallback(value) => {
                w.u8(2);
                w.value(value);
            }
            MissingPolicy::SkipFrame => w.u8(3),
        }
    }
    w.len(meta.outputs.len());
    for output in &meta.outputs {
        w.str(&output.path);
        w.u8(output.kind.tag());
        w.flag(output.enum_class_index.is_some());
        w.u32(output.enum_class_index.unwrap_or(0));
    }
    match meta.time {
        TimeBase::Recorded => {
            w.u8(0);
            w.u64(0);
            w.u64(0);
        }
        TimeBase::Grid { t0, step } => {
            w.u8(1);
            w.f64(t0);
            w.f64(step);
        }
    }
    w.flag(meta.duplicate_time == DuplicateTime::Allow);
    w.flag(meta.on_refusal == OnRefusal::RecordAndContinue);
    w.u8(non_finite_tag(meta.non_finite));
    w.flag(meta.retains_inputs);
    w.flag(meta.snapshot_every.is_some());
    w.u64(meta.snapshot_every.unwrap_or(0));
    w.len(meta.snapshot_at.len());
    meta.snapshot_at.iter().for_each(|&r| w.u64(r));
    let l = meta.limits;
    for v in [
        l.max_rows,
        u64::from(l.max_columns),
        u64::from(l.max_cell_bytes),
        l.max_source_bytes,
        l.max_steps,
        l.max_frames,
        l.max_trace_bytes,
    ] {
        w.u64(v);
    }
    w.flag(meta.origin.is_some());
    w.str(meta.origin.as_ref().map_or("", |o| o.as_str()));
    w.len(meta.ignored_columns.len());
    meta.ignored_columns.iter().for_each(|c| w.str(c));

    w.len(trace.times.len());
    trace.times.iter().for_each(|t| w.f64(*t));
    w.0.extend_from_slice(&trace.status);
    trace.outputs.iter().for_each(|(_, c)| w.column(c));
    w.flag(trace.mask.is_some());
    if let Some(mask) = &trace.mask {
        w.0.extend_from_slice(mask);
    }
    w.flag(trace.inputs.is_some());
    if let Some(inputs) = &trace.inputs {
        inputs.iter().for_each(|(_, c)| w.column(c));
    }

    w.len(trace.diagnostics.len());
    for row in &trace.diagnostics {
        w.u64(row.frame);
        w.f64(row.time);
        w.u8(match row.level {
            AssertLevel::Warning => 0,
        });
        w.str(&row.block);
        w.str(&row.message);
    }
    w.len(trace.refusals.len());
    for row in &trace.refusals {
        w.u64(row.frame);
        match &row.cause {
            RefusalCause::MissingInput { slot, path } => {
                w.u8(0);
                w.len(*slot);
                w.str(path);
            }
            RefusalCause::Engine { message } => {
                w.u8(1);
                w.str(message);
            }
        }
    }
    w.len(trace.snapshots.len());
    for (frame, snapshot) in &trace.snapshots {
        w.u64(*frame);
        let bytes = snapshot.as_bytes();
        w.len(bytes.len());
        w.0.extend_from_slice(bytes);
    }
    w.0
}

impl Trace {
    /// The fixed little-endian encoding of the whole trace documented in [`crate::canonical`]:
    /// metadata, then every column in order with raw bits, then the sparse tables and snapshots.
    /// Two runs with the same source, options and engine give identical bytes whatever their
    /// chunk sizes. Allocates the returned buffer.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        encode(self)
    }
}
