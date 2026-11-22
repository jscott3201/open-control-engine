//! `oce diff` against `oce-diff` and against itself (diff note, "CLI" and "Test plan"; GitHub
//! issue #668).
//!
//! - **Oracle.** For every worked example, in the text form and as CXF, the diff the binary's
//!   report carries is `oce-diff`'s golden byte for byte once the envelope is removed, the
//!   Markdown on stdout is its table golden and the stderr rendering its text golden. The wire
//!   form's diff is the library's diff of the two wire documents.
//! - **Exit.** `0` exactly when the diff is `equal`; a `reordered`-only pair is `1` with
//!   `graph_equal` true; `--no-renames` adds `rename-pass-off` and leaves `graph_equal` alone; an
//!   invalid draft that parses is diffed.
//! - **Determinism.** Two runs give the same bytes; stdin and a file give the same diff.
//! - **Bounds.** Each side is read at its form's bound, one byte over is refused about that side,
//!   and `rule.cxf.json` is CXF.
//! - **Vocabulary.** Every code a diff run reports is one `oce vocab` lists.

mod support;

use std::fs::File;

use oce_author::{MAX_TEXT_BYTES, from_wire_bytes, parse_text};
use oce_diff::{DiffOptions, diff};
use serde_json::Value;
use support::diff_inputs::{
    EXAMPLES, PAIRS, carried_diff, emitted, example, example_forms, golden, own, text_of, write,
};
use support::{Run, oce, oce_in, scratch};

/// The exit a diff golden calls for: `0` when it is `equal`, else `1`.
fn expected_exit(golden_json: &str) -> i32 {
    let diff: Value = serde_json::from_str(golden_json).expect("a diff golden is JSON");
    if diff["summary"]["equal"] == true {
        0
    } else {
        1
    }
}

/// The one refusal of an exit-2 run: its code and `at`.
fn refusal(run: &Run) -> (String, Value) {
    assert_eq!(run.code, 2, "{}", run.transcript());
    let report = run.report();
    assert_eq!(report["ok"], false);
    assert!(report.get("diff").is_none(), "a refusal carries no diff");
    let errors = report["errors"].as_array().expect("errors");
    assert_eq!(errors.len(), 1, "{}", run.transcript());
    (
        errors[0]["code"].as_str().expect("a code").to_owned(),
        errors[0].get("at").cloned().unwrap_or(Value::Null),
    )
}

#[test]
fn the_text_forms_diff_is_oce_diffs_golden_once_the_envelope_is_removed() {
    for name in PAIRS {
        let (base, head) = (example(name, "base"), example(name, "head"));
        let json = golden(name, "diff.json");
        let run = oce(&["diff", &base, &head], b"");
        assert_eq!(run.code, expected_exit(&json), "{}", run.transcript());
        assert_eq!(carried_diff(&run), json, "{name}");
        assert!(run.stderr.is_empty(), "{name}");
        let report = run.report();
        assert_eq!(report["form"], "text", "{name}");
        assert_eq!(report["input"], base.as_str(), "{name}");
        assert_eq!(report["head"], head.as_str(), "{name}");
        assert!(
            report.get("stages").is_none(),
            "{name}: nothing is validated"
        );

        let table = oce(&["diff", &base, &head, "--format", "markdown"], b"");
        assert_eq!(table.code, run.code, "{name}");
        assert_eq!(table.stdout_text(), golden(name, "diff.md"), "{name}");
        let rendered = oce(&["diff", &base, &head, "--render", "always"], b"");
        assert_eq!(
            rendered.stdout, run.stdout,
            "{name}: --render moves no stdout byte"
        );
        let stderr = String::from_utf8(rendered.stderr).expect("UTF-8");
        assert_eq!(stderr, golden(name, "diff.txt"), "{name}");
    }
}

