//! Issue #771's three races through the built binary, each against a swapper that exchanges a
//! directory or an entry atomically while `oce` runs:
//!
//! - `verify --commission --out DIR`: `DIR/outputs` exchanged for a link to a victim directory
//!   the moment `outputs/0` appears (the reviewer's reproduction waited for the first curve's
//!   temporary file, and sent the curves into the victim in 50 runs of 50 before the change;
//!   waiting for the folder catches a wider window, which a fast file system can otherwise
//!   finish inside);
//! - `verify --commission SPEC` and `verify VECTORS`: the document's folder exchanged flat out
//!   with another folder whose files of the same names are over-size decoys, so a run that pairs
//!   one folder's document with the other's file answers `input-too-large`;
//! - `fmt --write FILE`: `FILE` exchanged flat out with a link to a victim outside its directory,
//!   so a run that read through the link writes the victim's canonical text into the directory.
//!
//! Each must never happen. The deterministic proofs are the adversaries of
//! `src/commands/contained_tests.rs`, `atomic_write_tests.rs` and `commission_out_tests.rs`,
//! which try every schedule of the swap between the code's own calls and beat the path-based
//! code that came before; these hold the binary to the same rules under a real race. Two more
//! exchange a named pipe in for the document and for `FILE`, under a hang guard: a run must never
//! block on it. The swap is an atomic exchange (`renameat_with` with `RenameFlags::EXCHANGE`), so
//! these run on Linux and macOS; a file system under cargo's target directory that refuses the
//! exchange (`EINVAL`, `ENOTSUP`, `ENOSYS`) skips each with a note rather than failing. Windows
//! keeps the path-based reads and writes and is untested here.

#![cfg(any(target_os = "linux", target_os = "macos"))]

mod support;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use rustix::fs::{CWD, RenameFlags, renameat_with};
use rustix::io::Errno;
use support::commission::{RUN_FILES, run_file, run_layout, subject};
use support::{Run, crate_dir, oce_in, oce_in_guarded, scratch};

/// The hang guard of a run that meets a named pipe: far longer than any run takes.
const GUARD: Duration = Duration::from_secs(60);

/// One byte past an engine's CXF bound.
fn over_the_bound() -> u64 {
    u64::try_from(oce_api::MAX_CXF_BYTES).expect("fits") + 1
}

/// Exchange `a` and `b`; whether the file system can (a skip note when it cannot).
fn exchangeable(a: &Path, b: &Path) -> bool {
    match renameat_with(CWD, a, CWD, b, RenameFlags::EXCHANGE) {
        Ok(()) => {
            renameat_with(CWD, a, CWD, b, RenameFlags::EXCHANGE).expect("exchange back");
            true
        }
        Err(error)
            if [Errno::INVAL, Errno::NOTSUP, Errno::OPNOTSUPP, Errno::NOSYS].contains(&error) =>
        {
            support::skip_without_exchange(error);
            false
        }
        Err(error) => panic!("the exchange failed: {error:?}"),
    }
}

/// A thread exchanging `a` and `b` as fast as it can until stopped; it counts its swaps.
struct Swapper {
    stop: Arc<AtomicBool>,
    swaps: Arc<AtomicU64>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Swapper {
    fn start(a: PathBuf, b: PathBuf) -> Self {
        let (stop, swaps) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicU64::new(0)),
        );
        let thread = {
            let (stop, swaps) = (stop.clone(), swaps.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if renameat_with(CWD, &a, CWD, &b, RenameFlags::EXCHANGE).is_ok() {
                        swaps.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        };
        Self {
            stop,
            swaps,
            thread: Some(thread),
        }
    }

    /// Stop, and how many swaps there were; an odd count leaves `a` and `b` exchanged.
    fn stop(mut self) -> u64 {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.join().expect("the swapper");
        }
        self.swaps.load(Ordering::Relaxed)
    }
}

