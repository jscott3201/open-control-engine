//! Golden-trace driver bound through the frozen `oce-api` facade.
//!
//! The driver loads CXF bytes through [`oce_api::Engine::load_cxf`], stages reference inputs through
//! [`oce_api::Engine::simulate`] or a driver-owned
//! [`oce_api::Engine::set_input`] / [`oce_api::Engine::step_realtime`] /
//! [`oce_api::Engine::get_output`] loop, and returns an encoded trace ready for funnel comparison.
//! It never depends on `oce-graph` or `oce-blocks`.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

use oce_api::{
    CollectSpec, Engine, InputSource, OcError, OutputTrace, PointDirection, PointInfo,
    PointValueType, SimSpec, Value,
};

use crate::{
    CombiTimeTable, ConfigError, CsvError, FunnelResult, Indicator, Mask, Series, Tolerances,
    ValueKind, VerifyConfig, compare, compare_masked,
};

/// Driver cadence selection.
#[derive(Clone, Debug, PartialEq)]
pub enum DriveCadence {
    /// Choose [`DriveMode::Uniform`] only when the reference time column is an exact fixed grid;
    /// otherwise tick the sorted, de-duplicated reference instants.
    Auto,
    /// Force the uniform [`Engine::simulate`] path.
    Uniform {
        /// First model time evaluated.
        t_start: f64,
        /// Last model time evaluated.
        t_stop: f64,
        /// Fixed simulation step.
        step: f64,
    },
    /// Force the event-aligned realtime loop, preserving the supplied instant order.
    EventAligned {
        /// Model times at which the engine is ticked.
        instants: Vec<f64>,
    },
}

/// Input replay mode used by the uniform path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriverInputReplay {
    /// Decode input columns from the reference [`CombiTimeTable`] and stage them with a closure.
    ReferenceTable,
    /// Pass through the frozen-but-deferred facade CSV input variant.
    ///
    /// This exists so callers can prove the facade returns the typed deferred error; normal B3
    /// conformance replay uses [`DriverInputReplay::ReferenceTable`].
    FacadeCsv {
        /// CSV path handed to [`InputSource::Csv`].
        path: PathBuf,
        /// `(input point, CSV column index)` bindings handed to [`InputSource::Csv`].
        bindings: Vec<(String, usize)>,
    },
}

/// Driver options for [`drive_trace_with_options`].
#[derive(Clone, Debug, PartialEq)]
pub struct DriverOptions {
    /// Cadence selection.
    pub cadence: DriveCadence,
    /// How input values are staged in the uniform path.
    pub input_replay: DriverInputReplay,
}

impl Default for DriverOptions {
    fn default() -> Self {
        Self {
            cadence: DriveCadence::Auto,
            input_replay: DriverInputReplay::ReferenceTable,
        }
    }
}

/// Drive mode actually used for a run.
#[derive(Clone, Debug, PartialEq)]
pub enum DriveMode {
    /// The uniform [`Engine::simulate`] path.
    Uniform {
        /// First model time evaluated.
        t_start: f64,
        /// Last model time evaluated.
        t_stop: f64,
        /// Fixed simulation step.
        step: f64,
    },
    /// The driver-owned event-aligned realtime loop.
    EventAligned {
        /// Tick instants used by the loop.
        instants: Vec<f64>,
    },
}

/// Numeric trace captured from the engine.
#[derive(Clone, Debug, PartialEq)]
pub struct CapturedTrace {
    /// Captured model times.
    pub times: Vec<f64>,
    /// Captured output and indicator columns, encoded as CombiTimeTable numeric values.
    pub columns: Vec<CapturedColumn>,
}

impl CapturedTrace {
    /// Return a captured column by point name.
    #[must_use]
    pub fn column(&self, name: &str) -> Option<&CapturedColumn> {
        self.columns.iter().find(|column| column.name == name)
    }

