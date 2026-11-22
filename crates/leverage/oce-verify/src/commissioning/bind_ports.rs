//! A reference's mapped points bound to the sequence's ports (commissioning note, "Point
//! mapping" and decision 27; owner rulings of 2026-11-21 on units and quantities).
//!
//! Every point an `inputs` source reads lands on one boundary input, and every point an
//! `expected` source reads on one boundary output, by replay's one name rule
//! ([`oce_replay::PathIndex`]); each, in this order, the first refusal winning:
//!
//! 1. its `cdl.name` resolves: `unknown-input`, `ambiguous-input`, `unknown-output`,
//!    `ambiguous-output`;
//! 2. no other point lands on the same port: `duplicate-input`, `duplicate-output`;
//! 3. its CDL type is the port's (`float` a Real, `int` an Integer, `bool` a Boolean):
//!    `value-type-mismatch`;
//! 4. on a Real port, its `cdl.unit` is the unit the port declares, the same spelling or none on
//!    both sides: `unit-refused`, cause `port-unit-mismatch`. The facade carries no unit for an
//!    Integer or Boolean port, and the trend reader already refuses every conversion but the
//!    identity on one, so there is nothing to compare;
//! 5. its conversion fits the port's declared quantity, matched by its local name: an absolute
//!    temperature row on a `TemperatureDifference` port is `unit-refused`, cause
//!    `absolute-for-difference`, a difference row on a `ThermodynamicTemperature` port
//!    `difference-for-absolute`.
//!
//! Then every boundary input must have a point (`input-not-supplied`, the first in slot order).
//! The inputs are bound in the merged columns' name order, then the outputs in the expected
//! series' name order, so one mapping has one first refusal.

use std::collections::BTreeMap;

use oce_api::{InputDefinition, Topology, ValueType};
use oce_replay::{ColumnKind, PathIndex};

use super::trend_types::{SeriesValues, TrendSources};
use super::units::{UnitConversion, UnitError};
use crate::typing::type_name;
use crate::verify_error::{VerifyError, VerifyErrorKind as Kind};

/// What a port declares beside its type: its unit and its quantity.
#[derive(Clone, Copy, Debug, Default)]
struct Labels<'t> {
    unit: Option<&'t str>,
    quantity: Option<&'t str>,
}

/// Every boundary port's labels, by canonical path.
struct PortLabels<'t> {
    inputs: BTreeMap<&'t str, Labels<'t>>,
    outputs: BTreeMap<&'t str, Labels<'t>>,
}

impl<'t> PortLabels<'t> {
    fn of(topology: &'t Topology) -> Self {
        Self {
            inputs: topology
                .boundary_inputs
                .iter()
                .map(|input| {
                    let labels = Labels {
                        unit: input.unit.as_deref(),
                        quantity: input.quantity.as_deref(),
                    };
                    (input.path.as_str(), labels)
                })
                .collect(),
            outputs: topology
                .boundary_outputs
                .iter()
                .map(|output| {
                    let labels = Labels {
                        unit: output.unit.as_deref(),
                        quantity: output.quantity.as_deref(),
                    };
                    (output.path.as_str(), labels)
                })
                .collect(),
        }
    }
}

/// The points bound to the ports: for each expected series, the output slot it lands on.
pub(crate) struct BoundPorts {
    /// Per entry of [`TrendSources::expected`], in its order, the output slot.
    pub(crate) expected_slots: Vec<usize>,
}

/// The sequence's boundary as bind sees it.
pub(crate) struct Boundary<'a> {
    /// The inputs, in slot order.
    pub(crate) inputs: &'a [InputDefinition],
    /// The outputs' canonical paths, in slot order.
    pub(crate) outputs: &'a [String],
    /// The outputs' types, in slot order.
    pub(crate) output_types: &'a [ValueType],
}

/// The type a CDL point's column carries, as the type of the port it may land on.
fn column_type(kind: &ColumnKind) -> &'static str {
    match kind {
        ColumnKind::Real => "Real",
        ColumnKind::Integer => "Integer",
        ColumnKind::Boolean => "Boolean",
        ColumnKind::Enum(_) => "Enum",
    }
}

fn series_type(values: &SeriesValues) -> &'static str {
    match values {
        SeriesValues::Real(_) => "Real",
        SeriesValues::Integer(_) => "Integer",
        SeriesValues::Boolean(_) => "Boolean",
    }
}

/// Whether a column of `kind` carries a port of `value_type`, exactly.
fn same_type(kind: &str, value_type: ValueType) -> bool {
    matches!(
        (kind, value_type),
        ("Real", ValueType::Real)
            | ("Integer", ValueType::Integer)
            | ("Boolean", ValueType::Boolean)
    )
}

