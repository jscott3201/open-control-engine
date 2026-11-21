//! Every registered class reports the heap its constructed block keeps alive
//! (`Block::constructed_words`, issue #700).
//!
//! A loader of untrusted parameters sums that report over a model and refuses past
//! `oce_graph::MAX_CONSTRUCTED_WORDS`, so a class that holds more than it reports is the defect
//! the budget exists to close. These tests measure, with `allocation_counter`, what each
//! registry constructor leaves allocated and compare it with the report:
//!
//! * never more held than reported, for every class under the seeded hostile parameter tables
//!   of `hostile_parameters.rs`, and no less than reported either, up to the rounding of each
//!   buffer to whole words;
//! * the peak of one construction never above `MAX_CONSTRUCTION_PEAK_BYTES`, the per-block cap a
//!   loader's memory bound adds to its budget;
//! * the audit as a list: exactly the classes named here own a buffer, and each is pinned at
//!   its widest shape.
//!
//! Byte counts are bytes requested of the allocator (`bytes_current`, `bytes_max`), measured on
//! a 64-bit host, where a reported word is eight held bytes.

use std::sync::Arc;

use allocation_counter::{AllocationInfo, measure};
use oce_blocks::{
    Block, BlockKind, BlockSignature, MAX_CONSTRUCTION_PEAK_BYTES, MAX_RESOLVED_PORT_WIDTH,
    PortKind, catalog, lookup,
};
use oce_model::{ParamTable, Value};

#[allow(
    dead_code,
    reason = "this binary uses part of the support the parameter fuzz shares"
)]
mod support;

use support::hostile_tables::{XorShift, env_u64, hostile_tables_sized_by_index};

/// Every class whose block owns a buffer sized by its parameters: the audit of issue #700. A
/// class outside this list must report zero and hold nothing but its own value, for every table.
const BUFFER_OWNING_CLASSES: [&str; 29] = [
    "CDL.Integers.MultiSum",
    "CDL.Integers.Sources.TimeTable",
    "CDL.Logical.MultiAnd",
    "CDL.Logical.MultiOr",
    "CDL.Logical.Sources.TimeTable",
    "CDL.Reals.MatrixGain",
    "CDL.Reals.MatrixMax",
    "CDL.Reals.MatrixMin",
    "CDL.Reals.MultiMax",
    "CDL.Reals.MultiMin",
    "CDL.Reals.MultiSum",
    "CDL.Reals.Sort",
    "CDL.Reals.Sources.TimeTable",
    "CDL.Routing.BooleanExtractSignal",
    "CDL.Routing.BooleanExtractor",
    "CDL.Routing.BooleanScalarReplicator",
    "CDL.Routing.BooleanVectorFilter",
    "CDL.Routing.BooleanVectorReplicator",
    "CDL.Routing.IntegerExtractSignal",
    "CDL.Routing.IntegerExtractor",
    "CDL.Routing.IntegerScalarReplicator",
    "CDL.Routing.IntegerVectorFilter",
    "CDL.Routing.IntegerVectorReplicator",
    "CDL.Routing.RealExtractSignal",
    "CDL.Routing.RealExtractor",
    "CDL.Routing.RealScalarReplicator",
    "CDL.Routing.RealVectorFilter",
    "CDL.Routing.RealVectorReplicator",
    "CDL.Utilities.Assert",
];

/// The one class whose buffer can be the parameter table's: a bound `message` is the table's
/// own `Arc<str>`, which the block did not allocate and does not report beyond the two words a
/// default message's reference counts take.
const SHARES_A_PARAMETER_BUFFER: &str = "CDL.Utilities.Assert";

const WORD_BYTES: i64 = 8;

/// One measured construction: the block and what constructing it allocated.
struct Constructed {
    block: Box<dyn Block>,
    info: AllocationInfo,
}

fn construct(class_path: &str, params: &ParamTable) -> Constructed {
    let entry = lookup(class_path).expect("a registered class");
    let mut block = None;
    let info = measure(|| block = Some((entry.make)(params)));
    Constructed {
        block: block.expect("constructed"),
        info,
    }
}

/// The bytes a constructed block keeps alive beyond its own value, and in how many buffers.
fn held_beyond_own_value(block: &dyn Block, info: &AllocationInfo) -> (i64, i64) {
    let own = i64::try_from(size_of_val(block)).expect("fits");
    (
        info.bytes_current - own,
        info.count_current - i64::from(own > 0),
    )
}