    /// Serialize the captured trace as an in-memory [`CombiTimeTable`].
    #[must_use]
    pub fn to_table(&self, name: impl Into<String>) -> CombiTimeTable {
        let n_rows = self.times.len();
        let n_cols = self.columns.len() + 1;
        let mut data = Vec::with_capacity(n_rows * n_cols);
        for row in 0..n_rows {
            data.push(self.times[row]);
            for column in &self.columns {
                data.push(column.values[row]);
            }
        }
        let mut col_names = Vec::with_capacity(n_cols);
        col_names.push("time".to_string());
        col_names.extend(self.columns.iter().map(|column| column.name.clone()));
        CombiTimeTable {
            name: name.into(),
            n_rows,
            n_cols,
            data,
            col_names: Some(col_names),
        }
    }
}

/// One encoded captured signal.
#[derive(Clone, Debug, PartialEq)]
pub struct CapturedColumn {
    /// Connector point path.
    pub name: String,
    /// Encoded signal kind.
    pub kind: ValueKind,
    /// Numeric CombiTimeTable-compatible values.
    pub values: Vec<f64>,
}

impl CapturedColumn {
    fn as_series<'a>(&'a self, times: &'a [f64]) -> Series<'a> {
        Series {
            x: times,
            y: &self.values,
        }
    }
}

/// One output comparison result.
#[derive(Clone, Debug, PartialEq)]
pub struct SignalComparison {
    /// Output point path.
    pub output: String,
    /// Reference CSV column name used as the oracle.
    pub reference_column: String,
    /// Tolerance applied to this output.
    pub tolerance: Tolerances,
    /// Whether indicator masking was applied.
    pub masked: bool,
    /// Funnel comparison result.
    pub result: FunnelResult,
}

/// Full driver result.
#[derive(Clone, Debug, PartialEq)]
pub struct DriverRun {
    /// Trace captured from the engine.
    pub trace: CapturedTrace,
    /// Per-output funnel comparisons against the reference table.
    pub comparisons: Vec<SignalComparison>,
    /// Drive path used.
    pub drive_mode: DriveMode,
}

/// Driver error.
#[derive(Debug)]
#[non_exhaustive]
pub enum DriverError {
    /// Verification config rejected the run.
    Config(ConfigError),
    /// CSV/table encoding rejected the run.
    Csv(CsvError),
    /// Frozen facade operation failed.
    Engine(OcError),
    /// The reference table has no usable column names.
    MissingColumnNames,
    /// A configured mapping names a reference column that is absent.
    MissingReferenceColumn(String),
    /// No mapped output points were configured.
    NoOutputs,
    /// A configured point has an unsupported or unknown kind.
    UnsupportedPointKind(String),
    /// A Boolean reference/input cell was not encoded as 0.0 or 1.0.
    InvalidBooleanCell {
        /// Column name.
        column: String,
        /// Offending value.
        value: f64,
    },
    /// An Integer reference/input cell was not exactly representable as `i64`.
    InvalidIntegerCell {
        /// Column name.
        column: String,
        /// Offending value.
        value: f64,
    },
    /// A comparison needed a captured column that is missing from the trace.
    MissingCapturedColumn(String),
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverError::Config(err) => write!(f, "{err}"),
            DriverError::Csv(err) => write!(f, "{err}"),
            DriverError::Engine(err) => write!(f, "{err}"),
            DriverError::MissingColumnNames => {
                write!(f, "reference table must include a # columns: header")
            }
            DriverError::MissingReferenceColumn(name) => {
                write!(f, "reference table is missing column {name:?}")
            }
            DriverError::NoOutputs => write!(f, "verification config mapped no output points"),
            DriverError::UnsupportedPointKind(kind) => {
                write!(f, "unsupported point kind {kind:?}")
            }
            DriverError::InvalidBooleanCell { column, value } => {
                write!(f, "column {column:?} has non-Boolean encoded value {value}")
            }
            DriverError::InvalidIntegerCell { column, value } => {
                write!(f, "column {column:?} has non-integer encoded value {value}")
            }
            DriverError::MissingCapturedColumn(name) => {
                write!(f, "captured trace is missing column {name:?}")
            }
        }
    }
}

