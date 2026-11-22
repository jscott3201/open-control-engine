//! The two guards on `-o` (owner rulings of 2026-11-21, GitHub #688, #697): `emit -o` and
//! `import -o` refuse an output that is the input file (on Unix, standard input opened on a
//! regular file included), and refuse a name `oce` would read as another form than the one
//! written (for `emit`, the CXF form: `.oce` and `.json` are refused). Each is a usage error decided before anything is read:
//! exit 2 with the report, and nothing written, replaced or left beside the target.
//!
//! "The input file" is file identity (`file_identity::same_file`): the same path in another
//! spelling, an input named through a symbolic link and, on Unix, a hard link to it. On Windows
//! identity is the canonical path, so a hard link is not recognised there; the Unix-only tests
//! say which. An `-o` that is itself a symbolic link, dangling or not, is refused before this
//! guard as `output-not-a-file` (issue #758, ruled by the owner on 2026-11-22): `oce` never writes
//! through a link, whatever it names.

mod support;

use std::fs;
use std::path::Path;

use support::{Run, author_fixture, case_in, crate_dir, oce_in, oce_in_with_stdin_file, scratch};

fn fixture(relative: &str) -> Vec<u8> {
    fs::read(crate_dir().join(author_fixture(relative))).expect("fixture")
}

/// A scratch directory holding the worked example in all three forms.
fn workspace(name: &str) -> std::path::PathBuf {
    let dir = scratch(name);
    fs::write(
        dir.join("econ.cxf.json"),
        fixture("emit/econ_enable.cxf.json"),
    )
    .expect("write");
    fs::write(dir.join("econ.oce"), fixture("text/econ_enable.oce")).expect("write");
    fs::write(dir.join("econ.json"), fixture("wire/econ_enable.json")).expect("write");
    fs::create_dir(dir.join("sub")).expect("mkdir");
    dir
}

/// Every file under `dir` with its bytes, sorted by name: what a refused run must leave as it was.
fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = fs::read_dir(dir)
        .expect("dir")
        .map(|entry| entry.expect("entry"))
        .filter(|entry| entry.path().is_file())
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).expect("read"),
            )
        })
        .collect();
    files.sort();
    files
}

/// The single error code of a refused run, after checking the run's shape.
fn refusal(run: &Run) -> String {
    assert_eq!(run.code, 2, "{}", run.transcript());
    assert!(run.stderr.is_empty(), "{}", run.transcript());
    let report = run.report();
    assert_eq!(report["ok"], false);
    for absent in ["output", "bytes", "dropped", "normalised"] {
        assert!(
            report.get(absent).is_none(),
            "{absent}: {}",
            run.transcript()
        );
    }
    let errors = report["errors"].as_array().expect("errors");
    assert_eq!(errors.len(), 1, "{}", run.transcript());
    errors[0]["code"].as_str().expect("code").to_owned()
}

// --- the output is the input ---

#[test]
fn import_refuses_an_output_that_is_its_input_under_any_spelling_of_the_path() {
    let dir = workspace("guard_import_same_path");
    // A name that reads as no form, so the only question is whether it is the input.
    fs::copy(dir.join("econ.cxf.json"), dir.join("doc")).expect("copy");
    let before = snapshot(&dir);
    for output in ["doc", "./doc", "sub/../doc"] {
        for flags in [&[][..], &["--wire"][..]] {
            let run = oce_in(
                &dir,
                &[&["import", "doc", "-o", output], flags].concat(),
                b"",
            );
            assert_eq!(refusal(&run), "output-is-input", "{output} {flags:?}");
            assert_eq!(snapshot(&dir), before, "{output}: something was written");
        }
    }
    // The input given in another spelling too.
    let run = oce_in(&dir, &["import", "./sub/../doc", "-o", "doc"], b"");
    assert_eq!(refusal(&run), "output-is-input");
    assert_eq!(snapshot(&dir), before);
}

