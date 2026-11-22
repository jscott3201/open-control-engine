//! What `oce verify --commission` reads, and how each read is bounded (commissioning note, "CLI"
//! "Reads"; issue #664): every file the reference names is resolved inside the spec's directory
//! and judged from its metadata before a byte is read, so a file one byte over its bound, the
//! files together over theirs, a link out of the directory, a named pipe and a directory are
//! each refused with a typed code at the spec field that names the file, and nothing is held.
//! Over-size files are sparse: their length is metadata, and no test writes their bytes.

mod support;

use std::fs::{self, File};
use std::path::Path;

use support::commission::run_layout;
use support::{Run, oce_in, oce_in_guarded};

const SPEC: &str = "spec.json";

fn commission(dir: &Path) -> Run {
    oce_in(dir, &["verify", "--commission", SPEC], b"")
}

/// The one error of a refused run: `(code, at, cause)`.
fn refusal(run: &Run) -> (String, String, Option<String>) {
    assert_eq!(run.code, 2, "{}", run.transcript());
    let report = run.report();
    let errors = report["errors"].as_array().expect("errors");
    assert_eq!(errors.len(), 1, "{}", run.transcript());
    let text = |key: &str| errors[0][key].as_str().map(str::to_owned);
    (
        text("code").expect("code"),
        text("at").unwrap_or_default(),
        text("cause"),
    )
}

fn sparse(path: &Path, len: u64) {
    File::create(path)
        .and_then(|file| file.set_len(len))
        .expect("a sparse file");
}

fn detail(run: &Run) -> String {
    run.report()["errors"][0]["detail"]
        .as_str()
        .expect("detail")
        .to_owned()
}

#[test]
fn each_file_one_byte_over_its_bound_is_refused_from_its_metadata() {
    const MIB: u64 = 1024 * 1024;
    let cxf = u64::try_from(oce_api::MAX_CXF_BYTES).expect("fits");
    for (file, len, at) in [
        ("plant.cxf.json", cxf + 1, "references[0].cxf"),
        ("points.json", 4 * MIB + 1, "references[0].pointNameMapping"),
        (
            "fan.csv",
            (1 << 30) + 1,
            "references[0].indicatorSources[0].path",
        ),
    ] {
        let dir = run_layout(&format!("commission_big_{file}"), "pass");
        sparse(&dir.join(file), len);
        let run = commission(&dir);
        assert_eq!(
            refusal(&run),
            ("input-too-large".to_owned(), at.to_owned(), None),
            "{file}"
        );
        // The metadata's refusal names the size; a refused read could only say "larger than".
        assert!(
            detail(&run).contains(&format!("the input is {len} bytes")),
            "{file}: {}",
            detail(&run)
        );
        assert!(detail(&run).contains(&format!("`{file}`")), "{file}");
    }
    // The spec itself, at 4 MiB and one byte.
    let dir = run_layout("commission_big_spec", "pass");
    sparse(&dir.join("big.json"), 4 * MIB + 1);
    let run = oce_in(&dir, &["verify", "--commission", "big.json"], b"");
    assert_eq!(refusal(&run).0, "input-too-large");
    assert!(detail(&run).starts_with("the input is 4194305 bytes"));
}

#[test]
fn a_file_at_its_bound_is_read_and_judged_by_the_library() {
    // A CXF of exactly 8 MiB is read whole, and the engine refuses its zeros.
    let dir = run_layout("commission_at_bound", "pass");
    sparse(
        &dir.join("plant.cxf.json"),
        u64::try_from(oce_api::MAX_CXF_BYTES).expect("fits"),
    );
    assert_eq!(refusal(&commission(&dir)).0, "subject-load-failed");
    // A mapping of exactly 4 MiB is read, and the mapping reader refuses it.
    let dir = run_layout("commission_mapping_at_bound", "pass");
    sparse(&dir.join("points.json"), 4 * 1024 * 1024);
    let (code, _, cause) = refusal(&commission(&dir));
    assert_eq!(
        (code.as_str(), cause.as_deref()),
        ("source-refused", Some("mapping-refused"))
    );
}

