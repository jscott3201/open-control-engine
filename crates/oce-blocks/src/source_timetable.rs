//! Typed `CDL.*.Sources.TimeTable` blocks.
//!
//! These source roots read only scheduler-provided model time and resolved table parameters. Reals
//! delegate to the same table semantics as `Modelica.Blocks.Sources.CombiTimeTable`; Integer and
//! Logical variants follow the Buildings CDL periodic step-table sources.

use std::borrow::Cow;

use oce_model::{EnumClassId, ParamTable, Value};

use crate::{
    Block, BlockKind, BlockSignature, Ctx, MAX_RESOLVED_PORT_WIDTH, PortKind, PortShape,
    ResolvedBlockSignature, Time, emit_real,
};

/// Minimum accepted integer/logical source table period from the pinned Buildings source.
pub(crate) const MIN_TIMETABLE_PERIOD: f64 = 1.0e-6;
pub(crate) const TIMETABLE_SMALL: f64 = 1.0e-37;

/// Source-verified `CDL.Types.Smoothness` members in ordinal order.
pub(crate) const SMOOTHNESS_MEMBERS: &[&str] = &["LinearSegments", "ConstantSegments"];

/// Source-verified `CDL.Types.Extrapolation` members in ordinal order.
pub(crate) const EXTRAPOLATION_MEMBERS: &[&str] = &["HoldLastPoint", "LastTwoPoints", "Periodic"];

/// Published catalog default for the `timeScale` parameter of every TimeTable class.
pub(crate) const TIMETABLE_TIME_SCALE_DEFAULT: f64 = 1.0;
/// Published catalog default member for `CDL.Reals.Sources.TimeTable.smoothness`.
pub(crate) const TIMETABLE_SMOOTHNESS_DEFAULT_MEMBER: &str = SMOOTHNESS_MEMBERS[0]; // LinearSegments
/// Published catalog default member for `CDL.Reals.Sources.TimeTable.extrapolation`.
pub(crate) const TIMETABLE_EXTRAPOLATION_DEFAULT_MEMBER: &str = EXTRAPOLATION_MEMBERS[2]; // Periodic
/// Published catalog default for each `CDL.Reals.Sources.TimeTable.offset` element.
pub(crate) const TIMETABLE_OFFSET_DEFAULT: f64 = 0.0;
/// Defensive totality fallback for required `period`; deliberately unpublished.
const TIMETABLE_PERIOD_FALLBACK: f64 = 1.0;

/// Interpolation mode for `CDL.Reals.Sources.TimeTable`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeTableSmoothness {
    /// Interpolate linearly between tabulated points.
    LinearSegments,
    /// Hold the previous tabulated point until the next time stamp.
    ConstantSegments,
}

impl TimeTableSmoothness {
    pub(crate) fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Enum {
                class: EnumClassId::SMOOTHNESS,
                ordinal: 1,
            }
            | Value::Integer(1) => Some(Self::LinearSegments),
            Value::Enum {
                class: EnumClassId::SMOOTHNESS,
                ordinal: 2,
            }
            | Value::Integer(2) => Some(Self::ConstantSegments),
            Value::String(s) => match s.rsplit('.').next().unwrap_or(s) {
                "LinearSegments" => Some(Self::LinearSegments),
                "ConstantSegments" => Some(Self::ConstantSegments),
                _ => None,
            },
            _ => None,
        }
    }
}

pub(crate) fn smoothness_from_member(member: &str) -> TimeTableSmoothness {
    match member {
        "LinearSegments" => TimeTableSmoothness::LinearSegments,
        "ConstantSegments" => TimeTableSmoothness::ConstantSegments,
        _ => unreachable!("registry-owned smoothness default uses valid CDL member tokens"),
    }
}

/// Extrapolation mode for `CDL.Reals.Sources.TimeTable`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeTableExtrapolation {
    /// Hold the boundary value outside the table scope.
    HoldLastPoint,
    /// Extrapolate linearly through the first or last two distinct table points.
    LastTwoPoints,
    /// Repeat the table range periodically.
    Periodic,
}

impl TimeTableExtrapolation {
    pub(crate) fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Enum {
                class: EnumClassId::EXTRAPOLATION,
                ordinal: 1,
            }
            | Value::Integer(1) => Some(Self::HoldLastPoint),
            Value::Enum {
                class: EnumClassId::EXTRAPOLATION,
                ordinal: 2,
            }
            | Value::Integer(2) => Some(Self::LastTwoPoints),
            Value::Enum {
                class: EnumClassId::EXTRAPOLATION,
                ordinal: 3,
            }
            | Value::Integer(3) => Some(Self::Periodic),
            Value::String(s) => match s.rsplit('.').next().unwrap_or(s) {
                "HoldLastPoint" => Some(Self::HoldLastPoint),
                "LastTwoPoints" => Some(Self::LastTwoPoints),
                "Periodic" => Some(Self::Periodic),
                _ => None,
            },
            _ => None,
        }
    }
}