/// The report covers what is held: never fewer words than the held bytes need, and (unless a
/// buffer is shared with the parameter table) no more than those bytes rounded up per buffer.
fn report_matches_what_is_held(
    block: &dyn Block,
    info: &AllocationInfo,
    may_share: bool,
) -> Result<(), String> {
    let (held, buffers) = held_beyond_own_value(block, info);
    let reported = WORD_BYTES * i64::try_from(block.constructed_words()).expect("fits");
    if held > reported {
        return Err(format!(
            "holds {held} bytes in {buffers} buffer(s) but reports {reported}"
        ));
    }
    if !may_share && reported - (WORD_BYTES - 1) * buffers > held {
        return Err(format!(
            "reports {reported} bytes but holds {held} in {buffers} buffer(s)"
        ));
    }
    Ok(())
}

#[test]
fn every_class_reports_what_its_block_holds_under_hostile_parameter_tables() {
    let positive = measure(|| drop(std::hint::black_box(vec![0_u8; 64])));
    assert_eq!(positive.count_total, 1, "the counter observes allocations");

    let cases = env_u64("OCE_PARAM_FUZZ_CASES", 200);
    let mut rng = XorShift(env_u64("OCE_PARAM_FUZZ_SEED", 0x0CEB_0700_C0DE_B10C).max(1));
    let mut failures = Vec::new();
    let mut largest_peak = 0u64;
    let mut reporting: Vec<&str> = Vec::new();
    for entry in catalog() {
        let owns = BUFFER_OWNING_CLASSES.contains(&entry.class_path);
        let may_share = entry.class_path == SHARES_A_PARAMETER_BUFFER;
        let mut reported_any = false;
        for params in &hostile_tables_sized_by_index(entry.class_path, cases, &mut rng) {
            let Constructed { block, info } = construct(entry.class_path, params);
            largest_peak = largest_peak.max(info.bytes_max);
            reported_any |= block.constructed_words() > 0;
            let mut defect = report_matches_what_is_held(block.as_ref(), &info, may_share).err();
            if !owns && block.constructed_words() != 0 {
                defect = Some("reports words but is not listed as owning a buffer".to_owned());
            }
            if info.bytes_max > MAX_CONSTRUCTION_PEAK_BYTES as u64 {
                defect = Some(format!("construction peaked at {} bytes", info.bytes_max));
            }
            if let Some(defect) = defect {
                failures.push(format!(
                    "{}: {defect}: {:?}",
                    entry.class_path, params.values
                ));
            }
        }
        if reported_any {
            reporting.push(entry.class_path);
        }
    }
    assert!(
        failures.is_empty(),
        "{} table(s) misreported; first: {:?}",
        failures.len(),
        failures.first()
    );
    reporting.sort_unstable();
    assert_eq!(
        reporting, BUFFER_OWNING_CLASSES,
        "the classes that reported a buffer under the fuzz are the audited ones"
    );
    assert!(
        largest_peak > 8 * 1024 * 1024,
        "the fuzz reaches full-width constructions: largest peak {largest_peak}"
    );
}

fn integers(values: &[(&str, i64)]) -> ParamTable {
    ParamTable {
        values: values
            .iter()
            .map(|(name, value)| (Arc::from(*name), Value::Integer(*value)))
            .collect(),
    }
}

/// The registered spelling of a routing class, as a `'static` path.
fn routing_class(kind: &str, name: &str) -> &'static str {
    let path = format!("CDL.Routing.{kind}{name}");
    catalog()
        .iter()
        .map(|entry| entry.class_path)
        .find(|registered| *registered == path)
        .expect("a registered routing class")
}