impl Error for DriverError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DriverError::Config(err) => Some(err),
            DriverError::Csv(err) => Some(err),
            DriverError::Engine(err) => Some(err),
            DriverError::MissingColumnNames
            | DriverError::MissingReferenceColumn(_)
            | DriverError::NoOutputs
            | DriverError::UnsupportedPointKind(_)
            | DriverError::InvalidBooleanCell { .. }
            | DriverError::InvalidIntegerCell { .. }
            | DriverError::MissingCapturedColumn(_) => None,
        }
    }
}

impl From<ConfigError> for DriverError {
    fn from(value: ConfigError) -> Self {
        Self::Config(value)
    }
}

impl From<CsvError> for DriverError {
    fn from(value: CsvError) -> Self {
        Self::Csv(value)
    }
}

impl From<OcError> for DriverError {
    fn from(value: OcError) -> Self {
        Self::Engine(value)
    }
}

/// Drive the engine using the default auto cadence and reference-table input replay.
///
/// # Errors
/// Returns [`DriverError`] for invalid config/reference data or any typed facade error.
pub fn drive_trace(
    cxf: &[u8],
    config: &VerifyConfig,
    reference: &CombiTimeTable,
) -> Result<DriverRun, DriverError> {
    drive_trace_with_options(cxf, config, reference, &DriverOptions::default())
}

/// Drive the engine with explicit options.
///
/// # Errors
/// Returns [`DriverError`] for invalid config/reference data or any typed facade error.
pub fn drive_trace_with_options(
    cxf: &[u8],
    config: &VerifyConfig,
    reference: &CombiTimeTable,
    options: &DriverOptions,
) -> Result<DriverRun, DriverError> {
    config.validate()?;
    validate_reference_shape(reference)?;

    let mut engine = Engine::in_memory();
    engine.load_cxf(cxf)?;

    let plan = Plan::new(&engine, config, reference)?;
    let (trace, drive_mode) = match &options.cadence {
        DriveCadence::Auto => {
            if let Some((t_start, t_stop, step)) = uniform_grid(&plan.times) {
                run_uniform(engine, &plan, t_start, t_stop, step, &options.input_replay)?
            } else {
                let instants = sorted_deduped(&plan.times);
                run_event_aligned(engine, &plan, instants)?
            }
        }
        DriveCadence::Uniform {
            t_start,
            t_stop,
            step,
        } => run_uniform(
            engine,
            &plan,
            *t_start,
            *t_stop,
            *step,
            &options.input_replay,
        )?,
        DriveCadence::EventAligned { instants } => {
            run_event_aligned(engine, &plan, instants.clone())?
        }
    };
    let comparisons = compare_outputs(config, &plan, &trace)?;
    Ok(DriverRun {
        trace,
        comparisons,
        drive_mode,
    })
}

#[derive(Clone, Debug)]
struct Plan {
    times: Vec<f64>,
    inputs: Vec<InputColumn>,
    outputs: Vec<OutputColumn>,
    capture: Vec<CaptureColumn>,
}

