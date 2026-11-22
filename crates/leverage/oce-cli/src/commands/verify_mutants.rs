//! Reading a vector set's mutants (vectors note, "CLI" "Reads"; the owner's ruling of 2026-11-11
//! on mutant paths, "Part 3's readings" and "Part 4's readings").
//!
//! Each mutant, in file order, is resolved inside the vector file's directory, through the handle
//! the set itself was read through (issue #771), and opened once
//! ([`super::contained`]: a path that resolves outside it is `mutant-outside-directory`, a path
//! that is no regular file `not-a-file`, a file that changed between its check and its open
//! `read-failed`). Then, from that one handle:
//!
//! 1. two listings that are one file are `duplicate-mutant`: one file is one (device, inode) on
//!    Unix, so case twins, normalization twins and hard links are caught where the file system
//!    folds them, and one canonical path on Windows, whose final path folds case and trailing
//!    dots and spaces (std has no stable file index there, so hard links are not caught);
//! 2. a file over `oce_api::MAX_CXF_BYTES` is `input-too-large`, and a running total over
//!    `VerifyLimits::max_mutant_bytes` (64 MiB) is `too-many-mutant-bytes`, each decided from the
//!    handle's metadata before its bytes are read. A file that grows after its metadata is read
//!    is still bounded: the read stops one byte past the size allowed and refuses.
//!
//! Every refusal names the mutant (`mutants[i]` and its listed path), never a host path.

use oce_verify::{VectorSet, VerifyLimits};

use super::contained::Root;
use super::file_identity::Identity;
use crate::codes;
use crate::input::{read_bounded, too_large};

/// One mutant as read.
pub(crate) struct Read {
    /// The path as the set lists it.
    pub(crate) path: String,
    /// The bytes.
    pub(crate) bytes: Vec<u8>,
    /// The identity of the file read, for the `--out` guard (`output-is-input`).
    pub(crate) identity: Identity,
}

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

/// Read every listed mutant of `set`, resolving each against `root`, the vector file's directory
/// as it was opened to read the set (module header), in file order; the first refusal stops the
/// reading.
pub(crate) fn read(set: &VectorSet, root: &Root) -> Result<Vec<Read>, Refused> {
    let Some(listed) = set.mutants().filter(|listed| !listed.is_empty()) else {
        return Ok(Vec::new());
    };
    let refuse = |index: usize, path: &str, (code, why): (&'static str, String)| Refused {
        at: format!("mutants[{index}]"),
        code,
        detail: format!("mutant `{path}`: {why}"),
    };
    let total_bound = VerifyLimits::MAX.max_mutant_bytes;
    let mut total: u64 = 0;
    let mut seen: Vec<(Identity, usize)> = Vec::with_capacity(listed.len());
    let mut read = Vec::with_capacity(listed.len());
    for (index, mutant) in listed.iter().enumerate() {
        let path = mutant.path();
        let opened = root
            .open(path, codes::MUTANT_OUTSIDE_DIRECTORY, "the vector file's")
            .map_err(|refusal| refuse(index, path, refusal))?;
        let (file, metadata, this) = (opened.file, opened.metadata, opened.identity);
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
        seen.push((this.to_owned(), index));
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
        // metadata was read is read that far and refused.
        let room = per_file.min(total_bound - total);
        let bound = usize::try_from(room).unwrap_or(usize::MAX);
        let bytes = read_bounded(file, bound).map_err(|refusal| refuse(index, path, refusal))?;
        total = total.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        read.push(Read {
            path: path.to_owned(),
            bytes,
            identity: this,
        });
    }
    Ok(read)
}