pub(crate) fn extrapolation_from_member(member: &str) -> TimeTableExtrapolation {
    match member {
        "HoldLastPoint" => TimeTableExtrapolation::HoldLastPoint,
        "LastTwoPoints" => TimeTableExtrapolation::LastTwoPoints,
        "Periodic" => TimeTableExtrapolation::Periodic,
        _ => unreachable!("registry-owned extrapolation default uses valid CDL member tokens"),
    }
}

#[derive(Clone, Copy, Debug)]
enum Sample {
    Row(usize),
    Linear { lo: usize, hi: usize, alpha: f64 },
}

#[derive(Clone, Debug)]
struct TableData {
    times: Vec<f64>,
    values: Vec<f64>,
    nout: usize,
}

impl TableData {
    fn fallback() -> Self {
        Self {
            times: vec![0.0],
            values: vec![0.0],
            nout: 1,
        }
    }

    fn from_params(params: &ParamTable) -> Self {
        let Some((rows, cols)) = inferred_matrix_shape(params, "table") else {
            return Self::fallback();
        };
        if rows == 0
            || cols < 2
            || rows
                .checked_mul(cols)
                .is_none_or(|n| n > MAX_RESOLVED_PORT_WIDTH)
        {
            return Self::fallback();
        }
        let nout = cols - 1;
        let mut times = vec![0.0; rows];
        let mut values = vec![0.0; rows * nout];
        for (name, value) in &params.values {
            let Some((row, col)) = parse_matrix_param_name(name.as_ref(), "table") else {
                continue;
            };
            if row == 0 || col == 0 || row > rows || col > cols {
                continue;
            }
            let Some(v) = numeric_value(value) else {
                continue;
            };
            if col == 1 {
                times[row - 1] = v;
            } else {
                values[(row - 1) * nout + (col - 2)] = v;
            }
        }
        let time_scale = find_param(params, "timeScale")
            .and_then(numeric_value)
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(TIMETABLE_TIME_SCALE_DEFAULT);
        for time in &mut times {
            *time *= time_scale;
        }
        Self {
            times,
            values,
            nout,
        }
    }

    fn rows(&self) -> usize {
        self.times.len()
    }

    fn value(&self, row: usize, out: usize) -> f64 {
        self.values[row * self.nout + out]
    }

    fn time_range(&self) -> f64 {
        self.times[self.rows() - 1] - self.times[0]
    }

    fn sample(
        &self,
        t: Time,
        smoothness: TimeTableSmoothness,
        extrapolation: TimeTableExtrapolation,
    ) -> Sample {
        if self.rows() == 1 || !t.is_finite() {
            return Sample::Row(0);
        }
        let t = match extrapolation {
            TimeTableExtrapolation::Periodic => self.periodic_time(t),
            TimeTableExtrapolation::HoldLastPoint | TimeTableExtrapolation::LastTwoPoints => t,
        };
        let first = self.times[0];
        let last = self.times[self.rows() - 1];
        if t < first {
            return match extrapolation {
                TimeTableExtrapolation::LastTwoPoints => self.boundary_linear(true, t),
                _ => Sample::Row(0),
            };
        }
        if t > last {
            return match extrapolation {
                TimeTableExtrapolation::LastTwoPoints => self.boundary_linear(false, t),
                _ => Sample::Row(self.rows() - 1),
            };
        }
        if let Some(row) = self.last_row_at_time(t) {
            return Sample::Row(row);
        }
        let hi = self.times.partition_point(|time| *time <= t);
        let lo = hi.saturating_sub(1);
        if smoothness == TimeTableSmoothness::ConstantSegments {
            return Sample::Row(lo);
        }
        self.linear_between(lo, hi, t)
    }

    fn periodic_time(&self, t: Time) -> Time {
        let range = self.time_range();
        if !range.is_finite() || range <= 0.0 {
            return self.times[0];
        }
        self.times[0] + (t - self.times[0]).rem_euclid(range)
    }

    fn last_row_at_time(&self, t: Time) -> Option<usize> {
        self.times.iter().rposition(|time| *time == t)
    }

    fn boundary_linear(&self, before: bool, t: Time) -> Sample {
        let Some((lo, hi)) = self.boundary_rows(before) else {
            return Sample::Row(if before { 0 } else { self.rows() - 1 });
        };
        self.linear_between(lo, hi, t)
    }

    fn boundary_rows(&self, before: bool) -> Option<(usize, usize)> {
        if before {
            let hi = self.times.iter().position(|time| *time > self.times[0])?;
            Some((0, hi))
        } else {
            let lo = self
                .times
                .iter()
                .rposition(|time| *time < self.times[self.rows() - 1])?;
            Some((lo, self.rows() - 1))
        }
    }

