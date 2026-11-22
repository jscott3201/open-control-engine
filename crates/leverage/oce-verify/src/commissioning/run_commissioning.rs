//! Running one reference of a commissioning spec and issuing its receipt (commissioning note,
//! "Sources, sampling and time", "Indicators", "Report and receipt" and decisions 27 to 42).
//!
//! [`run_commissioning`] does, in this order, every refusal an error with no receipt:
//!
//! 1. **The reference**: in range (`reference-out-of-range`), with a `cxf` and a `window`
//!    (`reference-incomplete`, naming the field missing first, `cxf` before `window`), and a
//!    `sampling`, its own or the spec's (`sampling-required`; owner ruling of 2026-11-21).
//! 2. **The sequence**: the CXF loaded through the facade (`subject-load-failed`).
//! 3. **The trends** ([`super::TrendSources::read`]): `too-many-sources`, or `source-refused` with
//!    the source reader's code as the cause.
//! 4. **Bind** ([`super::bind_reference`]): the boundary, the mapped points, the compared
//!    outputs, the keys, the indicators and the parameters.
//! 5. **The bounds** ([`super::run_bounds`]), before the replay: the instants per output, the
//!    run's work, the grid envelope's work (`too-many-points`) and the trace's bytes
//!    (`replay-failed`, cause `trace-too-large`).
//! 6. **The parameter edits**, applied in passes inside one halt on the engine just loaded
//!    (`param-refused`; decision 29), before any frame. The header keeps the content id as
//!    loaded and the body takes the id after the edits (decision 30).
//! 7. **The replay** ([`super::drive`]): `frame-refused`, `run-refused`, `replay-failed`.
//! 8. **The indicators** (`indicator-not-boolean`) and **the comparison** of each output
//!    ([`super::compare_output`]; `compare-refused`).
//!
//! **What a run holds.** The trend sources, bounded by [`super::TrendLimits`] (the merged
//! inputs up to 1 GiB); the trace, frames times the bytes of a row, checked against the
//! replay's `max_trace_bytes` (1 GiB) before the replay; the instants (8 bytes each); one
//! output's held series, mask and curves, a few `f64` per instant under the per-output cap; the
//! indicator series, one per distinct signal; and a receipt whose lists are complete and bounded
//! at bind (`parameters` 256, `sources` 256, `outputs` 4,096) or capped by constants (decision
//! 42). The design note's "As landed" states the measured peak.

use oce_api::{ENGINE_VERSION, Topology, ValueType};
use oce_replay::{FrameStatus, TraceColumn};

use super::bind_ports::Boundary;
use super::bind_reference::{BindInputs, bind_reference};
use super::choices::FunnelReference;
use super::compare_output::{Comparing, OutputPlan, Sampled, Values, compare_output};
use super::drive::{drive, executed_rows, uncovered_at, uncovered_stretches};
use super::indicator_mask::{IndicatorSeries, active_at, indicator_series};
use super::prepare::Instants;
use super::report::{
    CommissioningBody, CommissioningParam, FindingRecord, MAX_FINDINGS, MAX_RUN_POINTS,
    MAX_RUN_WARNINGS, MAX_UNIT_COLUMNS, MAX_UNVERIFIED_OUTPUTS, OutputStatus, SpecNumber,
    TimeRecord, capped,
};
use super::run_bounds::{Caps, bytes_per_frame, check_points, check_trace_bytes};
use super::run_options::{CommissionOptions, CommissionRun, MAX_TOTAL_ALIGNED_GRID_WORK};
use super::source_error::{SourceError, SourceErrorKind};
use super::spec::CommissioningSpec;
use super::trend_types::{SeriesValues, SpecInputs, TrendSeries, TrendSources};
use crate::envelope::Envelope;
use crate::limits::{MAX_LOAD_WARNINGS, lowered};
use crate::receipt::{
    Comparison, RECEIPT_SCHEMA, Receipt, ReceiptBody, ReceiptHeader, ReceiptKind, ReceiptValue,
    UnmatchedWarning,
};
use crate::verify_error::{VerifyError, VerifyErrorKind as Kind};

