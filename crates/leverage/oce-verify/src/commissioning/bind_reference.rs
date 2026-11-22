//! Binding one reference of a commissioning spec to its sequence, before any frame (commissioning
//! note, decisions 17, 27, 29, 34, 40 and 42).
//!
//! After the sequence's boundary (an Enum port `enum-port-unsupported`, a String port
//! `unsupported-port-type`) and its mapped points ([`super::bind_ports`]), in this order, the
//! first refusal winning:
//!
//! 1. **The compared outputs**: those an `expected` source supplies (decision 40), at most
//!    [`MAX_COMPARED_OUTPUTS`] (`too-many-outputs`). Every other boundary output is unverified,
//!    listed in the receipt and changing no verdict.
//! 2. **The keys**: the `outputs` keys, then the `indicators` keys, matched against every boundary
//!    output by #658's selection; its refusal is `spec-refused` with the spec reader's code as the
//!    cause (`unmatched-key`, `ambiguous-output`, `conflicting-override`,
//!    `work-limit-exceeded`).
//! 3. **The indicator names** of every rule, in key order and then list order, each once: a name
//!    that starts with `<sequence>.` resolves its rest against the inputs and outputs together;
//!    any other must equal the `cdl.name` of a point an `indicatorSources` source reads.
//!    Neither is `unknown-indicator` (decision 27). At most [`MAX_INDICATORS`] distinct names
//!    (`too-many-indicators`).
//! 4. **The parameters**, in path order: each key resolves against `Engine::params()`
//!    (`param-path-unknown`, `param-path-ambiguous`, two keys on one parameter
//!    `param-path-duplicate`) and its value is typed by the parameter (`value-type-mismatch`,
//!    `value-not-representable`). Values are in CDL units, never converted (decision 24).

use std::collections::BTreeMap;

use oce_api::ValueType;
use oce_replay::PathIndex;

use super::bind_ports::{Boundary, bind_ports, resolve};
use super::error::SpecError;
use super::limits::SpecLimits;
use super::model::SpecReference;
use super::report::MAX_COMPARED_OUTPUTS;
use super::spec::CommissioningSpec;
use super::tolerances::PartialTolerances;
use super::trend_types::TrendSources;
use crate::number::Literal;
use crate::receipt::ReceiptValue;
use crate::verify_error::{VerifyError, VerifyErrorKind as Kind};

/// The most distinct indicator names one reference's rules may give: one bit each in a rule's
/// set, so the rules selecting an output join in one OR apiece. Each costs one series held for
/// the comparison and one read per instant of every output it gates, which the run's work
/// budget counts.
pub const MAX_INDICATORS: usize = 64;

/// Where an indicator's values come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum IndicatorSignal {
    /// A boundary input of the sequence, by canonical path, read from the trace's retained
    /// inputs.
    Input(String),
    /// A boundary output of the sequence, by canonical path, read from the trace.
    Output(String),
    /// A point an `indicatorSources` source reads, by its index in [`TrendSources::indicators`].
    File(usize),
}

/// One distinct indicator name and its signal.
#[derive(Clone, Debug)]
pub(crate) struct BoundIndicator {
    /// The name as the spec writes it.
    pub(crate) name: String,
    /// Where its values come from.
    pub(crate) signal: IndicatorSignal,
}

/// One compared output.
#[derive(Clone, Debug)]
pub(crate) struct ComparedOutput {
    /// The canonical path.
    pub(crate) path: String,
    /// The output's type: Real, Integer or Boolean.
    pub(crate) value_type: ValueType,
    /// The index of its expected series in [`TrendSources::expected`].
    pub(crate) expected: usize,
    /// Its tolerances after the overrides.
    pub(crate) tolerances: PartialTolerances,
    /// Its indicators, as indexes into [`BoundReference::indicators`], ANDed; empty for none.
    pub(crate) indicators: Vec<usize>,
}

/// One parameter edit: the canonical path, the value as written and the value typed.
pub(crate) struct BoundEdit {
    /// The canonical parameter path.
    pub(crate) path: String,
    /// The value as the spec writes it.
    pub(crate) written: Literal,
    /// The value typed by the parameter.
    pub(crate) value: ReceiptValue,
}

