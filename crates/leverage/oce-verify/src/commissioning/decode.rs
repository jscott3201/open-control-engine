//! The typed reader of a commissioning spec: one pass of the strict cursor into the spec's
//! closed objects (commissioning note, "Test-spec file").
//!
//! Every object is closed. A key the format does not define is `unknown-key` at the key, a
//! required key that is absent is `missing-key` at the object, and a value of another JSON type
//! is `wrong-type` at the value; nothing is skipped, defaulted or read as `null`. The four
//! fields of OBC §12.7 Scenario 1 the verifier does not act on (`generateJson`,
//! `modelJsonDirectory`, `runController` and `controller`) are defined: each takes any one
//! strict JSON value, which is kept for the canonical form and never interpreted (decision 18).
//! The reader stops at the first refusal in document order, and every refusal carries the line
//! and column of the value it concerns.
//!
//! The strict-JSON pass runs before this one and refuses every lexical defect and repeated key,
//! so the cursor's own refusals do not occur here; they are still returned, never unwrapped, and
//! a repeated key is still refused, so this reader is strict standing alone.

use oce_replay::TimeOrigin;

use super::choices::FunnelReference;
use super::decode_sources::{sources, window};
use super::error::{SpecErrorKind, shown};
use super::fields::{
    KeyBound, Read, Reader, choice, expect, keyed, literal, name, opaque, path, put, real, refuse,
    required, text, unknown_key,
};
use super::limits::SpecLimits;
use super::mapping::PointMapping;
use super::model::{
    ComparisonWindow, IndicatorRule, OutputOverride, PointNameMapping, SpecReference, TrendSource,
};
use super::spec::CommissioningSpec;
use super::tolerances::{PartialTolerances, TOLERANCE_NAMES};
use crate::canonical::Json;
use crate::envelope::Envelope;
use crate::error::Location;
use crate::model::Parameter;
use crate::number::Number;
use crate::strict::{Kind, Refusal};

/// Read `text`, comments already removed, as one spec and nothing after it, under `limits`
/// (already [`SpecLimits::effective`]). The strict pass refuses trailing text before this runs;
/// it is refused here too, so the typed reader is strict standing alone.
///
/// # Errors
/// As [`spec`], and `json-syntax` for anything after the object.
pub(crate) fn document(text: &str, limits: &SpecLimits) -> Read<CommissioningSpec> {
    let mut cursor = Reader::reporting(text);
    let read = spec(&mut cursor, limits)?;
    cursor.finish()?;
    Ok(read)
}

/// Read the spec the cursor stands before under `limits` (already [`SpecLimits::effective`]).
///
/// # Errors
/// The first shape or rule refusal in document order; any strict-JSON refusal the cursor meets.
fn spec(cursor: &mut Reader<'_>, limits: &SpecLimits) -> Read<CommissioningSpec> {
    expect(cursor, Kind::Object, "a commissioning spec", "an object")?;
    let mut object = cursor.open_object()?;
    let (mut tag, mut origin, mut global, mut sampled, mut listed) = (None, None, None, None, None);
    let (mut model_json_directory, mut controller) = (None, None);
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        match key.as_str() {
            "schema" => put(&mut tag, schema(cursor)?, &key, at)?,
            "time" => put(&mut origin, time(cursor)?, &key, at)?,
            "tolerances" => put(&mut global, tolerances(cursor)?, &key, at)?,
            "sampling" => put(&mut sampled, sampling(cursor)?, &key, at)?,
            "references" => put(&mut listed, references(cursor, limits)?, &key, at)?,
            "modelJsonDirectory" => put(&mut model_json_directory, opaque(cursor)?, &key, at)?,
            "controller" => put(&mut controller, opaque(cursor)?, &key, at)?,
            _ => return Err(unknown_key(&key, at)),
        }
    }
    Ok(CommissioningSpec {
        time_origin: origin,
        tolerances: global,
        sampling: sampled,
        references: required(listed, "references", object.opened())?,
        model_json_directory,
        controller,
    })
}

/// `schema`: exactly the format's tag.
fn schema(cursor: &mut Reader<'_>) -> Read<()> {
    let (tag, at) = text(cursor, "the schema tag")?;
    if tag != crate::COMMISSIONING_SPEC_SCHEMA {
        return Err(unknown_schema(&tag, at));
    }
    Ok(())
}

/// `unknown-schema` at the tag's value.
pub(crate) fn unknown_schema(tag: &str, at: Location) -> Refusal<SpecErrorKind> {
    refuse(
        SpecErrorKind::UnknownSchema,
        at,
        format!(
            "the schema is {}; this reader reads `{}` only",
            shown(tag),
            crate::COMMISSIONING_SPEC_SCHEMA
        ),
    )
}