/// Run the reference at `reference` of `spec` against the bytes in `inputs` and issue its
/// receipt. Blocks until the run is compared; opens no file and reads no clock.
///
/// An output that fails, or compares nothing, is a verdict inside the receipt (`passed` false),
/// never an error. The same spec, bytes and options give the same receipt bytes on every target,
/// whatever [`CommissionOptions::chunk_frames`] and [`CommissionOptions::timings`] say.
///
/// ```
/// use oce_verify::{
///     CommissionOptions, CommissioningSpec, OutputStatus, ReceiptBody, SpecInputs, SpecLimits,
///     run_commissioning,
/// };
///
/// // One `CDL.Logical.Not`, its input trended as `Fan` and its output as `Alarm`.
/// let cxf = br#"{"@context": {"S231": "http://data.ashrae.org/S231#"}, "@graph": [
///   {"@id": "urn:doc#top", "@type": "S231:Block",
///    "S231:containsBlock": [{"@id": "urn:doc#top.not"}],
///    "S231:hasInput": [{"@id": "urn:doc#top.u"}], "S231:hasOutput": [{"@id": "urn:doc#top.y"}]},
///   {"@id": "urn:doc#top.not", "@type": "CDL.Logical.Not",
///    "S231:hasInput": {"@id": "urn:doc#top.not.u"}, "S231:hasOutput": {"@id": "urn:doc#top.not.y"}},
///   {"@id": "urn:doc#top.not.u", "@type": "S231:BooleanInput",
///    "S231:isOfDataType": {"@id": "S231:Boolean"}},
///   {"@id": "urn:doc#top.not.y", "@type": "S231:BooleanOutput",
///    "S231:isOfDataType": {"@id": "S231:Boolean"}, "S231:isConnectedTo": {"@id": "urn:doc#top.y"}},
///   {"@id": "urn:doc#top.u", "@type": "S231:BooleanInput",
///    "S231:isOfDataType": {"@id": "S231:Boolean"}, "S231:isConnectedTo": {"@id": "urn:doc#top.not.u"}},
///   {"@id": "urn:doc#top.y", "@type": "S231:BooleanOutput",
///    "S231:isOfDataType": {"@id": "S231:Boolean"}}
/// ]}"#;
/// let spec = CommissioningSpec::from_json_str(r#"{"references": [{
///   "model": "Inverter", "sequence": "inv", "cxf": "inv.json",
///   "pointNameMapping": [
///     {"cdl": {"name": "u", "type": "bool"}, "device": {"name": "Fan", "type": "bool"}},
///     {"cdl": {"name": "y", "type": "bool"}, "device": {"name": "Alarm", "type": "bool"}}],
///   "window": {"start": 0, "stop": 20}, "sampling": 10,
///   "inputs": [{"path": "fan.csv", "format": "csv", "timeColumn": "time",
///               "columns": {"Fan": "Fan"}}],
///   "expected": [{"path": "alarm.csv", "format": "csv", "timeColumn": "time",
///                 "columns": {"Alarm": "Alarm"}}]}]}"#, &SpecLimits::default())?;
/// let inputs = SpecInputs {
///     cxf: cxf.to_vec(),
///     files: vec![
///         ("fan.csv".to_owned(), b"time,Fan\n0,true\n10,false\n20,false\n".to_vec()),
///         ("alarm.csv".to_owned(), b"time,Alarm\n0,false\n10,true\n20,false\n".to_vec()),
///     ],
/// };
/// let run = run_commissioning(&spec, 0, &inputs, &CommissionOptions::default())?;
/// let ReceiptBody::Commissioning(body) = &run.receipt.body else {
///     unreachable!("a commissioning run issues a commissioning receipt");
/// };
/// // The trended alarm is wrong at 20 s: a Boolean mismatch is an exceedance of 1 there.
/// let output = &body.outputs[0];
/// assert_eq!(output.status, OutputStatus::Fail);
/// assert_eq!(output.worst.map(|w| (w.t, w.exceedance)), Some((20.0, 1.0)));
/// assert_eq!(run.receipt.header.comparison.envelope_version(), Some("funnel@a12fa96"));
/// assert!(!run.receipt.header.passed);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
/// A [`VerifyError`] whose code is one of [`crate::VERIFY_ERROR_CODES`], for every refusal in the
/// order the module documentation gives; then no receipt exists.
pub fn run_commissioning(
    spec: &CommissioningSpec,
    reference: usize,
    inputs: &SpecInputs,
    options: &CommissionOptions,
) -> Result<CommissionRun, VerifyError> {
    let place = format!("references[{reference}]");
    let Some(written) = spec.references().get(reference) else {
        return Err(VerifyError::new(
            Kind::ReferenceOutOfRange,
            "references",
            format!(
                "the spec has {} references, so no reference {reference}",
                spec.references().len()
            ),
        ));
    };
    if written.cxf().is_none() {
        return Err(incomplete(&place, "cxf"));
    }
    let Some(window) = written.window() else {
        return Err(incomplete(&place, "window"));
    };
    let Some(sampling) = written.sampling.or(spec.sampling) else {
        return Err(VerifyError::new(
            Kind::SamplingRequired,
            format!("{place}.sampling"),
            "neither the reference nor the spec gives a `sampling`, which a run needs: without a \
             grid the envelope would judge a held value between two samples as a ramp",
        ));
    };
    let (mut engine, load) = crate::bind::load_at(&inputs.cxf, &format!("{place}.cxf"))?;
    let setup = |call: &str, error: &oce_api::OcError| {
        crate::bind::setup_failed(place.as_str(), call, error)
    };
    let content_id = engine
        .content_id()
        .map_err(|error| setup("its content id", &error))?
        .to_string();
    let definitions = engine
        .input_definitions()
        .map_err(|error| setup("its input definitions", &error))?;
    let slots = engine
        .frame_slots()
        .map_err(|error| setup("its slot map", &error))?;
    let outputs = slots.outputs().to_vec();
    let output_types = slots.output_types().to_vec();
    let topology: Topology = engine.topology();

    let trends = TrendSources::read(spec, reference, inputs, &options.trends)
        .map_err(|error| source_refused(&place, &error))?;
    let params = crate::bind::parameter_types(&engine);
    let bound = bind_reference(&BindInputs {
        spec,
        index: reference,
        reference: written,
        boundary: Boundary {
            inputs: &definitions,
            outputs: &outputs,
            output_types: &output_types,
        },
        topology: &topology,
        params: &params,
        trends: &trends,
    })?;

    let envelope = written.envelope().unwrap_or(Envelope::Funnel);
    let role = written
        .funnel_reference()
        .unwrap_or(FunnelReference::Engine);
    let window_instants = Instants {
        start: window.start_s(),
        stop: window.stop_s(),
        step: sampling.seconds(),
    };
    let count = window_instants.count(MAX_RUN_POINTS);
    let compare = options.compare.effective();
    check_points(
        &bound,
        &Caps {
            count,
            envelope,
            limits: &compare,
            aligned_grid_work: options
                .max_total_aligned_grid_work
                .min(MAX_TOTAL_ALIGNED_GRID_WORK),
        },
        &place,
    )?;
    let replay_limits = lowered(&options.replay);
    check_trace_bytes(
        trends.inputs.times().len(),
        bytes_per_frame(
            &definitions,
            &output_types,
            !trends.inputs.overrides().is_empty(),
        ),
        replay_limits.max_trace_bytes,
        &place,
    )?;
    let instants = window_instants.instants(count);

    let edits: Vec<(&str, ReceiptValue)> = bound
        .edits
        .iter()
        .map(|edit| (edit.path.as_str(), edit.value))
        .collect();
    crate::parameters::apply(&mut engine, &edits, &format!("{place}.parameters"))?;
    let edited_id = if edits.is_empty() {
        None
    } else {
        Some(
            engine
                .content_id()
                .map_err(|error| setup("the edited content id", &error))?
                .to_string(),
        )
    };

    let TrendSources {
        inputs: merged,
        expected,
        indicators: indicator_files,
        sources,
        mapping,
        unit_columns,
    } = trends;
    let trace = drive(
        engine,
        &definitions,
        merged,
        &replay_limits,
        options.chunk_frames,
        &place,
    )?;
    let rows = executed_rows(&trace);
    let (uncovered, _) = uncovered_at(&uncovered_stretches(&trace), &instants);
    let indicator_series: Vec<IndicatorSeries> = bound
        .indicators
        .iter()
        .map(|indicator| {
            indicator_series(
                indicator,
                &trace,
                &rows,
                &indicator_files,
                &format!("{place}.indicators"),
            )
        })
        .collect::<Result<_, _>>()?;
    let engine_times: Vec<f64> = rows.iter().map(|row| trace.times()[*row]).collect();

    let comparing = Comparing {
        envelope,
        role,
        limits: compare,
        bounds: options.bounds,
    };
    let mut reports = Vec::with_capacity(bound.compared.len());
    let mut curves = Vec::with_capacity(bound.compared.len());
    for output in &bound.compared {
        // A trace whose engine accepted no frame has no output column: no engine sample.
        let column = trace
            .outputs()
            .iter()
            .find(|(path, _)| *path == output.path)
            .map(|(_, column)| column);
        let engine_values = engine_values(column, output.value_type, &rows);
        let series = &expected[output.expected];
        let trend_values = trend_values(series);
        let gates: Vec<&IndicatorSeries> = output
            .indicators
            .iter()
            .map(|index| &indicator_series[*index])
            .collect();
        let active = active_at(&gates, &instants);
        let plan = OutputPlan {
            path: &output.path,
            value_type: output.value_type,
            engine: Sampled {
                times: &engine_times,
                values: engine_values.as_values(),
            },
            trend: Sampled {
                times: &series.times,
                values: trend_values.as_values(),
            },
            instants: &instants,
            tolerances: output.tolerances,
            active: &active,
            uncovered: &uncovered,
        };
        let (report, curve) = compare_output(&plan, &comparing)?;
        reports.push(report);
        curves.push(curve);
    }

    let status = if reports.iter().any(|r| r.status == OutputStatus::Fail) {
        OutputStatus::Fail
    } else if reports.is_empty() || reports.iter().any(|r| r.status == OutputStatus::NoData) {
        OutputStatus::NoData
    } else {
        OutputStatus::Pass
    };
    let mut frames = crate::run::frame_counts(&trace);
    // An engine refusal leaves no receipt, so every refused row is a missing sample.
    frames.refused = u64::try_from(
        trace
            .status()
            .iter()
            .filter(|byte| FrameStatus::from_byte(**byte) == Some(FrameStatus::RefusedMissing))
            .count(),
    )
    .unwrap_or(u64::MAX);
    let warnings: Vec<UnmatchedWarning> = trace
        .diagnostics()
        .iter()
        .map(|row| UnmatchedWarning {
            t: row.time,
            block: row.block.clone(),
            message: row.message.clone(),
        })
        .collect();
    let (warnings, warnings_total) = capped(warnings, MAX_RUN_WARNINGS);
    let (unverified_outputs, unverified_outputs_total) =
        capped(bound.unverified, MAX_UNVERIFIED_OUTPUTS);
    let findings: Vec<FindingRecord> = spec
        .findings_for(reference)
        .into_iter()
        .map(|finding| FindingRecord {
            code: finding.code().to_owned(),
            at: finding.at().to_owned(),
        })
        .collect();
    let (findings, findings_total) = capped(findings, MAX_FINDINGS);
    let (unit_columns, unit_columns_total) = capped(unit_columns, MAX_UNIT_COLUMNS);
    let body = CommissioningBody {
        status,
        reference: u64::try_from(reference).unwrap_or(u64::MAX),
        model: written.model().to_owned(),
        sequence: written.sequence().to_owned(),
        data: options.data,
        parameters: bound
            .edits
            .iter()
            .map(|edit| CommissioningParam {
                path: edit.path.clone(),
                value: edit.written,
            })
            .collect(),
        content_id: edited_id,
        time: TimeRecord {
            origin: spec.time_origin().cloned(),
            window_start: SpecNumber(window.start),
            window_stop: SpecNumber(window.stop),
            sampling: SpecNumber(sampling),
        },
        mapping,
        unit_columns,
        unit_columns_total,
        sources,
        frames,
        load_warnings: load
            .warnings
            .iter()
            .take(MAX_LOAD_WARNINGS)
            .map(|warning| warning.code.as_str().to_owned())
            .collect(),
        load_warnings_total: u64::try_from(load.warnings.len()).unwrap_or(u64::MAX),
        warnings,
        warnings_total,
        outputs: reports,
        unverified_outputs,
        unverified_outputs_total,
        findings,
        findings_total,
    };
    let mut receipt = Receipt {
        header: ReceiptHeader {
            schema: RECEIPT_SCHEMA.to_owned(),
            kind: ReceiptKind::Commissioning,
            content_id,
            engine_version: crate::bind::engine_version(ENGINE_VERSION),
            spec_id: spec.spec_id(),
            comparison: Comparison::tolerance(envelope),
            passed: status == OutputStatus::Pass,
            receipt_id: String::new(),
            timings: options.timings.unwrap_or_default(),
        },
        body: ReceiptBody::Commissioning(body),
    };
    receipt.header.receipt_id = receipt.computed_receipt_id();
    Ok(CommissionRun {
        receipt,
        trace,
        curves,
    })
}