/// The widest shape of every buffer-owning class: its parameters and the words it must report.
/// `W` is `MAX_RESOLVED_PORT_WIDTH` (2^20); a port-kind buffer of `n` ports is `n / 8` words, a
/// Real, Integer or index buffer one word per element.
fn widest_shapes() -> Vec<(&'static str, ParamTable, usize)> {
    const W: usize = MAX_RESOLVED_PORT_WIDTH;
    let w = i64::try_from(W).expect("fits");
    let kinds = W / 8;
    let mut shapes = vec![
        ("CDL.Logical.MultiAnd", integers(&[("nin", w)]), kinds),
        ("CDL.Logical.MultiOr", integers(&[("nin", w)]), kinds),
        ("CDL.Reals.MultiMax", integers(&[("nin", w)]), kinds),
        ("CDL.Reals.MultiMin", integers(&[("nin", w)]), kinds),
        ("CDL.Reals.MultiSum", integers(&[("nin", w)]), kinds + W),
        ("CDL.Integers.MultiSum", integers(&[("nin", w)]), kinds + W),
        // A dense square matrix: 1024 inputs, 1024 outputs, 2^20 gains.
        (
            "CDL.Reals.MatrixGain",
            integers(&[("nin", 1024), ("nout", 1024)]),
            2 * 128 + W,
        ),
        // The widest: one row of 2^20 gains.
        (
            "CDL.Reals.MatrixGain",
            integers(&[("nin", w), ("nout", 1)]),
            kinds + 1 + W,
        ),
        (
            "CDL.Reals.MatrixMax",
            integers(&[("nRow", 1024), ("nCol", 1024)]),
            kinds + 128,
        ),
        (
            "CDL.Reals.MatrixMin",
            integers(&[("nRow", 1024), ("nCol", 1024)]),
            kinds + 128,
        ),
        // `y[nin]` and `yIdx[nin]` after `u[nin]`.
        ("CDL.Reals.Sort", integers(&[("nin", w)]), 3 * kinds),
        // The time column and one value column of 2^19 rows, one offset, one output kind.
        (
            "CDL.Reals.Sources.TimeTable",
            integers(&[("table_524288_2", 0)]),
            W + 2,
        ),
        // The widest block of all: one row, 2^20 - 1 output columns, as many offsets and kinds.
        (
            "CDL.Reals.Sources.TimeTable",
            integers(&[("table_1_1048576", 0)]),
            1 + 2 * (W - 1) + kinds,
        ),
        (
            "CDL.Integers.Sources.TimeTable",
            integers(&[("table_1_1048576", 0)]),
            1 + (W - 1) + kinds,
        ),
        // The Boolean table keeps the Integer table and its own output kinds.
        (
            "CDL.Logical.Sources.TimeTable",
            integers(&[("table_1_1048576", 0)]),
            1 + (W - 1) + 2 * kinds,
        ),
        // The fallback message: two reference counts and no bytes.
        ("CDL.Utilities.Assert", integers(&[]), 2),
    ];
    for kind in ["Boolean", "Integer", "Real"] {
        let both = integers(&[("nin", w), ("nout", w)]);
        shapes.extend([
            (
                routing_class(kind, "ExtractSignal"),
                both.clone(),
                2 * kinds + W,
            ),
            (
                routing_class(kind, "Extractor"),
                integers(&[("nin", w)]),
                kinds + 1,
            ),
            (
                routing_class(kind, "ScalarReplicator"),
                integers(&[("nout", w)]),
                kinds,
            ),
            (routing_class(kind, "VectorFilter"), both, 2 * kinds + W),
            (
                routing_class(kind, "VectorReplicator"),
                integers(&[("nin", 1024), ("nout", 1024)]),
                128 + kinds,
            ),
        ]);
    }
    shapes
}

#[test]
fn the_widest_shape_of_every_buffer_owning_class_reports_its_pinned_words_within_the_peak_cap() {
    let shapes = widest_shapes();
    let mut covered: Vec<&str> = shapes.iter().map(|(class, _, _)| *class).collect();
    covered.sort_unstable();
    covered.dedup();
    assert_eq!(
        covered, BUFFER_OWNING_CLASSES,
        "one shape per audited class"
    );

    let mut widest_report = 0usize;
    let mut largest_peak = 0u64;
    for (class, params, words) in &shapes {
        let Constructed { block, info } = construct(class, params);
        assert_eq!(block.constructed_words(), *words, "{class} {params:?}");
        report_matches_what_is_held(block.as_ref(), &info, false)
            .unwrap_or_else(|defect| panic!("{class} {params:?}: {defect}"));
        assert!(
            info.bytes_max <= MAX_CONSTRUCTION_PEAK_BYTES as u64,
            "{class} {params:?}: construction peaked at {} bytes",
            info.bytes_max
        );
        widest_report = widest_report.max(*words);
        largest_peak = largest_peak.max(info.bytes_max);
    }
    // The widest report is what `oce_graph::MAX_CONSTRUCTED_WORDS` documents for one block, and
    // the largest peak (a selector vector read, then copied to its kept buffer, beside two
    // port-kind buffers) is 18 MiB, well inside the 32 MiB cap.
    assert_eq!(widest_report, 2_228_223);
    assert_eq!(MAX_CONSTRUCTION_PEAK_BYTES, 32 * 1024 * 1024);
    assert!(
        (17 * 1024 * 1024..=20 * 1024 * 1024).contains(&largest_peak),
        "largest construction peak {largest_peak}"
    );
}

