//! Registry-wide block tick-allocation census.
//!
//! Each current-thread allocator region contains eight consecutive ticks. For every sweep value,
//! each block is measured from its initial tick and again after a warm-up tick; construction,
//! parameter forcing, input and state allocation, initialization, and output sizing occur outside
//! the regions. `allocation-counter` installs this test binary's global allocator, but its counters
//! are local to the measured thread, excluding unrelated process-thread traffic that a
//! process-global meter can misattribute (#231). The printed totals are detection evidence, not
//! production-allocator benchmarks.
//!
//! Scope limits: the three `Sources.TimeTable` classes exercise their 1x1 fallback tables because
//! they have no Structural rule, and PID exercises its default controller type. Both paths were
//! verified free of allocation sites when this census was introduced. Boolean-typed structural
//! parameters are forced to `true`, matching every affected catalog default (`rowMax`, `rowMin`,
//! and `msk_i`); column-wise reductions and non-all-true masks are outside this census, and their
//! branches were verified free of allocation sites. No catalog block delegates work to another
//! thread; this census would not observe allocations on such a worker if that contract changes.

#![allow(clippy::print_stdout)] // The work-order capture command requires passing-test evidence.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use allocation_counter::{AllocationInfo, measure};
use oce_blocks::{
    Block, BlockKind, BlockSignature, Ctx, NoopDiagnostics, ParamRule, PortKind, catalog, lookup,
};
use oce_model::{ParamTable, Value};

const SWEEP: [f64; 8] = [
    1.0,
    0.0,
    -0.0,
    -1.0,
    f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
    1e300,
];
const EXPECTED_ALLOCATING: [&str; 0] = [];
const EXPECTED_POSITIVE_CONTROL: [&str; 1] = ["CDL.Reals.Sort"];
const NARROW_WIDTH: usize = 3;
const WIDE_WIDTH: usize = 65;
/// Long enough to expose short periodic duty cycles and several rounds of amortized growth.
const MEASURED_TICKS: usize = 8;
const WINDOW_MODES: [(bool, &str); 2] = [(false, "initial"), (true, "warmed")];
// Tracks the `nin * nout` maximum of `CDL.Routing.*VectorReplicator` at the wide width.
const MAX_OUTPUT_ARITY: usize = WIDE_WIDTH * WIDE_WIDTH;

struct FirstTickAllocation;

impl Block for FirstTickAllocation {
    fn signature(&self) -> &'static BlockSignature {
        static SIGNATURE: BlockSignature = BlockSignature {
            class_path: "Test.FirstTickAllocation",
            inputs: &[],
            outputs: &[],
            stateful: true,
        };
        &SIGNATURE
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn state_len(&self) -> usize {
        1
    }

    fn update_state(&self, _ctx: &Ctx<'_>, _inputs: &[Value], region: &mut [u64]) {
        if region[0] == 0 {
            let allocated: Vec<u64> = Vec::with_capacity(64);
            std::hint::black_box(&allocated);
            region[0] = 1;
        }
    }
}

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

fn measure_window(ticks: usize, step: &mut dyn FnMut(usize)) -> AllocationInfo {
    measure(|| {
        for index in 0..ticks {
            step(index);
        }
    })
}

