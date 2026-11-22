//! A directory opened once, and every read and write `oce` makes below it relative to that handle
//! (issue #771): the document's directory for a vector set's or a spec's reads
//! (`contained.rs`), the parent of every file a verb writes (`atomic_write.rs`), and
//! `verify --commission --out`'s `DIR`, `DIR/outputs` and `DIR/outputs/<k>`
//! (`commission_out.rs`).
//!
//! **Unix.** [`Directory::open`] opens a path with `openat(O_DIRECTORY)`, following the links on
//! the caller's path: the caller named it (owner ruling of 2026-11-22). From then on nothing below
//! it is reached by path. An entry is judged with `statat` from the handle ([`Directory::entry`]);
//! a file is read with `openat` from the handle ([`Directory::read`]): its entry is checked first,
//! so a named pipe, a socket or a device is refused and never opened, then it is opened with
//! `O_NONBLOCK` (so a pipe swapped in after the check is opened at once and refused by its kind),
//! the handle must be a regular file with the device and inode that were checked, and
//! `O_NONBLOCK` is cleared before the caller reads. `follow` decides whether a link at the entry
//! itself is followed (a document the caller named) or refused (a file a document names, a file
//! `fmt --write` rewrites). [`Directory::make`] creates a directory with `mkdirat` and opens it
//! with `openat(O_DIRECTORY | O_NOFOLLOW)`, so a link or a non-directory swapped in after the
//! create is refused, and requires the directory opened to be empty. `mkdirat` returns no handle,
//! so what is opened cannot be proved to be what was made; the writer also requires every
//! directory it made to be owned by the user who owns the temporary file it creates inside
//! (`atomic_write`), which refuses a directory another user swapped in. A process running as the
//! same user is not stopped by that, and needs no swap: it can rewrite the outputs anyway.
//!
//! The handles are opened `O_RDONLY`: rustix offers no `O_SEARCH` on macOS (nor `O_PATH`
//! outside Linux), so a directory that can be searched but not listed (mode `--x` or `-wx`) is
//! refused where a path-based call used to pass through it (issue #771, breaking in place).
//!
//! **Windows** has no `openat` in std: a [`Directory`] is its path, and every call joins a name
//! onto it. An entry swapped for a link, or a directory swapped, between two calls is followed
//! there; nothing is tested there.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::path::Path;
#[cfg(not(unix))]
use std::path::PathBuf;

use super::steps::step;

/// What a directory entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A regular file.
    File,
    /// A symbolic link (only ever seen when links are not followed).
    Link,
    /// A directory.
    Directory,
    /// A named pipe, a socket or a device.
    Other,
}

/// One directory entry's metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    /// What it is.
    pub(crate) kind: Kind,
    /// Its size in bytes.
    pub(crate) len: u64,
    /// Its permission bits (`& 0o7777`) on Unix; `None` elsewhere.
    pub(crate) mode: Option<u32>,
    /// Its device and inode on Unix; `None` elsewhere.
    pub(crate) identity: Option<(u64, u64)>,
}

/// Why [`Directory::read`] refused an entry.
#[derive(Debug)]
pub(crate) enum ReadError {
    /// The operating system's failure.
    Io(io::Error),
    /// A symbolic link where none is followed.
    Link,
    /// Something other than a regular file.
    NotFile(Kind),
    /// The entry changed between its check and its open (Unix: no identity elsewhere).
    #[cfg_attr(
        not(unix),
        allow(dead_code, reason = "no identity to compare off Unix")
    )]
    Changed,
}

/// Why [`Directory::make`] refused.
#[derive(Debug)]
pub(crate) enum MakeError {
    /// Something already has the name.
    Exists,
    /// The operating system's failure.
    Io(io::Error),
    /// What was opened under the name is not an empty directory: it changed after it was made
    /// (Unix: the directory is reached by path elsewhere).
    #[cfg_attr(not(unix), allow(dead_code, reason = "no handle to check off Unix"))]
    NotFresh,
}

/// A directory opened once (module header).
#[derive(Debug)]
pub(crate) struct Directory {
    /// The handle.
    #[cfg(unix)]
    fd: std::os::fd::OwnedFd,
    /// The path (Windows: no handle).
    #[cfg(not(unix))]
    path: PathBuf,
}

/// The last component of `path` as a name to open from its parent, and the parent (`.` for a
/// bare name). A path whose bytes end in `/` or `/.` keeps one `/`, so the system still requires
/// a directory there, as it does for the path. `None` for a path with no last name (`/`, `..`).
pub(crate) fn split(path: &Path) -> Option<(&Path, OsString)> {
    let name = path.file_name()?.to_os_string();
    #[cfg(unix)]
    let name = {
        use std::os::unix::ffi::OsStrExt;
        let bytes = path.as_os_str().as_bytes();
        let mut name = name;
        if bytes.ends_with(b"/") || bytes.ends_with(b"/.") {
            name.push("/");
        }
        name
    };
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    Some((parent, name))
}

