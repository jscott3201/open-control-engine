//! The vector set as written: the closed shapes the reader fills and the published JSON Schema
//! is derived from (vectors note, "The vector format"; decision 17).
//!
//! [`crate::decode`] builds these from the strict cursor, one struct literal per object, so a
//! field added here does not compile until the reader fills it; `schemars` derives
//! `schema/vectors.schema.json` from the same definitions, doc comments included. Nothing is
//! judged here: a value keeps the kind it was written as, a map keeps file order, and the clock,
//! step and window rules are applied when a [`crate::VectorSet`] is made from these.
//!
//! Three small wrappers carry what a derive cannot say on its own: [`Optional`] is a key that
//! may be absent (never `null`), [`Entries`] is an open map from names to values, and
//! [`Seconds`] and [`ValueDto`] are a number and a value without a `format` keyword.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};

use crate::number::{Literal, Number};

/// A vector set: scenarios (inputs over one clock, expectations over windows of it) to check
/// against one loaded control sequence. Strict JSON: every object is closed, no key repeats, and
/// a key this schema does not define is refused, never ignored.
#[derive(Debug, JsonSchema)]
#[schemars(title = "VectorSet", deny_unknown_fields)]
pub(crate) struct VectorSetDto {
    /// The format tag: exactly "oce/vectors/v1".
    #[schemars(schema_with = "schema_tag")]
    pub(crate) schema: String,
    /// The set's clock. Every scenario runs on it.
    pub(crate) clock: ClockDto,
    /// Parameter edits applied to every scenario before its first frame: parameter path to
    /// value. At most 256 entries. A scenario's own params override these.
    #[schemars(default)]
    pub(crate) params: Optional<Entries<ValueDto>>,
    /// The scenarios, in order. At most 4,096; each runs on a freshly loaded sequence.
    pub(crate) scenarios: Vec<ScenarioDto>,
}

/// The set's clock: ticks at k * step_s for k = 0..=N, where N * step_s equals horizon_s
/// exactly in binary64. At most 16,777,216 ticks.
#[derive(Debug, JsonSchema)]
#[schemars(rename = "Clock", deny_unknown_fields)]
pub(crate) struct ClockDto {
    /// Seconds between ticks: positive.
    pub(crate) step_s: Seconds,
    /// The last tick, in seconds: not negative, and a whole number of steps.
    pub(crate) horizon_s: Seconds,
}

/// One scenario: what every input holds over the clock, and what is expected of the outputs.
#[derive(Debug, JsonSchema)]
#[schemars(rename = "Scenario", deny_unknown_fields)]
pub(crate) struct ScenarioDto {
    /// The scenario name: not empty, unique in the set.
    pub(crate) name: String,
    /// What the scenario shows, for people.
    #[schemars(default)]
    pub(crate) description: Optional<String>,
    /// Parameter edits for this scenario only: parameter path to value. At most 256 entries.
    #[schemars(default)]
    pub(crate) params: Optional<Entries<ValueDto>>,
    /// Input name to what it holds: a constant, or a step list. Every input of the sequence has
    /// exactly one entry.
    pub(crate) inputs: Entries<InputDto>,
    /// The expectations, in order. At most 4,096.
    pub(crate) expect: Vec<ExpectationDto>,
}

/// One step of a step list: from time t on, the input holds value (zero-order hold).
#[derive(Debug, JsonSchema)]
#[schemars(rename = "Step", deny_unknown_fields)]
pub(crate) struct StepDto {
    /// The time in seconds from which the value holds. The first step is at 0, each later one is
    /// after the one before it, and none is after horizon_s. It need not be a tick: it takes
    /// effect at the first tick at or after it.
    pub(crate) t: Seconds,
    /// The value held from t.
    pub(crate) value: ValueDto,
}

/// A window expectation: the output equals the value at every tick from from_s to to_s, both
/// ends included.
#[derive(Debug, JsonSchema)]
#[schemars(rename = "Window", deny_unknown_fields)]
pub(crate) struct WindowDto {
    /// The output name.
    pub(crate) output: String,
    /// The first second of the window: not negative, not after to_s.
    pub(crate) from_s: Seconds,
    /// The last second of the window: not after horizon_s. The window must hold at least one
    /// tick.
    pub(crate) to_s: Seconds,
    /// The value the output must equal, exactly; a Real is compared by its bits.
    pub(crate) equals: ValueDto,
}

