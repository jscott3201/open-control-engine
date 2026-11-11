//! `oce verify` and the files around it (vectors note, "CLI": "Forms", "Reads" and "Output"):
//! - `--out DIR` writes the receipt atomically and prints the report, and writes nothing on exit 2;
//! - `--timings` fills the header's timings and never moves `receipt_id`;
//! - a text or wire subject verifies to the receipt of the CXF `oce emit` makes of it;
//! - `rule.cxf.json` is CXF, `rule.json` is wire, and `--subject-form` overrides both;
//! - an over-size subject, vector file or mutant total is refused from its metadata.
//!
//! Each run starts in a scratch directory with the inputs copied in, so every path it prints is
//! relative. No time is asserted (`TESTING.md`).

mod support;

use std::fs;
use std::path::Path;

use serde_json::Value;
use support::{case_in, crate_dir, oce, oce_in, oce_in_guarded, scratch};

const AHU: &str = "../../oce-api/tests/fixtures/library_rules/AHU-0016";

/// Copy the in-repo AHU-0016 rule and its set into `dir`.
fn ahu_into(dir: &Path) {
    let from = crate_dir().join(AHU);
    fs::copy(from.join("vectors.json"), dir.join("vectors.json")).expect("copy the set");
    fs::copy(from.join("rule.cxf.jsonld"), dir.join("rule.cxf.jsonld")).expect("copy the rule");
}

fn receipt_of(dir: &Path) -> Vec<u8> {
    let run = oce_in(
        dir,
        &["verify", "vectors.json", "--subject", "rule.cxf.jsonld"],
        b"",
    );
    assert_eq!(run.code, 0, "{}", run.transcript());
    run.stdout
}

#[test]
fn out_writes_the_receipt_atomically_and_prints_the_report() {
    let dir = scratch("verify_out_pass");
    ahu_into(&dir);
    fs::create_dir(dir.join("out")).expect("the output directory");
    case_in(
        &dir,
        "verify_out_pass",
        0,
        &[
            "verify",
            "vectors.json",
            "--subject",
            "rule.cxf.jsonld",
            "--out",
            "out",
        ],
    );
    let written = fs::read(dir.join("out/receipt.json")).expect("the receipt is written");
    assert_eq!(written, receipt_of(&dir), "the file is the stdout receipt");
    // Only the receipt is left: the temporary file was renamed over it.
    let names: Vec<_> = fs::read_dir(dir.join("out"))
        .expect("out")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(names, ["receipt.json"]);
    // A second run replaces the receipt with the same bytes.
    let again = oce_in(
        &dir,
        &[
            "verify",
            "vectors.json",
            "--subject",
            "rule.cxf.jsonld",
            "--out",
            "out",
        ],
        b"",
    );
    assert_eq!(again.code, 0);
    assert_eq!(
        fs::read(dir.join("out/receipt.json")).expect("again"),
        written
    );
}

