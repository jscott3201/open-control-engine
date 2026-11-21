//! The union-of-times merge: the equal-time rule, its independence of order, and its bounds.

use super::*;

fn real(values: &[f64]) -> Values {
    Values::Real(values.to_vec())
}

/// Merge `(times, one Real column)` sources and give the frame times and, per source, its
/// merged column's cells (`None` for missing).
fn merged(sources: &[(&[f64], &[f64])]) -> (Vec<f64>, Vec<Vec<Option<f64>>>) {
    let values: Vec<Vec<Values>> = sources.iter().map(|(_, v)| vec![real(v)]).collect();
    let merging: Vec<Merging<'_>> = sources
        .iter()
        .zip(&values)
        .map(|((times, _), columns)| Merging { times, columns })
        .collect();
    let (times, columns) = merge(&merging, &ReplayLimits::default()).expect("merges");
    let cells = columns
        .into_iter()
        .map(|mut per_source| match per_source.remove(0) {
            Column::Real(values) => values
                .into_iter()
                .map(|x| (!x.is_nan()).then_some(x))
                .collect(),
            other => panic!("{other:?}"),
        })
        .collect();
    (times, cells)
}

#[test]
fn two_files_with_one_row_each_at_an_instant_give_one_frame() {
    let (times, cells) = merged(&[(&[0.0, 10.0], &[1.0, 2.0]), (&[10.0], &[3.0])]);
    assert_eq!(times, [0.0, 10.0]);
    assert_eq!(cells, [vec![Some(1.0), Some(2.0)], vec![None, Some(3.0)]]);
}

#[test]
fn two_rows_at_an_instant_beside_one_give_two_frames_the_kth_row_in_the_kth() {
    let (times, cells) = merged(&[(&[10.0, 10.0], &[1.0, 2.0]), (&[10.0], &[3.0])]);
    assert_eq!(times, [10.0, 10.0]);
    // The second file has no second row at 10: missing there, for its hold to fill.
    assert_eq!(cells, [vec![Some(1.0), Some(2.0)], vec![Some(3.0), None]]);
    // Three rows against two: three frames, the third missing in the second file.
    let (times, cells) = merged(&[
        (&[5.0, 5.0], &[1.0, 2.0]),
        (&[5.0, 5.0, 5.0], &[3.0, 4.0, 5.0]),
    ]);
    assert_eq!(times, [5.0, 5.0, 5.0]);
    assert_eq!(
        cells,
        [
            vec![Some(1.0), Some(2.0), None],
            vec![Some(3.0), Some(4.0), Some(5.0)]
        ]
    );
}

#[test]
fn the_union_interleaves_and_a_source_is_missing_where_it_has_no_row() {
    let (times, cells) = merged(&[
        (&[-1.0, 0.0, 2.5], &[1.0, 2.0, 3.0]),
        (&[0.0, 1.0, 1.0, 7.0], &[4.0, 5.0, 6.0, 7.0]),
        (&[], &[]),
    ]);
    assert_eq!(times, [-1.0, 0.0, 1.0, 1.0, 2.5, 7.0]);
    assert_eq!(
        cells,
        [
            vec![Some(1.0), Some(2.0), None, None, Some(3.0), None],
            vec![None, Some(4.0), Some(5.0), Some(6.0), None, Some(7.0)],
            vec![None; 6],
        ]
    );
}

#[test]
fn the_frames_never_exceed_the_rows_of_all_sources_together() {
    // 64 sources of 64 rows at 64 distinct times each: no product of sources.
    let times: Vec<Vec<f64>> = (0..64)
        .map(|s| (0..64).map(|r| f64::from(s * 64 + r)).collect())
        .collect();
    let values: Vec<Vec<f64>> = times.clone();
    let sources: Vec<(&[f64], &[f64])> = times
        .iter()
        .zip(&values)
        .map(|(t, v)| (t.as_slice(), v.as_slice()))
        .collect();
    let (frames, _) = merged(&sources);
    assert_eq!(frames.len(), 64 * 64);
    // And at one shared time each, as many frames as the deepest source has rows there.
    let same: Vec<Vec<f64>> = (1..=64).map(|n| vec![0.0; n]).collect();
    let sources: Vec<(&[f64], &[f64])> =
        same.iter().map(|t| (t.as_slice(), t.as_slice())).collect();
    assert_eq!(merged(&sources).0.len(), 64);
}

/// A deterministic shuffle of `0..n` from `seed`.
fn shuffled(n: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    let mut state = seed;
    for i in (1..n).rev() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        order.swap(i, (state >> 33) as usize % (i + 1));
    }
    order
}

