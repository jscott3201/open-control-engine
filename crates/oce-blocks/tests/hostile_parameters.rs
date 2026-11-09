//! Hostile parameter tables through every registered constructor (GitHub #400, design note
//! decision 12).
//!
//! A compiled-schedule blob is trusted for semantics but hostile for memory safety: on a target,
//! `oce-validate`'s parameter rules are not linked, so every registry constructor receives the
//! blob's parameter table as written. This fuzz feeds every class seeded random tables — the
//! class's own parameter names (rules, defaults and their flattened `_i` / `_row_col` members)
//! mixed with junk names, carrying values of every kind at their edges (NaN, infinities, signed
//! zero, subnormals, `i64` extremes, widths around [`MAX_RESOLVED_PORT_WIDTH`], unknown enum
//! classes and ordinals, arbitrary strings) — and requires construction and every structural query
//! to return without a panic: the resolved signature, kind, state length and revision, and, while
//! the resolved shape stays small enough to enumerate, `feeds_through` over every port pair,
//! `init_state`, one emit and state update at two times with typed zero inputs, and the
//! contract port names (`resolved_ports`, GitHub #440), which must name every resolved port
//! exactly once with the signature's kind.
//!
//! A seeded xorshift64* generator, no dependency: `OCE_PARAM_FUZZ_CASES` overrides the per-class
//! case count (default 200) and `OCE_PARAM_FUZZ_SEED` the seed.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use oce_blocks::{
    BlockKind, Ctx, MAX_RESOLVED_ARITY, MAX_RESOLVED_PORT_WIDTH, NoopDiagnostics, PortKind,
    catalog, lookup, resolved_ports,
};
use oce_model::{ParamTable, Value};

/// Shapes above these are constructed and queried for signature and state length only.
const MAX_ENUMERATED_PORTS: usize = 256;
const MAX_INITIALIZED_WORDS: usize = 1 << 16;
/// Shapes up to this many ports per side also have their contract names synthesized and checked.
const MAX_NAMED_PORTS: usize = 4096;

mod support;

use support::hostile_tables::{Names, RULE_VARIANTS, XorShift, env_u64, hostile_tables, names_of};

fn zero(kind: PortKind) -> Value {
    match kind {
        PortKind::Real => Value::Real(0.0),
        PortKind::Integer => Value::Integer(0),
        PortKind::Boolean => Value::Boolean(false),
    }
}

/// Construct and query one block; returns nothing, panics on a defect.
fn exercise(class_path: &str, params: &ParamTable) {
    let entry = lookup(class_path).unwrap();
    let block = (entry.make)(params);
    let signature = block.resolved_signature();
    let kind = block.kind();
    let state_len = block.state_len();
    let _ = block.state_contract_revision();
    let _ = block.signature();
    if kind == BlockKind::Algebraic {
        assert_eq!(state_len, 0, "an algebraic block reports state");
    }
    let (inputs, outputs) = (signature.inputs.len(), signature.outputs.len());
    // The compiled-schedule decoder refuses longer port lists before allocating for them.
    assert!(inputs <= MAX_RESOLVED_ARITY && outputs <= MAX_RESOLVED_ARITY);
    if inputs <= MAX_NAMED_PORTS && outputs <= MAX_NAMED_PORTS {
        check_names(class_path, params, &signature.inputs, &signature.outputs);
    }
    if inputs > MAX_ENUMERATED_PORTS
        || outputs > MAX_ENUMERATED_PORTS
        || state_len > MAX_INITIALIZED_WORDS
    {
        return;
    }
    for i in 0..inputs {
        for j in 0..outputs {
            let _ = block.feeds_through(i, j);
        }
    }
    let mut region = vec![0u64; state_len];
    block.init_state(&mut region, params);
    let values: Vec<Value> = signature.inputs.iter().map(|k| zero(*k)).collect();
    let diagnostics = NoopDiagnostics;
    for t in [0.0, 1.0] {
        let _ = block.time_is_representable(t, &region);
        let ctx = Ctx::new(t, &diagnostics);
        let mut emitted = 0usize;
        let mut emit = |port: usize, _value: Value| {
            assert!(port < outputs, "emitted port {port} of {outputs}");
            emitted += 1;
        };
        match kind {
            BlockKind::Algebraic => block.step_algebraic(&ctx, &values, &mut emit),
            BlockKind::Stateful => {
                block.emit_from_state(&ctx, &values, &region, &mut emit);
                block.update_state(&ctx, &values, &mut region);
            }
        }
    }
}

