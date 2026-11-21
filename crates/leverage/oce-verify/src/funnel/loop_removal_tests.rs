//! Loop removal: hand-derived loops, the gap buffer against a line-for-line transliteration of
//! funnel's `removeLoop`, the work counter at its boundary and the place the C leaves its arrays.
//!
//! The transliteration is derived from funnel's `algorithmRectangle.c`, as the module is; its
//! licence is `LICENSE-funnel` at the crate root.

use super::*;

fn corners(points: &[(f64, f64)]) -> Vec<Corner> {
    points.iter().map(|&(x, y)| Corner { x, y }).collect()
}

fn bits(corners: &[Corner]) -> Vec<(u64, u64)> {
    corners
        .iter()
        .map(|corner| (corner.x.to_bits(), corner.y.to_bits()))
        .collect()
}

/// A budget no test input comes near.
fn ample() -> Work {
    Work::new(u64::MAX)
}

/// funnel's `removeLoop`, transliterated statement for statement onto two `Vec`s that are copied
/// whole at every edit, as the C's `removeRange`, `insertAt` and `removeAt` copy them: the
/// oracle for the gap buffer. `None` where the C would read outside an array.
fn remove_loops_as_the_c_does(input: &[Corner], lower: bool) -> Option<Vec<Corner>> {
    let mut x: Vec<f64> = input.iter().map(|corner| corner.x).collect();
    let mut y: Vec<f64> = input.iter().map(|corner| corner.y).collect();
    let mut j = 1_usize;
    let mut scanned = 0_u32;
    while j + 2 < x.len() {
        scanned += 1;
        assert!(scanned < 1_000_000, "the transliteration does not end");
        if x[j + 1] < x[j] {
            let size = x.len();
            let mut add_point = true;
            let (mut ix, mut iy) = (0.0, 0.0);
            let mut i = j;
            let mut i_previous = i;
            loop {
                if i == 0 {
                    return None;
                }
                if x[j + 1] < x[i - 1] {
                    i -= 1;
                } else {
                    break;
                }
            }
            let mut k_max = j + 1;
            while x[k_max] < x[j] && k_max < size - 1 {
                k_max += 1;
            }
            let mut k = j + 1;
            let mut on_segment = y[i - 1];
            while ((lower && on_segment < y[k]) || (!lower && y[k] < on_segment)) && k < k_max {
                i_previous = i;
                k += 1;
                while (x[i] < x[k]
                    || (lower
                        && equ(x[i], x[k])
                        && y[i] < y[k]
                        && !(k + 1 < size && equ(x[k], x[k + 1]) && y[k + 1] < y[k]))
                    || (!lower
                        && equ(x[i], x[k])
                        && y[i] > y[k]
                        && !(k + 1 < size && equ(x[k], x[k + 1]) && y[k + 1] > y[k])))
                    && i < j
                {
                    i += 1;
                }
                on_segment = if !equ(x[i], x[i - 1]) {
                    (y[i] - y[i - 1]) / (x[i] - x[i - 1]) * (x[k] - x[i - 1]) + y[i - 1]
                } else {
                    y[i]
                };
            }
            i = if i_previous > 1 {
                i_previous - 1
            } else {
                i_previous
            };
            if !equ(x[k], x[k - 1]) {
                on_segment = (y[k] - y[k - 1]) / (x[k] - x[k - 1]) * (x[i] - x[k - 1]) + y[k - 1];
            }
            while (!equ(x[k], x[k - 1])
                && ((lower && y[i] < on_segment) || (!lower && on_segment < y[i])))
                || (equ(x[k], x[k - 1]) && x[i] < x[k])
            {
                i += 1;
                if i >= size {
                    return None;
                }
                if !equ(x[k], x[k - 1]) {
                    on_segment =
                        (y[k] - y[k - 1]) / (x[k] - x[k - 1]) * (x[i] - x[k - 1]) + y[k - 1];
                }
            }
            if equ(x[i], x[i - 1]) && equ(x[k], x[k - 1]) {
                add_point = false;
            } else if equ(x[i], x[i - 1]) {
                ix = x[i];
                iy = y[k - 1] + ((x[i] - x[k - 1]) * (y[k] - y[k - 1])) / (x[k] - x[k - 1]);
            } else if equ(x[k], x[k - 1]) {
                ix = x[k];
                iy = y[i - 1] + ((x[k] - x[i - 1]) * (y[i] - y[i - 1])) / (x[i] - x[i - 1]);
            } else {
                let a1 = (y[i] - y[i - 1]) / (x[i] - x[i - 1]);
                let a2 = (y[k] - y[k - 1]) / (x[k] - x[k - 1]);
                if !equ(a1, a2) {
                    ix = (a1 * x[i - 1] - a2 * x[k - 1] - y[i - 1] + y[k - 1]) / (a1 - a2);
                    iy = if a1.abs() > a2.abs() {
                        a2 * (ix - x[k - 1]) + y[k - 1]
                    } else {
                        a1 * (ix - x[i - 1]) + y[i - 1]
                    };
                } else {
                    add_point = false;
                }
            }
            // removeRange(array, size, i, k - i): the elements before i, then those from k on,
            // for a negative count as well.
            let remove_range = |array: &[f64]| [&array[..i], &array[k..]].concat();
            let mut new_x = remove_range(&x);
            let mut new_y = remove_range(&y);
            if add_point && (!equ(new_x[i], ix) || !equ(new_y[i], iy)) {
                new_x.insert(i, ix);
                new_y.insert(i, iy);
            }
            j = i;
            if equ(new_x[i - 1], new_x[i]) && equ(new_y[i - 1], new_y[i]) {
                new_x.remove(i);
                new_y.remove(i);
                j = i - 1;
            }
            x = new_x;
            y = new_y;
        }
        j += 1;
    }
    Some(x.into_iter().zip(y).map(|(x, y)| Corner { x, y }).collect())
}

