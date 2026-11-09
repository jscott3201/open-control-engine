//! The two commands that write files (`fmt --write`, `emit -o`) through the built binary: a stale
//! file beside the target is ignored, the target keeps its permissions, a new file is owner-only
//! (Unix), a symbolic-link target is followed, and a failed write reports no `changed`, `output`
//! or `bytes` and leaves no temporary file behind. Planted links and files at the temporary names
//! themselves are unit-tested in `src/commands/atomic_write.rs`, which can predict the names.

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
fn emit_to_a_directory_fails_typed_without_output_or_bytes() {
    let dir = scratch("emit_to_directory");
    fs::write(dir.join("econ.oce"), clean()).expect("input");
    fs::create_dir(dir.join("outdir")).expect("a directory at the output");
    let run = oce_in(&dir, &["emit", "econ.oce", "-o", "outdir"], b"");
    assert_eq!(run.code, 2, "{}", run.transcript());
    let report = run.report();
    assert_eq!(report["ok"], false);
    assert_eq!(report["errors"][0]["code"], "write-failed");
    let object = report.as_object().expect("an object");
    assert!(!object.contains_key("output"), "{}", run.transcript());
    assert!(!object.contains_key("bytes"), "{}", run.transcript());
    assert_eq!(names(&dir), ["econ.oce", "outdir"], "no temporary file");
    assert!(names(&dir.join("outdir")).is_empty());
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
        let run = oce_in(&dir, &["emit", "econ.oce", "-o", "out.json"], b"");
        assert_eq!(run.code, 0, "{}", run.transcript());
        assert_eq!(mode(&dir.join("out.json")), 0o600);
        assert_eq!(names(&dir), ["econ.oce", "out.json"], "no temporary file");
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
            fs::write(dir.join("out.json"), b"old").expect("output");
            fs::set_permissions(dir.join("out.json"), fs::Permissions::from_mode(wanted))
                .expect("chmod");
            let run = oce_in(&dir, &["emit", "econ.oce", "-o", "out.json"], b"");
            assert_eq!(run.code, 0, "{}", run.transcript());
            assert_eq!(mode(&dir.join("out.json")), wanted, "emit -o at {wanted:o}");
        }
    }

    #[test]
    fn fmt_write_through_a_link_rewrites_the_linked_file_and_keeps_the_link() {
        let dir = scratch("link_target");
        fs::create_dir(dir.join("real")).expect("a directory for the real file");
        fs::write(dir.join("real/t.oce"), non_canonical()).expect("real");
        symlink("real/t.oce", dir.join("link.oce")).expect("link");
        let run = oce_in(&dir, &["fmt", "--write", "link.oce"], b"");
        assert_eq!(run.code, 0, "{}", run.transcript());
        assert_eq!(run.report()["output"], "link.oce");
        let link = fs::symlink_metadata(dir.join("link.oce")).expect("link metadata");
        assert!(link.file_type().is_symlink(), "the link is still a link");
        assert_eq!(fs::read(dir.join("real/t.oce")).expect("real"), canonical());
        assert_eq!(names(&dir), ["link.oce", "real"]);
        assert_eq!(names(&dir.join("real")), ["t.oce"], "no temporary file");
    }

    #[test]
    fn emit_output_through_a_dangling_link_fails_typed() {
        let dir = scratch("dangling_link");
        fs::write(dir.join("econ.oce"), clean()).expect("input");
        symlink("missing/out.json", dir.join("out.json")).expect("link");
        let run = oce_in(&dir, &["emit", "econ.oce", "-o", "out.json"], b"");
        assert_eq!(run.code, 2, "{}", run.transcript());
        assert_eq!(run.report()["errors"][0]["code"], "write-failed");
        assert_eq!(names(&dir), ["econ.oce", "out.json"]);
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
