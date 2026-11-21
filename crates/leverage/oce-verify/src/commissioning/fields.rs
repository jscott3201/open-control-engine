//! The small readers the closed objects of a commissioning spec and of a point mapping are built
//! from, and the stages every such document passes before its shape is judged.
//!
//! A load runs four stages before the typed reader, and the first refusal wins:
//!
//! 1. **Bytes.** The length against [`SpecLimits::max_bytes`], before a byte is examined; then
//!    UTF-8; then no byte-order mark.
//! 2. **Comments.** `//` and `/* */` outside strings are blanked in place
//!    ([`super::comments`]); a block comment that is never closed is `unterminated-comment`.
//! 3. **Strict JSON.** One pass of the crate's strict cursor over the whole text: syntax, the
//!    three profile bounds, repeated keys, control characters, non-finite numbers and integer
//!    tokens outside `i64`, at the first such place in document order.
//! 4. **The tag,** for a spec ([`super::spec`]).
//!
//! Every refusal of the typed readers here carries the line and column of the value it concerns,
//! in the text as written: comments keep their bytes' places.

use std::borrow::Cow;
use std::collections::BTreeSet;

use super::comments;
use super::error::{SpecError, SpecErrorKind, shown};
use super::limits::SpecLimits;
use crate::canonical::Json;
use crate::error::Location;
use crate::limits::MAX_TIME_INTEGER;
use crate::number::{Literal, Number};
use crate::relative_path::{self, PathDefect};
use crate::strict::{self, Cursor, Kind, Refusal, duplicate_key};

/// The strict cursor, reporting this reader's kinds.
pub(crate) type Reader<'a> = Cursor<'a, SpecErrorKind>;

/// The result of one read of this reader.
pub(crate) type Read<T> = strict::Read<T, SpecErrorKind>;

/// A refusal of `kind` at `at`.
pub(crate) fn refuse(
    kind: SpecErrorKind,
    at: Location,
    detail: impl Into<String>,
) -> Refusal<SpecErrorKind> {
    Refusal::new(kind, at, detail)
}

/// The text of `bytes`, held to the byte bound of `limits` (already
/// [`SpecLimits::effective`]).
///
/// # Errors
/// `input-too-large`, decided from the length alone, then `not-utf8`.
pub(crate) fn utf8<'a>(bytes: &'a [u8], limits: &SpecLimits) -> Result<&'a str, SpecError> {
    too_large(bytes.len(), limits)?;
    std::str::from_utf8(bytes).map_err(|error| {
        SpecError::on(
            SpecErrorKind::NotUtf8,
            "document",
            &format!("byte {} is not UTF-8", error.valid_up_to()),
        )
    })
}

fn too_large(length: usize, limits: &SpecLimits) -> Result<(), SpecError> {
    let length = u64::try_from(length).unwrap_or(u64::MAX);
    if length > limits.max_bytes {
        return Err(SpecError::on(
            SpecErrorKind::InputTooLarge,
            "document",
            &format!(
                "the document is {length} bytes; at most {} are allowed",
                limits.max_bytes
            ),
        ));
    }
    Ok(())
}

/// `text` ready for a typed reader: within the byte bound, without a byte-order mark, its
/// comments blanked, and one strict JSON document. The second value is the root object's
/// `schema` string, when it has one.
///
/// # Errors
/// `input-too-large`, `byte-order-mark`, `unterminated-comment`, or the first strict-JSON
/// refusal in document order.
pub(crate) fn prepared<'a>(
    text: &'a str,
    limits: &SpecLimits,
) -> Result<(Cow<'a, str>, Option<String>), SpecError> {
    too_large(text.len(), limits)?;
    if text.starts_with('\u{feff}') {
        return Err(SpecError::on(
            SpecErrorKind::ByteOrderMark,
            "document",
            "the document starts with a byte-order mark",
        ));
    }
    let stripped = comments::strip(text).map_err(|opened| {
        SpecError::in_text(
            SpecErrorKind::UnterminatedComment,
            opened,
            "a `/*` comment is never closed",
        )
    })?;
    let tag = strict::scan_reporting::<SpecErrorKind>(&stripped)?;
    Ok((stripped, tag))
}

/// Put `value` in `slot`, refusing a key written twice.
pub(crate) fn put<T>(slot: &mut Option<T>, value: T, key: &str, at: Location) -> Read<()> {
    if slot.is_some() {
        return Err(duplicate_key(key, at));
    }
    *slot = Some(value);
    Ok(())
}

