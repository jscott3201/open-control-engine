//! Opening a file a document names by a relative path, held inside the document's directory: a
//! vector set's mutants (owner ruling of 2026-11-11 on mutant paths) and a commissioning spec's
//! CXF, mapping and trend files (commissioning note, "CLI" "Reads"; issue #664).
//!
//! Every listed path is lexically sound already: each format refuses an absolute path, an empty,
//! `.` or `..` segment and a backslash (`mutant-path-invalid`, `path-invalid`). The format cannot
//! see every alias, so the file is **resolved** here before a byte is read:
//!
//! 1. its `/`-separated segments are joined onto the document's directory, and the result is
//!    canonicalized, following every symbolic link, so a link that stays inside is honoured;
//! 2. the canonical path must lie inside the canonical directory, or the refusal is the caller's
//!    outside code (a link out, or a Windows drive prefix such as `C:`);
//! 3. the canonical path is opened **without following a link anywhere below the directory**
//!    (Unix; issue #758, ruled by the owner on 2026-11-22). The directory is opened by its
//!    canonical path and must still be the directory step 1 resolved (device and inode); each
//!    directory below it is opened from its parent's handle with `openat` and
//!    `O_DIRECTORY | O_NOFOLLOW`; the last segment's own entry is read from its directory's handle
//!    without following a link and must be a regular file (`not-a-file` otherwise: opening a
//!    named pipe for reading blocks until a writer comes, and a socket or a device is no input),
//!    and is then opened from that handle with `O_NOFOLLOW | O_NONBLOCK`, and the handle must be a
//!    regular file with the identity that was checked; `O_NONBLOCK` is then cleared, so the
//!    caller reads an ordinary blocking handle. A canonical path holds no link, so a link found
//!    at any step, or a directory that is no longer one (a named pipe swapped in for a directory
//!    is refused by `O_DIRECTORY` before it is opened), was swapped in after the
//!    canonicalization, and is `read-failed`, never followed. A pipe swapped in for the last entry
//!    after its check is opened without blocking and refused by its kind. The caller reads the
//!    size and the bytes from the one handle.
//!
//! Each handle pins what it names, so no rename in the directory can redirect a later step: the
//! review of GitHub #757 raced a link swapped in at the last component (8 reads outside the
//! directory in 3,000 with a following check), and issue #758's harness swapped an intermediate
//! directory for a link: with the path-based checks of GitHub #757, 135 reads in 1,000,000
//! escaped on macOS and 168,695 in 1,000,000 on Linux; with these opens, none on either, and none
//! under any schedule of swaps between two of these calls (`contained_tests.rs`, whose control,
//! the path-based checks, must be beaten by some schedule).
//!
//! **Windows** has less (untested there): the canonical path's own entry, read without following
//! a link, must be a regular file, it is opened by path, and the handle must be a regular file.
//! Nothing ties the handle to the entry checked: std has no stable file index there, so a file's
//! identity is its canonical path, which cannot tell two files at one path apart. An intermediate
//! directory, or the last entry, swapped for a link after the check is followed there; std has no
//! `openat`. A hard link inside the directory to a file outside it is read on every platform: a
//! hard link has no other path to resolve.

use std::fs::{File, Metadata};
use std::path::{Component, Path, PathBuf};

use super::file_identity::{Identity, identity};
use crate::codes;
use crate::input::io_refusal;

