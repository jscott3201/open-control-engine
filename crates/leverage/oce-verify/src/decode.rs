//! The typed reader: one pass of the strict cursor into the closed shapes of [`crate::dto`].
//!
//! Every object is closed. A key the shape does not define is `unknown-key` at the key, a
//! required key that is absent is `missing-key` at the object, and a value of another JSON type
//! is `wrong-type` at the value; nothing is skipped, defaulted or read as `null`. The count
//! bounds of [`VerifyLimits`] are applied as the items arrive, so a set past a bound is refused
//! before the item past it is read. A time's integer token is held to
//! [`MAX_TIME_INTEGER`]. The reader stops at the first refusal in document order.
//!
//! [`crate::strict::scan`] runs before this pass and refuses every lexical defect and repeated
//! key, so the cursor's own refusals do not occur here; they are still returned, never unwrapped,
//! and a repeated key is still refused, so this reader is strict standing alone.

use crate::dto::{
    ClockDto, DiagnosticDto, Entries, ExpectationDto, InputDto, InstantDto, Optional, ScenarioDto,
    Seconds, StepDto, ValueDto, VectorSetDto, WindowDto,
};
use crate::error::{Location, VectorErrorKind};
use crate::limits::{MAX_TIME_INTEGER, VerifyLimits};
use crate::number::{Literal, Number};
use crate::strict::{Cursor, Kind, Read, Refusal, duplicate_key};

/// Read `text` as one vector set under `limits` (already [`VerifyLimits::effective`]).
///
/// # Errors
/// `unknown-key`, `missing-key`, `wrong-type`, `ambiguous-expectation`, a count bound or
/// `integer-out-of-range` for a time, at the first such place in document order; any strict-JSON
/// refusal the cursor meets.
pub(crate) fn decode(text: &str, limits: &VerifyLimits) -> Read<VectorSetDto> {
    let mut cursor = Cursor::new(text);
    let set = vector_set(&mut cursor, limits)?;
    cursor.finish()?;
    Ok(set)
}

/// Put `value` in `slot`, refusing a key written twice.
fn put<T>(slot: &mut Option<T>, value: T, key: &str, at: Location) -> Read<()> {
    if slot.is_some() {
        return Err(duplicate_key(key, at));
    }
    *slot = Some(value);
    Ok(())
}

/// The value of a required key, or `missing-key` at the object.
fn required<T>(slot: Option<T>, key: &str, object: Location) -> Read<T> {
    slot.ok_or_else(|| {
        Refusal::new(
            VectorErrorKind::MissingKey,
            object,
            format!("missing key `{key}`"),
        )
    })
}

fn unknown_key(key: &str, at: Location) -> Refusal {
    Refusal::new(
        VectorErrorKind::UnknownKey,
        at,
        format!("unknown key `{key}`"),
    )
}

fn too_many(kind: VectorErrorKind, at: Location, what: &str, limit: u32) -> Refusal {
    Refusal::new(kind, at, format!("more than {limit} {what}"))
}

/// Whether `count` items are more than `limit` allows.
fn exceeds(count: usize, limit: u32) -> bool {
    u64::try_from(count).unwrap_or(u64::MAX) > u64::from(limit)
}

fn vector_set(cursor: &mut Cursor<'_>, limits: &VerifyLimits) -> Read<VectorSetDto> {
    cursor.expect(Kind::Object, "a vector set object")?;
    let mut object = cursor.open_object()?;
    let (mut schema, mut clock_value, mut params_value, mut scenarios_value) =
        (None, None, None, None);
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        match key.as_str() {
            "schema" => {
                cursor.expect(Kind::String, "the schema tag, a string")?;
                put(&mut schema, cursor.string()?, &key, at)?;
            }
            "clock" => put(&mut clock_value, clock(cursor)?, &key, at)?,
            "params" => put(&mut params_value, params(cursor, limits)?, &key, at)?,
            "scenarios" => put(&mut scenarios_value, scenarios(cursor, limits)?, &key, at)?,
            _ => return Err(unknown_key(&key, at)),
        }
    }
    let opened = object.opened();
    Ok(VectorSetDto {
        schema: required(schema, "schema", opened)?,
        clock: required(clock_value, "clock", opened)?,
        params: Optional(params_value),
        scenarios: required(scenarios_value, "scenarios", opened)?,
    })
}

fn clock(cursor: &mut Cursor<'_>) -> Read<ClockDto> {
    cursor.expect(Kind::Object, "the clock, an object")?;
    let mut object = cursor.open_object()?;
    let (mut step_s, mut horizon_s) = (None, None);
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        match key.as_str() {
            "step_s" => put(&mut step_s, seconds(cursor)?, &key, at)?,
            "horizon_s" => put(&mut horizon_s, seconds(cursor)?, &key, at)?,
            _ => return Err(unknown_key(&key, at)),
        }
    }
    let opened = object.opened();
    Ok(ClockDto {
        step_s: required(step_s, "step_s", opened)?,
        horizon_s: required(horizon_s, "horizon_s", opened)?,
    })
}