#[test]
fn the_cxf_forms_diff_is_oce_diffs_cxf_golden_once_the_envelope_is_removed() {
    let dir = example_forms("diff_files_cxf");
    for name in EXAMPLES {
        let (base, head) = (format!("{name}.base.jsonld"), format!("{name}.head.jsonld"));
        let json = golden(name, "cxf.diff.json");
        let run = oce_in(&dir, &["diff", &base, &head], b"");
        assert_eq!(run.code, expected_exit(&json), "{}", run.transcript());
        assert_eq!(carried_diff(&run), json, "{name}");
        assert_eq!(run.report()["form"], "cxf", "{name}");
        let table = oce_in(&dir, &["diff", &base, &head, "--format", "markdown"], b"");
        assert_eq!(table.stdout_text(), golden(name, "cxf.diff.md"), "{name}");
        let rendered = oce_in(&dir, &["diff", &base, &head, "--render", "always"], b"");
        let stderr = String::from_utf8(rendered.stderr).expect("UTF-8");
        assert_eq!(stderr, golden(name, "cxf.diff.txt"), "{name}");
    }
}

#[test]
fn the_wire_forms_diff_is_the_librarys_diff_of_the_two_wire_documents() {
    let dir = example_forms("diff_files_wire");
    for name in EXAMPLES {
        let (base, head) = (format!("{name}.base.json"), format!("{name}.head.json"));
        let read = |file: &str| {
            from_wire_bytes(&std::fs::read(dir.join(file)).expect("read")).expect("wire reads")
        };
        let expected = diff(&read(&base), &read(&head), &DiffOptions::default());
        let run = oce_in(&dir, &["diff", &base, &head], b"");
        assert_eq!(carried_diff(&run), expected.to_json(), "{name}");
        assert_eq!(run.code, i32::from(!expected.summary.equal), "{name}");
        assert_eq!(run.report()["form"], "wire", "{name}");
    }
}

#[test]
fn a_name_ending_in_cxf_json_is_read_as_cxf() {
    let dir = example_forms("diff_files_cxf_json");
    let bytes = |side: &str| std::fs::read(dir.join(format!("plant_reset.{side}.jsonld")));
    write(&dir, "base.cxf.json", &bytes("base").expect("read"));
    write(&dir, "head.cxf.json", &bytes("head").expect("read"));
    let run = oce_in(&dir, &["diff", "base.cxf.json", "head.cxf.json"], b"");
    assert_eq!(run.code, 1, "{}", run.transcript());
    assert_eq!(run.report()["form"], "cxf");
    assert_eq!(carried_diff(&run), golden("plant_reset", "cxf.diff.json"));
    // The two CXF names agree with each other; a `.cxf.json` beside a wire name does not.
    let mixed = oce_in(
        &dir,
        &["diff", "base.cxf.json", "plant_reset.head.json"],
        b"",
    );
    assert_eq!(refusal(&mixed), ("mixed-forms".to_owned(), Value::Null));
    // --from overrides the name: CXF read as wire is the wire reader's refusal of the base.
    let forced = oce_in(
        &dir,
        &["diff", "base.cxf.json", "head.cxf.json", "--from", "wire"],
        b"",
    );
    assert_eq!(refusal(&forced).1, "base");
}

#[test]
fn exit_zero_means_equal_and_a_reordered_only_pair_is_one_with_the_same_structure() {
    let reference = own("reference.oce");
    let same = oce(&["diff", &reference, &reference], b"");
    assert_eq!(same.code, 0, "{}", same.transcript());
    assert_eq!(same.report()["ok"], true);
    assert_eq!(same.report()["diff"]["summary"]["equal"], true);

    let run = oce(&["diff", &reference, &own("reordered.oce")], b"");
    assert_eq!(run.code, 1, "{}", run.transcript());
    let summary = &run.report()["diff"]["summary"];
    assert_eq!(summary["equal"], false);
    for flag in [
        "graph_equal",
        "interface_equal",
        "parameters_equal",
        "metadata_equal",
    ] {
        assert_eq!(summary[flag], true, "{flag}");
    }
    let graph = run.report()["diff"]["graph"].clone();
    let entries = graph.as_array().expect("a graph section");
    assert!(!entries.is_empty());
    assert!(entries.iter().all(|entry| entry["kind"] == "reordered"));
}

