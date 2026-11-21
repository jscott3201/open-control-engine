//! The absent elements of a range, counted and named from the present ones (issue #726).

use super::{Absent, NAMED_ABSENT, absent_in};

fn absent(count: usize, first: &[usize]) -> Absent {
    Absent {
        count,
        first: first.to_vec(),
    }
}

/// The same answer by walking every position of the range: the oracle.
fn walked(present: &[usize], low: usize, high: usize) -> Absent {
    let missing: Vec<usize> = (low..=high).filter(|p| !present.contains(p)).collect();
    absent(missing.len(), &missing[..missing.len().min(NAMED_ABSENT)])
}

#[test]
fn a_full_range_has_nothing_absent_and_an_empty_one_has_nothing_either() {
    assert_eq!(absent_in([1, 2, 3], 1, 3), Absent::default());
    assert_eq!(absent_in([], 5, 4), Absent::default());
    assert_eq!(absent_in([7], 7, 7), Absent::default());
}

#[test]
fn one_absent_position_is_counted_and_named_wherever_it_is() {
    assert_eq!(absent_in([2, 3, 4], 1, 4), absent(1, &[1]));
    assert_eq!(absent_in([1, 2, 4], 1, 4), absent(1, &[3]));
    assert_eq!(absent_in([1, 2, 3], 1, 4), absent(1, &[4]));
    assert_eq!(absent_in([], 9, 9), absent(1, &[9]));
}

#[test]
fn the_first_four_absent_positions_are_named_in_ascending_order_and_the_rest_counted() {
    assert_eq!(absent_in([], 1, 1 << 20), absent(1 << 20, &[1, 2, 3, 4]));
    assert_eq!(absent_in([1, 3, 5, 7], 1, 10), absent(6, &[2, 4, 6, 8]));
    // The last position of a 524,288 x 2 table bound alone: every other one absent.
    assert_eq!(
        absent_in([1 << 20], 1, 1 << 20),
        absent((1 << 20) - 1, &[1, 2, 3, 4])
    );
}

#[test]
fn positions_outside_the_range_or_out_of_order_are_skipped() {
    assert_eq!(absent_in([0, 2, 9], 1, 3), absent(2, &[1, 3]));
    // A repeat is not counted twice, and a step backwards is skipped: the caller yields
    // ascending positions, so neither happens in use.
    assert_eq!(absent_in([2, 2, 3], 1, 4), absent(2, &[1, 4]));
    assert_eq!(absent_in([3, 1], 1, 4), absent(3, &[1, 2, 4]));
}

#[test]
fn the_ends_of_usize_neither_wrap_nor_panic() {
    assert_eq!(
        absent_in([usize::MAX], usize::MAX, usize::MAX),
        Absent::default()
    );
    assert_eq!(
        absent_in([], usize::MAX, usize::MAX),
        absent(1, &[usize::MAX])
    );
    assert_eq!(
        absent_in([usize::MAX], usize::MAX - 5, usize::MAX),
        absent(
            5,
            &[
                usize::MAX - 5,
                usize::MAX - 4,
                usize::MAX - 3,
                usize::MAX - 2
            ]
        )
    );
    assert_eq!(absent_in([], 0, usize::MAX).count, usize::MAX);
    assert_eq!(absent_in([0], 0, usize::MAX).count, usize::MAX);
}

#[test]
fn counting_agrees_with_walking_every_position_of_small_ranges() {
    // Every subset of 1..=7 as the present positions, against every range inside 0..=8.
    for mask in 0_u32..(1 << 7) {
        let present: Vec<usize> = (1..=7).filter(|p| mask & (1 << (p - 1)) != 0).collect();
        for low in 0..=8 {
            for high in low..=8 {
                assert_eq!(
                    absent_in(present.iter().copied(), low, high),
                    walked(&present, low, high),
                    "present {present:?}, range {low}..={high}"
                );
            }
        }
    }
}

#[test]
fn two_ranges_join_their_counts_and_name_the_first_four_of_both() {
    let joined = absent(2, &[1, 2]).then(absent(5, &[8, 9, 10, 11]));
    assert_eq!(joined, absent(7, &[1, 2, 8, 9]));
    assert_eq!(
        joined.named(|p| format!("k_{p}")),
        "k_1, k_2, k_8, k_9 and 3 more"
    );
    let empty_first = Absent::default().then(absent(1, &[3]));
    assert_eq!(empty_first.named(|p| format!("k_{p}")), "k_3");
    assert_eq!(
        absent(4, &[1, 2, 3, 4]).named(|p| p.to_string()),
        "1, 2, 3, 4",
        "four named, none more"
    );
}