    fn linear_between(&self, lo: usize, hi: usize, t: Time) -> Sample {
        let t_lo = self.times[lo];
        let t_hi = self.times[hi];
        let span = t_hi - t_lo;
        if span == 0.0 || !span.is_finite() {
            return Sample::Row(lo);
        }
        Sample::Linear {
            lo,
            hi,
            alpha: (t - t_lo) / span,
        }
    }
}

/// `CDL.Reals.Sources.TimeTable` - Real table lookup with interpolation and extrapolation.
#[derive(Clone, Debug)]
pub struct RealTimeTable {
    table: TableData,
    smoothness: TimeTableSmoothness,
    extrapolation: TimeTableExtrapolation,
    offsets: Vec<f64>,
    outputs: Vec<PortKind>,
}

impl RealTimeTable {
    pub(crate) fn from_params(params: &ParamTable) -> Self {
        let table = TableData::from_params(params);
        let smoothness = find_param(params, "smoothness")
            .and_then(TimeTableSmoothness::from_value)
            .unwrap_or_else(|| smoothness_from_member(TIMETABLE_SMOOTHNESS_DEFAULT_MEMBER));
        let extrapolation = find_param(params, "extrapolation")
            .and_then(TimeTableExtrapolation::from_value)
            .unwrap_or_else(|| extrapolation_from_member(TIMETABLE_EXTRAPOLATION_DEFAULT_MEMBER));
        let offsets = read_offsets(params, table.nout);
        Self::new(table, smoothness, extrapolation, offsets)
    }

    fn new(
        table: TableData,
        smoothness: TimeTableSmoothness,
        extrapolation: TimeTableExtrapolation,
        offsets: Vec<f64>,
    ) -> Self {
        let outputs = PortShape::new(PortKind::Real, table.nout).to_kinds();
        Self {
            table,
            smoothness,
            extrapolation,
            offsets,
            outputs,
        }
    }
}

impl Default for RealTimeTable {
    fn default() -> Self {
        Self::new(
            TableData::fallback(),
            smoothness_from_member(TIMETABLE_SMOOTHNESS_DEFAULT_MEMBER),
            extrapolation_from_member(TIMETABLE_EXTRAPOLATION_DEFAULT_MEMBER),
            vec![TIMETABLE_OFFSET_DEFAULT],
        )
    }
}

impl Block for RealTimeTable {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Sources.TimeTable",
            inputs: &[],
            outputs: &[PortKind::Real],
            stateful: false,
        };
        &SIG
    }

    fn resolved_signature(&self) -> ResolvedBlockSignature<'_> {
        ResolvedBlockSignature {
            class_path: self.signature().class_path,
            inputs: Cow::Borrowed(&[]),
            outputs: Cow::Borrowed(self.outputs.as_slice()),
        }
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn step_algebraic(&self, ctx: &Ctx<'_>, _inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let sample = self
            .table
            .sample(ctx.t(), self.smoothness, self.extrapolation);
        for out in 0..self.table.nout {
            let value = match sample {
                Sample::Row(row) => self.table.value(row, out),
                Sample::Linear { lo, hi, alpha } => {
                    let y0 = self.table.value(lo, out);
                    y0 + alpha * (self.table.value(hi, out) - y0)
                }
            };
            emit_real(out, value + self.offsets[out], emit);
        }
    }
}

/// `CDL.Integers.Sources.TimeTable` - periodic Integer step table.
#[derive(Clone, Debug)]
pub struct IntegerTimeTable {
    table: TableData,
    period: f64,
    outputs: Vec<PortKind>,
}

impl IntegerTimeTable {
    pub(crate) fn from_params(params: &ParamTable) -> Self {
        Self::new(
            TableData::from_params(params),
            find_param(params, "period")
                .and_then(numeric_value)
                .unwrap_or(TIMETABLE_PERIOD_FALLBACK),
        )
    }

    fn new(table: TableData, period: f64) -> Self {
        Self {
            outputs: PortShape::new(PortKind::Integer, table.nout).to_kinds(),
            table,
            period,
        }
    }

    fn period_eff(&self) -> f64 {
        if self.period.is_finite() && self.period >= MIN_TIMETABLE_PERIOD {
            self.period
        } else {
            TIMETABLE_PERIOD_FALLBACK
        }
    }

    fn row_at(&self, t: Time) -> usize {
        if self.table.rows() == 1 || !t.is_finite() {
            return 0;
        }
        let t_shifted = t.rem_euclid(self.period_eff());
        self.table
            .times
            .iter()
            .rposition(|time| t_shifted >= *time - MIN_TIMETABLE_PERIOD)
            .unwrap_or(0)
    }
}