/// The value of a required key, or `missing-key` at the object.
pub(crate) fn required<T>(slot: Option<T>, key: &str, object: Location) -> Read<T> {
    slot.ok_or_else(|| {
        refuse(
            SpecErrorKind::MissingKey,
            object,
            format!("missing key `{key}`"),
        )
    })
}

/// `unknown-key` at a key no shape defines.
pub(crate) fn unknown_key(key: &str, at: Location) -> Refusal<SpecErrorKind> {
    refuse(
        SpecErrorKind::UnknownKey,
        at,
        format!("unknown key {}", shown(key)),
    )
}

/// Whether `count` items are more than `limit` allows.
pub(crate) fn exceeds(count: usize, limit: u32) -> bool {
    u64::try_from(count).unwrap_or(u64::MAX) > u64::from(limit)
}

/// Require the next value to be of `kind`, and give its place. `what` names the value and
/// `shape` its JSON type (`a string`) in the `wrong-type` sentence, which is written only when
/// the refusal is: a value of the right type costs no allocation here.
pub(crate) fn expect(
    cursor: &mut Reader<'_>,
    kind: Kind,
    what: &str,
    shape: &str,
) -> Read<Location> {
    let (found, at) = cursor.peek()?;
    if found == kind {
        Ok(at)
    } else {
        Err(refuse(
            SpecErrorKind::WrongType,
            at,
            format!("expected {what}, {shape}, found {}", found.described()),
        ))
    }
}

/// A string value and where it starts; `what` names it in a `wrong-type` refusal.
pub(crate) fn text(cursor: &mut Reader<'_>, what: &str) -> Read<(String, Location)> {
    let at = expect(cursor, Kind::String, what, "a string")?;
    Ok((cursor.string()?, at))
}

/// `empty-name` at `at` when `name` is empty.
pub(crate) fn not_empty(name: &str, at: Location, what: &str) -> Read<()> {
    if name.is_empty() {
        return Err(refuse(
            SpecErrorKind::EmptyName,
            at,
            format!("{what} must not be empty"),
        ));
    }
    Ok(())
}

/// A string value that is a name: not empty.
pub(crate) fn name(cursor: &mut Reader<'_>, what: &str) -> Read<String> {
    let (name, at) = text(cursor, what)?;
    not_empty(&name, at, what)?;
    Ok(name)
}

/// A string value that is a path relative to the document: `/`-separated, not absolute, with no
/// empty, `.` or `..` segment and no backslash ([`crate::relative_path`]).
pub(crate) fn path(cursor: &mut Reader<'_>, what: &str) -> Read<String> {
    let (path, at) = text(cursor, what)?;
    let why = match relative_path::defect(&path) {
        None => return Ok(path),
        Some(PathDefect::Backslash) => "holds a backslash; segments are separated by `/` only",
        Some(PathDefect::Absolute) => "is absolute; it must be relative to the spec file",
        Some(PathDefect::EmptySegment) => "has an empty segment",
        Some(PathDefect::DotSegment) => "has a `.` segment",
        Some(PathDefect::ParentSegment) => {
            "has a `..` segment; it must stay inside the spec file's directory"
        }
    };
    Err(refuse(
        SpecErrorKind::PathInvalid,
        at,
        format!("{what} {} {why}", shown(&path)),
    ))
}

/// A number the format reads as a real, and where it starts. An integer token is held to
/// [`MAX_TIME_INTEGER`], the range in which every integer is a binary64, so it never names a
/// value other than the one it spells.
pub(crate) fn real(cursor: &mut Reader<'_>, what: &str) -> Read<(Number, Location)> {
    let at = expect(cursor, Kind::Number, what, "a number")?;
    let number = cursor.number()?;
    if let Number::Integer(value) = number
        && value.unsigned_abs() > MAX_TIME_INTEGER.unsigned_abs()
    {
        return Err(refuse(
            SpecErrorKind::IntegerOutOfRange,
            at,
            format!(
                "an integer beyond {MAX_TIME_INTEGER} is not exact in binary64; write it as a real"
            ),
        ));
    }
    Ok((number, at))
}

/// A parameter value: a number or a boolean, with the kind it was written as.
pub(crate) fn literal(cursor: &mut Reader<'_>) -> Read<Literal> {
    let (kind, at) = cursor.peek()?;
    match kind {
        Kind::Number => cursor.number().map(Literal::from),
        Kind::Boolean => cursor.boolean().map(Literal::Boolean),
        other => Err(refuse(
            SpecErrorKind::WrongType,
            at,
            format!(
                "expected a value (a number or a boolean), found {}",
                other.described()
            ),
        )),
    }
}