#[test]
fn the_merge_does_not_depend_on_the_order_of_the_sources() {
    // Random non-decreasing times on a coarse grid, so many instants are shared and repeated.
    let mut state = 7_u64;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        state >> 33
    };
    for _ in 0..50 {
        let count = 1 + (next() % 6) as usize;
        let sources: Vec<(Vec<f64>, Vec<f64>)> = (0..count)
            .map(|s| {
                let rows = (next() % 12) as usize;
                let mut times: Vec<f64> = (0..rows).map(|_| (next() % 5) as f64 - 2.0).collect();
                times.sort_by(f64::total_cmp);
                let values = (0..rows).map(|r| (s * 100 + r) as f64).collect();
                (times, values)
            })
            .collect();
        let view = |order: &[usize]| {
            let ordered: Vec<(&[f64], &[f64])> = order
                .iter()
                .map(|&i| (sources[i].0.as_slice(), sources[i].1.as_slice()))
                .collect();
            let (times, cells) = merged(&ordered);
            // Back in the original order, so the two runs compare source by source.
            let mut by_source = vec![Vec::new(); count];
            for (position, &i) in order.iter().enumerate() {
                by_source[i] = cells[position]
                    .iter()
                    .map(|c| c.map(f64::to_bits))
                    .collect();
            }
            (
                times.iter().map(|t| t.to_bits()).collect::<Vec<_>>(),
                by_source,
            )
        };
        let straight = view(&(0..count).collect::<Vec<_>>());
        for seed in 0..4 {
            assert_eq!(view(&shuffled(count, seed)), straight);
        }
    }
}

#[test]
fn integer_and_boolean_columns_carry_a_cleared_flag_where_missing() {
    let ints = vec![Values::Integer(vec![Some(4), None])];
    let bools = vec![Values::Boolean(vec![Some(true)])];
    let sources = [
        Merging {
            times: &[0.0, 1.0],
            columns: &ints,
        },
        Merging {
            times: &[1.0],
            columns: &bools,
        },
    ];
    let (times, columns) = merge(&sources, &ReplayLimits::default()).expect("merges");
    assert_eq!(times, [0.0, 1.0]);
    match &columns[0][0] {
        Column::Integer { values, valid } => {
            assert_eq!(values, &[4, 0]);
            assert_eq!(valid.as_deref(), Some(&[true, false][..]));
        }
        other => panic!("{other:?}"),
    }
    match &columns[1][0] {
        Column::Boolean { values, valid } => {
            assert_eq!(values, &[false, true]);
            assert_eq!(valid.as_deref(), Some(&[false, true][..]));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn each_bound_admits_its_limit_and_refuses_one_past() {
    let columns = vec![real(&[0.0, 1.0, 2.0])];
    let three = [Merging {
        times: &[0.0, 1.0, 2.0],
        columns: &columns,
    }];
    let at = |limits: ReplayLimits| merge(&three, &limits).map(|(t, _)| t.len());
    let max = ReplayLimits::default();
    // Frames.
    assert_eq!(at(ReplayLimits { max_rows: 3, ..max }), Ok(3));
    assert_eq!(
        at(ReplayLimits { max_rows: 2, ..max }).unwrap_err().code(),
        "too-many-rows"
    );
    // Columns, the time column included.
    assert_eq!(
        at(ReplayLimits {
            max_columns: 2,
            ..max
        }),
        Ok(3)
    );
    assert_eq!(
        at(ReplayLimits {
            max_columns: 1,
            ..max
        })
        .unwrap_err()
        .code(),
        "too-many-columns"
    );
    // Bytes: three frames of a time and one Real, 48 bytes.
    assert_eq!(
        at(ReplayLimits {
            max_source_bytes: 48,
            ..max
        }),
        Ok(3)
    );
    assert_eq!(
        at(ReplayLimits {
            max_source_bytes: 47,
            ..max
        })
        .unwrap_err()
        .code(),
        "too-many-bytes"
    );
}

#[test]
fn a_merge_past_the_frame_cap_is_refused_before_anything_is_allocated() {
    let times: Vec<f64> = (0..10_000).map(f64::from).collect();
    let columns = vec![real(&times)];
    let sources = [Merging {
        times: &times,
        columns: &columns,
    }];
    let limits = ReplayLimits {
        max_rows: 100,
        ..ReplayLimits::default()
    };
    let mut result = None;
    let info = allocation_counter::measure(|| result = Some(merge(&sources, &limits)));
    assert_eq!(result.expect("ran").unwrap_err().code(), "too-many-rows");
    // The heap of one cursor and the refusal, never a frame column.
    assert!(info.bytes_max < 2_048, "{info:?}");
}
