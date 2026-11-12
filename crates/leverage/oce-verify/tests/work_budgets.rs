//! The four work budgets on real runs (the work-budget note, `docs/design/vector-work-budget.md`):
//! - each budget at its bound on the in-repo sets, with and without mutants: the run is accepted,
//!   its receipt byte-identical to one under the defaults; one unit lower, it is refused;
//! - the order of the budgets, and that they come before the subject's parameter trials and every
//!   mutant check;
//! - what the load bytes count;
//! - that no host can raise a budget past its default.

mod support;

use oce_verify::{VectorRun, VerifyError, VerifyLimits, VerifyOptions};

use support::{
    ahu_mutants, library_subject, library_vectors, mutant_set, run_mutants_with, run_with, subject,
};

fn with_limits(limits: VerifyLimits) -> VerifyOptions {
    VerifyOptions {
        limits,
        ..VerifyOptions::default()
    }
}

fn ahu_set() -> String {
    String::from_utf8(library_vectors("AHU-0016")).expect("UTF-8")
}

/// The four budgets set to `frames`, `instants`, `load_bytes` and `frame_bytes`.
fn budgets(frames: u64, instants: u64, load_bytes: u64, frame_bytes: u64) -> VerifyLimits {
    VerifyLimits {
        max_total_frames: frames,
        max_total_instants: instants,
        max_total_load_bytes: load_bytes,
        max_total_frame_bytes: frame_bytes,
        ..VerifyLimits::MAX
    }
}

/// Run `run` under each budget at `exact` and one unit lower: accepted with the receipt the
/// defaults give, then refused with the budget's own code before any frame.
fn pin_at_bound(exact: [u64; 4], run: &dyn Fn(VerifyLimits) -> Result<VectorRun, VerifyError>) {
    let defaults = run(VerifyLimits::MAX)
        .expect("inside every default")
        .receipt;
    let [frames, instants, load_bytes, frame_bytes] = exact;
    let at = run(budgets(frames, instants, load_bytes, frame_bytes)).expect("at every bound");
    assert_eq!(at.receipt.to_canonical_json(), defaults.to_canonical_json());
    for (index, code) in [
        "too-many-frames",
        "too-many-instants",
        "too-many-load-bytes",
        "too-many-frame-bytes",
    ]
    .into_iter()
    .enumerate()
    {
        let mut lowered = exact;
        lowered[index] -= 1;
        let [frames, instants, load_bytes, frame_bytes] = lowered;
        let error = run(budgets(frames, instants, load_bytes, frame_bytes))
            .expect_err("one unit past the bound");
        assert_eq!(
            (error.code(), error.subject()),
            (code, "scenarios"),
            "{error}"
        );
    }
}

#[test]
fn each_budget_admits_the_in_repo_set_at_its_bound_and_refuses_it_one_past() {
    // AHU-0016: 7 scenarios of 31 ticks, 216 instants, a 5,985-byte rule loaded 1 + 0 + 7 times.
    let ahu = library_subject("AHU-0016");
    assert_eq!(ahu.len(), 5_985);
    pin_at_bound([217, 216, 47_880, 1_298_745], &|limits| {
        run_with(&ahu_set(), &ahu, &with_limits(limits))
    });
}

#[test]
fn each_budget_admits_the_mutant_set_at_its_bound_and_refuses_it_one_past() {
    // The same set with three mutants: every mutant counts as if it ran every scenario, and their
    // bytes join the rule's (25,618 bytes together), as the note's table records.
    let ahu = library_subject("AHU-0016");
    let together: usize = ahu.len() + ahu_mutants().iter().map(|(_, b)| b.len()).sum::<usize>();
    assert_eq!(together, 25_618);
    pin_at_bound([868, 864, 204_944, 5_559_106], &|limits| {
        run_mutants_with(&mutant_set(), &ahu, &ahu_mutants(), &with_limits(limits))
    });
}