#[test]
fn the_files_together_over_their_bound_are_refused_before_the_last_is_read() {
    // The mapping and the first sources are small; one source of exactly 1 GiB is within its
    // own bound and passes the bound on the files together, so it is refused before its read.
    let dir = run_layout("commission_total", "pass");
    sparse(&dir.join("outputs.csv"), 1 << 30);
    let run = commission(&dir);
    assert_eq!(
        refusal(&run),
        (
            "source-refused".to_owned(),
            "references[0].expected[0].path".to_owned(),
            Some("too-many-total-bytes".to_owned())
        )
    );
    assert!(
        detail(&run).contains("at most 1073741824 are read together"),
        "{}",
        detail(&run)
    );
    // That the CXF is not counted, and the bound's edge, are unit tests of the reader
    // (`src/commands/commission_files.rs`), under a small bound.
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_spec_directory_is_file_outside_directory_and_a_link_inside_is_read() {
    let dir = run_layout("commission_links", "pass");
    let outside = dir.join("elsewhere");
    fs::create_dir(&outside).expect("elsewhere");
    let spec_dir = dir.join("spec_dir");
    fs::create_dir(&spec_dir).expect("spec dir");
    for file in [
        "spec.json",
        "points.json",
        "inputs.csv",
        "outputs.csv",
        "fan.csv",
        "plant.cxf.json",
    ] {
        fs::rename(dir.join(file), spec_dir.join(file)).expect("move");
    }
    let reference = commission(&spec_dir);
    assert_eq!(reference.code, 0, "{}", reference.transcript());
    // A source moved out, a link left in its place.
    fs::rename(spec_dir.join("inputs.csv"), outside.join("inputs.csv")).expect("move out");
    std::os::unix::fs::symlink(outside.join("inputs.csv"), spec_dir.join("inputs.csv"))
        .expect("a link out");
    let run = commission(&spec_dir);
    assert_eq!(
        refusal(&run),
        (
            "file-outside-directory".to_owned(),
            "references[0].inputs[0].path".to_owned(),
            None
        )
    );
    assert_eq!(
        detail(&run),
        "trend source `inputs.csv`: it resolves to a file outside the spec's directory"
    );
    // The CXF through a relative link out: the same refusal at its own field.
    fs::remove_file(spec_dir.join("inputs.csv")).expect("unlink");
    fs::rename(outside.join("inputs.csv"), spec_dir.join("inputs.csv")).expect("move back");
    fs::rename(
        spec_dir.join("plant.cxf.json"),
        outside.join("plant.cxf.json"),
    )
    .expect("out");
    std::os::unix::fs::symlink(
        "../elsewhere/plant.cxf.json",
        spec_dir.join("plant.cxf.json"),
    )
    .expect("a relative link out");
    let (code, at, _) = refusal(&commission(&spec_dir));
    assert_eq!(
        (code.as_str(), at.as_str()),
        ("file-outside-directory", "references[0].cxf")
    );
    // A link that stays inside the directory is followed and read: the same receipt.
    fs::remove_file(spec_dir.join("plant.cxf.json")).expect("unlink");
    fs::create_dir(spec_dir.join("models")).expect("models");
    fs::rename(
        outside.join("plant.cxf.json"),
        spec_dir.join("models/plant.cxf.json"),
    )
    .expect("in");
    std::os::unix::fs::symlink("models/plant.cxf.json", spec_dir.join("plant.cxf.json"))
        .expect("a link inside");
    assert_eq!(commission(&spec_dir).stdout, reference.stdout);
}

#[test]
fn a_directory_where_a_file_is_named_is_not_a_file() {
    let dir = run_layout("commission_directory", "pass");
    fs::remove_file(dir.join("points.json")).expect("remove");
    fs::create_dir(dir.join("points.json")).expect("a directory");
    let (code, at, _) = refusal(&commission(&dir));
    assert_eq!(
        (code.as_str(), at.as_str()),
        ("not-a-file", "references[0].pointNameMapping")
    );
}

#[cfg(unix)]
#[test]
fn a_named_pipe_is_never_opened_as_the_spec_or_a_file_it_names() {
    // A named pipe blocks an open for reading until a writer comes; each path is judged a
    // regular file from its metadata first. The guard is a hang guard, not a speed assertion.
    let guard = std::time::Duration::from_secs(120);
    for pipe in ["pipe.json", "fan.csv", "plant.cxf.json"] {
        let dir = run_layout(&format!("commission_pipe_{pipe}"), "pass");
        let _ = fs::remove_file(dir.join(pipe));
        let made = std::process::Command::new("mkfifo")
            .arg(dir.join(pipe))
            .status()
            .expect("mkfifo runs");
        assert!(made.success(), "{pipe}");
        let spec = if pipe == "pipe.json" {
            "pipe.json"
        } else {
            SPEC
        };
        let run = oce_in_guarded(&dir, &["verify", "--commission", spec], guard);
        assert_eq!(refusal(&run).0, "not-a-file", "{pipe}");
    }
}
