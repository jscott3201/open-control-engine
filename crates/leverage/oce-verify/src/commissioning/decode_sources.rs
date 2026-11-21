//! The typed reader of a reference's trend sources and of its comparison window (commissioning
//! note, "A trend source" and "`window`").
//!
//! Every object is closed, as everywhere in the format. What is judged here is what the spec
//! alone can say: a path that stays inside the spec's directory, a format from the closed list,
//! a time column that is a position counted from 1 or a header name, a `columns` map of
//! non-empty names within the key bound, a hold that is not negative, and a window whose start
//! is not after its stop. Whether a column exists in its file, and which way a format addresses
//! its columns, is the source reader's to decide when it has the file's bytes.

use super::choices::TrendFormat;
use super::error::SpecErrorKind;
use super::fields::{
    KeyBound, Read, Reader, choice, expect, keyed, name, path, put, real, refuse, required,
    unknown_key,
};
use super::limits::SpecLimits;
use super::model::{ComparisonWindow, SourceColumn, SourceTimeColumn, TrendSource};
use crate::number::Number;
use crate::strict::Kind;

/// A list of trend sources, in file order; `what` names the list (`the inputs`).
pub(crate) fn sources(
    cursor: &mut Reader<'_>,
    what: &str,
    limits: &SpecLimits,
) -> Read<Vec<TrendSource>> {
    expect(cursor, Kind::Array, what, "a list")?;
    let mut list = cursor.open_list()?;
    let mut all = Vec::new();
    while cursor.next_item(&mut list)? {
        all.push(source(cursor, limits)?);
    }
    Ok(all)
}

fn source(cursor: &mut Reader<'_>, limits: &SpecLimits) -> Read<TrendSource> {
    expect(cursor, Kind::Object, "a trend source", "an object")?;
    let mut object = cursor.open_object()?;
    let (mut file, mut format, mut table) = (None, None, None);
    let (mut time, mut mapped, mut held) = (None, None, None);
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        match key.as_str() {
            "path" => put(&mut file, path(cursor, "the source path")?, &key, at)?,
            "format" => put(
                &mut format,
                choice(
                    cursor,
                    "the source format",
                    TrendFormat::from_name,
                    "`csv` or `combiTimeTable`",
                )?,
                &key,
                at,
            )?,
            "table" => put(&mut table, name(cursor, "the table name")?, &key, at)?,
            "timeColumn" => put(&mut time, time_column(cursor)?, &key, at)?,
            "columns" => put(&mut mapped, columns(cursor, limits)?, &key, at)?,
            "hold" => put(&mut held, hold(cursor)?, &key, at)?,
            _ => return Err(unknown_key(&key, at)),
        }
    }
    let opened = object.opened();
    Ok(TrendSource {
        path: required(file, "path", opened)?,
        format: required(format, "format", opened)?,
        table,
        time_column: required(time, "timeColumn", opened)?,
        columns: required(mapped, "columns", opened)?,
        hold_max_age: held,
    })
}

/// `timeColumn`: a position counted from 1 (an integer token) or a header name (a string).
fn time_column(cursor: &mut Reader<'_>) -> Read<SourceTimeColumn> {
    let (kind, at) = cursor.peek()?;
    match kind {
        Kind::String => name(cursor, "the time column name").map(SourceTimeColumn::Name),
        Kind::Number => match cursor.number()? {
            Number::Integer(position) => u32::try_from(position)
                .ok()
                .filter(|position| *position >= 1)
                .map(SourceTimeColumn::Index)
                .ok_or_else(|| {
                    refuse(
                        SpecErrorKind::IntegerOutOfRange,
                        at,
                        format!(
                            "a column position is counted from 1 and is at most {}",
                            u32::MAX
                        ),
                    )
                }),
            Number::Real(_) => Err(refuse(
                SpecErrorKind::WrongType,
                at,
                "expected the time column, a header name (a string) or a position counted from 1 (an integer), found a real number",
            )),
        },
        other => Err(refuse(
            SpecErrorKind::WrongType,
            at,
            format!(
                "expected the time column, a header name (a string) or a position counted from 1 (an integer), found {}",
                other.described()
            ),
        )),
    }
}

/// `columns`: file column to device point name, sorted by the column's bytes.
fn columns(cursor: &mut Reader<'_>, limits: &SpecLimits) -> Read<Vec<SourceColumn>> {
    let bound = KeyBound {
        kind: SpecErrorKind::TooManyKeys,
        limit: limits.max_keys,
        what: "keys in one columns map",
    };
    let mut all: Vec<SourceColumn> = keyed(cursor, "the columns", bound, |cursor| {
        name(cursor, "a device point name")
    })?
    .into_iter()
    .map(|(column, point)| SourceColumn { column, point })
    .collect();
    // The keys are unique, so an unstable sort orders them one way.
    all.sort_unstable_by(|left, right| left.column.as_bytes().cmp(right.column.as_bytes()));
    Ok(all)
}

/// `hold`: `{ "maxAge": seconds }`, finite and at least zero.
fn hold(cursor: &mut Reader<'_>) -> Read<Number> {
    expect(cursor, Kind::Object, "the hold", "an object")?;
    let mut object = cursor.open_object()?;
    let mut max_age = None;
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        match key.as_str() {
            "maxAge" => {
                let (age, value_at) = real(cursor, "the hold's maxAge in seconds")?;
                if age.seconds() < 0.0 {
                    return Err(refuse(
                        SpecErrorKind::InvalidHold,
                        value_at,
                        "maxAge must not be negative",
                    ));
                }
                put(&mut max_age, age, &key, at)?;
            }
            _ => return Err(unknown_key(&key, at)),
        }
    }
    required(max_age, "maxAge", object.opened())
}

/// `window`: `{ "start": seconds, "stop": seconds }`, both present, `start` not after `stop`.
pub(crate) fn window(cursor: &mut Reader<'_>) -> Read<ComparisonWindow> {
    expect(cursor, Kind::Object, "the window", "an object")?;
    let mut object = cursor.open_object()?;
    let (mut start, mut stop) = (None, None);
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        match key.as_str() {
            "start" => put(
                &mut start,
                real(cursor, "the window's start in seconds")?.0,
                &key,
                at,
            )?,
            "stop" => put(
                &mut stop,
                real(cursor, "the window's stop in seconds")?.0,
                &key,
                at,
            )?,
            _ => return Err(unknown_key(&key, at)),
        }
    }
    let opened = object.opened();
    let start = required(start, "start", opened)?;
    let stop = required(stop, "stop", opened)?;
    if start.seconds() > stop.seconds() {
        return Err(refuse(
            SpecErrorKind::InvalidWindow,
            opened,
            "start must not be after stop",
        ));
    }
    Ok(ComparisonWindow { start, stop })
}

#[cfg(test)]
#[path = "decode_sources_tests.rs"]
mod tests;
