//! The writer's tests (`atomic_write.rs`): the target rule, the temporary names, the modes, and
//! the adversary of issue #771, which swaps the directory a write goes into for a link to a
//! victim directory at every point between the writer's own file-system calls.

#[cfg(unix)]
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::*;

#[cfg(unix)]
#[test]
fn a_link_that_reaches_the_write_is_replaced_and_its_target_never_written() {
    let _serial = COUNTER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // The backstop for a link planted after `target_accepted`: the rename replaces the
    // entry, so neither a link to a file nor a dangling link carries the write anywhere.
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("no_follow");
    let victim = dir.join("victim.txt");
    fs::write(&victim, b"PRECIOUS").expect("victim");
    fs::set_permissions(&victim, fs::Permissions::from_mode(0o644)).expect("chmod");
    let target = dir.join("receipt.json");
    std::os::unix::fs::symlink(&victim, &target).expect("a link");
    assert_eq!(write_atomically(&target, b"new"), Ok(()));
    assert_eq!(fs::read(&victim).expect("victim"), b"PRECIOUS");
    let entry = fs::symlink_metadata(&target).expect("the entry");
    assert!(
        entry.file_type().is_file(),
        "the link was replaced by a file"
    );
    assert_eq!(fs::read(&target).expect("target"), b"new");
    // The linked file lent nothing: the new file is a new file, owner-only.
    assert_eq!(entry.permissions().mode() & 0o777, NEW_FILE_MODE);
    // A dangling link is replaced too, and nothing is created where it pointed.
    let dangling = dir.join("dangling.json");
    std::os::unix::fs::symlink(dir.join("nowhere.txt"), &dangling).expect("a link");
    assert_eq!(write_atomically(&dangling, b"x"), Ok(()));
    assert!(!dir.join("nowhere.txt").exists());
    assert!(fs::symlink_metadata(&dangling).expect("entry").is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_absent_path_and_a_regular_file_are_accepted_and_a_directory_is_not() {
    let dir = scratch("target_kinds");
    assert_eq!(
        target_accepted(&dir.join("absent"), "absent", Named::Caller),
        Ok(())
    );
    fs::write(dir.join("file"), b"x").expect("file");
    assert_eq!(
        target_accepted(&dir.join("file"), "file", Named::Caller),
        Ok(())
    );
    fs::create_dir(dir.join("dir")).expect("dir");
    let (code, detail) =
        target_accepted(&dir.join("dir"), "out/dir", Named::Caller).expect_err("a dir");
    assert_eq!(code, codes::OUTPUT_NOT_A_FILE);
    assert_eq!(
        detail,
        format!("`out/dir` is a directory; {ONLY_FILES}"),
        "names the path as shown"
    );
    // A parent that is a file cannot hold the entry: left to the write, which fails.
    assert_eq!(
        target_accepted(&dir.join("file/below"), "below", Named::Caller),
        Ok(())
    );
    // A name that is oce's own is removed or replaced by its owner, whatever it is.
    let (_, detail) =
        target_accepted(&dir.join("dir"), "out/receipt.json", Named::Tool).expect_err("dir");
    assert_eq!(
        detail,
        format!("`out/receipt.json` is a directory: remove or replace that entry; {ONLY_FILES}")
    );
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_link_dangling_or_not_and_a_named_pipe_are_not_a_file() {
    let dir = scratch("target_links");
    fs::write(dir.join("file"), b"x").expect("file");
    std::os::unix::fs::symlink("file", dir.join("link")).expect("link");
    std::os::unix::fs::symlink("nowhere", dir.join("dangling")).expect("link");
    std::os::unix::fs::symlink("link", dir.join("chain")).expect("link");
    for name in ["link", "dangling", "chain"] {
        let (code, detail) = target_accepted(&dir.join(name), name, Named::Caller).expect_err(name);
        assert_eq!(code, codes::OUTPUT_NOT_A_FILE, "{name}");
        assert!(
            detail.starts_with(&format!("`{name}` is a symbolic link")),
            "{detail}"
        );
        assert!(
            detail.contains("give the path of the file it names"),
            "{detail}"
        );
    }
    let made = std::process::Command::new("mkfifo")
        .arg(dir.join("fifo"))
        .status()
        .expect("mkfifo runs");
    assert!(made.success());
    let (code, detail) =
        target_accepted(&dir.join("fifo"), "fifo", Named::Caller).expect_err("a fifo");
    assert_eq!(code, codes::OUTPUT_NOT_A_FILE);
    assert!(
        detail.contains("a named pipe, a socket or a device"),
        "{detail}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Serializes the tests that predict the counter, and every test that writes beside them, for a
/// runner that shares one process.
pub(crate) static COUNTER: Mutex<()> = Mutex::new(());

/// A fresh directory under the system temporary directory for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "oce-cli-atomic-write-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create the scratch directory");
    dir
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("list")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_regular_file_at_the_next_temporary_names_is_never_clobbered() {
    let _serial = COUNTER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = scratch("regular");
    let target = dir.join("t.oce");
    fs::write(&target, b"old").expect("target");
    let next = peek_counter();
    for counter in next..next + 3 {
        fs::write(temporary_path(&target, counter), b"someone else's").expect("plant");
    }
    assert_eq!(write_atomically(&target, b"new"), Ok(()));
    assert_eq!(fs::read(&target).expect("read"), b"new");
    for counter in next..next + 3 {
        assert_eq!(
            fs::read(temporary_path(&target, counter)).expect("planted file"),
            b"someone else's"
        );
    }
    assert_eq!(
        names(&dir).len(),
        4,
        "only the target and the planted files"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn every_temporary_name_taken_is_a_typed_failure_and_the_target_is_untouched() {
    let _serial = COUNTER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = scratch("exhausted");
    let target = dir.join("t.oce");
    fs::write(&target, b"old").expect("target");
    let next = peek_counter();
    for counter in next..next + NAME_ATTEMPTS {
        fs::write(temporary_path(&target, counter), b"x").expect("plant");
    }
    let error = write_atomically(&target, b"new").expect_err("no free name");
    assert_eq!(error.0, codes::WRITE_FAILED);
    assert_eq!(fs::read(&target).expect("read"), b"old");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn every_temporary_name_a_dangling_link_is_a_typed_failure_that_creates_nothing() {
    let _serial = COUNTER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = scratch("exhausted_links");
    let target = dir.join("t.oce");
    let victim = dir.join("victim.txt");
    fs::write(&target, b"old").expect("target");
    let next = peek_counter();
    for counter in next..next + NAME_ATTEMPTS {
        std::os::unix::fs::symlink(&victim, temporary_path(&target, counter)).expect("plant");
    }
    let error = write_atomically(&target, b"new").expect_err("no free name");
    assert_eq!(error.0, codes::WRITE_FAILED);
    assert_eq!(fs::read(&target).expect("read"), b"old");
    assert!(
        fs::symlink_metadata(&victim).is_err(),
        "no write went through a dangling link"
    );
    for counter in next..next + NAME_ATTEMPTS {
        let planted = fs::symlink_metadata(temporary_path(&target, counter)).expect("planted");
        assert!(
            planted.file_type().is_symlink(),
            "the planted link is intact"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn the_temporary_file_is_never_more_permissive_than_the_target() {
    let _serial = COUNTER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &Path| fs::metadata(path).expect("metadata").permissions().mode() & 0o777;
    let dir = scratch("temporary_mode");
    let target = dir.join("secret.oce");
    fs::write(&target, b"old").expect("target");
    let directory = Directory::open(&dir).expect("the directory");
    for wanted in [0o600, 0o640, 0o400] {
        fs::set_permissions(&target, fs::Permissions::from_mode(wanted)).expect("chmod");
        let (temporary, file) =
            create_temporary(&directory, OsStr::new("secret.oce"), Some(wanted)).expect("create");
        let temporary = dir.join(temporary);
        let created = mode(&temporary);
        drop(file);
        let _ = fs::remove_file(&temporary);
        assert_eq!(created & !wanted, 0, "{created:o} widens {wanted:o}");
    }
    let (temporary, file) =
        create_temporary(&directory, OsStr::new("new.oce"), None).expect("create");
    let temporary = dir.join(temporary);
    let created = mode(&temporary);
    drop(file);
    let _ = fs::remove_file(&temporary);
    assert_eq!(created, NEW_FILE_MODE, "a new file is owner-only");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_link_planted_at_the_next_temporary_names_never_redirects_the_write() {
    let _serial = COUNTER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = scratch("planted_link");
    let target = dir.join("v.oce");
    let victim = dir.join("victim.txt");
    fs::write(&target, b"old").expect("target");
    fs::write(&victim, b"victim").expect("victim");
    let next = peek_counter();
    for counter in next..next + 3 {
        std::os::unix::fs::symlink(&victim, temporary_path(&target, counter)).expect("plant");
    }
    assert_eq!(write_atomically(&target, b"new"), Ok(()));
    assert_eq!(fs::read(&victim).expect("victim"), b"victim");
    assert_eq!(fs::read(&target).expect("target"), b"new");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_rename_removes_the_temporary_file() {
    let _serial = COUNTER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = scratch("rename");
    let target = dir.join("out");
    fs::create_dir(&target).expect("a directory at the target");
    fs::write(target.join("inside"), b"x").expect("non-empty");
    let error = write_atomically(&target, b"new").expect_err("rename over a directory");
    assert_eq!(error.0, codes::WRITE_FAILED);
    assert_eq!(names(&dir), ["out"], "no temporary file is left behind");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_long_target_name_keeps_the_temporary_name_bounded() {
    let long = "n".repeat(240);
    let path = temporary_path(Path::new(&long), u64::MAX);
    let name = path
        .file_name()
        .expect("a name")
        .to_string_lossy()
        .into_owned();
    assert!(name.len() <= 255, "{}", name.len());
    assert!(name.starts_with(&format!(".{}.", "n".repeat(NAME_PREFIX_BYTES))));
    assert!(name.ends_with(&format!(".{}.oce-tmp", u64::MAX)));
}

#[test]
fn a_long_multibyte_target_name_is_cut_on_bytes_at_a_character_boundary() {
    // Three-byte characters: 200 is not a multiple of 3, so the cut must step back.
    let long = "\u{20ac}".repeat(120);
    let path = temporary_path(Path::new(&long), u64::MAX);
    let name = path.file_name().expect("a name").to_str().expect("UTF-8");
    assert!(name.len() <= 255, "{}", name.len());
    let prefix = "\u{20ac}".repeat(NAME_PREFIX_BYTES / 3);
    assert!(name.starts_with(&format!(".{prefix}.")), "{name}");
    assert!(name.ends_with(&format!(".{}.oce-tmp", u64::MAX)));
}

#[cfg(unix)]
#[test]
fn a_directory_that_cannot_be_written_is_a_typed_failure_that_changes_nothing() {
    let _serial = COUNTER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("read_only");
    let target = dir.join("t.oce");
    fs::write(&target, b"old").expect("target");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).expect("chmod");
    // A user the permission bits do not bind (root) can write anyway: nothing to test then.
    let probe = fs::write(dir.join("probe"), b"");
    let result = write_atomically(&target, b"new");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("chmod back");
    if probe.is_ok() {
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    let (code, detail) = result.expect_err("a read-only directory");
    assert_eq!(code, codes::WRITE_FAILED);
    assert_eq!(detail, "the output could not be written: permission denied");
    assert_eq!(names(&dir), ["t.oce"], "nothing is created");
    assert_eq!(fs::read(&target).expect("target"), b"old");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_directory_that_can_be_searched_but_not_listed_is_refused_as_permission_denied() {
    let _serial = COUNTER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // The handles are opened read-only (rustix has no `O_SEARCH` on macOS): a parent with mode
    // `-wx` is refused where a path-based write used to pass through it (issue #771).
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("search_only");
    let parent = dir.join("p");
    fs::create_dir(&parent).expect("p");
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o300)).expect("chmod");
    let listable = fs::read_dir(&parent).is_ok();
    let result = write_atomically(&parent.join("t.oce"), b"new");
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).expect("chmod back");
    if !listable {
        let (code, detail) = result.expect_err("a search-only directory");
        assert_eq!(code, codes::WRITE_FAILED);
        assert_eq!(detail, "the output could not be written: permission denied");
        assert!(names(&parent).is_empty(), "nothing is created");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn an_entry_that_is_no_longer_the_file_read_is_never_replaced() {
    let _serial = COUNTER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // `fmt --write`'s condition: the entry must still be the file read when the rename comes.
    let dir = scratch("unchanged");
    let directory = Directory::open(&dir).expect("the directory");
    fs::write(dir.join("t.oce"), b"read").expect("t");
    let read = directory
        .entry(OsStr::new("t.oce"), false)
        .expect("the file read");
    // Another file takes the name after the read.
    fs::rename(dir.join("t.oce"), dir.join("moved")).expect("move");
    fs::write(dir.join("t.oce"), b"another").expect("another");
    let plan = Plan {
        lend: Lend::Read(read.mode),
        unchanged: Some(read),
        made: &[],
    };
    let (code, detail) =
        write_in(&directory, OsStr::new("t.oce"), b"new", &plan).expect_err("changed");
    assert_eq!(code, codes::WRITE_FAILED);
    assert!(detail.ends_with("the file changed after it was read; nothing is written"));
    assert_eq!(names(&dir), ["moved", "t.oce"], "no temporary file is left");
    assert_eq!(fs::read(dir.join("t.oce")).expect("t"), b"another");
    // The file read, back at its name, is replaced, and its mode is the one read.
    fs::remove_file(dir.join("t.oce")).expect("unlink");
    fs::rename(dir.join("moved"), dir.join("t.oce")).expect("back");
    assert_eq!(
        write_in(&directory, OsStr::new("t.oce"), b"new", &plan),
        Ok(())
    );
    assert_eq!(fs::read(dir.join("t.oce")).expect("t"), b"new");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_directory_oce_made_that_someone_else_owns_is_refused_before_a_byte_is_written() {
    let _serial = COUNTER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // The root directory stands in for a directory another user swapped in: owned by root, so
    // not by the user who owns the temporary file (unless that user is root: nothing to test).
    let dir = scratch("not_made");
    let directory = Directory::open(&dir).expect("the directory");
    let foreign = Directory::open(Path::new("/")).expect("the root directory");
    let plan = Plan {
        made: &[&directory, &foreign],
        ..Plan::PLAIN
    };
    let result = write_in(&directory, OsStr::new("errors.csv"), b"x,y\n", &plan);
    if foreign.owner().expect("owner") == directory.owner().expect("owner") {
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    let (code, detail) = result.expect_err("a directory oce did not make");
    assert_eq!(code, codes::WRITE_FAILED);
    assert!(
        detail.contains("replaced by one it does not own"),
        "{detail}"
    );
    assert!(names(&dir).is_empty(), "no temporary file is left");
    // Every directory owned as made: the write goes through.
    let plan = Plan {
        made: &[&directory],
        ..Plan::PLAIN
    };
    assert_eq!(
        write_in(&directory, OsStr::new("errors.csv"), b"x,y\n", &plan),
        Ok(())
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The adversary of the writer (module header): every schedule of swaps of the directory a write
/// goes into for a link to a victim directory, over the points between the writer's calls.
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod schedules {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use rustix::fs::{CWD, RenameFlags, renameat_with};

    use super::super::write_atomically;
    use crate::commands::steps::{step, with_adversary};

    /// `base/real` (the directory written into), `base/victim` (holding `keep`) and
    /// `base/real.alt`, a link to `victim`, which exchanges with `real`.
    struct Layout {
        base: PathBuf,
    }

    impl Layout {
        fn make(test: &str) -> Option<Self> {
            let base = std::env::temp_dir().join(format!(
                "oce-cli-write-schedule-{test}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&base);
            fs::create_dir_all(base.join("real")).expect("real");
            fs::create_dir_all(base.join("victim")).expect("victim");
            fs::write(base.join("victim/keep"), b"PRECIOUS").expect("keep");
            let base = fs::canonicalize(&base).expect("canonical");
            std::os::unix::fs::symlink(base.join("victim"), base.join("real.alt")).expect("link");
            let layout = Self { base };
            match renameat_with(
                CWD,
                layout.base.join("real"),
                CWD,
                layout.base.join("real.alt"),
                RenameFlags::EXCHANGE,
            ) {
                Ok(()) => {
                    layout.swap();
                    Some(layout)
                }
                Err(error) => {
                    #[allow(
                        clippy::print_stderr,
                        reason = "an explicit skip note in the test output"
                    )]
                    {
                        eprintln!("skipped: no atomic exchange on this file system ({error:?})");
                    }
                    None
                }
            }
        }

        fn swap(&self) {
            renameat_with(
                CWD,
                self.base.join("real"),
                CWD,
                self.base.join("real.alt"),
                RenameFlags::EXCHANGE,
            )
            .expect("exchange");
        }

        /// Put `real` back under its name and give it `target` as `t`: a file, or a non-empty
        /// directory, so that the rename fails.
        fn reset(&self, rename_fails: bool) {
            if fs::symlink_metadata(self.base.join("real"))
                .expect("real")
                .is_symlink()
            {
                self.swap();
            }
            let real = self.base.join("real");
            let _ = fs::remove_dir_all(&real);
            fs::create_dir(&real).expect("real");
            if rename_fails {
                fs::create_dir_all(real.join("t/inside")).expect("a directory at the target");
            } else {
                fs::write(real.join("t"), b"old").expect("t");
            }
        }

        /// What one schedule left: the victim untouched, no temporary file anywhere, and `t`.
        fn left(&self) -> (bool, bool, Option<Vec<u8>>) {
            let names = |dir: &Path| -> Vec<String> {
                let mut names: Vec<String> = fs::read_dir(dir)
                    .expect("list")
                    .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
                    .collect();
                names.sort();
                names
            };
            let (real, victim) = (self.base.join("real"), self.base.join("victim"));
            let victim_untouched = names(&victim) == ["keep"]
                && fs::read(victim.join("keep")).expect("keep") == b"PRECIOUS";
            let stranded = [&real, &victim]
                .iter()
                .any(|dir| names(dir).iter().any(|name| name.ends_with(".oce-tmp")));
            let t = fs::read(real.join("t")).ok();
            (victim_untouched, stranded, t)
        }
    }

    impl Drop for Layout {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    /// The writer as it was before issue #771, with the adversary at the same points: every
    /// step by path.
    fn path_based(path: &Path, bytes: &[u8]) -> Result<(), ()> {
        let temporary = path.with_file_name(".t.control.oce-tmp");
        step();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(drop)?;
        step();
        let written = file.write_all(bytes).and_then(|()| file.sync_all());
        drop(file);
        step();
        written
            .and_then(|()| fs::rename(&temporary, path))
            .map_err(|_| {
                let _ = fs::remove_file(&temporary);
            })
    }

    /// Run `write` under every schedule of swaps over its points; what each left.
    fn every_schedule(
        layout: &Layout,
        rename_fails: bool,
        write: &dyn Fn(&Path) -> bool,
    ) -> Vec<(bool, bool, Option<Vec<u8>>)> {
        let target = layout.base.join("real/t");
        let points = std::rc::Rc::new(std::cell::Cell::new(0_u32));
        let counted = points.clone();
        layout.reset(rename_fails);
        with_adversary(Some(Box::new(move || counted.set(counted.get() + 1))));
        assert_eq!(write(&target), !rename_fails, "a run with no swap");
        let k = points.get();
        assert!((3..12).contains(&k), "{k} points");
        let mut results = Vec::new();
        for schedule in 0..(1_u32 << k) {
            layout.reset(rename_fails);
            let base = layout.base.clone();
            let mut index = 0;
            with_adversary(Some(Box::new(move || {
                if schedule & (1 << index) != 0 {
                    renameat_with(
                        CWD,
                        base.join("real"),
                        CWD,
                        base.join("real.alt"),
                        RenameFlags::EXCHANGE,
                    )
                    .expect("exchange");
                }
                index += 1;
            })));
            let _ = write(&target);
            with_adversary(None);
            // Look with `real` back under its name.
            if fs::symlink_metadata(layout.base.join("real"))
                .expect("real")
                .is_symlink()
            {
                layout.swap();
            }
            results.push(layout.left());
        }
        results
    }

    fn handles(target: &Path) -> bool {
        write_atomically(target, b"new").is_ok()
    }

    fn control(target: &Path) -> bool {
        path_based(target, b"new").is_ok()
    }

    #[test]
    fn no_schedule_of_parent_swaps_writes_outside_the_directory_opened() {
        let _serial = super::COUNTER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(layout) = Layout::make("written") else {
            return;
        };
        for (victim_untouched, stranded, t) in every_schedule(&layout, false, &handles) {
            assert!(victim_untouched, "a write reached the victim");
            assert!(!stranded, "a temporary file was left");
            assert_eq!(
                t.as_deref(),
                Some(&b"new"[..]),
                "the write landed in `real`"
            );
        }
        let control = every_schedule(&layout, false, &control);
        assert!(
            control
                .iter()
                .any(|(victim_untouched, _, _)| !victim_untouched),
            "the adversary is not live: the path-based writer held"
        );
    }

    #[test]
    fn no_schedule_of_parent_swaps_strands_a_temporary_file_after_a_failed_rename() {
        let _serial = super::COUNTER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(layout) = Layout::make("failed") else {
            return;
        };
        for (victim_untouched, stranded, t) in every_schedule(&layout, true, &handles) {
            assert!(victim_untouched, "a write reached the victim");
            assert!(!stranded, "a temporary file was left");
            assert_eq!(t, None, "the directory at the target stays");
        }
        let control = every_schedule(&layout, true, &control);
        assert!(
            control.iter().any(|(_, stranded, _)| *stranded),
            "the adversary is not live: the path-based clean-up held"
        );
    }
}
