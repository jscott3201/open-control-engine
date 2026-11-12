//! The cost of the load gate's vector rules (GitHub #626), in allocations, never in elapsed
//! time. Each rule reads a block's elements in one pass over its parameters and looks each judged
//! index up in an ordered map, building no string per index up to the length; the rules it
//! replaced formatted the name of every index from 1 to the length and scanned the parameters
//! for it, so a 2^20 width cost a million allocations before any element was judged.
//!
//! Measured through `class_ports`, which runs exactly the load gate's parameter rules for one
//! block and, on a refusal, returns before it names any port, so the count is the rules'. Each of
//! the four rules is measured: `RealArrayElements` (`Reals.MultiSum`), `IntegerArrayElements`
//! (`Integers.MultiSum`), `IntegerArrayElementsInRange` (`ExtractSignal`) and
//! `BooleanArrayElements` (`VectorFilter`). The tables at the widest length with a wrong-kind
//! element inside it are refused by the old rules too, which makes the old counts in the
//! brackets comparable. A `VectorFilter` table in those two tests keeps the mask's true-count
//! rule out of their counts: its wrong-kind member stops that rule at index 1, and its table
//! past the length leaves `nout` out (refused as missing), so the rule has no count to check.
//! The true-count rule, one pass since GitHub #634, is pinned on its own at the end.

use allocation_counter::{AllocationInfo, measure};
use oce_api::{ClassPortsError, Value, class_ports};

/// `oce_blocks::MAX_RESOLVED_PORT_WIDTH`, the widest length the load gate accepts.
const WIDTH: i64 = 1 << 20;

fn refused(class: &str, params: &[(&str, Value)]) -> AllocationInfo {
    let mut result = None;
    let info = measure(|| result = Some(class_ports(class, params)));
    assert!(
        matches!(result, Some(Err(ClassPortsError::InvalidParameters { .. }))),
        "{class}: {result:?}"
    );
    info
}

/// The four rules at the widest length: a table with a wrong-kind element inside the length,
/// then one with an element one past it.
fn widest() -> Vec<(&'static str, Vec<(String, Value)>)> {
    let width = |name: &str| (name.to_owned(), Value::Integer(WIDTH));
    let past = |base: &str| format!("{base}_{}", WIDTH + 1);
    vec![
        (
            "CDL.Reals.MultiSum",
            vec![width("nin"), ("k_1".to_owned(), Value::Boolean(true))],
        ),
        (
            "CDL.Integers.MultiSum",
            vec![width("nin"), ("k_1".to_owned(), Value::Real(1.5))],
        ),
        (
            "CDL.Routing.RealExtractSignal",
            vec![
                width("nin"),
                width("nout"),
                ("extract_1".to_owned(), Value::Real(1.5)),
            ],
        ),
        (
            "CDL.Routing.RealVectorFilter",
            vec![
                width("nin"),
                ("nout".to_owned(), Value::Integer(1)),
                ("msk_1".to_owned(), Value::Real(1.5)),
            ],
        ),
        (
            "CDL.Reals.MultiSum",
            vec![width("nin"), (past("k"), Value::Real(1.0))],
        ),
        (
            "CDL.Integers.MultiSum",
            vec![width("nin"), (past("k"), Value::Integer(1))],
        ),
        (
            "CDL.Routing.RealExtractSignal",
            vec![
                width("nin"),
                width("nout"),
                (past("extract"), Value::Integer(1)),
            ],
        ),
        (
            "CDL.Routing.RealVectorFilter",
            vec![width("nin"), (past("msk"), Value::Boolean(true))],
        ),
    ]
}

#[test]
fn judging_a_vector_at_the_widest_length_allocates_nothing_per_index() {
    // The wrong-kind tables [measured: 17, 17, 19 and 21 allocations for the four rules; the
    // per-index rules made 1,097,169, 1,097,169, 2,097,170 and 2,087,173], then one past the
    // length [16, 16, 18 and 21].
    for (class, params) in widest() {
        let params: Vec<(&str, Value)> = params
            .iter()
            .map(|(name, value)| (name.as_str(), value.clone()))
            .collect();
        let info = refused(class, &params);
        assert!(info.count_total <= 64, "{class} {params:?}: {info:?}");
    }
}

#[test]
fn refusing_many_elements_past_the_length_costs_linearly_in_the_elements() {
    // `n` elements past a length of 2, each refused once: at most six allocations and 400 bytes
    // of peak per element, for each rule [measured: 20,502 to 20,507 and 327,706 to 327,711
    // allocations, 5.0 per element; 340 to 348 bytes per element, at both sizes].
    for (class, base, fixed, value) in many() {
        for n in [4_096_u64, 65_536] {
            let names: Vec<String> = (3..n + 3).map(|i| format!("{base}_{i}")).collect();
            let mut params: Vec<(&str, Value)> =
                fixed.iter().map(|(name, v)| (*name, v.clone())).collect();
            params.extend(names.iter().map(|name| (name.as_str(), value.clone())));
            let info = refused(class, &params);
            assert!(info.count_total <= 6 * n + 64, "{class} {n}: {info:?}");
            assert!(info.bytes_max <= 400 * n, "{class} {n}: {info:?}");
        }
    }
}

/// One rule's table of elements past a small length: class, base, the fixed parameters, and an
/// element value.
type Many = (
    &'static str,
    &'static str,
    Vec<(&'static str, Value)>,
    Value,
);

/// The four rules with elements past a small length.
fn many() -> Vec<Many> {
    vec![
        (
            "CDL.Reals.MultiSum",
            "k",
            vec![("nin", Value::Integer(2))],
            Value::Real(1.0),
        ),
        (
            "CDL.Integers.MultiSum",
            "k",
            vec![("nin", Value::Integer(2))],
            Value::Integer(1),
        ),
        (
            "CDL.Routing.RealExtractSignal",
            "extract",
            vec![("nin", Value::Integer(2)), ("nout", Value::Integer(2))],
            Value::Integer(1),
        ),
        (
            "CDL.Routing.RealVectorFilter",
            "msk",
            vec![("nin", Value::Integer(2)), ("nout", Value::Integer(2))],
            Value::Boolean(true),
        ),
    ]
}

#[test]
fn counting_a_masks_true_members_allocates_nothing_per_index() {
    // The `VectorFilter` true-count rule (GitHub #634) at two widths, every member defaulted to
    // `true` against `nout = 0`, so the table is refused for its count before any port is named:
    // at most 64 allocations, and a peak of 16 bytes per index [measured: 18 allocations at both
    // widths, 8 bytes of peak per index for the member table; the per-index rule made 121,090
    // and 2,087,170 allocations].
    for nin in [1_i64 << 16, WIDTH] {
        let info = refused(
            "CDL.Routing.RealVectorFilter",
            &[("nin", Value::Integer(nin)), ("nout", Value::Integer(0))],
        );
        assert!(info.count_total <= 64, "{nin}: {info:?}");
        let bytes = 16 * u64::try_from(nin).expect("fits");
        assert!(info.bytes_max <= bytes, "{nin}: {info:?}");
    }
}
