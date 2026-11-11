//! The golden receipts (vectors note decision 19, "Fixtures"): the two in-repo library rules'
//! pass receipts, read in place from the facade's fixtures, a fail receipt (AHU-0016's
//! `both_open` with its first window ending at 900 s, where the delay fires) and a `no_data`
//! receipt (a scenario whose `expect` is empty), each compared byte for byte. They regenerate
//! only through the crate's bless switch (`OCE_BLESS_VERIFY=1 cargo nextest run -p oce-verify`);
//! every receipt embeds the engine version, so a workspace version bump re-blesses them, and so
//! does #517 when it re-vendors the two rules.
//!
//! Beside the bytes: `receipt_id` is checked against an independent derivation from the golden
//! (the `receipt_id` line and the `timings` block cut out of the canonical text, hashed with a
//! second FNV-1a-128), and it is equal across two runs, across chunk sizes 1, 7 and the whole run,
//! and with and without caller timings.

mod support;

use oce_api::{CompatibilityDescriptor, Engine};
use oce_verify::{
    Failure, ReceiptValue, ScenarioStatus, Timings, VectorInputs, VectorSet, VerifyLimits,
    VerifyOptions, run_vectors,
};

use support::{
    AHU_0016, body, compare_or_bless, fixture, fixture_bytes, library_subject, library_vectors,
};

/// One golden: a set's bytes, its subject's bytes and the golden receipt's file.
struct Golden {
    name: &'static str,
    set: Vec<u8>,
    subject: Vec<u8>,
    receipt: &'static str,
}

fn goldens() -> Vec<Golden> {
    vec![
        Golden {
            name: "AHU-0016",
            set: library_vectors("AHU-0016"),
            subject: library_subject("AHU-0016"),
            receipt: "receipts/AHU-0016.receipt.json",
        },
        Golden {
            name: "AHU-0004",
            set: library_vectors("AHU-0004"),
            subject: library_subject("AHU-0004"),
            receipt: "receipts/AHU-0004.receipt.json",
        },
        Golden {
            name: "both_open_900",
            set: fixture_bytes("receipts/both_open_900.vectors.json"),
            subject: library_subject("AHU-0016"),
            receipt: "receipts/both_open_900.receipt.json",
        },
        Golden {
            name: "empty_expect",
            set: fixture_bytes("receipts/empty_expect.vectors.json"),
            subject: library_subject("AHU-0016"),
            receipt: "receipts/empty_expect.receipt.json",
        },
    ]
}

fn set_of(golden: &Golden) -> VectorSet {
    VectorSet::from_json_bytes(&golden.set, &VerifyLimits::default())
        .unwrap_or_else(|error| panic!("{}: {error}", golden.name))
}

fn run(golden: &Golden, options: &VerifyOptions) -> oce_verify::Receipt {
    run_vectors(
        &set_of(golden),
        &VectorInputs::new(&golden.subject),
        options,
    )
    .unwrap_or_else(|error| {
        panic!(
            "{}: {} at {}: {error}",
            golden.name,
            error.code(),
            error.subject()
        )
    })
    .receipt
}