#[test]
fn no_renames_adds_its_note_and_leaves_graph_equal_alone() {
    for name in PAIRS {
        let (base, head) = (example(name, "base"), example(name, "head"));
        let on = oce(&["diff", &base, &head], b"").report();
        let off = oce(&["diff", &base, &head, "--no-renames"], b"").report();
        let notes = off["diff"]["notes"].as_array().expect("notes").clone();
        assert!(notes.contains(&Value::from("rename-pass-off")), "{name}");
        assert!(
            !on["diff"]["notes"]
                .as_array()
                .expect("notes")
                .contains(&Value::from("rename-pass-off"))
        );
        assert_eq!(
            on["diff"]["summary"]["graph_equal"], off["diff"]["summary"]["graph_equal"],
            "{name}"
        );
        assert_eq!(off["diff"]["summary"]["blocks"]["renamed"], 0, "{name}");
    }
}

#[test]
fn an_invalid_draft_that_parses_is_diffed_as_the_library_diffs_it() {
    let (reference, draft) = (own("reference.oce"), own("draft.oce"));
    let validated = oce(&["validate", "--author-side", &draft], b"");
    assert_eq!(
        validated.code,
        1,
        "the draft is invalid: {}",
        validated.transcript()
    );
    let run = oce(&["diff", &reference, &draft], b"");
    assert_eq!(run.code, 1, "{}", run.transcript());
    let parse = |file: &str| parse_text(&text_of(file)).expect("parses");
    let expected = diff(&parse(&reference), &parse(&draft), &DiffOptions::default());
    assert_eq!(carried_diff(&run), expected.to_json());
}

#[test]
fn two_runs_and_stdin_against_a_file_give_the_same_bytes() {
    for name in PAIRS {
        let (base, head) = (example(name, "base"), example(name, "head"));
        for format in ["json", "markdown"] {
            let args = [
                "diff", &base, &head, "--format", format, "--render", "always",
            ];
            let (first, second) = (oce(&args, b""), oce(&args, b""));
            assert_eq!(first.stdout, second.stdout, "{name} {format}");
            assert_eq!(first.stderr, second.stderr, "{name} {format}");
            assert_eq!(first.code, second.code, "{name} {format}");
        }
        let file = oce(&["diff", &base, &head], b"");
        let piped = oce(
            &["diff", &base, "-", "--from", "text"],
            text_of(&head).as_bytes(),
        );
        assert_eq!(carried_diff(&piped), carried_diff(&file), "{name}");
        assert_eq!(piped.report()["head"], "-");
        let piped = oce(
            &["diff", "-", &head, "--from", "text"],
            text_of(&base).as_bytes(),
        );
        assert_eq!(carried_diff(&piped), carried_diff(&file), "{name}");
        assert_eq!(piped.report()["input"], "-");
    }
}