/// `time`: `{ "origin": instant }`, an RFC 3339 instant with an explicit offset.
fn time(cursor: &mut Reader<'_>) -> Read<TimeOrigin> {
    expect(cursor, Kind::Object, "the time", "an object")?;
    let mut object = cursor.open_object()?;
    let mut origin = None;
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        match key.as_str() {
            "origin" => {
                let (written, value_at) = text(cursor, "the time origin")?;
                let parsed = TimeOrigin::parse(&written).map_err(|_| {
                    refuse(
                        SpecErrorKind::InvalidTimeOrigin,
                        value_at,
                        format!(
                            "the origin {} is not an instant written `YYYY-MM-DDTHH:MM:SS[.fraction]` with `Z` or a `+hh:mm` or `-hh:mm` offset",
                            shown(&written)
                        ),
                    )
                })?;
                put(&mut origin, parsed, &key, at)?;
            }
            _ => return Err(unknown_key(&key, at)),
        }
    }
    required(origin, "origin", object.opened())
}

/// A tolerance object: any of the six names, each a number that is not negative.
fn tolerances(cursor: &mut Reader<'_>) -> Read<PartialTolerances> {
    expect(cursor, Kind::Object, "the tolerances", "an object")?;
    let mut object = cursor.open_object()?;
    let mut written = [None; 6];
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        let Some(slot) = TOLERANCE_NAMES.iter().position(|known| *known == key) else {
            return Err(unknown_key(&key, at));
        };
        let (value, value_at) = real(cursor, "a tolerance")?;
        if value.seconds() < 0.0 {
            return Err(refuse(
                SpecErrorKind::InvalidTolerance,
                value_at,
                format!("tolerance `{key}` must not be negative"),
            ));
        }
        put(&mut written[slot], value, &key, at)?;
    }
    Ok(PartialTolerances { written })
}

/// `sampling`: seconds, above zero.
fn sampling(cursor: &mut Reader<'_>) -> Read<Number> {
    let (value, at) = real(cursor, "the sampling in seconds")?;
    if value.seconds() > 0.0 {
        Ok(value)
    } else {
        Err(refuse(
            SpecErrorKind::InvalidSampling,
            at,
            "sampling must be above zero",
        ))
    }
}

fn references(cursor: &mut Reader<'_>, limits: &SpecLimits) -> Read<Vec<SpecReference>> {
    expect(cursor, Kind::Array, "the references", "a list")?;
    let mut list = cursor.open_list()?;
    let mut all = Vec::new();
    while cursor.next_item(&mut list)? {
        all.push(reference(cursor, limits)?);
    }
    Ok(all)
}

/// The values of one reference's keys, as they arrive.
#[derive(Default)]
struct ReferenceKeys {
    model: Option<String>,
    sequence: Option<String>,
    point_name_mapping: Option<PointNameMapping>,
    outputs: Option<Vec<OutputOverride>>,
    indicators: Option<Vec<IndicatorRule>>,
    sampling: Option<Number>,
    controller_output: Option<String>,
    cxf: Option<String>,
    parameters: Option<Vec<Parameter>>,
    inputs: Option<Vec<TrendSource>>,
    expected: Option<Vec<TrendSource>>,
    indicator_sources: Option<Vec<TrendSource>>,
    window: Option<ComparisonWindow>,
    envelope: Option<Envelope>,
    funnel_reference: Option<FunnelReference>,
    generate_json: Option<Json>,
    run_controller: Option<Json>,
}

