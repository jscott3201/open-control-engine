//! The tests of `commission_out.rs`: the checks, the writes, and the adversary of issue #771,
//! which swaps `DIR/outputs`, or `DIR` itself, for a link to a victim directory at every point
//! between the writer's own file-system calls.

use std::path::PathBuf;

use super::*;

/// `dir` opened as [`check()`] opens it, without its checks (a test of the write alone).
fn opened(dir: &Path) -> Out<'_> {
    Out {
        dir: dir.as_os_str(),
        handle: Directory::open(dir).expect("the directory"),
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "oce-cli-commission-out-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create the directory");
    dir
}

#[test]
fn an_outputs_directory_made_after_the_check_still_refuses_the_write_and_writes_nothing() {
    // The race the check before the run cannot close: `outputs/` appears between the check
    // and the write. The plain create refuses it, and the receipt is not written.
    let dir = scratch("race");
    let out = check(dir.as_os_str(), &[]).expect("checked");
    fs::create_dir(dir.join(OUTPUTS)).expect("a racing writer's outputs");
    let refused = write(&out, &[], b"{}").expect_err("refused");
    assert_eq!(
        (refused.code, refused.at.as_str()),
        (codes::WRITE_FAILED, OUTPUTS)
    );
    // The refusal of an existing `outputs/`, not a failed create's generic one.
    assert_eq!(refused.detail, outputs_exist(dir.as_os_str()).detail);
    assert!(!dir.join(RECEIPT).exists());
    let _ = fs::remove_dir_all(&dir);
}

/// `count` outputs' curves, each a real run's (the type cannot be built outside its crate).
fn curves(count: usize) -> Vec<OutputCurves> {
    thread_local! {
        /// The real run's curves, made once per thread: the adversaries write thousands of times.
        static ONE: OutputCurves = sample_curves();
    }
    let one = ONE.with(OutputCurves::clone);
    (0..count)
        .map(|k| {
            let mut copy = one.clone();
            copy.output = format!("urn:doc#top.y{k}");
            copy
        })
        .collect()
}

/// One `OutputCurves` from a real run (the type is non-exhaustive outside its crate).
fn sample_curves() -> OutputCurves {
    let cxf = br#"{"@context": {"S231": "http://data.ashrae.org/S231#"}, "@graph": [
  {"@id": "urn:doc#top", "@type": "S231:Block",
   "S231:containsBlock": [{"@id": "urn:doc#top.not"}],
   "S231:hasInput": [{"@id": "urn:doc#top.u"}], "S231:hasOutput": [{"@id": "urn:doc#top.y"}]},
  {"@id": "urn:doc#top.not", "@type": "CDL.Logical.Not",
   "S231:hasInput": {"@id": "urn:doc#top.not.u"}, "S231:hasOutput": {"@id": "urn:doc#top.not.y"}},
  {"@id": "urn:doc#top.not.u", "@type": "S231:BooleanInput",
   "S231:isOfDataType": {"@id": "S231:Boolean"}},
  {"@id": "urn:doc#top.not.y", "@type": "S231:BooleanOutput",
   "S231:isOfDataType": {"@id": "S231:Boolean"}, "S231:isConnectedTo": {"@id": "urn:doc#top.y"}},
  {"@id": "urn:doc#top.u", "@type": "S231:BooleanInput",
   "S231:isOfDataType": {"@id": "S231:Boolean"}, "S231:isConnectedTo": {"@id": "urn:doc#top.not.u"}},
  {"@id": "urn:doc#top.y", "@type": "S231:BooleanOutput",
   "S231:isOfDataType": {"@id": "S231:Boolean"}}
]}"#;
    let spec = oce_verify::CommissioningSpec::from_json_str(
        r#"{"references": [{"model": "Inverter", "sequence": "inv", "cxf": "inv.json",
  "pointNameMapping": [
{"cdl": {"name": "u", "type": "bool"}, "device": {"name": "Fan", "type": "bool"}},
{"cdl": {"name": "y", "type": "bool"}, "device": {"name": "Alarm", "type": "bool"}}],
  "window": {"start": 0, "stop": 10}, "sampling": 10,
  "inputs": [{"path": "fan.csv", "format": "csv", "timeColumn": "time",
          "columns": {"Fan": "Fan"}}],
  "expected": [{"path": "alarm.csv", "format": "csv", "timeColumn": "time",
            "columns": {"Alarm": "Alarm"}}]}]}"#,
        &oce_verify::SpecLimits::default(),
    )
    .expect("the spec loads");
    let inputs = oce_verify::SpecInputs {
        cxf: cxf.to_vec(),
        files: vec![
            (
                "fan.csv".to_owned(),
                b"time,Fan\n0,true\n10,false\n".to_vec(),
            ),
            (
                "alarm.csv".to_owned(),
                b"time,Alarm\n0,false\n10,true\n".to_vec(),
            ),
        ],
    };
    let run =
        oce_verify::run_commissioning(&spec, 0, &inputs, &oce_verify::CommissionOptions::default())
            .expect("runs");
    run.curves.into_iter().next().expect("one curve")
}

