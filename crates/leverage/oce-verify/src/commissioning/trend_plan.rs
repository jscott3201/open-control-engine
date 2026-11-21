//! How each trend source of a reference is to be read, decided before any row is (issue #660).
//!
//! For every column a source maps: where the file holds it, which point of the mapping it trends,
//! the cell rule its two types call for ([`CellRule`]) and the conversion its two units call for
//! ([`UnitConversion`]). Every refusal that needs no row is found here, so a wrong unit, type or
//! address is refused before a single cell is converted.
//!
//! **Addressing, per format** (left to this part by GitHub #704; decided with the file in hand):
//! a `combiTimeTable` matrix has no names, so its `timeColumn` is a position and each `columns`
//! key a position counted from 1, written as a canonical decimal (`"3"`, never `"03"` or
//! `"+3"`); a `csv` file has a header row, so its `timeColumn` and its `columns` keys are header
//! names, compared byte for byte (a header named `3` is a name). A `table` is a
//! `combiTimeTable`'s. The time column is never also a point. Anything else is
//! `column-reference-invalid`.
//!
//! **One point, one column per role** (left to this part by the review of GitHub #704): two
//! columns that name one device point, in one source or in two sources of one role, are refused
//! as `point-read-twice`: whichever of the two the reader took, the other's samples would be
//! silently dropped. Two roles may read one point: an indicator column may also be an input.

use std::collections::BTreeMap;

use super::choices::{PointType, TrendFormat};
use super::mapping::{PointMapping, PointPair};
use super::model::{SourceColumn, SourceTimeColumn};
use super::source_error::{SourceError, SourceErrorKind as Kind};
use super::trend_types::SourceRole;
use super::type_rules::CellRule;
use super::units::UnitConversion;

/// Where a file holds a column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Address {
    /// A matrix column, counted from 1.
    Position(u32),
    /// A header name.
    Name(String),
}

impl Address {
    /// The column as a refusal names it: the position in decimal, or the header name.
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Position(position) => position.to_string(),
            Self::Name(name) => name.clone(),
        }
    }
}

/// One column to read, and what each of its cells becomes.
#[derive(Clone, Debug)]
pub(crate) struct PlannedColumn {
    pub(crate) address: Address,
    pub(crate) device: String,
    pub(crate) cdl: String,
    pub(crate) device_type: PointType,
    pub(crate) cdl_type: PointType,
    pub(crate) rule: CellRule,
    pub(crate) unit: UnitConversion,
    /// `(device unit, CDL unit)` when the mapping gives both.
    pub(crate) units: Option<(String, String)>,
}

/// How a source's time column is found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TimeAddress {
    /// A matrix column, counted from 1 (always seconds).
    Position(u32),
    /// A header name (seconds or instants, by its first row).
    Name(String),
}

/// One source to read.
#[derive(Clone, Debug)]
pub(crate) struct PlannedSource {
    pub(crate) role: SourceRole,
    pub(crate) path: String,
    pub(crate) format: TrendFormat,
    pub(crate) table: Option<String>,
    pub(crate) time: TimeAddress,
    pub(crate) columns: Vec<PlannedColumn>,
    pub(crate) hold_max_age_s: Option<f64>,
    /// The `controllerOutput` read in place of `expected`: a time column of seconds only.
    pub(crate) seconds_only: bool,
}

/// The mapping's points by device name.
pub(crate) struct DevicePoints<'a> {
    by_device: BTreeMap<&'a str, &'a PointPair>,
}

impl<'a> DevicePoints<'a> {
    pub(crate) fn new(mapping: &'a PointMapping) -> Self {
        Self {
            by_device: mapping
                .points()
                .iter()
                .map(|pair| (pair.device().name(), pair))
                .collect(),
        }
    }

