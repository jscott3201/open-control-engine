//! Exact `CDL.Utilities.Assert` warning-observation conformance against its Tier-A golden.
//!
//! `Assert` has no output connector, so the shared output harness cannot see it. Its Tier-A golden
//! carries a `warning` column instead: `1.0` means exactly one Warning record with the golden's
//! `message` at that row's time, `0.0` means none. The comparison here drives a standalone
//! `Assert` CXF through the frozen facade and checks each completed frame's diagnostics.
//!
//! This lives apart from `per_block_utilities.rs` because that file is a bound source of the
//! retained native strict-bit evidence; Assert has no Real output and no libm dependence.

use std::fs;
use std::path::PathBuf;

use oce_api::{AssertLevel, Engine, Value};
use oce_conformance::CombiTimeTable;
use serde_json::Value as Json;

const ASSERT_INPUT: &str = "http://example.org#utilities_assert.u";

fn assert_golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/golden-gen/goldens/CDL/Utilities/Assert")
}

fn assert_cxf(message: &str) -> String {
    // The CXF string literal is JSON-in-JSON: the parameter value is a quoted Modelica string.
    let literal = serde_json::to_string(&format!("\"{message}\"")).unwrap();
    format!(
        r#"{{
  "@context": {{"S231": "http://data.ashrae.org/S231P#"}},
  "@graph": [
    {{"@id": "http://example.org#utilities_assert", "@type": "S231:Block",
     "S231:containsBlock": {{"@id": "http://example.org#utilities_assert.block"}},
     "S231:hasInput": {{"@id": "{ASSERT_INPUT}"}}}},
    {{"@id": "http://example.org#utilities_assert.block", "@type": "CDL.Utilities.Assert",
     "S231:hasInput": {{"@id": "http://example.org#utilities_assert.block.u"}},
     "S231:hasParameter": {{"@id": "http://example.org#utilities_assert.block.message"}}}},
    {{"@id": "http://example.org#utilities_assert.block.message", "@type": "S231:Parameter",
     "S231:isOfDataType": {{"@id": "S231:String"}},
     "S231:value": {literal}}},
    {{"@id": "http://example.org#utilities_assert.block.u", "@type": "S231:BooleanInput",
     "S231:isOfDataType": {{"@id": "S231:Boolean"}}}},
    {{"@id": "{ASSERT_INPUT}", "@type": "S231:BooleanInput",
     "S231:isOfDataType": {{"@id": "S231:Boolean"}},
     "S231:isConnectedTo": {{"@id": "http://example.org#utilities_assert.block.u"}}}}
  ]
}}
"#
    )
}

/// One rendered line per frame: time bits, record count and each record's fields.
fn drive_assert(reference: &CombiTimeTable, message: &str) -> Vec<String> {
    let names = reference
        .col_names
        .as_ref()
        .expect("named reference columns");
    assert_eq!(names, &["time", "u", "warning"]);
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(assert_cxf(message).as_bytes())
        .expect("standalone Assert CXF loads");
    assert_eq!(report.block_count, 1);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);

    let mut lines = Vec::with_capacity(reference.n_rows);
    for row in reference.data.chunks_exact(reference.n_cols) {
        let (t, u) = (row[0], row[1]);
        assert!(u == 0.0 || u == 1.0, "Boolean input cell {u}");
        let prepared = engine
            .prepare_frame(t, &[(ASSERT_INPUT, Value::Boolean(u == 1.0))])
            .expect("Assert frame prepares");
        let completed = engine
            .execute_frame(prepared)
            .expect("a warning never stops the frame");
        assert!(completed.outputs().is_empty(), "Assert has no output port");
        let mut line = format!(
            "{:016x}|count={}",
            t.to_bits(),
            completed.diagnostics().len()
        );
        for event in completed.diagnostics() {
            line.push_str(&format!(
                "|{}|{}|{:016x}|{:?}",
                event.block,
                event.message,
                event.t.to_bits(),
                event.level
            ));
        }
        lines.push(line);
    }
    lines
}

fn expected_lines(reference: &CombiTimeTable, message: &str) -> Vec<String> {
    reference
        .data
        .chunks_exact(reference.n_cols)
        .map(|row| {
            let (t, warning) = (row[0], row[2]);
            match warning {
                0.0 => format!("{:016x}|count=0", t.to_bits()),
                1.0 => format!(
                    "{:016x}|count=1|CDL.Utilities.Assert|{message}|{:016x}|{:?}",
                    t.to_bits(),
                    t.to_bits(),
                    AssertLevel::Warning
                ),
                other => panic!("warning cell must be 0.0 or 1.0, got {other}"),
            }
        })
        .collect()
}

#[test]
fn assert_warning_records_match_tier_a_observation_golden_exactly() {
    let dir = assert_golden_dir();
    let prov: Json =
        serde_json::from_slice(&fs::read(dir.join("warning.prov.json")).unwrap()).unwrap();
    assert_eq!(prov["class_path"], "CDL.Utilities.Assert");
    assert_eq!(prov["signal"], "warning");
    assert_eq!(prov["depends_on_oce_blocks"], false);
    let message = prov["message"]
        .as_str()
        .expect("golden records its message");
    let reference = CombiTimeTable::read(&dir.join("reference.csv")).unwrap();
    assert_eq!(
        reference.n_rows as u64,
        prov["n_samples"].as_u64().unwrap(),
        "a zero-row comparison must not pass vacuously"
    );

    let expected = expected_lines(&reference, message);
    assert!(
        expected.iter().any(|line| line.ends_with("|count=0")),
        "golden must exercise silent frames"
    );
    assert!(
        expected
            .first()
            .is_some_and(|line| line.contains("|count=1|")),
        "golden must exercise a false first tick"
    );
    assert_eq!(drive_assert(&reference, message), expected);
}

#[test]
fn assert_warning_observation_runs_are_deterministic() {
    let dir = assert_golden_dir();
    let reference = CombiTimeTable::read(&dir.join("reference.csv")).unwrap();
    let message = "determinism probe";
    assert_eq!(
        drive_assert(&reference, message),
        drive_assert(&reference, message)
    );
}
