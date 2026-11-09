//! The resumable cursor (decisions 6, 12, 15 and 16): a [`Replay`] owns the engine for the run,
//! [`Replay::advance`] executes at most a chunk of frames and returns, and [`Replay::finish`]
//! hands the engine back with the trace.
//!
//! Execution goes through the facade's slot entry: `Engine::frame_slots` once in
//! [`Replay::new`], then `Engine::execute_slots` per frame with a reused value buffer in slot
//! order. Once the trace columns have room (each chunk reserves it, or [`Replay::reserve`]), an
//! executed, substituted or skipped frame allocates nothing, except that a frame whose engine
//! emits warnings appends owned diagnostic rows (and the engine may grow its reused warning
//! records), a refused frame appends its refusal row, and an opt-in snapshot allocates its bytes.

use oce_api::{Engine, FrameSlots, NonFiniteInputPolicy, Value, ValueType};

use crate::frame::{Attempt, Cells};
use crate::{
    ColumnKind, DuplicateTime, FrameSource, InputMap, OnRefusal, RefusalCause, ReplayError,
    ReplayLimits, SourceError, SourceKind, TimeBase, TimeOrigin, Trace, TraceColumn, TraceInput,
    TraceKind, TraceMeta,
};

/// The most rows one chunk reserves ahead; a larger chunk grows its columns geometrically.
const RESERVE_CAP: usize = 65_536;

/// What to record beyond the always-present columns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TraceOptions {
    /// Retain one column per input (the values submitted or substituted, zero bits where none
    /// resolved).
    pub inputs: bool,
    /// Take a state snapshot after every row `r` with `(r + 1) % n == 0`. Must not be zero.
    pub snapshot_every: Option<u64>,
    /// Take a state snapshot after each of these rows.
    pub snapshot_at: Vec<u64>,
}

/// The options of one run.
#[derive(Clone, Debug)]
pub struct ReplayOptions {
    /// The time base; always declared.
    pub time: TimeBase,
    /// A wall-clock origin for the trace header. If the source also reports one they must be
    /// equal.
    pub origin: Option<TimeOrigin>,
    /// The duplicate-time policy. Default `Refuse`.
    pub duplicate_time: DuplicateTime,
    /// The refusal policy. Default `Stop`.
    pub on_refusal: OnRefusal,
    /// The non-finite input policy [`Replay::new`] sets on its engine and records. Default
    /// `Refuse` (decision 12); NaN never reaches the engine either way, so it governs ±∞ cells.
    pub non_finite: NonFiniteInputPolicy,
    /// What the trace keeps.
    pub trace: TraceOptions,
    /// The run's limits.
    pub limits: ReplayLimits,
}

impl ReplayOptions {
    /// The defaults for `time`: no origin, `DuplicateTime::Refuse`, `OnRefusal::Stop`,
    /// `NonFiniteInputPolicy::Refuse`, outputs only, the documented maximum limits.
    #[must_use]
    pub fn new(time: TimeBase) -> Self {
        Self {
            time,
            origin: None,
            duplicate_time: DuplicateTime::Refuse,
            on_refusal: OnRefusal::Stop,
            non_finite: NonFiniteInputPolicy::Refuse,
            trace: TraceOptions::default(),
            limits: ReplayLimits::MAX,
        }
    }
}

/// Where a run stands after [`Replay::advance`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Progress {
    /// The chunk ran out; `done` rows exist so far.
    More {
        /// Rows so far.
        done: u64,
    },
    /// The source (or grid) is exhausted; `frames` rows in all.
    Done {
        /// Rows in all.
        frames: u64,
    },
    /// A refused frame under `OnRefusal::Stop` ended the run; its row is in the trace.
    Stopped {
        /// The refused row.
        frame: u64,
        /// Why.
        cause: RefusalCause,
    },
}

enum Cursor {
    Running,
    Done,
    Stopped(u64, RefusalCause),
    Failed,
}

enum Step {
    Frame,
    End,
    Stop(u64, RefusalCause),
}

/// One run: the engine, the map, the source, the policy state, the trace and the cursor.
pub struct Replay<S: FrameSource> {
    pub(crate) engine: Engine,
    pub(crate) slots: FrameSlots,
    pub(crate) map: InputMap,
    source: S,
    kind: SourceKind,
    time: TimeBase,
    duplicate: DuplicateTime,
    pub(crate) on_refusal: OnRefusal,
    pub(crate) limits: ReplayLimits,
    pub(crate) trace: Trace,
    pub(crate) cells: Cells,
    snapshot_at: Vec<u64>,
    snapshot_every: Option<u64>,
    prev_time: Option<f64>,
    source_rows: u64,
    tick: u64,
    pending: Option<f64>,
    pending_cells: Vec<Option<Value>>,
    exhausted: bool,
    last_time: Option<f64>,
    cursor: Cursor,
}