/// A deterministic 64-bit generator (SplitMix64).
struct Sequence(u64);

impl Sequence {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u32) -> f64 {
        f64::from((self.next() % u64::from(bound)) as u32)
    }
}

/// An outline-like corner list: mostly advancing in `x` with backward jumps of varied depth, on
/// a coarse grid so that equal `x`, equal `y` and parallel segments all occur.
fn outline_like(sequence: &mut Sequence) -> Vec<Corner> {
    let count = 2 + sequence.next() % 24;
    let mut x = 0.0;
    (0..count)
        .map(|_| {
            x += match sequence.next() % 10 {
                0 | 1 => -sequence.below(6) * 0.5,
                2 => 0.0,
                _ => (1.0 + sequence.below(4)) * 0.5,
            };
            Corner {
                x,
                y: sequence.below(7) * 0.5 - 1.5,
            }
        })
        .collect()
}

#[test]
fn a_peak_under_rectangles_wider_than_its_base_is_cut_out_of_the_lower_bound() {
    // The lower outline of (0,0) (2,0) (3,4) (4,0) (8,0) with half-width 1.5 and half-height
    // 0.5: it climbs to the peak's right corner at x = 4.5 and comes back to its left corner at
    // x = 1.5. The flat segment before the loop and the falling one after it meet at
    // (2.5, -0.5), which is already the corner after the loop, so no corner is added.
    let outline = corners(&[
        (-1.5, -0.5),
        (3.5, -0.5),
        (4.5, 3.5),
        (1.5, 3.5),
        (2.5, -0.5),
        (9.5, -0.5),
    ]);
    let mut work = ample();
    let cut = remove_loops(outline, Side::Lower, &mut work).expect("a loop funnel defines");
    assert_eq!(
        bits(&cut),
        bits(&corners(&[(-1.5, -0.5), (2.5, -0.5), (9.5, -0.5)]))
    );
}

#[test]
fn a_valley_under_wide_rectangles_gets_a_corner_where_the_two_slopes_cross() {
    // The upper outline of the valley (0,4) (4,0) (8,4) with half-width 3 and half-height 0:
    // down along the right corners to (7,0), back to the valley's left corner (1,0), up along
    // the left corners. The falling segment from (3,4) to (7,0) and the rising one from (1,0) to
    // (5,4) cross at (4,3), which replaces the two corners at the bottom.
    let outline = corners(&[
        (-3.0, 4.0),
        (3.0, 4.0),
        (7.0, 0.0),
        (1.0, 0.0),
        (5.0, 4.0),
        (11.0, 4.0),
    ]);
    let mut work = ample();
    let cut = remove_loops(outline, Side::Upper, &mut work).expect("a loop funnel defines");
    assert_eq!(
        bits(&cut),
        bits(&corners(&[
            (-3.0, 4.0),
            (3.0, 4.0),
            (4.0, 3.0),
            (5.0, 4.0),
            (11.0, 4.0)
        ]))
    );
}

