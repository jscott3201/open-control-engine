//! Registry-wide block tick-allocation census.
//!
//! Each allocator region contains exactly one tick call; construction, parameter forcing, input
//! and state allocation, initialization, warm-up, and output sizing occur outside it. The census
//! intentionally prints its two-arm measurement table for explicit gate evidence.
//!
//! Scope limits: the three `Sources.TimeTable` classes exercise their 1x1 fallback tables because
//! they have no Structural rule, and PID exercises its default controller type. Both paths were
//! verified free of allocation sites when this census was introduced. Boolean-typed structural
//! parameters are forced to `true`, matching every affected catalog default (`rowMax`, `rowMin`,
//! and `msk_i`); column-wise reductions and non-all-true masks are outside this census, and their
//! branches were verified free of allocation sites.

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
const NARROW_WIDTH: usize = 3;
const WIDE_WIDTH: usize = 65;
/// Ticks in the confirmation window. Long enough to contain a periodic allocator's duty cycle and
/// several rounds of amortized growth; short enough that an uncorrelated ambient event is still
/// unlikely to land in it after already landing in the first measurement.
const CONFIRM_TICKS: usize = 8;
// Tracks the `nin * nout` maximum of `CDL.Routing.*VectorReplicator` at the wide width.
const MAX_OUTPUT_ARITY: usize = WIDE_WIDTH * WIDE_WIDTH;

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

fn drive_value(kind: PortKind, value: f64) -> Value {
    match kind {
        PortKind::Real => Value::Real(value),
        PortKind::Integer => Value::Integer(value as i64),
        PortKind::Boolean => Value::Boolean(value > 0.0),
    }
}

fn typed_inputs(block: &dyn Block, value: f64) -> Vec<Value> {
    block
        .resolved_signature()
        .inputs
        .iter()
        .map(|kind| drive_value(*kind, value))
        .collect()
}

/// Measure `ticks` consecutive iterations of `step` as a single allocator region.
///
/// The confirmation pass measures a WINDOW rather than one tick. An allocator need not run on every
/// tick: a buffer that grows amortized allocates only when it outgrows capacity, and a flush every
/// Nth tick allocates on one tick in N. A single-tick confirmation dropped exactly those, so the
/// pin went blind to the shape it most needs to catch. Pinned by
/// `a_periodic_allocator_survives_the_confirmation_window`, which also shows a one-tick window
/// missing the same allocator.
fn measure_window(ticks: usize, step: &mut dyn FnMut(usize)) -> Stats {
    let region = Region::new(GLOBAL);
    for index in 0..ticks {
        step(index);
    }
    region.change()
}

fn tick(
    block: &dyn Block,
    params: &ParamTable,
    value: f64,
    output: &mut [Value],
    ticks: usize,
) -> Stats {
    let inputs = typed_inputs(block, value);
    let mut state = vec![0; block.state_len()];
    block.init_state(&mut state, params);
    let diagnostics = NoopDiagnostics;
    let warmup = Ctx::new(0.0, &diagnostics);
    let output_arity = block.resolved_signature().outputs.len();
    assert!(
        output_arity <= output.len(),
        "{} output arity {output_arity} exceeds census sink",
        block.signature().class_path
    );

    {
        let mut emit = |index, value| output[index] = value;
        match block.kind() {
            BlockKind::Algebraic => block.step_algebraic(&warmup, &inputs, &mut emit),
            BlockKind::Stateful => {
                block.emit_from_state(&warmup, &inputs, &state, &mut emit);
                block.update_state(&warmup, &inputs, &mut state);
            }
        }
    }

    // Model time advances across the window. Holding it at one instant would re-evaluate the block
    // at the same `t` repeatedly, which the blocks carrying a prior-time state word reject as
    // non-monotonic; `ticks == 1` still measures exactly `t = 1.0`, as this census always has.
    let mut step = |index: usize| {
        let measured = Ctx::new(1.0 + index as f64, &diagnostics);
        let mut emit = |port, value| output[port] = value;
        match block.kind() {
            BlockKind::Algebraic => block.step_algebraic(&measured, &inputs, &mut emit),
            BlockKind::Stateful => {
                block.emit_from_state(&measured, &inputs, &state, &mut emit);
                block.update_state(&measured, &inputs, &mut state);
            }
        }
    };
    measure_window(ticks, &mut step)
}

fn add_stats(total: &mut Stats, measured: Stats) {
    total.allocations += measured.allocations;
    total.deallocations += measured.deallocations;
    total.reallocations += measured.reallocations;
    total.bytes_allocated += measured.bytes_allocated;
    total.bytes_deallocated += measured.bytes_deallocated;
    total.bytes_reallocated += measured.bytes_reallocated;
}