impl<S: FrameSource> std::fmt::Debug for Replay<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Replay")
            .field("inputs", &self.map.len())
            .field("rows", &self.trace.rows())
            .finish_non_exhaustive()
    }
}

fn trace_kind(value_type: ValueType) -> TraceKind {
    match value_type {
        ValueType::Real | ValueType::String => TraceKind::Real,
        ValueType::Integer => TraceKind::Integer,
        ValueType::Boolean => TraceKind::Boolean,
        ValueType::Enum(_) => TraceKind::Enum,
    }
}

impl<S: FrameSource> Replay<S> {
    /// Take ownership of `engine` for a run of `source` through `map`.
    ///
    /// Checks the limits, the time base and trace options, that `source` is bound to `map`, and
    /// that `map` still matches `engine.input_definitions()` (same paths, types, bounds, order);
    /// then sets `options.non_finite` on the engine, resolves its slot map once and records the
    /// metadata. The caller loads, restores or checkpoints before the run; replay never reloads.
    ///
    /// # Errors
    /// `Limit`, `InvalidTimeBase`, `InvalidTraceOptions`, `SourceNotBound`, `Engine` (no loaded
    /// model, pending edits), `StaleMap`, `OriginConflict`, in that order.
    pub fn new(
        mut engine: Engine,
        map: InputMap,
        source: S,
        options: ReplayOptions,
    ) -> Result<Self, ReplayError> {
        options.limits.validate()?;
        if let TimeBase::Grid { t0, step } = options.time
            && !(t0.is_finite() && step.is_finite() && step > 0.0)
        {
            return Err(ReplayError::InvalidTimeBase);
        }
        if options.trace.snapshot_every == Some(0) {
            return Err(ReplayError::InvalidTraceOptions);
        }
        if source.map_id() != map.id() {
            return Err(ReplayError::SourceNotBound);
        }
        if !map.matches(&engine.input_definitions()?) {
            return Err(ReplayError::StaleMap);
        }
        let origin = match (options.origin, source.origin()) {
            (Some(a), Some(b)) if &a != b => return Err(ReplayError::OriginConflict),
            (Some(a), _) => Some(a),
            (None, b) => b.cloned(),
        };
        engine.set_non_finite_input_policy(options.non_finite);
        let slots = engine.frame_slots()?;
        let n = map.len();
        let inputs = (0..n)
            .map(|slot| TraceInput {
                path: map.inputs()[slot].path.clone(),
                kind: trace_kind(map.inputs()[slot].value_type),
                enum_class: match map.kind(slot) {
                    ColumnKind::Enum(class) => Some(class.clone()),
                    _ => None,
                },
                policy: map.policy(slot).clone(),
            })
            .collect::<Vec<_>>();
        let mut ignored = map.ignored_columns().to_vec();
        ignored.extend(source.ignored_columns().iter().cloned());
        let mut snapshot_at = options.trace.snapshot_at.clone();
        snapshot_at.sort_unstable();
        snapshot_at.dedup();
        let trace = Trace {
            times: Vec::new(),
            status: Vec::new(),
            outputs: Vec::new(),
            mask: map.can_substitute().then(Vec::new),
            inputs: options.trace.inputs.then(|| {
                inputs
                    .iter()
                    .map(|i| (i.path.clone(), TraceColumn::new(i.kind, 0, 0)))
                    .collect()
            }),
            diagnostics: Vec::new(),
            refusals: Vec::new(),
            snapshots: Vec::new(),
            sparse_bytes: 0,
            meta: TraceMeta {
                inputs,
                outputs: Vec::new(),
                time: options.time,
                duplicate_time: options.duplicate_time,
                on_refusal: options.on_refusal,
                non_finite: options.non_finite,
                retains_inputs: options.trace.inputs,
                snapshot_every: options.trace.snapshot_every,
                snapshot_at: snapshot_at.clone(),
                limits: options.limits,
                origin,
                ignored_columns: ignored,
            },
        };
        Ok(Self {
            cells: Cells::new(map.inputs()),
            kind: source.kind(),
            engine,
            slots,
            source,
            time: options.time,
            duplicate: options.duplicate_time,
            on_refusal: options.on_refusal,
            limits: options.limits,
            trace,
            snapshot_at,
            snapshot_every: options.trace.snapshot_every,
            prev_time: None,
            source_rows: 0,
            tick: 0,
            pending: None,
            pending_cells: vec![None; n],
            exhausted: false,
            last_time: None,
            cursor: Cursor::Running,
            map,
        })
    }

