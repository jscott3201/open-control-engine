//! M1 **exit #1** end-to-end golden: load the canonical `minimal_loop.jsonld` through the full
//! `Engine::load_cxf` pipeline (CXF resolve → flatten → validate → BUILD → schedule) and tick it,
//! asserting the deterministic converging trace **bit-exactly** (`TESTING.md` pillars 2 + 4), plus
//! that malformed CXF is a typed error and never a panic (pillar 1).
//!
//! The model is a geometric feedback loop cut by a `UnitDelay`:
//! `con(2.0) → add → del(UnitDelay) → gain(×0.5) → add` (the k<1 scaler makes it converge).
//! The sum `add.y(k) = 2 + 0.5·add.y(k-1)` has fixpoint 4.0, with `add.y(k) = 4 − 2^(1−k)` — all
//! dyadic, hence f64-exact — so the trace is checked by `to_bits`, never `==`/epsilon.

// Reach the value/IO types through the facade re-export (R-PUB-1: `oce-api` is the single public
// surface) — an external binder names `oce_api::Value`/`ConnectorId`, never a direct oce-model dep.
use oce_api::{ConnectorId, Engine, OcError, Value};

// One source of truth for the canonical fixture (PR-4 authored it; PR-6/PR-11 exercise it).
const MINIMAL_LOOP: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

/// Resolve the `add.y` (feedback sum) output connector id **structurally** from the model — the
/// output of the `CDL.Reals.Add` block — rather than hardcoding `ConnectorId(3)`. This stays
/// correct if PR-9's array normalization renumbers connectors (the resolver is deterministic, so
/// the id matches the one `load_cxf` builds).
fn add_y_id() -> ConnectorId {
    let (model, _report) =
        oce_cxf::import_cxf(MINIMAL_LOOP.as_bytes(), &oce_cxf::ResolveOptions::default())
            .expect("minimal_loop must resolve");
    let add = model
        .blocks
        .iter()
        .find(|b| b.class_iri.as_ref() == "CDL.Reals.Add")
        .expect("an Add block");
    *add.outputs.first().expect("Add has one output")
}

fn add_y(eng: &Engine, id: ConnectorId) -> f64 {
    match eng.outputs().get(id) {
        Some(Value::Real(r)) => *r,
        other => panic!("add.y must be a Real output, got {other:?}"),
    }
}

#[test]
fn load_cxf_minimal_loop_report() {
    let mut eng = Engine::in_memory();
    let report = eng
        .load_cxf(MINIMAL_LOOP.as_bytes())
        .expect("minimal_loop must load end-to-end");
    assert!(
        report.warnings.is_empty(),
        "no warnings expected, got {:?}",
        report.warnings
    );
    assert_eq!(report.block_count, 5, "con, add, del, gain, gt");
    assert_eq!(report.stateful_blocks, 1, "only the UnitDelay is stateful");
}

#[test]
fn minimal_loop_converges_to_four_bit_exact() {
    let id = add_y_id();
    let mut eng = Engine::in_memory();
    eng.load_cxf(MINIMAL_LOOP.as_bytes()).expect("load");

    // First ticks are dyadic-exact: 2, 3, 3.5, 3.75, 3.875, 3.9375, 3.96875.
    let expected_prefix = [2.0_f64, 3.0, 3.5, 3.75, 3.875, 3.9375, 3.96875];
    for (k, &want) in expected_prefix.iter().enumerate() {
        let outs = eng.tick(k as f64).expect("tick must succeed");
        let got = match outs.get(id) {
            Some(Value::Real(r)) => *r,
            o => panic!("add.y must be a Real, got {o:?}"),
        };
        assert_eq!(
            got.to_bits(),
            want.to_bits(),
            "add.y at tick {k}: got {got}, want {want} (bit-exact)"
        );
    }

    // After enough ticks the dyadic error underflows the ULP of 4.0 → add.y is EXACTLY 4.0
    // (reached at tick index 53; the loop below runs well past it).
    for k in 7..80 {
        eng.tick(k as f64).expect("tick");
    }
    assert_eq!(
        add_y(&eng, id).to_bits(),
        4.0_f64.to_bits(),
        "add.y must converge to exactly 4.0"
    );
}

#[test]
fn load_cxf_trace_is_deterministic() {
    // Two independent loads + identical tick sequences → bit-identical add.y traces (exit #4 seed;
    // PR-11 owns the full byte-compare-of-trace-and-schedule harness).
    let id = add_y_id();
    let run = || {
        let mut eng = Engine::in_memory();
        eng.load_cxf(MINIMAL_LOOP.as_bytes()).expect("load");
        (0..32)
            .map(|k| {
                eng.tick(k as f64).expect("tick");
                add_y(&eng, id).to_bits()
            })
            .collect::<Vec<u64>>()
    };
    assert_eq!(run(), run(), "two loads + tick runs must be bit-identical");
}

#[test]
fn load_cxf_rejects_malformed_input_without_panic() {
    // Pillar 1 / R-ERR-1: malformed CXF is a typed OcError, never a panic. Reaching each assert
    // proves no panic; load_cxf is a total function over arbitrary bytes.
    let mut eng = Engine::in_memory();

    // Invalid JSON.
    assert!(matches!(
        eng.load_cxf(b"{ this is not json"),
        Err(OcError::Cxf(_))
    ));

    // A non-subset class that strips cleanly but is intentionally not registered → ClassNotFound,
    // surfaced as Cxf.
    let unknown = MINIMAL_LOOP.replace(
        "Buildings.Controls.OBC.CDL.Reals.Add",
        "Buildings.Controls.OBC.CDL.Reals.NotRegisteredForTest",
    );
    assert!(matches!(
        eng.load_cxf(unknown.as_bytes()),
        Err(OcError::Cxf(_))
    ));

    // Empty @graph → MalformedDocument, surfaced as Cxf.
    assert!(matches!(
        eng.load_cxf(br#"{"@context":{},"@graph":[]}"#),
        Err(OcError::Cxf(_))
    ));

    // A block whose declared interface arity disagrees with its class signature must be rejected at
    // LOAD (the PR-6 review's CRITICAL find) — without the arity check this loads then panics on
    // tick. Strip the Add block's output declaration (the resolver's `containsBlock`/`hasOutput`
    // structure is matched by substring on the unique add.y output reference line).
    let no_add_output = MINIMAL_LOOP.replace(
        r#""S231:hasOutput": { "@id": "http://example.org#MinLoop.add.y" }"#,
        r#""S231:label": "add-without-output""#,
    );
    // Sanity: the replacement actually changed the document.
    assert_ne!(
        no_add_output, MINIMAL_LOOP,
        "fixture substring must match for this test"
    );
    assert!(
        matches!(eng.load_cxf(no_add_output.as_bytes()), Err(OcError::Cxf(_))),
        "a block with mismatched output arity must be rejected at load, not panic on tick"
    );
}
