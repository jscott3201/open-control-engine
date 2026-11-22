//! Reading a document and the files it names by relative paths, held inside the document's
//! directory: a vector set and its mutants (owner ruling of 2026-11-11 on mutant paths), a
//! commissioning spec and its CXF, mapping and trend files (commissioning note, "CLI" "Reads";
//! issue #664).
//!
//! **The document** ([`Root::read_document`]; issue #771). Its directory, as the document's path
//! gives it (`dir/spec.json` gives `dir`, a bare name `.`), is opened **once**, following the
//! links on that path (the caller named it, so `bundle -> real` reads `real`), then canonicalized,
//! and the handle must have the device and inode of the canonical directory. The document is
//! then read **through that handle**: its entry, following a link there (the caller named it, so
//! a spec that is a link reads the files beside the link, not beside its target), must be a
//! regular file (`not-a-file`; a named pipe is never opened), is opened with `O_NONBLOCK`, must
//! be the entry checked, and its size is judged from that handle before a byte is read. Every
//! file the document names is then resolved against that same handle, never against the
//! directory's path again, so a folder exchanged during the run cannot pair the document with
//! another folder's files.
//!
//! **A file it names** ([`Root::open`]). Every listed path is lexically sound already: each format
//! refuses an absolute path, an empty, `.` or `..` segment and a backslash (`mutant-path-invalid`,
//! `path-invalid`). The format cannot see every alias, so the file is **resolved** here before a
//! byte is read:
//!
//! 1. its `/`-separated segments are joined onto the document's directory, and the result is
//!    canonicalized, following every symbolic link, so a link that stays inside is honoured;
//! 2. the canonical path must lie inside the canonical directory, or the refusal is the caller's
//!    outside code (a link out, or a Windows drive prefix such as `C:`);
//! 3. the segments below the canonical directory are opened **from the document's directory
//!    handle without following a link anywhere** (Unix; issue #758, ruled by the owner on
//!    2026-11-22): each directory with `openat` and `O_DIRECTORY | O_NOFOLLOW` from its parent's
//!    handle; the last entry's own metadata, read from its directory's handle without following a
//!    link, must be a regular file (`not-a-file` otherwise: opening a named pipe for reading
//!    blocks until a writer comes, and a socket or a device is no input), and it is then opened
//!    with `O_NOFOLLOW | O_NONBLOCK`, must be a regular file with the identity that was checked,
//!    and is cleared of `O_NONBLOCK` (`directory::read_at`). A canonical path holds no link, so a
//!    link found at any step, or a directory that is no longer one (a named pipe swapped in for a
//!    directory is refused by `O_DIRECTORY` before it is opened), was swapped in after the
//!    canonicalization, and is `read-failed`, never followed. The caller reads the size and the
//!    bytes from the one handle.
//!
//! Step 1 is the one call left that reads the directory by path: it only decides which segments
//! step 3 walks, and step 3 walks them from the pinned handle with no link followed. So a folder
//! swapped in at the directory's path after the pin can change **which file inside the pinned
//! directory** is read (its links steer the canonicalization), and can never reach a file outside
//! it (`contained_tests.rs` tries every schedule of that swap).
//!
//! Each handle pins what it names, so no rename in the directory can redirect a later step: the
//! review of GitHub #757 raced a link swapped in at the last component (8 reads outside the
//! directory in 3,000 with a following check), and issue #758's harness swapped an intermediate
//! directory for a link: with the path-based checks of GitHub #757, 135 reads in 1,000,000
//! escaped on macOS and 168,695 in 1,000,000 on Linux; with these opens, none on either, and none
//! under any schedule of swaps between two of these calls (`contained_tests.rs`, whose controls,
//! the path-based checks, must be beaten by some schedule).
//!
//! **Windows** has less (untested there): the document is read by path, its directory is
//! canonicalized by path, and each file's canonical path's own entry, read without following a
//! link, must be a regular file, it is opened by path, and the handle must be a regular file.
//! Nothing ties the handle to the entry checked: std has no stable file index there, so a file's
//! identity is its canonical path, which cannot tell two files at one path apart. An intermediate
//! directory, or the last entry, swapped for a link after the check is followed there; std has no
//! `openat`. A hard link inside the directory to a file outside it is read on every platform: a
//! hard link has no other path to resolve.

use std::fs::{File, Metadata};
use std::path::{Component, Path, PathBuf};

use super::directory::{self, Directory, ReadError};
#[cfg(not(unix))]
use super::file_identity::identity_of;
use super::file_identity::{Identity, identity};
use super::steps::step;
use crate::codes;
use crate::input::{io_refusal, read_bounded, too_large};