#[cfg(unix)]
mod handle {
    use std::ffi::OsStr;
    use std::fs::File;
    use std::io;
    use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

    use rustix::fs::{AtFlags, CWD, FileType, Mode, OFlags, Stat, fstat, openat, statat};
    use rustix::io::Errno;

    use super::{Entry, Kind, ReadError, step};

    /// The flags of every directory handle: read-only (module header), never inherited.
    pub(super) const DIRECTORY: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::CLOEXEC);

    /// An entry's metadata from its `stat`.
    #[allow(
        clippy::unnecessary_cast,
        reason = "st_dev and st_mode are narrower than u64 and u32 on macOS"
    )]
    pub(crate) fn entry_of(stat: &Stat) -> Entry {
        let kind = match FileType::from_raw_mode(stat.st_mode) {
            FileType::RegularFile => Kind::File,
            FileType::Symlink => Kind::Link,
            FileType::Directory => Kind::Directory,
            _ => Kind::Other,
        };
        Entry {
            kind,
            len: u64::try_from(stat.st_size).unwrap_or(0),
            mode: Some((stat.st_mode as u32) & 0o7777),
            // The widening std's `MetadataExt::dev` makes, so the two agree.
            identity: Some((stat.st_dev as u64, stat.st_ino as u64)),
        }
    }

    /// `statat` from `dir`, following a link at `name` or not.
    pub(crate) fn entry_at(dir: BorrowedFd<'_>, name: &OsStr, follow: bool) -> io::Result<Entry> {
        let flags = if follow {
            AtFlags::empty()
        } else {
            AtFlags::SYMLINK_NOFOLLOW
        };
        Ok(entry_of(&statat(dir, name, flags)?))
    }

    /// Open `name` below `dir` for reading (module header): checked, opened with `O_NONBLOCK`
    /// (and `O_NOFOLLOW` unless `follow`), held to the identity checked, then made blocking.
    pub(crate) fn read_at(
        dir: BorrowedFd<'_>,
        name: &OsStr,
        follow: bool,
    ) -> Result<(File, Entry), ReadError> {
        step();
        let checked = entry_at(dir, name, follow).map_err(ReadError::Io)?;
        match checked.kind {
            Kind::File => {}
            Kind::Link => return Err(ReadError::Link),
            kind => return Err(ReadError::NotFile(kind)),
        }
        let mut flags = OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC;
        if !follow {
            flags |= OFlags::NOFOLLOW;
        }
        step();
        let handle = openat(dir, name, flags, Mode::empty()).map_err(|error| {
            if error == Errno::LOOP && !follow {
                ReadError::Link
            } else {
                ReadError::Io(error.into())
            }
        })?;
        let opened = entry_of(&fstat(&handle).map_err(|error| ReadError::Io(error.into()))?);
        if opened.kind != Kind::File {
            return Err(ReadError::NotFile(opened.kind));
        }
        // The handle must be the entry that was checked: anything else is the entry changing
        // between the check and the open, and is refused, never read.
        if opened.identity != checked.identity {
            return Err(ReadError::Changed);
        }
        let flags =
            rustix::fs::fcntl_getfl(&handle).map_err(|error| ReadError::Io(error.into()))?;
        rustix::fs::fcntl_setfl(&handle, flags - OFlags::NONBLOCK)
            .map_err(|error| ReadError::Io(error.into()))?;
        Ok((File::from(handle), opened))
    }

    /// Open the directory at `path`, following the links on it.
    pub(crate) fn open_path(path: &std::path::Path) -> io::Result<OwnedFd> {
        Ok(openat(CWD, path, DIRECTORY, Mode::empty())?)
    }

    /// Whether the directory `fd` names holds nothing but `.` and `..`.
    pub(crate) fn is_empty(fd: &OwnedFd) -> io::Result<bool> {
        let mut entries = rustix::fs::Dir::read_from(fd.as_fd())?;
        while let Some(entry) = entries.read() {
            let name = entry?.file_name().to_bytes().to_vec();
            if name != b"." && name != b".." {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[cfg(unix)]
pub(crate) use handle::{entry_of, read_at};

impl Directory {
    /// Open the directory at `path`, following the links on it: the caller named it.
    ///
    /// # Errors
    /// The system's: `NotFound`, `NotADirectory`, `PermissionDenied` (a directory that cannot be
    /// listed, module header), ...
    pub(crate) fn open(path: &Path) -> io::Result<Self> {
        #[cfg(unix)]
        {
            Ok(Self {
                fd: handle::open_path(path)?,
            })
        }
        #[cfg(not(unix))]
        {
            if std::fs::metadata(path)?.is_dir() {
                Ok(Self {
                    path: path.to_path_buf(),
                })
            } else {
                Err(io::ErrorKind::NotADirectory.into())
            }
        }
    }

    /// The handle, for a walk below it (Unix).
    #[cfg(unix)]
    pub(crate) fn fd(&self) -> std::os::fd::BorrowedFd<'_> {
        use std::os::fd::AsFd;
        self.fd.as_fd()
    }

    /// The handle's own metadata.
    ///
    /// # Errors
    /// The system's.
    #[cfg_attr(
        not(unix),
        allow(dead_code, reason = "the identity check is Unix only")
    )]
    pub(crate) fn metadata(&self) -> io::Result<Entry> {
        #[cfg(unix)]
        {
            Ok(entry_of(&rustix::fs::fstat(&self.fd)?))
        }
        #[cfg(not(unix))]
        {
            Ok(entry_of_metadata(&std::fs::metadata(&self.path)?))
        }
    }

    /// The owner of the directory itself (Unix); `None` elsewhere.
    ///
    /// # Errors
    /// The system's.
    pub(crate) fn owner(&self) -> io::Result<Option<u32>> {
        #[cfg(unix)]
        {
            Ok(Some(rustix::fs::fstat(&self.fd)?.st_uid))
        }
        #[cfg(not(unix))]
        {
            Ok(None)
        }
    }

    /// The metadata of `name` below this directory, following a link there or not.
    ///
    /// # Errors
    /// The system's (`NotFound` for an absent name).
    pub(crate) fn entry(&self, name: &OsStr, follow: bool) -> io::Result<Entry> {
        #[cfg(unix)]
        {
            handle::entry_at(self.fd(), name, follow)
        }
        #[cfg(not(unix))]
        {
            let path = self.path.join(name);
            let metadata = if follow {
                std::fs::metadata(path)?
            } else {
                std::fs::symlink_metadata(path)?
            };
            Ok(entry_of_metadata(&metadata))
        }
    }

    /// Open `name` below this directory for reading (module header). The entry is the handle's.
    ///
    /// # Errors
    /// [`ReadError`].
    pub(crate) fn read(&self, name: &OsStr, follow: bool) -> Result<(File, Entry), ReadError> {
        #[cfg(unix)]
        {
            handle::read_at(self.fd(), name, follow)
        }
        #[cfg(not(unix))]
        {
            step();
            let checked = self.entry(name, follow).map_err(ReadError::Io)?;
            match checked.kind {
                Kind::File => {}
                Kind::Link => return Err(ReadError::Link),
                kind => return Err(ReadError::NotFile(kind)),
            }
            step();
            let file = File::open(self.path.join(name)).map_err(ReadError::Io)?;
            let opened = entry_of_metadata(&file.metadata().map_err(ReadError::Io)?);
            if opened.kind != Kind::File {
                return Err(ReadError::NotFile(opened.kind));
            }
            Ok((file, opened))
        }
    }

    /// Create the directory `name` below this one and open it (module header).
    ///
    /// # Errors
    /// [`MakeError`].
    pub(crate) fn make(&self, name: &OsStr) -> Result<Self, MakeError> {
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, OFlags, mkdirat, openat};
            use rustix::io::Errno;
            mkdirat(&self.fd, name, Mode::from_raw_mode(0o777)).map_err(|error| {
                if error == Errno::EXIST {
                    MakeError::Exists
                } else {
                    MakeError::Io(error.into())
                }
            })?;
            step();
            let fd = openat(
                &self.fd,
                name,
                handle::DIRECTORY | OFlags::NOFOLLOW,
                Mode::empty(),
            )
            .map_err(|error| {
                if [Errno::LOOP, Errno::NOTDIR, Errno::NOENT].contains(&error) {
                    MakeError::NotFresh
                } else {
                    MakeError::Io(error.into())
                }
            })?;
            if !handle::is_empty(&fd).map_err(MakeError::Io)? {
                return Err(MakeError::NotFresh);
            }
            Ok(Self { fd })
        }
        #[cfg(not(unix))]
        {
            let path = self.path.join(name);
            std::fs::create_dir(&path).map_err(|error| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    MakeError::Exists
                } else {
                    MakeError::Io(error)
                }
            })?;
            Ok(Self { path })
        }
    }

    /// The path of `name` below this directory (Windows: the writer works by path there).
    #[cfg(not(unix))]
    pub(crate) fn join(&self, name: &OsStr) -> PathBuf {
        self.path.join(name)
    }
}

/// An entry's metadata from std's (Windows).
#[cfg(not(unix))]
fn entry_of_metadata(metadata: &std::fs::Metadata) -> Entry {
    let kind = metadata.file_type();
    Entry {
        kind: if kind.is_symlink() {
            Kind::Link
        } else if kind.is_file() {
            Kind::File
        } else if kind.is_dir() {
            Kind::Directory
        } else {
            Kind::Other
        },
        len: metadata.len(),
        mode: None,
        identity: None,
    }
}
