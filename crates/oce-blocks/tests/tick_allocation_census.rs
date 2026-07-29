//! Registry-wide block tick-allocation census.
//!
//! Each allocator region contains exactly one tick call; construction, parameter forcing, input
//! and state allocation, initialization, warm-up, and output sizing occur outside it. The census
//! intentionally prints its two-arm measurement table for explicit gate evidence.
//!
//! Scope limits: the three `Sources.TimeTable` classes exercise their 1x1 fallback tables because
//! they have no Structural rule, and PID exercises its default controller type. Both paths were
//! verified free of allocation sites when this census was introduced.

#![allow(clippy::print_stdout)] // The work-order capture command requires passing-test evidence.

use std::alloc::System;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use oce_blocks::{Block, BlockKind, Ctx, NoopDiagnostics, ParamRule, PortKind, catalog, lookup};
use oce_model::{ParamTable, Value};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, Stats, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

const SWEEP: [f64; 5] = [1.0, 0.0, -1.0, f64::NAN, 1e300];
const EXPECTED_ALLOCATING: [&str; 0] = [];
const EXPECTED_POSITIVE_CONTROL: [&str; 1] = ["CDL.Reals.Sort"];
const MAX_OUTPUT_ARITY: usize = 65 * 65;

fn force_params(rules: &[ParamRule], width: usize) -> ParamTable {
    #[derive(Clone, Copy)]
    enum ArrayElementKind {
        Boolean,
        Integer,
    }

    let mut values = Vec::new();
    for rule in rules {
        match rule {
            ParamRule::Structural { name } => {
                let is_boolean = rules.iter().any(
                    |candidate| matches!(candidate, ParamRule::Boolean { name: boolean_name } if boolean_name == name),
                );
                let value = if is_boolean {
                    Value::Boolean(true)
                } else {
                    Value::Integer(i64::try_from(width).unwrap())
                };
                values.push((Arc::from(*name), value));
            }
            ParamRule::StructuralArrayElements { base } => {
                let element_kind = rules
                    .iter()
                    .find_map(|candidate| match candidate {
                        ParamRule::BooleanArrayElements {
                            base: element_base, ..
                        } if element_base == base => Some(ArrayElementKind::Boolean),
                        ParamRule::IntegerArrayElementsInRange {
                            base: element_base, ..
                        } if element_base == base => Some(ArrayElementKind::Integer),
                        _ => None,
                    })
                    .unwrap_or(ArrayElementKind::Integer);
                for index in 1..=width {
                    let value = match element_kind {
                        ArrayElementKind::Boolean => Value::Boolean(true),
                        ArrayElementKind::Integer => Value::Integer(i64::try_from(index).unwrap()),
                    };
                    values.push((Arc::from(format!("{base}_{index}")), value));
                }
            }
            _ => {}
        }
    }
    ParamTable { values }
}

fn typed_inputs(block: &dyn Block, value: f64) -> Vec<Value> {
    block
        .resolved_signature()
        .inputs
        .iter()
        .map(|kind| match kind {
            PortKind::Real => Value::Real(value),
            PortKind::Integer => Value::Integer(value as i64),
            PortKind::Boolean => Value::Boolean(value > 0.0),
        })
        .collect()
}

fn tick(block: &dyn Block, params: &ParamTable, value: f64, output: &mut [Value]) -> Stats {
    let inputs = typed_inputs(block, value);
    let mut state = vec![0; block.state_len()];
    block.init_state(&mut state, params);
    let diagnostics = NoopDiagnostics;
    let warmup = Ctx::new(0.0, &diagnostics);
    let measured = Ctx::new(1.0, &diagnostics);
    let output_arity = block.resolved_signature().outputs.len();
    assert!(
        output_arity <= output.len(),
        "{} output arity {output_arity} exceeds census sink",
        block.signature().class_path
    );

    let mut emit = |index, value| output[index] = value;
    match block.kind() {
        BlockKind::Algebraic => block.step_algebraic(&warmup, &inputs, &mut emit),
        BlockKind::Stateful => {
            block.emit_from_state(&warmup, &inputs, &state, &mut emit);
            block.update_state(&warmup, &inputs, &mut state);
        }
    }

    let region = Region::new(GLOBAL);
    match block.kind() {
        BlockKind::Algebraic => block.step_algebraic(&measured, &inputs, &mut emit),
        BlockKind::Stateful => {
            block.emit_from_state(&measured, &inputs, &state, &mut emit);
            block.update_state(&measured, &inputs, &mut state);
        }
    }
    region.change()
}