#[test]
fn a_shape_past_the_port_width_holds_nothing_instead_of_more() {
    let past = i64::try_from(MAX_RESOLVED_PORT_WIDTH).expect("fits") + 1;
    for (class, params) in [
        // 2^19 + 1 rows of two columns is one cell pair past the width: the table falls back.
        (
            "CDL.Reals.Sources.TimeTable",
            integers(&[("table_524289_2", 0)]),
        ),
        (
            "CDL.Reals.Sources.TimeTable",
            integers(&[("table_1048577_1048577", 0)]),
        ),
        (
            "CDL.Reals.MatrixGain",
            integers(&[("nin", 1024), ("nout", 1025)]),
        ),
        (
            "CDL.Reals.MatrixGain",
            integers(&[("nin", i64::MAX), ("nout", i64::MAX)]),
        ),
        (
            "CDL.Reals.MatrixMax",
            integers(&[("nRow", past), ("nCol", past)]),
        ),
    ] {
        let Constructed { block, info } = construct(class, &params);
        assert!(
            block.constructed_words() <= 4,
            "{class} {params:?} reports {}",
            block.constructed_words()
        );
        report_matches_what_is_held(block.as_ref(), &info, false)
            .unwrap_or_else(|defect| panic!("{class} {params:?}: {defect}"));
        assert!(info.bytes_max < 4096, "{class} {params:?}: {info:?}");
    }
}

#[test]
fn a_bound_message_stays_the_parameter_tables_and_is_not_charged_to_the_block() {
    // The message of the model is in its parameter table, whose bytes a loader has already
    // bounded by its input: the block shares that string and allocates nothing for it.
    let message: Arc<str> = Arc::from("x".repeat(100));
    let params = ParamTable {
        values: vec![(Arc::from("message"), Value::String(Arc::clone(&message)))],
    };
    let Constructed { block, info } = construct("CDL.Utilities.Assert", &params);
    assert_eq!(block.constructed_words(), 2);
    assert_eq!(held_beyond_own_value(block.as_ref(), &info), (0, 0));
    assert_eq!(Arc::strong_count(&message), 3, "table, block and this test");

    // With no message bound the block allocates its own empty one: two reference counts.
    let Constructed { block, info } = construct("CDL.Utilities.Assert", &ParamTable::default());
    assert_eq!(block.constructed_words(), 2);
    assert_eq!(held_beyond_own_value(block.as_ref(), &info), (16, 1));
}

/// A block that owns a buffer and reports none of it: the defect the check must catch.
struct HoldsWithoutReporting {
    _buffer: Vec<u64>,
}

impl Block for HoldsWithoutReporting {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "Test.HoldsWithoutReporting",
            inputs: &[],
            outputs: &[PortKind::Real],
            stateful: false,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }
}

/// A block that owns nothing and reports a buffer: the opposite defect.
struct ReportsWithoutHolding;

impl Block for ReportsWithoutHolding {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "Test.ReportsWithoutHolding",
            inputs: &[],
            outputs: &[PortKind::Real],
            stateful: false,
        };
        &SIG
    }

    fn constructed_words(&self) -> usize {
        8
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }
}

#[test]
fn a_block_that_holds_more_or_less_than_it_reports_fails_the_check() {
    let mut block: Option<Box<dyn Block>> = None;
    let info = measure(|| {
        block = Some(Box::new(HoldsWithoutReporting {
            _buffer: vec![0; 1024],
        }));
    });
    let block = block.expect("constructed");
    assert_eq!(
        block.constructed_words(),
        0,
        "the trait default reports nothing"
    );
    let defect = report_matches_what_is_held(block.as_ref(), &info, false)
        .expect_err("an unreported buffer is a defect");
    assert_eq!(defect, "holds 8192 bytes in 1 buffer(s) but reports 0");

    let mut block: Option<Box<dyn Block>> = None;
    let info = measure(|| block = Some(Box::new(ReportsWithoutHolding)));
    let block = block.expect("constructed");
    let defect = report_matches_what_is_held(block.as_ref(), &info, false)
        .expect_err("a report of a buffer that is not held is a defect");
    assert_eq!(defect, "reports 64 bytes but holds 0 in 0 buffer(s)");
    // The same report passes only for the class that may share its buffer with the table.
    assert_eq!(
        report_matches_what_is_held(block.as_ref(), &info, true),
        Ok(())
    );
}
