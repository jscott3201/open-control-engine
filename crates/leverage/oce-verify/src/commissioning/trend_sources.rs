//! Reading a reference's trend sources (commissioning note, "A trend source", "Sources, sampling
//! and time" and "Report and receipt"; issue #660): from the bytes a caller hands in to the
//! merged inputs, the expected and indicator series and the receipt's records of the files.
//!
//! In this order, the first refusal winning, so nothing is read past a refusal that needs no
//! row:
//!
//! 1. the reference: in range (`reference-out-of-range`), with at most
//!    [`TrendLimits::max_sources`] sources (`too-many-sources`);
//! 2. the files handed in: in the order given, a path the reference does not name is
//!    `file-unlisted` and a path given twice `duplicate-file`; then the files named, the
//!    mapping's first and then the sources' in receipt order, one with no bytes `file-missing`;
//! 3. the mapping: required when there is a source (`mapping-required`); a file is read with
//!    [`PointMapping::from_json_bytes`] (`mapping-refused`, its code the cause);
//! 4. every source's columns, in receipt order: each addressed as its format takes
//!    (`column-reference-invalid`), each device point in the mapping (`unmapped-point`), its
//!    types and units convertible (`type-pair-refused`, `unit-refused`,
//!    `unit-on-discrete-point`), and no device point read twice in one role
//!    (`point-read-twice`). A `controllerOutput`'s header is read here, its columns being its
//!    header's names;
//! 5. each file, in receipt order ([`super::trend_read`]);
//! 6. the input sources merged ([`super::trend_merge`]).

use std::collections::BTreeSet;

use oce_replay::TimeOrigin;

use super::choices::TrendFormat;
use super::limits::SpecLimits;
use super::mapping::PointMapping;
use super::model::{PointNameMapping, SpecReference, TrendSource};
use super::source_error::{SourceError, SourceErrorKind as Kind};
use super::spec::CommissioningSpec;
use super::trend_limits::TrendLimits;
use super::trend_merge::{Merging, merge};
use super::trend_plan::{
    Address, DevicePoints, PlannedColumn, PlannedSource, TimeAddress, addresses,
    one_column_per_point,
};
use super::trend_read::{ReadSource, Values, controller_output_header, read_source};
use super::trend_types::{
    InputColumn, MappingRecord, SeriesValues, SourceRecord, SourceRole, SpecInputs, TrendInputs,
    TrendSeries, TrendSources, UnitColumn,
};
use crate::fnv::tagged;

/// One source as the reference declares it.
enum Declared<'a> {
    Listed(SourceRole, &'a TrendSource),
    /// The `controllerOutput` read in place of `expected`.
    ControllerOutput(&'a str),
}

impl Declared<'_> {
    fn path(&self) -> &str {
        match self {
            Self::Listed(_, source) => source.path(),
            Self::ControllerOutput(path) => path,
        }
    }
}

/// The sources of `reference` in receipt order: `inputs`, `expected` (or the
/// `controllerOutput` when there is no `expected`), `indicatorSources`.
fn declared(reference: &SpecReference) -> Vec<Declared<'_>> {
    fn listed<'a>(
        all: &mut Vec<Declared<'a>>,
        role: SourceRole,
        sources: Option<&'a [TrendSource]>,
    ) {
        all.extend(
            sources
                .unwrap_or_default()
                .iter()
                .map(|source| Declared::Listed(role, source)),
        );
    }
    let mut all = Vec::new();
    listed(&mut all, SourceRole::Inputs, reference.inputs());
    match (reference.expected(), reference.controller_output()) {
        (None, Some(path)) => all.push(Declared::ControllerOutput(path)),
        (expected, _) => listed(&mut all, SourceRole::Expected, expected),
    }
    listed(
        &mut all,
        SourceRole::Indicators,
        reference.indicator_sources(),
    );
    all
}

/// The bytes handed in under `path`.
fn bytes_of<'i>(inputs: &'i SpecInputs, path: &str) -> &'i [u8] {
    inputs
        .files
        .iter()
        .find(|(given, _)| given == path)
        .map_or(&[], |(_, bytes)| bytes.as_slice())
}