#[test]
fn emit_refuses_an_output_that_is_its_input_under_any_spelling_of_the_path() {
    let dir = workspace("guard_emit_same_path");
    let before = snapshot(&dir);
    for (input, outputs) in [
        ("econ.oce", ["econ.oce", "./econ.oce", "sub/../econ.oce"]),
        (
            "econ.json",
            ["econ.json", "./econ.json", "sub/../econ.json"],
        ),
    ] {
        for output in outputs {
            let run = oce_in(&dir, &["emit", input, "-o", output], b"");
            assert_eq!(refusal(&run), "output-is-input", "{input} -> {output}");
            // No stage ran: the guard is decided before the read.
            assert_eq!(run.report()["stages"], serde_json::json!([]));
            assert_eq!(snapshot(&dir), before, "{output}: something was written");
        }
    }
}

#[test]
fn a_different_file_and_stdin_are_not_the_input() {
    let dir = workspace("guard_distinct");
    // A copy with the same bytes is another file.
    fs::copy(dir.join("econ.cxf.json"), dir.join("copy")).expect("copy");
    let run = oce_in(&dir, &["import", "econ.cxf.json", "-o", "copy"], b"");
    assert_eq!(run.code, 0, "{}", run.transcript());
    let run = oce_in(&dir, &["emit", "econ.oce", "-o", "out.cxf.json"], b"");
    assert_eq!(run.code, 0, "{}", run.transcript());
    // An output that does not exist yet is no file, whatever it is named.
    let run = oce_in(&dir, &["import", "econ.cxf.json", "-o", "sub/new.oce"], b"");
    assert_eq!(run.code, 0, "{}", run.transcript());
    // Stdin is no file: a file named `-` beside it is never consulted, and `-o -` is stdout.
    let text = fixture("text/econ_enable.oce");
    let run = oce_in(
        &dir,
        &["emit", "-", "--from", "text", "-o", "stdin.cxf.json"],
        &text,
    );
    assert_eq!(run.code, 0, "{}", run.transcript());
    let run = oce_in(&dir, &["emit", "econ.json", "-o", "-"], b"");
    assert_eq!(run.code, 0, "{}", run.transcript());
    assert!(
        run.report().get("tool").is_none(),
        "stdout is the CXF document"
    );
}

