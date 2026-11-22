//! Opening a file a document names by a relative path, held inside the document's directory: a
//! vector set's mutants (owner ruling of 2026-11-11 on mutant paths) and a commissioning spec's
//! CXF, mapping and trend files (commissioning note, "CLI" "Reads"; issue #664).
//!
//! Every listed path is lexically sound already: each format refuses an absolute path, an empty,
//! `.` or `..` segment and a backslash (`mutant-path-invalid`, `path-invalid`). The format cannot
//! see every alias, so the file is **resolved** here before a byte is read:
//!
//! 1. its `/`-separated segments are joined onto the document's directory, and the result is
//!    canonicalized, following every symbolic link;
//! 2. the canonical path must lie inside the canonical directory, or the refusal is the caller's
//!    outside code (a link out, or a Windows drive prefix such as `C:`);
//! 3. the canonical path's metadata must be a regular file before it is opened, or the refusal
//!    is `not-a-file`: opening a named pipe for reading blocks until a writer comes, and a socket
//!    or a device is no input. The path is then opened **once**; the handle must be a regular
//!    file with the identity that was checked (`read-failed` otherwise: the path changed between
//!    the check and the open). The caller reads the size and the bytes from that one handle.
//!
//! What stays racy: between the canonicalization (1) and the open (3) another writer in the
//! directory can swap a component for a link, which the open then follows, or swap the file for a
//! named pipe, which the open then waits on. std offers no component-by-component no-follow open,
//! and a non-blocking open needs a per-system constant std does not export without `libc`. A hard
//! link inside the directory to a file outside it is read: a hard link has no other path to
//! resolve.

use std::fs::{File, Metadata};
use std::path::{Path, PathBuf};

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
        Ok(Self {
            directory,
            canonical,
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
        let canonical = std::fs::canonicalize(&joined).map_err(io_refusal)?;
        if !canonical.starts_with(&self.canonical) {
            return Err((
                outside,
                format!("it resolves to a file outside {whose} directory"),
            ));
        }
        // A regular file, judged from the path's metadata **before** the open.
        let checked = std::fs::metadata(&canonical).map_err(io_refusal)?;
        if !checked.is_file() {
            return Err(not_a_file());
        }
        let file = File::open(&canonical).map_err(io_refusal)?;
        let metadata = file.metadata().map_err(io_refusal)?;
        // The handle must be the file that was checked: anything else is the path changing
        // between the check and the open, and is refused, never read.
        if !metadata.is_file() {
            return Err(not_a_file());
        }
        let this = identity(&canonical, &metadata);
        if this != identity(&canonical, &checked) {
            return Err((
                codes::READ_FAILED,
                "the file changed between its check and its open".to_owned(),
            ));
        }
        Ok(Opened {
            file,
            metadata,
            identity: this,
            canonical,
        })
    }
}

/// The refusal of a path that is not a regular file.
pub(crate) fn not_a_file() -> Refusal {
    (
        codes::NOT_A_FILE,
        "the path is not a regular file".to_owned(),
    )
}