/// A refusal: the code and a path-free sentence.
pub(crate) type Refusal = (&'static str, String);

/// The directory a document's relative paths resolve against, opened once.
pub(crate) struct Root {
    /// The directory as the document's path gives it (`.` for a bare file name).
    directory: PathBuf,
    /// The directory, canonicalized.
    canonical: PathBuf,
    /// The directory, opened once: every read below it starts here.
    #[cfg_attr(not(unix), allow(dead_code, reason = "Windows reads by path"))]
    handle: Directory,
}

/// A document read through its directory's handle, and that directory.
pub(crate) struct Document {
    /// The document's directory, for the files it names.
    pub(crate) root: Root,
    /// The document's bytes, at most the caller's bound.
    pub(crate) bytes: Vec<u8>,
    /// The document's identity, for the `--out` guard (`output-is-input`); `None` where it
    /// cannot be told (Windows, a path that no longer resolves).
    pub(crate) identity: Option<Identity>,
}

/// One file opened inside a [`Root`].
pub(crate) struct Opened {
    /// The handle, opened once.
    pub(crate) file: File,
    /// The handle's metadata: a regular file.
    pub(crate) metadata: Metadata,
    /// The file's identity, for a duplicate or same-file check.
    pub(crate) identity: Identity,
}

impl Root {
    /// Open the directory of `document` once and read `document` through it, at most `bound`
    /// bytes (module header, "The document").
    ///
    /// # Errors
    /// The document's refusal, as a read of it: an I/O refusal (`not-found`, `read-failed`, ...),
    /// `not-a-file`, `input-too-large`, or `read-failed` for a directory that changed while it
    /// was resolved.
    pub(crate) fn read_document(document: &Path, bound: usize) -> Result<Document, Refusal> {
        let Some((directory, name)) = directory::split(document) else {
            // No last name (`/`, `..`): a directory, or nothing.
            std::fs::metadata(document).map_err(io_refusal)?;
            return Err(not_a_file());
        };
        let directory = directory.to_path_buf();
        step();
        let handle = Directory::open(&directory).map_err(io_refusal)?;
        step();
        let canonical = std::fs::canonicalize(&directory).map_err(io_refusal)?;
        #[cfg(unix)]
        {
            let resolved = std::fs::metadata(&canonical).map_err(io_refusal)?;
            let opened = handle.metadata().map_err(io_refusal)?;
            if opened.identity != Some(identity(&canonical, &resolved)) {
                return Err(changed());
            }
        }
        let root = Self {
            directory,
            canonical,
            handle,
        };
        let (file, entry) = root.handle.read(&name, true).map_err(|error| match error {
            ReadError::Io(error) => io_refusal(error),
            ReadError::Link | ReadError::NotFile(_) => not_a_file(),
            ReadError::Changed => changed(),
        })?;
        if entry.len > u64::try_from(bound).unwrap_or(u64::MAX) {
            return Err(too_large(entry.len, bound));
        }
        let identity = document_identity(&file, document);
        let bytes = read_bounded(file, bound)?;
        Ok(Document {
            root,
            bytes,
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
        })
    }

    /// Open `directories` then `last` below the directory's handle, following no link (module
    /// header, step 3). The handle is a regular file with the identity its entry had.
    #[cfg(unix)]
    fn open_below(
        &self,
        _canonical: &Path,
        directories: &[&std::ffi::OsStr],
        last: &std::ffi::OsStr,
    ) -> Result<File, Refusal> {
        use rustix::fs::{Mode, OFlags, openat};
        use std::os::fd::{AsFd, OwnedFd};
        // A link or a non-directory where the canonical path had a directory: the path changed.
        let directory_errno = |error: rustix::io::Errno| {
            if error == rustix::io::Errno::LOOP || error == rustix::io::Errno::NOTDIR {
                changed()
            } else {
                io_refusal(error.into())
            }
        };
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let mut below: Option<OwnedFd> = None;
        for segment in directories {
            step();
            let parent = below.as_ref().map_or(self.handle.fd(), AsFd::as_fd);
            below = Some(openat(parent, *segment, flags, Mode::empty()).map_err(directory_errno)?);
        }
        let parent = below.as_ref().map_or(self.handle.fd(), AsFd::as_fd);
        let (file, _) = directory::read_at(parent, last, false).map_err(|error| match error {
            ReadError::Io(error) => io_refusal(error),
            ReadError::NotFile(_) => not_a_file(),
            // A link at a canonical path's last entry was swapped in after the canonicalization.
            ReadError::Link | ReadError::Changed => changed(),
        })?;
        Ok(file)
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

/// The identity of the document read: its handle's on Unix, its canonical path elsewhere.
fn document_identity(file: &File, document: &Path) -> Option<Identity> {
    #[cfg(unix)]
    {
        let _ = document;
        file.metadata()
            .ok()
            .map(|metadata| identity(Path::new(""), &metadata))
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        identity_of(document)
    }
}

/// The refusal of a path that changed between its check and its open.
pub(crate) fn changed() -> Refusal {
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

#[cfg(test)]
#[path = "contained_tests.rs"]
mod contained_tests;