#[test]
fn a_missing_input_is_reported_as_missing_not_as_the_output() {
    let dir = workspace("guard_missing");
    let import = oce_in(&dir, &["import", "nope", "-o", "nope"], b"");
    assert_eq!(refusal(&import), "not-found");
    let emit = oce_in(&dir, &["emit", "nope", "--from", "text", "-o", "nope"], b"");
    assert_eq!(refusal(&emit), "not-found");
    assert!(!dir.join("nope").exists());
}

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn an_input_named_through_a_link_is_the_input_and_an_output_link_is_not_a_file() {
        let dir = workspace("guard_symlink");
        symlink("econ.cxf.json", dir.join("link")).expect("link");
        symlink("econ.oce", dir.join("text-link")).expect("link");
        // A link in another directory, and a link to a link.
        symlink("../econ.cxf.json", dir.join("sub/far")).expect("link");
        symlink("link", dir.join("link-to-link")).expect("link");
        let before = snapshot(&dir);
        for output in ["link", "sub/far", "link-to-link"] {
            // A link at `-o` is refused for being a link, whatever it names (issue #758).
            let run = oce_in(&dir, &["import", "econ.cxf.json", "-o", output], b"");
            assert_eq!(refusal(&run), "output-not-a-file", "{output}");
            // The link as the input, the file it names as the output: the input file.
            let run = oce_in(&dir, &["import", output, "-o", "econ.cxf.json"], b"");
            assert_eq!(refusal(&run), "output-is-input", "{output} as the input");
        }
        let run = oce_in(&dir, &["emit", "econ.oce", "-o", "text-link"], b"");
        assert_eq!(refusal(&run), "output-not-a-file");
        let run = oce_in(
            &dir,
            &["emit", "text-link", "--from", "text", "-o", "econ.oce"],
            b"",
        );
        assert_eq!(refusal(&run), "output-is-input");
        assert_eq!(snapshot(&dir), before);
        for link in ["link", "text-link", "link-to-link"] {
            let kind = fs::symlink_metadata(dir.join(link))
                .expect("metadata")
                .file_type();
            assert!(kind.is_symlink(), "{link} is still a link");
        }
    }

    #[test]
    fn a_hard_link_to_the_input_is_the_input_for_both_verbs() {
        let dir = workspace("guard_hard_link");
        fs::hard_link(dir.join("econ.cxf.json"), dir.join("twin")).expect("hard link");
        fs::hard_link(dir.join("econ.oce"), dir.join("text-twin")).expect("hard link");
        let before = snapshot(&dir);
        let run = oce_in(&dir, &["import", "econ.cxf.json", "-o", "twin"], b"");
        assert_eq!(refusal(&run), "output-is-input");
        let run = oce_in(&dir, &["emit", "econ.oce", "-o", "text-twin"], b"");
        assert_eq!(refusal(&run), "output-is-input");
        assert_eq!(snapshot(&dir), before);
    }

    #[test]
    fn a_file_opened_as_stdin_is_the_input_for_both_verbs_under_any_name_of_it() {
        let dir = workspace("guard_stdin_file");
        fs::copy(dir.join("econ.cxf.json"), dir.join("doc")).expect("copy");
        fs::copy(dir.join("econ.oce"), dir.join("text-doc")).expect("copy");
        symlink("doc", dir.join("link")).expect("link");
        fs::hard_link(dir.join("doc"), dir.join("twin")).expect("hard link");
        symlink("text-doc", dir.join("text-link")).expect("link");
        fs::hard_link(dir.join("text-doc"), dir.join("text-twin")).expect("hard link");
        let before = snapshot(&dir);
        // A link at `-o` is a link before it is the input (issue #758).
        let expected = |output: &str| {
            if output.ends_with("link") {
                "output-not-a-file"
            } else {
                "output-is-input"
            }
        };
        for output in ["doc", "./doc", "sub/../doc", "link", "twin"] {
            for flags in [&[][..], &["--wire"][..]] {
                let args = [&["import", "-", "-o", output], flags].concat();
                let run = oce_in_with_stdin_file(&dir, &args, &dir.join("doc"));
                assert_eq!(refusal(&run), expected(output), "{output} {flags:?}");
                assert_eq!(snapshot(&dir), before, "{output}: something was written");
            }
        }
        for output in [
            "text-doc",
            "./text-doc",
            "sub/../text-doc",
            "text-link",
            "text-twin",
        ] {
            let args = ["emit", "-", "--from", "text", "-o", output];
            let run = oce_in_with_stdin_file(&dir, &args, &dir.join("text-doc"));
            assert_eq!(refusal(&run), expected(output), "{output}");
            // No stage ran: the guard is decided before the read.
            assert_eq!(run.report()["stages"], serde_json::json!([]));
            assert_eq!(snapshot(&dir), before, "{output}: something was written");
        }
    }

    #[test]
    fn a_file_opened_as_stdin_is_not_the_input_of_another_output() {
        let dir = workspace("guard_stdin_other_file");
        // The same bytes under another name are another file; so is a file that is not there yet.
        let args = ["emit", "-", "--from", "text", "-o", "out.cxf.json"];
        let run = oce_in_with_stdin_file(&dir, &args, &dir.join("econ.oce"));
        assert_eq!(run.code, 0, "{}", run.transcript());
        fs::write(dir.join("doc"), b"stale").expect("doc");
        let args = ["emit", "-", "--from", "text", "-o", "doc"];
        let run = oce_in_with_stdin_file(&dir, &args, &dir.join("econ.oce"));
        assert_eq!(run.code, 0, "{}", run.transcript());
        let run = oce_in_with_stdin_file(
            &dir,
            &["import", "-", "-o", "imported"],
            &dir.join("econ.cxf.json"),
        );
        assert_eq!(run.code, 0, "{}", run.transcript());
        // `-o -` is stdout, whatever stdin is.
        let args = ["emit", "-", "--from", "text", "-o", "-"];
        let run = oce_in_with_stdin_file(&dir, &args, &dir.join("econ.oce"));
        assert_eq!(run.code, 0, "{}", run.transcript());
    }

    #[test]
    fn a_stdin_that_is_a_device_is_never_the_input() {
        // `oce emit - -o /dev/null < /dev/null`: the same device node, but no file to replace.
        let dir = workspace("guard_stdin_device");
        // The output is the device: refused for not being a regular file, before anything is
        // read or written (issue #758), so no temporary file is ever tried beside it.
        let null = Path::new("/dev/null");
        for args in [
            &["emit", "-", "--from", "text", "-o", "/dev/null"][..],
            &["import", "-", "-o", "/dev/null"][..],
        ] {
            let run = oce_in_with_stdin_file(&dir, args, null);
            let code = &run.report()["errors"][0]["code"];
            assert_eq!(code, "output-not-a-file", "{args:?}: {}", run.transcript());
        }
    }

    #[test]
    fn a_dangling_link_is_not_the_input_and_is_not_a_file() {
        let dir = workspace("guard_dangling");
        symlink("missing/out", dir.join("dangling")).expect("link");
        let run = oce_in(&dir, &["import", "econ.cxf.json", "-o", "dangling"], b"");
        assert_eq!(refusal(&run), "output-not-a-file");
        assert!(!dir.join("missing").exists());
    }
}