fn run_arm(width: usize, expected: &[&str], output: &mut [Value]) -> BTreeMap<&'static str, Stats> {
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    let mut allocating = BTreeMap::new();
    for entry in catalog() {
        let params = force_params(entry.param_rules, width);
        for value in SWEEP {
            let block = (lookup(entry.class_path).unwrap().make)(&params);
            let stats = tick(block.as_ref(), &params, value, output, 1);
            // `Stats == default` is the six-field purity check: the type derives both over 6 fields.
            if stats == Stats::default() {
                continue;
            }
            // Confirm before accusing (#231). `GLOBAL` is a `#[global_allocator]`, so the meter is
            // process-global: anything allocating on ANY thread during the window is billed to
            // whichever block happens to be mid-tick. That the attribution is arbitrary rather than
            // real is settled by evidence, not assumed — two CI reds named different blocks with
            // different sizes, and one of them, `CDL.Reals.Line`, is `struct Line { limit_below:
            // bool, limit_above: bool }` in a file containing zero allocation-capable constructs.
            //
            // A second measurement over `CONFIRM_TICKS` separates the two cases. What it rests on is
            // that an ambient event is uncorrelated with this block, so accusing requires it to land
            // twice on the same block and sweep value — not on the block allocating every tick,
            // which is false of amortized growth and of a periodic flush. A one-tick confirmation
            // asserted exactly that and dropped both shapes; `a_periodic_allocator_survives_the_
            // confirmation_window` now pins the difference instead of arguing it.
            //
            // The window is the reason this is not a loosening. `EXPECTED_POSITIVE_CONTROL` still
            // requires `CDL.Reals.Sort` to be DETECTED at the wide width, but that only ever
            // covered a per-tick-persistent allocator; it passed while the retry hid a real one.
            let confirm_block = (lookup(entry.class_path).unwrap().make)(&params);
            let confirmed = tick(
                confirm_block.as_ref(),
                &params,
                value,
                output,
                CONFIRM_TICKS,
            );
            if confirmed == Stats::default() {
                continue;
            }
            add_stats(allocating.entry(entry.class_path).or_default(), confirmed);
        }
    }
    let measured = allocating.keys().copied().collect::<BTreeSet<_>>();
    let differences = measured
        .symmetric_difference(&expected)
        .map(|class_path| match allocating.get(class_path) {
            // All six fields, because all six decide the verdict: the purity check is
            // `stats == Stats::default()`, which compares the whole struct. Printing only the two
            // allocation fields meant an event landing on the dealloc side alone reported
            // `allocs=0 bytes=0`, which reads as a broken harness rather than as evidence.
            Some(stats) => format!(
                "{class_path}: allocs={} deallocs={} reallocs={} bytes_alloc={} \
                 bytes_dealloc={} bytes_realloc={}",
                stats.allocations,
                stats.deallocations,
                stats.reallocations,
                stats.bytes_allocated,
                stats.bytes_deallocated,
                stats.bytes_reallocated
            ),
            None => format!("{class_path}: expected but absent"),
        })
        .collect::<Vec<_>>();
    assert!(
        differences.is_empty(),
        "width {width} allocating-set mismatch; measured={measured:?}, expected={expected:?}; \
         differences={differences:?}"
    );
    allocating
}

/// The confirmation pass must not hide an allocator that skips ticks.
///
/// This is the claim the previous one-tick confirmation asserted in a comment and did not hold: a
/// real allocator was dropped whenever it did not happen to run on the single confirming tick. The
/// `CDL.Reals.Sort` positive control could not catch that — it allocates on every tick, so it
/// survived a retry that was busy discarding the shapes that do not.
#[test]
fn a_periodic_allocator_survives_the_confirmation_window() {
    // One tick in three, the duty cycle a single-tick confirmation drops.
    let periodic = |index: usize| {
        if index.is_multiple_of(3) {
            let grown: Vec<u64> = Vec::with_capacity(64);
            std::hint::black_box(&grown);
        }
    };

    let windowed = measure_window(CONFIRM_TICKS, &mut { periodic });
    assert_ne!(
        windowed,
        Stats::default(),
        "the confirmation window must detect an allocator that runs on one tick in three"
    );

    // Control: the same allocator, offset off tick 0, is invisible to a one-tick window. Without
    // this the test could pass against a confirmation that never widened at all.
    let offset = |index: usize| periodic(index + 1);
    assert_eq!(
        measure_window(1, &mut { offset }),
        Stats::default(),
        "a one-tick window must miss this allocator — otherwise it does not demonstrate the gap"
    );

    // Control: a genuinely pure step stays clean across the whole window, so the window is not
    // simply accusing everything put through it.
    let pure = |index: usize| {
        std::hint::black_box(index);
    };
    assert_eq!(
        measure_window(CONFIRM_TICKS, &mut { pure }),
        Stats::default(),
        "a pure step must stay clean across the confirmation window"
    );
}

#[test]
fn tick_allocation_census() {
    let entries = catalog();
    // Count is a change tripwire; iteration over the catalog below proves enumeration.
    assert_eq!(entries.len(), 136);
    assert!(SWEEP.iter().any(|value| *value <= 0.0));
    assert!(SWEEP.iter().any(|value| value.is_nan()));
    let booleans = SWEEP.map(|value| drive_value(PortKind::Boolean, value));
    assert!(
        booleans
            .iter()
            .any(|value| matches!(value, Value::Boolean(true)))
            && booleans
                .iter()
                .any(|value| matches!(value, Value::Boolean(false)))
    );
    let integers = SWEEP.map(|value| drive_value(PortKind::Integer, value));
    assert!(
        integers
            .iter()
            .any(|value| matches!(value, Value::Integer(integer) if *integer <= 0))
    );

    for entry in entries {
        if entry
            .param_rules
            .iter()
            .any(|rule| matches!(rule, ParamRule::Structural { .. }))
        {
            let narrow_params = force_params(entry.param_rules, NARROW_WIDTH);
            let wide_params = force_params(entry.param_rules, WIDE_WIDTH);
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
    let narrow = run_arm(NARROW_WIDTH, &EXPECTED_ALLOCATING, &mut output);
    let wide = run_arm(WIDE_WIDTH, &EXPECTED_POSITIVE_CONTROL, &mut output);

    println!("arm width={NARROW_WIDTH}");
    if narrow.is_empty() {
        println!("(none)");
    }
    for (class_path, stats) in narrow {
        println!(
            "{class_path} -> allocs={} bytes={}",
            stats.allocations, stats.bytes_allocated
        );
    }
    println!("arm width={WIDE_WIDTH}");
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
