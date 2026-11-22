//! A known divergence from OpenModelica (issue #800): `CDL.Integers.OnCounter` counting the pulses
//! of `CDL.Logical.Change` when its input changes on consecutive frames.
//!
//! `OnCounterChangePilot` (`tools/openmodelica-event-reference/events/`) feeds a held Boolean `u`
//! through `Change` into `OnCounter` (`y_start = 3`) at 1 s frames; `u` changes at 2 and 3 s,
//! 5 s, 7 and 8 s, and 9 s. In Modelica `change(u)` is true only at each change's event instant,
//! so `OnCounter`'s `when trigger` fires at each: six events, and the count reaches 9. Under the
//! engine's HostTick v1 profile `Change` holds its output for a whole frame, so a change on two
//! consecutive frames is one `true` lasting two frames, one rising edge to `trigger`, and the
//! engine counts three events: the count reaches 6. They part at 3 s, the second of two
//! consecutive changes.
//!
//! This is not settled design: it is the open question of issue #800, registered as
//! `DVG-000002` in `fixtures/known_divergence/register.json`. The test pins both signals, the
//! engine's against `change_pulses.engine.csv` (the register's engine evidence) and
//! OpenModelica's against its run, so an engine change that removes or moves the divergence fails
//! here and must update the register, the execution profile and the verification-evidence page.
//! This test is in the `conformance` binary, which no per-PR job runs (it runs nightly in
//! `release-gate.yml` and in `gate.sh full`): an `OnCounter` or `Change` change that closes the
//! divergence would merge on `CI OK` and fail first in the nightly run.

use oce_api::{Engine, Value};

use crate::open_modelica_event_triggers::{assert_run_record, columns, sha256, vendored_source};
use oce_conformance::CombiTimeTable;

const DIR: &str = "fixtures/open_modelica/integers_on_counter";
const CANONICAL: &str =
    include_str!("fixtures/open_modelica/integers_on_counter/change_pulses.canonical.csv");
const ENGINE: &str =
    include_str!("fixtures/open_modelica/integers_on_counter/change_pulses.engine.csv");
const PROVENANCE: &str =
    include_str!("fixtures/open_modelica/integers_on_counter/change_pulses.provenance.json");
const RAW: &[u8] =
    include_bytes!("fixtures/open_modelica/integers_on_counter/change_pulses.raw.csv");
const PILOT: &[u8] =
    include_bytes!("../../../tools/openmodelica-event-reference/events/OnCounterChangePilot.mo");
const REGISTER: &str = include_str!("fixtures/known_divergence/register.json");

const MODEL: &str = "http://example.org#change_pulses";

/// `u -> Change -> OnCounter(y_start = 3).trigger`, `reset` straight to the counter, its count
/// the model's output: the pilot's wiring, in the CXF form `block_harness` writes.
fn document() -> String {
    let m = MODEL;
    format!(
        r#"{{
  "@context": {{ "S231": "http://data.ashrae.org/S231#", "base": "http://example.org#" }},
  "@graph": [
    {{ "@id": "{m}", "@type": "S231:Block", "S231:label": "change_pulses",
      "S231:containsBlock": [{{ "@id": "{m}.cha" }}, {{ "@id": "{m}.cnt" }}],
      "S231:hasInput": [{{ "@id": "{m}.u" }}, {{ "@id": "{m}.reset" }}],
      "S231:hasOutput": {{ "@id": "{m}.y" }} }},
    {{ "@id": "{m}.cha", "@type": "http://example.org#Buildings.Controls.OBC.CDL.Logical.Change",
      "S231:label": "cha", "S231:hasInput": {{ "@id": "{m}.cha.u" }},
      "S231:hasOutput": {{ "@id": "{m}.cha.y" }} }},
    {{ "@id": "{m}.cnt", "@type": "http://example.org#Buildings.Controls.OBC.CDL.Integers.OnCounter",
      "S231:label": "cnt", "S231:hasParameter": {{ "@id": "{m}.cnt.y_start" }},
      "S231:hasInput": [{{ "@id": "{m}.cnt.trigger" }}, {{ "@id": "{m}.cnt.reset" }}],
      "S231:hasOutput": {{ "@id": "{m}.cnt.y" }} }},
    {{ "@id": "{m}.cnt.y_start", "S231:value": {{ "@value": "3",
      "@type": "http://www.w3.org/2001/XMLSchema#integer" }} }},
    {{ "@id": "{m}.cha.u", "@type": "S231:BooleanInput", "S231:isOfDataType": {{ "@id": "S231:Boolean" }} }},
    {{ "@id": "{m}.cnt.trigger", "@type": "S231:BooleanInput", "S231:isOfDataType": {{ "@id": "S231:Boolean" }} }},
    {{ "@id": "{m}.cnt.reset", "@type": "S231:BooleanInput", "S231:isOfDataType": {{ "@id": "S231:Boolean" }} }},
    {{ "@id": "{m}.u", "@type": "S231:BooleanInput", "S231:isOfDataType": {{ "@id": "S231:Boolean" }},
      "S231:isConnectedTo": {{ "@id": "{m}.cha.u" }}, "S231:label": "change_pulses.u" }},
    {{ "@id": "{m}.reset", "@type": "S231:BooleanInput", "S231:isOfDataType": {{ "@id": "S231:Boolean" }},
      "S231:isConnectedTo": {{ "@id": "{m}.cnt.reset" }}, "S231:label": "change_pulses.reset" }},
    {{ "@id": "{m}.cha.y", "@type": "S231:BooleanOutput", "S231:isOfDataType": {{ "@id": "S231:Boolean" }},
      "S231:isConnectedTo": {{ "@id": "{m}.cnt.trigger" }} }},
    {{ "@id": "{m}.cnt.y", "@type": "S231:IntegerOutput", "S231:isOfDataType": {{ "@id": "S231:Integer" }},
      "S231:isConnectedTo": {{ "@id": "{m}.y" }} }},
    {{ "@id": "{m}.y", "@type": "S231:IntegerOutput", "S231:isOfDataType": {{ "@id": "S231:Integer" }} }}
  ]
}}"#
    )
}