#[test]
fn a_pipe_carrying_the_bytes_of_the_output_is_not_checked_and_the_output_is_replaced() {
    // The documented limit: only a regular file as stdin is the input; a pipe is no file.
    let dir = workspace("guard_stdin_pipe");
    fs::copy(dir.join("econ.cxf.json"), dir.join("doc")).expect("copy");
    let run = oce_in(
        &dir,
        &["import", "-", "-o", "doc"],
        &fixture("emit/econ_enable.cxf.json"),
    );
    assert_eq!(run.code, 0, "{}", run.transcript());
    assert!(
        fs::read(dir.join("doc"))
            .expect("doc")
            .starts_with(b"sequence ")
    );
    fs::copy(dir.join("econ.oce"), dir.join("text-doc")).expect("copy");
    let run = oce_in(
        &dir,
        &["emit", "-", "--from", "text", "-o", "text-doc"],
        &fixture("text/econ_enable.oce"),
    );
    assert_eq!(run.code, 0, "{}", run.transcript());
    assert!(
        fs::read(dir.join("text-doc"))
            .expect("doc")
            .starts_with(b"{")
    );
}

// --- the output's name against the form written ---

#[test]
fn import_refuses_an_output_name_that_reads_as_another_form() {
    let dir = workspace("guard_name");
    let before = snapshot(&dir);
    let contradicting: [(&[&str], &str); 8] = [
        // Text into a name read as wire or as CXF.
        (&[], "out.json"),
        (&[], "out.cxf.json"),
        (&[], "out.jsonld"),
        (&[], "sub/out.json"),
        // Wire into a name read as text or as CXF.
        (&["--wire"], "out.oce"),
        (&["--wire"], "out.cxf.json"),
        (&["--wire"], "out.jsonld"),
        (&["--wire"], "sub/out.oce"),
    ];
    for (flags, name) in contradicting {
        let run = oce_in(
            &dir,
            &[&["import", "econ.cxf.json", "-o", name], flags].concat(),
            b"",
        );
        assert_eq!(refusal(&run), "output-form-mismatch", "{flags:?} {name}");
        assert!(!dir.join(name).exists(), "{name} was written");
        // The same answer for a document on stdin, and before the input is looked at.
        let stdin = oce_in(
            &dir,
            &[&["import", "-", "-o", name], flags].concat(),
            b"junk",
        );
        assert_eq!(refusal(&stdin), "output-form-mismatch", "{flags:?} {name}");
        let missing = oce_in(
            &dir,
            &[&["import", "nope", "-o", name], flags].concat(),
            b"",
        );
        assert_eq!(
            refusal(&missing),
            "output-form-mismatch",
            "{flags:?} {name}"
        );
    }
    assert_eq!(snapshot(&dir), before);
}