fn reference(cursor: &mut Reader<'_>, limits: &SpecLimits) -> Read<SpecReference> {
    expect(cursor, Kind::Object, "a reference", "an object")?;
    let mut object = cursor.open_object()?;
    let mut keys = ReferenceKeys::default();
    while let Some((key, at)) = cursor.next_key(&mut object)? {
        let k = key.as_str();
        match k {
            "model" => put(&mut keys.model, name(cursor, "the model")?, k, at)?,
            "sequence" => put(&mut keys.sequence, name(cursor, "the sequence")?, k, at)?,
            "pointNameMapping" => put(&mut keys.point_name_mapping, mapping(cursor)?, k, at)?,
            "outputs" => put(&mut keys.outputs, outputs(cursor, limits)?, k, at)?,
            "indicators" => put(&mut keys.indicators, indicators(cursor, limits)?, k, at)?,
            "sampling" => put(&mut keys.sampling, sampling(cursor)?, k, at)?,
            "controllerOutput" => put(
                &mut keys.controller_output,
                path(cursor, "the controllerOutput path")?,
                k,
                at,
            )?,
            "cxf" => put(&mut keys.cxf, path(cursor, "the cxf path")?, k, at)?,
            "parameters" => put(&mut keys.parameters, parameters(cursor, limits)?, k, at)?,
            "inputs" => put(
                &mut keys.inputs,
                sources(cursor, "the inputs", limits)?,
                k,
                at,
            )?,
            "expected" => put(
                &mut keys.expected,
                sources(cursor, "the expected sources", limits)?,
                k,
                at,
            )?,
            "indicatorSources" => put(
                &mut keys.indicator_sources,
                sources(cursor, "the indicator sources", limits)?,
                k,
                at,
            )?,
            "window" => put(&mut keys.window, window(cursor)?, k, at)?,
            "envelope" => put(
                &mut keys.envelope,
                choice(
                    cursor,
                    "the envelope",
                    Envelope::from_name,
                    "`funnel` or `aligned-grid`",
                )?,
                k,
                at,
            )?,
            "funnelReference" => put(
                &mut keys.funnel_reference,
                choice(
                    cursor,
                    "the funnelReference",
                    FunnelReference::from_name,
                    "`engine` or `trend`",
                )?,
                k,
                at,
            )?,
            "generateJson" => put(&mut keys.generate_json, opaque(cursor)?, k, at)?,
            "runController" => put(&mut keys.run_controller, opaque(cursor)?, k, at)?,
            _ => return Err(unknown_key(k, at)),
        }
    }
    let opened = object.opened();
    Ok(SpecReference {
        model: required(keys.model, "model", opened)?,
        sequence: required(keys.sequence, "sequence", opened)?,
        point_name_mapping: keys.point_name_mapping,
        outputs: keys.outputs,
        indicators: keys.indicators,
        sampling: keys.sampling,
        controller_output: keys.controller_output,
        cxf: keys.cxf,
        parameters: keys.parameters,
        inputs: keys.inputs,
        expected: keys.expected,
        indicator_sources: keys.indicator_sources,
        window: keys.window,
        envelope: keys.envelope,
        funnel_reference: keys.funnel_reference,
        generate_json: keys.generate_json,
        run_controller: keys.run_controller,
    })
}

/// `pointNameMapping`: a mapping file's path, or the mapping inline.
fn mapping(cursor: &mut Reader<'_>) -> Read<PointNameMapping> {
    let (kind, at) = cursor.peek()?;
    match kind {
        Kind::String => path(cursor, "the pointNameMapping path").map(PointNameMapping::Path),
        Kind::Array => PointMapping::read(cursor).map(PointNameMapping::Inline),
        other => Err(refuse(
            SpecErrorKind::WrongType,
            at,
            format!(
                "expected the pointNameMapping, a path (a string) or the mapping (a list), found {}",
                other.described()
            ),
        )),
    }
}

/// The bound on an `outputs` or `indicators` map.
fn override_bound(limits: &SpecLimits, what: &'static str) -> KeyBound {
    KeyBound {
        kind: SpecErrorKind::TooManyKeys,
        limit: limits.max_keys,
        what,
    }
}

/// `outputs`: key to tolerance object, sorted by key bytes.
fn outputs(cursor: &mut Reader<'_>, limits: &SpecLimits) -> Read<Vec<OutputOverride>> {
    let bound = override_bound(limits, "keys in one outputs map");
    let mut all: Vec<OutputOverride> = keyed(cursor, "the outputs", bound, tolerances)?
        .into_iter()
        .map(|(key, tolerances)| OutputOverride { key, tolerances })
        .collect();
    // The keys are unique, so an unstable sort orders them one way.
    all.sort_unstable_by(|left, right| left.key.as_bytes().cmp(right.key.as_bytes()));
    Ok(all)
}

/// `indicators`: key to a list of indicator names, sorted by key bytes; each list keeps its
/// order.
fn indicators(cursor: &mut Reader<'_>, limits: &SpecLimits) -> Read<Vec<IndicatorRule>> {
    let bound = override_bound(limits, "keys in one indicators map");
    let mut all: Vec<IndicatorRule> = keyed(cursor, "the indicators", bound, |cursor| {
        expect(cursor, Kind::Array, "the indicator names", "a list")?;
        let mut list = cursor.open_list()?;
        let mut names = Vec::new();
        while cursor.next_item(&mut list)? {
            names.push(name(cursor, "an indicator name")?);
        }
        Ok(names)
    })?
    .into_iter()
    .map(|(key, names)| IndicatorRule { key, names })
    .collect();
    // The keys are unique, so an unstable sort orders them one way.
    all.sort_unstable_by(|left, right| left.key.as_bytes().cmp(right.key.as_bytes()));
    Ok(all)
}

/// `parameters`: parameter path to value, sorted by path bytes.
fn parameters(cursor: &mut Reader<'_>, limits: &SpecLimits) -> Read<Vec<Parameter>> {
    let bound = KeyBound {
        kind: SpecErrorKind::TooManyParameters,
        limit: limits.max_parameters,
        what: "parameters in one reference",
    };
    let mut all: Vec<Parameter> = keyed(cursor, "the parameters", bound, literal)?
        .into_iter()
        .map(|(path, value)| Parameter { path, value })
        .collect();
    // The paths are unique, so an unstable sort orders them one way.
    all.sort_unstable_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    Ok(all)
}

#[cfg(test)]
#[path = "decode_tests.rs"]
mod tests;