/// The answer of a run: `verdict` for exit 0 or 1, else its first error's code.
fn answer(run: &Run) -> String {
    if run.code < 2 {
        return "verdict".to_owned();
    }
    run.report()["errors"][0]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn tally(answers: &mut BTreeMap<String, usize>, answer: String) {
    *answers.entry(answer).or_default() += 1;
}

/// The names in `dir`, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("list")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn outputs_swapped_for_a_link_mid_write_never_carries_a_curve_out_of_dir() {
    // The reviewer's reproduction of issue #771 (i): `out/outl` is a link to `victim`, and
    // `victim/0` a link back to `out/outl/0`; once `out/outputs/0` exists, `outputs` and `outl`
    // exchange, so every later folder and curve is made while `outputs` names the victim.
    const RUNS: usize = 20;
    let dir = run_layout("handle_race_outputs", "pass");
    let (out, victim) = (dir.join("out"), dir.join("victim"));
    fs::create_dir_all(dir.join("probe/a")).expect("probe");
    fs::create_dir_all(dir.join("probe/b")).expect("probe");
    if !exchangeable(&dir.join("probe/a"), &dir.join("probe/b")) {
        return;
    }
    let mut answers = BTreeMap::new();
    let mut swapped = 0;
    for _ in 0..RUNS {
        let _ = fs::remove_dir_all(&out);
        let _ = fs::remove_dir_all(&victim);
        fs::create_dir(&out).expect("out");
        fs::create_dir(&victim).expect("victim");
        std::os::unix::fs::symlink(out.join("outl/0"), victim.join("0")).expect("victim/0");
        std::os::unix::fs::symlink(&victim, out.join("outl")).expect("outl");
        let done = Arc::new(AtomicBool::new(false));
        let helper = {
            let (done, out) = (done.clone(), out.clone());
            std::thread::spawn(move || {
                while !done.load(Ordering::Relaxed) {
                    if fs::symlink_metadata(out.join("outputs/0")).is_ok() {
                        return renameat_with(
                            CWD,
                            out.join("outputs"),
                            CWD,
                            out.join("outl"),
                            RenameFlags::EXCHANGE,
                        )
                        .is_ok();
                    }
                }
                false
            })
        };
        let run = oce_in(
            &dir,
            &[
                "verify",
                "--commission",
                "spec.json",
                "--out",
                "out",
                "--bounds",
            ],
            b"",
        );
        done.store(true, Ordering::Relaxed);
        swapped += usize::from(helper.join().expect("the helper"));
        assert_eq!(names(&victim), ["0"], "a curve reached the victim");
        tally(&mut answers, answer(&run));
        // Every file a run lists exists under the directory it was written to: `outl` after the
        // exchange, `outputs` otherwise; never under the victim.
        let report = run.report();
        for output in report["curves"].as_array().into_iter().flatten() {
            for file in output["files"].as_array().into_iter().flatten() {
                let file = file.as_str().expect("a name");
                let below = file.strip_prefix("out/outputs/").expect("under DIR");
                assert!(
                    out.join("outputs").join(below).is_file()
                        || fs::symlink_metadata(out.join("outl").join(below))
                            .is_ok_and(|m| m.is_file()),
                    "{file}"
                );
            }
        }
    }
    assert!(swapped > 0, "the exchange never landed: {answers:?}");
    for answer in answers.keys() {
        assert!(
            ["verdict", "write-failed"].contains(&answer.as_str()),
            "an answer the race cannot explain: {answers:?}"
        );
    }
}