#[test]
fn a_name_that_gives_no_form_is_accepted_for_either_and_the_flag_alone_chooses_the_form() {
    let dir = workspace("guard_free_names");
    for name in [
        "out", "out.txt", "out.OCE", "out.JSON", "out.wire", ".hidden",
    ] {
        for (flags, starts) in [
            (&[][..], "sequence econ_enable\n"),
            (&["--wire"][..], "{\n"),
        ] {
            let run = oce_in(
                &dir,
                &[&["import", "econ.cxf.json", "-o", name], flags].concat(),
                b"",
            );
            assert_eq!(run.code, 0, "{flags:?} {name}: {}", run.transcript());
            let written = fs::read_to_string(dir.join(name)).expect("the output");
            assert!(written.starts_with(starts), "{flags:?} {name}");
            fs::remove_file(dir.join(name)).expect("remove");
        }
    }
}

#[test]
fn every_file_import_writes_under_a_form_name_is_read_back_by_name_in_that_form() {
    // "Exit 0 means `oce` reads what it wrote", by name: no `--from` anywhere below.
    let dir = workspace("guard_read_back");
    let expected = oce_in(&dir, &["id", "econ.cxf.json"], b"");
    assert_eq!(expected.code, 0, "{}", expected.transcript());
    for (flags, name) in [(&[][..], "out.oce"), (&["--wire"][..], "out.json")] {
        let run = oce_in(
            &dir,
            &[&["import", "econ.cxf.json", "-o", name], flags].concat(),
            b"",
        );
        assert_eq!(run.code, 0, "{name}: {}", run.transcript());
        let validated = oce_in(&dir, &["validate", name], b"");
        assert_eq!(validated.code, 0, "{name}: {}", validated.transcript());
        let checked = oce_in(&dir, &["fmt", name, "--check"], b"");
        assert_eq!(checked.code, 0, "{name}: {}", checked.transcript());
        let id = oce_in(&dir, &["id", name], b"");
        assert_eq!(id.stdout, expected.stdout, "{name}: {}", id.transcript());
        let emitted = oce_in(&dir, &["emit", name], b"");
        assert_eq!(emitted.code, 0, "{name}: {}", emitted.transcript());
    }
}

/// The names `emit -o` refuses: CXF written under a name `oce` reads as text or as wire.
const EMIT_REFUSED_NAMES: [&str; 6] = [
    "out.oce",
    "out.json",
    "sub/out.oce",
    "sub/out.json",
    "./out.json",
    "sub/../out.oce",
];

/// The names `emit -o` accepts: CXF names, and names that give no form (the rule is
/// case-sensitive).
const EMIT_ACCEPTED_NAMES: [&str; 9] = [
    "out.jsonld",
    "out.cxf.json",
    "sub/out.cxf.json",
    "out",
    "out.txt",
    "out.OCE",
    "out.JSON",
    "out.wire",
    ".hidden",
];

#[test]
fn emit_refuses_an_output_name_that_reads_as_text_or_wire_and_writes_nothing() {
    let dir = workspace("guard_emit_name");
    let before = snapshot(&dir);
    for name in EMIT_REFUSED_NAMES {
        for input in ["econ.oce", "econ.json"] {
            let run = oce_in(&dir, &["emit", input, "-o", name], b"");
            assert_eq!(refusal(&run), "output-form-mismatch", "{input} {name}");
            assert_eq!(run.report()["stages"], serde_json::json!([]));
            assert_eq!(snapshot(&dir), before, "{name}: something was written");
            assert!(
                snapshot(&dir.join("sub")).is_empty(),
                "{name}: something was left in sub"
            );
        }
        // Decided before the input is looked at: stdin, a missing file, an unreadable document
        // and a sequence with an error finding all answer the same.
        let stdin = oce_in(&dir, &["emit", "-", "--from", "text", "-o", name], b"junk");
        assert_eq!(refusal(&stdin), "output-form-mismatch", "{name}");
        let no_form = oce_in(&dir, &["emit", "-", "-o", name], b"junk");
        assert_eq!(refusal(&no_form), "output-form-mismatch", "{name}");
        let missing = oce_in(&dir, &["emit", "nope.oce", "-o", name], b"");
        assert_eq!(refusal(&missing), "output-form-mismatch", "{name}");
        let namespace = oce_in(
            &dir,
            &["emit", "econ.oce", "--namespace", "bad", "-o", name],
            b"",
        );
        assert_eq!(refusal(&namespace), "output-form-mismatch", "{name}");
        assert_eq!(snapshot(&dir), before, "{name}: something was written");
    }
}

