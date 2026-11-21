//! The point mapping (commissioning note, "Point mapping"; decisions 5 and 27): OBC §12.7
//! Listing 12.2's shape, unchanged.
//!
//! A mapping is a list of pairs, `{ "cdl": {name, unit?, type}, "device": {name, unit?, type} }`:
//! one point of a CDL sequence and the point of the real controller that trends it. It is read
//! from a file of its own ([`PointMapping::from_json_bytes`]) or inline in a spec's
//! `pointNameMapping`, by one reader, with comments removed the same way.
//!
//! A name is kept exactly as written. A `cdl.name` carries no sequence prefix, as Listing 12.2
//! writes none, so one mapping can serve several references; whether a name exists in a sequence
//! is decided when a run binds the mapping to one, and whether a pair of units converts is the
//! unit table's question. What is decided here is the shape, the three types, and that no name
//! appears twice on a side: a point mapped twice would make one signal two.

use super::choices::PointType;
use super::error::{SpecError, SpecErrorKind, shown};
use super::fields::{
    self, Read, Reader, choice, expect, first_repeat, name, put, refuse, required, text,
    unknown_key,
};
use super::limits::SpecLimits;
use crate::canonical::Json;
use crate::error::Location;
use crate::strict::Kind;

/// One side of a mapped point: its name, its unit when it has one, and its type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointSide {
    pub(crate) name: String,
    pub(crate) unit: Option<String>,
    pub(crate) kind: PointType,
}

impl PointSide {
    /// The point's name, as written and never empty. On the CDL side it is a boundary input or
    /// output of the sequence with no sequence prefix, or an indicator's name; on the device
    /// side, the controller's point name, which a trend source's `columns` map gives to a
    /// column.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The unit as written, when the point has one: "some points may not have a unit, but only
    /// have a type" (OBC §12.7). Not checked here: the unit table decides which spellings and
    /// which pairs it knows.
    #[must_use]
    pub fn unit(&self) -> Option<&str> {
        self.unit.as_deref()
    }

    /// The point's data type on this side.
    #[must_use]
    pub fn point_type(&self) -> PointType {
        self.kind
    }
}

/// One mapped point: the CDL side and the device side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointPair {
    pub(crate) cdl: PointSide,
    pub(crate) device: PointSide,
}

impl PointPair {
    /// The point as the CDL sequence has it.
    #[must_use]
    pub fn cdl(&self) -> &PointSide {
        &self.cdl
    }

    /// The point as the real controller has it.
    #[must_use]
    pub fn device(&self) -> &PointSide {
        &self.device
    }
}

/// A point mapping: the pairs, in file order.
///
/// Built only by a load, so its rules hold for a value of this type: every name is non-empty, no
/// CDL name repeats and no device name repeats. Two mappings are equal when they write the same
/// canonical document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointMapping {
    pub(crate) points: Vec<PointPair>,
}

impl PointMapping {
    /// Read one point-mapping file from `bytes` under `limits`. Strict and bounded; it opens
    /// nothing and never panics. `//` and `/* */` comments outside strings are removed first.
    ///
    /// # Errors
    /// A [`SpecError`] whose [`SpecError::code`] is one of [`crate::SPEC_ERROR_CODES`], from the
    /// first failing stage: the bytes, the comments, strict JSON, then the shape and the
    /// duplicate names.
    pub fn from_json_bytes(bytes: &[u8], limits: &SpecLimits) -> Result<Self, SpecError> {
        let limits = limits.effective();
        Self::from_text(fields::utf8(bytes, &limits)?, &limits)
    }

    /// [`Self::from_json_bytes`] for text that is already UTF-8.
    ///
    /// # Errors
    /// As [`Self::from_json_bytes`], without `not-utf8`.
    pub fn from_json_str(text: &str, limits: &SpecLimits) -> Result<Self, SpecError> {
        Self::from_text(text, &limits.effective())
    }

    fn from_text(text: &str, limits: &SpecLimits) -> Result<Self, SpecError> {
        let (stripped, _) = fields::prepared(text, limits)?;
        Ok(Self::document(&stripped)?)
    }

    /// Read `text`, comments already removed, as one mapping and nothing after it. The strict
    /// pass refuses trailing text before this runs; it is refused here too, so the typed reader
    /// is strict standing alone.
    ///
    /// # Errors
    /// As [`Self::read`], and `json-syntax` for anything after the list.
    fn document(text: &str) -> Read<Self> {
        let mut cursor = Reader::reporting(text);
        let mapping = Self::read(&mut cursor)?;
        cursor.finish()?;
        Ok(mapping)
    }