/// Two folders `cur` and `alt` under `base` that exchange: each holds its own document, naming
/// its own file, and the other folder's file name as an over-size decoy.
fn pairing_layout(base: &Path, vectors: bool) -> Vec<&'static str> {
    let (a, b) = (base.join("cur"), base.join("alt"));
    for dir in [&a, &b] {
        fs::create_dir_all(dir).expect("dir");
    }
    let decoy = |path: PathBuf| {
        fs::File::create(path)
            .expect("decoy")
            .set_len(over_the_bound())
            .expect("sparse");
    };
    if vectors {
        let rule = crate_dir().join("../../oce-api/tests/fixtures/library_rules/AHU-0016");
        fs::copy(rule.join("rule.cxf.jsonld"), base.join("rule.cxf.jsonld")).expect("rule");
        let mutant = crate_dir().join("../oce-verify/tests/fixtures/AHU-0016/mutants");
        let set = |listed: &str| {
            serde_json::json!({
                "schema": "oce/vectors/v1",
                "clock": { "step_s": 60, "horizon_s": 60 },
                "mutants": [{ "path": listed }],
                "scenarios": [{
                    "name": "both_closed",
                    "inputs": { "htg_vlv_cmd": 0.0, "clg_vlv_cmd": 0.0 },
                    "expect": [{ "output": "yFault", "from_s": 0, "to_s": 60, "equals": false }]
                }]
            })
            .to_string()
        };
        for (dir, own, other) in [(&a, "a", "b"), (&b, "b", "a")] {
            fs::write(dir.join("vectors.json"), set(&format!("{own}.cxf.json"))).expect("set");
            fs::copy(
                mutant.join("late_alarm.cxf.json"),
                dir.join(format!("{own}.cxf.json")),
            )
            .expect("mutant");
            decoy(dir.join(format!("{other}.cxf.json")));
        }
        vec!["verify", "cur/vectors.json", "--subject", "rule.cxf.jsonld"]
    } else {
        let spec = String::from_utf8(run_file("spec.json")).expect("UTF-8");
        for (dir, own, other) in [(&a, "a", "b"), (&b, "b", "a")] {
            for file in RUN_FILES {
                fs::write(dir.join(file), run_file(file)).expect("a run file");
            }
            let named = spec.replacen(
                r#""cxf": "plant.cxf.json""#,
                &format!(r#""cxf": "{own}.cxf.json""#),
                1,
            );
            fs::write(dir.join("spec.json"), named).expect("spec");
            fs::write(dir.join(format!("{own}.cxf.json")), subject("plant")).expect("cxf");
            decoy(dir.join(format!("{other}.cxf.json")));
        }
        vec!["verify", "--commission", "cur/spec.json"]
    }
}

fn no_pairing_across_folders(test: &str, vectors: bool) {
    const RUNS: usize = 300;
    let base = scratch(test);
    let args = pairing_layout(&base, vectors);
    let (a, b) = (base.join("cur"), base.join("alt"));
    if !exchangeable(&a, &b) {
        return;
    }
    // The control, nothing racing: the run reads its own folder.
    assert_eq!(answer(&oce_in(&base, &args, b"")), "verdict");
    let swapper = Swapper::start(a, b);
    let mut answers = BTreeMap::new();
    for _ in 0..RUNS {
        tally(&mut answers, answer(&oce_in(&base, &args, b"")));
    }
    assert!(swapper.stop() > 0, "the swapper never swapped");
    assert_eq!(
        answers.get("input-too-large"),
        None,
        "a document was paired with another folder's file: {answers:?}"
    );
    // A canonicalization that resolves while the folders exchange can see the other folder's
    // path (macOS's `realpath` reads the path of the file it opened), and refuses the file as
    // outside the directory: a refusal, never a pairing.
    for answer in answers.keys() {
        assert!(
            [
                "verdict",
                "read-failed",
                "not-found",
                "file-outside-directory",
                "mutant-outside-directory"
            ]
            .contains(&answer.as_str()),
            "an answer the race cannot explain: {answers:?}"
        );
    }
}

#[test]
fn a_spec_folder_exchanged_mid_run_never_pairs_the_spec_with_another_folders_files() {
    no_pairing_across_folders("handle_race_spec", false);
}

#[test]
fn a_vector_folder_exchanged_mid_run_never_pairs_the_set_with_another_folders_mutant() {
    no_pairing_across_folders("handle_race_vectors", true);
}

/// `base/d/t.oce` (a text file that is not canonical) and `base/victim.oce` outside `d`.
fn fmt_layout(test: &str) -> (PathBuf, Vec<u8>, Vec<u8>) {
    let base = scratch(test);
    fs::create_dir_all(base.join("d")).expect("d");
    let fixtures = crate_dir().join("../oce-author/tests/fixtures/text/fmt");
    let text = fs::read(fixtures.join("comments.in.oce")).expect("text");
    let victim = fs::read(fixtures.join("sections.in.oce")).expect("victim");
    fs::write(base.join("victim.oce"), &victim).expect("victim");
    (base, text, victim)
}