#[test]
fn emit_refuses_a_contradicting_name_for_an_input_with_an_error_finding() {
    let dir = workspace("guard_emit_name_findings");
    let stage_one = crate_dir().join("tests/fixtures/inputs/stage_one.oce");
    fs::copy(stage_one, dir.join("stage_one.oce")).expect("copy");
    let before = snapshot(&dir);
    let run = oce_in(&dir, &["emit", "stage_one.oce", "-o", "out.json"], b"");
    assert_eq!(refusal(&run), "output-form-mismatch");
    assert_eq!(snapshot(&dir), before);
}

#[test]
fn emit_accepts_the_cxf_names_and_the_names_that_give_no_form() {
    let dir = workspace("guard_emit_accepted");
    let stdout = oce_in(&dir, &["emit", "econ.oce"], b"");
    assert_eq!(stdout.code, 0, "{}", stdout.transcript());
    for name in EMIT_ACCEPTED_NAMES {
        let run = oce_in(&dir, &["emit", "econ.oce", "-o", name], b"");
        assert_eq!(run.code, 0, "{name}: {}", run.transcript());
        assert_eq!(run.report()["output"], name);
        assert_eq!(fs::read(dir.join(name)).expect("the output"), stdout.stdout);
    }
    let dash = oce_in(&dir, &["emit", "econ.json", "-o", "-"], b"");
    assert_eq!(dash.stdout, stdout.stdout, "-o - is stdout");
}

#[test]
fn every_file_emit_writes_under_a_cxf_name_is_read_back_by_name_as_cxf() {
    let dir = workspace("guard_emit_read_back");
    let expected = oce_in(&dir, &["id", "econ.oce"], b"");
    assert_eq!(expected.code, 0, "{}", expected.transcript());
    for name in ["out.jsonld", "out.cxf.json"] {
        let run = oce_in(&dir, &["emit", "econ.oce", "-o", name], b"");
        assert_eq!(run.code, 0, "{name}: {}", run.transcript());
        let id = oce_in(&dir, &["id", name], b"");
        assert_eq!(id.stdout, expected.stdout, "{name}: {}", id.transcript());
        // `validate` reads authored forms only, so by name a CXF file is `id`'s to read.
        let from_stdin = oce_in(
            &dir,
            &["id", "-", "--from", "cxf"],
            &fs::read(dir.join(name)).expect("read"),
        );
        assert_eq!(from_stdin.stdout, expected.stdout, "{name}");
    }
}

// --- the transcripts ---

#[test]
fn the_refusals_are_pinned() {
    let dir = workspace("guard_transcripts");
    case_in(
        &dir,
        "import_output_form_mismatch",
        2,
        &["import", "econ.cxf.json", "-o", "econ.json"],
    );
    fs::copy(dir.join("econ.cxf.json"), dir.join("doc")).expect("copy");
    case_in(
        &dir,
        "import_output_is_input",
        2,
        &["import", "doc", "-o", "./doc", "--wire"],
    );
    case_in(
        &dir,
        "emit_output_is_input",
        2,
        &["emit", "econ.oce", "-o", "econ.oce"],
    );
    case_in(
        &dir,
        "emit_output_form_mismatch",
        2,
        &["emit", "econ.oce", "-o", "out.json"],
    );
}
