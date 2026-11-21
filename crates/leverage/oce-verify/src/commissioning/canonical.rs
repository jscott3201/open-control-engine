//! The canonical document of a commissioning spec (commissioning note, "Test-spec file": "the
//! verifier always writes strict JSON with a `schema` tag, so a stored spec is canonical").
//!
//! It is written by the crate's one canonical writer ([`crate::canonical`]): sorted keys, one
//! spelling per number, two-space indentation, ASCII only. Two things are this format's own:
//!
//! - **`schema` is always written,** whether or not the document that was loaded had one, so an
//!   LBL spec and its stored form have one identity.
//! - **The keys of an `outputs` and an `indicators` map are written in declaration order,** not
//!   sorted. "Overrides apply in declaration order, later fields winning", so the order of
//!   those keys is part of what the spec means: a writer that sorted them would give two specs
//!   that compare an output under different tolerances one canonical form and one `spec_id`,
//!   and a stored spec would run differently from the file it was written from. Every other
//!   object is sorted, a reference's `parameters` and a source's `columns` among them.
//!
//! Everything a spec holds is written, the fields the verifier ignores included: a key the file
//! gave is in the canonical form, and one it did not give is not.

use super::model::{PointNameMapping, SourceTimeColumn, SpecReference, TrendSource};
use super::spec::CommissioningSpec;
use super::tolerances::{PartialTolerances, TOLERANCE_NAMES};
use crate::canonical::Json;
use crate::number::Number;

fn entry(key: &str, value: Json) -> (String, Json) {
    (key.to_owned(), value)
}

fn number(value: Number) -> Json {
    Json::Raw(value.canonical())
}

/// A tolerance object: the names it gives, sorted.
fn tolerances(written: &PartialTolerances) -> Json {
    Json::object(
        TOLERANCE_NAMES
            .iter()
            .zip(written.written)
            .filter_map(|(name, value)| value.map(|value| entry(name, number(value)))),
    )
}

fn source(source: &TrendSource) -> Json {
    let mut entries = vec![
        entry("path", Json::str(&source.path)),
        entry("format", Json::str(source.format.as_str())),
        entry(
            "timeColumn",
            match &source.time_column {
                SourceTimeColumn::Index(position) => Json::Raw(position.to_string()),
                SourceTimeColumn::Name(name) => Json::str(name),
            },
        ),
        entry(
            "columns",
            Json::object(
                source
                    .columns
                    .iter()
                    .map(|column| (column.column.clone(), Json::str(&column.point))),
            ),
        ),
    ];
    if let Some(table) = &source.table {
        entries.push(entry("table", Json::str(table)));
    }
    if let Some(max_age) = source.hold_max_age {
        entries.push(entry(
            "hold",
            Json::object([entry("maxAge", number(max_age))]),
        ));
    }
    Json::object(entries)
}

fn sources(list: &[TrendSource]) -> Json {
    Json::List(list.iter().map(source).collect())
}

fn reference(reference: &SpecReference) -> Json {
    let mut entries = vec![
        entry("model", Json::str(&reference.model)),
        entry("sequence", Json::str(&reference.sequence)),
    ];
    let mut add = |key: &str, value: Option<Json>| {
        if let Some(value) = value {
            entries.push(entry(key, value));
        }
    };
    add(
        "pointNameMapping",
        reference
            .point_name_mapping
            .as_ref()
            .map(|mapping| match mapping {
                PointNameMapping::Path(path) => Json::str(path),
                PointNameMapping::Inline(mapping) => mapping.to_json(),
            }),
    );
    // Declaration order, not sorted: see the module documentation.
    add(
        "outputs",
        reference.outputs.as_ref().map(|overrides| {
            Json::Object(
                overrides
                    .iter()
                    .map(|each| (each.key.clone(), tolerances(&each.tolerances)))
                    .collect(),
            )
        }),
    );
    add(
        "indicators",
        reference.indicators.as_ref().map(|rules| {
            Json::Object(
                rules
                    .iter()
                    .map(|rule| {
                        (
                            rule.key.clone(),
                            Json::List(rule.names.iter().map(|name| Json::str(name)).collect()),
                        )
                    })
                    .collect(),
            )
        }),
    );
    add("sampling", reference.sampling.map(number));
    add(
        "controllerOutput",
        reference.controller_output.as_deref().map(Json::str),
    );
    add("cxf", reference.cxf.as_deref().map(Json::str));
    add(
        "parameters",
        reference.parameters.as_ref().map(|parameters| {
            Json::object(
                parameters
                    .iter()
                    .map(|each| (each.path.clone(), Json::Raw(each.value.canonical()))),
            )
        }),
    );
    add("inputs", reference.inputs.as_deref().map(sources));
    add("expected", reference.expected.as_deref().map(sources));
    add(
        "indicatorSources",
        reference.indicator_sources.as_deref().map(sources),
    );
    add(
        "window",
        reference.window.map(|window| {
            Json::object([
                entry("start", number(window.start)),
                entry("stop", number(window.stop)),
            ])
        }),
    );
    add(
        "envelope",
        reference
            .envelope
            .map(|envelope| Json::str(envelope.as_str())),
    );
    add(
        "funnelReference",
        reference
            .funnel_reference
            .map(|role| Json::str(role.as_str())),
    );
    add("generateJson", reference.generate_json.clone());
    add("runController", reference.run_controller.clone());
    Json::object(entries)
}

/// The canonical value of `spec`: its tag, then every key it holds.
pub(crate) fn spec(spec: &CommissioningSpec) -> Json {
    let mut entries = vec![
        entry("schema", Json::str(crate::COMMISSIONING_SPEC_SCHEMA)),
        entry(
            "references",
            Json::List(spec.references.iter().map(reference).collect()),
        ),
    ];
    if let Some(origin) = &spec.time_origin {
        entries.push(entry(
            "time",
            Json::object([entry("origin", Json::str(origin.as_str()))]),
        ));
    }
    if let Some(global) = &spec.tolerances {
        entries.push(entry("tolerances", tolerances(global)));
    }
    if let Some(sampling) = spec.sampling {
        entries.push(entry("sampling", number(sampling)));
    }
    if let Some(directory) = &spec.model_json_directory {
        entries.push(entry("modelJsonDirectory", directory.clone()));
    }
    if let Some(controller) = &spec.controller {
        entries.push(entry("controller", controller.clone()));
    }
    Json::object(entries)
}

#[cfg(test)]
#[path = "canonical_tests.rs"]
mod tests;
