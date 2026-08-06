//! `InputSource::Constant` staging: what pre-resolving the point names once changed, and what it
//! did not.
//!
//! `simulate` resolves a `Constant` list's names to `(ConnectorId, Value)` once, before the horizon
//! runs, and writes that plan on every step. Only the resolution moved; the write cadence is
//! unchanged. Three of the tests below are placement or all-or-nothing pins rather than value
//! checks, because that is where the change is observable:
//!
//! - staging is now all-or-nothing across the whole list. Per-name staging wrote every pair
//!   preceding a bad one before refusing; the resolved plan writes nothing at all. This matches
//!   what `simulate`'s sibling collect path already documents for itself — an unknown name fails
//!   fast with no partial trace and no advanced model state.
//! - the resolution sits immediately *after* `simulate`'s `prev_t` reset. A failed staging has
//!   always left `prev_t` cleared, so a later backwards `tick` still succeeds; resolving above the
//!   reset would preserve the prior tick's `prev_t` and turn that `tick` into
//!   `Err(OcError::TimeRegression)` — a public error-surface change.
//!
//! Two *distinct* names sharing one connector is absent here because it cannot be built, not
//! because the corpus happens not to contain it: `IoInventory::build_at_load` fills `input_by_path`
//! from `point_rows_at_load`, which walks `model.connectors` once and emits at most one row per
//! connector, so every `ConnectorId` lands under exactly one path key.

use super::common::*;

/// A single `Add` with undriven (host-staged) inputs: `conn#0`, `conn#1` in, `conn#2` out.
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

/// The same `Add`, but both input connectors carry one shared authored `@id`. That is the shape a
/// composite boundary input takes after CXF import: one host-visible point path, several staging
/// targets, which `IoInventory::input_by_path` records as a `Vec<ConnectorId>`.
fn fan_out_add_model(shared: &str) -> ModelGraph {
    let mut model = free_add_model();
    for cid in model.external_inputs.clone() {
        model.connectors[cid.0 as usize].iri = Some(Arc::from(shared));
    }
    model
}

/// A `Ramp`, whose two inputs are a `Real` and a `Boolean`, fanned out under one shared `@id` — a
/// name whose fan-out is *type-heterogeneous*, so no single value can satisfy every target.
fn mixed_type_fan_out_model(shared: &str) -> ModelGraph {
    let mut mb = Mb::new();
    let (_, inputs, _) = mb.block(
        "CDL.Reals.Ramp",
        &[ValueType::Real, ValueType::Boolean],
        &[ValueType::Real],
        vec![
            rp("raisingSlewRate", 2.0),
            rp("fallingSlewRate", -3.0),
            rp("Td", 0.1),
        ],
    );
    let mut model = mb.finish();
    for &cid in &inputs {
        model.connectors[cid.0 as usize].iri = Some(Arc::from(shared));
    }
    model.external_inputs = inputs;
    model
}

fn constant_spec(t_stop: f64, pairs: Vec<(String, Value)>, collect: CollectSpec) -> SimSpec {
    SimSpec {
        t_start: 0.0,
        t_stop,
        step: 1.0,
        inputs: InputSource::Constant(pairs),
        collect,
    }
}

fn named(points: &[&str]) -> CollectSpec {
    CollectSpec::Named {
        points: points.iter().map(|p| (*p).to_string()).collect(),
        stride: 1,
    }
}

fn loaded(model: ModelGraph) -> Engine<MemStore> {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(model, None).expect("BUILD");
    eng
}

// ---- all-or-nothing refusal (the observable pre-resolution changed) ----

#[test]
fn an_unknown_constant_name_refuses_and_stages_no_pair_before_it() {
    let mut eng = loaded(free_add_model());
    let spec = constant_spec(
        2.0,
        vec![
            ("conn#0".to_string(), Value::Real(3.0)),
            ("nope".to_string(), Value::Real(4.0)),
        ],
        CollectSpec::None,
    );
    assert!(
        matches!(eng.simulate(&spec), Err(OcError::UnknownPoint(p)) if p == "nope"),
        "an unknown Constant name is still OcError::UnknownPoint"
    );
    // Per-name staging wrote `conn#0 = Real(3.0)` before reaching the bad name. The resolved plan
    // is built and type-checked in full before anything is written, so the run stages nothing.
    assert!(
        eng.state.values[0].bit_eq(&Value::Real(0.0)),
        "the pair preceding the unknown name must not be staged, got {:?}",
        eng.state.values[0]
    );
}