/// A time: a number whose integer token, if it is one, is within [`MAX_TIME_INTEGER`].
fn seconds(cursor: &mut Cursor<'_>) -> Read<Seconds> {
    let at = cursor.expect(Kind::Number, "a time in seconds, a number")?;
    let number = cursor.number()?;
    if let Number::Integer(value) = number
        && value.unsigned_abs() > MAX_TIME_INTEGER.unsigned_abs()
    {
        return Err(Refusal::new(
            VectorErrorKind::IntegerOutOfRange,
            at,
            format!(
                "an integer time beyond {MAX_TIME_INTEGER} is not exact in binary64; write it as a real"
            ),
        ));
    }
    Ok(Seconds(number))
}

/// A value: a number or a boolean.
fn value(cursor: &mut Cursor<'_>) -> Read<ValueDto> {
    let (kind, at) = cursor.peek()?;
    match kind {
        Kind::Number => cursor
            .number()
            .map(|number| ValueDto(Literal::from(number))),
        Kind::Boolean => cursor
            .boolean()
            .map(|boolean| ValueDto(Literal::Boolean(boolean))),
        other => Err(Refusal::new(
            VectorErrorKind::WrongType,
            at,
            format!(
                "expected a value (a number or a boolean), found {}",
                other.described()
            ),
        )),
    }
}

fn params(cursor: &mut Cursor<'_>, limits: &VerifyLimits) -> Read<Entries<ValueDto>> {
    cursor.expect(Kind::Object, "a params map, an object")?;
    let mut object = cursor.open_object()?;
    let mut entries: Vec<(String, ValueDto)> = Vec::new();
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        if entries.iter().any(|(seen, _)| *seen == key) {
            return Err(duplicate_key(&key, at));
        }
        if exceeds(entries.len() + 1, limits.max_parameters) {
            return Err(too_many(
                VectorErrorKind::TooManyParameters,
                at,
                "parameters in one map",
                limits.max_parameters,
            ));
        }
        entries.push((key, value(cursor)?));
    }
    Ok(Entries(entries))
}

fn scenarios(cursor: &mut Cursor<'_>, limits: &VerifyLimits) -> Read<Vec<ScenarioDto>> {
    cursor.expect(Kind::Array, "the scenarios, a list")?;
    let mut list = cursor.open_list()?;
    let mut all = Vec::new();
    while cursor.next_item(&mut list)? {
        let (_, at) = cursor.peek()?;
        if exceeds(all.len() + 1, limits.max_scenarios) {
            return Err(too_many(
                VectorErrorKind::TooManyScenarios,
                at,
                "scenarios",
                limits.max_scenarios,
            ));
        }
        all.push(scenario(cursor, limits)?);
    }
    Ok(all)
}

fn scenario(cursor: &mut Cursor<'_>, limits: &VerifyLimits) -> Read<ScenarioDto> {
    cursor.expect(Kind::Object, "a scenario, an object")?;
    let mut object = cursor.open_object()?;
    let (mut name, mut description, mut params_value, mut inputs_value, mut expect_value) =
        (None, None, None, None, None);
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        match key.as_str() {
            "name" => put(&mut name, text(cursor, "the scenario name")?, &key, at)?,
            "description" => put(&mut description, text(cursor, "the description")?, &key, at)?,
            "params" => put(&mut params_value, params(cursor, limits)?, &key, at)?,
            "inputs" => put(&mut inputs_value, inputs(cursor)?, &key, at)?,
            "expect" => put(&mut expect_value, expectations(cursor, limits)?, &key, at)?,
            _ => return Err(unknown_key(&key, at)),
        }
    }
    let opened = object.opened();
    Ok(ScenarioDto {
        name: required(name, "name", opened)?,
        description: Optional(description),
        params: Optional(params_value),
        inputs: required(inputs_value, "inputs", opened)?,
        expect: required(expect_value, "expect", opened)?,
    })
}

/// A string value; `what` names it in a `wrong-type` refusal.
fn text(cursor: &mut Cursor<'_>, what: &str) -> Read<String> {
    cursor.expect(Kind::String, &format!("{what}, a string"))?;
    cursor.string()
}

fn inputs(cursor: &mut Cursor<'_>) -> Read<Entries<InputDto>> {
    cursor.expect(Kind::Object, "the inputs, an object")?;
    let mut object = cursor.open_object()?;
    let mut entries: Vec<(String, InputDto)> = Vec::new();
    let mut written_at = Vec::new();
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        written_at.push(at);
        entries.push((key, input(cursor)?));
    }
    // A repeat is looked for once the map is read, through a sorted index of the names: no name
    // is copied, and a map of many inputs is not a quadratic scan.
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by(|left, right| {
        entries[*left]
            .0
            .cmp(&entries[*right].0)
            .then(left.cmp(right))
    });
    let repeat = order
        .windows(2)
        .filter(|pair| entries[pair[0]].0 == entries[pair[1]].0)
        .map(|pair| pair[1])
        .min();
    if let Some(index) = repeat {
        return Err(duplicate_key(&entries[index].0, written_at[index]));
    }
    Ok(Entries(entries))
}