#[test]
fn out_with_a_failing_verdict_reports_the_verdict_and_writes_the_receipt() {
    let dir = scratch("verify_out_fail");
    ahu_into(&dir);
    let set = fs::read_to_string(dir.join("vectors.json")).expect("set");
    // `both_open`'s first window ending at 900 s fails, where the persistence delay fires.
    let failing = set.replacen(r#""to_s": 840"#, r#""to_s": 900"#, 1);
    assert_ne!(failing, set);
    fs::write(dir.join("vectors.json"), failing).expect("write");
    fs::create_dir(dir.join("out")).expect("out");
    case_in(
        &dir,
        "verify_out_fail",
        1,
        &[
            "verify",
            "vectors.json",
            "--subject",
            "rule.cxf.jsonld",
            "--out",
            "out",
        ],
    );
    assert!(dir.join("out/receipt.json").is_file());
}

#[test]
fn out_on_a_refusal_writes_nothing() {
    let dir = scratch("verify_out_refused");
    ahu_into(&dir);
    fs::create_dir(dir.join("out")).expect("out");
    case_in(
        &dir,
        "verify_out_refused",
        2,
        &[
            "verify",
            "vectors.json",
            "--subject",
            "missing.cxf.json",
            "--out",
            "out",
        ],
    );
    assert_eq!(fs::read_dir(dir.join("out")).expect("out").count(), 0);
}

#[test]
fn out_into_a_directory_that_does_not_exist_is_write_failed() {
    let dir = scratch("verify_out_missing_dir");
    ahu_into(&dir);
    let run = oce_in(
        &dir,
        &[
            "verify",
            "vectors.json",
            "--subject",
            "rule.cxf.jsonld",
            "--out",
            "absent",
        ],
        b"",
    );
    assert_eq!(run.code, 2, "{}", run.transcript());
    assert_eq!(run.report()["errors"][0]["code"], "write-failed");
    assert!(
        !dir.join("absent").exists(),
        "the directory is never created"
    );
}

#[test]
fn timings_fill_the_header_and_never_move_the_receipt_id() {
    let dir = scratch("verify_timings");
    ahu_into(&dir);
    let parse = |bytes: &[u8]| -> Value { serde_json::from_slice(bytes).expect("a receipt") };
    let plain = parse(&receipt_of(&dir));
    let timed = oce_in(
        &dir,
        &[
            "verify",
            "vectors.json",
            "--subject",
            "rule.cxf.jsonld",
            "--timings",
        ],
        b"",
    );
    assert_eq!(timed.code, 0);
    let timed = parse(&timed.stdout);
    assert_eq!(timed["receipt_id"], plain["receipt_id"]);
    assert_eq!(timed["body"], plain["body"]);
    let timings = &timed["timings"];
    for key in ["load_ms", "run_ms", "compare_ms"] {
        assert!(timings[key].is_u64(), "{key}: {timings}");
    }
    // Comparing cannot be timed apart from running from outside the library (owner ruling).
    assert_eq!(timings["compare_ms"], 0);
    assert_eq!(
        plain["timings"],
        serde_json::json!({ "load_ms": 0, "run_ms": 0, "compare_ms": 0 })
    );
}

#[test]
fn an_authored_subject_verifies_to_the_receipt_of_its_emitted_cxf() {
    let dir = scratch("verify_emitted");
    let inputs = crate_dir().join("tests/fixtures/inputs/verify");
    fs::copy(inputs.join("econ.vectors.json"), dir.join("vectors.json")).expect("set");
    let authored = crate_dir().join("../oce-author/tests/fixtures");
    for (source, name) in [
        ("text/econ_enable.oce", "econ.oce"),
        ("wire/econ_enable.json", "econ.json"),
    ] {
        fs::copy(authored.join(source), dir.join(name)).expect("copy the sequence");
        let emitted = oce_in(&dir, &["emit", name], b"");
        assert_eq!(emitted.code, 0, "{}", emitted.transcript());
        fs::write(dir.join("econ.cxf.json"), &emitted.stdout).expect("write the CXF");
        let from_source = oce_in(&dir, &["verify", "vectors.json", "--subject", name], b"");
        let from_cxf = oce_in(
            &dir,
            &["verify", "vectors.json", "--subject", "econ.cxf.json"],
            b"",
        );
        assert_eq!(from_source.code, 0, "{}", from_source.transcript());
        assert_eq!(from_source.stdout, from_cxf.stdout, "{name}");
    }
}

#[test]
fn the_name_gives_the_form_cxf_before_wire_and_the_flag_overrides_it() {
    let dir = scratch("verify_forms");
    ahu_into(&dir);
    let rule = fs::read(dir.join("rule.cxf.jsonld")).expect("rule");
    fs::write(dir.join("rule.cxf.json"), &rule).expect("cxf.json");
    fs::write(dir.join("rule.json"), &rule).expect("json");
    let verify = |subject: &str, form: Option<&str>| {
        let mut args = vec!["verify", "vectors.json", "--subject", subject];
        if let Some(form) = form {
            args.extend(["--subject-form", form]);
        }
        oce_in(&dir, &args, b"")
    };
    let expected = receipt_of(&dir);
    // `rule.cxf.json` is CXF.
    assert_eq!(verify("rule.cxf.json", None).stdout, expected);
    // `rule.json` is read as the wire form, which a CXF document is not.
    let as_wire = verify("rule.json", None);
    assert_eq!(as_wire.code, 2);
    assert_ne!(
        as_wire.report()["errors"][0]["code"],
        "unknown-subject-form"
    );
    // --subject-form cxf reads it as CXF.
    assert_eq!(verify("rule.json", Some("cxf")).stdout, expected);
}

#[test]
fn an_over_size_subject_is_refused_from_its_metadata() {
    let dir = scratch("verify_big_subject");
    ahu_into(&dir);
    let big = fs::File::create(dir.join("big.cxf.json")).expect("create");
    big.set_len(u64::try_from(oce_api::MAX_CXF_BYTES).expect("fits") + 1)
        .expect("a sparse file one byte past the bound");
    let run = oce_in(
        &dir,
        &["verify", "vectors.json", "--subject", "big.cxf.json"],
        b"",
    );
    assert_eq!(run.code, 2, "{}", run.transcript());
    let error = &run.report()["errors"][0];
    assert_eq!(error["code"], "input-too-large");
    assert_eq!(error["at"], "subject");
    // The metadata's refusal names the size; a refused read could only say "larger than".
    assert!(
        error["detail"]
            .as_str()
            .expect("detail")
            .starts_with("the input is 8388609 bytes"),
        "{error}"
    );
}

#[test]
fn an_over_size_vector_file_is_refused_from_its_metadata() {
    let dir = scratch("verify_big_vectors");
    ahu_into(&dir);
    fs::File::create(dir.join("big.json"))
        .expect("create")
        .set_len(4 * 1024 * 1024 + 1)
        .expect("sparse");
    let run = oce_in(
        &dir,
        &["verify", "big.json", "--subject", "rule.cxf.jsonld"],
        b"",
    );
    assert_eq!(run.code, 2);
    let error = &run.report()["errors"][0];
    assert_eq!(error["code"], "input-too-large");
    assert!(
        error["detail"]
            .as_str()
            .expect("detail")
            .starts_with("the input is 4194305 bytes"),
        "{error}"
    );
}

/// A set on AHU-0016's subject listing `count` mutants named `m<i>.cxf.json`.
fn listing(count: usize) -> String {
    let mutants: Vec<String> = (0..count)
        .map(|i| format!(r#"{{"path": "m{i}.cxf.json"}}"#))
        .collect();
    format!(
        r#"{{"schema": "oce/vectors/v1", "clock": {{"step_s": 60, "horizon_s": 60}}, "mutants": [{}], "scenarios": [{{"name": "s", "inputs": {{"htg_vlv_cmd": 0.0, "clg_vlv_cmd": 0.0}}, "expect": []}}]}}"#,
        mutants.join(", ")
    )
}

#[test]
fn the_mutants_total_is_bounded_from_their_metadata_at_its_boundary() {
    let dir = scratch("verify_mutant_total");
    ahu_into(&dir);
    let eight_mib = u64::try_from(oce_api::MAX_CXF_BYTES).expect("fits");
    let sparse = |name: &str, len: u64| {
        fs::File::create(dir.join(name))
            .expect("create")
            .set_len(len)
            .expect("sparse");
    };
    // Eight files of 8 MiB are exactly 64 MiB: inside the bound, so the CLI reads them and the
    // engine refuses the first (zeros are not CXF).
    for i in 0..8 {
        sparse(&format!("m{i}.cxf.json"), eight_mib);
    }
    fs::write(dir.join("at.json"), listing(8)).expect("set");
    let at = oce_in(
        &dir,
        &["verify", "at.json", "--subject", "rule.cxf.jsonld"],
        b"",
    );
    assert_eq!(at.code, 2);
    assert_eq!(at.report()["errors"][0]["code"], "mutant-load-failed");
    // One byte more in a ninth file passes the bound before it is read.
    sparse("m8.cxf.json", 1);
    fs::write(dir.join("past.json"), listing(9)).expect("set");
    let past = oce_in(
        &dir,
        &["verify", "past.json", "--subject", "rule.cxf.jsonld"],
        b"",
    );
    assert_eq!(past.code, 2);
    let error = &past.report()["errors"][0];
    assert_eq!(error["code"], "too-many-mutant-bytes");
    assert_eq!(error["at"], "mutants[8]");
    // One mutant over 8 MiB is refused as over-size on its own.
    sparse("m0.cxf.json", eight_mib + 1);
    let one = oce_in(
        &dir,
        &["verify", "at.json", "--subject", "rule.cxf.jsonld"],
        b"",
    );
    let error = &one.report()["errors"][0];
    assert_eq!(
        (error["code"].as_str(), error["at"].as_str()),
        (Some("input-too-large"), Some("mutants[0]"))
    );
}

#[test]
fn the_crate_directory_run_needs_no_scratch() {
    // The golden transcripts run from the crate directory; this one checks the support helper
    // the scratch runs share gives the same bytes there.
    let vectors = format!("{AHU}/vectors.json");
    let rule = format!("{AHU}/rule.cxf.jsonld");
    let here = oce(&["verify", &vectors, "--subject", &rule], b"");
    let dir = scratch("verify_same_bytes");
    ahu_into(&dir);
    assert_eq!(here.stdout, receipt_of(&dir));
}

#[cfg(unix)]
#[test]
fn a_named_pipe_is_never_opened_as_the_vector_file_the_subject_or_the_receipt() {
    // A named pipe blocks an open for reading until a writer comes. Each file `verify` reads is
    // judged a regular file from its metadata first, and `--out` writes a fresh temporary file
    // and renames it over the target, so a pipe at `receipt.json` is replaced, never opened. The
    // guard is a hang guard, not an assertion on speed.
    let dir = scratch("verify_named_pipes");
    ahu_into(&dir);
    fs::create_dir(dir.join("out")).expect("out");
    for name in ["pipe.json", "pipe.cxf.json", "out/receipt.json"] {
        let made = std::process::Command::new("mkfifo")
            .arg(dir.join(name))
            .status()
            .expect("mkfifo runs");
        assert!(made.success(), "{name}");
    }
    let guard = std::time::Duration::from_secs(120);
    let run = |args: &[&str]| oce_in_guarded(&dir, args, guard);
    let vectors = run(&["verify", "pipe.json", "--subject", "rule.cxf.jsonld"]);
    assert_eq!(vectors.code, 2);
    assert_eq!(vectors.report()["errors"][0]["code"], "not-a-file");
    let subject = run(&["verify", "vectors.json", "--subject", "pipe.cxf.json"]);
    assert_eq!(subject.code, 2);
    let error = &subject.report()["errors"][0];
    assert_eq!(
        (error["code"].as_str(), error["at"].as_str()),
        (Some("not-a-file"), Some("subject"))
    );
    let out = run(&[
        "verify",
        "vectors.json",
        "--subject",
        "rule.cxf.jsonld",
        "--out",
        "out",
    ]);
    assert_eq!(out.code, 0, "{}", out.transcript());
    let written = fs::symlink_metadata(dir.join("out/receipt.json")).expect("the receipt");
    assert!(written.is_file(), "the pipe was replaced by the receipt");
    assert_eq!(
        fs::read(dir.join("out/receipt.json")).expect("read"),
        receipt_of(&dir)
    );
}