/// What binding a reference learned.
pub(crate) struct BoundReference {
    /// The compared outputs, in slot order, which is canonical path byte order.
    pub(crate) compared: Vec<ComparedOutput>,
    /// The canonical paths of every other boundary output, in slot order.
    pub(crate) unverified: Vec<String>,
    /// Every distinct indicator name of the reference's rules.
    pub(crate) indicators: Vec<BoundIndicator>,
    /// The parameter edits, sorted by canonical path bytes.
    pub(crate) edits: Vec<BoundEdit>,
}

/// What a bind reads beside the spec: the reference, the sequence and the trends.
pub(crate) struct BindInputs<'a> {
    /// The spec.
    pub(crate) spec: &'a CommissioningSpec,
    /// The reference's index.
    pub(crate) index: usize,
    /// The reference.
    pub(crate) reference: &'a SpecReference,
    /// The sequence's boundary.
    pub(crate) boundary: Boundary<'a>,
    /// The sequence's topology, for its ports' units and quantities.
    pub(crate) topology: &'a oce_api::Topology,
    /// The sequence's parameters, sorted by path, as `crate::bind::parameter_types` gives them.
    pub(crate) params: &'a [(String, ValueType)],
    /// The reference's trends.
    pub(crate) trends: &'a TrendSources,
}

/// Bind the reference in `inputs` to its sequence, in the order the module documentation gives.
///
/// # Errors
/// The first refusal.
pub(crate) fn bind_reference(inputs: &BindInputs<'_>) -> Result<BoundReference, VerifyError> {
    let place = format!("references[{}]", inputs.index);
    crate::bind::boundary(
        inputs.boundary.inputs,
        inputs.boundary.outputs,
        inputs.boundary.output_types,
    )?;
    let ports = bind_ports(
        inputs.trends,
        &inputs.boundary,
        inputs.topology,
        &format!("{place}.pointNameMapping"),
    )?;
    let mut by_slot: Vec<Option<usize>> = vec![None; inputs.boundary.outputs.len()];
    for (series, slot) in ports.expected_slots.iter().enumerate() {
        by_slot[*slot] = Some(series);
    }
    let compared_count = by_slot.iter().filter(|s| s.is_some()).count();
    if compared_count > MAX_COMPARED_OUTPUTS {
        return Err(VerifyError::new(
            Kind::TooManyOutputs,
            format!("{place}.expected"),
            format!(
                "the expected sources supply {compared_count} outputs; at most {MAX_COMPARED_OUTPUTS} are compared"
            ),
        ));
    }

    let paths: Vec<&str> = inputs.boundary.outputs.iter().map(String::as_str).collect();
    let limits = SpecLimits::default();
    let spec_refused = |map: &str, error: SpecError| {
        VerifyError::new(
            Kind::SpecRefused,
            format!("{place}.{map}"),
            error.to_string(),
        )
        .with_cause(error.code())
    };
    let selection = inputs
        .reference
        .select_output_keys(&paths, &limits)
        .map_err(|error| spec_refused("outputs", error))?;
    let indicator_selection = inputs
        .reference
        .select_indicator_keys(&paths, &limits)
        .map_err(|error| spec_refused("indicators", error))?;

    let (indicators, rule_masks) = bind_indicators(inputs, &place)?;

    let mut compared = Vec::with_capacity(compared_count);
    let mut unverified = Vec::new();
    for (slot, path) in inputs.boundary.outputs.iter().enumerate() {
        let Some(expected) = by_slot[slot] else {
            unverified.push(path.clone());
            continue;
        };
        compared.push(ComparedOutput {
            path: path.clone(),
            value_type: inputs.boundary.output_types[slot],
            expected,
            tolerances: selection.tolerances_for(slot, inputs.spec.tolerances()),
            indicators: {
                // One bit per distinct indicator, so the rules selecting an output join in one
                // OR each, whatever their lists hold.
                let mask = indicator_selection
                    .keys_for(slot)
                    .fold(0_u64, |mask, rule| mask | rule_masks[rule]);
                (0..indicators.len())
                    .filter(|index| mask & (1 << index) != 0)
                    .collect()
            },
        });
    }
    let edits = bind_edits(inputs, &place)?;
    Ok(BoundReference {
        compared,
        unverified,
        indicators,
        edits,
    })
}

