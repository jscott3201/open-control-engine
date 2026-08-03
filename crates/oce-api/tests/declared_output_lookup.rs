//! Characterization of the declared boundary-output identity space on the facade lookup
//! surfaces, plus a no-drift capture of the surfaces that must NOT move with it.
//!
//! A CXF root `S231:hasOutput` declares a composite's output contract. This suite derives that
//! declared-IRI set per fixture with a plain `serde_json` walk that shares no helper with
//! `oce-cxf`, then records, for every declared IRI, the outcome on all three lookup surfaces —
//! `get_output`, `watch`, and `simulate(CollectSpec::Named)` — and the `set_input` refusal.
//!
//! The no-drift capture pins the surfaces the boundary-output facade slice must leave
//! byte-identical: `point_list(None)` rows, `Outputs::to_map` keys, `io_summary()`, and one
//! `step_realtime` `StepReport.written` count per fixture. Those unchanged bytes are the
//! falsifiable proof that the D2 (rows) and D3 (durable keys) fences held.

use std::fs;
use std::path::PathBuf;

use oce_api::oce_store::Store;
use oce_api::{CollectSpec, Engine, InputSource, OcError, SimSpec, Value};

/// Declared outputs resolve on the lookup surfaces only when a real connector carries the
/// declared IRI — today that is exactly the pass-through class. The elided class returns
/// `UnknownPoint` everywhere.
const RESOLVABLE_DECLARED_OUTPUTS: usize = 2;

/// Declared outputs with no facade identity at all (the elided class).
const UNRESOLVABLE_DECLARED_OUTPUTS: usize = 130;

/// Whether a declared boundary output is expected to resolve on the three lookup surfaces.
/// Base behavior: only the pass-through declared outputs (which own a real connector) resolve.
fn lookup_should_resolve(is_pass_through: bool) -> bool {
    is_pass_through
}

/// Every `.jsonld` document in the G36 corpus, sorted for deterministic iteration order.
fn corpus_fixtures() -> Vec<PathBuf> {
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
    fixtures
}

fn load_fixture(path: &PathBuf) -> (String, Vec<u8>, Engine<impl Store>) {
    let fixture_name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("UTF-8 fixture stem")
        .to_owned();
    let bytes = fs::read(path).expect("read G36 fixture");
    let mut engine = Engine::in_memory();
    engine
        .load_cxf(&bytes)
        .unwrap_or_else(|error| panic!("{fixture_name} loads: {error:?}"));
    (fixture_name, bytes, engine)
}

/// One declared boundary output read straight out of the raw document.
struct DeclaredOutputExpectation {
    iri: String,
    /// Whether the declared node is wired directly to a root boundary input (the pass-through
    /// form, which lowers to a real connector carrying this IRI).
    is_pass_through: bool,
}

/// The root `S231:hasOutput` IRIs of a fixture, in authored array order, each classified as
/// pass-through or elided — a plain `serde_json` walk sharing no helper with `oce-cxf`.
///
/// The root is the unique node that carries `S231:containsBlock` and is itself contained by no
/// other node's `containsBlock` (asserted). A declared output is a pass-through iff an
/// `S231:isConnectedTo` edge — written on either endpoint, the property is symmetric — joins it
/// to one of the root's `S231:hasInput` IRIs.
fn declared_outputs(bytes: &[u8]) -> Vec<DeclaredOutputExpectation> {
    let document: serde_json::Value = serde_json::from_slice(bytes).expect("fixture is JSON");
    let graph = document["@graph"].as_array().expect("@graph is an array");

    let refs = |node: &serde_json::Value, key: &str| -> Vec<String> {
        match &node[key] {
            serde_json::Value::Array(items) => items
                .iter()
                .map(|item| item["@id"].as_str().expect("reference has @id").to_owned())
                .collect(),
            serde_json::Value::Object(map) => {
                vec![map["@id"].as_str().expect("reference has @id").to_owned()]
            }
            _ => Vec::new(),
        }
    };

    let contained: Vec<String> = graph
        .iter()
        .flat_map(|node| refs(node, "S231:containsBlock"))
        .collect();
    let roots: Vec<&serde_json::Value> = graph
        .iter()
        .filter(|node| {
            !refs(node, "S231:containsBlock").is_empty()
                && !contained
                    .iter()
                    .any(|id| node["@id"].as_str().expect("node has @id") == id)
        })
        .collect();
    assert_eq!(roots.len(), 1, "fixture must have exactly one root");
    let root = roots[0];

    let boundary_inputs = refs(root, "S231:hasInput");
    let declared = refs(root, "S231:hasOutput");

    declared
        .into_iter()
        .map(|iri| {
            let is_pass_through = graph.iter().any(|node| {
                let id = node["@id"].as_str().expect("node has @id");
                let targets = refs(node, "S231:isConnectedTo");
                (id == iri && targets.iter().any(|t| boundary_inputs.contains(t)))
                    || (boundary_inputs.contains(&id.to_owned())
                        && targets.iter().any(|t| t == &iri))
            });
            DeclaredOutputExpectation {
                iri,
                is_pass_through,
            }
        })
        .collect()
}

