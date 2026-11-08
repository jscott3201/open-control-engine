//! The round trip's controls: the comparison must fail, and say exactly where, when one bit of a
//! recorded artifact is flipped; the artifact must survive its own writer and reader unchanged; and
//! the reader must refuse malformed, truncated, mutated, incomplete, foreign and oversized input
//! with a typed error instead of panicking. No production code is perturbed: each control feeds a
//! perturbed *recorded* artifact to the same comparison the suite runs.

use std::path::PathBuf;

use oce_api::Value;
use oce_model::EnumClassId;

use crate::artifact::{self, ArtifactError, Recorded};
use crate::trace::{self, FrameRecord, Place, RECORDED_VS_CURRENT, Trace, Warning};
use crate::{Mode, fixture_cxf, mode_from, workspace_root};

/// A fixture whose Real outputs and state words move, for the perturbation controls.
const VARIED: &str = "ahu_economizer";

fn run(name: &str) -> (Trace, Vec<u8>) {
    let (trace, blob, _) = trace::run_facade(&fixture_cxf(name)).unwrap_or_else(|e| panic!("{e}"));
    (trace, blob)
}

/// Writes `trace` through the artifact writer and reads it back through the reader.
fn reparse(name: &str, blob: &[u8], trace: &Trace) -> Result<Recorded, ArtifactError> {
    let text = artifact::render(name, blob, trace);
    artifact::parse(name, "control", text.as_bytes(), blob.to_vec())
}

#[test]
fn a_one_bit_flip_in_a_recorded_output_names_the_fixture_frame_slot_and_both_bit_patterns() {
    let (current, blob) = run(VARIED);
    let clean = reparse(VARIED, &blob, &current).expect("the unperturbed artifact reads back");
    trace::compare(VARIED, RECORDED_VS_CURRENT, &clean.trace, &current)
        .expect("the unperturbed artifact matches");

    let frame = 117;
    let slot = current.frames[frame]
        .outputs
        .iter()
        .position(|value| matches!(value, Value::Real(_)))
        .expect("a Real boundary output");
    let Value::Real(x) = current.frames[frame].outputs[slot] else {
        panic!("slot {slot} is Real");
    };
    let flipped = x.to_bits() ^ 1;
    let mut perturbed = current.clone();
    perturbed.frames[frame].outputs[slot] = Value::Real(f64::from_bits(flipped));
    let recorded = reparse(VARIED, &blob, &perturbed).expect("a perturbed artifact is well formed");

    let mismatch = trace::compare(VARIED, RECORDED_VS_CURRENT, &recorded.trace, &current)
        .expect_err("one flipped bit must fail the comparison");
    assert_eq!(mismatch.place, Place::Output { frame, slot });
    assert_eq!(
        mismatch.to_string(),
        format!(
            "fixture {VARIED}: frame {frame}, output slot {slot} ({}): recorded Real 0x{flipped:016x}, \
             current Real 0x{:016x}",
            current.outputs[slot],
            x.to_bits()
        )
    );
}

#[test]
fn a_one_bit_flip_in_a_recorded_state_word_names_the_frame_and_the_word_index() {
    let (current, blob) = run(VARIED);
    let frame = 42;
    let words = &current.frames[frame].words;
    let index = words.len() / 2;
    assert!(!words.is_empty(), "{VARIED} has state words");
    let flipped = words[index] ^ (1 << 63);
    let mut perturbed = current.clone();
    perturbed.frames[frame].words[index] = flipped;
    let recorded = reparse(VARIED, &blob, &perturbed).expect("a perturbed artifact is well formed");

    let mismatch = trace::compare(VARIED, RECORDED_VS_CURRENT, &recorded.trace, &current)
        .expect_err("one flipped bit must fail the comparison");
    assert_eq!(mismatch.place, Place::Word { frame, index });
    assert_eq!(
        mismatch.to_string(),
        format!(
            "fixture {VARIED}: frame {frame}, state word {index}: recorded 0x{flipped:016x}, \
             current 0x{:016x}",
            words[index]
        )
    );
}

#[test]
fn a_one_bit_flip_in_a_recorded_blob_is_refused_by_its_header_and_named_by_byte_offset() {
    let (current, blob) = run(VARIED);
    let text = artifact::render(VARIED, &blob, &current);
    let offset = blob.len() / 3;
    let mut perturbed = blob.clone();
    perturbed[offset] ^= 0x10;
    let refused = artifact::parse(VARIED, "control", text.as_bytes(), perturbed.clone())
        .expect_err("a blob that differs from its trace header is refused");
    assert!(
        matches!(&refused, ArtifactError::BlobHeader { fixture, .. } if fixture == VARIED),
        "{refused}"
    );
    let mismatch = trace::compare_bytes(VARIED, RECORDED_VS_CURRENT, &perturbed, &blob)
        .expect_err("one flipped bit must fail the byte comparison");
    assert_eq!(mismatch.place, Place::BlobByte { offset });
    assert_eq!(
        mismatch.to_string(),
        format!(
            "fixture {VARIED}: blob byte {offset}: recorded 0x{:02x}, current 0x{:02x}",
            perturbed[offset], blob[offset]
        )
    );
}

