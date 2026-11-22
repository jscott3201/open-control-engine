//! The adversary of `commission_out.rs`'s `write()` (issue #771): `DIR/outputs`, or `DIR`
//! itself, swapped for a link to a victim directory at every point between the writer's own
//! file-system calls, with the path-based writer that came before as the control; and the
//! refusals a directory swapped in after it was made, or owned by someone else, earns.

/// Every schedule of swaps over the points of [`write()`](super::super::write).
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod schedules {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use rustix::fs::{CWD, RenameFlags, renameat_with};

    use super::super::tests::curves;
    use super::super::{OUTPUTS, RECEIPT, check, write};
    use crate::commands::directory::Directory;
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
                crate::commands::steps::skip_without_exchange(error);
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

    #[test]
    fn an_output_whose_folder_was_made_and_no_file_written_is_not_listed() {
        // The report's rule (module header): `outputs/1` is made, then its first curve fails
        // (a non-empty directory planted where it goes, so the rename fails); the refusal lists
        // output 0's file and not output 1, whose folder holds nothing.
        let _serial = crate::commands::atomic_write::tests::COUNTER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(layout) = Layout::make("listed", false, false) else {
            return;
        };
        let out = layout.base.join("out");
        let checked = check(out.as_os_str(), &[]).expect("checked");
        let second = out.join(OUTPUTS).join("1");
        let mut seen = 0;
        with_adversary(Some(Box::new(move || {
            if second.exists() {
                seen += 1;
                // The first point after `outputs/1` exists is its own open; the second is the
                // start of its first curve's write.
                if seen == 2 {
                    fs::create_dir_all(second.join("errors.csv/inside")).expect("planted");
                }
            }
        })));
        let refused = write(&checked, &curves(2), b"{}\n").expect_err("refused");
        with_adversary(None);
        assert_eq!(
            (refused.code, refused.at.as_str()),
            (crate::codes::WRITE_FAILED, "outputs/1/errors.csv")
        );
        let listed: Vec<(usize, usize)> = refused
            .written
            .iter()
            .map(|output| (output.index, output.files.len()))
            .collect();
        assert_eq!(listed, [(0, 1)], "output 1 made no file and is not listed");
        assert!(!stranded(&layout.base));
    }

    #[test]
    fn an_outputs_directory_someone_else_owns_refuses_every_curve_below_it() {
        // The owner check covers `outputs` as well as `outputs/<k>`: `outputs` is declared owned
        // by another user once it is open (no test can `chown`), and the first curve is refused
        // before a byte is written, nothing left in `outputs/0`.
        let _serial = crate::commands::atomic_write::tests::COUNTER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(layout) = Layout::make("foreign", false, false) else {
            return;
        };
        let out = layout.base.join("out");
        let checked = check(out.as_os_str(), &[]).expect("checked");
        let outputs = out.join(OUTPUTS);
        let mut declared = false;
        with_adversary(Some(Box::new(move || {
            if !declared && outputs.join("0").exists() {
                declared = true;
                let handle = Directory::open(&outputs).expect("outputs");
                crate::commands::directory::foreign::declare(
                    &handle.metadata().expect("its metadata"),
                );
            }
        })));
        let refused = write(&checked, &curves(1), b"{}\n").expect_err("refused");
        with_adversary(None);
        crate::commands::directory::foreign::clear();
        assert_eq!(
            (refused.code, refused.at.as_str()),
            (crate::codes::WRITE_FAILED, "outputs/0/errors.csv")
        );
        assert!(
            refused.detail.contains("does not own"),
            "{}",
            refused.detail
        );
        assert_eq!(
            fs::read_dir(out.join("outputs/0")).expect("list").count(),
            0
        );
        assert!(!out.join(RECEIPT).exists());
    }
}
