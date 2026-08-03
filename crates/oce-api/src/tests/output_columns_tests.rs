//! Agreement between the trace and durable output-column sets.
//!
//! `IoInventory::trace_columns` (the `CollectSpec::All` recording set) and
//! `IoInventory::durable_columns` (the `projected_output_batch` key set) are separate methods
//! serving separate contracts (`_spec/18` D2/D3). Today they are required to be identical: this
//! assertion is the artifact that legitimately flips if either fence is ever reopened, turning a
//! store or trace decision into a red test instead of a silent divergence.

use std::fs;

use super::common::Engine;

#[test]
fn trace_and_durable_output_columns_agree_over_the_g36_corpus() {
    let fixture_dir = format!(
        "{}/../oce-cxf/tests/fixtures/g36",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut fixtures = fs::read_dir(fixture_dir)
        .expect("read G36 fixture corpus")
        .map(|entry| entry.expect("fixture directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonld")
        })
        .collect::<Vec<_>>();
    fixtures.sort();
    assert_eq!(fixtures.len(), 46, "G36 corpus size moved");

    for fixture in fixtures {
        let bytes = fs::read(&fixture).expect("read G36 fixture");
        let mut engine = Engine::in_memory();
        engine
            .load_cxf(&bytes)
            .unwrap_or_else(|error| panic!("{} loads: {error:?}", fixture.display()));
        let trace = engine.io.trace_columns();
        let durable = engine.io.durable_columns();
        assert!(
            !trace.is_empty(),
            "{}: agreement must not be vacuous",
            fixture.display()
        );
        assert_eq!(
            trace,
            durable,
            "{}: trace and durable output columns diverged — a D2/D3 fence moved without its \
             own decision",
            fixture.display()
        );
    }
}