fn reference() -> CombiTimeTable {
    CombiTimeTable::parse(CANONICAL).expect("the canonical table")
}

/// The engine's count at the reference's frames, fed the reference's `u` and `reset`.
fn engine_counts() -> Vec<f64> {
    let mut engine = Engine::new();
    let (report, _) = engine
        .load_cxf(document().as_bytes())
        .expect("the model loads");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let (names, rows) = columns(&reference());
    let at = |row: &[f64], name: &str| row[names.iter().position(|n| n == name).expect(name)];
    let (u, reset) = (format!("{MODEL}.u"), format!("{MODEL}.reset"));
    rows.iter()
        .map(|row| {
            let entries = [
                (u.as_str(), Value::Boolean(at(row, "u") == 1.0)),
                (reset.as_str(), Value::Boolean(at(row, "reset") == 1.0)),
            ];
            let frame = engine.prepare_frame(row[0], &entries).expect("prepares");
            let done = engine.execute_frame(frame).expect("executes");
            let outputs = done.outputs();
            assert_eq!(outputs.len(), 1, "{outputs:?}");
            match &outputs[0].1 {
                Value::Integer(count) => *count as f64,
                other => panic!("t {}: {other:?}", row[0]),
            }
        })
        .collect()
}

/// `change_pulses.engine.csv`: `x,y`, one row per frame.
fn recorded_engine() -> Vec<f64> {
    ENGINE
        .lines()
        .skip(1)
        .map(|line| {
            line.split_once(',')
                .expect("two fields")
                .1
                .parse()
                .expect("a count")
        })
        .collect()
}

#[test]
fn the_engine_counts_one_event_where_openmodelica_counts_two_consecutive_change_pulses() {
    let (_, rows) = columns(&reference());
    let openmodelica: Vec<f64> = rows.iter().map(|row| row[row.len() - 1]).collect();
    let engine = engine_counts();
    // OpenModelica: every change of `u` is an event (2, 3, 5, 7, 8 and 9 s).
    assert_eq!(
        openmodelica,
        [3.0, 3.0, 4.0, 5.0, 5.0, 6.0, 6.0, 7.0, 8.0, 9.0]
    );
    // The engine under HostTick v1: `Change` true at 2-3, 5, 7-9 s is three rising edges.
    assert_eq!(engine, [3.0, 3.0, 4.0, 4.0, 4.0, 5.0, 5.0, 6.0, 6.0, 6.0]);
    assert_eq!(
        engine,
        recorded_engine(),
        "the engine no longer gives the registered divergence (DVG-000002, issue #800): update \
         register.json, docs/execution-profile.md and docs/verification-evidence.md"
    );
    let differ: Vec<usize> = (0..engine.len())
        .filter(|k| engine[*k] != openmodelica[*k])
        .collect();
    assert_eq!(differ, [3, 4, 5, 6, 7, 8, 9]);
    // A second run repeats every count.
    assert_eq!(engine, engine_counts());
}

#[test]
fn the_divergence_is_registered_with_this_evidence() {
    assert!(REGISTER.contains(r#""id": "DVG-000002""#));
    assert!(REGISTER.contains(&format!(r#""sha256": "{}""#, sha256(ENGINE.as_bytes()))));
    assert!(REGISTER.contains(&format!(r#""sha256": "{}""#, sha256(CANONICAL.as_bytes()))));
    assert!(REGISTER.contains("https://github.com/jscott3201/open-control-engine/issues/800"));
}

#[test]
fn the_pulse_run_s_provenance_binds_its_pilot_table_and_raw_result() {
    let record: serde_json::Value = serde_json::from_str(PROVENANCE).expect("provenance JSON");
    assert_eq!(record["class"], "CDL.Integers.OnCounter");
    assert_run_record(
        &record,
        DIR,
        PILOT,
        vendored_source("CDL.Integers.OnCounter"),
        RAW,
    );
    assert_eq!(
        record["change_pulses_sha256"].as_str(),
        Some(sha256(CANONICAL.as_bytes()).as_str())
    );
    // The divergence depends on `Change` as much as on `OnCounter`: its source is bound too.
    assert_eq!(
        record["also_source"].as_str(),
        Some("Buildings/Controls/OBC/CDL/Logical/Change.mo")
    );
    assert_eq!(
        record["also_source_sha256"].as_str(),
        Some(sha256(vendored_source("CDL.Logical.Change")).as_str())
    );
    let pilot = std::str::from_utf8(PILOT).expect("UTF-8");
    assert!(pilot.contains("Buildings.Controls.OBC.CDL.Logical.Change consecutivePulsesChange"));
    assert!(pilot.contains("connect(consecutivePulsesChange.y, consecutivePulses.trigger);"));
}