/// The contract names cover the constructed block exactly: one unique name per resolved port, in
/// signature order, with the signature's kind. Any table a constructor accepts must be nameable.
fn check_names(class_path: &str, params: &ParamTable, inputs: &[PortKind], outputs: &[PortKind]) {
    let ports = resolved_ports(class_path, params)
        .unwrap_or_else(|error| panic!("{class_path}: ports not nameable: {error}"));
    for (named, kinds) in [(&ports.inputs, inputs), (&ports.outputs, outputs)] {
        let named_kinds: Vec<PortKind> = named.iter().map(|port| port.kind).collect();
        assert_eq!(
            named_kinds, kinds,
            "{class_path}: names disagree with the signature"
        );
        let mut names: Vec<&str> = named.iter().map(|port| port.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            named.len(),
            "{class_path}: duplicate port name"
        );
    }
}

#[test]
fn every_constructor_survives_hostile_parameter_tables() {
    let cases = env_u64("OCE_PARAM_FUZZ_CASES", 200);
    let mut rng = XorShift(env_u64("OCE_PARAM_FUZZ_SEED", 0x0CEB_0400_7A2A_3500).max(1));
    let mut failures = Vec::new();
    for entry in catalog() {
        let tables = hostile_tables(entry.class_path, cases, &mut rng);
        for params in &tables {
            let outcome = catch_unwind(AssertUnwindSafe(|| exercise(entry.class_path, params)));
            if outcome.is_err() {
                failures.push(format!("{}: {:?}", entry.class_path, params.values));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} hostile table(s) panicked; first: {:?}",
        failures.len(),
        failures.first()
    );
}

#[test]
fn every_rule_variant_is_projected_and_used_by_the_registry() {
    let mut used: Vec<&str> = catalog()
        .iter()
        .flat_map(|entry| entry.param_rules)
        .map(|rule| rule.project::<Names>().0)
        .collect();
    used.sort_unstable();
    used.dedup();
    let mut all = RULE_VARIANTS.to_vec();
    all.sort_unstable();
    assert_eq!(
        used, all,
        "every ParamRule variant the fuzz derives names from is in use"
    );
}

/// The arity bound is the true maximum: `CDL.Reals.Sort` reaches it at `nin` equal to the port
/// width, and every width-driven class stays within it at and past its widest parameters.
#[test]
fn the_resolved_arity_bound_is_reached_and_never_exceeded() {
    let width = MAX_RESOLVED_PORT_WIDTH as i64;
    let sort = (lookup("CDL.Reals.Sort").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("nin"), Value::Integer(width))],
    });
    assert_eq!(sort.resolved_signature().outputs.len(), MAX_RESOLVED_ARITY);
    let mut widest = 0usize;
    for entry in catalog().iter().filter(|entry| entry.width_driven) {
        for n in [width, width + 1, i64::MAX] {
            let values = names_of(entry.class_path)
                .into_iter()
                .filter(|name| !name.contains('_'))
                .map(|name| (Arc::from(name.as_str()), Value::Integer(n)))
                .collect();
            let block = (lookup(entry.class_path).unwrap().make)(&ParamTable { values });
            let signature = block.resolved_signature();
            widest = widest
                .max(signature.inputs.len())
                .max(signature.outputs.len());
        }
    }
    assert_eq!(widest, MAX_RESOLVED_ARITY);
}
