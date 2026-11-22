//! The adversary of `contained.rs` (issue #758): an attacker who controls the document's directory
//! and acts between any two of [`Root::open`]'s file-system calls. [`step`] is where it acts; each
//! test enumerates every schedule of swaps over those points (2^k for k points) and requires that
//! no schedule makes the open read outside the directory. A copy of the path-based checks GitHub
//! #757 merged runs under the same adversary as the control: a schedule must beat it, or the
//! adversary is not live.
//!
//! A swap exchanges two entries atomically (`renameat_with` with `RenameFlags::EXCHANGE`:
//! `renamex_np` with `RENAME_SWAP` on macOS, `renameat2` with `RENAME_EXCHANGE` on Linux), so the
//! name always exists, as the real thing or as a link out. Other Unix systems have no such call
//! here and run none of these tests; on Linux or macOS a file system under the temporary
//! directory that refuses the exchange (`EINVAL`, `ENOTSUP`, `ENOSYS`) skips each test that
//! swaps, with a note, rather than failing: the rule is then untested there. CI's Linux runners
//! and APFS support it.

use std::cell::RefCell;

thread_local! {
    /// The adversary of the running test: called at every [`step`].
    static ADVERSARY: RefCell<Option<Box<dyn FnMut()>>> = RefCell::new(None);
}