fn incomplete(place: &str, field: &str) -> VerifyError {
    VerifyError::new(
        Kind::ReferenceIncomplete,
        format!("{place}.{field}"),
        format!("the reference has no `{field}`, which a run needs; it loads and cannot run"),
    )
}

/// A source reader's refusal as the run's: `too-many-sources`, `reference-out-of-range` or
/// `source-refused` with the reader's code as the cause, at the file it names.
fn source_refused(place: &str, error: &SourceError) -> VerifyError {
    let kind = match error.kind() {
        SourceErrorKind::TooManySources => Kind::TooManySources,
        SourceErrorKind::ReferenceOutOfRange => Kind::ReferenceOutOfRange,
        _ => Kind::SourceRefused,
    };
    let subject = error.file().map_or_else(|| place.to_owned(), str::to_owned);
    let refusal = VerifyError::new(kind, subject, error.to_string());
    if kind == Kind::SourceRefused {
        refusal.with_cause(error.code())
    } else {
        refusal
    }
}

/// One output's engine values at the executed rows, as the comparison takes them.
enum OwnedValues {
    Real(Vec<f64>),
    Discrete(Vec<i64>),
}

impl OwnedValues {
    fn as_values(&self) -> Values<'_> {
        match self {
            Self::Real(values) => Values::Real(values),
            Self::Discrete(values) => Values::Discrete(values),
        }
    }
}

