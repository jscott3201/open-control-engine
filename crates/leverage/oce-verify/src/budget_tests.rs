//! The four budgets as arithmetic: each product at its limit and one past it, the order they are
//! checked in, and hostile counts at the `u64` and `u128` extremes, which refuse and never wrap
//! or panic.

use super::{Work, check};
use crate::limits::VerifyLimits;

/// A run of one scenario of one tick on a one-byte subject with no mutant, no edit list and no
/// expectation: one frame, no instant, two loads of one byte, one byte-frame.
fn least() -> Work {
    Work {
        ticks: 1,
        scenarios: 1,
        mutants: 0,
        instants: 0,
        edit_lists: 0,
        document_bytes: 1,
    }
}

fn code(work: &Work, limits: &VerifyLimits) -> Option<&'static str> {
    check(work, limits).err().map(|error| {
        assert_eq!(error.subject(), "scenarios", "{error}");
        error.code()
    })
}

#[test]
fn each_product_counts_what_the_note_says() {
    let work = Work {
        ticks: 31,
        scenarios: 7,
        mutants: 3,
        instants: 216,
        edit_lists: 2,
        document_bytes: 25_618,
    };
    assert_eq!(work.frames(), Some(31 * 7 * 4));
    assert_eq!(work.compared_instants(), Some(4 * 216));
    assert_eq!(work.load_bytes(), Some(25_618 * (1 + 2 + 7)));
    assert_eq!(work.frame_bytes(), Some(31 * 7 * 25_618));
}

#[test]
fn each_budget_admits_its_limit_and_refuses_one_past_it() {
    let work = Work {
        ticks: 31,
        scenarios: 7,
        mutants: 3,
        instants: 216,
        edit_lists: 0,
        document_bytes: 25_618,
    };
    let exact = VerifyLimits {
        max_total_frames: 868,
        max_total_instants: 864,
        max_total_load_bytes: 204_944,
        max_total_frame_bytes: 5_559_106,
        ..VerifyLimits::MAX
    };
    assert_eq!(code(&work, &exact), None);
    for (lowered, expected) in [
        (
            VerifyLimits {
                max_total_frames: 867,
                ..exact
            },
            "too-many-frames",
        ),
        (
            VerifyLimits {
                max_total_instants: 863,
                ..exact
            },
            "too-many-instants",
        ),
        (
            VerifyLimits {
                max_total_load_bytes: 204_943,
                ..exact
            },
            "too-many-load-bytes",
        ),
        (
            VerifyLimits {
                max_total_frame_bytes: 5_559_105,
                ..exact
            },
            "too-many-frame-bytes",
        ),
    ] {
        assert_eq!(code(&work, &lowered), Some(expected));
    }
}

#[test]
fn the_first_budget_exceeded_in_table_order_wins() {
    let nothing = VerifyLimits {
        max_total_frames: 0,
        max_total_instants: 0,
        max_total_load_bytes: 0,
        max_total_frame_bytes: 0,
        ..VerifyLimits::MAX
    };
    let work = Work {
        instants: 1,
        ..least()
    };
    assert_eq!(code(&work, &nothing), Some("too-many-frames"));
    let frames = VerifyLimits {
        max_total_frames: 1,
        ..nothing
    };
    assert_eq!(code(&work, &frames), Some("too-many-instants"));
    let instants = VerifyLimits {
        max_total_instants: 1,
        ..frames
    };
    assert_eq!(code(&work, &instants), Some("too-many-load-bytes"));
    let loads = VerifyLimits {
        max_total_load_bytes: 2,
        ..instants
    };
    assert_eq!(code(&work, &loads), Some("too-many-frame-bytes"));
    let all = VerifyLimits {
        max_total_frame_bytes: 1,
        ..loads
    };
    assert_eq!(code(&work, &all), None);
}

#[test]
fn a_scenario_past_the_replay_frame_limit_is_refused_first_and_names_the_clock() {
    let mut limits = VerifyLimits::MAX;
    limits.replay.max_frames = 30;
    limits.max_total_frames = 0;
    let error = check(
        &Work {
            ticks: 31,
            ..least()
        },
        &limits,
    )
    .expect_err("past the replay limit");
    assert_eq!(
        (error.code(), error.subject()),
        ("too-many-frames", "clock")
    );
    // A set with no scenario runs no frame, whatever its clock.
    let none = Work {
        ticks: 31,
        scenarios: 0,
        ..least()
    };
    assert!(check(&none, &limits).is_ok());
}

