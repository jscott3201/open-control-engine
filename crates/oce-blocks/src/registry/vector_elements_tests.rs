//! `vector_elements` against the per-index reading it replaced (GitHub #634): for every table of
//! up to three bindings drawn from the spellings a hostile document may use, and every length
//! around them, it returns the very binding (by reference) that `find(params, "base_i")` returned
//! for each `i`, so every constructor that reads a vector through it reads the same values.

use alloc::format;
use alloc::sync::Arc;
use alloc::vec::Vec;

use oce_model::{ParamTable, Value};

use super::{bool_or, find, int_or, real_or, vector_elements};

/// The reading `vector_elements` replaced: build each name, scan the table for it.
fn per_index<'a>(params: &'a ParamTable, base: &str, n: usize) -> Vec<Option<&'a Value>> {
    (1..=n)
        .map(|i| find(params, &format!("{base}_{i}")))
        .collect()
}

/// Names an element of `k` or not: canonical indices, index 0, a leading zero, signs, an empty
/// or non-decimal suffix, a matrix spelling, another base and case, and an index too large for
/// `usize`.
const NAMES: [&str; 16] = [
    "k_1",
    "k_2",
    "k_3",
    "k_10",
    "k_0",
    "k_01",
    "k_+1",
    "k_-1",
    "k_",
    "k_x",
    "k_1_2",
    "K_1",
    "k",
    "kk_1",
    "k_\u{0661}",
    "k_340282366920938463463374607431768211456",
];

fn value(i: usize) -> Value {
    match i % 3 {
        0 => Value::Real(i as f64),
        1 => Value::Integer(i as i64),
        _ => Value::Boolean(i.is_multiple_of(2)),
    }
}

#[test]
fn vector_elements_returns_the_binding_every_per_index_lookup_returned() {
    let mut tables = 0;
    for a in 0..NAMES.len() {
        for b in 0..NAMES.len() {
            for c in 0..NAMES.len() {
                let params = ParamTable {
                    values: [a, b, c]
                        .iter()
                        .enumerate()
                        .map(|(slot, &name)| (Arc::from(NAMES[name]), value(slot + name)))
                        .collect(),
                };
                for n in [0, 1, 2, 3, 9, 10, 11] {
                    let ours = vector_elements(&params, "k", n);
                    let theirs = per_index(&params, "k", n);
                    assert_eq!(ours.len(), theirs.len());
                    for (i, (ours, theirs)) in ours.iter().zip(&theirs).enumerate() {
                        let same = match (ours, theirs) {
                            (Some(ours), Some(theirs)) => core::ptr::eq(*ours, *theirs),
                            (None, None) => true,
                            _ => false,
                        };
                        assert!(same, "{params:?} n={n} element {}", i + 1);
                    }
                }
                tables += 1;
            }
        }
    }
    assert_eq!(tables, 16 * 16 * 16);
}

#[test]
fn the_value_readers_are_the_scalar_readers_applied_to_a_binding() {
    // Each kind against each reader, unbound included: a Real reads Integer by promotion, and
    // anything else is the default.
    let bindings = [
        None,
        Some(Value::Real(2.5)),
        Some(Value::Integer(7)),
        Some(Value::Boolean(true)),
        Some(Value::String(Arc::from("x"))),
    ];
    let reals: Vec<u64> = bindings
        .iter()
        .map(|b| real_or(b.as_ref(), -1.0).to_bits())
        .collect();
    let expected: Vec<u64> = [-1.0, 2.5, 7.0, -1.0, -1.0]
        .iter()
        .map(|v: &f64| v.to_bits())
        .collect();
    assert_eq!(reals, expected);
    let ints: Vec<i64> = bindings.iter().map(|b| int_or(b.as_ref(), -1)).collect();
    assert_eq!(ints, [-1, -1, 7, -1, -1]);
    let bools: Vec<bool> = bindings
        .iter()
        .map(|b| bool_or(b.as_ref(), false))
        .collect();
    assert_eq!(bools, [false, false, false, true, false]);
}
