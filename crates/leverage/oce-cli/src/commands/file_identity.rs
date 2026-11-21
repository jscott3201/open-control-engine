//! What makes two paths one file, for the mutant reader's duplicate check and for the guard that
//! keeps `emit -o` and `import -o` from replacing their own input.
//!
//! On Unix one file is one (device, inode), read from the metadata of the resolved path. So two
//! spellings of a path, a symbolic link and its target, a hard link, and a case or normalization
//! twin where the file system folds them are all one file. Elsewhere one file is one canonical
//! path, the final path Windows gives, which folds case and trailing dots and spaces and
//! resolves links; std has no stable file index there, so **a hard link is not recognised on
//! Windows**.

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

/// Whether `first` and `second` name one existing file (module header), following every
/// symbolic link. A path that does not resolve (missing, or a dangling link) is no file, so the
/// answer is no: a write to it creates a new file and replaces nothing.
pub(crate) fn same_file(first: &Path, second: &Path) -> bool {
    let of = |path: &Path| {
        let canonical = std::fs::canonicalize(path).ok()?;
        let metadata = std::fs::metadata(&canonical).ok()?;
        Some(identity(&canonical, &metadata))
    };
    matches!((of(first), of(second)), (Some(first), Some(second)) if first == second)
}