#[test]
fn a_type_mismatched_constant_value_refuses_and_stages_no_pair_before_it() {
    let mut eng = loaded(free_add_model());
    let spec = constant_spec(
        2.0,
        vec![
            ("conn#0".to_string(), Value::Real(3.0)),
            ("conn#1".to_string(), Value::Boolean(true)),
        ],
        CollectSpec::None,
    );
    assert!(
        matches!(eng.simulate(&spec), Err(OcError::InputType(p)) if p == "conn#1"),
        "a wrong-typed Constant value is still OcError::InputType, named by point"
    );
    assert!(
        eng.state.values[0].bit_eq(&Value::Real(0.0)),
        "the pair preceding the mismatch must not be staged, got {:?}",
        eng.state.values[0]
    );
    assert!(
        eng.state.values[1].bit_eq(&Value::Real(0.0)),
        "the mismatched pair itself must not be staged, got {:?}",
        eng.state.values[1]
    );
}

#[test]
fn a_refusal_leaves_no_tick_run_and_no_trace() {
    let mut eng = loaded(free_add_model());
    let spec = constant_spec(
        9.0,
        vec![("nope".to_string(), Value::Real(1.0))],
        named(&["conn#2"]),
    );
    assert!(matches!(eng.simulate(&spec), Err(OcError::UnknownPoint(_))));
    assert!(
        eng.outputs().is_empty() || eng.state.values[2].bit_eq(&Value::Real(0.0)),
        "no tick ran, so the output is still at its initial value"
    );
}

// ---- placement: the resolution sits after simulate's prev_t reset ----

#[test]
fn a_backwards_tick_still_succeeds_after_a_failed_constant_staging() {
    // THE PLACEMENT PIN. `simulate` clears `prev_t` (the fresh-time-axis reset, R-SIM-2) and only
    // then resolves the Constant names. Move that resolution above the reset and this test reds
    // with `Err(OcError::TimeRegression)`: the failed run would return with the prior tick's
    // `prev_t` still in place, and the backwards tick below would be refused.
    let mut eng = loaded(free_add_model());
    eng.tick(10.0).expect("a forward tick sets prev_t");
    let spec = constant_spec(
        20.0,
        vec![("nope".to_string(), Value::Real(1.0))],
        CollectSpec::None,
    );
    assert!(matches!(eng.simulate(&spec), Err(OcError::UnknownPoint(_))));
    assert!(
        eng.tick(5.0).is_ok(),
        "prev_t must have been cleared before the staging failure, so a backwards tick is allowed"
    );
}

#[test]
fn a_failed_collect_still_refuses_a_backwards_tick() {
    // The control for the pin above, and the asymmetry that makes it worth pinning: collect
    // resolution runs *before* the `prev_t` reset, so a failed collect leaves `prev_t` intact and
    // the same backwards tick is refused. The two paths legitimately sit on opposite sides of the
    // reset; this test is what stops someone "fixing" the asymmetry by moving either one.
    let mut eng = loaded(free_add_model());
    eng.tick(10.0).expect("a forward tick sets prev_t");
    let spec = SimSpec {
        t_start: 0.0,
        t_stop: 20.0,
        step: 1.0,
        inputs: InputSource::None,
        collect: named(&["nope"]),
    };
    assert!(matches!(eng.simulate(&spec), Err(OcError::UnknownPoint(_))));
    assert!(
        matches!(eng.tick(5.0), Err(OcError::TimeRegression { .. })),
        "a failed collect leaves prev_t intact, so the backwards tick is still refused"
    );
}

// ---- fan-out: every connector a name resolves to is written ----

#[test]
fn a_fan_out_name_stages_every_connector_it_resolves_to() {
    let mut eng = loaded(fan_out_add_model("urn:point#u"));
    assert_eq!(
        eng.io.resolve_inputs("urn:point#u").map(<[_]>::len),
        Some(2),
        "the shared @id must fan out to both Add inputs, else this test proves nothing"
    );
    let spec = constant_spec(
        1.0,
        vec![("urn:point#u".to_string(), Value::Real(4.0))],
        named(&["conn#2"]),
    );
    let metrics = eng
        .simulate(&spec)
        .expect("the fan-out name stages cleanly");
    for cid in [0_usize, 1] {
        assert!(
            eng.state.values[cid].bit_eq(&Value::Real(4.0)),
            "connector {cid} must be staged, got {:?}",
            eng.state.values[cid]
        );
    }
    // The load-bearing assertion: `Add` sums both inputs, so writing only the first connector
    // yields 4.0 here instead of 8.0.
    let col = metrics.trace.column(0).expect("one recorded column");
    assert!(
        col.iter().all(|v| v.bit_eq(&Value::Real(8.0))),
        "both fanned-out connectors must carry the staged value every step, got {col:?}"
    );
}

