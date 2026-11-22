//! The vector set as written: the closed shapes the reader fills and the published JSON Schema
//! is derived from (vectors note, "The vector format"; decision 17).
//!
//! [`crate::decode`] builds these from the strict cursor, one struct literal per object, so a
//! field added here does not compile until the reader fills it; `schemars` derives
//! `schema/vectors.schema.json` from the same definitions, doc comments included. Nothing is
//! judged here: a value keeps the kind it was written as, a map keeps file order, and the clock,
//! step and window rules are applied when a [`crate::VectorSet`] is made from these.
//!
//! Small wrappers carry what a derive cannot say on its own: [`Optional`] is a key that may be
//! absent (never `null`), [`Entries`] is an open map from names to values, [`Seconds`],
//! [`Amount`] and [`ValueDto`] are a number and a value without a `format` keyword, and
//! [`EnvelopeName`] is a string with two allowed values.

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
    /// Mutants: CXF documents that should fail the vectors, each a deliberate defect in the
    /// sequence, scored only when the sequence itself passes. At most 256, in order.
    #[schemars(default)]
    pub(crate) mutants: Optional<Vec<MutantDto>>,
    /// The scenarios, in order. At most 4,096; each runs on a freshly loaded sequence.
    pub(crate) scenarios: Vec<ScenarioDto>,
    /// The envelope every tolerance window of the set is compared under: "funnel" (LBNL funnel,
    /// the default when the key is absent) or "aligned-grid".
    #[schemars(default)]
    pub(crate) envelope: Optional<EnvelopeName>,
}

/// One mutant: where its CXF document is, relative to this file, and what defect it carries.
#[derive(Debug, JsonSchema)]
#[schemars(rename = "Mutant", deny_unknown_fields)]
pub(crate) struct MutantDto {
    /// The mutant's CXF document: a path inside this file's directory, its segments separated by
    /// a slash, none empty, "." or "..", and no backslash. Unique in the set.
    pub(crate) path: String,
    /// What the mutant changes, for people.
    #[schemars(default)]
    pub(crate) description: Optional<String>,
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
    /// The window's mask: 1 to 16 names of Boolean inputs or outputs. A tick counts only where
    /// every one is true; elsewhere it is masked, never checked.
    #[schemars(default)]
    pub(crate) when: Optional<Vec<String>>,
}

/// A tolerance window: a Real output lies inside the envelope built around the expected signal
/// `of` with the tolerances `within`, at every tick from from_s to to_s, both ends included.
#[derive(Debug, JsonSchema)]
#[schemars(rename = "ToleranceWindow", deny_unknown_fields)]
pub(crate) struct ToleranceWindowDto {
    /// The output name: a Real output.
    pub(crate) output: String,
    /// The first second of the window: not negative, not after to_s.
    pub(crate) from_s: Seconds,
    /// The last second of the window: not after horizon_s. The window must hold at least one
    /// tick.
    pub(crate) to_s: Seconds,
    /// The tolerances of the envelope: at least one of the six names.
    pub(crate) within: WithinDto,
    /// The expected signal: a number at every tick, or a step list from from_s.
    pub(crate) of: OfDto,
    /// The window's mask: 1 to 16 names of Boolean inputs or outputs. A tick counts only where
    /// every one is true; elsewhere both signals are zero there and the tick is masked, never
    /// checked.
    #[schemars(default)]
    pub(crate) when: Optional<Vec<String>>,
}

/// The six tolerances of a tolerance window, funnel's names: each optional, at least one
/// present, each finite and not negative; an absent name is zero.
#[derive(Debug, Default, JsonSchema)]
#[schemars(rename = "Within", deny_unknown_fields)]
pub(crate) struct WithinDto {
    /// Absolute tolerance along time, in seconds.
    #[schemars(default)]
    pub(crate) atolx: Optional<Amount>,
    /// Absolute tolerance along the value, in the output's unit.
    #[schemars(default)]
    pub(crate) atoly: Optional<Amount>,
    /// Tolerance along time relative to the time of each point: a fraction, at most 1.
    #[schemars(default)]
    pub(crate) ltolx: Optional<Amount>,
    /// Tolerance along the value relative to the magnitude of each point: a fraction.
    #[schemars(default)]
    pub(crate) ltoly: Optional<Amount>,
    /// Tolerance along time relative to the window's span: a fraction.
    #[schemars(default)]
    pub(crate) rtolx: Optional<Amount>,
    /// Tolerance along the value relative to the expected signal's range: a fraction.
    #[schemars(default)]
    pub(crate) rtoly: Optional<Amount>,
}

/// One step of an expected signal: from time t on, the signal holds value (zero-order hold).
#[derive(Debug, JsonSchema)]
#[schemars(rename = "OfStep", deny_unknown_fields)]
pub(crate) struct OfStepDto {
    /// The time in seconds from which the value holds. The first step is at the window's from_s
    /// exactly, each later one is after the one before it, and none is after to_s. It need not
    /// be a tick: it takes effect at the first tick at or after it.
    pub(crate) t: Seconds,
    /// The value held from t: a number.
    pub(crate) value: Amount,
}

/// What a tolerance window expects over its ticks.
#[derive(Debug)]
pub(crate) enum OfDto {
    /// One value at every tick.
    Constant(Amount),
    /// A step list.
    Steps(Vec<OfStepDto>),
}

impl JsonSchema for OfDto {
    fn schema_name() -> Cow<'static, str> {
        "Of".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "The expected signal of a tolerance window: one number at every tick, or a step list of at least one step (at most 65,536) whose first t is the window's from_s.",
            "anyOf": [
                { "type": "number" },
                { "type": "array", "items": generator.subschema_for::<OfStepDto>() },
            ]
        })
    }
}

/// The envelope's name as written.
#[derive(Debug)]
pub(crate) struct EnvelopeName(pub(crate) String);

impl JsonSchema for EnvelopeName {
    fn schema_name() -> Cow<'static, str> {
        "EnvelopeName".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "enum": ["funnel", "aligned-grid"],
        })
    }
}

/// A number that is not a time and not a port value: a tolerance or an expected signal's value.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Amount(pub(crate) Number);

impl JsonSchema for Amount {
    fn schema_name() -> Cow<'static, str> {
        "Amount".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "number" })
    }
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

/// One expectation, in exactly one of its three forms.
#[derive(Debug)]
pub(crate) enum ExpectationDto {
    /// `output`, `from_s`, `to_s`, `equals`, optionally `when`.
    Window(WindowDto),
    /// `output`, `from_s`, `to_s`, `within`, `of`, optionally `when`. Boxed, so a list of exact
    /// windows holds no room for the tolerance form's keys.
    ToleranceWindow(Box<ToleranceWindowDto>),
    /// `diagnostic`, `at_s`.
    Instant(InstantDto),
}

impl JsonSchema for ExpectationDto {
    fn schema_name() -> Cow<'static, str> {
        "Expectation".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "One expectation, in exactly one of three forms: a window (output, from_s, to_s, equals), a tolerance window (output, from_s, to_s, within, of), either with an optional when, or an instant (diagnostic, at_s).",
            "anyOf": [
                generator.subschema_for::<WindowDto>(),
                generator.subschema_for::<ToleranceWindowDto>(),
                generator.subschema_for::<InstantDto>(),
            ]
        })
    }
}
