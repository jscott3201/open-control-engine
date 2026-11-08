# Historical replay API

**Design note, written before any replay code.** It fixes how recorded or scripted inputs drive the
engine frame by frame, what happens when a sample is missing, what a run leaves behind, and how a
long run yields to a Python interpreter or a browser event loop. Decisions 1 to 10 were settled by
the owner (1 to 4) or accepted from the prior-art review (5 to 10) and are stated here, not
reopened. Decisions 11 to 23 were made while drafting and ruled on by the architect pass (see
[Review rulings](#review-rulings)). It answers GitHub #435; #445 builds the crate and #444 adds the
facade entry it moves to after #401.

## What the replay API is, and is not

It is one **leverage crate**, `crates/leverage/oce-replay` (see `crates/leverage/README.md`): it
sits above the facade and depends on `oce-api` only. It holds four things and nothing else:

- **Frame sources**: slot-indexed frames `(t, &[Value])` after a one-time name→slot mapping, and
  three adapters (columnar arrays, a bounded CSV with a column map, step lists).
- **Missing-sample policy**: `MissingPolicy`, `OnRefusal`, `DuplicateTime` and the time base.
- **The trace**: a columnar, typed, bit-exact record of a run.
- **A resumable cursor**: `Replay::advance(max_frames)`, with a blocking `run` built on it.

Its consumers are `oce-verify` (M3), `oce-wasm` (M4), `oce-py` (M5), `oce-host` (M6), the FDD
runner, the commissioning verifier (#436) and the benchmark.

It is **not** engine policy. The engine keeps its contract unchanged: the
[complete-frame contract](../complete-frame-contract.md) says a missing required input is refused
with "no hold-last, zero/false seed or stored-sample fill-in", and
[host responsibilities](../host-responsibilities.md) say "the engine neither invents missing data
nor qualifies a host's substitution policy". Replay is that host policy applied offline, made
explicit and written down frame by frame. It adds nothing to `oce-api`, it runs no thread, async
runtime, clock, file system walk or network listener, and it never touches
[the §7.17 seam](../architecture.md#the-organising-idea-the-cdl-717-non-computational-seam). Reading
a CSV file from a path is the caller's job; the CSV adapter takes a byte reader.

## Frame sources

**One shape, mapped once.** Every source yields frames **by slot**: a time and one cell per engine
input, in the order of `Engine::input_definitions()`, which is canonical lexical UTF-8 path order.
Names are resolved exactly once, when the run is built, into an `InputMap`; after that no frame
touches a string. The compiled-schedule blob lists its inputs "strictly increasing by path bytes"
and says "the host resolves names to slots once, at startup, and indexes thereafter", so the same
`InputMap` indexes the facade today and #401's slot-indexed driver later (decision 4).

`InputMap::resolve` takes the engine's input definitions, the source's column names and a
`ColumnMap` (source column → engine input). A map target is a canonical input path or a suffix that
matches exactly one path at a `.` boundary; an ambiguous suffix, an unknown target, two columns on
one input, a column whose type cannot carry the input's `ValueType`, or an engine input no column
supplies is a typed error at construction. Source columns the map does not name are ignored and
listed in the trace metadata.

**A source says "missing" with `None`.** The engine gets the owner's `(t, &[Value])` shape; one
level down (decision 13), a source yields `(t, &[Option<Value>])`. The policy stage
(next section) turns that into the complete `(t, &[Value])` the engine receives, or into no frame
at all. NaN in a Real column, NaT in a time or datetime column, a blank CSV cell and a cleared
validity bit are all `None`, never a value, whatever `NonFiniteInputPolicy` says (decision 2).

| Adapter | Input | Missing marker | Typical caller |
| --- | --- | --- | --- |
| `ColumnarSource` | one time column and one owned typed column per input (numpy or pandas shape) | NaN (Real); validity mask (Integer, Boolean, Enum) | `oce-py`, `oce-wasm`, the benchmark |
| `CsvSource` | a byte reader, a header row, a time column and a `ColumnMap` | empty cell or a configured token (`NaN`, `NA`) | `oce-verify`, `oce-host`, #436 |
| `StepSource` | per input, a list of `(t, value)` changes; a constant is a one-step list | none: a step list defines a value on every interval it covers | `oce-verify` vector format v2 |

The columnar adapter owns its columns. Python and JavaScript copy their arrays in before the run, so
no foreign buffer is borrowed across a released lock (a day of 14 inputs at 5 s is under 2 MB).

**Time base.** A run declares one of two (decision 5):

- `TimeBase::Recorded`: one frame per source row, at the row's time. Sample times pass through
  unchanged; irregular steps are visible to timers and integrators, as they were in the field.
- `TimeBase::Grid { t0, step }`: **sampling** is zero-order hold at `t0 + (k as f64) * step`. Frame
  `k` is at that time, computed by multiply, never by accumulating `step`, and each input takes
  the latest sample at or before it. Ticks run to the last sample time unless
  `ReplayLimits::max_frames` stops them first.

Replay resamples only its own inputs, only in `Grid` mode. When a reference series and a run have
different time bases, the consumer (the verifier) resamples; the replay crate never does.

**Time origin.** Engine time is model seconds from `t0`. A source may carry a wall-clock origin, an
ISO-8601 instant with an explicit offset, as metadata. Converting source timestamps to model
seconds is the adapter's job (decision 17); the trace keeps the origin once in its header, never
per frame, and the engine never sees it.

Rows must have nondecreasing time; a decrease is a typed `SourceError::TimeRegression` before the
engine sees anything. A row whose time is missing is refused, never substituted. Equal times follow
the `DuplicateTime` policy: `Refuse` by default, `Allow` as an explicit opt-in. The contract says an
equal timestamp "is neither a read nor an idempotent retry": it is a legal HostTick transition with
zero elapsed time, and CDL §12 verification data can carry equal-time samples. Under `Allow` each
equal-time row is its own frame and its own trace row; replay never collapses rows.

**Bounds.** Every adapter is bounded at construction or while reading, with a typed error and never
a panic: rows, columns, bytes, cell length and step-list length (decision 8; numbers, decision 19).
The row and column caps are documented per source, because the envelope comparison downstream is
quadratic in rows; overflow is `SourceError::TooManyRows` or `TooManyColumns`, never truncation.

## Missing samples

The engine requires every input on every frame. Replay therefore decides, per input and per frame,
what an absent sample becomes, and records the decision (decision 2):

| `MissingPolicy` | The frame gets | Recorded as |
| --- | --- | --- |
| `Refuse` (**default**) | nothing: the frame is refused by replay before the engine | status `RefusedMissing`, a refusal row naming the first missing input |
| `HoldLast { max_age }` | the last *observed* value, if `t - t_sample <= max_age`; otherwise as `Refuse` | status `Substituted`, the input's bit in the substitution mask |
| `Fallback(Value)` | a value the caller declared for that input, type and domain checked at construction | status `Substituted`, the input's bit in the substitution mask |
| `SkipFrame` | nothing: the frame is not submitted and the run continues | status `Skipped` |

`OnRefusal` decides what a refused frame does to the run: `Stop` (**default**) ends the run with
`Progress::Stopped`; `RecordAndContinue` writes the row and moves on. It covers both replay
refusals (missing input) and engine refusals (`OcError` from preparation or execution, such as a
domain violation). Continuing is safe because refusal is atomic. The contract requires that "for
every ordinary refused frame, the entire observable engine execution/replay image MUST remain
unchanged": no time advance, no state change, no sequence number. Nothing needs rolling back.

A hold never holds a substituted value: `HoldLast` reads the last sample the source observed and
measures age from that sample's time. `SkipFrame` changes the state trajectory (the next executed
frame sees a longer step), and `HoldLast` and `Fallback` feed the engine a value no sensor reported
at that instant, so all three are opt-ins and a trace with any non-`Executed` row says so
(`Trace::is_clean()` is false). The policy is per run, with per-input overrides resolved in the
`InputMap`; `Fallback` exists only as an override, because one value cannot fit every input type.

Under `Grid`, a sample is current for the interval `(tick - step, tick]` it arrived in. Carrying it
further is a hold, governed by `MissingPolicy` like any other (decision 11), so a change-of-value
trend log replays on a grid only with an explicit `HoldLast { max_age }`. A step list is a signal,
not a sample: its value is defined on every interval it covers and is never missing after its
first step.

## Trace

The trace is **columnar and typed, with raw bits** (decision 3). One row per attempted frame:

| Column | Type | Width (B) | Present |
| --- | --- | ---: | --- |
| time | `f64`, raw bits kept | 8 | always |
| status | `u8`: 0 `Executed`, 1 `Substituted`, 2 `Skipped`, 3 `RefusedMissing`, 4 `RefusedEngine` | 1 | always |
| one per output | Real `f64` bits, Integer `i64`, Boolean `u8` (0/1), Enum `u32` ordinal | 8 / 8 / 1 / 4 | always |
| substitution mask | one bit per input slot | `ceil(N_in / 8)` | when any input's policy can substitute |
| one per input | as outputs | as outputs | opt-in (`TraceOptions::inputs`) |

Beside the rows: **sparse tables** for diagnostics (`frame`, `block`, `message`, time bits, level;
the engine emits Warning only) and refusals (`frame`, `RefusalCause`); **state snapshots**, opt-in,
every N frames or at named frame indices, each an `EngineStateSnapshot` taken after that frame;
and **metadata**: input and output paths and types in slot order, enum class paths per Enum column,
the options and limits of the run, the non-finite policy in force, and the ignored source columns.

Output columns come from the first accepted frame: the facade lists outputs on a `CompletedFrame`,
not before ("inventory and host-selected trace columns are not the authority"). Every later frame's
output paths are checked against them; a difference is a typed stop. Rows that did not execute
carry zero bits in the output columns and are masked by their status; `to_pandas` writes a pandas
NA there, never a number.

**Indicators are not the trace's concern.** Indicator and mask signals (when a comparison applies)
are derived by the verifier from the trace's output columns and, where needed, its opt-in input
columns. That is why the trace must be able to retain inputs; replay computes no indicator itself.

**Why not one `ReplayRecord` per frame.** A [replay record](../replay-record.md) is "one accepted
frame record, not a sequence": it repeats the compatibility descriptor (up to 1,024 B) and every
input and output path in every frame. For the controller fixture below that is 1,855 B per frame
plus the descriptor, at least 32 MB per day at 5 s against 1.5 MB here. A record is an **audit
unit**: a trace that retains inputs can produce the canonical record for any one frame on demand
(by re-executing it on a compatible engine, as the record's own workflow requires), and that is how
a disputed frame is escalated.

**Conversion.** Columns are plain `Vec<T>`, so `oce-py` hands them to numpy without a per-row
object and `Trace.to_pandas()` builds a frame indexed by time, with nullable dtypes where status
masks a row. Arrow (M4, M5) maps each column to a primitive array; `oce-replay` carries neither
dependency. `Trace::canonical_bytes()` is a fixed little-endian encoding for tests (decision 23).

## Size budget per frame

Per frame, excluding the sparse tables and snapshots:

```text
bytes = 8 (time) + 1 (status) + Σ width(output)
      + [ceil(N_in / 8)   if a substituting policy is in force]
      + [Σ width(input)   if inputs are retained]
width: Real 8, Integer 8, Boolean 1, Enum 4
```

Worked numbers for `cooling_only_controller` (the "Controller" row of
[the benchmarks](../benchmarks.md#complete-frame-observations): 14 inputs, 10 outputs). The prior
art review computed them assuming every column is Real. The fixture's 10 outputs are 5 Real and 5
Integer, all 8 bytes, so the outputs-only figure is exact; its 14 inputs are 8 Real, 3 Integer and
3 Boolean, 91 B in all.

| Trace | B / frame | 1 day at 5 s (17,280 frames) | 1 day at 1 s (86,400 frames) |
| --- | ---: | ---: | ---: |
| outputs only | 89 | 1.5 MB | 7.7 MB |
| outputs and substitution mask | 91 | 1.6 MB | 7.9 MB |
| outputs and inputs | 180 | 3.1 MB | 15.6 MB |
| one `ReplayRecord` per frame | ≥ 1,855 + descriptor | ≥ 32 MB | ≥ 160 MB |

A diagnostic row is about 24 B plus its two strings. A snapshot is the size of the engine's state:
the delay example in [state compatibility](../state-compatibility.md) is about 33 KB, so a snapshot
every 720 frames (one hour at 5 s) adds about 0.8 MB per day to that model. All of it counts
against `ReplayLimits::max_trace_bytes`, checked before each chunk allocates.

For scale only (local 2026-09-19 observations, not re-blessed): 3,246 ns and 27 allocations per
release prepare-and-commit on this fixture, so a 1 Hz day takes about 0.3 s. Appending to
preallocated columns allocates nothing per frame; #401's slot driver removes the facade's 27.

## Resumable cursor

A replay is a cursor, not a closure (decision 6). `Replay::advance(max_frames)` executes at most
that many source frames and returns; `run` is a loop over it. The engine is owned by the `Replay`
for the run (`execute_frame` takes `&mut self`), and `Replay::finish` hands it back with the trace.

- **Python (M5).** `engine.run(frames)` copies numpy columns into a `ColumnarSource` while it holds
  the GIL, then releases it around each `advance(4096)` (about 13 ms on the largest fixture) and
  calls `check_signals()` between chunks so Ctrl-C works. Per-frame release would pay the GIL's
  switch interval on microsecond frames. A dict-per-frame iterator is the documented slow path.
- **Browser (M4).** The engine lives in a dedicated worker; the main thread never runs frames. The
  worker calls `advance(chunk)`, then yields to its event loop to receive cancel and progress
  messages. No `SharedArrayBuffer` is needed.
- **Chunk-size independence.** A test proves chunk sizes 1, 7 and the whole run give
  byte-identical `Trace::canonical_bytes()`.

## Rust API sketch

```rust
// crates/leverage/oce-replay — depends on oce-api only.
use oce_api::{Engine, EngineStateSnapshot, InputDefinition, NonFiniteInputPolicy, Value, ValueType};

/// Names resolved to slots once; slot order is `Engine::input_definitions()` order.
pub struct InputMap { /* paths, types, per-slot policy, source column of each slot */ }
pub struct ColumnMap { pub entries: Vec<(String, String)> } // source column -> input path or suffix

impl InputMap {
    pub fn resolve(
        inputs: &[InputDefinition], columns: &[ColumnSpec], map: &ColumnMap,
        missing: &MissingPolicy, overrides: &[(String, MissingPolicy)],
    ) -> Result<InputMap, MapError>;
}
pub struct ColumnSpec { pub name: String, pub value_type: ValueType }

/// Yields one row by slot; `None` is a missing sample. Not `Iterator`: it lends its buffer.
pub trait FrameSource {
    fn columns(&self) -> &[ColumnSpec];
    fn next_row(&mut self) -> Result<Option<(f64, &[Option<Value>])>, SourceError>;
}

pub enum TimeColumn { Seconds(Vec<f64>), EpochNanos { values: Vec<i64>, origin: i64 } }
/// Wall-clock origin metadata: an ISO-8601 instant with an explicit offset; header only.
pub struct TimeOrigin(String);
pub enum Column {
    Real(Vec<f64>),                                       // NaN is missing
    Integer { values: Vec<i64>, valid: Option<Vec<bool>> },
    Boolean { values: Vec<bool>, valid: Option<Vec<bool>> },
    Enum { class: String, ordinals: Vec<u32>, valid: Option<Vec<bool>> },
}
pub struct ColumnarSource { /* owned columns, bound to an InputMap */ }
pub struct CsvSource<R: std::io::Read> { /* bounded reader, bound to an InputMap */ }
pub struct StepSource { /* per-slot change lists, bound to an InputMap */ }
impl ColumnarSource {
    pub fn new(map: &InputMap, time: TimeColumn, columns: Vec<(String, Column)>,
               limits: &ReplayLimits) -> Result<Self, SourceError>;
}
// CsvSource::new(map, reader, format, limits) and StepSource::new(map, steps, limits) alike.

pub enum TimeBase { Recorded, Grid { t0: f64, step: f64 } }  // default: none, always declared
pub enum DuplicateTime { Refuse, Allow }                       // default Refuse
pub enum MissingPolicy { Refuse, HoldLast { max_age: f64 }, Fallback(Value), SkipFrame }
pub enum OnRefusal { Stop, RecordAndContinue }                // default Stop

pub struct ReplayOptions {
    pub time: TimeBase,
    pub origin: Option<TimeOrigin>,
    pub duplicate_time: DuplicateTime,
    pub on_refusal: OnRefusal,
    pub non_finite: NonFiniteInputPolicy,                     // default Refuse (decision 12)
    pub trace: TraceOptions,
    pub limits: ReplayLimits,
}
pub struct TraceOptions {
    pub inputs: bool, pub snapshot_every: Option<u64>, pub snapshot_at: Vec<u64>,
}
pub struct ReplayLimits {
    pub max_rows: u64, pub max_columns: u32, pub max_cell_bytes: u32, pub max_source_bytes: u64,
    pub max_steps: u64, pub max_frames: u64, pub max_trace_bytes: u64,
}

pub struct Replay<S: FrameSource> { /* engine, map, source, policy state, trace, cursor */ }
pub enum Progress {
    More { done: u64 }, Done { frames: u64 }, Stopped { frame: u64, cause: RefusalCause },
}

impl<S: FrameSource> Replay<S> {
    /// Checks `map` against `engine.input_definitions()` (same paths and types, same order).
    pub fn new(engine: Engine, map: InputMap, source: S, options: ReplayOptions)
        -> Result<Self, ReplayError>;
    /// Refuses unless every `[from, to]` window contains at least one tick.
    pub fn check_coverage(&self, windows: &[(f64, f64)]) -> Result<(), ReplayError>;
    pub fn advance(&mut self, max_frames: usize) -> Result<Progress, ReplayError>;
    pub fn run(self) -> Result<(Engine, Trace), ReplayError>;
    pub fn trace(&self) -> &Trace;
    pub fn finish(self) -> (Engine, Trace);
}

pub struct Trace { /* meta, time, status, outputs, substitution mask, inputs, tables, snapshots */ }
pub enum TraceColumn { Real(Vec<f64>), Integer(Vec<i64>), Boolean(Vec<u8>), Enum(Vec<u32>) }
impl Trace { // accessors borrow; none allocates except canonical_bytes
    pub fn times(&self) -> &[f64];      pub fn status(&self) -> &[u8];
    pub fn outputs(&self) -> &[(String, TraceColumn)];
    pub fn inputs(&self) -> Option<&[(String, TraceColumn)]>;
    pub fn diagnostics(&self) -> &[DiagnosticRow];  pub fn refusals(&self) -> &[RefusalRow];
    pub fn snapshots(&self) -> &[(u64, EngineStateSnapshot)];
    pub fn is_clean(&self) -> bool;     pub fn canonical_bytes(&self) -> Vec<u8>;
}
```

**How the loop drives the engine today.** The policy stage fills a reused `Vec<Value>` in slot
order; the facade driver fills a reused `Vec<(&str, Value)>` keyed by the `InputMap`'s paths, calls
`engine.prepare_frame(t, &entries)` then `engine.execute_frame(prepared)`, and appends the
`CompletedFrame`'s outputs to the columns by index. It is private, behind one internal
`execute(t, &[Value])` trait, so swapping it changes nothing public.

**How it will drive #401.** #401 extracts the frame driver into `oce-graph` as a slot-indexed core
that both the facade and the `no_std` entry call. A leverage crate reaches it only through
`oce-api`, so replay switches when the facade exposes a slot-indexed execute over the same driver
(decision 14). The `InputMap`'s slot order is already the blob's I/O order, so the switch is the
private driver alone; the trace and every public signature stay as above, and the chunk-size and
golden tests prove the bits did not move.

## Decisions

Decisions 1 to 4 are the owner's. Decisions 5 to 10 are technical recommendations accepted by the
orchestrator. Decisions 11 to 23 were proposed while drafting and settled by the architect pass,
which replaced 22; each ruling is recorded under [Review rulings](#review-rulings).

**1. Home: a leverage crate, `crates/leverage/oce-replay`.** Sources, missing-sample policy, trace
and cursor, depending on `oce-api` only, consumed by `oce-verify`, `oce-py`, `oce-wasm` and
`oce-host`. Rejected: a module in `oce-verify` (the host would depend on a verification crate for
its staleness policy, or duplicate it); a loop in `oce-api` (policy inside the engine crate). Why:
the policy types are defined once and the facade stays policy-free.

**2. Missing data: refuse and stop by default.** `MissingPolicy::Refuse` and `OnRefusal::Stop`;
`HoldLast { max_age }`, `Fallback`, `SkipFrame` and `RecordAndContinue` are explicit opt-ins; every
substitution or skip is written into the trace; NaN, NaT and blank are always missing, never a
value, whatever `NonFiniteInputPolicy` says. Rejected: record-and-continue by default (silent
refused runs); hold-last by default (fabricated inputs, the prototype bridge's habit). Why: the
engine never invents an input, and replay must not invent one silently either.

**3. Trace: columnar, typed, raw bits.** Per frame: time, status byte, output columns; sparse
diagnostics; inputs and state snapshots opt-in; a documented size per frame. Rejected: one
`ReplayRecord` per frame (descriptor and paths repeated, tens of MB per day); JSON lines. Why:
exact, compact, and converts to pandas or Arrow directly.

**4. Input shape: slot-indexed after one name→slot mapping.** `FrameSource` yields rows by slot;
adapters for numpy and pandas columns, a bounded CSV with a column map, and step lists. Rejected:
named maps per frame (a string-keyed prepare path per frame that cannot drive the `no_std` entry).
Why: the same shape on host and target.

**5. Time: `Recorded` or `Grid { t0, step }` with zero-order hold.** Ticks are `t0 + k * step` by
multiply, never accumulated; `Recorded` passes sample times through unchanged; `DuplicateTime` is a
policy, `Refuse` by default and `Allow` as an explicit opt-in, with each equal-time frame its own
trace row; a reference on a different time base is resampled by the consumer, never by replay.
Rejected: accumulating ticks (drift); collapsing equal times (first-wins or last-wins); a fixed
refusal (CDL §12 data carries equal-time samples). Why: an equal-time resubmission is a real, legal
HostTick transition, so it is the caller's decision, recorded. (`Allow` and consumer-side
resampling were added by the orchestrator from the #436 research.)

**6. A resumable cursor.** `Replay::advance(max_frames)`; Python releases the GIL per chunk of
about 4,096 frames, copies numpy columns first and checks signals between chunks; the browser
worker yields between chunks; chunk sizes 1, 7 and N give byte-identical traces. Rejected: a
blocking `run` only; releasing per frame. Why: responsiveness without per-frame lock traffic.

**7. Coverage before verdicts.** A run is refused unless every expectation window contains a tick
(`Replay::check_coverage`). Rejected: reporting a pass over windows no tick reached. Why: kept from
the prototype bridge; every verdict must rest on checks that ran.

**8. Bounded sources.** Row, column, byte, cell and step caps, documented per source, each a typed
error. Rejected: trusting the file; truncating. Why: the hostile-input rule (GitHub #381, #382), and
the downstream envelope comparison is quadratic in rows.

**9. The conformance driver is rebuilt on this API later, in its own PR.** Its quadratic
`value_at` scan and f64-only capture are fixed by that rebuild, not in M3's first cut. The replay
API never delivers a NaN input, so the conformance cases that feed NaN and ±∞ keep driving the
facade directly and the rebuild excludes them. Why: scope rule; the first cut must leave every
golden byte-identical.

**10. Sequencing.** #445 builds `oce-replay` on the facade path; the slot shape moves to #401's
driver through #444's facade entry, with or after #401; time and missing-sample semantics are
designed with #436; M3 vector format v2 maps onto the step, CSV and series sources.

**11. samples and signals.** As stated under [Missing samples](#missing-samples).
Rejected: treating zero-order hold as free (it hides staleness, the defect decision 2 exists to
prevent); requiring `max_age` on every grid run.

**12. replay selects the non-finite policy.** `ReplayOptions::non_finite` defaults to
`Refuse` and `Replay::new` applies it to the engine it owns, recording it in the trace metadata.
NaN never reaches the engine (decision 2), so the option only governs ±∞ cells. Rejected:
inheriting the engine's current default (`Admit` today, planned to become `Refuse` under #363),
which would make a replay's outcome depend on host configuration it does not record.

**13. a source marks missing with `Option`.** Sources yield `&[Option<Value>]`; only the
policy stage produces the `&[Value]` the driver submits. Rejected: a placeholder value plus a mask
(a seed one bug away from submission). This refines the cell type of decision 4, not its shape.

**14. a facade slot entry after #401.** Replay switches to slot-indexed execution when
`oce-api` exposes one over #401's core driver: #444, its own facade issue. Rejected: a leverage
crate reaching `oce-graph` (the leverage rule).

**15. the replay owns the engine.** `Replay::new` takes the `Engine` by value and
`finish` returns it. Rejected: `&mut Engine` (a lifetime in every `oce-py` and `oce-wasm` class).
The caller loads, restores or checkpoints before the run; replay never reloads.

**16. outputs from the first accepted frame, checked every frame.** A changed output set
is a typed stop. Rejected: trusting the inventory.

**17. time columns and origin.** Seconds as `f64`, used as given; epoch nanoseconds as
`i64` relative to an integer origin, subtracted in integers and converted by a fixed split (whole
seconds plus the nanosecond remainder); or, in CSV, ISO-8601 instants with an explicit `Z` or
`±hh:mm` offset (an RFC 3339 subset), converted against the source's origin the same way. Every
conversion is pinned by goldens. The origin is header metadata only, as the orchestrator directed
from the #436 research.
Rejected: zone names and local-time rules (a tz database in the crate); per-frame wall-clock
columns in the trace (8 B per frame for a value one header field determines).

**18. one row per attempted frame.** Refused and skipped frames keep their row, with
zero bits in output columns masked by status. Rejected: dropping them (times would no longer align
with the source).

**19. default limits.** `max_rows` and `max_frames` 16,777,216 (194 days at 1 s),
`max_columns` 4,096, `max_cell_bytes` 256, `max_source_bytes` 1 GiB, `max_steps` 1,000,000 per
input, `max_trace_bytes` 1 GiB; each pinned at its boundary. These are defaults and documented
maxima: a caller may lower any of them, never raise one above its documented maximum.

**20. a dense substitution mask.** One bit per input per frame while any input's policy
can substitute; which substitution it was follows from that input's policy. Rejected: a sparse row
per substitution (24 B each, larger than the mask for change-of-value logs).

**21. Boolean one byte, Enum ordinal and class path.** Booleans are one `u8` per frame
so numpy can take the column as `bool`; an Enum column stores the `u32` ordinal and its class path
once in metadata. String is metadata, never a signal, so no String column exists.

**22. The `csv` crate, inside this crate's caps.** `CsvSource` parses with the `csv` crate, a
dependency of `oce-replay` only, and enforces its own row, column, byte and cell caps around it.
Rejected: a hand-written reader (the drafted proposal). Why: a leverage crate is never embedded;
quoting, byte-order marks and CRLF are edge cases a well-tested reader already handles; the caps,
not the parser, are the hostile-input boundary.

**23. `Trace::canonical_bytes()`.** A fixed little-endian encoding (magic, metadata,
then each column in order, raw bits) for goldens, the chunk-size test and hashing. It is not a
published interchange format until a later note says so.

## Relation to other work

- **#401 (slot-indexed frame driver).** Replay's `InputMap` order is the blob's I/O order; the
  facade path is used until #444's slot entry exists (decision 14). The #401 dual-path test is the
  natural first replay consumer once both exist.
- **#436 (commissioning verifier).** Uses `Recorded` time, `DuplicateTime::Allow`, the header
  origin, `CsvSource` with its caps and an input-retaining trace; converts units before the source;
  resamples references, builds its funnel envelope and derives indicators from the trace's exact
  times and bits. The two notes settle time and missing-sample semantics together.
- **M3 vector format v2.** Constant and step inputs map onto `StepSource`, CSV columns onto
  `CsvSource`, series onto `ColumnarSource`; expected diagnostics read the diagnostics table and
  expected state reads snapshots; windows go through `check_coverage`. Comparison is `oce-verify`'s
  layer over the trace; the trace never embeds a tolerance.
- **M5 (`oce-py`)** builds `engine.run(frames) -> Trace` and `Trace.to_pandas()` on the cursor and
  columns; **M4 (`oce-wasm`)** runs the worker through `advance`; **M6 (`oce-host`)** reuses
  `MissingPolicy`, `OnRefusal` and `CsvSource` and adds wall-clock staleness, never a second
  policy vocabulary.
- **Conformance driver.** Rebuilt on this API in a separate, later PR, without the NaN and ±∞
  strict-bit cases, which keep driving the facade directly (decision 9).
- **Implementation.** #445 builds the `oce-replay` crate (sources, policy, trace, cursor); #444
  adds the facade slot-indexed execute entry after #401 (decision 14). Goldens use `oce-bless` as a
  dev-dependency, under the rule change in the authoring note's decision 19.

## Test plan

- **Determinism goldens.** Every corpus fixture replayed through `StepSource` and `ColumnarSource`
  gives output bits identical to the conformance goldens; `Trace::canonical_bytes()` goldens for a
  handful of fixtures, compared bit for bit and regenerated only through `oce-bless`.
- **Chunk-size independence.** Chunk sizes 1, 7 and N give byte-identical traces, for clean runs and
  for runs with every status present.
- **Missing policy.** For each `MissingPolicy` and `OnRefusal` pair, a scenario with the exact
  status column, substitution mask and refusal rows; `HoldLast` at `max_age` exactly is held and one
  ulp beyond is refused; a hold never holds a substituted value; NaN, NaT, blank and cleared
  validity bits are missing under both `NonFiniteInputPolicy` values; ±∞ is a value.
- **Refusal atomicity.** After a refused frame under `RecordAndContinue`, the engine's state
  snapshot bytes and next sequence equal those of a run that never saw the frame.
- **Time.** `Grid` ticks at `k = 2^53` and with a fractional step equal `t0 + k * step` computed
  directly; equal times refused under `Refuse` and kept as separate rows and transitions under
  `Allow`; `Recorded` times passed through bit for bit; regression and missing time refused before
  the engine; origin, ISO-8601 and epoch-nanosecond conversion goldens, negative offsets included.
- **Bounds.** Every limit accepted at the limit and refused one past, with the typed error, and a
  request to raise one above its maximum refused; a hostile CSV corpus (unterminated quote, BOM,
  overlong cell, ragged rows, non-UTF-8) refuses or parses as the caps and the reader define.
- **Mapping.** Exact path, unique suffix, ambiguous suffix, unknown target, unmapped engine input,
  two columns on one input, type mismatch, and a stale `InputMap` after a reload.
- **Trace size and coverage.** A run's byte count equals the formula for every fixture (the budget
  is a test, not a claim); a window with no tick refuses, one touching a tick at a bound passes.

## Out of scope

- Any change to the engine's frame contract, `NonFiniteInputPolicy` or `oce-api` in this note's
  implementation PR (the slot entry is its own issue).
- Comparison, tolerances, funnels and verdicts (`oce-verify`, #436).
- Interpolation other than zero-order hold, resampling by averaging, resampling a reference or an
  output onto another time base, indicator and mask signals, and unit conversion.
- Time-zone names and local-time rules; reading files by path; compression; Arrow or pandas
  dependencies in `oce-replay`.
- A stable on-disk trace format, and producing `ReplayRecord`s in bulk.
- Live hosting, wall-clock staleness and BACnet sources (M6).
- Rebuilding the conformance driver (decision 9).

## Review rulings

The architect pass on the draft ruled:

- 11 accepted: samples and signals.
- 12 accepted: replay selects `NonFiniteInputPolicy::Refuse` explicitly and records it.
- 13 accepted as a refinement of decision 4; the owner's `(t, &[Value])` shape stays the summary.
- 14 accepted; the facade slot-indexed execute entry is #444.
- 15 accepted: the `Replay` owns the `Engine`, and `finish` returns it.
- 16 accepted: the output set is fixed by the first accepted frame and checked every frame.
- 17 accepted: the time column forms and the header-only origin.
- 18 accepted: one row per attempted frame, zeroed and status-masked when not executed.
- 19 accepted; the limits are defaults a caller may lower, never raise above the documented maxima.
- 20 accepted: the dense per-frame substitution bitmask.
- 21 accepted: Boolean as `u8`, Enum as `u32` ordinal with its class path in metadata.
- 22 rejected and replaced: the `csv` crate inside this crate's caps, as with the authoring
  note's `clap` ruling.
- 23 accepted: `Trace::canonical_bytes()` is for goldens and hashing, not a published format.
- NaN as missing vs the strict-bit evidence: accepted; recorded under decision 9.
- Corrected sizes accepted: 180 B per frame with inputs (3.1 MB a day at 5 s), and a
  `ReplayRecord` at least 1,855 B per frame.
- `oce-bless` as a dev-dependency accepted under the authoring note's decision 19.
- Implementing issues filed: #444 (facade slot entry) and #445 (`oce-replay` crate).