fn tick(
    block: &dyn Block,
    params: &ParamTable,
    value: f64,
    output: &mut [Value],
    ticks: usize,
    warm_up: bool,
) -> AllocationInfo {
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

    if warm_up {
        let mut emit = |index, value| output[index] = value;
        match block.kind() {
            BlockKind::Algebraic => block.step_algebraic(&warmup, &inputs, &mut emit),
            BlockKind::Stateful => {
                block.emit_from_state(&warmup, &inputs, &state, &mut emit);
                block.update_state(&warmup, &inputs, &mut state);
            }
        }
    }

    // Model time advances across the window. Holding it at one instant would re-evaluate blocks
    // carrying a prior-time state word at a non-monotonic timestamp.
    let first_time = if warm_up { 1.0 } else { 0.0 };
    let mut step = |index: usize| {
        let measured = Ctx::new(first_time + index as f64, &diagnostics);
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

fn measure_block_windows(
    mut make_block: impl FnMut() -> Box<dyn Block>,
    params: &ParamTable,
    value: f64,
    output: &mut [Value],
) -> [AllocationInfo; 2] {
    WINDOW_MODES.map(|(warm_up, _)| {
        let block = make_block();
        tick(
            block.as_ref(),
            params,
            value,
            output,
            MEASURED_TICKS,
            warm_up,
        )
    })
}

fn render_info(class_path: &str, mode: &str, info: &AllocationInfo) -> String {
    format!(
        "{class_path} ({mode}): allocs={} net_allocs={} peak_allocs_sum={} bytes_alloc={} \
         net_bytes={} peak_bytes_sum={}",
        info.count_total,
        info.count_current,
        info.count_max,
        info.bytes_total,
        info.bytes_current,
        info.bytes_max,
    )
}

fn run_arm(
    width: usize,
    expected: &[&str],
    output: &mut [Value],
) -> BTreeMap<&'static str, [AllocationInfo; 2]> {
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    let mut allocating: BTreeMap<&'static str, [AllocationInfo; 2]> = BTreeMap::new();
    for entry in catalog() {
        let params = force_params(entry.param_rules, width);
        for value in SWEEP {
            let make = lookup(entry.class_path).unwrap().make;
            let windows = measure_block_windows(|| make(&params), &params, value, output);
            for (mode_index, info) in windows.into_iter().enumerate() {
                // All six fields decide purity, so deallocation of memory created before the
                // region remains visible through a negative current count even with zero allocs.
                if info != AllocationInfo::default() {
                    allocating.entry(entry.class_path).or_default()[mode_index] += info;
                }
            }
        }
    }
    let measured = allocating.keys().copied().collect::<BTreeSet<_>>();
    let differences = measured
        .symmetric_difference(&expected)
        .map(|class_path| match allocating.get(class_path) {
            Some(windows) => windows
                .iter()
                .zip(WINDOW_MODES)
                .filter(|(info, _)| **info != AllocationInfo::default())
                .map(|(info, (_, mode))| render_info(class_path, mode, info))
                .collect::<Vec<_>>()
                .join("; "),
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

#[test]
fn periodic_allocations_are_visible_in_the_measured_window() {
    assert_eq!(MEASURED_TICKS, 8, "the census window size is a tripwire");
    let mut periodic = |index: usize| {
        if index % 3 == 2 {
            let grown: Vec<u64> = Vec::with_capacity(64);
            std::hint::black_box(&grown);
        }
    };

    let windowed = measure_window(MEASURED_TICKS, &mut periodic);
    assert_ne!(
        windowed,
        AllocationInfo::default(),
        "the measured window must detect an allocator that runs on one tick in three"
    );

    // This proves the allocator does not fire on tick 0; the full-window assertion above therefore
    // fails if the measured window shrinks below the first allocation at tick 2.
    assert_eq!(
        measure_window(1, &mut periodic),
        AllocationInfo::default(),
        "a one-tick window must miss this allocator; otherwise it does not demonstrate the gap"
    );

    // Control: a genuinely pure step stays clean across the whole window, so the window is not
    // accusing everything put through it.
    let pure = |index: usize| {
        std::hint::black_box(index);
    };
    assert_eq!(
        measure_window(MEASURED_TICKS, &mut { pure }),
        AllocationInfo::default(),
        "a pure step must stay clean across the measured window"
    );
}

#[test]
fn initial_tick_allocations_are_visible_without_warm_up() {
    let params = ParamTable::default();
    let mut output = [];
    let [initial, warmed] =
        measure_block_windows(|| Box::new(FirstTickAllocation), &params, 0.0, &mut output);
    assert_ne!(
        initial,
        AllocationInfo::default(),
        "the initial window must detect an allocation made only on the first tick"
    );
    assert_eq!(
        warmed,
        AllocationInfo::default(),
        "the post-warm-up window must not invent a recurring allocation"
    );
}

#[test]
fn allocations_on_another_thread_do_not_enter_the_measurement() {
    let start = AtomicBool::new(false);
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            while !start.load(Ordering::Acquire) {
                std::hint::spin_loop();
            }
            let grown: Vec<u64> = Vec::with_capacity(64);
            std::hint::black_box(&grown);
            drop(grown);
            done.store(true, Ordering::Release);
        });
        // The flags place the worker allocation strictly inside the current-thread region. The
        // worker body is infallible before setting `done`, so neither spin can outlive the worker.
        let info = measure(|| {
            start.store(true, Ordering::Release);
            while !done.load(Ordering::Acquire) {
                std::hint::spin_loop();
            }
        });
        worker.join().expect("allocation worker");
        assert_eq!(
            info,
            AllocationInfo::default(),
            "the census must not attribute another thread's allocation to the measured block"
        );
    });
}

#[test]
fn tick_allocation_census() {
    let entries = catalog();
    // Count is a change tripwire; iteration over the catalog below proves enumeration.
    assert_eq!(entries.len(), 136);
    assert!(SWEEP.iter().any(|value| *value <= 0.0));
    assert!(SWEEP.iter().any(|value| value.is_nan()));
    assert!(SWEEP.contains(&f64::INFINITY));
    assert!(SWEEP.contains(&f64::NEG_INFINITY));
    assert!(
        SWEEP
            .iter()
            .any(|value| value.to_bits() == (-0.0_f64).to_bits())
    );
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
    for (class_path, windows) in narrow {
        for (info, (_, mode)) in windows.iter().zip(WINDOW_MODES) {
            if *info != AllocationInfo::default() {
                println!("{}", render_info(class_path, mode, info));
            }
        }
    }
    println!("arm width={WIDE_WIDTH}");
    if wide.is_empty() {
        println!("(none)");
    }
    for (class_path, windows) in wide {
        for (info, (_, mode)) in windows.iter().zip(WINDOW_MODES) {
            if *info != AllocationInfo::default() {
                println!("{}", render_info(class_path, mode, info));
            }
        }
    }
}
