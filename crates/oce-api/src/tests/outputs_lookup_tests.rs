//! The `Outputs::get` lookup contract: the entry ordering its binary search rests on, and its
//! agreement with independent oracles across the whole connector-id space.
//!
//! `Outputs::get` is `O(log n)` only because `entries` is ascending by `ConnectorId`.
//! `oce-validate` rejects any connector whose id differs from its arena index at load;
//! `Engine::resume` changes parameters only and preserves that arena. The ordering pin below guards
//! the validated invariant at the consumer.
//!
//! The `debug_assert` in `Outputs::build` does **not** stand in for it. The workspace release
//! profile leaves `debug-assertions` off, so that assert is absent from the `--release` codegen the
//! gate's second test pass uses; these tests run under both profiles and are the real net.
//!
//! A value test alone could not do this job either. On ascending entries a linear scan and a binary
//! search return the same answer for every key, so their agreement is evidence about neither the
//! ordering nor the search. The ordering is therefore asserted directly, on top of the oracles.

use super::common::*;
use crate::{OcError, Outputs};

/// A composite G36 sequence — 30+ output connectors across many blocks, so the ordering assertion
/// runs against a resolver-minted arena rather than a three-connector toy.
const AHU_SUPPLY_AIR_TEMP_RESET: &str =
    include_str!("../../../oce-cxf/tests/fixtures/g36/ahu_supply_air_temp_reset.jsonld");

/// The pre-change implementation of `Outputs::get`, kept as an independent oracle. It inspects
/// every entry, so it stays correct under any ordering of `entries` and disagrees with the binary
/// search exactly when the ordering contract is broken.
fn linear_scan(outputs: &Outputs, c: ConnectorId) -> Option<Value> {
    outputs
        .iter()
        .find(|(id, _)| *id == c)
        .map(|(_, v)| v.clone())
}

/// A single `Add` with undriven (host-staged) inputs: `conn#0`, `conn#1` in, `conn#2` out — the
/// smallest model with both an output and non-output ids to probe.
fn free_add_model() -> ModelGraph {
    let mut mb = Mb::new();
    let (_, inputs, _) = mb.block(
        "CDL.Reals.Add",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![],
    );
    let mut model = mb.finish();
    model.external_inputs = inputs;
    model
}

/// The engines these tests read `Outputs` from, each ticked once so the values are live: the
/// hand-built accumulator, the hand-built free `Add`, and a resolver-built G36 sequence.
fn ticked_engines() -> Vec<(&'static str, Engine<MemStore>)> {
    let mut accumulator = Engine::in_memory();
    let (m, _, _, _) = build_accumulator_model();
    accumulator.build_model_in_memory(m, None).expect("BUILD");

    let mut free_add = Engine::in_memory();
    free_add
        .build_model_in_memory(free_add_model(), None)
        .expect("BUILD");

    let mut g36 = Engine::in_memory();
    g36.load_cxf(AHU_SUPPLY_AIR_TEMP_RESET.as_bytes())
        .expect("the G36 fixture must load end-to-end");

    let mut engines = vec![
        ("accumulator", accumulator),
        ("free_add", free_add),
        ("g36 ahu_supply_air_temp_reset", g36),
    ];
    for (label, eng) in &mut engines {
        eng.tick(0.0)
            .unwrap_or_else(|e| panic!("{label} tick: {e}"));
    }
    engines
}

#[test]
fn entries_are_strictly_ascending_by_connector_id() {
    for (label, eng) in ticked_engines() {
        let ids: Vec<ConnectorId> = eng.outputs().iter().map(|(id, _)| id).collect();
        assert!(
            !ids.is_empty(),
            "{label} must have outputs for the ordering to mean anything"
        );
        assert!(
            ids.windows(2).all(|w| w[0] < w[1]),
            "{label}: Outputs entries must be strictly ascending by ConnectorId (Outputs::get \
             binary-searches them), got {ids:?}"
        );
    }
}

#[test]
fn scrambled_connector_ids_refuse_before_output_lookup_is_built() {
    let mut model = free_add_model();
    model.connectors[0].id = ConnectorId(1);
    model.connectors[1].id = ConnectorId(0);

    let mut engine = Engine::in_memory();
    let err = engine
        .build_model_in_memory(model, None)
        .expect_err("connector ids that differ from arena positions must refuse");
    let OcError::Validate(err) = err else {
        panic!("expected the structural validation seam, got {err:?}");
    };
    assert_eq!(err.diagnostics.len(), 2, "both displaced ids are named");
    assert!(
        err.diagnostics
            .iter()
            .all(|diag| diag.message.contains("connector id invariant")),
        "unexpected diagnostics: {:?}",
        err.diagnostics
    );
}