#[test]
fn the_budgets_are_checked_in_table_order() {
    let ahu = library_subject("AHU-0016");
    let code = |limits| {
        run_with(&ahu_set(), &ahu, &with_limits(limits))
            .expect_err("over a budget")
            .code()
    };
    assert_eq!(code(budgets(0, 0, 0, 0)), "too-many-frames");
    assert_eq!(code(budgets(217, 0, 0, 0)), "too-many-instants");
    assert_eq!(code(budgets(217, 216, 0, 0)), "too-many-load-bytes");
    assert_eq!(code(budgets(217, 216, 47_880, 0)), "too-many-frame-bytes");
}

/// A hysteresis set on a 30-step clock with `scenarios`, each holding `u` at 0.
fn hysteresis_set(params: &[&str]) -> String {
    let scenarios: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(index, body)| {
            format!(
                r#"{{"name": "s{index}", "params": {{{body}}}, "inputs": {{"u": 0.0}}, "expect": [{{"output": "y", "from_s": 0, "to_s": 1800, "equals": false}}]}}"#
            )
        })
        .collect();
    format!(
        r#"{{"schema": "oce/vectors/v1", "clock": {{"step_s": 60, "horizon_s": 1800}}, "scenarios": [{}]}}"#,
        scenarios.join(", ")
    )
}

