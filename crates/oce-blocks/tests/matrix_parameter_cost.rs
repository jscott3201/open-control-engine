//! The cost of building a `CDL.Reals.MatrixGain` whose gain matrix `K` fills its shape (GitHub
//! #643), in allocations, never in elapsed time. The constructor reads `K`'s cells in one pass
//! over the parameter table; it used to build each cell's name `K_<row>_<col>` and scan the whole
//! table for it, one allocation and one table scan per cell, so a block of `nout × nin` cells
//! cost that many allocations and on the order of its square in comparisons. A 1,041-byte CXF
//! document whose `K` broadcasts one value over 256 × 256 cells, with no ports declared, took
//! 2.7 s in release to be refused.
//!
//! The block is built at two shapes with every cell present and in its shape, the worst case the
//! per-cell reading had. The count is the constructor's, with its block: it must not grow with
//! the cells.

use allocation_counter::{AllocationInfo, measure};
use oce_blocks::lookup;
use oce_model::{ParamTable, Value};
use std::sync::Arc;

/// `nout = nin = n`, then every cell `K_<row>_<col>` bound to a distinct Real.
fn full_table(n: usize) -> ParamTable {
    let width = i64::try_from(n).expect("fits");
    let mut values: Vec<(Arc<str>, Value)> = vec![
        (Arc::from("nout"), Value::Integer(width)),
        (Arc::from("nin"), Value::Integer(width)),
    ];
    for row in 1..=n {
        for col in 1..=n {
            let gain = (row * n + col) as f64;
            values.push((Arc::from(format!("K_{row}_{col}")), Value::Real(gain)));
        }
    }
    ParamTable { values }
}

fn built(params: &ParamTable) -> AllocationInfo {
    let entry = lookup("CDL.Reals.MatrixGain").expect("a registered class");
    measure(|| drop((entry.make)(params)))
}

#[test]
fn building_a_matrix_gain_allocates_nothing_per_cell() {
    // At most 64 allocations and 32 bytes of peak per cell, at 64 x 64 (4,096 cells) and
    // 256 x 256 (65,536) [measured: 11 and 13 allocations, a peak of 20 bytes per cell at both
    // shapes. The per-cell reading made 7,138 and 129,392 allocations, with the same peak].
    for n in [64_usize, 256] {
        let params = full_table(n);
        let info = built(&params);
        assert!(info.count_total <= 64, "{n}x{n}: {info:?}");
        let bytes = u64::try_from(32 * n * n).expect("fits");
        assert!(info.bytes_max <= bytes, "{n}x{n}: {info:?}");
    }
}