fn add_stats(total: &mut Stats, measured: Stats) {
    total.allocations += measured.allocations;
    total.deallocations += measured.deallocations;
    total.reallocations += measured.reallocations;
    total.bytes_allocated += measured.bytes_allocated;
    total.bytes_deallocated += measured.bytes_deallocated;
    total.bytes_reallocated += measured.bytes_reallocated;
}

fn assert_no_heap_traffic(stats: Stats, class_path: &str, width: usize, value: f64) {
    assert_eq!(
        stats.allocations, 0,
        "{class_path} width {width} value {value:?} allocations: {stats:?}"
    );
    assert_eq!(
        stats.reallocations, 0,
        "{class_path} width {width} value {value:?} reallocations: {stats:?}"
    );
    assert_eq!(
        stats.deallocations, 0,
        "{class_path} width {width} value {value:?} deallocations: {stats:?}"
    );
    assert_eq!(
        stats.bytes_allocated, 0,
        "{class_path} width {width} value {value:?} allocated bytes: {stats:?}"
    );
    assert_eq!(
        stats.bytes_reallocated, 0,
        "{class_path} width {width} value {value:?} reallocated bytes: {stats:?}"
    );
    assert_eq!(
        stats.bytes_deallocated, 0,
        "{class_path} width {width} value {value:?} deallocated bytes: {stats:?}"
    );
}

fn run_arm(width: usize, expected: &[&str], output: &mut [Value]) -> BTreeMap<&'static str, Stats> {
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    let mut allocating = BTreeMap::new();
    for entry in catalog() {
        let params = force_params(entry.param_rules, width);
        for value in SWEEP {
            let block = (lookup(entry.class_path).unwrap().make)(&params);
            let stats = tick(block.as_ref(), &params, value, output);
            if stats == Stats::default() {
                assert_no_heap_traffic(stats, entry.class_path, width, value);
            } else {
                add_stats(allocating.entry(entry.class_path).or_default(), stats);
            }
        }
    }
    let measured = allocating.keys().copied().collect::<BTreeSet<_>>();
    assert_eq!(
        measured, expected,
        "width {width} allocating-set mismatch; measured={measured:?}, expected={expected:?}"
    );
    allocating
}

#[test]
fn tick_allocation_census() {
    let entries = catalog();
    // Count is a change tripwire; iteration over the catalog below proves enumeration.
    assert_eq!(entries.len(), 136);
    assert!(SWEEP.iter().any(|value| *value <= 0.0));
    assert!(SWEEP.iter().any(|value| value.is_nan()));
    let booleans = SWEEP.map(|value| value > 0.0);
    assert!(booleans.contains(&true) && booleans.contains(&false));
    let integers = SWEEP.map(|value| value as i64);
    assert!(integers.iter().any(|value| *value <= 0));

    for entry in entries {
        if entry
            .param_rules
            .iter()
            .any(|rule| matches!(rule, ParamRule::Structural { .. }))
        {
            let narrow_params = force_params(entry.param_rules, 3);
            let wide_params = force_params(entry.param_rules, 65);
            let narrow = (lookup(entry.class_path).unwrap().make)(&narrow_params);
            let wide = (lookup(entry.class_path).unwrap().make)(&wide_params);
            let narrow_signature = narrow.resolved_signature();
            let wide_signature = wide.resolved_signature();
            let narrow_arity = narrow_signature.inputs.len() + narrow_signature.outputs.len();
            let wide_arity = wide_signature.inputs.len() + wide_signature.outputs.len();
            assert_ne!(
                narrow_arity, wide_arity,
                "{} ignored forced Structural width",
                entry.class_path
            );
        }
    }

    let mut output = vec![Value::Real(0.0); MAX_OUTPUT_ARITY];
    let narrow = run_arm(3, &EXPECTED_ALLOCATING, &mut output);
    let wide = run_arm(65, &EXPECTED_POSITIVE_CONTROL, &mut output);

    println!("arm width=3");
    if narrow.is_empty() {
        println!("(none)");
    }
    for (class_path, stats) in narrow {
        println!(
            "{class_path} -> allocs={} bytes={}",
            stats.allocations, stats.bytes_allocated
        );
    }
    println!("arm width=65");
    if wide.is_empty() {
        println!("(none)");
    }
    for (class_path, stats) in wide {
        println!(
            "{class_path} -> allocs={} bytes={}",
            stats.allocations, stats.bytes_allocated
        );
    }
}