#[test]
fn each_side_is_read_at_its_forms_bound_and_refused_one_byte_over() {
    let dir = scratch("diff_files_bounds");
    // A parseable text of exactly the bound: comment lines, then one shorter.
    let mut at = b"sequence s\nderived_from none\n".to_vec();
    while at.len() < MAX_TEXT_BYTES {
        let room = (MAX_TEXT_BYTES - at.len()).min(8_000);
        at.push(b'#');
        at.extend(std::iter::repeat_n(b'x', room.saturating_sub(2)));
        at.push(b'\n');
        at.truncate(MAX_TEXT_BYTES);
    }
    assert_eq!(at.len(), MAX_TEXT_BYTES);
    write(&dir, "at.oce", &at);
    let mut over = at.clone();
    over.push(b'\n');
    write(&dir, "over.oce", &over);
    let run = oce_in(&dir, &["diff", "at.oce", "at.oce"], b"");
    assert_eq!(run.code, 0, "the bound is read: {}", run.transcript());
    let too_large = |at: &str| ("input-too-large".to_owned(), Value::from(at));
    assert_eq!(
        refusal(&oce_in(&dir, &["diff", "over.oce", "at.oce"], b"")),
        too_large("base")
    );
    assert_eq!(
        refusal(&oce_in(&dir, &["diff", "at.oce", "over.oce"], b"")),
        too_large("head")
    );
    let piped = oce_in(&dir, &["diff", "at.oce", "-", "--from", "text"], &over);
    assert_eq!(refusal(&piped), too_large("head"));
    // A CXF side is read at 8 MiB, from its metadata: a sparse file one byte over.
    let cxf = emitted(&text_of(&own("reference.oce")));
    write(&dir, "small.jsonld", &cxf);
    File::create(dir.join("over.jsonld"))
        .and_then(|file| file.set_len(oce_api::MAX_CXF_BYTES as u64 + 1))
        .expect("a sparse file");
    let run = oce_in(&dir, &["diff", "small.jsonld", "over.jsonld"], b"");
    assert_eq!(refusal(&run), too_large("head"));
    let detail = run.report()["errors"][0]["detail"].clone();
    assert_eq!(
        detail,
        format!(
            "the input is {} bytes; at most {} are read",
            oce_api::MAX_CXF_BYTES + 1,
            oce_api::MAX_CXF_BYTES
        )
    );
    // The same name read as text is held to the text bound instead.
    let as_text = oce_in(
        &dir,
        &["diff", "over.jsonld", "at.oce", "--from", "text"],
        b"",
    );
    assert_eq!(refusal(&as_text), too_large("base"));
}

#[test]
fn every_code_a_diff_run_reports_is_listed_by_vocab() {
    let vocab = oce(&["vocab", "--json"], b"").report();
    let listed: Vec<String> = ["error_codes", "parse_codes", "wire_codes", "diff_codes"]
        .iter()
        .flat_map(|table| vocab[*table].as_array().expect("a table").clone())
        .map(|row| row["code"].as_str().expect("a code").to_owned())
        .collect();
    let listed_diff: Vec<&str> = vocab["diff_codes"]
        .as_array()
        .expect("diff_codes")
        .iter()
        .map(|row| row["code"].as_str().expect("a code"))
        .collect();
    assert_eq!(listed_diff, oce_diff::DiffError::CODES);
    let reference = own("reference.oce");
    let runs = [
        oce(&["diff", "-", "-", "--from", "text"], b""),
        oce(
            &[
                "diff",
                &reference,
                "../oce-author/tests/fixtures/wire/econ_enable.json",
            ],
            b"",
        ),
        oce(&["diff", "-", &reference], b""),
        oce(
            &["diff", &reference, "tests/fixtures/inputs/plain.txt"],
            b"",
        ),
        oce(&["diff", &reference, "missing.oce"], b""),
        oce(
            &["diff", &reference, "tests/fixtures/inputs/malformed.oce"],
            b"",
        ),
        oce(
            &[
                "diff",
                &reference,
                "tests/fixtures/inputs/malformed.json",
                "--from",
                "wire",
            ],
            b"",
        ),
        oce(
            &[
                "diff",
                &reference,
                "tests/fixtures/inputs/verify/unknown_class.cxf.json",
                "--from",
                "cxf",
            ],
            b"",
        ),
    ];
    let mut seen = Vec::new();
    for run in &runs {
        let (code, _) = refusal(run);
        assert!(listed.contains(&code), "{code} is not listed by vocab");
        seen.push(code);
    }
    assert_eq!(
        seen,
        [
            "stdin-twice",
            "mixed-forms",
            "missing-from",
            "unknown-form",
            "not-found",
            "bad-escape",
            "json-syntax",
            "not-loadable"
        ]
    );
}
