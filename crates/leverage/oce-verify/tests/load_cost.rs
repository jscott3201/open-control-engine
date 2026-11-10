//! What a load costs, in allocations, never in wall-clock time: a refusal stops where it is
//! found, and the memory a load holds is linear in the document, under a pinned factor.
//!
//! Measured with `allocation-counter` (counters local to the measuring thread). The dense shapes
//! are the worst the format has: a map of one-byte constants is the most values per byte, and
//! its entry count just past a power of two is where a growing list wastes the most. The
//! factors below are ceilings with headroom over what was measured (in brackets); the byte bound
//! (`VerifyLimits::max_bytes`, 4 MiB, lowerable) turns them into an absolute bound.

mod support;

use allocation_counter::AllocationInfo;
use oce_verify::{VectorSet, VerifyLimits};

use support::{set_of, set_on};

/// The allocations of one load, and whether it loaded.
fn measured(bytes: &[u8]) -> (AllocationInfo, bool) {
    let mut loaded = false;
    let info = allocation_counter::measure(|| {
        loaded = VectorSet::from_json_bytes(bytes, &VerifyLimits::default()).is_ok();
    });
    (info, loaded)
}

fn length(document: &str) -> u64 {
    u64::try_from(document.len()).expect("fits")
}

/// One scenario whose `inputs` map holds `count` constants under the shortest unique names.
fn constants(count: usize) -> String {
    let entries: Vec<String> = (0..count).map(|index| format!("\"{index:x}\":0")).collect();
    set_of(&format!(
        "{{\"name\":\"s\",\"inputs\":{{{}}},\"expect\":[]}}",
        entries.join(",")
    ))
}

/// One scenario with `lists` step lists of `steps` steps each.
fn step_lists(lists: usize, steps: usize) -> String {
    let list: Vec<String> = (0..steps)
        .map(|t| format!("{{\"t\":{t},\"value\":0}}"))
        .collect();
    let entries: Vec<String> = (0..lists)
        .map(|index| format!("\"{index:x}\":[{}]", list.join(",")))
        .collect();
    set_on(
        "1",
        "65536",
        &format!(
            "{{\"name\":\"s\",\"inputs\":{{{}}},\"expect\":[]}}",
            entries.join(",")
        ),
    )
}