#[test]
fn a_product_past_u64_is_compared_in_full() {
    // 2^24 ticks, 2^12 scenarios and 2^40 bytes: 2^76 byte-frames, which a u64 would wrap to 0.
    let work = Work {
        ticks: 1 << 24,
        scenarios: 1 << 12,
        mutants: 0,
        instants: 0,
        edit_lists: 0,
        document_bytes: 1 << 40,
    };
    assert_eq!(work.frame_bytes(), Some(1 << 76));
    let roomy = VerifyLimits {
        max_total_frames: u64::MAX,
        max_total_instants: u64::MAX,
        max_total_load_bytes: u64::MAX,
        max_total_frame_bytes: u64::MAX,
        ..VerifyLimits::MAX
    };
    assert_eq!(code(&work, &roomy), Some("too-many-frame-bytes"));
}

#[test]
fn hostile_counts_at_the_extremes_refuse_and_never_panic() {
    let roomy = VerifyLimits {
        max_total_frames: u64::MAX,
        max_total_instants: u64::MAX,
        max_total_load_bytes: u64::MAX,
        max_total_frame_bytes: u64::MAX,
        ..VerifyLimits::MAX
    };
    let mut wide_open = roomy;
    wide_open.replay.max_frames = u64::MAX;
    assert_eq!(code(&least(), &wide_open), None);
    // Every factor at u64::MAX: each product passes u128 or u64 and is refused, in order.
    let huge = Work {
        ticks: u64::MAX,
        scenarios: u128::from(u64::MAX),
        mutants: u128::from(u64::MAX),
        instants: u128::from(u64::MAX),
        edit_lists: u128::from(u64::MAX),
        document_bytes: u128::from(u64::MAX),
    };
    assert_eq!(huge.frames(), None, "three u64 factors pass u128");
    assert_eq!(code(&huge, &wide_open), Some("too-many-frames"));
    // Counts that saturated at u128::MAX: `1 + m` and `1 + e + s` do not fit, and refuse.
    let saturated = Work {
        ticks: 1,
        scenarios: u128::MAX,
        mutants: u128::MAX,
        instants: u128::MAX,
        edit_lists: u128::MAX,
        document_bytes: u128::MAX,
    };
    assert_eq!(saturated.frames(), None);
    assert_eq!(saturated.compared_instants(), None);
    assert_eq!(saturated.load_bytes(), None);
    assert_eq!(saturated.frame_bytes(), None);
    for (work, expected) in [
        (
            Work {
                mutants: 1,
                instants: u128::from(u64::MAX),
                ..least()
            },
            "too-many-instants",
        ),
        (
            Work {
                edit_lists: u128::from(u64::MAX),
                document_bytes: u128::from(u64::MAX),
                ..least()
            },
            "too-many-load-bytes",
        ),
        (
            Work {
                ticks: u64::MAX,
                document_bytes: 2,
                ..least()
            },
            "too-many-frame-bytes",
        ),
    ] {
        assert_eq!(code(&work, &wide_open), Some(expected), "{work:?}");
    }
    // At the largest u64 each product can reach without passing u64::MAX, it is admitted.
    let at_most = Work {
        ticks: 1,
        scenarios: 1,
        mutants: 0,
        instants: u128::from(u64::MAX),
        edit_lists: 0,
        document_bytes: 1,
    };
    assert_eq!(at_most.compared_instants(), Some(u128::from(u64::MAX)));
    assert_eq!(code(&at_most, &wide_open), None);
}

#[test]
fn the_defaults_are_the_ruled_budgets() {
    let max = VerifyLimits::MAX;
    assert_eq!(max.max_total_frames, 1 << 24);
    assert_eq!(max.max_total_instants, 1 << 28);
    assert_eq!(max.max_total_load_bytes, 1 << 30);
    assert_eq!(max.max_total_frame_bytes, 1 << 40);
    // Each default admits its own value and refuses one more.
    let work = Work {
        ticks: 1 << 23,
        scenarios: 2,
        mutants: 0,
        instants: 1 << 28,
        edit_lists: (1 << 30) - 3,
        document_bytes: 1,
    };
    assert_eq!(code(&work, &max), None);
    for (past, expected) in [
        (
            Work {
                ticks: (1 << 23) + 1,
                ..work
            },
            "too-many-frames",
        ),
        (
            Work {
                instants: (1 << 28) + 1,
                ..work
            },
            "too-many-instants",
        ),
        (
            Work {
                edit_lists: (1 << 30) - 2,
                ..work
            },
            "too-many-load-bytes",
        ),
    ] {
        assert_eq!(code(&past, &max), Some(expected));
    }
    let frame_bytes = Work {
        ticks: 1 << 20,
        scenarios: 1,
        mutants: 0,
        instants: 0,
        edit_lists: 0,
        document_bytes: 1 << 20,
    };
    assert_eq!(code(&frame_bytes, &max), None);
    assert_eq!(
        code(
            &Work {
                document_bytes: (1 << 20) + 1,
                ..frame_bytes
            },
            &max
        ),
        Some("too-many-frame-bytes")
    );
}
