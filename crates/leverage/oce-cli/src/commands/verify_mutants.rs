//! Reading a vector set's mutants (vectors note, "CLI" "Reads"; the owner's ruling of 2026-11-11
//! on mutant paths, "Part 3's readings" and "Part 4's readings").
//!
//! Every listed path is lexically sound already: the format refuses an absolute path, an empty,
//! `.` or `..` segment and a backslash (`mutant-path-invalid`). The format cannot see every alias,
//! so each mutant, in file order, is **resolved** here before a byte is read:
//!
//! 1. its `/`-separated segments are joined onto the vector file's directory, and the result is
//!    canonicalized, following every symbolic link;
//! 2. the canonical path must lie inside the canonical directory, or the refusal is
//!    `mutant-outside-directory` (a link out, or a Windows drive prefix such as `C:`);
//! 3. the canonical path's metadata must be a regular file before it is opened, or the refusal
//!    is `not-a-file`: opening a named pipe for reading blocks until a writer comes, and a socket
//!    or a device is no mutant. The path is then opened **once**; the handle must be a regular
//!    file with the identity that was checked (`read-failed` otherwise: the path changed between
//!    the check and the open), and its size and bytes come from that one handle: a file over
//!    `oce_api::MAX_CXF_BYTES` is `input-too-large`, and a running total over
//!    `VerifyLimits::max_mutant_bytes` (64 MiB) is `too-many-mutant-bytes`, each decided from the
//!    handle's metadata before its bytes are read;
//! 4. two listings that are one file are `duplicate-mutant`: one file is one (device, inode) on
//!    Unix, so case twins, normalization twins and hard links are caught where the file system
//!    folds them, and one canonical path on Windows, whose final path folds case and trailing
//!    dots and spaces (std has no stable file index there, so hard links are not caught).
//!
//! What stays racy: between the canonicalization (1) and the open (3) another writer in the
//! directory can swap a component for a link, which the open then follows, or swap the file for a
//! named pipe, which the open then waits on; std offers no component-by-component no-follow open
//! and no non-blocking open. A hard link inside the directory to a file outside it is read: a
//! hard link has no other path to resolve. A file that grows after its metadata is read is still
//! bounded: the read stops one byte past the size allowed and refuses.
//!
//! Every refusal names the mutant (`mutants[i]` and its listed path), never a host path.

use std::fs::File;
use std::path::{Path, PathBuf};

use oce_verify::{VectorSet, VerifyLimits};

use crate::codes;
use crate::input::{io_refusal, read_bounded, too_large};

/// One mutant as read: its listed path and its bytes.
pub(crate) type Read = (String, Vec<u8>);

/// A refusal of one mutant: the place, the code and a detail naming the listing.
pub(crate) struct Refused {
    /// `mutants[i]`, written as the error's `at`.
    pub(crate) at: String,
    /// The code.
    pub(crate) code: &'static str,
    /// The sentence, naming the listed path.
    pub(crate) detail: String,
}

/// `oce-verify`'s word for one mutant listed twice, which two listings of one file share (owner
/// ruling of 2026-11-11).
const DUPLICATE_MUTANT: &str = "duplicate-mutant";

/// `oce-verify`'s word for the mutants' bytes past their bound, which the read reuses.
const TOO_MANY_MUTANT_BYTES: &str = "too-many-mutant-bytes";

/// What makes two listings one file: the device and inode of the open file.
#[cfg(unix)]
type Identity = (u64, u64);

/// What makes two listings one file: the canonical path, the final path Windows gives.
#[cfg(not(unix))]
type Identity = PathBuf;

/// The refusal of a path that is not a regular file.
fn not_a_file() -> (&'static str, String) {
    (
        codes::NOT_A_FILE,
        "the path is not a regular file".to_owned(),
    )
}

#[cfg(unix)]
fn identity(_canonical: &Path, metadata: &std::fs::Metadata) -> Identity {
    use std::os::unix::fs::MetadataExt;
    (metadata.dev(), metadata.ino())
}

#[cfg(not(unix))]
fn identity(canonical: &Path, _metadata: &std::fs::Metadata) -> Identity {
    canonical.to_path_buf()
}

