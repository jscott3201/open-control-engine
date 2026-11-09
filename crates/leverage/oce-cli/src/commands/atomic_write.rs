//! Writing a file atomically, for `fmt --write` and `emit -o`.
//!
//! The bytes go to a uniquely named temporary file beside the target, opened with `create_new`
//! (`O_EXCL`: it never follows a symbolic link and never truncates a file that is already there),
//! are synced, take the target's permissions when the target exists (Unix), and are renamed over
//! the target. A target that is a symbolic link is followed: the temporary file is created beside
//! the resolved file and renamed over it, so the link stays a link and the file it names changes.
//! The file is replaced, not rewritten in place, so a hard link to the old inode keeps the old
//! bytes. On every failure the temporary file (and only a file this run created) is removed and
//! the target is untouched.

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::codes;

/// How many temporary names one write tries before it gives up with `write-failed`.
pub(crate) const NAME_ATTEMPTS: u64 = 16;

/// The longest prefix of the target's file name a temporary name carries (in characters), so the
/// temporary name stays within a file system's name limit when the target's name is long.
const NAME_PREFIX_CHARS: usize = 96;

/// The per-process counter that makes each temporary name unique within this process.
static NEXT_NAME: AtomicU64 = AtomicU64::new(0);

/// Write `bytes` to `path` atomically (module header). The error is `write-failed` and a detail
/// that names the failure by kind, never by an operating-system message that could carry a path.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), (&'static str, String)> {
    let target = resolve(path).map_err(failed)?;
    // The target's permissions, when it exists; a missing target is a new file.
    let existing = fs::metadata(&target).ok();
    let (temporary, mut file) = create_temporary(&target)?;
    let written = file
        .write_all(bytes)
        .and_then(|()| copy_permissions(&file, existing.as_ref()))
        .and_then(|()| file.sync_all());
    drop(file);
    let result = written.and_then(|()| fs::rename(&temporary, &target));
    result.map_err(|error| {
        let _ = fs::remove_file(&temporary);
        failed(error)
    })
}

/// The file the write replaces: `path` itself, or the file a symbolic link at `path` resolves to.
/// A dangling link is refused (`NotFound`) rather than created through.
fn resolve(path: &Path) -> io::Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path),
        _ => Ok(path.to_path_buf()),
    }
}

/// Create a fresh temporary file beside `target` with `create_new`, trying [`NAME_ATTEMPTS`]
/// unique names; a name that already exists (a file, a directory or a planted link) is skipped,
/// never opened.
fn create_temporary(target: &Path) -> Result<(PathBuf, File), (&'static str, String)> {
    for _ in 0..NAME_ATTEMPTS {
        let temporary = temporary_path(target, NEXT_NAME.fetch_add(1, Ordering::Relaxed));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(failed(error)),
        }
    }
    Err((
        codes::WRITE_FAILED,
        format!("the output could not be written: no free temporary name in {NAME_ATTEMPTS} tries"),
    ))
}

/// `dir/.name.<pid>.<counter>.oce-tmp` beside `target`, so the rename never crosses a file system.
pub(crate) fn temporary_path(target: &Path, counter: u64) -> PathBuf {
    let name: String = target
        .file_name()
        .map_or_else(|| "output".into(), OsStr::to_string_lossy)
        .chars()
        .take(NAME_PREFIX_CHARS)
        .collect();
    let pid = std::process::id();
    target.with_file_name(format!(".{name}.{pid}.{counter}.oce-tmp"))
}

/// The counter value the next temporary name will use (for the unit tests).
#[cfg(test)]
pub(crate) fn peek_counter() -> u64 {
    NEXT_NAME.load(Ordering::Relaxed)
}

/// Give the temporary file the target's permissions (Unix mode bits) through its open handle, so
/// a `0600` or `0640` file keeps its mode. Elsewhere this does nothing: the only permission bit is
/// read-only, and a read-only target refuses the rename regardless.
#[cfg(unix)]
fn copy_permissions(file: &File, existing: Option<&fs::Metadata>) -> io::Result<()> {
    match existing {
        Some(metadata) => file.set_permissions(metadata.permissions()),
        None => Ok(()),
    }
}

#[cfg(not(unix))]
fn copy_permissions(_file: &File, _existing: Option<&fs::Metadata>) -> io::Result<()> {
    Ok(())
}

fn failed(error: io::Error) -> (&'static str, String) {
    (
        codes::WRITE_FAILED,
        format!("the output could not be written: {}", error.kind()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes the tests that predict the counter, for a runner that shares one process.
    static COUNTER: Mutex<()> = Mutex::new(());

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
        let path = temporary_path(Path::new(&long), 7);
        let name = path
            .file_name()
            .expect("a name")
            .to_string_lossy()
            .into_owned();
        assert!(name.len() < 140, "{}", name.len());
        assert!(name.starts_with(&format!(".{}", "n".repeat(NAME_PREFIX_CHARS))));
        assert!(name.ends_with(".7.oce-tmp"));
    }
}
