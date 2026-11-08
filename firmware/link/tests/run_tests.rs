//! The firmware run on the host: the embedded fixture, every golden, the host reference runner and
//! the refusals (issue #402).
//!
//! The run is the code the bare-metal image executes; here it executes against the host build of
//! the same `no_std` crates. Every golden must reach the run digest
//! `crates/oce-graph/tests/fixtures/blob_frame_digests.tsv` holds for it (the table the facade's
//! dual-path test checks), so the image's `PASS` means it reproduced the facade's run.

use std::process::Command;

use oce_firmware_link::{
    BLOB, DIGEST_TABLE, FIXTURE, PANIC_EXIT_CODE, Sink, TICKS, Verdict, expected_digest, run,
    run_blob,
};

/// Collects a run's lines and counts the `after_frames` calls.
#[derive(Default)]
struct Lines {
    lines: Vec<String>,
    after_frames: u32,
}

impl Sink for Lines {
    fn line(&mut self, bytes: &[u8]) {
        let text = std::str::from_utf8(bytes).expect("ASCII lines");
        let text = text
            .strip_suffix('\n')
            .expect("each line ends in a newline");
        assert!(!text.contains('\n'), "one line per call: {text:?}");
        self.lines.push(text.to_owned());
    }

    fn after_frames(&mut self) {
        self.after_frames += 1;
    }
}

fn golden(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../crates/oce-api/tests/fixtures/blob/{name}.oceb",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn the_embedded_fixture_is_the_checked_in_golden_and_passes() {
    assert_eq!(BLOB, golden(FIXTURE).as_slice());
    let mut lines = Lines::default();
    assert_eq!(run(&mut lines), Verdict::Pass);
    assert_eq!(lines.after_frames, 1);
    assert_eq!(lines.lines.len(), TICKS as usize + 3);
    assert_eq!(lines.lines[0], format!("fixture {FIXTURE} frames {TICKS}"));
    for (k, line) in lines.lines[1..=TICKS as usize].iter().enumerate() {
        let prefix = format!("frame {k} seq {} digest ", k + 1);
        let digest = line
            .strip_prefix(&prefix)
            .unwrap_or_else(|| panic!("{line}"));
        assert_eq!(digest.len(), 16, "{line}");
        assert!(
            digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
    }
    let expected = format!("{:016x}", expected_digest(DIGEST_TABLE, FIXTURE).unwrap());
    let last_frame = &lines.lines[TICKS as usize];
    assert!(last_frame.ends_with(&expected), "{last_frame}");
    assert_eq!(
        lines.lines[TICKS as usize + 1],
        format!("digest {expected} expected {expected}")
    );
    assert_eq!(lines.lines[TICKS as usize + 2], "PASS");
}

#[test]
fn every_golden_reaches_its_table_digest_through_the_firmware_run() {
    let rows: Vec<&str> = DIGEST_TABLE.lines().collect();
    assert_eq!(rows.len(), 46);
    for row in rows {
        let (name, digest) = row.split_once('\t').unwrap();
        let expected = expected_digest(DIGEST_TABLE, name);
        assert_eq!(
            expected.map(|d| format!("{d:016x}")).as_deref(),
            Some(digest)
        );
        let mut lines = Lines::default();
        let verdict = run_blob(name, &golden(name), expected, &mut lines);
        assert_eq!(verdict, Verdict::Pass, "{name}: {:?}", lines.lines.last());
    }
}

#[test]
fn the_host_runner_prints_the_run_and_exits_with_its_verdict() {
    let output = Command::new(env!("CARGO_BIN_EXE_blob-frames"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(Verdict::Pass.exit_code()));
    assert!(output.stderr.is_empty());
    let mut lines = Lines::default();
    run(&mut lines);
    let expected: String = lines.lines.iter().map(|line| format!("{line}\n")).collect();
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
}

#[test]
fn a_wrong_expected_digest_is_a_mismatch_after_every_frame() {
    let right = expected_digest(DIGEST_TABLE, FIXTURE).unwrap();
    let mut lines = Lines::default();
    let verdict = run_blob(FIXTURE, BLOB, Some(right ^ 1), &mut lines);
    assert_eq!(verdict, Verdict::Mismatch);
    assert_eq!(verdict.exit_code(), 1);
    assert_eq!(lines.after_frames, 1);
    assert_eq!(lines.lines.len(), TICKS as usize + 3);
    assert_eq!(lines.lines.last().unwrap(), "MISMATCH");
}

#[test]
fn a_blob_that_does_not_decode_is_a_fault_not_a_panic() {
    let mut truncated = BLOB.to_vec();
    truncated.truncate(BLOB.len() / 2);
    let mut flipped = BLOB.to_vec();
    flipped[BLOB.len() / 2] ^= 0x5a;
    for (label, bytes) in [
        ("truncated", truncated),
        ("flipped", flipped),
        ("empty", Vec::new()),
    ] {
        let mut lines = Lines::default();
        let verdict = run_blob(FIXTURE, &bytes, Some(0), &mut lines);
        assert_eq!(verdict, Verdict::Fault, "{label}");
        assert_eq!(verdict.exit_code(), 3);
        assert_eq!(lines.after_frames, 0, "{label}");
        assert_eq!(
            lines.lines,
            [
                format!("fixture {FIXTURE} frames {TICKS}"),
                "FAULT the blob did not decode".to_owned()
            ],
            "{label}"
        );
    }
}

#[test]
fn a_fixture_the_table_does_not_list_is_a_fault_before_decoding() {
    assert_eq!(expected_digest(DIGEST_TABLE, "no_such_fixture"), None);
    let mut lines = Lines::default();
    assert_eq!(
        run_blob("no_such_fixture", BLOB, None, &mut lines),
        Verdict::Fault
    );
    assert_eq!(
        lines.lines.last().unwrap(),
        "FAULT no digest for the fixture"
    );
}

#[test]
fn the_digest_table_parser_refuses_malformed_rows() {
    let table = "a\t0123456789abcdef\nb\t0123456789ABCDEF\nc\t0123\nd 0123456789abcdef\ne\t0123456789abcdeg\n";
    assert_eq!(expected_digest(table, "a"), Some(0x0123_4567_89ab_cdef));
    assert_eq!(expected_digest(table, "b"), None, "uppercase");
    assert_eq!(expected_digest(table, "c"), None, "short");
    assert_eq!(expected_digest(table, "d"), None, "no tab");
    assert_eq!(expected_digest(table, "e"), None, "not hex");
    assert_eq!(expected_digest(table, ""), None);
    assert_eq!(expected_digest("", "a"), None);
}

#[test]
fn the_exit_codes_are_distinct_and_match_the_emulation_convention() {
    let codes = [
        Verdict::Pass.exit_code(),
        Verdict::Mismatch.exit_code(),
        Verdict::Fault.exit_code(),
        PANIC_EXIT_CODE,
    ];
    assert_eq!(codes, [0, 1, 3, 101]);
}
