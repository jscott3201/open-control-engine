//! The work bounds of tolerance windows (vectors note decision 48, and the owner ruling of
//! 2026-11-21 on `aligned-grid`'s quadratic cost): each window against the point cap of the
//! set's envelope, and, under `aligned-grid`, the windows' ticks squared, times one plus the
//! mutants, against `VerifyLimits::max_total_aligned_grid_work`, both refused as
//! `too-many-points` at bind before any comparison work.

mod support;

use std::time::{Duration, Instant};

use oce_verify::{VerifyLimits, VerifyOptions};

use support::{refused, run_with, set_on, subject};

/// The `signals` subject's inputs at plain values.
const INPUTS: &str = r#""ahu.T": 1.0, "zone.T": 2.0, "n": 3, "b": true, "lim": 5.0"#;

/// A set on a 1 s clock to `horizon_s` with `envelope`, of `scenarios` scenarios each holding
/// `windows` tolerance windows on `ahu.y` over the whole clock.
fn windows(envelope: &str, horizon_s: u32, scenarios: usize, windows: usize) -> String {
    let window = format!(
        r#"{{"output": "ahu.y", "from_s": 0, "to_s": {horizon_s}, "within": {{"atoly": 0.5}}, "of": 1}}"#
    );
    let expect = vec![window; windows].join(",");
    let all: Vec<String> = (0..scenarios)
        .map(|index| support::named(&format!("s{index}"), INPUTS, &expect))
        .collect();
    set_on("1", &horizon_s.to_string(), &all.join(",")).replacen(
        '{',
        &format!("{{\"envelope\": \"{envelope}\", "),
        1,
    )
}

fn with_work(work: u64) -> VerifyOptions {
    VerifyOptions {
        limits: VerifyLimits {
            max_total_aligned_grid_work: work,
            ..VerifyLimits::MAX
        },
        ..VerifyOptions::default()
    }
}

#[test]
fn the_reviewers_attack_is_refused_at_bind_before_any_comparison() {
    // Two scenarios of 4,096 aligned-grid windows of 32,768 ticks: 2^28 instants, exactly the
    // instant budget, and 2^43 units of quadratic work, hours of comparison. It is refused at
    // the budget step, after one subject load and the names.
    let text = windows("aligned-grid", 32_767, 2, 4_096);
    let start = Instant::now();
    let error = refused(&text, &subject("signals"));
    let elapsed = start.elapsed();
    assert_eq!(
        (error.code(), error.subject()),
        ("too-many-points", "scenarios")
    );
    assert!(
        error.detail().contains("8796093022208 units"),
        "{}",
        error.detail()
    );
    assert!(elapsed < Duration::from_secs(60), "{elapsed:?}");
    // The same shape under funnel is held by the instant budget alone, and admitted by it: the
    // quadratic bound is aligned-grid's.
    let funnel = windows("funnel", 32_767, 2, 4_096);
    let options = VerifyOptions {
        limits: VerifyLimits {
            max_total_instants: (1 << 28) - 1,
            ..VerifyLimits::MAX
        },
        ..VerifyOptions::default()
    };
    let error = run_with(&funnel, &subject("signals"), &options).expect_err("over the instants");
    assert_eq!(error.code(), "too-many-instants");
}

#[test]
fn the_quadratic_bound_admits_a_set_at_it_and_refuses_one_unit_past() {
    // Two windows of 31 ticks: 2 * 961 = 1,922 units.
    let text = windows("aligned-grid", 30, 1, 2);
    assert!(run_with(&text, &subject("signals"), &with_work(1_922)).is_ok());
    let error = run_with(&text, &subject("signals"), &with_work(1_921)).expect_err("one past");
    assert_eq!(
        (error.code(), error.subject()),
        ("too-many-points", "scenarios")
    );
    // Under funnel the bound does not apply.
    let funnel = windows("funnel", 30, 1, 2);
    assert!(run_with(&funnel, &subject("signals"), &with_work(0)).is_ok());
    // An exact window adds no work.
    let exact = set_on(
        "1",
        "30",
        &support::scenario(
            INPUTS,
            r#"{"output": "ahu.y", "from_s": 0, "to_s": 30, "equals": 1.0}"#,
        ),
    )
    .replacen('{', "{\"envelope\": \"aligned-grid\", ", 1);
    assert!(run_with(&exact, &subject("signals"), &with_work(0)).is_ok());
}

#[test]
fn each_mutant_counts_the_quadratic_work_again() {
    let text = windows("aligned-grid", 30, 1, 1).replacen(
        "\"scenarios\"",
        r#""mutants": [{"path": "m/a.cxf.json"}], "scenarios""#,
        1,
    );
    let signals = subject("signals");
    let set = support::load(&text).expect("loads");
    // 961 units on the subject and 961 on the mutant: refused at 1,921 before the mutant's bytes
    // are judged, admitted at 1,922 as far as the mutant checks.
    let at = |work| {
        oce_verify::run_vectors(
            &set,
            &oce_verify::VectorInputs::new(&signals).with_mutants(&[("m/a.cxf.json", b"[]")]),
            &with_work(work),
        )
        .expect_err("refused either way")
        .code()
    };
    assert_eq!(at(1_921), "too-many-points");
    assert_eq!(at(1_922), "mutant-load-failed");
}

#[test]
fn the_point_cap_is_judged_before_the_quadratic_work() {
    // One window past the aligned-grid cap is named by its place, whatever the work bound says.
    let text = windows("aligned-grid", 32_768, 1, 1);
    let error = run_with(&text, &subject("signals"), &with_work(0)).expect_err("refused");
    assert_eq!(
        (error.code(), error.subject()),
        ("too-many-points", "scenarios[0].expect[0]")
    );
}

#[test]
fn a_masked_window_counts_its_ticks_in_the_quadratic_work() {
    // A `when` changes which ticks are judged, never the comparison's cost: both calls still run
    // over every tick of the window.
    let text =
        windows("aligned-grid", 30, 1, 2).replace(r#""of": 1}"#, r#""of": 1, "when": ["b"]}"#);
    assert!(run_with(&text, &subject("signals"), &with_work(1_922)).is_ok());
    let error = run_with(&text, &subject("signals"), &with_work(1_921)).expect_err("one past");
    assert_eq!(
        (error.code(), error.subject()),
        ("too-many-points", "scenarios")
    );
}

#[test]
fn the_quadratic_bound_is_checked_straight_after_the_instant_budget() {
    // Past the work bound and past the load-byte or frame-byte budget at once: the work bound,
    // checked straight after the instants, wins; past the instants too, the instants win.
    let text = windows("aligned-grid", 30, 1, 2);
    let options = |limits: VerifyLimits| VerifyOptions {
        limits,
        ..VerifyOptions::default()
    };
    let over = VerifyLimits {
        max_total_aligned_grid_work: 1_921,
        ..VerifyLimits::MAX
    };
    for limits in [
        VerifyLimits {
            max_total_load_bytes: 0,
            ..over
        },
        VerifyLimits {
            max_total_frame_bytes: 0,
            ..over
        },
        VerifyLimits {
            max_total_load_bytes: 0,
            max_total_frame_bytes: 0,
            ..over
        },
    ] {
        let error = run_with(&text, &subject("signals"), &options(limits)).expect_err("refused");
        assert_eq!(error.code(), "too-many-points", "{limits:?}");
    }
    let error = run_with(
        &text,
        &subject("signals"),
        &options(VerifyLimits {
            max_total_instants: 61,
            ..over
        }),
    )
    .expect_err("refused");
    assert_eq!(error.code(), "too-many-instants");
}
