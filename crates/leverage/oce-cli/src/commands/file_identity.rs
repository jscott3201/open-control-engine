//! What makes two paths one file, for the mutant reader's duplicate check and for the guard that
//! keeps `emit -o`, `import -o` and both forms of `verify --out` from replacing their own input
//! (`output-is-input`).
//!
//! On Unix one file is one (device, inode), read from the metadata of the resolved path. So two
//! spellings of a path, a symbolic link and its target, a hard link, and a case or normalization
//! twin where the file system folds them are all one file. Elsewhere one file is one canonical
//! path, the final path Windows gives, which folds case and trailing dots and spaces and
//! resolves links; std has no stable file index there, so **a hard link is not recognised on
//! Windows**.
//!
//! Both forms of `verify --out` compare an output with the identity of each file the run read:
//! the document and each file it names **as they were read** (from the handle each was read
//! through, on Unix; issue #771), a vector subject by its path. `--commission` takes its output's
//! identity, `DIR/receipt.json`, from `DIR`'s handle; the vector form resolves `DIR/receipt.json`
//! by path, as it writes a single file whose parent is opened only at the write.

use std::fs::Metadata;
use std::path::Path;

/// What makes two listings one file: the device and inode of the file.
#[cfg(unix)]
pub(crate) type Identity = (u64, u64);

/// What makes two listings one file: the canonical path, the final path Windows gives.
#[cfg(not(unix))]
pub(crate) type Identity = std::path::PathBuf;

/// The identity of the file at `canonical`, whose metadata is `metadata`.
#[cfg(unix)]
pub(crate) fn identity(_canonical: &Path, metadata: &Metadata) -> Identity {
    use std::os::unix::fs::MetadataExt;
    (metadata.dev(), metadata.ino())
}

/// The identity of the file at `canonical`, whose metadata is `metadata`.
#[cfg(not(unix))]
pub(crate) fn identity(canonical: &Path, _metadata: &Metadata) -> Identity {
    canonical.to_path_buf()
}

/// The identity of the existing file at `path` (module header), following every symbolic link;
/// `None` for a path that does not resolve (missing, or a dangling link).
pub(crate) fn identity_of(path: &Path) -> Option<Identity> {
    let canonical = std::fs::canonicalize(path).ok()?;
    let metadata = std::fs::metadata(&canonical).ok()?;
    Some(identity(&canonical, &metadata))
}

/// Whether `first` and `second` name one existing file (module header), following every
/// symbolic link. A path that does not resolve (missing, or a dangling link) is no file, so the
/// answer is no: a write to it creates a new file and replaces nothing.
pub(crate) fn same_file(first: &Path, second: &Path) -> bool {
    matches!((identity_of(first), identity_of(second)), (Some(first), Some(second)) if first == second)
}

/// How the report names the first of `inputs` (each its name as reported and the identity of
/// the file read) that is the file `output` identifies; `None` when none is, or when `output`
/// names no existing file.
pub(crate) fn input_at<'i>(
    output: Option<&Identity>,
    inputs: &'i [(String, Identity)],
) -> Option<&'i str> {
    let output = output?;
    inputs
        .iter()
        .find(|(_, read)| read == output)
        .map(|(name, _)| name.as_str())
}

/// Whether standard input is a regular file that is the file at `output`, so that writing
/// `output` replaces what the input is read from (`oce import - -o doc < doc`).
///
/// Unix only: the answer is the identity of file descriptor 0 against the identity of `output`
/// (module header). Not a regular file (a pipe, a terminal, a closed descriptor) or an `output`
/// that does not resolve is no. On other platforms the answer is always no: standard input is
/// not checked there.
#[cfg(unix)]
pub(crate) fn stdin_is_file(output: &Path) -> bool {
    use std::os::fd::AsFd;
    let Ok(duplicate) = std::io::stdin().as_fd().try_clone_to_owned() else {
        return false;
    };
    let Ok(metadata) = std::fs::File::from(duplicate).metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    let Ok(canonical) = std::fs::canonicalize(output) else {
        return false;
    };
    std::fs::metadata(&canonical)
        .is_ok_and(|target| identity(&canonical, &target) == identity(&canonical, &metadata))
}

/// Whether standard input is the file at `output`: never, off Unix (module header).
#[cfg(not(unix))]
pub(crate) fn stdin_is_file(_output: &Path) -> bool {
    false
}
