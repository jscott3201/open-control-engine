//! `oce verify --commission` through the built binary, one golden transcript per outcome class
//! (commissioning note, "CLI", decisions 21 and 33 to 37; issue #664): the receipt on stdout for
//! a verdict (exit 0 for `pass`, 1 for `fail` and `no_data`), the report for every refusal
//! (exit 2: usage, the spec, the reference, a file, the library's bind and run refusals). Each run
//! starts in a scratch directory holding `oce-verify`'s run fixture (`support::commission`), so
//! every path it prints is relative.
//!
//! **Oracle.** The three verdicts' stdout is, byte for byte, `oce-verify`'s golden receipt of the
//! same inputs, as `tests/replay_gate.rs` holds a vector run to its receipt. Regenerate the
//! transcripts with `OCE_BLESS_CLI=1`; the library's receipts are blessed only in `oce-verify`.

mod support;

use std::fs;

use support::commission::{edit, golden_receipt, run_layout};
use support::{case_in, oce_in};

/// The run fixture's spec, as the transcripts name it.
const SPEC: &str = "spec.json";

// --- verdicts: the receipt on stdout, the library's own bytes ---

#[test]
fn each_verdict_prints_the_librarys_golden_receipt_and_its_exit_status() {
    for (case, exit) in [("pass", 0), ("fail", 1), ("no_data", 1)] {
        let dir = run_layout(&format!("commission_{case}"), case);
        let name = format!("verify_commission_{case}");
        case_in(&dir, &name, exit, &["verify", "--commission", SPEC]);
        let run = oce_in(&dir, &["verify", "--commission", SPEC], b"");
        assert!(
            run.stdout == golden_receipt(case),
            "{case}: stdout is not the library's golden receipt\n{}",
            run.stdout_text()
        );
        assert!(run.stderr.is_empty(), "{case}: {}", run.transcript());
        // A second run gives the same bytes.
        assert_eq!(
            oce_in(&dir, &["verify", "--commission", SPEC], b"").stdout,
            run.stdout
        );
    }
}

#[test]
fn a_verdict_and_a_refusal_render_for_people_on_stderr_and_leave_stdout_alone() {
    let dir = run_layout("commission_render", "fail");
    case_in(
        &dir,
        "verify_commission_render_fail",
        1,
        &["--render", "always", "verify", "--commission", SPEC],
    );
    let never = oce_in(
        &dir,
        &["--render", "never", "verify", "--commission", SPEC],
        b"",
    );
    let always = oce_in(
        &dir,
        &["--render", "always", "verify", "--commission", SPEC],
        b"",
    );
    assert_eq!(never.stdout, always.stdout, "--render changes stderr only");
    assert!(never.stderr.is_empty());
    case_in(
        &dir,
        "verify_commission_render_refused",
        2,
        &[
            "--render",
            "always",
            "verify",
            "--commission",
            "absent.json",
        ],
    );
}

// --- usage: clap's message on stderr, exit 2 ---

#[test]
fn a_vector_flag_beside_commission_and_bounds_without_out_are_usage_errors() {
    let dir = run_layout("commission_usage", "pass");
    case_in(
        &dir,
        "verify_commission_usage_vectors",
        2,
        &["verify", "--commission", SPEC, "vectors.json"],
    );
    case_in(
        &dir,
        "verify_commission_usage_bounds",
        2,
        &["verify", "--commission", SPEC, "--bounds"],
    );
    for args in [
        &[
            "verify",
            "--commission",
            SPEC,
            "--subject",
            "plant.cxf.json",
        ][..],
        &["verify", "--commission", SPEC, "--subject-form", "cxf"][..],
        &["verify", "--commission", SPEC, "--require-mutants-caught"][..],
        &[
            "verify",
            "--reference",
            "0",
            "v.json",
            "--subject",
            "s.cxf.json",
        ][..],
        &[
            "verify",
            "--data",
            "synthetic",
            "v.json",
            "--subject",
            "s.cxf.json",
        ][..],
        &[
            "verify",
            "--bounds",
            "--out",
            "o",
            "v.json",
            "--subject",
            "s.cxf.json",
        ][..],
        &["verify", "--reference", "0"][..],
        &["verify", "--commission", SPEC, "--reference", "-1"][..],
        &["verify", "--commission", SPEC, "--data", "recorded"][..],
        &["verify", "--commission"][..],
    ] {
        let run = oce_in(&dir, args, b"");
        assert_eq!(run.code, 2, "{args:?}: {}", run.transcript());
        assert!(run.stdout.is_empty(), "{args:?}: clap writes stderr only");
        assert!(
            String::from_utf8_lossy(&run.stderr).starts_with("error: "),
            "{args:?}: {}",
            run.transcript()
        );
    }
}

// --- the spec and the reference ---