/// A trace that uses every value form, every escape and the integer extremes.
fn synthetic() -> (Trace, Vec<u8>) {
    let warning = |source: &str, message: &str, time: f64| Warning {
        source: source.into(),
        message: message.into(),
        time: time.to_bits(),
    };
    let outputs = vec![
        Value::Real(-0.0),
        Value::Real(f64::from_bits(0x7ff8_0000_0000_0001)),
        Value::Integer(i64::MIN),
        Value::Integer(i64::MAX),
        Value::Integer(-1),
        Value::Integer(0),
        Value::Boolean(true),
        Value::String("tab\there\\back\nline\rcr\u{1}\u{7f} é".into()),
        Value::Enum {
            class: EnumClassId(u32::MAX),
            ordinal: 1,
        },
    ];
    let frame = |k: u64, warnings: Vec<Warning>| FrameRecord {
        time: (k as f64 * 0.75).to_bits(),
        sequence: k + 1,
        outputs: outputs.clone(),
        warnings,
        words: vec![0, u64::MAX, k],
    };
    let trace = Trace {
        outputs: (0..outputs.len())
            .map(|slot| format!("urn:oce:point/{slot}\t\\"))
            .collect(),
        initial_words: vec![0, 1, 2],
        frames: vec![
            frame(0, Vec::new()),
            frame(
                1,
                vec![
                    warning("Assert", "x < 1\twas\nfalse", 0.75),
                    warning("", "", -0.0),
                ],
            ),
            frame(2, Vec::new()),
        ],
    };
    (trace, vec![0xa5; 37])
}

#[test]
fn a_rendered_trace_reads_back_to_the_same_run_and_renders_byte_identically() {
    for (name, (trace, blob)) in [("synthetic", synthetic()), (VARIED, run(VARIED))] {
        let text = artifact::render(name, &blob, &trace);
        let recorded = artifact::parse(name, "control", text.as_bytes(), blob.clone())
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        trace::compare(name, RECORDED_VS_CURRENT, &recorded.trace, &trace)
            .unwrap_or_else(|mismatch| panic!("{mismatch}"));
        assert_eq!(recorded.trace.frames.len(), trace.frames.len(), "{name}");
        assert_eq!(
            artifact::render(name, &recorded.blob, &recorded.trace),
            text,
            "{name}"
        );
    }
}

#[test]
fn malformed_trace_files_are_refused_with_their_line() {
    let (trace, blob) = synthetic();
    let text = artifact::render("synthetic", &blob, &trace);
    let w = format!("\t{:016x}\t{:016x}\t{:016x}\n", 0, u64::MAX, 2);
    let cases: Vec<(&str, String, usize)> = vec![
        (
            "a newer format",
            text.replacen("round-trip\t1", "round-trip\t2", 1),
            1,
        ),
        (
            "another fixture",
            text.replacen("fixture\tsynthetic", "fixture\tother", 1),
            2,
        ),
        (
            "no final newline",
            text.trim_end_matches('\n').to_owned(),
            1,
        ),
        ("a blank line", text.replacen("\nwords", "\n\nwords", 1), 14),
        (
            "an extra output",
            text.replacen("outputs\t9", "outputs\t10", 1),
            14,
        ),
        (
            "an output slot out of order",
            text.replacen("output\t1\t", "output\t2\t", 1),
            6,
        ),
        (
            "a huge word count",
            text.replacen("words\t3", "words\t99999999", 1),
            14,
        ),
        (
            "a short load line",
            text.replacen("load\t", "load\t0\t", 1),
            15,
        ),
        ("uppercase hex", text.replacen(&w, &w.to_uppercase(), 1), 26),
        (
            "a skipped frame",
            text.replacen("frame\t1\t", "frame\t7\t", 1),
            19,
        ),
        (
            "a non-canonical sequence",
            text.replacen("\t1\nout", "\t01\nout", 1),
            16,
        ),
        ("a missing value", text.replacen("\tb:1\t", "\t", 1), 17),
        ("a bad Boolean", text.replacen("b:1", "b:2", 1), 17),
        (
            "a negative zero integer",
            text.replacen("i:0\t", "i:-0\t", 1),
            17,
        ),
        (
            "an integer past i64",
            text.replacen("i:9223372036854775807", "i:9223372036854775808", 1),
            17,
        ),
        (
            "an unknown tag",
            text.replacen("e:4294967295", "z:4294967295", 1),
            17,
        ),
        ("an unknown escape", text.replacen("\\t", "\\q", 1), 5),
        ("a short \\x escape", text.replacen("\\x01", "\\x1", 1), 17),
        (
            "a raw control character",
            text.replacen("\\x01", "\u{1}", 1),
            17,
        ),
        (
            "a warning with a missing field",
            text.replacen("\t8000000000000000\nstate", "\nstate", 1),
            22,
        ),
        (
            "a missing state line",
            text.replacen("state", "status", 1),
            18,
        ),
        (
            "a wrong end count",
            text.replacen("end\t3", "end\t4", 1),
            27,
        ),
        ("a line after the end", format!("{text}end\t3\n"), 28),
        ("a truncated file", text[..text.len() / 2].to_owned(), 0),
    ];
    for (what, mutated, line) in cases {
        assert_ne!(mutated, text, "{what}: the mutation applied");
        match artifact::parse("synthetic", "control", mutated.as_bytes(), blob.clone()) {
            Err(ArtifactError::Malformed { line: at, .. }) if line == 0 || at == line => {}
            other => panic!("{what}: expected a refusal at line {line}, got {other:?}"),
        }
    }
    let mut bad_utf8 = text.into_bytes();
    bad_utf8[3] = 0xff;
    assert!(matches!(
        artifact::parse("synthetic", "control", &bad_utf8, blob),
        Err(ArtifactError::Malformed { line: 1, .. })
    ));
}