/// Check the files handed in against the paths named: the module's step 2.
fn check_files(named: &[&str], inputs: &SpecInputs) -> Result<(), SourceError> {
    let named_set: BTreeSet<&str> = named.iter().copied().collect();
    let mut given: BTreeSet<&str> = BTreeSet::new();
    for (path, _) in &inputs.files {
        if !named_set.contains(path.as_str()) {
            return Err(SourceError::new(
                Kind::FileUnlisted,
                "the run was handed bytes under a path the reference does not name",
            )
            .in_file(path));
        }
        if !given.insert(path.as_str()) {
            return Err(SourceError::new(
                Kind::DuplicateFile,
                "the run was handed the bytes of this path twice",
            )
            .in_file(path));
        }
    }
    for path in named {
        if !given.contains(path) {
            return Err(SourceError::new(
                Kind::FileMissing,
                "the reference names this file and the run was handed no bytes for it",
            )
            .in_file(path));
        }
    }
    Ok(())
}

/// Plan every source: the module's step 4.
fn plan_sources(
    declared: &[Declared<'_>],
    points: &DevicePoints<'_>,
    inputs: &SpecInputs,
    limits: &TrendLimits,
) -> Result<Vec<PlannedSource>, SourceError> {
    let mut planned = Vec::with_capacity(declared.len());
    for source in declared {
        planned.push(match source {
            Declared::Listed(role, source) => {
                let path = source.path();
                let (time, places) = addresses(
                    path,
                    source.format(),
                    source.table(),
                    source.time_column(),
                    source.columns(),
                )?;
                let mut columns = Vec::with_capacity(places.len());
                for (address, column) in places.into_iter().zip(source.columns()) {
                    columns.push(points.plan(address, column.point(), path)?);
                }
                PlannedSource {
                    role: *role,
                    path: path.to_owned(),
                    format: source.format(),
                    table: source.table().map(str::to_owned),
                    time,
                    columns,
                    hold_max_age_s: source.hold_max_age_s(),
                    seconds_only: false,
                }
            }
            Declared::ControllerOutput(path) => {
                let (time, headers) =
                    controller_output_header(bytes_of(inputs, path), &limits.replay)
                        .map_err(|error| error.in_file(path))?;
                let mut columns: Vec<PlannedColumn> = Vec::with_capacity(headers.len());
                for header in headers {
                    columns.push(points.plan(Address::Name(header.clone()), &header, path)?);
                }
                PlannedSource {
                    role: SourceRole::Expected,
                    path: (*path).to_owned(),
                    format: TrendFormat::Csv,
                    table: None,
                    time: TimeAddress::Name(time),
                    columns,
                    hold_max_age_s: None,
                    seconds_only: true,
                }
            }
        });
    }
    one_column_per_point(&planned)?;
    Ok(planned)
}

/// The series of one read source, one per column, with the missing cells left out.
fn series_of(source: &PlannedSource, read: &ReadSource) -> Vec<TrendSeries> {
    source
        .columns
        .iter()
        .zip(&read.columns)
        .map(|(column, values)| {
            let mut times = Vec::new();
            let values = match values {
                Values::Real(values) => SeriesValues::Real(
                    values
                        .iter()
                        .zip(&read.times)
                        .filter(|(value, _)| !value.is_nan())
                        .map(|(value, t)| {
                            times.push(*t);
                            *value
                        })
                        .collect(),
                ),
                Values::Integer(values) => SeriesValues::Integer(
                    values
                        .iter()
                        .zip(&read.times)
                        .filter_map(|(value, t)| value.inspect(|_| times.push(*t)))
                        .collect(),
                ),
                Values::Boolean(values) => SeriesValues::Boolean(
                    values
                        .iter()
                        .zip(&read.times)
                        .filter_map(|(value, t)| value.inspect(|_| times.push(*t)))
                        .collect(),
                ),
            };
            TrendSeries {
                name: column.cdl.clone(),
                device: column.device.clone(),
                times,
                values,
            }
        })
        .collect()
}

impl TrendSources {
    /// Read the trend sources of `spec`'s reference `reference` from the files in `inputs`,
    /// under `limits`, in the order the module documentation gives. Bounded, deterministic and
    /// never a panic: the same spec and bytes give the same values, whatever the order of
    /// `inputs.files` and of the input sources' declarations. No engine is loaded and no file is
    /// opened.
    ///
    /// # Errors
    /// A [`SourceError`], whose [`SourceError::code`] is one of [`crate::SOURCE_ERROR_CODES`].
    pub fn read(
        spec: &CommissioningSpec,
        reference: usize,
        inputs: &SpecInputs,
        limits: &TrendLimits,
    ) -> Result<Self, SourceError> {
        let limits = limits.effective();
        let Some(reference) = spec.references().get(reference) else {
            return Err(SourceError::new(
                Kind::ReferenceOutOfRange,
                format!(
                    "the spec has {} references, so no reference {reference}",
                    spec.references().len()
                ),
            ));
        };
        let declared = declared(reference);
        if declared.len() as u64 > u64::from(limits.max_sources) {
            return Err(SourceError::new(
                Kind::TooManySources,
                format!(
                    "the reference names {} trend sources; at most {} are read",
                    declared.len(),
                    limits.max_sources
                ),
            ));
        }
        let mapping_path = match reference.point_name_mapping() {
            Some(PointNameMapping::Path(path)) => Some(path.as_str()),
            _ => None,
        };
        let mut named: Vec<&str> = mapping_path.into_iter().collect();
        for source in &declared {
            if !named.contains(&source.path()) {
                named.push(source.path());
            }
        }
        check_files(&named, inputs)?;
        let loaded;
        let (mapping, record) = match reference.point_name_mapping() {
            Some(PointNameMapping::Inline(mapping)) => (Some(mapping), None),
            Some(PointNameMapping::Path(path)) => {
                let bytes = bytes_of(inputs, path);
                loaded = PointMapping::from_json_bytes(bytes, &SpecLimits::default()).map_err(
                    |error| {
                        SourceError::new(Kind::MappingRefused, error.to_string())
                            .in_file(path)
                            .caused_by(error.code())
                    },
                )?;
                let record = MappingRecord {
                    path: path.clone(),
                    digest: tagged("", bytes),
                };
                (Some(&loaded), Some(record))
            }
            // `PointNameMapping` is non-exhaustive to its users, never to this crate.
            None => (None, None),
        };
        let planned = match mapping {
            Some(mapping) => plan_sources(&declared, &DevicePoints::new(mapping), inputs, &limits)?,
            None if declared.is_empty() => Vec::new(),
            None => {
                return Err(SourceError::new(
                    Kind::MappingRequired,
                    "the reference reads trend sources and names no `pointNameMapping`",
                ));
            }
        };
        let origin = spec.time_origin();
        let mut reads = Vec::with_capacity(planned.len());
        for source in &planned {
            reads.push(read_source(
                source,
                bytes_of(inputs, &source.path),
                origin,
                &limits.replay,
            )?);
        }
        assemble(&planned, &reads, inputs, record, origin, &limits)
    }
}

/// The result, from the planned and read sources: the module's step 6 and the records.
fn assemble(
    planned: &[PlannedSource],
    reads: &[ReadSource],
    inputs: &SpecInputs,
    mapping: Option<MappingRecord>,
    origin: Option<&TimeOrigin>,
    limits: &TrendLimits,
) -> Result<TrendSources, SourceError> {
    let sources = planned
        .iter()
        .zip(reads)
        .map(|(source, read)| SourceRecord {
            role: source.role,
            path: source.path.clone(),
            table: source.table.clone(),
            digest: tagged("", bytes_of(inputs, &source.path)),
            rows: read.rows,
        })
        .collect();
    // A device point names one mapped pair, so two rows of one device point are equal.
    let unit_columns: BTreeSet<UnitColumn> = planned
        .iter()
        .flat_map(|source| &source.columns)
        .filter_map(|column| {
            column.units.as_ref().map(|(from, to)| UnitColumn {
                device: column.device.clone(),
                cdl: column.cdl.clone(),
                from: from.clone(),
                to: to.clone(),
            })
        })
        .collect();
    let in_role = |role: SourceRole| {
        planned
            .iter()
            .zip(reads)
            .filter(move |(source, _)| source.role == role)
    };
    let merging: Vec<Merging<'_>> = in_role(SourceRole::Inputs)
        .map(|(_, read)| Merging {
            times: &read.times,
            columns: &read.columns,
        })
        .collect();
    let (times, merged) = merge(&merging, &limits.replay)?;
    let mut columns: Vec<InputColumn> = in_role(SourceRole::Inputs)
        .zip(merged)
        .flat_map(|((source, _), merged)| {
            source
                .columns
                .iter()
                .zip(merged)
                .map(|(column, values)| InputColumn {
                    name: column.cdl.clone(),
                    device: column.device.clone(),
                    values,
                    hold_max_age_s: source.hold_max_age_s,
                })
        })
        .collect();
    columns.sort_by(|a, b| a.name.cmp(&b.name));
    let series = |role| {
        let mut all: Vec<TrendSeries> = in_role(role)
            .flat_map(|(source, read)| series_of(source, read))
            .collect();
        all.sort_by(|a, b| a.name.cmp(&b.name));
        all
    };
    Ok(TrendSources {
        inputs: TrendInputs {
            times,
            columns,
            origin: origin.cloned(),
        },
        expected: series(SourceRole::Expected),
        indicators: series(SourceRole::Indicators),
        sources,
        mapping,
        unit_columns: unit_columns.into_iter().collect(),
    })
}