#[test]
fn a_spec_that_cannot_be_read_or_parsed_is_an_error() {
    let dir = run_layout("commission_spec", "pass");
    case_in(
        &dir,
        "verify_commission_missing_spec",
        2,
        &["verify", "--commission", "absent.json"],
    );
    // A trailing comma, after a comment: refused at its place in the text as written.
    fs::write(
        dir.join("trailing.json"),
        "{\n  // one test\n  \"references\": [],\n}\n",
    )
    .expect("write");
    case_in(
        &dir,
        "verify_commission_malformed_spec",
        2,
        &["verify", "--commission", "trailing.json"],
    );
}

#[test]
fn a_spec_with_several_references_needs_reference_and_lists_them() {
    let dir = run_layout("commission_references", "pass");
    let spec = fs::read_to_string(dir.join(SPEC)).expect("spec");
    let at = spec
        .find(
            r#"    {
      "model": "Plant","#,
        )
        .expect("the reference");
    let end = spec.rfind("\n  ]").expect("the end of the list");
    let reference = &spec[at..end];
    let second = reference.replace(r#""model": "Plant""#, r#""model": "Plant B""#);
    let two = format!("{},\n{second}{}", &spec[..end], &spec[end..]);
    fs::write(dir.join("two.json"), &two).expect("write");
    case_in(
        &dir,
        "verify_commission_reference_required",
        2,
        &["verify", "--commission", "two.json"],
    );
    case_in(
        &dir,
        "verify_commission_reference_out_of_range",
        2,
        &["verify", "--commission", "two.json", "--reference", "2"],
    );
    // Either reference by index: the second is the first with another model label.
    let first = oce_in(
        &dir,
        &["verify", "--commission", "two.json", "--reference", "0"],
        b"",
    );
    let second = oce_in(
        &dir,
        &["verify", "--commission", "two.json", "--reference", "1"],
        b"",
    );
    assert_eq!((first.code, second.code), (0, 0), "{}", second.transcript());
    assert!(second.stdout_text().contains(r#""model": "Plant B""#));
    assert!(second.stdout_text().contains(r#""reference": 1"#));
    // A spec with one reference needs no flag, and takes `--reference 0` to the same bytes.
    let bare = oce_in(&dir, &["verify", "--commission", SPEC], b"");
    let flagged = oce_in(
        &dir,
        &["verify", "--commission", SPEC, "--reference", "0"],
        b"",
    );
    assert_eq!(bare.stdout, flagged.stdout);
}

#[test]
fn a_reference_the_library_cannot_run_is_refused_before_any_other_file_is_read() {
    // No window: the refusal is the library's, and no source is opened (the trend files are
    // gone). A reference with no sampling runs by change of value
    // (`tests/commission_change_of_value.rs`).
    let dir = run_layout("commission_incomplete", "pass");
    for file in support::commission::RUN_FILES {
        fs::remove_file(dir.join(file)).expect("remove");
    }
    fs::remove_file(dir.join("plant.cxf.json")).expect("remove");
    edit(&dir, SPEC, r#""window": { "start": 0, "stop": 30 },"#, "");
    case_in(
        &dir,
        "verify_commission_reference_incomplete",
        2,
        &["verify", "--commission", SPEC],
    );
}

// --- the files the reference names ---

#[test]
fn a_file_that_cannot_be_read_is_an_error_at_the_field_that_names_it() {
    let dir = run_layout("commission_missing_source", "pass");
    fs::remove_file(dir.join("outputs.csv")).expect("remove");
    case_in(
        &dir,
        "verify_commission_missing_source",
        2,
        &["verify", "--commission", SPEC],
    );
    // A `..` segment never reaches the file system: the spec reader refuses the path.
    let dir = run_layout("commission_dotdot_source", "pass");
    edit(
        &dir,
        SPEC,
        r#""path": "inputs.csv""#,
        r#""path": "../inputs.csv""#,
    );
    case_in(
        &dir,
        "verify_commission_dotdot_source",
        2,
        &["verify", "--commission", SPEC],
    );
}

// --- the library's refusals, each under its own code ---

#[test]
fn the_librarys_bind_and_run_refusals_pass_through_with_their_causes() {
    let dir = run_layout("commission_source_refused", "pass");
    edit(
        &dir,
        "inputs.csv",
        "10,21,1,true,2,50",
        "10,21,1,maybe,2,50",
    );
    case_in(
        &dir,
        "verify_commission_source_refused",
        2,
        &["verify", "--commission", SPEC],
    );
    let dir = run_layout("commission_spec_refused", "pass");
    edit(&dir, SPEC, r#""plant.TSet""#, r#""plant.TSetx""#);
    case_in(
        &dir,
        "verify_commission_spec_refused",
        2,
        &["verify", "--commission", SPEC],
    );
    let dir = run_layout("commission_unit_refused", "pass");
    edit(
        &dir,
        "points.json",
        r#"{ "name": "TSet", "unit": "K", "type": "float" }"#,
        r#"{ "name": "TSet", "unit": "degC", "type": "float" }"#,
    );
    case_in(
        &dir,
        "verify_commission_unit_refused",
        2,
        &["verify", "--commission", SPEC],
    );
    let dir = run_layout("commission_subject_refused", "pass");
    fs::write(dir.join("plant.cxf.json"), b"{\"@graph\": []}").expect("write");
    case_in(
        &dir,
        "verify_commission_subject_not_loading",
        2,
        &["verify", "--commission", SPEC],
    );
    // 2000 % is 20, past the `lim` input's declared maximum of 10: the engine refuses the frame.
    let dir = run_layout("commission_frame_refused", "pass");
    edit(
        &dir,
        "inputs.csv",
        "10,21,1,true,2,50",
        "10,21,1,true,2,2000",
    );
    case_in(
        &dir,
        "verify_commission_frame_refused",
        2,
        &["verify", "--commission", SPEC],
    );
}

// --- --out: the report on stdout, the files beside it ---

#[test]
fn out_prints_the_report_and_its_refusals_are_write_failed_or_output_is_input() {
    let dir = run_layout("commission_out_transcripts", "fail");
    fs::create_dir(dir.join("out")).expect("out");
    case_in(
        &dir,
        "verify_commission_out_fail",
        1,
        &["verify", "--commission", SPEC, "--out", "out"],
    );
    // A second run into the same directory: its `outputs/` is there.
    case_in(
        &dir,
        "verify_commission_out_outputs_exist",
        2,
        &["verify", "--commission", SPEC, "--out", "out"],
    );
    case_in(
        &dir,
        "verify_commission_out_missing_dir",
        2,
        &["verify", "--commission", SPEC, "--out", "absent"],
    );
    // A spec named receipt.json, written into its own directory.
    fs::copy(dir.join(SPEC), dir.join("receipt.json")).expect("copy");
    case_in(
        &dir,
        "verify_commission_out_is_input",
        2,
        &["verify", "--commission", "receipt.json", "--out", "."],
    );
}

// --- oce vocab lists every code the verb prints ---

#[test]
fn vocab_lists_every_code_and_cause_the_commissioning_transcripts_print() {
    let vocab = support::oce(&["vocab", "--json"], b"").report();
    let codes = |key: &str| -> Vec<String> {
        vocab[key]
            .as_array()
            .unwrap_or_else(|| panic!("vocab has {key}"))
            .iter()
            .map(|row| row["code"].as_str().expect("a code").to_owned())
            .collect()
    };
    assert_eq!(codes("spec_codes"), oce_verify::SPEC_ERROR_CODES);
    assert_eq!(codes("source_codes"), oce_verify::SOURCE_ERROR_CODES);
    assert_eq!(codes("unit_codes"), oce_verify::UNIT_ERROR_CODES);
    assert_eq!(
        codes("frame_refusal_codes"),
        oce_verify::ENGINE_REFUSAL_CLASSES
    );
    assert_eq!(codes("verify_codes"), oce_verify::VERIFY_ERROR_CODES);
    let tool = codes("error_codes");
    for own in [
        "reference-required",
        "file-outside-directory",
        "output-is-input",
    ] {
        assert!(tool.iter().any(|code| code == own), "{own}");
    }
    let mut known: Vec<String> = Vec::new();
    for key in [
        "error_codes",
        "verify_codes",
        "spec_codes",
        "source_codes",
        "unit_codes",
        "frame_refusal_codes",
    ] {
        known.extend(codes(key));
    }
    let dir = support::crate_dir().join("tests/fixtures/runs");
    let mut seen = 0;
    for entry in fs::read_dir(&dir).expect("runs") {
        let path = entry.expect("entry").path();
        let name = path
            .file_name()
            .expect("a name")
            .to_string_lossy()
            .into_owned();
        if !name.starts_with("verify_commission_") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("a transcript");
        let stdout = text
            .split_once("--- stdout\n")
            .and_then(|(_, rest)| rest.split_once("--- stderr\n"))
            .map(|(stdout, _)| stdout)
            .expect("a transcript");
        let Ok(report) = serde_json::from_str::<serde_json::Value>(stdout) else {
            continue;
        };
        for error in report["errors"].as_array().into_iter().flatten() {
            for key in ["code", "cause"] {
                if let Some(code) = error[key].as_str() {
                    seen += 1;
                    assert!(
                        known.iter().any(|k| k == code),
                        "{name}: {key} {code} is not in vocab"
                    );
                }
            }
        }
    }
    assert!(seen >= 20, "only {seen} codes seen");
}