/// An input: a constant, or a step list. Any other JSON type, an object included, is
/// `wrong-type` (decision 5: no series or file source in this format).
fn input(cursor: &mut Cursor<'_>) -> Read<InputDto> {
    let (kind, at) = cursor.peek()?;
    match kind {
        Kind::Number | Kind::Boolean => value(cursor).map(InputDto::Constant),
        Kind::Array => {
            let mut list = cursor.open_list()?;
            let mut steps = Vec::new();
            while cursor.next_item(&mut list)? {
                steps.push(step(cursor)?);
            }
            Ok(InputDto::Steps(steps))
        }
        other => Err(Refusal::new(
            VectorErrorKind::WrongType,
            at,
            format!(
                "expected an input (a number, a boolean or a step list), found {}",
                other.described()
            ),
        )),
    }
}

fn step(cursor: &mut Cursor<'_>) -> Read<StepDto> {
    cursor.expect(Kind::Object, "a step, an object")?;
    let mut object = cursor.open_object()?;
    let (mut t, mut held) = (None, None);
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        match key.as_str() {
            "t" => put(&mut t, seconds(cursor)?, &key, at)?,
            "value" => put(&mut held, value(cursor)?, &key, at)?,
            _ => return Err(unknown_key(&key, at)),
        }
    }
    let opened = object.opened();
    Ok(StepDto {
        t: required(t, "t", opened)?,
        value: required(held, "value", opened)?,
    })
}

fn expectations(cursor: &mut Cursor<'_>, limits: &VerifyLimits) -> Read<Vec<ExpectationDto>> {
    cursor.expect(Kind::Array, "the expectations, a list")?;
    let mut list = cursor.open_list()?;
    let mut all = Vec::new();
    while cursor.next_item(&mut list)? {
        let (_, at) = cursor.peek()?;
        if exceeds(all.len() + 1, limits.max_expectations) {
            return Err(too_many(
                VectorErrorKind::TooManyExpectations,
                at,
                "expectations in one scenario",
                limits.max_expectations,
            ));
        }
        all.push(expectation(cursor)?);
    }
    Ok(all)
}

/// One expectation. The six keys of both forms are read into one closed object; then exactly one
/// form must be present, and whole.
fn expectation(cursor: &mut Cursor<'_>) -> Read<ExpectationDto> {
    cursor.expect(Kind::Object, "an expectation, an object")?;
    let mut object = cursor.open_object()?;
    let (mut output, mut from_s, mut to_s, mut equals) = (None, None, None, None);
    let (mut diagnostic_value, mut at_s) = (None, None);
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        match key.as_str() {
            "output" => put(&mut output, text(cursor, "the output name")?, &key, at)?,
            "from_s" => put(&mut from_s, seconds(cursor)?, &key, at)?,
            "to_s" => put(&mut to_s, seconds(cursor)?, &key, at)?,
            "equals" => put(&mut equals, value(cursor)?, &key, at)?,
            "diagnostic" => put(&mut diagnostic_value, diagnostic(cursor)?, &key, at)?,
            "at_s" => put(&mut at_s, seconds(cursor)?, &key, at)?,
            _ => return Err(unknown_key(&key, at)),
        }
    }
    let opened = object.opened();
    let window = output.is_some() || from_s.is_some() || to_s.is_some() || equals.is_some();
    let instant = diagnostic_value.is_some() || at_s.is_some();
    match (window, instant) {
        (true, false) => Ok(ExpectationDto::Window(WindowDto {
            output: required(output, "output", opened)?,
            from_s: required(from_s, "from_s", opened)?,
            to_s: required(to_s, "to_s", opened)?,
            equals: required(equals, "equals", opened)?,
        })),
        (false, true) => Ok(ExpectationDto::Instant(InstantDto {
            diagnostic: required(diagnostic_value, "diagnostic", opened)?,
            at_s: required(at_s, "at_s", opened)?,
        })),
        (true, true) => Err(Refusal::new(
            VectorErrorKind::AmbiguousExpectation,
            opened,
            "an expectation is a window (output, from_s, to_s, equals) or an instant (diagnostic, at_s), not both",
        )),
        (false, false) => Err(Refusal::new(
            VectorErrorKind::AmbiguousExpectation,
            opened,
            "an expectation is a window (output, from_s, to_s, equals) or an instant (diagnostic, at_s); this one is neither",
        )),
    }
}

fn diagnostic(cursor: &mut Cursor<'_>) -> Read<DiagnosticDto> {
    cursor.expect(Kind::Object, "a diagnostic, an object")?;
    let mut object = cursor.open_object()?;
    let (mut block, mut message) = (None, None);
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        match key.as_str() {
            "block" => put(&mut block, text(cursor, "the block class path")?, &key, at)?,
            "message" => put(&mut message, text(cursor, "the message")?, &key, at)?,
            _ => return Err(unknown_key(&key, at)),
        }
    }
    Ok(DiagnosticDto {
        block: required(block, "block", object.opened())?,
        message: Optional(message),
    })
}

#[cfg(test)]
#[path = "decode_tests.rs"]
mod tests;