#[test]
fn an_outline_that_never_runs_backwards_is_returned_unchanged() {
    let outline = corners(&[(0.0, 1.0), (1.0, 1.0), (1.0, 3.0), (2.0, 3.0), (5.0, -1.0)]);
    for side in [Side::Lower, Side::Upper] {
        let mut work = ample();
        let cut = remove_loops(outline.clone(), side, &mut work).expect("no loop");
        assert_eq!(bits(&cut), bits(&outline));
        // One unit per step of the scan, nothing else.
        assert_eq!(work.spent(), 2);
    }
}

#[test]
fn an_outline_of_fewer_than_four_corners_is_never_scanned() {
    for count in 0..4_u8 {
        let outline: Vec<Corner> = (0..count)
            .map(|index| Corner {
                x: -f64::from(index),
                y: 0.0,
            })
            .collect();
        let mut work = Work::new(0);
        let cut = remove_loops(outline.clone(), Side::Upper, &mut work).expect("nothing to do");
        assert_eq!(bits(&cut), bits(&outline));
        assert_eq!(work.spent(), 0);
    }
}

#[test]
fn the_gap_buffer_gives_the_transliterated_c_its_result_on_ten_thousand_outlines() {
    let mut sequence = Sequence(655);
    let (mut defined, mut undefined, mut edited) = (0, 0, 0);
    for _ in 0..10_000 {
        let outline = outline_like(&mut sequence);
        for (side, lower) in [(Side::Lower, true), (Side::Upper, false)] {
            let expected = remove_loops_as_the_c_does(&outline, lower);
            let mut work = ample();
            match (remove_loops(outline.clone(), side, &mut work), expected) {
                (Ok(ours), Some(theirs)) => {
                    assert_eq!(bits(&ours), bits(&theirs), "{outline:?} {side:?}");
                    defined += 1;
                    if ours.len() != outline.len() {
                        edited += 1;
                    }
                }
                (Err(CompareError::LoopRemovalUndefined), None) => undefined += 1,
                (ours, theirs) => panic!("{outline:?} {side:?}: {ours:?} against {theirs:?}"),
            }
        }
    }
    // The sample reaches both outcomes and real edits, so the agreement is not vacuous.
    assert_eq!(defined + undefined, 20_000);
    assert!(edited > 5_000, "{edited} outlines were edited");
    assert!(undefined > 100, "{undefined} outlines were refused");
}

#[test]
fn a_search_that_would_read_before_the_first_corner_is_refused() {
    // Corner 2 lies left of corner 0, so the backward search for a segment left of it runs off
    // the start of the list: the C reads X[-1].
    let outline = corners(&[(0.0, 0.0), (1.0, 1.0), (-1.0, 2.0), (2.0, 3.0), (3.0, 3.0)]);
    for side in [Side::Lower, Side::Upper] {
        let mut work = ample();
        assert_eq!(
            remove_loops(outline.clone(), side, &mut work),
            Err(CompareError::LoopRemovalUndefined)
        );
        assert_eq!(
            remove_loops_as_the_c_does(&outline, side == Side::Lower),
            None
        );
    }
}

#[test]
fn the_work_counter_accepts_its_limit_and_refuses_one_unit_less() {
    let outline = corners(&[
        (-1.5, -0.5),
        (3.5, -0.5),
        (4.5, 3.5),
        (1.5, 3.5),
        (2.5, -0.5),
        (9.5, -0.5),
    ]);
    let mut work = ample();
    remove_loops(outline.clone(), Side::Lower, &mut work).expect("defined");
    let needed = work.spent();

    let mut exact = Work::new(needed);
    assert!(remove_loops(outline.clone(), Side::Lower, &mut exact).is_ok());
    let mut short = Work::new(needed - 1);
    assert_eq!(
        remove_loops(outline, Side::Lower, &mut short),
        Err(CompareError::WorkLimitExceeded { limit: needed - 1 })
    );
    // Two steps of the scan to reach the backward segment, one step back for i, two forward for
    // the last corner k may reach, one step of k, one corner passed by the gap, three dropped.
    assert_eq!(needed, 10);
}