#[test]
fn a_type_heterogeneous_fan_out_is_refused_at_load_not_at_staging() {
    // Why the resolver's type check runs over every connector of a name and yet cannot be shown to
    // bite past the first: no loadable model has a name whose fan-out targets differ in type. The
    // projection admits a repeated point key only when both rows are external inputs of the same
    // direction *and the same value type*, and it runs on every load path, so a `Real`/`Boolean`
    // fan-out never survives to be staged. The check stays as defense in depth matching
    // `set_input`'s shape; no test can give it teeth, and this pins the reason.
    let mut eng = Engine::in_memory();
    let err = eng
        .build_model_in_memory(mixed_type_fan_out_model("urn:point#u"), None)
        .expect_err("a type-heterogeneous fan-out must not load");
    let detail = format!("{err}");
    assert!(
        detail.contains("duplicate point DomainKey") && detail.contains("urn:point#u"),
        "the refusal must name the colliding key, got {detail}"
    );
}

#[test]
fn every_fan_out_that_survives_load_is_type_homogeneous() {
    // The positive half of the same invariant, over the shape staging actually sees: a fan-out that
    // loads has every target at one value type, so one type check per name would have sufficed.
    let eng = loaded(fan_out_add_model("urn:point#u"));
    let targets = eng
        .io
        .resolve_inputs("urn:point#u")
        .expect("the shared @id resolves");
    assert!(targets.len() > 1, "this must be a real fan-out");
    let types: Vec<_> = targets
        .iter()
        .map(|c| eng.model.connectors[c.0 as usize].value_type)
        .collect();
    assert!(
        types.windows(2).all(|w| w[0] == w[1]),
        "a loaded fan-out is type-homogeneous, got {types:?}"
    );
}

// ---- ordering within one list ----

#[test]
fn duplicate_constant_names_stay_last_wins() {
    let mut eng = loaded(free_add_model());
    let spec = constant_spec(
        1.0,
        vec![
            ("conn#0".to_string(), Value::Real(1.0)),
            ("conn#1".to_string(), Value::Real(0.0)),
            ("conn#0".to_string(), Value::Real(9.0)),
        ],
        named(&["conn#2"]),
    );
    let metrics = eng.simulate(&spec).expect("duplicate names stage cleanly");
    assert!(
        eng.state.values[0].bit_eq(&Value::Real(9.0)),
        "the later pair wins, got {:?}",
        eng.state.values[0]
    );
    let col = metrics.trace.column(0).expect("one recorded column");
    assert!(
        col.iter().all(|v| v.bit_eq(&Value::Real(9.0))),
        "the output reflects the last value written for the duplicated name, got {col:?}"
    );
}

// ---- equivalence with the per-name staging it replaces ----

#[test]
fn constant_staging_matches_per_name_set_input_bit_exactly_over_a_horizon() {
    // The differential oracle for the happy path: a `Constant` run against a hand-driven run that
    // calls the untouched public `set_input` for each pair before each tick. Same values, same
    // times, bit for bit.
    let pairs = [
        ("conn#0".to_string(), Value::Real(1.5)),
        ("conn#1".to_string(), Value::Real(-2.25)),
    ];
    let mut simulated = loaded(free_add_model());
    let metrics = simulated
        .simulate(&constant_spec(
            5.0,
            pairs.to_vec(),
            named(&["conn#2", "conn#2"]),
        ))
        .expect("the Constant run must succeed");

    let mut driven = loaded(free_add_model());
    let mut want_times = Vec::new();
    let mut want_values = Vec::new();
    for k in 0..=5 {
        let t = k as f64;
        for (name, value) in &pairs {
            driven.set_input(name, value.clone()).expect("set_input");
        }
        driven.tick(t).expect("tick");
        want_times.push(t);
        want_values.push(driven.get_output("conn#2").expect("conn#2"));
    }

    assert_eq!(metrics.ticks, 6, "six grid points over [0, 5] at step 1");
    assert_eq!(metrics.trace.times(), want_times.as_slice());
    for j in 0..metrics.trace.columns().len() {
        let col = metrics.trace.column(j).expect("column in range");
        assert_eq!(col.len(), want_values.len(), "column {j} row count");
        for (i, (got, want)) in col.iter().zip(&want_values).enumerate() {
            assert!(
                got.bit_eq(want),
                "column {j} row {i}: Constant staging gave {got:?}, set_input gave {want:?}"
            );
        }
    }
}