    /// The column at `address`, trending the device point `device`.
    ///
    /// # Errors
    /// `unmapped-point`, `type-pair-refused`, `unit-refused` or `unit-on-discrete-point`.
    pub(crate) fn plan(
        &self,
        address: Address,
        device: &str,
        path: &str,
    ) -> Result<PlannedColumn, SourceError> {
        let at = |error: SourceError| error.in_file(path).at_column(&address.label());
        let Some(pair) = self.by_device.get(device) else {
            return Err(at(SourceError::new(
                Kind::UnmappedPoint,
                format!("the mapping has no device point named `{device}`"),
            )));
        };
        let (cdl, device_side) = (pair.cdl(), pair.device());
        let rule =
            CellRule::for_types(device_side.point_type(), cdl.point_type()).map_err(|error| {
                at(SourceError::new(Kind::TypePairRefused, error.to_string())
                    .caused_by(error.code()))
            })?;
        let unit = UnitConversion::resolve(device_side.unit(), cdl.unit()).map_err(|error| {
            at(SourceError::new(Kind::UnitRefused, error.to_string()).caused_by(error.code()))
        })?;
        if cdl.point_type() != PointType::Float
            && !matches!(unit, UnitConversion::Unitless | UnitConversion::Identity)
        {
            return Err(at(SourceError::new(
                Kind::UnitOnDiscretePoint,
                format!(
                    "the CDL point `{}` is `{}`, and a unit conversion would change its value",
                    cdl.name(),
                    cdl.point_type()
                ),
            )));
        }
        Ok(PlannedColumn {
            address,
            device: device.to_owned(),
            cdl: cdl.name().to_owned(),
            device_type: device_side.point_type(),
            cdl_type: cdl.point_type(),
            rule,
            unit,
            units: device_side
                .unit()
                .zip(cdl.unit())
                .map(|(from, to)| (from.to_owned(), to.to_owned())),
        })
    }
}

/// A `combiTimeTable` position: a canonical decimal from 1.
fn position(text: &str) -> Option<u32> {
    if text.starts_with('0') || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|&p| p >= 1)
}

fn invalid(path: &str, detail: String) -> SourceError {
    SourceError::new(Kind::ColumnReferenceInvalid, detail).in_file(path)
}

/// The time column and the addresses of `columns`, by the rules of `format`.
///
/// # Errors
/// `column-reference-invalid`.
pub(crate) fn addresses(
    path: &str,
    format: TrendFormat,
    table: Option<&str>,
    time: &SourceTimeColumn,
    columns: &[SourceColumn],
) -> Result<(TimeAddress, Vec<Address>), SourceError> {
    let time = match (format, time) {
        (TrendFormat::CombiTimeTable, SourceTimeColumn::Index(index)) if *index >= 1 => {
            TimeAddress::Position(*index)
        }
        (TrendFormat::Csv, SourceTimeColumn::Name(name)) => TimeAddress::Name(name.clone()),
        (TrendFormat::CombiTimeTable, _) => {
            return Err(invalid(
                path,
                "a combiTimeTable source gives its `timeColumn` as a position from 1".to_owned(),
            ));
        }
        (TrendFormat::Csv, _) => {
            return Err(invalid(
                path,
                "a csv source gives its `timeColumn` as a header name".to_owned(),
            ));
        }
    };
    if format == TrendFormat::Csv && table.is_some() {
        return Err(invalid(
            path,
            "`table` names a combiTimeTable matrix; a csv file has none".to_owned(),
        ));
    }
    let mut addresses = Vec::with_capacity(columns.len());
    for column in columns {
        let address = match format {
            TrendFormat::CombiTimeTable => {
                Address::Position(position(column.column()).ok_or_else(|| {
                    invalid(
                        path,
                        format!(
                            "a combiTimeTable column is a position from 1 written in decimal, \
                             not `{}`",
                            column.column()
                        ),
                    )
                    .at_column(column.column())
                })?)
            }
            TrendFormat::Csv => Address::Name(column.column().to_owned()),
        };
        let is_time = match (&time, &address) {
            (TimeAddress::Position(t), Address::Position(c)) => t == c,
            (TimeAddress::Name(t), Address::Name(c)) => t == c,
            _ => false,
        };
        if is_time {
            return Err(invalid(path, "the time column is not a point".to_owned())
                .at_column(column.column()));
        }
        addresses.push(address);
    }
    Ok((time, addresses))
}

/// Refuse a device point read twice within one role.
///
/// # Errors
/// `point-read-twice`, at the second column met in declaration order.
pub(crate) fn one_column_per_point(sources: &[PlannedSource]) -> Result<(), SourceError> {
    let mut seen: BTreeMap<(SourceRole, &str), ()> = BTreeMap::new();
    for source in sources {
        for column in &source.columns {
            if seen
                .insert((source.role, column.device.as_str()), ())
                .is_some()
            {
                return Err(SourceError::new(
                    Kind::PointReadTwice,
                    format!(
                        "the device point `{}` is read by two columns of the {} sources",
                        column.device,
                        source.role.as_str()
                    ),
                )
                .in_file(&source.path)
                .at_column(&column.address.label()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "trend_plan_tests.rs"]
mod tests;
