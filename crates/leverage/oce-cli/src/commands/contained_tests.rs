//! The adversary of `contained.rs` (issues #758 and #771): an attacker who controls the document's
//! directory and acts between any two of the reader's file-system calls (`steps::step`). Each test
//! enumerates every schedule of swaps over those points (2^k for k points) and requires that no
//! schedule makes a read leave the directory, or pair the document with another folder's file. A
//! copy of the path-based reader that came before runs under the same adversary as the control: a
//! schedule must beat it, or the adversary is not live.
//!
//! A swap exchanges two entries atomically (`renameat_with` with `RenameFlags::EXCHANGE`:
//! `renamex_np` with `RENAME_SWAP` on macOS, `renameat2` with `RENAME_EXCHANGE` on Linux), so the
//! name always exists, as the real thing or as a link out. Other Unix systems have no such call
//! here and run none of these tests; on Linux or macOS a file system under the temporary
//! directory that refuses the exchange (`EINVAL`, `ENOTSUP`, `ENOSYS`) skips each test that
//! swaps, with a note, rather than failing: the rule is then untested there. CI's Linux runners
//! and APFS support it.

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod schedules {
    use std::fs::{self, File};
    use std::io::Read;
    use std::path::{Path, PathBuf};

    use rustix::fs::{CWD, RenameFlags, renameat_with};

    use super::super::{Refusal, Root, changed, not_a_file};
    use crate::codes;
    use crate::commands::file_identity::identity;
    use crate::commands::steps::{step, with_adversary};

    /// What one schedule's read gave.
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Gave {
        /// The file inside the directory.
        Inside,
        /// The file outside: the containment rule failed.
        Outside,
        /// Another file inside the pinned directory than the one listed.
        InsideOther,
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
        /// `root/<listed>` reads "INSIDE" and `root/doc.json` "DOC-INSIDE"; `outside/` holds the
        /// same names reading "OUTSIDE" and "DOC-OUTSIDE". The entry `root/<swapped>` exchanges
        /// with `root/<swapped>.alt`, a link to its counterpart under `outside/`; an empty
        /// `swapped` is the directory itself, `root` exchanging with `root.alt`, a link to
        /// `outside`.
        fn make(test: &str, listed: &'static str, swapped: &str) -> Self {
            let base = std::env::temp_dir()
                .join(format!("oce-cli-contained-{test}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&base);
            for side in ["root", "outside"] {
                let file = base.join(side).join(listed);
                fs::create_dir_all(file.parent().expect("a parent")).expect("directories");
                let (text, doc) = if side == "root" {
                    ("INSIDE", "DOC-INSIDE")
                } else {
                    ("OUTSIDE", "DOC-OUTSIDE")
                };
                fs::write(&file, text).expect("the file");
                fs::write(base.join(side).join("doc.json"), doc).expect("the document");
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
                    crate::commands::steps::skip_without_exchange(error);
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

        fn document(&self) -> PathBuf {
            self.base.join("root").join("doc.json")
        }

        fn root(&self) -> Root {
            Root::read_document(&self.document(), 64)
                .map_err(|(code, _)| code)
                .expect("the document")
                .root
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
                match text.as_str() {
                    "INSIDE" => Gave::Inside,
                    "INSIDE-OTHER" => Gave::InsideOther,
                    _ => Gave::Outside,
                }
            }
            Err((code, _)) => Gave::Refused(code),
        }
    }

    /// Every schedule of exchanging `name` and `link` over the points `act` passes, and what
    /// each gave. `act` runs once with no swap first, and must give `clean`.
    fn schedules<T: PartialEq + std::fmt::Debug>(
        name: &Path,
        link: &Path,
        reset: &dyn Fn(),
        act: &dyn Fn() -> T,
        clean: &T,
    ) -> Vec<T> {
        let points = std::rc::Rc::new(std::cell::Cell::new(0_u32));
        let counted = points.clone();
        reset();
        with_adversary(Some(Box::new(move || counted.set(counted.get() + 1))));
        assert_eq!(&act(), clean, "a run with no swap");
        let k = points.get();
        assert!((3..16).contains(&k), "{k} points");
        let mut results = Vec::new();
        for schedule in 0..(1_u32 << k) {
            reset();
            let (name, link) = (name.to_path_buf(), link.to_path_buf());
            let mut index = 0;
            with_adversary(Some(Box::new(move || {
                if schedule & (1 << index) != 0 {
                    Layout::swap(&name, &link);
                }
                index += 1;
            })));
            results.push(act());
        }
        with_adversary(None);
        reset();
        results
    }

    /// Every schedule of swaps over the points `open` passes, and what each gave; the root is
    /// opened before the adversary starts.
    fn every_schedule(
        layout: &Layout,
        open: &dyn Fn(&Root, &str) -> Result<File, Refusal>,
    ) -> Vec<Gave> {
        let root = layout.root();
        schedules(
            &layout.name,
            &layout.link,
            &|| layout.reset(),
            &|| classify(open(&root, layout.listed)),
            &Gave::Inside,
        )
    }

    fn contained(root: &Root, path: &str) -> Result<File, Refusal> {
        root.open(path, codes::FILE_OUTSIDE_DIRECTORY, "the spec's")
            .map(|opened| opened.file)
    }

    /// The control: the path-based checks GitHub #757 merged, with the adversary at the same
    /// points (canonicalize, the entry's check, the open), from the directory's path.
    fn path_based(root: &Root, path: &str) -> Result<File, Refusal> {
        path_based_in(&root.directory, &root.canonical, path)
    }

    fn path_based_in(directory: &Path, canonical_root: &Path, path: &str) -> Result<File, Refusal> {
        let joined = path
            .split('/')
            .fold(directory.to_path_buf(), |joined, segment| {
                joined.join(segment)
            });
        step();
        let canonical = fs::canonicalize(&joined).map_err(crate::input::io_refusal)?;
        if !canonical.starts_with(canonical_root) {
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
        // The directory is the handle the document was read through: a swap of its path after
        // that is refused by the canonicalization or walked from the handle, never followed.
        let layout = Layout::make("itself", "sub/file.csv", "");
        if !layout.swappable() {
            return;
        }
        let results = every_schedule(&layout, &contained);
        assert!(!results.contains(&Gave::Outside), "{results:?}");
        assert!(results.contains(&Gave::Inside), "{results:?}");
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

    #[test]
    fn a_folder_exchanged_between_the_directory_open_and_its_canonicalization_is_read_failed() {
        // The one schedule the directory's identity check exists for: the handle is opened on
        // `root`, then `root` is exchanged for the link out before the canonicalization. The
        // canonical path is then `outside`'s, which is not the handle's directory: refused, so
        // the outside refusal never judges against a directory the reads do not come from.
        let layout = Layout::make("identity", "sub/file.csv", "");
        if !layout.swappable() {
            return;
        }
        let (name, link) = (layout.name.clone(), layout.link.clone());
        let mut index = 0;
        with_adversary(Some(Box::new(move || {
            if index == 1 {
                Layout::swap(&name, &link);
            }
            index += 1;
        })));
        let result = Root::read_document(&layout.document(), 64).map(|_| ());
        with_adversary(None);
        layout.reset();
        assert_eq!(result.map_err(|(code, _)| code), Err(codes::READ_FAILED));
    }

    /// What one schedule of the document read and one named read gave: the two texts, or the
    /// first refusal.
    type Pair = Result<(String, String), &'static str>;

    /// The reader through handles: the document through its directory's handle, then the file.
    fn document_then_file(layout: &Layout) -> Pair {
        let document = Root::read_document(&layout.document(), 64).map_err(|(code, _)| code)?;
        let text = String::from_utf8(document.bytes).expect("UTF-8");
        let mut file = contained(&document.root, layout.listed).map_err(|(code, _)| code)?;
        let mut read = String::new();
        file.read_to_string(&mut read).expect("read");
        Ok((text, read))
    }

    /// The control: the reader before issue #771, the document read by path, its directory
    /// canonicalized after, the file by the path-based checks; the adversary between each.
    fn by_path(layout: &Layout) -> Pair {
        let directory = layout.base.join("root");
        step();
        let text = fs::read_to_string(layout.document()).map_err(|_| codes::READ_FAILED)?;
        step();
        let canonical = fs::canonicalize(&directory).map_err(|_| codes::READ_FAILED)?;
        let mut file =
            path_based_in(&directory, &canonical, layout.listed).map_err(|(code, _)| code)?;
        let mut read = String::new();
        file.read_to_string(&mut read).expect("read");
        Ok((text, read))
    }

    fn mixed(pair: &Pair) -> bool {
        matches!(pair, Ok((document, file))
            if (document == "DOC-INSIDE") != (file == "INSIDE"))
    }

    #[test]
    fn no_schedule_of_folder_swaps_pairs_a_document_with_another_folders_file() {
        // Issue #771 (ii): the document's folder exchanged with another between any two calls,
        // the document's read included. The document and every file it names come through one
        // handle, so the two always come from one folder.
        let layout = Layout::make("pairing", "sub/file.csv", "");
        if !layout.swappable() {
            return;
        }
        let inside = Ok(("DOC-INSIDE".to_owned(), "INSIDE".to_owned()));
        let results = schedules(
            &layout.name,
            &layout.link,
            &|| layout.reset(),
            &|| document_then_file(&layout),
            &inside,
        );
        assert!(
            !results.iter().any(mixed),
            "a schedule paired the document with another folder's file: {results:?}"
        );
        assert!(results.contains(&inside), "{results:?}");
        assert!(
            results.iter().any(Result::is_err),
            "no swap landed: {results:?}"
        );
        let control = schedules(
            &layout.name,
            &layout.link,
            &|| layout.reset(),
            &|| by_path(&layout),
            &inside,
        );
        assert!(
            control.iter().any(mixed),
            "the adversary is not live: the path-based reader paired nothing: {control:?}"
        );
    }

    #[test]
    fn a_folder_swapped_after_the_pin_can_change_the_file_inside_never_reach_outside() {
        // The stated limit (module header of `contained.rs`): the canonicalization reads the
        // directory by path. `root.alt` is another folder whose `sub` is a link to `other`: in
        // place, its links steer the canonicalization to `other/file.csv`, which is then read
        // from the pinned directory ("INSIDE-OTHER"), never from the folder swapped in
        // ("OUTSIDE"). The path-based reader reads the swapped folder's file.
        let layout = Layout::make("limit", "sub/file.csv", "");
        let base = layout.base.clone();
        fs::create_dir_all(base.join("root/other")).expect("other");
        fs::write(base.join("root/other/file.csv"), "INSIDE-OTHER").expect("other file");
        fs::remove_file(base.join("root.alt")).expect("drop the link");
        fs::create_dir_all(base.join("root.alt/other")).expect("alt");
        fs::write(base.join("root.alt/other/file.csv"), "OUTSIDE").expect("alt file");
        fs::write(base.join("root.alt/doc.json"), "DOC-OUTSIDE").expect("alt doc");
        std::os::unix::fs::symlink("other", base.join("root.alt/sub")).expect("sub -> other");
        if !layout.swappable() {
            return;
        }
        let root = layout.root();
        let reset = || {
            if fs::symlink_metadata(base.join("root/sub")).is_ok_and(|m| m.is_symlink()) {
                Layout::swap(&layout.name, &layout.link);
            }
        };
        let results = schedules(
            &layout.name,
            &layout.link,
            &reset,
            &|| classify(contained(&root, layout.listed)),
            &Gave::Inside,
        );
        assert!(!results.contains(&Gave::Outside), "{results:?}");
        assert!(
            results.contains(&Gave::InsideOther),
            "the stated limit did not show: {results:?}"
        );
        let control = schedules(
            &layout.name,
            &layout.link,
            &reset,
            &|| classify(path_based(&root, layout.listed)),
            &Gave::Inside,
        );
        assert!(control.contains(&Gave::Outside), "{control:?}");
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
        let root = layout.root();
        let counted = std::rc::Rc::new(std::cell::Cell::new(0_u32));
        let counter = counted.clone();
        with_adversary(Some(Box::new(move || counter.set(counter.get() + 1))));
        assert_eq!(classify(contained(&root, layout.listed)), Gave::Inside);
        with_adversary(None);
        let at = point.unwrap_or(counted.get() - 1);
        let (name, link, document) = (layout.name.clone(), layout.link.clone(), layout.document());
        let (sent, received) = std::sync::mpsc::channel();
        let opener = std::thread::spawn(move || {
            let root = Root::read_document(&document, 64)
                .map_err(|(code, _)| code)
                .expect("the document")
                .root;
            let mut index = 0;
            with_adversary(Some(Box::new(move || {
                if index == at {
                    Layout::swap(&name, &link);
                }
                index += 1;
            })));
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
        // the canonicalization, then `sub`'s open: the swap lands before the second.
        if let Some(gave) = pipe_swapped_in("pipe_directory", "sub", Some(1)) {
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

    #[test]
    fn a_document_that_is_a_link_is_read_and_its_files_resolve_beside_the_link() {
        // The orchestrator's call on issue #771: the caller named the document, so a link at
        // its own entry is followed, from the directory's handle; the files it names resolve
        // against the directory of the link, not of its target.
        let layout = Layout::make("document_link", "sub/file.csv", "sub");
        let link = layout.base.join("root/link.json");
        std::os::unix::fs::symlink("../outside/doc.json", &link).expect("doc link");
        let document = Root::read_document(&link, 64)
            .map_err(|(code, _)| code)
            .expect("read");
        assert_eq!(document.bytes, b"DOC-OUTSIDE");
        assert_eq!(
            classify(contained(&document.root, "sub/file.csv")),
            Gave::Inside
        );
        // A pipe there is still never opened, and a directory is no document.
        let made = std::process::Command::new("mkfifo")
            .arg(layout.base.join("root/pipe.json"))
            .status()
            .expect("mkfifo runs");
        assert!(made.success());
        for name in ["pipe.json", "sub"] {
            let refused = Root::read_document(&layout.base.join("root").join(name), 64)
                .map(|_| ())
                .expect_err(name);
            assert_eq!(refused.0, codes::NOT_A_FILE, "{name}");
        }
        // A document past its bound is refused from the handle's size, before a byte is read.
        let refused = Root::read_document(&layout.document(), 4)
            .map(|_| ())
            .expect_err("too large");
        assert_eq!(refused.0, codes::INPUT_TOO_LARGE);
    }
}