#[test]
fn every_truncation_and_seeded_byte_mutation_is_refused_or_read_without_panicking() {
    let (trace, blob) = run(VARIED);
    let text = artifact::render(VARIED, &blob, &trace);
    let read = |bytes: &[u8]| artifact::parse(VARIED, "control", bytes, blob.clone());
    // Every proper prefix is refused: the end line and the final newline are the last bytes.
    for cut in (0..text.len()).step_by(97).chain([text.len() - 1]) {
        assert!(
            read(&text.as_bytes()[..cut]).is_err(),
            "prefix of {cut} bytes"
        );
    }
    // Seeded single-byte mutations: refused or read, never a panic.
    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut refused = 0;
    for _ in 0..2000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let mut bytes = text.clone().into_bytes();
        let at = (seed % bytes.len() as u64) as usize;
        bytes[at] = (seed >> 32) as u8;
        refused += usize::from(read(&bytes).is_err());
    }
    assert!(refused > 1000, "{refused} of 2000 mutations refused");
}

/// A fresh directory under the target's test scratch space.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("blob-round-trip-controls")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    dir
}

#[test]
fn an_artifact_directory_with_a_missing_a_foreign_or_an_oversized_file_is_refused() {
    let (trace, blob) = synthetic();
    let fixtures = vec!["first".to_owned(), "second".to_owned()];

    let dir = scratch("complete");
    for name in &fixtures {
        artifact::write(&dir, name, &blob, &trace).unwrap();
    }
    let read = artifact::read_dir(&dir, &fixtures).expect("a complete artifact reads");
    assert_eq!(read.len(), 2);
    assert_eq!(read[1].fixture, "second");
    assert_eq!(read[1].blob, blob);

    let dir = scratch("missing");
    artifact::write(&dir, "first", &blob, &trace).unwrap();
    match artifact::read_dir(&dir, &fixtures) {
        Err(ArtifactError::Missing { path }) => assert!(path.ends_with("second.oceb"), "{path}"),
        other => panic!("expected Missing, got {other:?}"),
    }

    let dir = scratch("foreign");
    for name in &fixtures {
        artifact::write(&dir, name, &blob, &trace).unwrap();
    }
    std::fs::write(dir.join("third.trace.tsv"), "").unwrap();
    assert_eq!(
        artifact::read_dir(&dir, &fixtures).err(),
        Some(ArtifactError::Unexpected {
            name: "third.trace.tsv".into()
        })
    );

    // Another fixture's trace under this fixture's name is refused by its header.
    let dir = scratch("swapped");
    for name in &fixtures {
        artifact::write(&dir, name, &blob, &trace).unwrap();
    }
    std::fs::copy(dir.join("first.trace.tsv"), dir.join("second.trace.tsv")).unwrap();
    assert!(matches!(
        artifact::read_dir(&dir, &fixtures),
        Err(ArtifactError::Malformed { line: 2, .. })
    ));

    let dir = scratch("oversized");
    let path = dir.join("big.oceb");
    std::fs::write(&path, [0_u8; 100]).unwrap();
    assert!(matches!(
        artifact::read_bounded(&path, 99),
        Err(ArtifactError::TooLarge { limit: 99, .. })
    ));
    assert_eq!(artifact::read_bounded(&path, 100).unwrap().len(), 100);
}

#[test]
fn the_mode_is_local_record_or_expect_and_both_variables_at_once_are_refused() {
    let root = workspace_root();
    assert_eq!(mode_from(None, None), Ok(Mode::Local));
    assert_eq!(mode_from(Some("".into()), Some("".into())), Ok(Mode::Local));
    assert_eq!(
        mode_from(Some("target/round-trip".into()), None),
        Ok(Mode::Record(root.join("target/round-trip")))
    );
    assert_eq!(
        mode_from(None, Some("target/round-trip".into())),
        Ok(Mode::Expect(root.join("target/round-trip")))
    );
    let absolute = root.join("elsewhere");
    assert_eq!(
        mode_from(None, Some(absolute.clone().into_os_string())),
        Ok(Mode::Expect(absolute))
    );
    assert!(mode_from(Some("a".into()), Some("b".into())).is_err());
}