fn engine_values(
    column: Option<&TraceColumn>,
    value_type: ValueType,
    rows: &[usize],
) -> OwnedValues {
    let Some(column) = column else {
        return match value_type {
            ValueType::Real => OwnedValues::Real(Vec::new()),
            _ => OwnedValues::Discrete(Vec::new()),
        };
    };
    match column {
        TraceColumn::Real(values) => OwnedValues::Real(rows.iter().map(|r| values[*r]).collect()),
        TraceColumn::Integer(values) => {
            OwnedValues::Discrete(rows.iter().map(|r| values[*r]).collect())
        }
        TraceColumn::Boolean(values) => {
            OwnedValues::Discrete(rows.iter().map(|r| i64::from(values[*r])).collect())
        }
        TraceColumn::Enum(values) => {
            OwnedValues::Discrete(rows.iter().map(|r| i64::from(values[*r])).collect())
        }
    }
}

fn trend_values(series: &TrendSeries) -> OwnedValues {
    match &series.values {
        SeriesValues::Real(values) => OwnedValues::Real(values.clone()),
        SeriesValues::Integer(values) => {
            OwnedValues::Discrete(values.iter().map(|x| i64::from(*x)).collect())
        }
        SeriesValues::Boolean(values) => {
            OwnedValues::Discrete(values.iter().map(|x| i64::from(*x)).collect())
        }
    }
}

#[cfg(test)]
#[path = "run_commissioning_tests.rs"]
mod tests;