/// An instant expectation: the frame at at_s emitted the diagnostic.
#[derive(Debug, JsonSchema)]
#[schemars(rename = "Instant", deny_unknown_fields)]
pub(crate) struct InstantDto {
    /// The run-time warning expected at at_s.
    pub(crate) diagnostic: DiagnosticDto,
    /// The time of the frame, in seconds: a tick of the clock.
    pub(crate) at_s: Seconds,
}

/// A run-time warning to expect.
#[derive(Debug, JsonSchema)]
#[schemars(rename = "Diagnostic", deny_unknown_fields)]
pub(crate) struct DiagnosticDto {
    /// The class path of the block that warns, for example "CDL.Utilities.Assert". A class, not
    /// an instance.
    pub(crate) block: String,
    /// The warning's message, matched byte for byte when present.
    #[schemars(default)]
    pub(crate) message: Optional<String>,
}

/// The schema of the `schema` key: a string with one allowed value.
fn schema_tag(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": "string",
        "enum": [crate::VECTORS_SCHEMA],
    })
}

/// A key that may be absent. Present, it holds its value; `null` is never a value.
#[derive(Debug)]
pub(crate) struct Optional<T>(pub(crate) Option<T>);

impl<T> Default for Optional<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T: JsonSchema> JsonSchema for Optional<T> {
    fn schema_name() -> Cow<'static, str> {
        T::schema_name()
    }

    fn schema_id() -> Cow<'static, str> {
        T::schema_id()
    }

    fn inline_schema() -> bool {
        T::inline_schema()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        T::json_schema(generator)
    }
}

/// An object used as a map: any key, every value of one shape, in file order.
#[derive(Debug)]
pub(crate) struct Entries<T>(pub(crate) Vec<(String, T)>);

impl<T: JsonSchema> JsonSchema for Entries<T> {
    fn schema_name() -> Cow<'static, str> {
        format!("Map_of_{}", T::schema_name()).into()
    }

    fn schema_id() -> Cow<'static, str> {
        format!("oce_verify::Entries<{}>", T::schema_id()).into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "object",
            "additionalProperties": generator.subschema_for::<T>(),
        })
    }
}

/// A time in seconds: a JSON number.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Seconds(pub(crate) Number);

impl JsonSchema for Seconds {
    fn schema_name() -> Cow<'static, str> {
        "Seconds".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "number" })
    }
}

/// A value: a number or a boolean.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ValueDto(pub(crate) Literal);

impl JsonSchema for ValueDto {
    fn schema_name() -> Cow<'static, str> {
        "Value".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A value: a number or a boolean. A number written without a fraction or exponent is an integer token, and only an integer token is taken by an Integer port or parameter; a Real takes any number.",
            "anyOf": [
                { "type": "number" },
                { "type": "boolean" },
            ]
        })
    }
}

/// What an input holds over the clock.
#[derive(Debug)]
pub(crate) enum InputDto {
    /// One value for the whole run.
    Constant(ValueDto),
    /// A step list.
    Steps(Vec<StepDto>),
}

impl JsonSchema for InputDto {
    fn schema_name() -> Cow<'static, str> {
        "Input".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "What an input holds over the clock: one value for the whole run, or a step list of at least one step (at most 65,536) starting at t = 0.",
            "anyOf": [
                { "type": "number" },
                { "type": "boolean" },
                { "type": "array", "items": generator.subschema_for::<StepDto>() },
            ]
        })
    }
}

/// One expectation, in exactly one of its two forms.
#[derive(Debug)]
pub(crate) enum ExpectationDto {
    /// `output`, `from_s`, `to_s`, `equals`.
    Window(WindowDto),
    /// `diagnostic`, `at_s`.
    Instant(InstantDto),
}

impl JsonSchema for ExpectationDto {
    fn schema_name() -> Cow<'static, str> {
        "Expectation".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "One expectation, in exactly one of two forms: a window (output, from_s, to_s, equals) or an instant (diagnostic, at_s).",
            "anyOf": [
                generator.subschema_for::<WindowDto>(),
                generator.subschema_for::<InstantDto>(),
            ]
        })
    }
}