#[test]
fn get_agrees_with_a_linear_scan_oracle_across_the_whole_id_space() {
    for (label, eng) in ticked_engines() {
        // Sweep past the last connector so out-of-range ids are covered too: both routes must
        // agree on `None` there, not just on the hits.
        let span = eng.model.connectors.len() as u32 + 4;
        let mut hits = 0_u32;
        for raw in 0..span {
            let c = ConnectorId(raw);
            let got = eng.outputs().get(c).cloned();
            let want = linear_scan(eng.outputs(), c);
            match (&got, &want) {
                (Some(g), Some(w)) => {
                    assert!(
                        g.bit_eq(w),
                        "{label}: get({c:?}) = {g:?} but the linear-scan oracle says {w:?}"
                    );
                    hits += 1;
                }
                (None, None) => {}
                _ => panic!("{label}: get({c:?}) = {got:?} but the oracle says {want:?}"),
            }
        }
        assert_eq!(
            hits as usize,
            eng.outputs().len(),
            "{label}: the sweep must reach every output, else the agreement is vacuous"
        );
    }
}

#[test]
fn get_agrees_with_the_path_keyed_read_for_every_output() {
    // An oracle that shares no code with the search: `to_map`/`get_output` resolve by point path
    // through a hash map, `get` resolves by id through the sorted entries.
    for (label, eng) in ticked_engines() {
        let by_path = eng.outputs().to_map();
        let by_id: Vec<ConnectorId> = eng.outputs().iter().map(|(id, _)| id).collect();
        assert_eq!(
            by_path.len(),
            by_id.len(),
            "{label}: to_map and iter enumerate the same outputs"
        );
        for ((path, want), cid) in by_path.iter().zip(&by_id) {
            let got = eng
                .outputs()
                .get(*cid)
                .unwrap_or_else(|| panic!("{label}: {cid:?} is an enumerated output"));
            assert!(
                got.bit_eq(want),
                "{label}: get({cid:?}) = {got:?} but to_map says {path} = {want:?}"
            );
            let via_engine = eng
                .get_output(path)
                .unwrap_or_else(|e| panic!("{label}: get_output({path}): {e}"));
            assert!(
                via_engine.bit_eq(want),
                "{label}: get_output({path}) = {via_engine:?} but to_map says {want:?}"
            );
        }
    }
}

#[test]
fn get_refuses_input_and_out_of_range_connector_ids() {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(free_add_model(), None)
        .expect("BUILD");
    eng.tick(0.0).expect("tick");
    // conn#0 and conn#1 are the staged inputs; only conn#2 is an output.
    assert!(
        eng.outputs().get(ConnectorId(0)).is_none(),
        "an input connector is not an output"
    );
    assert!(
        eng.outputs().get(ConnectorId(1)).is_none(),
        "an input connector is not an output"
    );
    assert!(
        eng.outputs().get(ConnectorId(2)).is_some(),
        "conn#2 is the Add output"
    );
    // Past the end of the arena, and at the far edge of the id space.
    assert!(eng.outputs().get(ConnectorId(3)).is_none());
    assert!(eng.outputs().get(ConnectorId(u32::MAX)).is_none());
}

#[test]
fn get_on_an_unloaded_engine_is_none_not_a_panic() {
    let eng = Engine::in_memory();
    assert!(eng.outputs().is_empty(), "no model, no outputs");
    assert!(eng.outputs().get(ConnectorId(0)).is_none());
    assert!(eng.outputs().get(ConnectorId(u32::MAX)).is_none());
}

#[test]
fn get_tracks_the_refreshed_value_across_ticks() {
    // `refresh_from` rewrites values in place and leaves keys and order alone, so a lookup must
    // keep resolving to the same entry and return the *current* value after each tick.
    let mut eng = Engine::in_memory();
    let (m, add_out, _, _) = build_accumulator_model();
    eng.build_model_in_memory(m, None).expect("BUILD");
    // The accumulator advances 1, 2, 3, 4 — all small integers, so f64-exact.
    for (k, want) in [1.0_f64, 2.0, 3.0, 4.0].into_iter().enumerate() {
        eng.tick(k as f64).expect("tick");
        let got = eng
            .outputs()
            .get(add_out)
            .expect("add.y stays an enumerated output");
        assert!(
            got.bit_eq(&Value::Real(want)),
            "tick {k}: add.y = {got:?}, want {want}"
        );
        let oracle = linear_scan(eng.outputs(), add_out).expect("the oracle sees the same output");
        assert!(
            oracle.bit_eq(got),
            "tick {k}: get says {got:?}, the linear-scan oracle says {oracle:?}"
        );
    }
}