/// Step 3: every distinct indicator name of the reference's rules, resolved, at most
/// [`MAX_INDICATORS`] of them (`too-many-indicators`), and for each rule the set of its names as
/// one bit per distinct name. A name is looked up in a map, and an `indicatorSources` point by a
/// binary search over the sorted series, so the work is `n log n` in the names.
fn bind_indicators(
    inputs: &BindInputs<'_>,
    place: &str,
) -> Result<(Vec<BoundIndicator>, Vec<u64>), VerifyError> {
    let reference = inputs.reference;
    // The inputs and the outputs together, in byte order, each tagged with its signal.
    let mut signals: Vec<(&str, bool)> = inputs
        .boundary
        .inputs
        .iter()
        .map(|input| (input.path.as_str(), true))
        .chain(
            inputs
                .boundary
                .outputs
                .iter()
                .map(|path| (path.as_str(), false)),
        )
        .collect();
    signals.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let signal_paths: Vec<&str> = signals.iter().map(|(path, _)| *path).collect();
    let index = PathIndex::new(&signal_paths);
    let prefix = format!("{}.", reference.sequence());

    let mut bound: Vec<BoundIndicator> = Vec::new();
    let mut by_name: BTreeMap<&str, usize> = BTreeMap::new();
    let mut masks = Vec::with_capacity(reference.indicators().len());
    for rule in reference.indicators() {
        let mut mask = 0_u64;
        for name in rule.names() {
            if let Some(at) = by_name.get(name.as_str()) {
                mask |= 1 << at;
                continue;
            }
            let subject = format!("{place}.indicators.{}", rule.key());
            if bound.len() == MAX_INDICATORS {
                return Err(VerifyError::new(
                    Kind::TooManyIndicators,
                    subject,
                    format!(
                        "indicator `{name}` would be the reference's {}th distinct indicator; at most {MAX_INDICATORS}",
                        MAX_INDICATORS + 1
                    ),
                ));
            }
            let signal = if let Some(rest) = name.strip_prefix(&prefix) {
                let at = resolve(
                    &index,
                    rest,
                    Kind::UnknownIndicator,
                    Kind::UnknownIndicator,
                    "input or output of the sequence",
                    &subject,
                )?;
                let (path, input) = signals[at];
                if input {
                    IndicatorSignal::Input(path.to_owned())
                } else {
                    IndicatorSignal::Output(path.to_owned())
                }
            } else {
                // The source reader sorts the indicator series by name.
                let found = inputs
                    .trends
                    .indicators
                    .binary_search_by(|series| series.name.as_str().cmp(name.as_str()));
                let Ok(found) = found else {
                    return Err(VerifyError::new(
                        Kind::UnknownIndicator,
                        subject,
                        format!(
                            "indicator `{name}` does not start with `{prefix}` and no `indicatorSources` point is named so"
                        ),
                    ));
                };
                IndicatorSignal::File(found)
            };
            mask |= 1 << bound.len();
            by_name.insert(name.as_str(), bound.len());
            bound.push(BoundIndicator {
                name: name.clone(),
                signal,
            });
        }
        masks.push(mask);
    }
    Ok((bound, masks))
}

/// Step 4: the parameter edits, resolved and typed, sorted by canonical path.
fn bind_edits(inputs: &BindInputs<'_>, place: &str) -> Result<Vec<BoundEdit>, VerifyError> {
    let Some(written) = inputs.reference.parameters() else {
        return Ok(Vec::new());
    };
    let paths: Vec<&str> = inputs
        .params
        .iter()
        .map(|(path, _)| path.as_str())
        .collect();
    let index = PathIndex::new(&paths);
    let mut rows: Vec<(usize, BoundEdit, &str)> = Vec::with_capacity(written.len());
    for parameter in written {
        let subject = format!("{place}.parameters.{}", parameter.path());
        let row = resolve(
            &index,
            parameter.path(),
            Kind::ParamPathUnknown,
            Kind::ParamPathAmbiguous,
            "parameter",
            &subject,
        )?;
        let (path, value_type) = &inputs.params[row];
        if let Some((_, _, first)) = rows.iter().find(|(seen, _, _)| *seen == row) {
            return Err(VerifyError::new(
                Kind::ParamPathDuplicate,
                subject,
                format!(
                    "`{}` and `{first}` both name parameter {path}",
                    parameter.path()
                ),
            )
            .with_path(path.clone()));
        }
        let value = crate::bind::typed_at(parameter.value(), *value_type, &subject, path)?;
        rows.push((
            row,
            BoundEdit {
                path: path.clone(),
                written: parameter.value(),
                value,
            },
            parameter.path(),
        ));
    }
    rows.sort_by_key(|(row, _, _)| *row);
    Ok(rows.into_iter().map(|(_, edit, _)| edit).collect())
}
