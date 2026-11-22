use super::*;

fn window(start: f64, stop: f64, step: f64) -> Instants {
    Instants { start, stop, step }
}

fn all(instants: Instants) -> Vec<f64> {
    instants.instants(instants.count(1_000))
}

#[test]
fn the_instants_are_the_grid_then_stop_when_the_grid_does_not_reach_it() {
    assert_eq!(all(window(0.0, 30.0, 10.0)), [0.0, 10.0, 20.0, 30.0]);
    assert_eq!(window(0.0, 30.0, 10.0).count(100), 4);
    // A sampling that does not divide the window ends with `stop`, off the grid.
    assert_eq!(all(window(0.0, 25.0, 10.0)), [0.0, 10.0, 20.0, 25.0]);
    assert_eq!(window(0.0, 25.0, 10.0).count(100), 4);
    assert_eq!(all(window(5.0, 5.0, 10.0)), [5.0]);
    assert_eq!(all(window(5.0, 9.0, 10.0)), [5.0, 9.0]);
    // 3 * 0.1 is 0.30000000000000004 in binary, past 0.3: the grid's own arithmetic decides,
    // and 0.3 follows as the off-grid stop.
    assert_eq!(all(window(0.0, 0.3, 0.1)), [0.0, 0.1, 0.2, 0.3]);
}

#[test]
fn the_count_saturates_above_the_cap_without_walking_the_grid() {
    assert!(window(0.0, 1e300, 1e-300).count(10) > 10);
    // At the cap exactly, and one past it: on the grid, and with an off-grid stop.
    assert_eq!(window(0.0, 9.0, 1.0).count(10), 10);
    assert!(window(0.0, 10.0, 1.0).count(10) > 10);
    assert_eq!(window(0.0, 8.5, 1.0).count(10), 10);
    assert!(window(0.0, 9.5, 1.0).count(10) > 10);
    assert_eq!(window(0.0, 9.0, 1.0).count(u64::MAX), 10);
}

#[test]
fn every_instant_holds_the_last_sample_at_or_before_it_and_the_later_of_an_equal_time_pair() {
    let times = [-5.0, 0.0, 10.0, 10.0, 15.0];
    let instants = all(window(0.0, 25.0, 10.0));
    assert_eq!(instants, [0.0, 10.0, 20.0, 25.0]);
    assert_eq!(held_indexes(&times, &instants), Some(vec![1, 3, 4, 4]));
    // A window starting between samples holds the one before.
    assert_eq!(held_indexes(&[0.0, 4.0], &[1.0, 6.0]), Some(vec![0, 1]));
    assert_eq!(held(&[2, 0], &[7, 8, 9]), [9, 7]);
}

#[test]
fn a_series_starting_after_the_window_start_cannot_be_held() {
    assert_eq!(held_indexes(&[1.0, 2.0], &[0.0, 5.0]), None);
    assert_eq!(held_indexes(&[], &[0.0]), None);
    assert_eq!(held_indexes(&[0.0], &[]), None);
}