#[test]
fn a_constant_run_is_bit_reproducible_across_engines() {
    let run = || {
        let mut eng = loaded(free_add_model());
        let metrics = eng
            .simulate(&constant_spec(
                4.0,
                vec![
                    ("conn#0".to_string(), Value::Real(0.1)),
                    ("conn#1".to_string(), Value::Real(0.2)),
                ],
                named(&["conn#2"]),
            ))
            .expect("run");
        metrics
            .trace
            .column(0)
            .expect("one column")
            .iter()
            .map(|v| match v {
                Value::Real(r) => r.to_bits(),
                other => panic!("conn#2 is Real, got {other:?}"),
            })
            .collect::<Vec<u64>>()
    };
    assert_eq!(run(), run(), "identical spec ⇒ identical bits (R-SIM-2)");
}

// ---- out of scope: Closure keeps resolving per step ----

#[test]
fn closure_inputs_still_resolve_at_every_step() {
    // A closure may name *different* points at different `t`, so its names cannot be pre-resolved
    // and it still goes through `set_input` per step. This model has two independent inputs; the
    // closure stages only one of them at a time, and switches which one halfway through.
    let mut eng = loaded(free_add_model());
    let spec = SimSpec {
        t_start: 0.0,
        t_stop: 3.0,
        step: 1.0,
        inputs: InputSource::Closure(Box::new(|t| {
            if t < 1.5 {
                vec![("conn#0".to_string(), Value::Real(2.0))]
            } else {
                vec![("conn#1".to_string(), Value::Real(5.0))]
            }
        })),
        collect: named(&["conn#2"]),
    };
    let metrics = eng.simulate(&spec).expect("the closure run must succeed");
    let col = metrics.trace.column(0).expect("one recorded column");
    // conn#0 is staged for t ∈ {0, 1} (sum 2.0), then conn#1 joins it for t ∈ {2, 3} (sum 7.0) —
    // conn#0 keeps its last staged value because nothing overwrites it.
    let want = [2.0, 2.0, 7.0, 7.0];
    assert_eq!(col.len(), want.len());
    for (i, (got, w)) in col.iter().zip(want).enumerate() {
        assert!(
            got.bit_eq(&Value::Real(w)),
            "row {i}: closure staging gave {got:?}, want {w}"
        );
    }
}

#[test]
fn a_closure_naming_an_unknown_point_still_refuses_mid_run() {
    // Unchanged from before the pre-resolution: a closure's names are only known at `t`, so its
    // refusal necessarily happens mid-run, after earlier steps have already ticked.
    let mut eng = loaded(free_add_model());
    let spec = SimSpec {
        t_start: 0.0,
        t_stop: 5.0,
        step: 1.0,
        inputs: InputSource::Closure(Box::new(|t| {
            let name = if t < 2.5 { "conn#0" } else { "nope" };
            vec![(name.to_string(), Value::Real(1.0))]
        })),
        collect: CollectSpec::None,
    };
    assert!(matches!(eng.simulate(&spec), Err(OcError::UnknownPoint(p)) if p == "nope"));
    assert!(
        eng.state.values[0].bit_eq(&Value::Real(1.0)),
        "the steps before the bad one did run and did stage, got {:?}",
        eng.state.values[0]
    );
}

#[test]
fn an_empty_constant_list_stages_nothing_and_still_runs() {
    let mut eng = loaded(free_add_model());
    let metrics = eng
        .simulate(&constant_spec(2.0, Vec::new(), named(&["conn#2"])))
        .expect("an empty Constant list is a valid no-op source");
    assert_eq!(metrics.ticks, 3);
    let col = metrics.trace.column(0).expect("one recorded column");
    assert!(
        col.iter().all(|v| v.bit_eq(&Value::Real(0.0))),
        "nothing staged, so the Add sums its initial inputs, got {col:?}"
    );
}