    /// Read the mapping the cursor stands before: a list of pairs.
    ///
    /// # Errors
    /// `wrong-type`, `unknown-key`, `missing-key`, `empty-name`, `unknown-value` or
    /// `duplicate-point`, at the first such place in document order; any strict-JSON refusal
    /// the cursor meets.
    pub(crate) fn read(cursor: &mut Reader<'_>) -> Read<Self> {
        expect(cursor, Kind::Array, "a point mapping", "a list")?;
        let mut list = cursor.open_list()?;
        let mut points = Vec::new();
        // Where each pair's two names are written, for the refusal of a repeat.
        let mut written_at = Vec::new();
        while cursor.next_item(&mut list)? {
            let (pair, places) = pair(cursor)?;
            points.push(pair);
            written_at.push(places);
        }
        let cdl: Vec<&str> = points.iter().map(|pair| pair.cdl.name.as_str()).collect();
        let device: Vec<&str> = points
            .iter()
            .map(|pair| pair.device.name.as_str())
            .collect();
        // The first repeat in file order wins; within one pair the CDL side is judged first.
        let repeat = match (first_repeat(&cdl), first_repeat(&device)) {
            (Some(on_cdl), Some(on_device)) if on_device < on_cdl => Some((on_device, false)),
            (Some(on_cdl), _) => Some((on_cdl, true)),
            (None, Some(on_device)) => Some((on_device, false)),
            (None, None) => None,
        };
        if let Some((index, on_cdl)) = repeat {
            let (side, name, at) = if on_cdl {
                ("cdl", cdl[index], written_at[index].0)
            } else {
                ("device", device[index], written_at[index].1)
            };
            return Err(refuse(
                SpecErrorKind::DuplicatePoint,
                at,
                format!(
                    "an earlier point already has the {side} name {}",
                    shown(name)
                ),
            ));
        }
        Ok(Self { points })
    }

    /// The mapped points, in file order. No CDL name repeats and no device name repeats.
    #[must_use]
    pub fn points(&self) -> &[PointPair] {
        &self.points
    }

    /// The canonical document of this mapping: the list in file order, every object's keys
    /// sorted by their bytes, two-space indentation, line feeds, one final line feed, ASCII
    /// only. Deterministic, and a fixed point: loading the result and writing it again gives the
    /// same bytes. A unit that was not written is not written.
    #[must_use]
    pub fn to_canonical_json(&self) -> String {
        self.to_json().to_document()
    }

    /// The mapping as a canonical value, for a spec that carries it inline.
    pub(crate) fn to_json(&self) -> Json {
        Json::List(
            self.points
                .iter()
                .map(|pair| {
                    Json::object([
                        ("cdl".to_owned(), side_json(&pair.cdl)),
                        ("device".to_owned(), side_json(&pair.device)),
                    ])
                })
                .collect(),
        )
    }
}

fn side_json(side: &PointSide) -> Json {
    let mut entries = vec![
        ("name".to_owned(), Json::str(&side.name)),
        ("type".to_owned(), Json::str(side.kind.as_str())),
    ];
    if let Some(unit) = &side.unit {
        entries.push(("unit".to_owned(), Json::str(unit)));
    }
    Json::object(entries)
}

/// One pair, and where its CDL name and its device name are written.
fn pair(cursor: &mut Reader<'_>) -> Read<(PointPair, (Location, Location))> {
    expect(cursor, Kind::Object, "a mapped point", "an object")?;
    let mut object = cursor.open_object()?;
    let (mut cdl, mut device) = (None, None);
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        match key.as_str() {
            "cdl" => put(&mut cdl, side(cursor, "the cdl point")?, &key, at)?,
            "device" => put(&mut device, side(cursor, "the device point")?, &key, at)?,
            _ => return Err(unknown_key(&key, at)),
        }
    }
    let opened = object.opened();
    let (cdl, cdl_at) = required(cdl, "cdl", opened)?;
    let (device, device_at) = required(device, "device", opened)?;
    Ok((PointPair { cdl, device }, (cdl_at, device_at)))
}

/// One side of a pair, and where its name is written.
fn side(cursor: &mut Reader<'_>, what: &str) -> Read<(PointSide, Location)> {
    expect(cursor, Kind::Object, what, "an object")?;
    let mut object = cursor.open_object()?;
    let (mut written_name, mut unit, mut kind) = (None, None, None);
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        match key.as_str() {
            "name" => {
                let (_, value_at) = cursor.peek()?;
                put(
                    &mut written_name,
                    (name(cursor, "a point name")?, value_at),
                    &key,
                    at,
                )?;
            }
            "unit" => put(&mut unit, text(cursor, "a point unit")?.0, &key, at)?,
            "type" => put(
                &mut kind,
                choice(
                    cursor,
                    "a point type",
                    PointType::from_name,
                    "`float`, `int` or `bool`",
                )?,
                &key,
                at,
            )?,
            _ => return Err(unknown_key(&key, at)),
        }
    }
    let opened = object.opened();
    let (name, name_at) = required(written_name, "name", opened)?;
    let kind = required(kind, "type", opened)?;
    Ok((PointSide { name, unit, kind }, name_at))
}

#[cfg(test)]
#[path = "mapping_tests.rs"]
mod tests;
