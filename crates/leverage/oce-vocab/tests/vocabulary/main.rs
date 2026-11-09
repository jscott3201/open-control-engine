//! The `oce-vocab` integration suite, one binary: the generator of `data/generated.json` and its
//! regenerate-and-diff test, coverage against the catalog in both directions, the curation
//! rules, the strict-JSON and agent-text properties of both files, the unit cross-check against
//! the vendored Modelica sources, and the curated notes held against a loaded engine.

mod generator;
mod support;
mod widths;

mod agent_text;
mod coverage;
mod curation;
mod declared_units;
mod engine_agreement;
mod freshness;
mod strict_json;
