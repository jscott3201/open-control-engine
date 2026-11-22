//! The read-containment rule raced through the built binary (issue #758, ruled by the owner on
//! 2026-11-22): a commissioning spec names its CXF as `sub/plant.cxf.json`, and a thread swaps
//! `sub` for a link to a directory outside the spec's, as fast as it can, while `oce verify
//! --commission` runs again and again. The file outside is larger than the CXF bound, so a run
//! whose open landed outside answers `input-too-large`: the escape this test counts, and must
//! never see. Every other run reads the CXF inside (exit `0`) or is refused for the swap it met
//! (`file-outside-directory` when the canonicalization saw the link, `read-failed` when a
//! component changed under the walk, `not-found`).
//!
//! The deterministic proof is the adversary of `src/commands/contained_tests.rs`, which beats the
//! path-based checks of GitHub #757 and not the `openat` walk; this test holds the binary to the
//! same rule under a real race. The swap is an atomic exchange (`renameat_with` with
//! `RenameFlags::EXCHANGE`), so it runs on Linux and macOS; Windows keeps the path-based checks
//! and is untested here.

#![cfg(any(target_os = "linux", target_os = "macos"))]

mod support;

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rustix::fs::{CWD, RenameFlags, renameat_with};
use support::commission::{RUN_FILES, run_file, subject};
use support::{Run, oce_in, scratch};

/// How many runs race the swap.
const RUNS: usize = 300;

/// One more byte than an engine's CXF bound: an open that lands outside is refused for its size.
fn over_the_bound() -> u64 {
    u64::try_from(oce_api::MAX_CXF_BYTES).expect("fits") + 1
}

/// `base/root` holds the run fixture with its CXF moved to `sub/`, `base/root/sub.alt` is a link
/// to `../outside`, and `base/outside/plant.cxf.json` is a sparse file past the CXF bound.
fn lay_out(base: &Path) {
    let root = base.join("root");
    fs::create_dir_all(root.join("sub")).expect("root/sub");
    fs::create_dir_all(base.join("outside")).expect("outside");
    let spec = String::from_utf8(run_file("spec.json"))
        .expect("UTF-8")
        .replacen(
            r#""cxf": "plant.cxf.json""#,
            r#""cxf": "sub/plant.cxf.json""#,
            1,
        );
    assert!(spec.contains("sub/plant.cxf.json"));
    fs::write(root.join("spec.json"), spec).expect("spec");
    for file in RUN_FILES {
        fs::write(root.join(file), run_file(file)).expect("a run file");
    }
    fs::write(root.join("sub/plant.cxf.json"), subject("plant")).expect("the CXF inside");
    let outside = fs::File::create(base.join("outside/plant.cxf.json")).expect("outside");
    outside.set_len(over_the_bound()).expect("a sparse file");
    std::os::unix::fs::symlink("../outside", root.join("sub.alt")).expect("the link out");
}

fn run(root: &Path) -> Run {
    oce_in(root, &["verify", "--commission", "spec.json"], b"")
}

/// The code of a refused run, or `None` for a run that reached a verdict.
fn code(run: &Run) -> Option<String> {
    if run.code < 2 {
        return None;
    }
    let report = run.report();
    Some(
        report["errors"][0]["code"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
    )
}

#[test]
fn a_directory_swapped_for_a_link_never_carries_a_read_out_of_the_spec_directory() {
    let base = scratch("read_containment_race");
    lay_out(&base);
    let root = fs::canonicalize(base.join("root")).expect("root");
    let (sub, alt) = (root.join("sub"), root.join("sub.alt"));
    let swap = move |sub: &Path, alt: &Path| {
        renameat_with(CWD, sub, CWD, alt, RenameFlags::EXCHANGE).expect("exchange");
    };

    // The controls, with nothing racing: the CXF inside is read; the link at `sub` is refused
    // by the canonicalization; and the file outside, named directly, answers the escape's code.
    assert_eq!(code(&run(&root)), None, "the run reads the CXF inside");
    swap(&sub, &alt);
    assert_eq!(code(&run(&root)).as_deref(), Some("file-outside-directory"));
    swap(&sub, &alt);
    fs::copy(
        base.join("outside/plant.cxf.json"),
        root.join("big.cxf.json"),
    )
    .expect("copy");
    let direct = fs::read_to_string(root.join("spec.json"))
        .expect("spec")
        .replacen("sub/plant.cxf.json", "big.cxf.json", 1);
    fs::write(root.join("direct.json"), direct).expect("direct spec");
    let named = oce_in(&root, &["verify", "--commission", "direct.json"], b"");
    assert_eq!(code(&named).as_deref(), Some("input-too-large"));
    fs::remove_file(root.join("big.cxf.json")).expect("remove");

    let stop = Arc::new(AtomicBool::new(false));
    let swaps = Arc::new(AtomicU64::new(0));
    let racer = {
        let (stop, swaps, sub, alt) = (stop.clone(), swaps.clone(), sub.clone(), alt.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                swap(&sub, &alt);
                swaps.fetch_add(1, Ordering::Relaxed);
            }
        })
    };
    let mut answers: std::collections::BTreeMap<String, usize> = Default::default();
    for _ in 0..RUNS {
        let answer = code(&run(&root)).unwrap_or_else(|| "verdict".to_owned());
        *answers.entry(answer).or_default() += 1;
    }
    stop.store(true, Ordering::Relaxed);
    racer.join().expect("the racer");
    if fs::symlink_metadata(&sub).expect("sub").is_symlink() {
        swap(&sub, &alt);
    }

    assert!(swaps.load(Ordering::Relaxed) > 0, "the racer never swapped");
    assert_eq!(
        answers.get("input-too-large"),
        None,
        "a read escaped the spec's directory: {answers:?}"
    );
    for answer in answers.keys() {
        assert!(
            [
                "verdict",
                "file-outside-directory",
                "read-failed",
                "not-found"
            ]
            .contains(&answer.as_str()),
            "an answer the race cannot explain: {answers:?}"
        );
    }
    assert!(
        answers.keys().any(|answer| answer != "verdict"),
        "no run met the swap, so the race proved nothing: {answers:?}"
    );
}