impl Default for IntegerTimeTable {
    fn default() -> Self {
        Self::new(TableData::fallback(), TIMETABLE_PERIOD_FALLBACK)
    }
}

impl Block for IntegerTimeTable {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Integers.Sources.TimeTable",
            inputs: &[],
            outputs: &[PortKind::Integer],
            stateful: false,
        };
        &SIG
    }

    fn resolved_signature(&self) -> ResolvedBlockSignature<'_> {
        ResolvedBlockSignature {
            class_path: self.signature().class_path,
            inputs: Cow::Borrowed(&[]),
            outputs: Cow::Borrowed(self.outputs.as_slice()),
        }
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn step_algebraic(&self, ctx: &Ctx<'_>, _inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let row = self.row_at(ctx.t());
        for out in 0..self.table.nout {
            emit(
                out,
                Value::Integer(integer_table_value(self.table.value(row, out))),
            );
        }
    }
}

/// `CDL.Logical.Sources.TimeTable` - periodic Boolean step table.
#[derive(Clone, Debug)]
pub struct LogicalTimeTable {
    inner: IntegerTimeTable,
    outputs: Vec<PortKind>,
}

impl LogicalTimeTable {
    pub(crate) fn from_params(params: &ParamTable) -> Self {
        Self::new(IntegerTimeTable::from_params(params))
    }

    fn new(inner: IntegerTimeTable) -> Self {
        Self {
            outputs: PortShape::new(PortKind::Boolean, inner.table.nout).to_kinds(),
            inner,
        }
    }
}

impl Default for LogicalTimeTable {
    fn default() -> Self {
        Self::new(IntegerTimeTable::default())
    }
}

impl Block for LogicalTimeTable {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Logical.Sources.TimeTable",
            inputs: &[],
            outputs: &[PortKind::Boolean],
            stateful: false,
        };
        &SIG
    }

    fn resolved_signature(&self) -> ResolvedBlockSignature<'_> {
        ResolvedBlockSignature {
            class_path: self.signature().class_path,
            inputs: Cow::Borrowed(&[]),
            outputs: Cow::Borrowed(self.outputs.as_slice()),
        }
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn step_algebraic(&self, ctx: &Ctx<'_>, _inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let row = self.inner.row_at(ctx.t());
        for out in 0..self.inner.table.nout {
            emit(
                out,
                Value::Boolean(integer_table_value(self.inner.table.value(row, out)) > 0),
            );
        }
    }
}

fn read_offsets(params: &ParamTable, nout: usize) -> Vec<f64> {
    let mut offsets = vec![TIMETABLE_OFFSET_DEFAULT; nout];
    for (name, value) in &params.values {
        let Some(idx) = parse_array_param_name(name.as_ref(), "offset") else {
            continue;
        };
        if (1..=nout).contains(&idx)
            && let Some(value) = numeric_value(value)
        {
            offsets[idx - 1] = value;
        }
    }
    offsets
}

fn inferred_matrix_shape(params: &ParamTable, base: &str) -> Option<(usize, usize)> {
    let mut rows = 0_usize;
    let mut cols = 0_usize;
    for (name, _) in &params.values {
        let Some((row, col)) = parse_matrix_param_name(name.as_ref(), base) else {
            continue;
        };
        rows = rows.max(row);
        cols = cols.max(col);
    }
    Some((rows, cols)).filter(|(rows, cols)| *rows > 0 && *cols > 0)
}

pub(crate) fn parse_matrix_param_name(name: &str, base: &str) -> Option<(usize, usize)> {
    let suffix = name.strip_prefix(base)?.strip_prefix('_')?;
    let (row, col) = suffix.split_once('_')?;
    if col.contains('_') {
        return None;
    }
    Some((row.parse().ok()?, col.parse().ok()?))
}

pub(crate) fn parse_array_param_name(name: &str, base: &str) -> Option<usize> {
    let suffix = name.strip_prefix(base)?.strip_prefix('_')?;
    if suffix.contains('_') {
        return None;
    }
    suffix.parse().ok()
}

pub(crate) fn numeric_value(value: &Value) -> Option<f64> {
    match value {
        Value::Real(v) => Some(*v),
        Value::Integer(v) => Some(*v as f64),
        _ => None,
    }
}

fn find_param<'a>(params: &'a ParamTable, name: &str) -> Option<&'a Value> {
    params
        .values
        .iter()
        .find(|(n, _)| n.as_ref() == name)
        .map(|(_, v)| v)
}

fn integer_table_value(value: f64) -> i64 {
    let shifted = value + TIMETABLE_SMALL;
    if shifted.is_finite() && shifted >= i64::MIN as f64 && shifted <= i64::MAX as f64 {
        shifted.floor() as i64
    } else {
        0
    }
}