    /// The trace so far.
    #[must_use]
    pub fn trace(&self) -> &Trace {
        &self.trace
    }

    /// Refuse unless every `[from, to]` window contains an executed row time so far; see
    /// [`Trace::check_coverage`].
    ///
    /// # Errors
    /// [`ReplayError::InvalidWindow`] or [`ReplayError::Uncovered`].
    pub fn check_coverage(&self, windows: &[(f64, f64)]) -> Result<(), ReplayError> {
        self.trace.check_coverage(windows)
    }

    /// Reserve room for `frames` more rows in every trace column, bounded by the frame and trace
    /// byte budgets, so the frames that follow append without allocating. Optional: every chunk
    /// reserves its own room.
    ///
    /// # Errors
    /// None today; the `Result` keeps room for a budget refusal.
    pub fn reserve(&mut self, frames: usize) -> Result<(), ReplayError> {
        let rows = self.trace.rows() as u64;
        let by_frames = self.limits.max_frames.saturating_sub(rows);
        let by_bytes = self
            .limits
            .max_trace_bytes
            .saturating_sub(self.trace.byte_size())
            / self.projected_bytes_per_frame();
        let n = (frames as u64).min(by_frames).min(by_bytes);
        let n = usize::try_from(n).unwrap_or(usize::MAX);
        let t = &mut self.trace;
        t.times.reserve(n);
        t.status.reserve(n);
        t.outputs.iter_mut().for_each(|(_, c)| c.reserve(n));
        let width = t.mask_width();
        if let Some(mask) = &mut t.mask {
            mask.reserve(n * width);
        }
        if let Some(inputs) = &mut t.inputs {
            inputs.iter_mut().for_each(|(_, c)| c.reserve(n));
        }
        Ok(())
    }

    /// Bytes per frame with every output not yet fixed counted at 8 bytes, the widest kind.
    pub(crate) fn projected_bytes_per_frame(&self) -> u64 {
        let unknown = if self.trace.meta.outputs.is_empty() {
            self.slots.outputs().len() as u64 * 8
        } else {
            0
        };
        self.trace.bytes_per_frame() + unknown
    }

    /// Execute at most `max_frames` frames and return. Resumable: the next call continues where
    /// this one stopped, and chunk sizes do not change the trace. After `Done` or `Stopped` it
    /// returns the same progress again without reading the source.
    ///
    /// # Errors
    /// A [`ReplayError`] ends the run: a source error (regression, duplicate under `Refuse`,
    /// missing or infinite time, a cap), `TooManyFrames`, `TraceTooLarge`, `OutputSetChanged`,
    /// `UnsupportedOutputType` or an `Engine` snapshot failure. Later calls return
    /// [`ReplayError::Failed`].
    pub fn advance(&mut self, max_frames: usize) -> Result<Progress, ReplayError> {
        match &self.cursor {
            Cursor::Running => {}
            Cursor::Done => {
                return Ok(Progress::Done {
                    frames: self.trace.rows() as u64,
                });
            }
            Cursor::Stopped(frame, cause) => {
                return Ok(Progress::Stopped {
                    frame: *frame,
                    cause: cause.clone(),
                });
            }
            Cursor::Failed => return Err(ReplayError::Failed),
        }
        self.reserve(max_frames.min(RESERVE_CAP))?;
        for _ in 0..max_frames {
            let step = match self.time {
                TimeBase::Recorded => self.recorded_step(),
                TimeBase::Grid { t0, step } => self.grid_step(t0, step),
            };
            match step {
                Ok(Step::Frame) => {}
                Ok(Step::End) => {
                    self.cursor = Cursor::Done;
                    return Ok(Progress::Done {
                        frames: self.trace.rows() as u64,
                    });
                }
                Ok(Step::Stop(frame, cause)) => {
                    self.cursor = Cursor::Stopped(frame, cause.clone());
                    return Ok(Progress::Stopped { frame, cause });
                }
                Err(error) => {
                    self.cursor = Cursor::Failed;
                    return Err(error);
                }
            }
        }
        Ok(Progress::More {
            done: self.trace.rows() as u64,
        })
    }

    /// Advance in chunks of 4,096 until the run is done or stopped, then hand back the engine
    /// and the trace.
    ///
    /// # Errors
    /// As [`Self::advance`]; the engine is dropped with the replay.
    pub fn run(mut self) -> Result<(Engine, Trace), ReplayError> {
        loop {
            match self.advance(4_096)? {
                Progress::More { .. } => {}
                Progress::Done { .. } | Progress::Stopped { .. } => return Ok(self.finish()),
            }
        }
    }