#[test]
fn an_over_budget_set_is_refused_before_its_parameter_edits_are_tried() {
    // Ruling 6: the subject's band is (1, 2), so uHigh 0.5 is refused in every pass, but each
    // budget, checked first, wins over that refusal.
    let hysteresis = subject("hysteresis");
    let set = hysteresis_set(&[r#""uHigh": 0.5"#]);
    let error = run_with(&set, &hysteresis, &VerifyOptions::default()).expect_err("refused");
    assert_eq!(error.code(), "param-refused", "the control");
    let loads = u64::try_from(hysteresis.len() * 3).expect("fits");
    for (limits, code) in [
        (budgets(30, 31, loads, 31 * 1_576), "too-many-frames"),
        (budgets(31, 30, loads, 31 * 1_576), "too-many-instants"),
        (
            budgets(31, 31, loads - 1, 31 * 1_576),
            "too-many-load-bytes",
        ),
        (
            budgets(31, 31, loads, 31 * 1_576 - 1),
            "too-many-frame-bytes",
        ),
    ] {
        let error = run_with(&set, &hysteresis, &with_limits(limits)).expect_err("over");
        assert_eq!(error.code(), code);
    }
    assert_eq!(
        run_with(
            &set,
            &hysteresis,
            &with_limits(budgets(31, 31, loads, 31 * 1_576))
        )
        .expect_err("inside every budget")
        .code(),
        "param-refused"
    );
}

#[test]
fn an_over_budget_set_loads_no_mutant() {
    // An unloadable mutant is `mutant-load-failed` inside the budgets; each new budget, one
    // unit past, wins over it (the frame budget's case is in `tests/mutant_validation.rs`).
    let ahu = library_subject("AHU-0016");
    let mut broken = ahu_mutants();
    broken[2].1 = b"{".to_vec();
    let together =
        u64::try_from(ahu.len() + broken.iter().map(|(_, bytes)| bytes.len()).sum::<usize>())
            .expect("fits");
    let run = |limits| run_mutants_with(&mutant_set(), &ahu, &broken, &with_limits(limits));
    let exact = [868, 864, together * 8, together * 217];
    let [frames, instants, load_bytes, frame_bytes] = exact;
    assert_eq!(
        run(budgets(frames, instants, load_bytes, frame_bytes))
            .expect_err("an invalid mutant")
            .code(),
        "mutant-load-failed"
    );
    for (limits, code) in [
        (
            budgets(frames, instants - 1, load_bytes, frame_bytes),
            "too-many-instants",
        ),
        (
            budgets(frames, instants, load_bytes - 1, frame_bytes),
            "too-many-load-bytes",
        ),
        (
            budgets(frames, instants, load_bytes, frame_bytes - 1),
            "too-many-frame-bytes",
        ),
    ] {
        assert_eq!(run(limits).expect_err("over").code(), code);
    }
}

#[test]
fn the_load_bytes_count_each_distinct_edit_list_once() {
    // Three scenarios, two distinct lists: the subject is loaded 1 + 2 + 3 times. Counting a
    // trial per scenario would need one load more.
    let hysteresis = subject("hysteresis");
    let set = hysteresis_set(&[r#""uHigh": 3.0"#, r#""uHigh": 3.0"#, r#""uLow": 0.5"#]);
    let loads = u64::try_from(hysteresis.len() * 6).expect("fits");
    let at = |load_bytes| {
        run_with(
            &set,
            &hysteresis,
            &with_limits(budgets(93, 93, load_bytes, 93 * 1_576)),
        )
    };
    assert!(at(loads).is_ok());
    assert_eq!(
        at(loads - 1).expect_err("one byte past").code(),
        "too-many-load-bytes"
    );
    // A host may lower a budget to zero; nothing is then loaded past the subject's first load.
    assert_eq!(at(0).expect_err("zero").code(), "too-many-load-bytes");
}

/// `bytes` padded with trailing spaces to `len` bytes: the same document, larger.
fn padded(bytes: &[u8], len: usize) -> Vec<u8> {
    let mut padded = bytes.to_vec();
    padded.resize(len, b' ');
    padded
}

/// A hysteresis set of `scenarios` scenarios on a clock of `ticks` ticks, each with `windows`
/// whole-clock windows.
fn sized_set(scenarios: usize, ticks: u64, windows: usize) -> String {
    let horizon = (ticks - 1) * 60;
    let expect =
        vec![
            format!(r#"{{"output": "y", "from_s": 0, "to_s": {horizon}, "equals": false}}"#);
            windows
        ]
        .join(",");
    let scenario = format!(r#"{{"name": "s{{}}", "inputs": {{"u": 0.0}}, "expect": [{expect}]}}"#);
    let all: Vec<String> = (0..scenarios)
        .map(|index| scenario.replace("s{}", &format!("s{index}")))
        .collect();
    format!(
        r#"{{"schema": "oce/vectors/v1", "clock": {{"step_s": 60, "horizon_s": {horizon}}}, "scenarios": [{}]}}"#,
        all.join(",")
    )
}

#[test]
fn no_host_can_raise_a_budget_past_its_default() {
    // Each set is one unit past one default. A host asking for u64::MAX gets the defaults, so
    // each is refused before any frame runs, as under the defaults themselves.
    let hysteresis = subject("hysteresis");
    let quarter_mib = padded(&hysteresis, 1 << 18);
    let raised = with_limits(budgets(u64::MAX, u64::MAX, u64::MAX, u64::MAX));
    for (set, subject, code) in [
        // Two scenarios of 2^23 + 1 ticks: 2^24 + 2 frames.
        (
            sized_set(2, (1 << 23) + 1, 0),
            hysteresis.clone(),
            "too-many-frames",
        ),
        // 4,096 whole-clock windows on 65,537 ticks: 268,439,552 instants, past 2^28.
        (
            sized_set(1, 65_537, 4_096),
            hysteresis.clone(),
            "too-many-instants",
        ),
        // A 2^18-byte subject loaded 1 + 0 + 4,096 times: 2^30 + 2^18 bytes.
        (
            sized_set(4_096, 1, 0),
            quarter_mib.clone(),
            "too-many-load-bytes",
        ),
        // One scenario of 2^22 + 1 ticks on a 2^18-byte subject: past 2^40 byte-frames.
        (
            sized_set(1, (1 << 22) + 1, 0),
            quarter_mib.clone(),
            "too-many-frame-bytes",
        ),
    ] {
        for options in [&raised, &VerifyOptions::default()] {
            let error = run_with(&set, &subject, options).expect_err("past a default");
            assert_eq!(
                (error.code(), error.subject()),
                (code, "scenarios"),
                "{error}"
            );
        }
    }
}
