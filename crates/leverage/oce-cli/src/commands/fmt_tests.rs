//! The adversary of `fmt --write` (issue #771): `FILE` exchanged, between any two of the reader's
//! and the writer's own file-system calls, for a link to a victim outside its directory or for
//! another regular file with a wider mode. No schedule may read the victim, write through the
//! link, or give the new file a mode other than the one of the file read; and no schedule may
//! replace the file that was not read, except one whose swap lands between the entry's last
//! check and the rename (the documented window, issue #780: that entry is replaced, never
//! written through). A copy of the path-based flow that came before (the target checked, then
//! read by path, then written by path with the mode of whatever was at the name) runs under the
//! same adversary as the control, and some schedule must beat it.

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod schedules {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};

    use rustix::fs::{CWD, RenameFlags, renameat_with};

    use super::super::Rewrite;
    use crate::commands::steps::{step, with_adversary};

    /// The mode of the file `fmt --write` reads, and the wider mode of the file swapped in.
    const READ_MODE: u32 = 0o640;
    const OTHER_MODE: u32 = 0o606;

    /// What one schedule did.
    #[derive(Debug, PartialEq, Eq)]
    struct Did {
        /// The bytes read, when the read was not refused.
        read: Option<Vec<u8>>,
        /// The victim outside the directory still holds its bytes.
        victim_untouched: bool,
        /// The regular files' bytes still in the directory, other than the new ones.
        kept: Vec<Vec<u8>>,
        /// The modes of the regular files holding the new bytes.
        written_modes: Vec<u32>,
        /// Whether the schedule swapped at the last point (between the entry's last check and
        /// the rename).
        at_the_rename: bool,
    }

    /// `base/d/t.oce` (read), `base/d/t.alt` (a link to `../victim.oce`, or another regular
    /// file), exchanging.
    struct Layout {
        base: PathBuf,
        link: bool,
    }

    impl Layout {
        fn make(test: &str, link: bool) -> Option<Self> {
            let base = std::env::temp_dir().join(format!(
                "oce-cli-fmt-schedule-{test}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&base);
            fs::create_dir_all(base.join("d")).expect("d");
            let base = fs::canonicalize(&base).expect("canonical");
            let layout = Self { base, link };
            layout.reset();
            match renameat_with(CWD, layout.t(), CWD, layout.alt(), RenameFlags::EXCHANGE) {
                Ok(()) => {
                    layout.reset();
                    Some(layout)
                }
                Err(error) => {
                    crate::commands::steps::skip_without_exchange(error);
                    None
                }
            }
        }

        fn t(&self) -> PathBuf {
            self.base.join("d/t.oce")
        }

        fn alt(&self) -> PathBuf {
            self.base.join("d/t.alt")
        }

        fn victim(&self) -> PathBuf {
            self.base.join("victim.oce")
        }

        fn reset(&self) {
            let _ = fs::remove_dir_all(self.base.join("d"));
            fs::create_dir(self.base.join("d")).expect("d");
            write_mode(&self.t(), b"READ", READ_MODE);
            write_mode(&self.victim(), b"VICTIM", 0o644);
            if self.link {
                std::os::unix::fs::symlink("../victim.oce", self.alt()).expect("link");
            } else {
                write_mode(&self.alt(), b"OTHER", OTHER_MODE);
            }
        }

        fn did(&self, read: Option<Vec<u8>>, at_the_rename: bool) -> Did {
            let mut written_modes = Vec::new();
            let mut kept = Vec::new();
            for name in [self.t(), self.alt()] {
                let entry = fs::symlink_metadata(&name).expect("entry");
                if !entry.is_file() {
                    continue;
                }
                let bytes = fs::read(&name).expect("read");
                if bytes == b"NEW" {
                    written_modes.push(entry.permissions().mode() & 0o777);
                } else {
                    kept.push(bytes);
                }
            }
            kept.sort();
            Did {
                read,
                victim_untouched: fs::read(self.victim()).expect("victim") == b"VICTIM",
                kept,
                written_modes,
                at_the_rename,
            }
        }
    }

    impl Drop for Layout {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    fn write_mode(path: &Path, bytes: &[u8], mode: u32) {
        fs::write(path, bytes).expect("write");
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("chmod");
    }

    /// Read and rewrite `t.oce` through handles; the bytes read, if any.
    fn handles(layout: &Layout) -> Option<Vec<u8>> {
        let (rewrite, bytes) = Rewrite::read(&layout.t(), "t.oce", 64).ok()?;
        let _ = rewrite.write(b"NEW");
        Some(bytes)
    }

    /// The flow before issue #771, with the adversary at the same points: the target checked,
    /// read by path, written by path with the mode of whatever is at the name.
    fn by_path(layout: &Layout) -> Option<Vec<u8>> {
        let t = layout.t();
        step();
        if !fs::symlink_metadata(&t).ok()?.is_file() {
            return None;
        }
        step();
        let bytes = fs::read(&t).ok()?;
        step();
        let mode = fs::symlink_metadata(&t)
            .ok()
            .filter(fs::Metadata::is_file)
            .map_or(0o600, |entry| entry.permissions().mode() & 0o777);
        let temporary = t.with_file_name(".t.control.oce-tmp");
        step();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&temporary)
            .ok()?;
        let _ = file.write_all(b"NEW");
        let _ = file.set_permissions(fs::Permissions::from_mode(mode));
        drop(file);
        step();
        if fs::rename(&temporary, &t).is_err() {
            let _ = fs::remove_file(&temporary);
        }
        Some(bytes)
    }

    fn every_schedule(layout: &Layout, run: &dyn Fn(&Layout) -> Option<Vec<u8>>) -> Vec<Did> {
        let points = std::rc::Rc::new(std::cell::Cell::new(0_u32));
        let counted = points.clone();
        layout.reset();
        with_adversary(Some(Box::new(move || counted.set(counted.get() + 1))));
        assert_eq!(
            run(layout).as_deref(),
            Some(&b"READ"[..]),
            "a run with no swap"
        );
        let k = points.get();
        assert!((4..14).contains(&k), "{k} points");
        let mut results = Vec::new();
        for schedule in 0..(1_u32 << k) {
            layout.reset();
            let (t, alt) = (layout.t(), layout.alt());
            let mut index = 0;
            with_adversary(Some(Box::new(move || {
                if schedule & (1 << index) != 0 {
                    renameat_with(CWD, &t, CWD, &alt, RenameFlags::EXCHANGE).expect("exchange");
                }
                index += 1;
            })));
            let read = run(layout);
            with_adversary(None);
            results.push(layout.did(read, schedule & (1 << (k - 1)) != 0));
        }
        results
    }

    /// The rule (module header) for one schedule of a layout whose other file is `other`
    /// (`None` for the link).
    fn holds(did: &Did, other: Option<&[u8]>) -> bool {
        let mode_of = |read: &[u8]| {
            if read == b"READ" {
                READ_MODE
            } else {
                OTHER_MODE
            }
        };
        let regular = |bytes: &[u8]| bytes == b"READ" || Some(bytes) == other;
        match did.read.as_deref() {
            // Refused before a byte was read: nothing written, everything kept.
            None => {
                did.written_modes.is_empty()
                    && did.victim_untouched
                    && did.kept.len() == 1 + usize::from(other.is_some())
            }
            Some(read) => {
                let unread: Vec<&[u8]> = [&b"READ"[..], other.unwrap_or_default()]
                    .into_iter()
                    .filter(|bytes| !bytes.is_empty() && *bytes != read)
                    .collect();
                regular(read)
                    && did.victim_untouched
                    && did.written_modes.iter().all(|mode| *mode == mode_of(read))
                    && (did.at_the_rename
                        || unread
                            .iter()
                            .all(|bytes| did.kept.iter().any(|kept| kept == bytes)))
            }
        }
    }

    #[test]
    fn no_schedule_of_link_swaps_reads_or_writes_through_the_link() {
        let _serial = crate::commands::atomic_write::tests::COUNTER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(layout) = Layout::make("link", true) else {
            return;
        };
        let results = every_schedule(&layout, &handles);
        assert!(results.iter().all(|did| holds(did, None)), "{results:?}");
        assert!(
            results.iter().any(|did| did.read.is_none()),
            "no swap landed"
        );
        let control = every_schedule(&layout, &by_path);
        assert!(
            control
                .iter()
                .any(|did| did.read.as_deref() == Some(&b"VICTIM"[..])),
            "the adversary is not live: the path-based flow never read the victim: {control:?}"
        );
    }

    #[test]
    fn no_schedule_of_file_swaps_replaces_another_file_or_lends_its_mode() {
        let _serial = crate::commands::atomic_write::tests::COUNTER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(layout) = Layout::make("file", false) else {
            return;
        };
        let results = every_schedule(&layout, &handles);
        assert!(
            results.iter().all(|did| holds(did, Some(b"OTHER"))),
            "{results:?}"
        );
        // The changed entry is refused (`write-failed`) rather than replaced.
        assert!(
            results
                .iter()
                .any(|did| did.read.is_some() && did.written_modes.is_empty()),
            "{results:?}"
        );
        let control = every_schedule(&layout, &by_path);
        assert!(
            !control.iter().all(|did| holds(did, Some(b"OTHER"))),
            "the adversary is not live: the path-based flow held: {control:?}"
        );
    }
}