/// A point between two file-system calls of `Root::open`: the adversary acts here.
pub(super) fn step() {
    ADVERSARY.with(|adversary| {
        if let Some(act) = adversary.borrow_mut().as_mut() {
            act();
        }
    });
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod schedules {
    use std::fs::{self, File};
    use std::io::Read;
    use std::path::{Path, PathBuf};

    use rustix::fs::{CWD, RenameFlags, renameat_with};

    use super::super::{Refusal, Root, changed, not_a_file};
    use super::{ADVERSARY, step};
    use crate::codes;
    use crate::commands::file_identity::identity;

    /// What one schedule's open gave.
    #[derive(Debug, PartialEq, Eq)]
    enum Gave {
        /// The file inside the directory.
        Inside,
        /// The file outside: the containment rule failed.
        Outside,
        /// A refusal, by code.
        Refused(&'static str),
    }

    /// A layout whose `swapped` entry (a directory or the file) exchanges with a link out.
    struct Layout {
        base: PathBuf,
        listed: &'static str,
        name: PathBuf,
        link: PathBuf,
    }

    impl Layout {
        /// `root/<listed>` reads "INSIDE"; `outside/` holds the same names reading "OUTSIDE". The
        /// entry `root/<swapped>` exchanges with `root/<swapped>.alt`, a link to its counterpart
        /// under `outside/`; an empty `swapped` is the directory itself, `root` exchanging with
        /// `root.alt`, a link to `outside`.
        fn make(test: &str, listed: &'static str, swapped: &str) -> Self {
            let base = std::env::temp_dir()
                .join(format!("oce-cli-contained-{test}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&base);
            for side in ["root", "outside"] {
                let file = base.join(side).join(listed);
                fs::create_dir_all(file.parent().expect("a parent")).expect("directories");
                let text = if side == "root" { "INSIDE" } else { "OUTSIDE" };
                fs::write(&file, text).expect("the file");
            }
            let base = fs::canonicalize(&base).expect("canonical base");
            let (name, link, target) = if swapped.is_empty() {
                (
                    base.join("root"),
                    base.join("root.alt"),
                    base.join("outside"),
                )
            } else {
                (
                    base.join("root").join(swapped),
                    base.join("root").join(format!("{swapped}.alt")),
                    base.join("outside").join(swapped),
                )
            };
            std::os::unix::fs::symlink(target, &link).expect("link");
            Self {
                base,
                listed,
                name,
                link,
            }
        }

        fn swap(name: &Path, link: &Path) {
            renameat_with(CWD, name, CWD, link, RenameFlags::EXCHANGE).expect("exchange");
        }

        /// Whether this file system exchanges two entries atomically; when it does not, the
        /// test that asks is skipped with a note (module header).
        fn swappable(&self) -> bool {
            use rustix::io::Errno;
            match renameat_with(CWD, &self.name, CWD, &self.link, RenameFlags::EXCHANGE) {
                Ok(()) => {
                    Self::swap(&self.name, &self.link);
                    true
                }
                Err(error)
                    if [Errno::INVAL, Errno::NOTSUP, Errno::OPNOTSUPP, Errno::NOSYS]
                        .contains(&error) =>
                {
                    #[allow(
                        clippy::print_stderr,
                        reason = "an explicit skip note in the test output"
                    )]
                    {
                        eprintln!("skipped: no atomic exchange on this file system ({error:?})");
                    }
                    false
                }
                Err(error) => panic!("the exchange failed: {error:?}"),
            }
        }

        /// Put the real entry back under its name.
        fn reset(&self) {
            use std::os::unix::fs::FileTypeExt;
            let at_name = fs::symlink_metadata(&self.name)
                .expect("the name")
                .file_type();
            if at_name.is_symlink() || at_name.is_fifo() {
                Self::swap(&self.name, &self.link);
            }
        }

        fn root(&self) -> Root {
            Root::of(&self.base.join("root").join("doc.json")).expect("the root")
        }
    }

    impl Drop for Layout {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    fn classify(result: Result<File, Refusal>) -> Gave {
        match result {
            Ok(mut file) => {
                let mut text = String::new();
                file.read_to_string(&mut text).expect("read");
                if text == "INSIDE" {
                    Gave::Inside
                } else {
                    Gave::Outside
                }
            }
            Err((code, _)) => Gave::Refused(code),
        }
    }

    /// Every schedule of swaps over the points `open` passes, and what each gave.
    fn every_schedule(
        layout: &Layout,
        open: &dyn Fn(&Root, &str) -> Result<File, Refusal>,
    ) -> Vec<Gave> {
        let root = layout.root();
        let points = std::rc::Rc::new(std::cell::Cell::new(0_u32));
        let counted = points.clone();
        ADVERSARY
            .with(|a| *a.borrow_mut() = Some(Box::new(move || counted.set(counted.get() + 1))));
        let clean = classify(open(&root, layout.listed));
        assert_eq!(clean, Gave::Inside, "a run with no swap reads the file");
        let k = points.get();
        assert!((3..16).contains(&k), "{k} points");
        let mut results = Vec::new();
        for schedule in 0..(1_u32 << k) {
            layout.reset();
            let (name, link) = (layout.name.clone(), layout.link.clone());
            let mut index = 0;
            ADVERSARY.with(|a| {
                *a.borrow_mut() = Some(Box::new(move || {
                    if schedule & (1 << index) != 0 {
                        Layout::swap(&name, &link);
                    }
                    index += 1;
                }));
            });
            results.push(classify(open(&root, layout.listed)));
        }
        ADVERSARY.with(|a| *a.borrow_mut() = None);
        layout.reset();
        results
    }

    fn contained(root: &Root, path: &str) -> Result<File, Refusal> {
        root.open(path, codes::FILE_OUTSIDE_DIRECTORY, "the spec's")
            .map(|opened| opened.file)
    }

    /// The control: the path-based checks GitHub #757 merged, with the adversary at the same
    /// points (canonicalize, the entry's check, the open).
    fn path_based(root: &Root, path: &str) -> Result<File, Refusal> {
        let joined = path
            .split('/')
            .fold(root.directory.clone(), |joined, segment| {
                joined.join(segment)
            });
        step();
        let canonical = fs::canonicalize(&joined).map_err(crate::input::io_refusal)?;
        if !canonical.starts_with(&root.canonical) {
            return Err((codes::FILE_OUTSIDE_DIRECTORY, String::new()));
        }
        step();
        let checked = fs::symlink_metadata(&canonical).map_err(crate::input::io_refusal)?;
        if checked.file_type().is_symlink() {
            return Err(changed());
        }
        if !checked.is_file() {
            return Err(not_a_file());
        }
        step();
        let file = File::open(&canonical).map_err(crate::input::io_refusal)?;
        let metadata = file.metadata().map_err(crate::input::io_refusal)?;
        if identity(&canonical, &metadata) != identity(&canonical, &checked) {
            return Err(changed());
        }
        Ok(file)
    }

    fn assert_never_outside(results: &[Gave], what: &str) {
        assert!(
            !results.contains(&Gave::Outside),
            "{what}: a schedule read outside: {results:?}"
        );
        assert!(results.contains(&Gave::Inside), "{what}: {results:?}");
        // Every refusal a swap earns is one of the rule's own codes.
        for result in results {
            if let Gave::Refused(code) = result {
                assert!(
                    [
                        codes::READ_FAILED,
                        codes::FILE_OUTSIDE_DIRECTORY,
                        codes::NOT_FOUND,
                        codes::NOT_A_FILE
                    ]
                    .contains(code),
                    "{what}: {code}"
                );
            }
        }
        assert!(
            results.iter().any(|r| matches!(r, Gave::Refused(_))),
            "{what}: no schedule was refused, so no swap landed"
        );
    }

    #[test]
    fn no_schedule_of_directory_swaps_reads_outside_and_the_merged_checks_are_beaten() {
        let layout = Layout::make("directory", "sub/file.csv", "sub");
        if !layout.swappable() {
            return;
        }
        assert_never_outside(&every_schedule(&layout, &contained), "openat");
        let control = every_schedule(&layout, &path_based);
        assert!(
            control.contains(&Gave::Outside),
            "the adversary is not live: the path-based checks held: {control:?}"
        );
    }

    #[test]
    fn no_schedule_of_swaps_two_directories_down_reads_outside() {
        let layout = Layout::make("deep", "a/b/file.csv", "a/b");
        if !layout.swappable() {
            return;
        }
        assert_never_outside(&every_schedule(&layout, &contained), "openat");
        assert!(every_schedule(&layout, &path_based).contains(&Gave::Outside));
    }

    #[test]
    fn no_schedule_of_swaps_of_the_directory_itself_reads_outside() {
        // The directory opened by its canonical path must still be the one resolved: a swap
        // between the canonicalization and that open is refused by its identity.
        let layout = Layout::make("itself", "sub/file.csv", "");
        if !layout.swappable() {
            return;
        }
        assert_never_outside(&every_schedule(&layout, &contained), "openat");
        assert!(every_schedule(&layout, &path_based).contains(&Gave::Outside));
    }

    #[test]
    fn no_schedule_of_last_component_swaps_reads_outside() {
        let layout = Layout::make("last", "sub/file.csv", "sub/file.csv");
        if !layout.swappable() {
            return;
        }
        let results = every_schedule(&layout, &contained);
        assert_never_outside(&results, "openat");
        // A link found at the last entry was swapped in after the canonicalization: the path
        // changed (`read-failed`), it is no `not-a-file`.
        assert!(
            !results.contains(&Gave::Refused(codes::NOT_A_FILE)),
            "{results:?}"
        );
    }

    /// What [`contained`] gives when the `swapped` entry is exchanged for a named pipe at the
    /// `point`-th of its steps (counted from 0; `None` for the last), on a thread of its own.
    /// The wait is a hang guard, never an assertion on speed: on a timeout the test opens the
    /// pipe for writing to release the reader, then fails. `None` when the file system cannot
    /// exchange (module header).
    fn pipe_swapped_in(test: &str, swapped: &str, point: Option<u32>) -> Option<Gave> {
        let layout = Layout::make(test, "sub/file.csv", swapped);
        fs::remove_file(&layout.link).expect("drop the link");
        let made = std::process::Command::new("mkfifo")
            .arg(&layout.link)
            .status()
            .expect("mkfifo runs");
        assert!(made.success());
        if !layout.swappable() {
            return None;
        }
        let counted = std::rc::Rc::new(std::cell::Cell::new(0_u32));
        let counter = counted.clone();
        ADVERSARY
            .with(|a| *a.borrow_mut() = Some(Box::new(move || counter.set(counter.get() + 1))));
        assert_eq!(
            classify(contained(&layout.root(), layout.listed)),
            Gave::Inside
        );
        ADVERSARY.with(|a| *a.borrow_mut() = None);
        let at = point.unwrap_or(counted.get() - 1);
        let (name, link, base) = (
            layout.name.clone(),
            layout.link.clone(),
            layout.base.clone(),
        );
        let (sent, received) = std::sync::mpsc::channel();
        let opener = std::thread::spawn(move || {
            let mut index = 0;
            ADVERSARY.with(|a| {
                *a.borrow_mut() = Some(Box::new(move || {
                    if index == at {
                        Layout::swap(&name, &link);
                    }
                    index += 1;
                }));
            });
            let root = Root::of(&base.join("root").join("doc.json")).expect("the root");
            let _ = sent.send(classify(contained(&root, "sub/file.csv")));
        });
        match received.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(gave) => {
                opener.join().expect("the opener");
                layout.reset();
                Some(gave)
            }
            Err(_) => {
                let _ = rustix::fs::open(
                    &layout.name,
                    rustix::fs::OFlags::WRONLY | rustix::fs::OFlags::NONBLOCK,
                    rustix::fs::Mode::empty(),
                );
                panic!("an open blocked on a named pipe swapped in for `{swapped}`");
            }
        }
    }

    #[test]
    fn a_named_pipe_swapped_in_before_the_last_open_is_refused_without_blocking() {
        // `O_NONBLOCK` on the last open: a pipe that replaces the file after its entry was
        // checked is opened at once and refused by its kind, where a blocking open would wait
        // for a writer forever.
        if let Some(gave) = pipe_swapped_in("pipe_last", "sub/file.csv", None) {
            assert_eq!(gave, Gave::Refused(codes::NOT_A_FILE));
        }
    }

    #[test]
    fn a_named_pipe_swapped_in_for_a_directory_is_refused_without_blocking() {
        // `O_DIRECTORY` on every directory below the root: a pipe swapped in for `sub` just
        // before it is opened is refused as no directory (the path changed) before it is
        // opened, where an open without the flag would wait for a writer forever. The steps are
        // the canonicalization, the root's open, then `sub`'s: the swap lands before the third.
        if let Some(gave) = pipe_swapped_in("pipe_directory", "sub", Some(2)) {
            assert_eq!(gave, Gave::Refused(codes::READ_FAILED));
        }
    }

    #[test]
    fn the_handle_returned_reads_as_an_ordinary_blocking_file() {
        // `O_NONBLOCK` guards the open only; the caller's handle is cleared of it.
        let layout = Layout::make("blocking", "sub/file.csv", "sub");
        let opened = layout
            .root()
            .open("sub/file.csv", codes::FILE_OUTSIDE_DIRECTORY, "the spec's")
            .map_err(|(code, _)| code)
            .expect("the file opens");
        let flags = rustix::fs::fcntl_getfl(&opened.file).expect("the handle's flags");
        assert!(!flags.contains(rustix::fs::OFlags::NONBLOCK), "{flags:?}");
    }

    #[test]
    fn a_link_that_stays_inside_is_still_followed_by_the_canonicalization() {
        let layout = Layout::make("inside_link", "sub/file.csv", "sub");
        std::os::unix::fs::symlink("sub", layout.base.join("root/alias")).expect("link");
        let root = layout.root();
        assert_eq!(classify(contained(&root, "alias/file.csv")), Gave::Inside);
        // A link to the directory itself names no file.
        std::os::unix::fs::symlink(".", layout.base.join("root/here")).expect("link");
        assert_eq!(
            classify(contained(&root, "here")),
            Gave::Refused(codes::NOT_A_FILE)
        );
    }
}