/// The `simulate(CollectSpec::Named)` outcome for one key: `Ok` iff the key resolves. Runs a
/// one-tick horizon on a fresh load so the probe cannot leak state into any other probe.
fn named_collect_resolves(path: &PathBuf, key: &str) -> Result<(), OcError> {
    let (_, _, mut engine) = load_fixture(path);
    engine
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop: 0.0,
            step: 1.0,
            inputs: InputSource::None,
            collect: CollectSpec::Named {
                points: vec![key.to_owned()],
                stride: 1,
            },
        })
        .map(|_| ())
}

#[test]
fn declared_output_lookup_outcome_is_uniform_across_the_three_surfaces() {
    let mut resolvable = 0usize;
    let mut unresolvable = 0usize;
    for fixture in corpus_fixtures() {
        let (fixture_name, bytes, engine) = load_fixture(&fixture);
        for expectation in declared_outputs(&bytes) {
            let iri = expectation.iri.as_str();
            let expect_ok = lookup_should_resolve(expectation.is_pass_through);

            let get_output_ok = engine.get_output(iri).is_ok();
            let watch_ok = engine.watch(&[iri]).is_ok();
            let named_ok = named_collect_resolves(&fixture, iri).is_ok();
            assert_eq!(
                (get_output_ok, watch_ok, named_ok),
                (expect_ok, expect_ok, expect_ok),
                "{fixture_name}: declared output {iri} must be {} on ALL THREE lookup surfaces \
                 (get_output, watch, CollectSpec::Named)",
                if expect_ok { "Ok" } else { "UnknownPoint" }
            );
            if !expect_ok {
                // The refusal is the typed UnknownPoint naming the queried key, never a panic
                // and never a different error class.
                assert!(
                    matches!(engine.get_output(iri), Err(OcError::UnknownPoint(ref p)) if p == iri),
                    "{fixture_name}: {iri} must refuse as UnknownPoint naming the key"
                );
                assert!(
                    matches!(engine.watch(&[iri]), Err(OcError::UnknownPoint(ref p)) if p == iri),
                    "{fixture_name}: watch({iri}) must refuse as UnknownPoint naming the key"
                );
            }
            if expect_ok {
                resolvable += 1;
            } else {
                unresolvable += 1;
            }
        }
    }
    assert_eq!(
        (resolvable, unresolvable),
        (RESOLVABLE_DECLARED_OUTPUTS, UNRESOLVABLE_DECLARED_OUTPUTS),
        "corpus-wide declared-output lookup split moved"
    );
}

#[test]
fn set_input_refuses_every_declared_output_name() {
    // The alias space is output-only (R18-2): a declared output name must never stage an input.
    // This holds before and after the boundary-output facade slice.
    let mut probed = 0usize;
    for fixture in corpus_fixtures() {
        let (fixture_name, bytes, mut engine) = load_fixture(&fixture);
        for expectation in declared_outputs(&bytes) {
            let iri = expectation.iri.as_str();
            assert!(
                engine.set_input(iri, Value::Real(1.0)).is_err(),
                "{fixture_name}: set_input({iri}) must refuse a declared output name"
            );
            probed += 1;
        }
    }
    assert_eq!(
        probed,
        RESOLVABLE_DECLARED_OUTPUTS + UNRESOLVABLE_DECLARED_OUTPUTS,
        "set_input negative probe count moved"
    );
}

/// Deterministic render of the surfaces the slice must not move: `point_list(None)` rows,
/// `to_map` keys, `io_summary()`, and one `step_realtime` written count.
fn no_drift_render() -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for fixture in corpus_fixtures() {
        let (fixture_name, _bytes, mut engine) = load_fixture(&fixture);
        let _ = writeln!(out, "fixture: {fixture_name}");
        let io = engine.io_summary();
        let _ = writeln!(
            out,
            "io_summary: ai={} ao={} di={} do={} network={} total={}",
            io.analog_inputs,
            io.analog_outputs,
            io.digital_inputs,
            io.digital_outputs,
            io.network,
            io.total
        );
        let rows = engine.point_list(None).expect("point inventory");
        let _ = writeln!(out, "point_list: {}", rows.len());
        for row in &rows {
            let _ = writeln!(
                out,
                "  {} {:?} {:?} {:?}",
                row.path, row.direction, row.value_type, row.io_class
            );
        }
        let keys = engine.outputs().to_map();
        let _ = writeln!(out, "to_map: {}", keys.len());
        for (key, _) in &keys {
            let _ = writeln!(out, "  {key}");
        }
        engine.set_realtime_epoch_unix_nanos(1_700_000_000_000_000_000);
        let report = engine.step_realtime(0.0).expect("one realtime step");
        let _ = writeln!(out, "step_realtime_written: {}", report.written);
    }
    out
}

#[test]
fn control_surfaces_match_the_no_drift_capture_byte_exactly() {
    let capture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/declared_output_no_drift.txt");
    let actual = no_drift_render();
    if oce_bless::enabled("OCE_BLESS") {
        fs::write(&capture_path, &actual).expect("write no-drift capture");
        return;
    }
    let expected = fs::read_to_string(&capture_path)
        .expect("no-drift capture missing; regenerate deliberately with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "a control surface moved: point_list rows, to_map keys, io_summary, or \
         step_realtime written drifted from the base capture — the D2/D3 fence did not hold"
    );
}