/// FNV-1a-128, written again here so `receipt_id` is checked against something other than the
/// crate's own hash.
fn reference_hash(bytes: &[u8]) -> u128 {
    let mut hash = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d_u128;
    for byte in bytes {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    hash
}

/// The canonical receipt `text` with its `receipt_id` line and its `timings` block removed, by
/// text alone: the header's keys sort `receipt_id` between `passed` and `schema`, and `timings`
/// last, so the line before `timings` loses its comma.
fn without_identity(text: &str) -> (String, String) {
    let mut lines: Vec<&str> = text.lines().collect();
    let id_line = lines
        .iter()
        .position(|line| line.starts_with("  \"receipt_id\": "))
        .expect("a receipt_id line");
    let id = lines.remove(id_line);
    let id = id
        .trim_start_matches("  \"receipt_id\": \"")
        .trim_end_matches("\",")
        .to_owned();
    let start = lines
        .iter()
        .position(|line| *line == "  \"timings\": {")
        .expect("a timings block");
    let end = start
        + lines[start..]
            .iter()
            .position(|line| *line == "  }")
            .expect("the timings block closes");
    assert_eq!(lines[end + 1], "}", "timings is the last header key");
    lines.drain(start..=end);
    let before = lines[start - 1].strip_suffix(',').expect("a comma to drop");
    lines[start - 1] = before;
    let mut reduced = lines.join("\n");
    reduced.push('\n');
    (reduced, id)
}

#[test]
fn each_receipt_is_its_golden_byte_for_byte() {
    for golden in goldens() {
        let written = run(&golden, &VerifyOptions::default()).to_canonical_json();
        compare_or_bless(&fixture(golden.receipt), &written);
    }
}

#[test]
fn each_golden_receipt_id_is_the_reference_hash_of_its_bytes_without_timings_and_id() {
    for golden in goldens() {
        let text = String::from_utf8(fixture_bytes(golden.receipt)).expect("ASCII");
        let (reduced, id) = without_identity(&text);
        assert_eq!(
            id,
            format!(
                "receipt:fnv1a128:{:032x}",
                reference_hash(reduced.as_bytes())
            ),
            "{}",
            golden.name
        );
        let receipt = run(&golden, &VerifyOptions::default());
        assert_eq!(receipt.header.receipt_id, id, "{}", golden.name);
        assert_eq!(receipt.computed_receipt_id(), id, "{}", golden.name);
    }
}

#[test]
fn the_receipt_id_is_one_across_runs_chunk_sizes_and_timings() {
    let timed = Timings {
        load_ms: 4,
        run_ms: 61,
        compare_ms: 9,
    };
    for golden in goldens() {
        let reference = run(&golden, &VerifyOptions::default());
        let bytes = reference.to_canonical_json();
        for chunk_frames in [1, 7, usize::MAX, 0] {
            let options = VerifyOptions {
                chunk_frames,
                ..VerifyOptions::default()
            };
            let again = run(&golden, &options);
            assert_eq!(
                again.to_canonical_json(),
                bytes,
                "{} in chunks of {chunk_frames}",
                golden.name
            );
        }
        let with_timings = run(
            &golden,
            &VerifyOptions {
                timings: Some(timed),
                ..VerifyOptions::default()
            },
        );
        assert_eq!(with_timings.header.receipt_id, reference.header.receipt_id);
        assert_eq!(with_timings.header.timings, timed);
        assert_eq!(reference.header.timings, Timings::default(), "zeros");
        let text = with_timings.to_canonical_json();
        assert!(text.contains("\"run_ms\": 61"), "{}", golden.name);
        // Only the timings differ.
        assert_eq!(
            text.replace("\"compare_ms\": 9", "\"compare_ms\": 0")
                .replace("\"load_ms\": 4", "\"load_ms\": 0")
                .replace("\"run_ms\": 61", "\"run_ms\": 0"),
            bytes,
            "{}",
            golden.name
        );
    }
}

#[test]
fn the_header_names_the_subject_as_loaded_the_set_and_the_engine() {
    for golden in goldens() {
        let receipt = run(&golden, &VerifyOptions::default());
        let mut engine = Engine::new();
        engine.load_cxf(&golden.subject).expect("the subject loads");
        let header = &receipt.header;
        assert_eq!(header.schema, "oce/receipt/v1");
        assert_eq!(header.kind.as_str(), "vectors");
        assert_eq!(header.comparison.mode.as_str(), "exact");
        assert_eq!(
            header.content_id,
            engine.content_id().expect("an id").to_string()
        );
        assert_eq!(header.spec_id, set_of(&golden).spec_id());
        assert_eq!(
            header.engine_version,
            CompatibilityDescriptor::current(None).oce_api_version()
        );
        assert_eq!(header.engine_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(header.passed, body(&receipt).status == ScenarioStatus::Pass);
    }
}

#[test]
fn the_pass_receipts_count_what_the_note_records() {
    let goldens = goldens();
    let ahu_0016 = run(&goldens[0], &VerifyOptions::default());
    let totals = body(&ahu_0016).totals;
    assert!(ahu_0016.header.passed);
    assert_eq!((totals.scenarios.total, totals.scenarios.pass), (7, 7));
    assert_eq!(
        (
            totals.frames.executed,
            totals.frames.substituted,
            totals.frames.refused
        ),
        (217, 0, 0)
    );
    assert_eq!(
        (
            totals.instants.checked,
            totals.instants.passed,
            totals.instants.failed
        ),
        (216, 216, 0)
    );
    assert_eq!((totals.failures, totals.warnings), (0, 0));
    let ahu_0004 = run(&goldens[1], &VerifyOptions::default());
    let totals = body(&ahu_0004).totals;
    assert!(ahu_0004.header.passed);
    assert_eq!((totals.scenarios.total, totals.scenarios.pass), (9, 9));
    assert_eq!(totals.frames.executed, 441);
    assert_eq!(totals.instants.failed, 0);
    for receipt in [&ahu_0016, &ahu_0004] {
        assert!(body(receipt).load_warnings.is_empty());
        for scenario in &body(receipt).scenarios {
            assert_eq!(scenario.status, ScenarioStatus::Pass, "{}", scenario.name);
            assert!(scenario.params.is_empty() && scenario.content_id.is_none());
        }
    }
}

#[test]
fn the_fail_receipt_records_the_delay_firing_at_900_seconds() {
    let receipt = run(&goldens()[2], &VerifyOptions::default());
    assert!(!receipt.header.passed);
    let body = body(&receipt);
    assert_eq!(body.status, ScenarioStatus::Fail);
    assert_eq!(
        (body.totals.scenarios.pass, body.totals.scenarios.fail),
        (6, 1)
    );
    let both_open = &body.scenarios[2];
    assert_eq!(both_open.name, "both_open");
    assert_eq!(both_open.status, ScenarioStatus::Fail);
    assert_eq!(
        (
            both_open.instants.checked,
            both_open.instants.passed,
            both_open.instants.failed
        ),
        (31, 30, 1)
    );
    assert_eq!(both_open.failures_total, 1);
    let Failure::ValueMismatch {
        expectation,
        output,
        t,
        expected,
        actual,
    } = &both_open.failures[0]
    else {
        panic!("a value mismatch: {:?}", both_open.failures);
    };
    assert_eq!(*expectation, 0);
    assert_eq!(output, &format!("{AHU_0016}yFault"));
    assert_eq!(t.to_bits(), 900.0_f64.to_bits());
    assert!(expected.bit_eq(&ReceiptValue::Boolean(false)));
    assert!(actual.bit_eq(&ReceiptValue::Boolean(true)));
}

#[test]
fn the_no_data_receipt_ran_every_frame_and_checked_nothing() {
    let receipt = run(&goldens()[3], &VerifyOptions::default());
    assert!(!receipt.header.passed);
    let body = body(&receipt);
    assert_eq!(body.status, ScenarioStatus::NoData);
    let scenario = &body.scenarios[0];
    assert_eq!(scenario.status, ScenarioStatus::NoData);
    assert_eq!(scenario.expectations, 0);
    assert_eq!(scenario.frames.executed, 31);
    assert_eq!(scenario.instants.checked, 0);
}

#[test]
fn the_fail_set_is_the_in_place_set_with_one_window_edited() {
    let goldens = goldens();
    let (in_place, edited) = (set_of(&goldens[0]), set_of(&goldens[2]));
    assert_eq!(in_place.clock(), edited.clock());
    assert_eq!(in_place.scenarios().len(), edited.scenarios().len());
    for (index, (left, right)) in in_place
        .scenarios()
        .iter()
        .zip(edited.scenarios())
        .enumerate()
    {
        if index != 2 {
            assert_eq!(left, right, "scenario {index} is unchanged");
            continue;
        }
        assert_eq!(left.inputs(), right.inputs());
        assert_eq!(left.expectations()[1], right.expectations()[1]);
        let (oce_verify::Expectation::Window(before), oce_verify::Expectation::Window(after)) =
            (&left.expectations()[0], &right.expectations()[0])
        else {
            panic!("both are windows");
        };
        assert_eq!(before.output(), after.output());
        assert_eq!(before.from_s().to_bits(), after.from_s().to_bits());
        assert_eq!(before.equals(), after.equals());
        assert_eq!(
            (before.to_s().to_bits(), after.to_s().to_bits()),
            (840.0_f64.to_bits(), 900.0_f64.to_bits())
        );
    }
}

#[test]
fn the_hashed_part_covers_the_engine_version_and_never_the_timings() {
    let golden = &goldens()[0];
    let issued = run(golden, &VerifyOptions::default());
    let mut edited = issued.clone();
    edited.header.timings.run_ms = 12_345;
    edited.header.receipt_id = "receipt:fnv1a128:0".to_owned();
    assert_eq!(edited.computed_receipt_id(), issued.header.receipt_id);
    // Two engine versions never issue one receipt id (decision 33).
    let mut other_version = issued.clone();
    other_version.header.engine_version = "0.5.1".to_owned();
    assert_ne!(
        other_version.computed_receipt_id(),
        issued.header.receipt_id
    );
    let mut other_subject = issued.clone();
    other_subject.header.passed = !other_subject.header.passed;
    assert_ne!(
        other_subject.computed_receipt_id(),
        issued.header.receipt_id
    );
}

#[test]
fn kept_traces_are_one_per_scenario_in_file_order_and_change_no_receipt_byte() {
    let golden = &goldens()[0];
    let set = set_of(golden);
    let kept = run_vectors(
        &set,
        &VectorInputs::new(&golden.subject),
        &VerifyOptions {
            keep_traces: true,
            ..VerifyOptions::default()
        },
    )
    .expect("a receipt");
    let names: Vec<&str> = kept.traces.iter().map(|(name, _)| name.as_str()).collect();
    let expected: Vec<&str> = set.scenarios().iter().map(|s| s.name()).collect();
    assert_eq!(names, expected);
    for (name, trace) in &kept.traces {
        assert_eq!(trace.rows(), 31, "{name}");
        assert!(trace.is_clean(), "{name}");
        #[expect(clippy::cast_precision_loss, reason = "small tick indexes")]
        let ticks: Vec<u64> = (0..31_u64).map(|k| (k as f64 * 60.0).to_bits()).collect();
        let times: Vec<u64> = trace.times().iter().map(|t| t.to_bits()).collect();
        assert_eq!(times, ticks, "{name}: one frame per tick, the horizon last");
    }
    let plain = run(golden, &VerifyOptions::default());
    assert!(
        run_vectors(
            &set,
            &VectorInputs::new(&golden.subject),
            &VerifyOptions::default()
        )
        .expect("a receipt")
        .traces
        .is_empty()
    );
    assert_eq!(kept.receipt.to_canonical_json(), plain.to_canonical_json());
}
