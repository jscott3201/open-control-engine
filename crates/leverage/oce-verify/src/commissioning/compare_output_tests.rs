use super::*;

use Fate::{Compared, Masked, Uncovered};

#[test]
fn two_to_the_thousand_is_the_out_of_range_bound() {
    assert_eq!(MAX_ENGINE_MAGNITUDE, 2.0_f64.powi(1000));
    assert_eq!(MAX_ENGINE_MAGNITUDE.to_bits(), 0x7e70_0000_0000_0000);
}

#[test]
fn the_time_in_exceedance_adds_the_step_to_the_next_instant_and_the_last_adds_nothing() {
    let times = [0.0, 1.0, 3.0, 6.0];
    let fates = [Compared, Compared, Compared, Compared];
    let verdict = verdict(&times, &fates, &[0.0, 0.5, 0.0, 0.25]);
    assert_eq!(verdict.status, OutputStatus::Fail);
    // 1 -> 3 counts 2; the last instant adds nothing.
    assert_eq!(verdict.exceedance_time_s, 2.0);
    assert_eq!(verdict.first, Some(1.0));
    assert_eq!(
        verdict.worst,
        Some(Exceedance {
            t: 1.0,
            exceedance: 0.5
        })
    );
}

#[test]
fn the_worst_is_the_earliest_instant_with_the_largest_exceedance() {
    let times = [0.0, 1.0, 2.0];
    let verdict = verdict(&times, &[Compared; 3], &[0.5, 0.75, 0.75]);
    assert_eq!(
        verdict.worst,
        Some(Exceedance {
            t: 1.0,
            exceedance: 0.75
        })
    );
}

#[test]
fn masked_and_uncovered_instants_never_fail_and_never_count_as_compared() {
    let times = [0.0, 1.0, 2.0, 3.0];
    let fates = [Masked, Uncovered, Compared, Masked];
    let verdict = verdict(&times, &fates, &[9.0, f64::NAN, 0.0, 9.0]);
    assert_eq!(verdict.status, OutputStatus::Pass);
    assert_eq!(verdict.worst, None);
    assert_eq!(verdict.first, None);
    assert_eq!(verdict.exceedance_time_s, 0.0);
}

#[test]
fn nothing_compared_is_no_data_never_pass() {
    let gated = verdict(&[0.0, 1.0], &[Masked, Uncovered], &[0.0, f64::NAN]);
    assert_eq!(gated.status, OutputStatus::NoData);
    let empty = verdict(&[], &[], &[]);
    assert_eq!(empty.status, OutputStatus::NoData);
}

#[test]
fn a_masked_run_is_its_first_and_last_instant_and_any_other_fate_ends_it() {
    let times = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let fates = [Masked, Masked, Compared, Masked, Uncovered, Masked, Masked];
    let verdict = verdict(&times, &fates, &[0.0; 7]);
    let (intervals, total) = verdict.masked_intervals;
    assert_eq!(total, 3);
    assert_eq!(
        intervals,
        [
            MaskedInterval { from: 0.0, to: 1.0 },
            MaskedInterval { from: 3.0, to: 3.0 },
            MaskedInterval { from: 5.0, to: 6.0 },
        ]
    );
}

#[test]
fn masked_intervals_are_capped_with_their_total() {
    let n = MAX_MASKED_INTERVALS + 1;
    let times: Vec<f64> = (0..2 * n).map(|k| k as f64).collect();
    let fates: Vec<Fate> = (0..2 * n)
        .map(|k| if k % 2 == 0 { Masked } else { Compared })
        .collect();
    let verdict = verdict(&times, &fates, &vec![0.0; 2 * n]);
    assert_eq!(verdict.masked_intervals.0.len(), MAX_MASKED_INTERVALS);
    assert_eq!(verdict.masked_intervals.1, n as u64);
}

#[test]
fn an_infinite_exceedance_fails_and_adds_its_step() {
    let verdict = verdict(&[0.0, 2.0], &[Compared; 2], &[f64::INFINITY, f64::INFINITY]);
    assert_eq!(verdict.status, OutputStatus::Fail);
    assert_eq!(verdict.exceedance_time_s, 2.0);
}

#[test]
fn masked_exceedances_are_capped_at_16_with_their_total() {
    let all: Vec<Exceedance> = (0..=MAX_MASKED_EXCEEDANCE)
        .map(|k| Exceedance {
            t: k as f64,
            exceedance: 1.0,
        })
        .collect();
    let (kept, total) = capped(all, MAX_MASKED_EXCEEDANCE);
    assert_eq!((kept.len(), total), (16, 17));
    assert_eq!(kept[15].t, 15.0, "the first in time order are kept");
}
