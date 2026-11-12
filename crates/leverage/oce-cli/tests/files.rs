//! The commands that write files and the properties over the `oce-author` fixtures: `fmt --check`
//! over every canonical text fixture, `fmt` against the `oce-author` fmt goldens and idempotent,
//! an atomic `fmt --write`, `emit -o` equal to `emit` to stdout, and every emitted document
//! loading warning-free in a throwaway `Engine`.

mod support;

use std::path::Path;

use support::{author_fixture, crate_dir, oce, oce_in, scratch};

/// The `oce-author` text fixtures, referenced in place, relative to the crate directory.
fn text_fixtures(subdir: &str, suffix: &str) -> Vec<String> {
    let dir = crate_dir()
        .join("../oce-author/tests/fixtures")
        .join(subdir);
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("the oce-author fixtures")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(suffix))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no fixtures in {subdir}");
    names
        .into_iter()
        .map(|name| author_fixture(&format!("{subdir}/{name}")))
        .collect()
}

fn read(relative: &str) -> Vec<u8> {
    std::fs::read(crate_dir().join(relative)).expect("read")
}

#[test]
fn fmt_check_accepts_every_canonical_text_fixture() {
    let mut fixtures = text_fixtures("text", ".oce");
    fixtures.extend(text_fixtures("text/fmt", ".out.oce"));
    for fixture in fixtures {
        let run = oce(&["fmt", "--check", &fixture], b"");
        assert_eq!(run.code, 0, "{fixture}\n{}", run.transcript());
        assert_eq!(run.report()["canonical"], true);
    }
}

#[test]
fn fmt_matches_the_author_goldens_and_is_idempotent() {
    for input in text_fixtures("text/fmt", ".in.oce") {
        let expected = read(&input.replace(".in.oce", ".out.oce"));
        let check = oce(&["fmt", "--check", &input], b"");
        assert_eq!(check.code, 1, "{input} is not canonical as written");
        let once = oce(&["fmt", &input], b"");
        assert_eq!(once.code, 0, "{}", once.transcript());
        assert_eq!(once.stdout, expected, "{input}");
        let twice = oce(&["fmt", "-", "--from", "text"], &once.stdout);
        assert_eq!(twice.stdout, once.stdout, "{input}: fmt is not idempotent");
        let recheck = oce(&["fmt", "--check", "-", "--from", "text"], &once.stdout);
        assert_eq!(recheck.code, 0, "{input}: fmt output is not canonical");
    }
}

#[test]
fn fmt_of_every_wire_bundle_is_the_text_bundle() {
    for wire in text_fixtures("wire", ".json") {
        let text = wire.replace("/wire/", "/text/").replace(".json", ".oce");
        if !crate_dir().join(&text).exists() {
            continue;
        }
        let run = oce(&["fmt", &wire], b"");
        assert_eq!(run.code, 0, "{}", run.transcript());
        assert_eq!(run.stdout, read(&text), "{wire}");
    }
}

#[test]
fn fmt_write_rewrites_atomically_and_only_on_change() {
    let dir = scratch("fmt_write");
    let input = author_fixture("text/fmt/sections.in.oce");
    std::fs::write(dir.join("sections.oce"), read(&input)).expect("copy");
    let first = oce_in(&dir, &["fmt", "--write", "sections.oce"], b"");
    assert_eq!(first.code, 0, "{}", first.transcript());
    let report = first.report();
    assert_eq!(report["changed"], true);
    assert_eq!(report["output"], "sections.oce");
    assert_eq!(
        std::fs::read(dir.join("sections.oce")).expect("read"),
        read(&author_fixture("text/fmt/sections.out.oce"))
    );
    let second = oce_in(&dir, &["fmt", "--write", "sections.oce"], b"");
    assert_eq!(second.code, 0);
    assert_eq!(second.report()["changed"], false);
    let mut left: Vec<String> = std::fs::read_dir(&dir)
        .expect("list")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, ["sections.oce"], "no temporary file is left behind");
}

#[test]
fn fmt_write_leaves_a_malformed_file_untouched() {
    let dir = scratch("fmt_write_malformed");
    let bytes = b"sequence s\nparam x : Real = 1\n";
    std::fs::write(dir.join("bad.oce"), bytes).expect("write");
    let run = oce_in(&dir, &["fmt", "--write", "bad.oce"], b"");
    assert_eq!(run.code, 2);
    assert_eq!(run.report()["errors"][0]["code"], "missing-derived-from");
    assert_eq!(std::fs::read(dir.join("bad.oce")).expect("read"), bytes);
}

#[test]
fn emit_output_file_equals_stdout_and_the_report_names_it() {
    let dir = scratch("emit_output");
    std::fs::write(
        dir.join("econ.oce"),
        read(&author_fixture("text/econ_enable.oce")),
    )
    .expect("copy");
    let stdout = oce_in(&dir, &["emit", "econ.oce"], b"");
    assert_eq!(stdout.code, 0);
    let file = oce_in(&dir, &["emit", "econ.oce", "-o", "econ.cxf.json"], b"");
    assert_eq!(file.code, 0, "{}", file.transcript());
    let report = file.report();
    assert_eq!(report["output"], "econ.cxf.json");
    assert_eq!(report["bytes"], stdout.stdout.len());
    assert_eq!(report["ok"], true);
    assert_eq!(
        std::fs::read(dir.join("econ.cxf.json")).expect("read"),
        stdout.stdout
    );
    let dash = oce_in(&dir, &["emit", "econ.oce", "-o", "-"], b"");
    assert_eq!(dash.stdout, stdout.stdout, "-o - is stdout");
}

#[test]
fn emit_refusal_writes_no_output_file() {
    let dir = scratch("emit_refusal");
    let stage_one = crate_dir().join("tests/fixtures/inputs/stage_one.oce");
    std::fs::copy(stage_one, dir.join("stage_one.oce")).expect("copy");
    let run = oce_in(&dir, &["emit", "stage_one.oce", "-o", "out.json"], b"");
    assert_eq!(run.code, 1);
    assert!(!Path::new(&dir.join("out.json")).exists());
}

#[test]
fn every_emitted_bundle_loads_warning_free_in_a_throwaway_engine() {
    let mut fixtures = text_fixtures("text", ".oce");
    fixtures.extend(text_fixtures("wire", ".json"));
    let mut emitted = 0;
    for fixture in fixtures {
        if oce(&["validate", &fixture], b"").code != 0 {
            continue;
        }
        for namespace in [None, Some("https://example.org/plant/")] {
            let mut args = vec!["emit", fixture.as_str()];
            if let Some(namespace) = namespace {
                args.extend(["--namespace", namespace]);
            }
            let run = oce(&args, b"");
            assert_eq!(run.code, 0, "{fixture}\n{}", run.transcript());
            let mut engine = oce_api::Engine::new();
            let (report, _) = engine.load_cxf(&run.stdout).expect("the emitted CXF loads");
            assert!(
                report.warnings.is_empty(),
                "{fixture}: {:?}",
                report.warnings
            );
            emitted += 1;
        }
    }
    assert!(
        emitted >= 16,
        "only {emitted} emitted documents were loaded"
    );
}
