//! The README Quickstart, compiled.
//!
//! `clippy --workspace --all-targets` builds this on every PR, so the snippet the README shows
//! cannot drift out of step with the facade without failing the gate.
//! `readme_quickstart.rs` asserts the two copies stay identical.
//!
//! The `allow` below is an artifact of THIS workspace, not of the example: the library lints set
//! `print_stdout = "warn"` and the gate runs clippy with `-D warnings`. A user pasting this into
//! their own project needs no such attribute, which is why it is not in the README.
#![allow(clippy::print_stdout)]

use oce_api::{CollectSpec, Engine, InputSource, SimSpec, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // An engine with the default in-memory store — no database.
    let mut engine = Engine::in_memory();

    // Parse, validate, and freeze the schedule.
    let cxf_bytes = std::fs::read("crates/oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld")?;
    engine.load_cxf(&cxf_bytes)?;

    // Simulate: feed inputs per tick, collect named outputs.
    let metrics = engine.simulate(&SimSpec {
        t_start: 0.0,
        t_stop: 4.0,
        step: 1.0,
        inputs: InputSource::Closure(Box::new(|t| {
            vec![("zone_temp".to_string(), Value::Real(22.0 + t))]
        })),
        collect: CollectSpec::Named {
            points: vec!["sat_setpoint".to_string()],
            stride: 1,
        },
    })?;

    println!("{} rows collected", metrics.trace.times().len());
    Ok(())
}