#[test]
fn a_link_exchanged_in_for_the_file_fmt_rewrites_is_never_read_or_written_through() {
    const RUNS: usize = 300;
    let (base, text, victim) = fmt_layout("handle_race_fmt");
    let canonical_victim = oce_in(&base, &["fmt", "victim.oce"], b"").stdout;
    assert!(!canonical_victim.is_empty());
    let (d, t, alt) = (base.join("d"), base.join("d/t.oce"), base.join("d/t.alt"));
    fs::write(&t, &text).expect("t");
    std::os::unix::fs::symlink("../victim.oce", &alt).expect("link");
    if !exchangeable(&t, &alt) {
        return;
    }
    let mut answers = BTreeMap::new();
    for _ in 0..RUNS {
        let swapper = Swapper::start(t.clone(), alt.clone());
        let run = oce_in(&d, &["fmt", "--write", "t.oce"], b"");
        swapper.stop();
        tally(&mut answers, answer(&run));
        for name in [&t, &alt] {
            let entry = fs::symlink_metadata(name).expect("entry");
            assert!(
                !(entry.is_file() && fs::read(name).expect("read") == canonical_victim),
                "the victim's text was written into the directory: {answers:?}"
            );
        }
        assert_eq!(fs::read(base.join("victim.oce")).expect("victim"), victim);
        // Reset: the text file at `t.oce`, the link at `t.alt`.
        let _ = fs::remove_file(&t);
        let _ = fs::remove_file(&alt);
        fs::write(&t, &text).expect("t");
        std::os::unix::fs::symlink("../victim.oce", &alt).expect("link");
    }
    assert!(
        answers.contains_key("output-not-a-file"),
        "no run met the link, so the race proved nothing: {answers:?}"
    );
    for answer in answers.keys() {
        assert!(
            [
                "verdict",
                "output-not-a-file",
                "read-failed",
                "write-failed"
            ]
            .contains(&answer.as_str()),
            "an answer the race cannot explain: {answers:?}"
        );
    }
}

/// `mkfifo` at `path`.
fn fifo(path: &Path) {
    let made = std::process::Command::new("mkfifo")
        .arg(path)
        .status()
        .expect("mkfifo runs");
    assert!(made.success());
}

#[test]
fn a_named_pipe_exchanged_in_for_a_document_or_for_fmts_file_never_blocks_a_run() {
    // A pipe swapped in after an entry's check is opened with `O_NONBLOCK` and refused by its
    // kind; a blocking open would wait for a writer forever, which the guard turns into a
    // failure.
    const RUNS: usize = 100;
    let dir = run_layout("handle_race_pipe_spec", "pass");
    fifo(&dir.join("spec.fifo"));
    if !exchangeable(&dir.join("spec.json"), &dir.join("spec.fifo")) {
        return;
    }
    let swapper = Swapper::start(dir.join("spec.json"), dir.join("spec.fifo"));
    let mut answers = BTreeMap::new();
    for _ in 0..RUNS {
        let run = oce_in_guarded(&dir, &["verify", "--commission", "spec.json"], GUARD);
        tally(&mut answers, answer(&run));
    }
    swapper.stop();
    for answer in answers.keys() {
        assert!(
            ["verdict", "not-a-file", "read-failed"].contains(&answer.as_str()),
            "{answers:?}"
        );
    }
    assert!(answers.contains_key("not-a-file"), "{answers:?}");

    let (base, text, _) = fmt_layout("handle_race_pipe_fmt");
    let d = base.join("d");
    fs::write(d.join("t.oce"), &text).expect("t");
    fifo(&d.join("t.fifo"));
    let swapper = Swapper::start(d.join("t.oce"), d.join("t.fifo"));
    let mut answers = BTreeMap::new();
    for _ in 0..RUNS {
        let run = oce_in_guarded(&d, &["fmt", "--write", "t.oce"], GUARD);
        tally(&mut answers, answer(&run));
    }
    swapper.stop();
    for answer in answers.keys() {
        assert!(
            [
                "verdict",
                "output-not-a-file",
                "read-failed",
                "write-failed"
            ]
            .contains(&answer.as_str()),
            "{answers:?}"
        );
    }
}