#[test]
fn a_receipt_write_that_fails_lists_every_curve_written_before_it() {
    // A non-empty directory where the receipt goes (the check would refuse it; this is the
    // write alone): the rename fails after both outputs' curves are written.
    let dir = scratch("receipt_fails");
    fs::create_dir_all(dir.join(RECEIPT).join("inside")).expect("a directory at the receipt");
    let refused = write(&opened(&dir), &curves(2), b"{}").expect_err("refused");
    assert_eq!(
        (refused.code, refused.at.as_str()),
        (codes::WRITE_FAILED, RECEIPT)
    );
    let shown_dir = dir.to_string_lossy().into_owned();
    let listed: Vec<(usize, Vec<String>)> = refused
        .written
        .iter()
        .map(|output| (output.index, output.files.clone()))
        .collect();
    assert_eq!(
        listed,
        [
            (0, vec![format!("{shown_dir}/outputs/0/errors.csv")]),
            (1, vec![format!("{shown_dir}/outputs/1/errors.csv")]),
        ]
    );
    for k in 0..2 {
        assert!(dir.join(format!("outputs/{k}/errors.csv")).is_file());
    }
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_link_planted_at_the_receipt_after_the_check_is_replaced_and_its_target_untouched() {
    // The race the check cannot close: a link appears at `receipt.json` between the check
    // and the write. The write replaces the entry and never writes where it points.
    let dir = scratch("planted_receipt");
    let victim = dir.join("victim.txt");
    fs::write(&victim, b"PRECIOUS").expect("victim");
    let out = dir.join("out");
    fs::create_dir(&out).expect("out");
    let checked = check(out.as_os_str(), &[]).expect("checked");
    std::os::unix::fs::symlink(&victim, out.join(RECEIPT)).expect("a planted link");
    let written = write(&checked, &curves(1), b"{}\n").expect("written");
    assert_eq!(written.len(), 1);
    assert_eq!(fs::read(&victim).expect("victim"), b"PRECIOUS");
    let entry = fs::symlink_metadata(out.join(RECEIPT)).expect("the receipt");
    assert!(entry.file_type().is_file());
    assert_eq!(fs::read(out.join(RECEIPT)).expect("receipt"), b"{}\n");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_non_file_that_appears_after_the_first_check_is_refused_by_the_second() {
    // `target()` runs before anything is read and `check()` after the reads; a link planted
    // between them is refused as the first check refuses it, and a regular file stays the
    // stale receipt `write-failed` names.
    let dir = scratch("late_link");
    let out = dir.join("out");
    fs::create_dir(&out).expect("out");
    assert!(target(out.as_os_str()).is_ok());
    std::os::unix::fs::symlink(dir.join("victim.txt"), out.join(RECEIPT)).expect("a link");
    let refused = check(out.as_os_str(), &[]).expect_err("a link");
    assert_eq!(
        (refused.code, refused.at.as_str()),
        (codes::OUTPUT_NOT_A_FILE, RECEIPT)
    );
    assert_eq!(
        target(out.as_os_str()).expect_err("a link").code,
        codes::OUTPUT_NOT_A_FILE
    );
    fs::remove_file(out.join(RECEIPT)).expect("unlink");
    fs::write(out.join(RECEIPT), b"{}\n").expect("a stale receipt");
    assert!(
        target(out.as_os_str()).is_ok(),
        "a regular file passes the first check"
    );
    let refused = check(out.as_os_str(), &[]).expect_err("a stale receipt");
    assert_eq!(refused.code, codes::WRITE_FAILED);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_run_with_no_compared_output_writes_an_empty_outputs_and_the_receipt() {
    let dir = scratch("empty");
    let written = write(&opened(&dir), &[], b"{}\n").expect("written");
    assert!(written.is_empty());
    assert!(dir.join(OUTPUTS).is_dir());
    assert_eq!(fs::read_dir(dir.join(OUTPUTS)).expect("list").count(), 0);
    assert_eq!(fs::read(dir.join(RECEIPT)).expect("receipt"), b"{}\n");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_shown_name_keeps_dir_as_given_with_one_slash() {
    assert_eq!(shown(OsStr::new("out"), RECEIPT), "out/receipt.json");
    assert_eq!(shown(OsStr::new("out/"), RECEIPT), "out/receipt.json");
    // The root directory keeps its slash; an empty DIR (refused before any report names
    // it) is never shown as the root's file.
    assert_eq!(shown(OsStr::new("/"), RECEIPT), "/receipt.json");
    assert_eq!(shown(OsStr::new(""), RECEIPT), "receipt.json");
    assert_eq!(
        shown(OsStr::new("."), "outputs/0/errors.csv"),
        "./outputs/0/errors.csv"
    );
    let missing = failed(OsStr::new("absent"), ".", "gone");
    assert_eq!(missing.detail, "`absent`: gone");
}

#[cfg(unix)]
#[test]
fn a_dir_removed_after_the_check_is_a_typed_refusal_that_writes_nothing() {
    let dir = scratch("vanished");
    let out = dir.join("out");
    fs::create_dir(&out).expect("out");
    let checked = check(out.as_os_str(), &[]).expect("checked");
    fs::remove_dir(&out).expect("DIR vanishes mid-run");
    let refused = write(&checked, &curves(1), b"{}\n").expect_err("refused");
    assert_eq!(
        (refused.code, refused.at.as_str()),
        (codes::WRITE_FAILED, OUTPUTS)
    );
    assert!(
        refused.detail.contains("it could not be created"),
        "{}",
        refused.detail
    );
    assert!(refused.written.is_empty());
    assert_eq!(
        fs::read_dir(&dir).expect("list").count(),
        0,
        "nothing written"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_dir_that_can_be_searched_but_not_listed_is_refused_as_unreadable() {
    // The handle is opened read-only (rustix has no `O_SEARCH` on macOS): issue #771 made this
    // a refusal where the path-based writer passed through.
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("search_only");
    let out = dir.join("out");
    fs::create_dir(&out).expect("out");
    fs::set_permissions(&out, fs::Permissions::from_mode(0o300)).expect("chmod");
    let listable = fs::read_dir(&out).is_ok();
    let result = check(out.as_os_str(), &[]).map(|_| ());
    fs::set_permissions(&out, fs::Permissions::from_mode(0o755)).expect("chmod back");
    if !listable {
        let refused = result.expect_err("refused");
        assert_eq!(
            (refused.code, refused.at.as_str()),
            (codes::WRITE_FAILED, ".")
        );
        assert!(
            refused
                .detail
                .ends_with("the output directory cannot be read: permission denied"),
            "{}",
            refused.detail
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

/// The adversary of [`write()`] (module header): every schedule of swaps over its points.
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod schedules {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use rustix::fs::{CWD, RenameFlags, renameat_with};

    use super::super::{OUTPUTS, RECEIPT, check, write};
    use super::curves;
    use crate::commands::steps::{step, with_adversary};

    /// What one schedule left.
    #[derive(Debug, PartialEq, Eq)]
    struct Left {
        /// The victim directory holds only `keep`, unchanged.
        victim_untouched: bool,
        /// A temporary file is left somewhere.
        stranded: bool,
        /// The write answered `Ok`.
        written: bool,
    }

    /// `base/out` (`DIR`), `base/victim` holding `keep`, and the link the adversary exchanges:
    /// `base/out/outl -> victim/` for `outputs`, or `base/out.alt -> victim/` for `DIR` itself.
    struct Layout {
        base: PathBuf,
        name: PathBuf,
        link: PathBuf,
        /// The victim holds nothing (a directory a made directory's emptiness check accepts).
        empty_victim: bool,
    }

    impl Layout {
        fn make(test: &str, swap_dir: bool, empty_victim: bool) -> Option<Self> {
            let base = std::env::temp_dir().join(format!(
                "oce-cli-commission-schedule-{test}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&base);
            fs::create_dir_all(&base).expect("base");
            let base = fs::canonicalize(&base).expect("canonical");
            let (name, link) = if swap_dir {
                (base.join("out"), base.join("out.alt"))
            } else {
                (base.join("out").join(OUTPUTS), base.join("out/outl"))
            };
            let layout = Self {
                base,
                name,
                link,
                empty_victim,
            };
            layout.reset();
            // Probe the exchange on two scratch directories.
            let (a, b) = (layout.base.join("probe.a"), layout.base.join("probe.b"));
            fs::create_dir(&a).expect("a");
            fs::create_dir(&b).expect("b");
            let probed = renameat_with(CWD, &a, CWD, &b, RenameFlags::EXCHANGE);
            let _ = fs::remove_dir(&a);
            let _ = fs::remove_dir(&b);
            if let Err(error) = probed {
                #[allow(
                    clippy::print_stderr,
                    reason = "an explicit skip note in the test output"
                )]
                {
                    eprintln!("skipped: no atomic exchange on this file system ({error:?})");
                }
                return None;
            }
            Some(layout)
        }

        /// A fresh `DIR`, the victim and the link.
        fn reset(&self) {
            for dir in ["out", "victim", "out.alt"] {
                let path = self.base.join(dir);
                if fs::symlink_metadata(&path).is_ok_and(|m| m.is_symlink()) {
                    fs::remove_file(&path).expect("unlink");
                } else {
                    let _ = fs::remove_dir_all(&path);
                }
            }
            fs::create_dir(self.base.join("out")).expect("out");
            fs::create_dir(self.base.join("victim")).expect("victim");
            if !self.empty_victim {
                // `keep`, and a `0/` where a curve written by path through the link would land.
                fs::write(self.base.join("victim/keep"), b"PRECIOUS").expect("keep");
                fs::create_dir(self.base.join("victim/0")).expect("victim/0");
            }
            std::os::unix::fs::symlink(self.base.join("victim"), &self.link).expect("link");
        }

        /// Exchange the name and the link; a name that does not exist yet (`outputs` before it
        /// is made) is no swap.
        fn swap(name: &Path, link: &Path) {
            let _ = renameat_with(CWD, name, CWD, link, RenameFlags::EXCHANGE);
        }

        fn left(&self, written: bool) -> Left {
            let victim = self.base.join("victim");
            let mut names: Vec<String> = fs::read_dir(&victim)
                .expect("list")
                .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            Left {
                victim_untouched: if self.empty_victim {
                    names.is_empty()
                } else {
                    names == ["0", "keep"]
                        && fs::read(victim.join("keep")).expect("keep") == b"PRECIOUS"
                        && fs::read_dir(victim.join("0")).expect("victim/0").count() == 0
                },
                stranded: stranded(&self.base),
                written,
            }
        }
    }

    impl Drop for Layout {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    /// Whether a temporary file is left anywhere under `dir`, following no link.
    fn stranded(dir: &Path) -> bool {
        fs::read_dir(dir).expect("list").any(|entry| {
            let entry = entry.expect("entry");
            let kind = entry.file_type().expect("kind");
            entry.file_name().to_string_lossy().ends_with(".oce-tmp")
                || (kind.is_dir() && stranded(&entry.path()))
        })
    }

    /// `write()` as it was before issue #771, with the adversary at the same points: every
    /// directory and file reached by path.
    fn path_based(dir: &Path, csv: &[u8], receipt: &[u8]) -> bool {
        let file = |path: &Path, bytes: &[u8]| -> bool {
            let temporary = path.with_file_name(".control.oce-tmp");
            step();
            let Ok(mut file) = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
            else {
                return false;
            };
            step();
            let written = file.write_all(bytes).is_ok();
            drop(file);
            step();
            if written && fs::rename(&temporary, path).is_ok() {
                return true;
            }
            let _ = fs::remove_file(&temporary);
            false
        };
        if fs::create_dir(dir.join(OUTPUTS)).is_err() {
            return false;
        }
        step();
        if fs::create_dir(dir.join(OUTPUTS).join("0")).is_err() {
            return false;
        }
        step();
        file(&dir.join(OUTPUTS).join("0/errors.csv"), csv) && file(&dir.join(RECEIPT), receipt)
    }

    /// What a run is handed: `DIR`, and the adversary it installs once `DIR` is checked.
    type Run = dyn Fn(&Path, Box<dyn FnMut()>) -> bool;

    /// Run `run` under every schedule of swaps over its points; what each left.
    fn every_schedule(layout: &Layout, run: &Run) -> Vec<Left> {
        let out = layout.base.join("out");
        let points = std::rc::Rc::new(std::cell::Cell::new(0_u32));
        let counted = points.clone();
        layout.reset();
        assert!(
            run(&out, Box::new(move || counted.set(counted.get() + 1))),
            "a run with no swap writes"
        );
        with_adversary(None);
        let k = points.get();
        assert!((4..14).contains(&k), "{k} points");
        let mut results = Vec::new();
        for schedule in 0..(1_u32 << k) {
            layout.reset();
            let (name, link) = (layout.name.clone(), layout.link.clone());
            let mut index = 0;
            let written = run(
                &out,
                Box::new(move || {
                    if schedule & (1 << index) != 0 {
                        Layout::swap(&name, &link);
                    }
                    index += 1;
                }),
            );
            with_adversary(None);
            results.push(layout.left(written));
        }
        results
    }

    /// The writer through handles: `DIR` checked (opened), then the adversary, then the write.
    fn handles(out: &Path, adversary: Box<dyn FnMut()>) -> bool {
        let checked = check(out.as_os_str(), &[]).expect("checked");
        with_adversary(Some(adversary));
        write(&checked, &curves(1), b"{}\n").is_ok()
    }

    /// The writer by path, with the adversary from the same moment.
    fn control(out: &Path, adversary: Box<dyn FnMut()>) -> bool {
        with_adversary(Some(adversary));
        path_based(out, b"x,y\n", b"{}\n")
    }

    fn assert_held(results: &[Left], what: &str) {
        for left in results {
            assert!(
                left.victim_untouched,
                "{what}: a write reached the victim: {results:?}"
            );
            assert!(
                !left.stranded,
                "{what}: a temporary file was left: {results:?}"
            );
        }
        assert!(
            results.iter().any(|left| left.written),
            "{what}: {results:?}"
        );
    }

    #[test]
    fn no_schedule_of_outputs_swaps_writes_a_curve_outside_dir() {
        let _serial = crate::commands::atomic_write::tests::COUNTER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(layout) = Layout::make("outputs", false, false) else {
            return;
        };
        let results = every_schedule(&layout, &handles);
        assert_held(&results, "outputs");
        // A link swapped in for `outputs` after it was made is refused by `O_NOFOLLOW`.
        assert!(results.iter().any(|left| !left.written), "{results:?}");
        let control = every_schedule(&layout, &control);
        assert!(
            control.iter().any(|left| !left.victim_untouched),
            "the adversary is not live: the path-based writer held: {control:?}"
        );
    }

    #[test]
    fn no_schedule_of_swaps_of_dir_itself_writes_outside_it() {
        let _serial = crate::commands::atomic_write::tests::COUNTER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // `DIR` exchanged for a link after the check: every file still goes into the directory
        // the check opened, and the run answers as if nothing had happened.
        let Some(layout) = Layout::make("dir", true, false) else {
            return;
        };
        let results = every_schedule(&layout, &handles);
        assert_held(&results, "dir");
        let control = every_schedule(&layout, &control);
        assert!(
            control.iter().any(|left| !left.victim_untouched),
            "the adversary is not live: the path-based writer held: {control:?}"
        );
    }

    #[test]
    fn a_directory_swapped_in_after_it_was_made_is_refused_unless_it_is_empty() {
        let _serial = crate::commands::atomic_write::tests::COUNTER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // The first point is right after `mkdirat` made `outputs`: a non-empty directory moved
        // in under that name is refused before a byte is written; the made one is left beside.
        let Some(layout) = Layout::make("fresh", false, false) else {
            return;
        };
        let out = layout.base.join("out");
        let checked = check(out.as_os_str(), &[]).expect("checked");
        let (outputs, aside) = (out.join(OUTPUTS), out.join("aside"));
        let mut acted = false;
        with_adversary(Some(Box::new(move || {
            if !acted {
                acted = true;
                fs::rename(&outputs, &aside).expect("aside");
                fs::create_dir(&outputs).expect("another");
                fs::write(outputs.join("theirs"), b"x").expect("theirs");
            }
        })));
        let refused = write(&checked, &curves(1), b"{}\n").expect_err("refused");
        with_adversary(None);
        assert_eq!(
            (refused.code, refused.at.as_str()),
            (crate::codes::WRITE_FAILED, OUTPUTS)
        );
        assert!(
            refused.detail.contains("replaced after it was made"),
            "{}",
            refused.detail
        );
        assert_eq!(
            fs::read_dir(out.join(OUTPUTS)).expect("list").count(),
            1,
            "nothing written into the directory swapped in"
        );
        assert!(!out.join(RECEIPT).exists());
        assert!(!stranded(&layout.base));
    }

    #[test]
    fn no_schedule_of_outputs_swaps_writes_into_an_empty_directory_linked_in() {
        // The emptiness check cannot refuse a link to an empty directory: `O_NOFOLLOW` must.
        let _serial = crate::commands::atomic_write::tests::COUNTER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(layout) = Layout::make("outputs_empty", false, true) else {
            return;
        };
        let results = every_schedule(&layout, &handles);
        assert_held(&results, "outputs, empty victim");
        let control = every_schedule(&layout, &control);
        assert!(
            control.iter().any(|left| !left.victim_untouched),
            "the adversary is not live: the path-based writer held: {control:?}"
        );
    }

    #[test]
    fn a_named_pipe_or_a_file_swapped_in_for_a_directory_just_made_is_refused_without_blocking() {
        // `O_DIRECTORY` on the open of a directory just made: a named pipe moved in under its
        // name is refused before it is opened (an open without the flag would wait for a writer
        // forever, which the hang guard turns into a failure), and so is a regular file. The
        // guard is a hang guard, never an assertion on speed.
        let _serial = crate::commands::atomic_write::tests::COUNTER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for pipe in [true, false] {
            let Some(layout) = Layout::make(&format!("swapped_in_{pipe}"), false, false) else {
                return;
            };
            let out = layout.base.join("out");
            let (sent, received) = std::sync::mpsc::channel();
            let worker = {
                let out = out.clone();
                std::thread::spawn(move || {
                    let checked = check(out.as_os_str(), &[]).expect("checked");
                    let (outputs, aside) = (out.join(OUTPUTS), out.join("aside"));
                    let mut acted = false;
                    with_adversary(Some(Box::new(move || {
                        if !acted {
                            acted = true;
                            fs::rename(&outputs, &aside).expect("aside");
                            if pipe {
                                let made = std::process::Command::new("mkfifo")
                                    .arg(&outputs)
                                    .status()
                                    .expect("mkfifo runs");
                                assert!(made.success());
                            } else {
                                fs::write(&outputs, b"x").expect("a file");
                            }
                        }
                    })));
                    let refused = write(&checked, &curves(1), b"{}\n").map(|_| ());
                    with_adversary(None);
                    let _ = sent.send(
                        refused.map_err(|refused| (refused.code, refused.at, refused.detail)),
                    );
                })
            };
            match received.recv_timeout(std::time::Duration::from_secs(60)) {
                Ok(result) => {
                    worker.join().expect("the worker");
                    let (code, at, detail) = result.expect_err("refused");
                    assert_eq!((code, at.as_str()), (crate::codes::WRITE_FAILED, OUTPUTS));
                    assert!(detail.contains("replaced after it was made"), "{detail}");
                    assert!(!out.join(RECEIPT).exists());
                    assert!(!stranded(&layout.base));
                }
                Err(_) => {
                    let _ = rustix::fs::open(
                        out.join(OUTPUTS),
                        rustix::fs::OFlags::WRONLY | rustix::fs::OFlags::NONBLOCK,
                        rustix::fs::Mode::empty(),
                    );
                    panic!("the open of a directory just made blocked on a named pipe");
                }
            }
        }
    }
}
