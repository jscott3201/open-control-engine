//! The cost of building a block whose vector parameter fills its length (GitHub #634), in
//! allocations, never in elapsed time. The constructors of `MultiSum` (`k`), `ExtractSignal`
//! (`extract`) and `VectorFilter` (`msk`) read the vector's elements in one pass over the
//! parameter table; they used to build each element's name and scan the whole table for it,
//! one allocation and one table scan per index, so a block of `n` elements cost `n` allocations
//! and on the order of `n` squared comparisons. A 1,440-byte CXF document whose `MultiSum` binds
//! `nin = 65536` and `k = fill(1, 65536)` took 4.1 s to build in release before this.
//!
//! Each class is built at two sizes with every element present and in its length, the worst
//! case the per-index reading had. The count is the constructor's, with its block: it must not
//! grow with `n`.

use allocation_counter::{AllocationInfo, measure};
use oce_blocks::lookup;
use oce_model::{ParamTable, Value};
use std::sync::Arc;

/// A table of `fixed` then `base_1` to `base_n` bound to `element(i)`.
fn table(
    fixed: &[(&str, Value)],
    base: &str,
    n: usize,
    element: impl Fn(usize) -> Value,
) -> ParamTable {
    let mut values: Vec<(Arc<str>, Value)> = fixed
        .iter()
        .map(|(name, value)| (Arc::from(*name), value.clone()))
        .collect();
    values.extend((1..=n).map(|i| (Arc::from(format!("{base}_{i}")), element(i))));
    ParamTable { values }
}

fn built(class: &str, params: &ParamTable) -> AllocationInfo {
    let entry = lookup(class).expect("a registered class");
    measure(|| drop((entry.make)(params)))
}

/// Every class whose constructor reads a vector, with its table at length `n`.
fn tables(n: usize) -> Vec<(String, ParamTable)> {
    let width = i64::try_from(n).expect("fits");
    let mut all = vec![
        (
            "CDL.Reals.MultiSum".to_owned(),
            table(&[("nin", Value::Integer(width))], "k", n, |_| {
                Value::Real(1.0)
            }),
        ),
        (
            "CDL.Integers.MultiSum".to_owned(),
            table(&[("nin", Value::Integer(width))], "k", n, |_| {
                Value::Integer(1)
            }),
        ),
    ];
    for kind in ["Real", "Integer", "Boolean"] {
        all.push((
            format!("CDL.Routing.{kind}ExtractSignal"),
            table(
                &[
                    ("nin", Value::Integer(width)),
                    ("nout", Value::Integer(width)),
                ],
                "extract",
                n,
                |i| Value::Integer(i64::try_from(n + 1 - i).expect("fits")),
            ),
        ));
        all.push((
            format!("CDL.Routing.{kind}VectorFilter"),
            table(
                &[
                    ("nin", Value::Integer(width)),
                    ("nout", Value::Integer(width)),
                ],
                "msk",
                n,
                |_| Value::Boolean(true),
            ),
        ));
    }
    all
}

#[test]
fn building_a_block_allocates_nothing_per_vector_element() {
    // At most 64 allocations and 32 bytes of peak per element, at both sizes [measured: 3 for
    // both `MultiSum` classes, 5 for each `ExtractSignal`, 16 then 20 for each `VectorFilter`
    // (its own index lists grow by doubling); peak 9, 18 and 13 bytes per element. The
    // per-index readers made 8,096, 4,101 and 4,111 allocations at 4,096 elements, with the
    // same peak].
    for n in [4_096_usize, 65_536] {
        for (class, params) in tables(n) {
            let info = built(&class, &params);
            assert!(info.count_total <= 64, "{class} {n}: {info:?}");
            let bytes = u64::try_from(32 * n).expect("fits");
            assert!(info.bytes_max <= bytes, "{class} {n}: {info:?}");
        }
    }
}