impl Plan {
    fn new(
        engine: &Engine,
        config: &VerifyConfig,
        reference: &CombiTimeTable,
    ) -> Result<Self, DriverError> {
        let point_info = engine
            .io()
            .iter()
            .map(|point| (point.path.clone(), point))
            .collect::<HashMap<_, _>>();
        let names = column_names(reference)?;
        let times = table_times(reference)?;
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();

        for reference_spec in &config.references {
            for mapping in &reference_spec.point_name_mapping {
                let column = find_column(reference, names, &mapping.device.name)
                    .or_else(|| find_column(reference, names, &mapping.cdl.name))
                    .ok_or_else(|| {
                        DriverError::MissingReferenceColumn(mapping.device.name.clone())
                    })?;
                let point = point_info.get(&mapping.cdl.name);
                let fallback_kind = point_end_kind(mapping.cdl.kind.as_deref())?;
                match point.map(|p| p.direction) {
                    Some(PointDirection::In) => {
                        let kind = point_value_kind(point.expect("point checked").value_type)?;
                        inputs.push(InputColumn {
                            point: mapping.cdl.name.clone(),
                            values: decode_values(reference, column, kind, &names[column])?,
                        });
                    }
                    Some(PointDirection::Out) => {
                        let kind = point_value_kind(point.expect("point checked").value_type)?;
                        outputs.push(OutputColumn {
                            point: mapping.cdl.name.clone(),
                            reference_column: names[column].clone(),
                            reference_values: reference_values(reference, column),
                            kind,
                        });
                    }
                    None => {
                        outputs.push(OutputColumn {
                            point: mapping.cdl.name.clone(),
                            reference_column: names[column].clone(),
                            reference_values: reference_values(reference, column),
                            kind: fallback_kind,
                        });
                    }
                }
            }
        }

        if outputs.is_empty() {
            return Err(DriverError::NoOutputs);
        }

        let mut capture = Vec::new();
        for output in &outputs {
            push_capture(&mut capture, &point_info, &output.point, output.kind)?;
            for signal in config.indicator_signals_for_output(&output.point)? {
                let kind = point_info
                    .get(&signal)
                    .map(|point| point_value_kind(point.value_type))
                    .transpose()?
                    .unwrap_or(ValueKind::Boolean);
                push_capture(&mut capture, &point_info, &signal, kind)?;
            }
        }

        Ok(Self {
            times,
            inputs,
            outputs,
            capture,
        })
    }
}

#[derive(Clone, Debug)]
struct InputColumn {
    point: String,
    values: Vec<Value>,
}

impl InputColumn {
    fn value_at(&self, times: &[f64], t: f64) -> Value {
        debug_assert_eq!(times.len(), self.values.len());
        let mut index = 0;
        for (i, sample_t) in times.iter().enumerate() {
            if *sample_t > t {
                break;
            }
            index = i;
        }
        self.values[index].clone()
    }
}

#[derive(Clone, Debug)]
struct OutputColumn {
    point: String,
    reference_column: String,
    reference_values: Vec<f64>,
    kind: ValueKind,
}

#[derive(Clone, Debug)]
struct CaptureColumn {
    point: String,
    kind: ValueKind,
}

fn run_uniform(
    mut engine: Engine,
    plan: &Plan,
    t_start: f64,
    t_stop: f64,
    step: f64,
    input_replay: &DriverInputReplay,
) -> Result<(CapturedTrace, DriveMode), DriverError> {
    let collect_points = plan
        .capture
        .iter()
        .map(|column| column.point.clone())
        .collect::<Vec<_>>();
    let inputs = match input_replay {
        DriverInputReplay::ReferenceTable => {
            let times = plan.times.clone();
            let columns = plan.inputs.clone();
            InputSource::Closure(Box::new(move |t| {
                columns
                    .iter()
                    .map(|column| (column.point.clone(), column.value_at(&times, t)))
                    .collect()
            }))
        }
        DriverInputReplay::FacadeCsv { path, bindings } => InputSource::Csv {
            path: path.clone(),
            bindings: bindings.clone(),
        },
    };
    let metrics = engine.simulate(&SimSpec {
        t_start,
        t_stop,
        step,
        inputs,
        collect: CollectSpec::Named {
            points: collect_points,
            stride: 1,
        },
    })?;
    let trace = trace_from_output_trace(&metrics.trace, &plan.capture)?;
    Ok((
        trace,
        DriveMode::Uniform {
            t_start,
            t_stop,
            step,
        },
    ))
}