/// A string that must be one of a closed list: `from_name` reads it, and `spellings` lists the
/// accepted strings for the refusal.
pub(crate) fn choice<T>(
    cursor: &mut Reader<'_>,
    what: &str,
    from_name: fn(&str) -> Option<T>,
    spellings: &str,
) -> Read<T> {
    let (written, at) = text(cursor, what)?;
    from_name(&written).ok_or_else(|| {
        refuse(
            SpecErrorKind::UnknownValue,
            at,
            format!("{what} is {}; expected {spellings}", shown(&written)),
        )
    })
}

/// Any one strict JSON value, kept whole for the canonical form and never interpreted: the
/// value of a field the verifier ignores (commissioning note, decision 18). Object keys are
/// sorted, a number keeps the kind it was written as, and a repeated key is refused.
pub(crate) fn opaque(cursor: &mut Reader<'_>) -> Read<Json> {
    match cursor.peek()?.0 {
        Kind::Object => {
            let mut object = cursor.open_object()?;
            let mut seen = BTreeSet::new();
            let mut entries = Vec::new();
            while let Some((key, at)) = cursor.next_key(&mut object)? {
                if !seen.insert(key.clone()) {
                    return Err(duplicate_key(&key, at));
                }
                let value = opaque(cursor)?;
                entries.push((key, value));
            }
            Ok(Json::object(entries))
        }
        Kind::Array => {
            let mut list = cursor.open_list()?;
            let mut items = Vec::new();
            while cursor.next_item(&mut list)? {
                items.push(opaque(cursor)?);
            }
            Ok(Json::List(items))
        }
        Kind::String => cursor.string().map(Json::Str),
        Kind::Number => cursor.number().map(|number| Json::Raw(number.canonical())),
        Kind::Boolean => cursor
            .boolean()
            .map(|boolean| Json::Raw(boolean.to_string())),
        Kind::Null => cursor.skip_value().map(|()| Json::Raw("null".to_owned())),
    }
}

/// The bound on one author-keyed map: the refusal past it, the most entries, and what the
/// entries are called in the sentence.
#[derive(Clone, Copy)]
pub(crate) struct KeyBound {
    /// The refusal of the entry past the bound.
    pub(crate) kind: SpecErrorKind,
    /// The most entries the map may have.
    pub(crate) limit: u32,
    /// The entries, for the sentence: `keys in one outputs map`.
    pub(crate) what: &'static str,
}

/// A map whose keys the author chooses: every entry in file order, each value read by `value`.
/// A key is a name, so an empty one is refused; the entry past `bound` is refused before its
/// value is read; and a key written twice is refused at its second place.
///
/// # Errors
/// `wrong-type` when no object is here, the bound's own kind, `empty-name`, `duplicate-key`, or
/// whatever `value` refuses.
pub(crate) fn keyed<T>(
    cursor: &mut Reader<'_>,
    what: &str,
    bound: KeyBound,
    mut value: impl FnMut(&mut Reader<'_>) -> Read<T>,
) -> Read<Vec<(String, T)>> {
    expect(cursor, Kind::Object, what, "an object")?;
    let mut object = cursor.open_object()?;
    let mut entries: Vec<(String, T)> = Vec::new();
    let mut written_at = Vec::new();
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        if exceeds(entries.len() + 1, bound.limit) {
            return Err(refuse(
                bound.kind,
                at,
                format!("more than {} {}", bound.limit, bound.what),
            ));
        }
        not_empty(&key, at, "a key")?;
        written_at.push(at);
        let read = value(cursor)?;
        entries.push((key, read));
    }
    let names: Vec<&str> = entries.iter().map(|(key, _)| key.as_str()).collect();
    if let Some(index) = first_repeat(&names) {
        return Err(duplicate_key(names[index], written_at[index]));
    }
    Ok(entries)
}

/// The index of the first name, in the order given, that an earlier name already has; `None`
/// when every name is distinct. Sorts an index of the names, so a long list is not a quadratic
/// scan and no name is copied.
pub(crate) fn first_repeat(names: &[&str]) -> Option<usize> {
    let mut order: Vec<usize> = (0..names.len()).collect();
    order.sort_unstable_by(|left, right| names[*left].cmp(names[*right]).then(left.cmp(right)));
    order
        .windows(2)
        .filter(|pair| names[pair[0]] == names[pair[1]])
        .map(|pair| pair[1])
        .min()
}

#[cfg(test)]
#[path = "fields_tests.rs"]
mod tests;
