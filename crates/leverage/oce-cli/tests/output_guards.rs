//! The two guards on `-o` (owner rulings of 2026-11-21, GitHub #688): `emit -o` and `import -o`
//! refuse an output that is the input file, and `import -o` refuses a name `oce` would read as
//! another form than the one written. Each is a usage error decided before anything is read:
//! exit 2 with the report, and nothing written, replaced or left beside the target.
//!
//! "The input file" is file identity (`file_identity::same_file`): the same path in another
//! spelling, a symbolic link to it and, on Unix, a hard link to it. On Windows identity is the
//! canonical path, so a hard link is not recognised there; the Unix-only tests say which.

mod support;

use std::fs;
use std::path::Path;

use support::{Run, author_fixture, case_in, crate_dir, oce_in, scratch};

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
        &["emit", "-", "--from", "text", "-o", "econ.oce"],
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
    let emit = oce_in(&dir, &["emit", "nope.oce", "-o", "nope.oce"], b"");
    assert_eq!(refusal(&emit), "not-found");
    assert!(!dir.join("nope").exists() && !dir.join("nope.oce").exists());
}

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn a_symbolic_link_to_the_input_is_the_input_for_both_verbs() {
        let dir = workspace("guard_symlink");
        symlink("econ.cxf.json", dir.join("link")).expect("link");
        symlink("econ.oce", dir.join("text-link")).expect("link");
        // A link in another directory, and a link to a link.
        symlink("../econ.cxf.json", dir.join("sub/far")).expect("link");
        symlink("link", dir.join("link-to-link")).expect("link");
        let before = snapshot(&dir);
        for output in ["link", "sub/far", "link-to-link"] {
            let run = oce_in(&dir, &["import", "econ.cxf.json", "-o", output], b"");
            assert_eq!(refusal(&run), "output-is-input", "{output}");
            // And the other way round: the link as the input, the file as the output.
            let run = oce_in(&dir, &["import", output, "-o", "econ.cxf.json"], b"");
            assert_eq!(refusal(&run), "output-is-input", "{output} as the input");
        }
        let run = oce_in(&dir, &["emit", "econ.oce", "-o", "text-link"], b"");
        assert_eq!(refusal(&run), "output-is-input");
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
    fn a_dangling_link_is_not_the_input_and_fails_as_a_write() {
        let dir = workspace("guard_dangling");
        symlink("missing/out", dir.join("dangling")).expect("link");
        let run = oce_in(&dir, &["import", "econ.cxf.json", "-o", "dangling"], b"");
        assert_eq!(run.code, 2, "{}", run.transcript());
        assert_eq!(run.report()["errors"][0]["code"], "write-failed");
    }
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
}