fn run_event_aligned(
    mut engine: Engine,
    plan: &Plan,
    instants: Vec<f64>,
) -> Result<(CapturedTrace, DriveMode), DriverError> {
    let mut columns = plan
        .capture
        .iter()
        .map(|column| CapturedColumn {
            name: column.point.clone(),
            kind: column.kind,
            values: Vec::with_capacity(instants.len()),
        })
        .collect::<Vec<_>>();
    for &t in &instants {
        for input in &plan.inputs {
            engine.set_input(&input.point, input.value_at(&plan.times, t))?;
        }
        engine.step_realtime(t)?;
        for (idx, capture) in plan.capture.iter().enumerate() {
            let value = engine.get_output(&capture.point)?;
            columns[idx].values.push(capture.kind.encode_value(&value)?);
        }
    }
    Ok((
        CapturedTrace {
            times: instants.clone(),
            columns,
        },
        DriveMode::EventAligned { instants },
    ))
}

fn trace_from_output_trace(
    trace: &OutputTrace,
    capture: &[CaptureColumn],
) -> Result<CapturedTrace, DriverError> {
    let mut columns = Vec::with_capacity(capture.len());
    for (idx, capture_column) in capture.iter().enumerate() {
        let values = trace
            .column(idx)
            .ok_or_else(|| DriverError::MissingCapturedColumn(capture_column.point.clone()))?
            .iter()
            .map(|value| capture_column.kind.encode_value(value))
            .collect::<Result<Vec<_>, _>>()?;
        columns.push(CapturedColumn {
            name: trace
                .columns()
                .get(idx)
                .cloned()
                .unwrap_or_else(|| capture_column.point.clone()),
            kind: capture_column.kind,
            values,
        });
    }
    Ok(CapturedTrace {
        times: trace.times().to_vec(),
        columns,
    })
}

fn compare_outputs(
    config: &VerifyConfig,
    plan: &Plan,
    trace: &CapturedTrace,
) -> Result<Vec<SignalComparison>, DriverError> {
    let mut comparisons = Vec::with_capacity(plan.outputs.len());
    for output in &plan.outputs {
        let captured = trace
            .column(&output.point)
            .ok_or_else(|| DriverError::MissingCapturedColumn(output.point.clone()))?;
        let tolerance = config.tolerance_for_output(&output.point)?;
        let reference = Series {
            x: &plan.times,
            y: &output.reference_values,
        };
        let test = captured.as_series(&trace.times);
        let indicator_signals = config.indicator_signals_for_output(&output.point)?;
        let result = if indicator_signals.is_empty() {
            compare(reference, test, &tolerance)
        } else {
            let mut indicators = Vec::with_capacity(indicator_signals.len());
            for signal in indicator_signals {
                let column = trace
                    .column(&signal)
                    .ok_or_else(|| DriverError::MissingCapturedColumn(signal.clone()))?;
                indicators.push(Indicator {
                    signal,
                    samples: trace
                        .times
                        .iter()
                        .copied()
                        .zip(column.values.iter().map(|value| *value != 0.0))
                        .collect(),
                });
            }
            compare_masked(reference, test, &tolerance, &Mask { indicators })
        };
        comparisons.push(SignalComparison {
            output: output.point.clone(),
            reference_column: output.reference_column.clone(),
            tolerance,
            masked: !config
                .indicator_signals_for_output(&output.point)?
                .is_empty(),
            result,
        });
    }
    Ok(comparisons)
}

fn validate_reference_shape(table: &CombiTimeTable) -> Result<(), DriverError> {
    table.to_csv_string().map(|_| ()).map_err(DriverError::Csv)
}

fn column_names(table: &CombiTimeTable) -> Result<&[String], DriverError> {
    let names = table
        .col_names
        .as_deref()
        .ok_or(DriverError::MissingColumnNames)?;
    if names.len() != table.n_cols {
        return Err(DriverError::MissingColumnNames);
    }
    Ok(names)
}

fn table_times(table: &CombiTimeTable) -> Result<Vec<f64>, DriverError> {
    let mut times = Vec::with_capacity(table.n_rows);
    for row in 0..table.n_rows {
        times.push(table.data[row * table.n_cols]);
    }
    Ok(times)
}