/// `scenarios` scenarios of `windows` windows each.
fn window_lists(scenarios: usize, windows: usize) -> String {
    let list = vec![r#"{"output":"y","from_s":0,"to_s":0,"equals":0}"#; windows].join(",");
    let all: Vec<String> = (0..scenarios)
        .map(|index| format!("{{\"name\":\"{index:x}\",\"inputs\":{{}},\"expect\":[{list}]}}"))
        .collect();
    set_of(&all.join(","))
}

/// A root object of `count` keys the format does not define.
fn unknown_keys(count: usize) -> String {
    let entries: Vec<String> = (0..count).map(|index| format!("\"{index:x}\":0")).collect();
    format!("{{{}}}", entries.join(","))
}

#[test]
fn a_refusal_at_the_first_bytes_costs_the_same_whatever_follows() {
    // Nine open brackets, and four million: the reader stops at the ninth either way, and what
    // it allocates is the refusal.
    let (short, _) = measured("[".repeat(9).as_bytes());
    let (long, loaded) = measured("[".repeat(4_000_000).as_bytes());
    assert!(!loaded);
    assert_eq!(
        (short.count_total, short.bytes_max),
        (long.count_total, long.bytes_max)
    );
    assert!(long.count_total <= 4 && long.bytes_max <= 256, "{long:?}");
    // A document over the byte bound is refused from its length alone.
    let (over, loaded) = measured(&vec![b'['; 5 * 1024 * 1024]);
    assert!(!loaded);
    assert!(over.count_total <= 4 && over.bytes_max <= 256, "{over:?}");
    // A list of 65,537 integers where a set belongs is read to its 65,537th item and no value
    // of it is kept.
    let list = format!("[{}]", vec!["0"; 65_537].join(","));
    let (listed, loaded) = measured(list.as_bytes());
    assert!(!loaded);
    assert!(
        listed.count_total <= 4 && listed.bytes_max <= 256,
        "{listed:?}"
    );
}

#[test]
fn a_map_of_constants_holds_at_most_twenty_four_bytes_per_document_byte() {
    // Entry counts just past a power of two [measured: 18.8, 17.7 and 17.3 bytes per byte], and
    // at one [11.5].
    let mut peaks = Vec::new();
    for count in [16_385, 65_537, 262_145, 262_144] {
        let document = constants(count);
        let (info, loaded) = measured(document.as_bytes());
        assert!(loaded, "{count} constants load");
        assert!(
            info.bytes_max <= 24 * length(&document),
            "{count} constants: {} bytes held for {} bytes read",
            info.bytes_max,
            document.len()
        );
        // The work is linear too: a bounded number of allocations per entry.
        let entries = u64::try_from(count).expect("fits");
        assert!(
            info.count_total <= 3 * entries + 64,
            "{count}: {}",
            info.count_total
        );
        peaks.push(info.bytes_max);
    }
    // Sixteen times the entries holds about sixteen times the memory: no super-linear term.
    let ratio = peaks[2] / peaks[0];
    assert!((12..=20).contains(&ratio), "{peaks:?}");
}

#[test]
fn step_lists_and_windows_hold_at_most_four_bytes_per_document_byte() {
    // [measured: 2.1 and 2.2]
    for (name, document, items) in [
        ("step lists", step_lists(3, 60_000), 180_000_u64),
        ("windows", window_lists(20, 4_096), 81_920),
        ("windows", window_lists(80, 1_000), 80_000),
    ] {
        assert!(
            document.len() > 3 * 1024 * 1024,
            "{name}: near the byte bound"
        );
        let (info, loaded) = measured(document.as_bytes());
        assert!(loaded, "{name}");
        assert!(
            info.bytes_max <= 4 * length(&document),
            "{name}: {} bytes held for {} bytes read",
            info.bytes_max,
            document.len()
        );
        assert!(
            info.count_total <= 16 * items + 1_024,
            "{name}: {}",
            info.count_total
        );
    }
}

#[test]
fn a_document_of_unknown_keys_is_refused_within_eight_bytes_per_document_byte() {
    // The strict-JSON pass remembers an object's keys to refuse a repeat, so a refused document
    // still costs its keys [measured: 5.7]; the shape pass then stops at the first key.
    let document = unknown_keys(400_000);
    let (info, loaded) = measured(document.as_bytes());
    assert!(!loaded);
    assert!(
        info.bytes_max <= 8 * length(&document),
        "{} bytes held for {} bytes read",
        info.bytes_max,
        document.len()
    );
    assert_eq!(support::code_of(&unknown_keys(10)), "unknown-key");
}

#[test]
fn a_clock_at_the_tick_bound_costs_what_a_clock_of_two_ticks_costs() {
    // Nothing is materialised per tick: the cadence, a window and an instant are arithmetic.
    let expectations = |horizon: &str| {
        format!(
            "{{\"output\":\"y\",\"from_s\":0,\"to_s\":{horizon},\"equals\":0}},{{\"diagnostic\":{{\"block\":\"B\"}},\"at_s\":{horizon}}}"
        )
    };
    let scenario = |horizon: &str| {
        format!(
            "{{\"name\":\"s\",\"inputs\":{{\"u\":[{{\"t\":0,\"value\":0}},{{\"t\":{horizon},\"value\":1}}]}},\"expect\":[{}]}}",
            expectations(horizon)
        )
    };
    let (small, small_loaded) = measured(set_on("1", "1", &scenario("1")).as_bytes());
    let (large, large_loaded) = measured(set_on("1", "16777215", &scenario("16777215")).as_bytes());
    assert!(small_loaded && large_loaded);
    assert_eq!(small.count_total, large.count_total);
    // The documents differ by the digits of the horizon, written five times.
    assert!(
        large.bytes_max - small.bytes_max <= 5 * 7 * 2,
        "{small:?} {large:?}"
    );
}