/// Read every listed mutant of `set`, resolving each against the directory of `vectors` (module
/// header), in file order; the first refusal stops the reading.
pub(crate) fn read(set: &VectorSet, vectors: &Path) -> Result<Vec<Read>, Refused> {
    let Some(listed) = set.mutants().filter(|listed| !listed.is_empty()) else {
        return Ok(Vec::new());
    };
    let directory = match vectors.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let refuse = |index: usize, path: &str, (code, why): (&'static str, String)| Refused {
        at: format!("mutants[{index}]"),
        code,
        detail: format!("mutant `{path}`: {why}"),
    };
    let root = std::fs::canonicalize(&directory).map_err(|error| {
        let (code, why) = io_refusal(error);
        Refused {
            at: "mutants".to_owned(),
            code,
            detail: format!("the vector file's directory: {why}"),
        }
    })?;
    let total_bound = VerifyLimits::MAX.max_mutant_bytes;
    let mut total: u64 = 0;
    let mut seen: Vec<(Identity, usize)> = Vec::with_capacity(listed.len());
    let mut read = Vec::with_capacity(listed.len());
    for (index, mutant) in listed.iter().enumerate() {
        let path = mutant.path();
        let mut joined = directory.clone();
        for segment in path.split('/') {
            joined.push(segment);
        }
        let canonical = std::fs::canonicalize(&joined)
            .map_err(|error| refuse(index, path, io_refusal(error)))?;
        if !canonical.starts_with(&root) {
            return Err(refuse(
                index,
                path,
                (
                    codes::MUTANT_OUTSIDE_DIRECTORY,
                    "it resolves to a file outside the vector file's directory".to_owned(),
                ),
            ));
        }
        // A regular file, judged from the path's metadata **before** the open: opening a named
        // pipe for reading blocks until a writer comes, and a socket or a device is no mutant.
        let checked = std::fs::metadata(&canonical)
            .map_err(|error| refuse(index, path, io_refusal(error)))?;
        if !checked.is_file() {
            return Err(refuse(index, path, not_a_file()));
        }
        let file =
            File::open(&canonical).map_err(|error| refuse(index, path, io_refusal(error)))?;
        let metadata = file
            .metadata()
            .map_err(|error| refuse(index, path, io_refusal(error)))?;
        // The handle must be the file that was checked: anything else is the path changing
        // between the check and the open, and is refused, never read.
        if !metadata.is_file() {
            return Err(refuse(index, path, not_a_file()));
        }
        let this = identity(&canonical, &metadata);
        if this != identity(&canonical, &checked) {
            return Err(refuse(
                index,
                path,
                (
                    codes::READ_FAILED,
                    "the file changed between its check and its open".to_owned(),
                ),
            ));
        }
        if let Some((_, first)) = seen.iter().find(|(other, _)| *other == this) {
            return Err(refuse(
                index,
                path,
                (
                    DUPLICATE_MUTANT,
                    format!(
                        "it is the same file as mutants[{first}] (`{}`)",
                        listed[*first].path()
                    ),
                ),
            ));
        }
        seen.push((this, index));
        let size = metadata.len();
        let per_file = u64::try_from(oce_api::MAX_CXF_BYTES).unwrap_or(u64::MAX);
        if size > per_file {
            return Err(refuse(index, path, too_large(size, oce_api::MAX_CXF_BYTES)));
        }
        if total.saturating_add(size) > total_bound {
            return Err(refuse(
                index,
                path,
                (
                    TOO_MANY_MUTANT_BYTES,
                    format!(
                        "the mutants up to this one are {} bytes; at most {total_bound} are read",
                        total.saturating_add(size)
                    ),
                ),
            ));
        }
        // The read stops one byte past what is still allowed: a file that grew since its
        // metadata was read is refused, never held.
        let room = per_file.min(total_bound - total);
        let bound = usize::try_from(room).unwrap_or(usize::MAX);
        let bytes = read_bounded(file, bound).map_err(|refusal| refuse(index, path, refusal))?;
        total = total.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        read.push((path.to_owned(), bytes));
    }
    Ok(read)
}