    /// End the run and hand back the engine and the trace.
    #[must_use]
    pub fn finish(self) -> (Engine, Trace) {
        (self.engine, self.trace)
    }

    /// Read the next source row into `dest`, applying replay's own time checks.
    fn read_row(&mut self, into_pending: bool) -> Result<Option<f64>, ReplayError> {
        let Some((t, row)) = self.source.next_row()? else {
            return Ok(None);
        };
        let number = self.source_rows + 1;
        if row.len() != self.map.len() {
            return Err(SourceError::RowWidth { row: number }.into());
        }
        if number > self.limits.max_rows {
            return Err(SourceError::TooManyRows {
                limit: self.limits.max_rows,
            }
            .into());
        }
        if t.is_nan() {
            return Err(SourceError::MissingTime { row: number }.into());
        }
        if t.is_infinite() {
            return Err(SourceError::NonFiniteTime { row: number }.into());
        }
        if let Some(previous) = self.prev_time {
            if t < previous {
                return Err(SourceError::TimeRegression { row: number }.into());
            }
            if t == previous && self.duplicate == DuplicateTime::Refuse {
                return Err(SourceError::DuplicateTime { row: number }.into());
            }
        }
        let dest = if into_pending {
            &mut self.pending_cells
        } else {
            &mut self.cells.current
        };
        dest.clone_from_slice(row);
        self.source_rows = number;
        self.prev_time = Some(t);
        Ok(Some(t))
    }

    fn check_frame_budget(&self) -> Result<(), ReplayError> {
        let rows = self.trace.rows() as u64;
        if rows >= self.limits.max_frames {
            return Err(ReplayError::TooManyFrames {
                limit: self.limits.max_frames,
            });
        }
        if self.trace.byte_size() + self.projected_bytes_per_frame() > self.limits.max_trace_bytes {
            return Err(ReplayError::TraceTooLarge {
                limit: self.limits.max_trace_bytes,
            });
        }
        Ok(())
    }

    fn recorded_step(&mut self) -> Result<Step, ReplayError> {
        let Some(t) = self.read_row(false)? else {
            return Ok(Step::End);
        };
        self.check_frame_budget()?;
        self.cells.observe_current(t);
        self.finish_attempt(t)
    }

    fn grid_step(&mut self, t0: f64, step: f64) -> Result<Step, ReplayError> {
        let k = self.tick;
        // The documented tick rule; `TimeBase::tick` is the same expression.
        #[allow(clippy::cast_precision_loss)]
        let tick = t0 + (k as f64) * step;
        loop {
            if self.pending.is_none() && !self.exhausted {
                match self.read_row(true)? {
                    Some(t) => self.pending = Some(t),
                    None => self.exhausted = true,
                }
            }
            match self.pending {
                Some(t) if t <= tick => {
                    self.cells.observe(&self.pending_cells, t);
                    self.last_time = Some(t);
                    self.pending = None;
                }
                _ => break,
            }
        }
        if self.pending.is_none() && self.last_time.is_none_or(|last| tick > last) {
            return Ok(Step::End);
        }
        self.check_frame_budget()?;
        #[allow(clippy::cast_precision_loss)]
        let previous = if k == 0 {
            t0 - step
        } else {
            t0 + ((k - 1) as f64) * step
        };
        self.cells
            .current_from_observed(previous, self.kind == SourceKind::Signals);
        self.tick = k + 1;
        self.finish_attempt(tick)
    }

    fn finish_attempt(&mut self, t: f64) -> Result<Step, ReplayError> {
        let row = self.trace.rows() as u64;
        let outcome = self.attempt(t)?;
        self.snapshot_if_due(row)?;
        if self.trace.byte_size() > self.limits.max_trace_bytes {
            return Err(ReplayError::TraceTooLarge {
                limit: self.limits.max_trace_bytes,
            });
        }
        Ok(match outcome {
            Attempt::Recorded => Step::Frame,
            Attempt::Refused(cause) => match self.on_refusal {
                OnRefusal::Stop => Step::Stop(row, cause),
                OnRefusal::RecordAndContinue => Step::Frame,
            },
        })
    }

    fn snapshot_if_due(&mut self, row: u64) -> Result<(), ReplayError> {
        let due = self
            .snapshot_every
            .is_some_and(|n| (row + 1).is_multiple_of(n))
            || self.snapshot_at.binary_search(&row).is_ok();
        if due {
            let snapshot = self.engine.state_snapshot()?;
            self.trace.sparse_bytes += 16 + snapshot.as_bytes().len() as u64;
            self.trace.snapshots.push((row, snapshot));
        }
        Ok(())
    }
}