/// Bind every mapped point of `trends` to the boundary, at `place`
/// (`references[i].pointNameMapping`).
///
/// # Errors
/// The first refusal in the order of the module documentation.
pub(crate) fn bind_ports(
    trends: &TrendSources,
    boundary: &Boundary<'_>,
    topology: &Topology,
    place: &str,
) -> Result<BoundPorts, VerifyError> {
    let labels = PortLabels::of(topology);
    // The unit pair of each converted point, by its `cdl.name`: a point with no row has no unit
    // on either side, since a unit on one side only is refused when the sources are read.
    let units: BTreeMap<&str, (&str, &str)> = trends
        .unit_columns
        .iter()
        .map(|column| {
            (
                column.cdl.as_str(),
                (column.from.as_str(), column.to.as_str()),
            )
        })
        .collect();

    let input_paths: Vec<&str> = boundary.inputs.iter().map(|d| d.path.as_str()).collect();
    let input_index = PathIndex::new(&input_paths);
    let mut supplied: Vec<Option<&str>> = vec![None; boundary.inputs.len()];
    for column in trends.inputs.columns() {
        let name = column.name();
        let subject = format!("{place}[{name}]");
        let slot = resolve(
            &input_index,
            name,
            Kind::UnknownInput,
            Kind::AmbiguousInput,
            "boundary input",
            &subject,
        )?;
        let definition = &boundary.inputs[slot];
        if let Some(first) = supplied[slot] {
            return Err(VerifyError::new(
                Kind::DuplicateInput,
                subject,
                format!(
                    "`{name}` and `{first}` both name boundary input {}",
                    definition.path
                ),
            )
            .with_path(definition.path.clone()));
        }
        let kind = column_type(&column.kind());
        check_point(
            &subject,
            &definition.path,
            definition.value_type,
            kind,
            units.get(name).copied(),
            labels
                .inputs
                .get(definition.path.as_str())
                .copied()
                .unwrap_or_default(),
        )?;
        supplied[slot] = Some(name);
    }
    if let Some(slot) = supplied.iter().position(Option::is_none) {
        let path = &boundary.inputs[slot].path;
        return Err(VerifyError::new(
            Kind::InputNotSupplied,
            place,
            format!("no point an `inputs` source reads lands on boundary input {path}"),
        )
        .with_path(path.clone()));
    }

    let output_index = PathIndex::new(boundary.outputs);
    let mut taken: Vec<Option<&str>> = vec![None; boundary.outputs.len()];
    let mut expected_slots = Vec::with_capacity(trends.expected.len());
    for series in &trends.expected {
        let name = series.name.as_str();
        let subject = format!("{place}[{name}]");
        let slot = resolve(
            &output_index,
            name,
            Kind::UnknownOutput,
            Kind::AmbiguousOutput,
            "boundary output",
            &subject,
        )?;
        let path = &boundary.outputs[slot];
        if let Some(first) = taken[slot] {
            return Err(VerifyError::new(
                Kind::DuplicateOutput,
                subject,
                format!("`{name}` and `{first}` both name boundary output {path}"),
            )
            .with_path(path.clone()));
        }
        check_point(
            &subject,
            path,
            boundary.output_types[slot],
            series_type(&series.values),
            units.get(name).copied(),
            labels
                .outputs
                .get(path.as_str())
                .copied()
                .unwrap_or_default(),
        )?;
        taken[slot] = Some(name);
        expected_slots.push(slot);
    }
    Ok(BoundPorts { expected_slots })
}

/// Resolve `name` against an indexed list, refusing at `subject` with the kinds of one side.
pub(crate) fn resolve(
    index: &PathIndex<'_>,
    name: &str,
    unknown: Kind,
    ambiguous: Kind,
    what: &str,
    subject: &str,
) -> Result<usize, VerifyError> {
    index.resolve(name).map_err(|candidates| {
        if candidates.is_empty() {
            VerifyError::new(
                unknown,
                subject,
                format!("`{name}` matches no {what}, by exact path or by suffix"),
            )
        } else {
            VerifyError::new(
                ambiguous,
                subject,
                format!(
                    "`{name}` matches {} {what}s: {}",
                    candidates.len(),
                    candidates.join(", ")
                ),
            )
            .with_candidates(candidates)
        }
    })
}

/// Steps 3 to 5 of the module documentation for one point landing on the port `path`.
fn check_point(
    subject: &str,
    path: &str,
    value_type: ValueType,
    point_type: &str,
    units: Option<(&str, &str)>,
    labels: Labels<'_>,
) -> Result<(), VerifyError> {
    if !same_type(point_type, value_type) {
        return Err(VerifyError::new(
            Kind::ValueTypeMismatch,
            subject,
            format!(
                "the mapping gives the point the CDL type {point_type}, and port {path} is {}",
                type_name(value_type)
            ),
        )
        .with_path(path));
    }
    if value_type != ValueType::Real {
        return Ok(());
    }
    let unit_refused = |error: UnitError| {
        VerifyError::new(Kind::UnitRefused, subject, format!("port {path}: {error}"))
            .with_path(path)
            .with_cause(error.code())
    };
    UnitConversion::check_port_unit(units.map(|(_, to)| to), labels.unit).map_err(unit_refused)?;
    if let Some((from, to)) = units {
        UnitConversion::resolve(Some(from), Some(to))
            .and_then(|conversion| conversion.for_port_quantity(labels.quantity))
            .map_err(unit_refused)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "bind_ports_tests.rs"]
mod tests;
