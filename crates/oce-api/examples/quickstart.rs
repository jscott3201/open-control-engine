//! The README Quickstart, compiled and executed by the repository gate.
//! The integration test pins these example bytes to the README snippet.
#![allow(clippy::print_stdout)]

use oce_api::{Engine, Value};

const ECONOMIZER: &str = "http://example.org#g36.ahu_economizer";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::in_memory(); // Default in-memory store, no database.
    let cxf_bytes = std::fs::read("crates/oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld")?;
    engine.load_cxf(&cxf_bytes)?;

    // The host owns cadence and supplies every required observation on each frame.
    for index in 0..=4 {
        let time = f64::from(index);
        let observations = [
            (format!("{ECONOMIZER}.return_air_temp"), Value::Real(24.0)),
            (
                format!("{ECONOMIZER}.outdoor_air_temp"),
                Value::Real(18.0 + time),
            ),
            (format!("{ECONOMIZER}.operating_mode"), Value::Integer(1)),
        ];
        let entries: Vec<_> = observations
            .iter()
            .map(|(p, v)| (p.as_str(), v.clone()))
            .collect();
        let prepared = engine.prepare_frame(time, &entries)?;
        let completed = engine.execute_frame(prepared)?;
        println!(
            "frame {} at {}: {:?}",
            completed.sequence(),
            completed.time(),
            completed.outputs()
        );
    }
    Ok(())
}
