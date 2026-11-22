//! The two commands that write files (`fmt --write`, `emit -o`) through the built binary: a stale
//! file beside the target is ignored, the target keeps its permissions, a new file is owner-only
//! (Unix), a symbolic-link or directory target is `output-not-a-file` before anything is read
//! (issue #758, ruled by the owner on 2026-11-22), and a failed write reports no `changed`,
//! `output` or `bytes` and leaves no temporary file behind. Planted links and files at the
//! temporary names themselves are unit-tested in `src/commands/atomic_write.rs`, which can predict
//! the names; `tests/output_targets.rs` holds every verb to every kind of target.

mod support;

use std::fs;
use std::path::Path;

use support::{author_fixture, crate_dir, oce_in, scratch};

/// The `oce-author` fmt input that is not canonical, so `fmt --write` always rewrites it.
fn non_canonical() -> Vec<u8> {
    fs::read(crate_dir().join(author_fixture("text/fmt/sections.in.oce"))).expect("fixture")
}

/// Its canonical text.
fn canonical() -> Vec<u8> {
    fs::read(crate_dir().join(author_fixture("text/fmt/sections.out.oce"))).expect("fixture")
}

/// A clean text sequence `emit` writes.
fn clean() -> Vec<u8> {
    fs::read(crate_dir().join(author_fixture("text/econ_enable.oce"))).expect("fixture")
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("list")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Pins only that a stale file beside the target (here at the pre-release fixed temporary name) is
/// ignored and kept; temporary names are now unique per run, so this is not a collision test.
#[test]
fn a_stale_file_beside_the_target_is_ignored() {
    let dir = scratch("planted_file");
    fs::write(dir.join("t.oce"), non_canonical()).expect("target");
    fs::write(dir.join(".t.oce.oce-tmp"), b"someone else's").expect("plant");
    let run = oce_in(&dir, &["fmt", "--write", "t.oce"], b"");
    assert_eq!(run.code, 0, "{}", run.transcript());
    assert_eq!(fs::read(dir.join("t.oce")).expect("read"), canonical());
    assert_eq!(
        fs::read(dir.join(".t.oce.oce-tmp")).expect("planted"),
        b"someone else's"
    );
    assert_eq!(names(&dir), [".t.oce.oce-tmp", "t.oce"]);
}

#[test]
fn emit_to_a_directory_is_refused_before_anything_is_read_without_output_or_bytes() {
    let dir = scratch("emit_to_directory");
    fs::write(dir.join("econ.oce"), clean()).expect("input");
    fs::create_dir(dir.join("outdir")).expect("a directory at the output");
    let run = oce_in(&dir, &["emit", "econ.oce", "-o", "outdir"], b"");
    assert_eq!(run.code, 2, "{}", run.transcript());
    let report = run.report();
    assert_eq!(report["ok"], false);
    assert_eq!(report["errors"][0]["code"], "output-not-a-file");
    assert_eq!(report["stages"], serde_json::json!([]), "nothing was read");
    let object = report.as_object().expect("an object");
    assert!(!object.contains_key("output"), "{}", run.transcript());
    assert!(!object.contains_key("bytes"), "{}", run.transcript());
    assert_eq!(names(&dir), ["econ.oce", "outdir"], "no temporary file");
    assert!(names(&dir.join("outdir")).is_empty());
}

#[test]
fn emit_into_a_missing_directory_fails_as_a_write_without_output_or_bytes() {
    // The target check cannot read an entry under a missing directory, so the write is what
    // fails, after both stages ran.
    let dir = scratch("emit_into_missing_directory");
    fs::write(dir.join("econ.oce"), clean()).expect("input");
    let run = oce_in(
        &dir,
        &["emit", "econ.oce", "-o", "missing/out.cxf.json"],
        b"",
    );
    assert_eq!(run.code, 2, "{}", run.transcript());
    let report = run.report();
    assert_eq!(report["errors"][0]["code"], "write-failed");
    assert_eq!(report["stages"], serde_json::json!(["author", "engine"]));
    let object = report.as_object().expect("an object");
    assert!(!object.contains_key("output"), "{}", run.transcript());
    assert!(!object.contains_key("bytes"), "{}", run.transcript());
    assert_eq!(names(&dir), ["econ.oce"], "nothing created");
}

#[cfg(unix)]
mod unix {
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};

    use super::{canonical, clean, names, non_canonical};
    use crate::support::{oce_in, scratch};

    fn mode(path: &std::path::Path) -> u32 {
        fs::metadata(path).expect("metadata").permissions().mode() & 0o7777
    }

    #[test]
    fn emit_output_to_a_new_file_is_owner_only() {
        let dir = scratch("new_file_mode");
        fs::write(dir.join("econ.oce"), clean()).expect("input");
        let run = oce_in(&dir, &["emit", "econ.oce", "-o", "out.cxf.json"], b"");
        assert_eq!(run.code, 0, "{}", run.transcript());
        assert_eq!(mode(&dir.join("out.cxf.json")), 0o600);
        assert_eq!(
            names(&dir),
            ["econ.oce", "out.cxf.json"],
            "no temporary file"
        );
    }

    #[test]
    fn fmt_write_and_emit_output_keep_the_target_permissions() {
        let dir = scratch("permissions");
        for wanted in [0o640, 0o600] {
            fs::write(dir.join("t.oce"), non_canonical()).expect("target");
            fs::set_permissions(dir.join("t.oce"), fs::Permissions::from_mode(wanted))
                .expect("chmod");
            let run = oce_in(&dir, &["fmt", "--write", "t.oce"], b"");
            assert_eq!(run.code, 0, "{}", run.transcript());
            assert_eq!(run.report()["changed"], true);
            assert_eq!(
                mode(&dir.join("t.oce")),
                wanted,
                "fmt --write at {wanted:o}"
            );

            fs::write(dir.join("econ.oce"), clean()).expect("input");
            fs::write(dir.join("out.cxf.json"), b"old").expect("output");
            fs::set_permissions(dir.join("out.cxf.json"), fs::Permissions::from_mode(wanted))
                .expect("chmod");
            let run = oce_in(&dir, &["emit", "econ.oce", "-o", "out.cxf.json"], b"");
            assert_eq!(run.code, 0, "{}", run.transcript());
            assert_eq!(
                mode(&dir.join("out.cxf.json")),
                wanted,
                "emit -o at {wanted:o}"
            );
        }
    }

    #[test]
    fn fmt_write_of_a_link_is_refused_and_the_real_path_is_the_way_out() {
        // Breaking in place (issue #758): until then the link was followed and its file
        // rewritten. The file `fmt --write` rewrites is its input, so nothing is read either.
        let dir = scratch("link_target");
        fs::create_dir(dir.join("real")).expect("a directory for the real file");
        fs::write(dir.join("real/t.oce"), non_canonical()).expect("real");
        symlink("real/t.oce", dir.join("link.oce")).expect("link");
        let run = oce_in(&dir, &["fmt", "--write", "link.oce"], b"");
        assert_eq!(run.code, 2, "{}", run.transcript());
        let report = run.report();
        let error = &report["errors"][0];
        assert_eq!(error["code"], "output-not-a-file");
        let detail = error["detail"].as_str().expect("a detail");
        assert!(
            detail.starts_with("`link.oce` is a symbolic link"),
            "{detail}"
        );
        assert!(
            detail.contains("give the path of the file it names"),
            "{detail}"
        );
        assert_eq!(report["form"], serde_json::Value::Null, "nothing was read");
        let object = report.as_object().expect("an object");
        assert!(!object.contains_key("changed") && !object.contains_key("output"));
        let link = fs::symlink_metadata(dir.join("link.oce")).expect("link metadata");
        assert!(link.file_type().is_symlink(), "the link is still a link");
        assert_eq!(
            fs::read(dir.join("real/t.oce")).expect("real"),
            non_canonical()
        );
        assert_eq!(names(&dir), ["link.oce", "real"]);
        assert_eq!(names(&dir.join("real")), ["t.oce"], "no temporary file");
        // The real path rewrites the file the link names, and the link stays a link.
        let run = oce_in(&dir, &["fmt", "--write", "real/t.oce"], b"");
        assert_eq!(run.code, 0, "{}", run.transcript());
        assert_eq!(
            fs::read(dir.join("link.oce")).expect("through"),
            canonical()
        );
        assert!(
            fs::symlink_metadata(dir.join("link.oce"))
                .expect("link")
                .is_symlink()
        );
    }

    #[test]
    fn emit_output_to_a_dangling_link_is_refused_and_creates_nothing() {
        let dir = scratch("dangling_link");
        fs::write(dir.join("econ.oce"), clean()).expect("input");
        fs::create_dir(dir.join("missing")).expect("the link's directory");
        symlink("missing/out.cxf.json", dir.join("out.cxf.json")).expect("link");
        let run = oce_in(&dir, &["emit", "econ.oce", "-o", "out.cxf.json"], b"");
        assert_eq!(run.code, 2, "{}", run.transcript());
        assert_eq!(run.report()["errors"][0]["code"], "output-not-a-file");
        assert_eq!(names(&dir), ["econ.oce", "missing", "out.cxf.json"]);
        assert!(
            names(&dir.join("missing")).is_empty(),
            "nothing created where it points"
        );
    }

    #[test]
    fn a_failed_fmt_write_reports_neither_changed_nor_output() {
        let dir = scratch("unwritable_directory");
        fs::write(dir.join("t.oce"), non_canonical()).expect("target");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).expect("chmod");
        let run = oce_in(&dir, &["fmt", "--write", "t.oce"], b"");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("chmod back");
        if run.code == 0 {
            // A privileged runner (root) writes regardless; there is no failure to observe.
            #[allow(
                clippy::print_stderr,
                reason = "an explicit skip note in the test output"
            )]
            {
                eprintln!("skipped: running as root or on a privileged filesystem");
            }
            return;
        }
        assert_eq!(run.code, 2, "{}", run.transcript());
        let report = run.report();
        assert_eq!(report["errors"][0]["code"], "write-failed");
        let object = report.as_object().expect("an object");
        assert!(!object.contains_key("changed"), "{}", run.transcript());
        assert!(!object.contains_key("output"), "{}", run.transcript());
        assert_eq!(fs::read(dir.join("t.oce")).expect("read"), non_canonical());
    }
}