fn find_column(table: &CombiTimeTable, names: &[String], name: &str) -> Option<usize> {
    names
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(idx, column)| (column == name && idx < table.n_cols).then_some(idx))
}

fn decode_values(
    table: &CombiTimeTable,
    column: usize,
    kind: ValueKind,
    column_name: &str,
) -> Result<Vec<Value>, DriverError> {
    let mut values = Vec::with_capacity(table.n_rows);
    for row in 0..table.n_rows {
        let value = table.data[row * table.n_cols + column];
        values.push(decode_value(kind, value, column_name)?);
    }
    Ok(values)
}

fn reference_values(table: &CombiTimeTable, column: usize) -> Vec<f64> {
    (0..table.n_rows)
        .map(|row| table.data[row * table.n_cols + column])
        .collect()
}

fn decode_value(kind: ValueKind, value: f64, column: &str) -> Result<Value, DriverError> {
    match kind {
        ValueKind::Real => Ok(Value::Real(value)),
        ValueKind::Boolean if value.to_bits() == 0.0f64.to_bits() => Ok(Value::Boolean(false)),
        ValueKind::Boolean if value.to_bits() == 1.0f64.to_bits() => Ok(Value::Boolean(true)),
        ValueKind::Boolean => Err(DriverError::InvalidBooleanCell {
            column: column.to_string(),
            value,
        }),
        ValueKind::Integer => {
            if value.fract() == 0.0
                && value.abs() <= 9_007_199_254_740_992.0
                && value >= i64::MIN as f64
                && value <= i64::MAX as f64
            {
                Ok(Value::Integer(value as i64))
            } else {
                Err(DriverError::InvalidIntegerCell {
                    column: column.to_string(),
                    value,
                })
            }
        }
    }
}

fn push_capture(
    capture: &mut Vec<CaptureColumn>,
    point_info: &HashMap<String, PointInfo>,
    point: &str,
    fallback_kind: ValueKind,
) -> Result<(), DriverError> {
    if capture.iter().any(|column| column.point == point) {
        return Ok(());
    }
    let kind = point_info
        .get(point)
        .map(|info| point_value_kind(info.value_type))
        .transpose()?
        .unwrap_or(fallback_kind);
    capture.push(CaptureColumn {
        point: point.to_string(),
        kind,
    });
    Ok(())
}

fn point_value_kind(value_type: PointValueType) -> Result<ValueKind, DriverError> {
    match value_type {
        PointValueType::Real => Ok(ValueKind::Real),
        PointValueType::Int => Ok(ValueKind::Integer),
        PointValueType::Bool => Ok(ValueKind::Boolean),
    }
}

fn point_end_kind(kind: Option<&str>) -> Result<ValueKind, DriverError> {
    match kind.unwrap_or("Real") {
        "Real" | "real" => Ok(ValueKind::Real),
        "Integer" | "Int" | "integer" | "int" => Ok(ValueKind::Integer),
        "Boolean" | "Bool" | "boolean" | "bool" => Ok(ValueKind::Boolean),
        other => Err(DriverError::UnsupportedPointKind(other.to_string())),
    }
}

fn uniform_grid(times: &[f64]) -> Option<(f64, f64, f64)> {
    if times.len() < 2 {
        return None;
    }
    let t_start = *times.first()?;
    let t_stop = *times.last()?;
    let step = times[1] - times[0];
    if !step.is_finite() || step <= 0.0 {
        return None;
    }
    let exact = times.iter().enumerate().all(|(idx, &t)| {
        let want = t_start + (idx as f64) * step;
        t.to_bits() == want.to_bits()
    });
    exact.then_some((t_start, t_stop, step))
}

fn sorted_deduped(times: &[f64]) -> Vec<f64> {
    let mut out = times.to_vec();
    out.sort_by(f64::total_cmp);
    out.dedup_by(|a, b| a.to_bits() == b.to_bits());
    out
}