/// A refusal: the code and a path-free sentence.
pub(crate) type Refusal = (&'static str, String);

/// The directory a document's relative paths resolve against.
pub(crate) struct Root {
    /// The directory as the document's path gives it (`.` for a bare file name).
    directory: PathBuf,
    /// The directory, canonicalized.
    canonical: PathBuf,
    /// The canonical directory's identity when it was resolved, which the directory opened at each
    /// read must still have (Unix).
    #[cfg(unix)]
    identity: Identity,
}

/// One file opened inside a [`Root`].
pub(crate) struct Opened {
    /// The handle, opened once.
    pub(crate) file: File,
    /// The handle's metadata: a regular file.
    pub(crate) metadata: Metadata,
    /// The file's identity, for a duplicate or same-file check.
    pub(crate) identity: Identity,
    /// The canonical path the file was opened at.
    pub(crate) canonical: PathBuf,
}

impl Root {
    /// The directory of `document`, canonicalized.
    ///
    /// # Errors
    /// The directory's I/O refusal (`not-found`, `read-failed`, ...).
    pub(crate) fn of(document: &Path) -> Result<Self, Refusal> {
        let directory = match document.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let canonical = std::fs::canonicalize(&directory).map_err(io_refusal)?;
        #[cfg(unix)]
        let identity = {
            let metadata = std::fs::metadata(&canonical).map_err(io_refusal)?;
            identity(&canonical, &metadata)
        };
        Ok(Self {
            directory,
            canonical,
            #[cfg(unix)]
            identity,
        })
    }

    /// Resolve `path` (`/`-separated, relative) inside this directory and open it once (module
    /// header). `outside` is the code of a path that resolves outside the directory, and `whose`
    /// names the directory in its sentence ("the vector file's").
    ///
    /// # Errors
    /// `outside`, `not-a-file`, `read-failed` or an I/O refusal.
    pub(crate) fn open(
        &self,
        path: &str,
        outside: &'static str,
        whose: &str,
    ) -> Result<Opened, Refusal> {
        let mut joined = self.directory.clone();
        for segment in path.split('/') {
            joined.push(segment);
        }
        step();
        let canonical = std::fs::canonicalize(&joined).map_err(io_refusal)?;
        let Ok(below) = canonical.strip_prefix(&self.canonical) else {
            return Err((
                outside,
                format!("it resolves to a file outside {whose} directory"),
            ));
        };
        let mut segments = Vec::new();
        for component in below.components() {
            match component {
                Component::Normal(segment) => segments.push(segment),
                // A canonical path below a canonical one holds nothing else.
                _ => return Err(changed()),
            }
        }
        // The directory itself (a link inside that names it) is no file.
        let Some((last, directories)) = segments.split_last() else {
            return Err(not_a_file());
        };
        let file = self.open_below(&canonical, directories, last)?;
        let metadata = file.metadata().map_err(io_refusal)?;
        if !metadata.is_file() {
            return Err(not_a_file());
        }
        let this = identity(&canonical, &metadata);
        Ok(Opened {
            file,
            metadata,
            identity: this,
            canonical,
        })
    }

    /// Open `directories` then `last` below the canonical directory, following no link (module
    /// header, step 3). The handle is a regular file with the identity its entry had.
    #[cfg(unix)]
    fn open_below(
        &self,
        _canonical: &Path,
        directories: &[&std::ffi::OsStr],
        last: &std::ffi::OsStr,
    ) -> Result<File, Refusal> {
        use rustix::fs::{AtFlags, CWD, FileType, Mode, OFlags, fstat, openat, statat};
        use std::os::fd::OwnedFd;
        let errno = |error: rustix::io::Errno| io_refusal(error.into());
        // A link or a non-directory where the canonical path had a directory: the path changed.
        let directory_errno = |error: rustix::io::Errno| {
            if error == rustix::io::Errno::LOOP || error == rustix::io::Errno::NOTDIR {
                changed()
            } else {
                errno(error)
            }
        };
        let directory_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
        step();
        let root = File::from(
            openat(CWD, &self.canonical, directory_flags, Mode::empty())
                .map_err(directory_errno)?,
        );
        let opened_root = root.metadata().map_err(io_refusal)?;
        if identity(&self.canonical, &opened_root) != self.identity {
            return Err(changed());
        }
        let mut directory = OwnedFd::from(root);
        for segment in directories {
            step();
            directory = openat(
                &directory,
                *segment,
                directory_flags | OFlags::NOFOLLOW,
                Mode::empty(),
            )
            .map_err(directory_errno)?;
        }
        step();
        let checked = statat(&directory, last, AtFlags::SYMLINK_NOFOLLOW).map_err(errno)?;
        match FileType::from_raw_mode(checked.st_mode) {
            FileType::RegularFile => {}
            FileType::Symlink => return Err(changed()),
            _ => return Err(not_a_file()),
        }
        step();
        let handle = openat(
            &directory,
            last,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| {
            if error == rustix::io::Errno::LOOP {
                changed()
            } else {
                errno(error)
            }
        })?;
        let opened = fstat(&handle).map_err(errno)?;
        if FileType::from_raw_mode(opened.st_mode) != FileType::RegularFile {
            return Err(not_a_file());
        }
        // The handle must be the entry that was checked: anything else is the entry changing
        // between the check and the open, and is refused, never read.
        if (opened.st_dev, opened.st_ino) != (checked.st_dev, checked.st_ino) {
            return Err(changed());
        }
        // A regular file: the caller reads an ordinary blocking handle.
        let flags = rustix::fs::fcntl_getfl(&handle).map_err(errno)?;
        rustix::fs::fcntl_setfl(&handle, flags - OFlags::NONBLOCK).map_err(errno)?;
        Ok(File::from(handle))
    }

    /// Open the canonical path by path, checking its own entry first and the handle's kind after
    /// (module header, "Windows": no identity ties the two together there).
    #[cfg(not(unix))]
    fn open_below(
        &self,
        canonical: &Path,
        _directories: &[&std::ffi::OsStr],
        _last: &std::ffi::OsStr,
    ) -> Result<File, Refusal> {
        step();
        let checked = std::fs::symlink_metadata(canonical).map_err(io_refusal)?;
        if checked.file_type().is_symlink() {
            return Err(changed());
        }
        if !checked.is_file() {
            return Err(not_a_file());
        }
        step();
        let file = File::open(canonical).map_err(io_refusal)?;
        if !file.metadata().map_err(io_refusal)?.is_file() {
            return Err(not_a_file());
        }
        Ok(file)
    }
}

/// The refusal of a path that changed between its check and its open.
fn changed() -> Refusal {
    (
        codes::READ_FAILED,
        "the file changed between its check and its open".to_owned(),
    )
}

/// The refusal of a path that is not a regular file.
pub(crate) fn not_a_file() -> Refusal {
    (
        codes::NOT_A_FILE,
        "the path is not a regular file".to_owned(),
    )
}

/// A point between two of [`Root::open`]'s file-system calls, where the unit tests' adversary
/// acts; nothing outside the tests.
#[cfg(not(test))]
fn step() {}

#[cfg(test)]
use contained_tests::step;

#[cfg(test)]
#[path = "contained_tests.rs"]
mod contained_tests;