#[test]
fn a_counter_refuses_the_unit_after_its_limit_and_never_wraps() {
    let mut work = Work::new(3);
    assert!(work.spend(2).is_ok());
    assert!(work.spend(1).is_ok());
    assert_eq!(
        work.spend(1),
        Err(CompareError::WorkLimitExceeded { limit: 3 })
    );
    let mut huge = Work::new(u64::MAX - 1);
    assert!(huge.spend(u64::MAX - 1).is_ok());
    assert!(huge.spend(u64::MAX).is_err());
    assert_eq!(huge.spent(), u64::MAX);
}

#[test]
fn the_work_of_a_run_of_loops_grows_linearly_with_its_length() {
    // The upper outline of a square wave under rectangles five samples wide: every other
    // sample adds a left and a right corner one level up, each pair running back over the pair
    // before it.
    let outline = |teeth: u32| -> Vec<Corner> {
        let mut outline = vec![Corner { x: -2.5, y: 1.0 }];
        for tooth in (1..teeth).step_by(2) {
            let x = f64::from(tooth);
            outline.push(Corner { x: x - 2.5, y: 2.0 });
            outline.push(Corner { x: x + 2.5, y: 2.0 });
        }
        outline.push(Corner {
            x: f64::from(teeth) + 2.5,
            y: 1.0,
        });
        outline
    };
    let spent = |teeth: u32| {
        let mut work = ample();
        remove_loops(outline(teeth), Side::Upper, &mut work).expect("defined");
        work.spent()
    };
    let (small, large) = (spent(1_000), spent(8_000));
    assert!(small >= 1_000, "{small}");
    assert!(large <= 9 * small, "{small} units, then {large}");
    assert!(large <= 20 * 8_000, "{large} units for 8,000 samples");
}

#[test]
fn keeping_from_an_earlier_corner_repeats_the_corners_between_as_the_c_does() {
    // `removeRange` with a negative count keeps the corners before `from` and appends those from
    // `to` on, which repeats `to..from`. No outline reaches it: the forward search for `i` never
    // passes `k` (the module's notes say why). The edit is funnel's all the same, and the
    // transliteration above makes it the same way.
    let outline = corners(&[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)]);
    let mut list = CornerList::new(outline.clone());
    let mut work = ample();
    list.keep_before_and_from(3, 1, &mut work)
        .expect("within the budget");
    assert_eq!(
        bits(&list.into_corners()),
        bits(&corners(&[
            (0.0, 0.0),
            (1.0, 1.0),
            (2.0, 2.0),
            (1.0, 1.0),
            (2.0, 2.0),
            (3.0, 3.0),
            (4.0, 4.0)
        ]))
    );

    // The ordinary direction deletes the range, wherever the gap was left.
    let mut list = CornerList::new(outline);
    list.move_gap(4, &mut work).expect("within the budget");
    list.keep_before_and_from(1, 3, &mut work)
        .expect("within the budget");
    assert_eq!(list.len(), 3);
    assert_eq!(
        bits(&list.into_corners()),
        bits(&corners(&[(0.0, 0.0), (3.0, 3.0), (4.0, 4.0)]))
    );
}

#[test]
fn the_list_reads_inserts_and_removes_on_either_side_of_its_gap() {
    let mut list = CornerList::new(corners(&[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)]));
    let mut work = ample();
    for (index, value) in [(3, 3.0), (0, -1.0), (2, 0.5)] {
        list.insert(index, Corner { x: value, y: value }, &mut work)
            .expect("within the budget");
    }
    let read: Vec<f64> = (0..list.len()).map(|index| list.x(index)).collect();
    assert_eq!(read, [-1.0, 0.0, 0.5, 1.0, 2.0, 3.0]);
    list.remove(5, &mut work).expect("within the budget");
    list.remove(0, &mut work).expect("within the budget");
    let read: Vec<f64> = (0..list.len()).map(|index| list.y(index)).collect();
    assert_eq!(read, [0.0, 0.5, 1.0, 2.0]);
}
