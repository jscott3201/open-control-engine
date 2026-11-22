//! The one way `oce` writes a file: `fmt --write`, `emit -o`, `import -o`, `verify --out` and
//! every file of `verify --commission --out` (issues #758 and #771; ruled by the owner on
//! 2026-11-22).
//!
//! **The target** ([`target_accepted`]) must be absent or a regular file. A symbolic link,
//! dangling or not, a directory, a named pipe, a socket or a device is `output-not-a-file`, and
//! every verb asks before it reads anything, so a refusal reads and writes nothing. The target's
//! own entry is never written through: a link planted there is refused, or, planted after the
//! check, replaced. A caller whose output is a link gives the path of the file it names; a name
//! that is `oce`'s own (`DIR/receipt.json`) is removed or replaced by its owner.
//!
//! **The directory** the file goes into is opened once ([`super::directory::Directory`]), and
//! every step of the write is relative to that handle (Unix; issue #771): for a path the caller
//! named ([`write_atomically`]), its parent, the caller's links on the way followed (they named
//! them); for `verify --commission --out`, `DIR` and the directories made below it
//! (`commission_out`). A directory swapped for a link, or exchanged, after it was opened cannot
//! redirect anything: the temporary file, the rename and the clean-up all happen in the directory
//! the handle holds.
//!
//! **The write** ([`write_in`]): the bytes go to a uniquely named temporary file in that
//! directory, created with `openat(O_CREAT | O_EXCL | O_NOFOLLOW)` (it never follows a link and
//! never truncates a file already there), are synced, and are renamed with `renameat` over the
//! target's own entry, never over a file a link there names. So a link planted between the check
//! and the write is replaced, its target untouched. On Unix the temporary file is created with
//! the mode bits of the file it replaces (`0600` when there is none, so a new file is owner-only;
//! for `fmt --write`, the mode of the file it read, from that handle) and the umask can only
//! narrow that; those exact permissions are then set through the handle before the rename. The
//! file is replaced, not rewritten in place, so a hard link to the old inode keeps the old bytes.
//! On a failure the target is untouched and the temporary file is removed with `unlinkat` from
//! the same handle, so none is left behind even when the directory's path was swapped meanwhile.
//! Two more conditions hold a write when its caller asks: every directory `oce` made for it must
//! be owned by the owner of the temporary file (a directory another user swapped in is refused
//! before a byte is written), and the entry must still be the file that was read
//! (`fmt --write`: device and inode, checked just before the rename; a swap in the instant
//! between that check and the rename replaces whatever was swapped in, never writes through it).
//!
//! **Windows** keeps the path-based write, untested: the parent is reached by path at each step.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, ErrorKind, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use super::directory::{Directory, Entry, Kind, split};
use super::steps::step;
use crate::codes;

/// The sentence every `output-not-a-file` refusal ends with.
const ONLY_FILES: &str =
    "oce writes only to a path that is absent or a regular file; nothing is read or written";

/// Who named the target, which decides the way out of a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Named {
    /// The caller named the path (`-o`, `fmt --write`'s file): a link there is left for the path
    /// of the file it names.
    Caller,
    /// The name is `oce`'s own inside the caller's directory (`DIR/receipt.json`): whatever is
    /// there is removed or replaced by its owner.
    Tool,
}

/// Whether `path` may be written: absent, or a regular file by its own entry, read without
/// following a link (module header). `shown` is the path as the report names it, and `named`
/// who named it. An entry that cannot be read (a parent that is a file or cannot be searched) is
/// left to the write, which then fails as `write-failed`.
///
/// # Errors
/// `output-not-a-file`, naming `shown`, what it is (a symbolic link, a directory, or another
/// non-file) and the way out.
pub(crate) fn target_accepted(
    path: &Path,
    shown: &str,
    named: Named,
) -> Result<(), (&'static str, String)> {
    let Ok(entry) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    let kind = entry.file_type();
    let kind = if kind.is_file() {
        return Ok(());
    } else if kind.is_symlink() {
        Kind::Link
    } else if kind.is_dir() {
        Kind::Directory
    } else {
        Kind::Other
    };
    Err(not_a_target(kind, shown, named))
}

/// The `output-not-a-file` refusal of a target that is `kind` (anything but a regular file).
pub(crate) fn not_a_target(kind: Kind, shown: &str, named: Named) -> (&'static str, String) {
    let what = match kind {
        Kind::Link => "a symbolic link, and oce never writes through one",
        Kind::Directory => "a directory",
        Kind::File | Kind::Other => "a named pipe, a socket or a device, not a regular file",
    };
    let way_out = match named {
        Named::Caller if kind == Kind::Link => ": give the path of the file it names",
        Named::Caller => "",
        Named::Tool => ": remove or replace that entry",
    };
    (
        codes::OUTPUT_NOT_A_FILE,
        format!("`{shown}` is {what}{way_out}; {ONLY_FILES}"),
    )
}

/// How many temporary names one write tries before it gives up with `write-failed`.
pub(crate) const NAME_ATTEMPTS: u64 = 16;

/// The longest prefix of the target's file name a temporary name carries, in bytes (cut at a
/// character boundary): with the leading `.`, the pid, the counter and `.oce-tmp` the temporary
/// name stays under the common 255-byte file-name limit when the target's name is long.
const NAME_PREFIX_BYTES: usize = 200;

/// The mode a new file (no target yet) is created with on Unix: owner read and write only.
#[cfg(unix)]
pub(crate) const NEW_FILE_MODE: u32 = 0o600;

/// The per-process counter that makes each temporary name unique within this process.
static NEXT_NAME: AtomicU64 = AtomicU64::new(0);

/// Whose permissions the new file takes (module header).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lend {
    /// The regular file at the name when the write starts, read from the handle without
    /// following a link; a new file (`0600` on Unix) when there is none.
    Entry,
    /// The file that was read (`fmt --write`): its mode bits from its handle.
    Read(Option<u32>),
}

/// What a write must hold to beyond the bytes (module header).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Plan<'p> {
    /// Whose permissions the new file takes.
    pub(crate) lend: Lend,
    /// The file that was read, which must still be the entry at the name before the rename
    /// (device and inode, Unix).
    pub(crate) unchanged: Option<Entry>,
    /// The directories `oce` made for this write, each of which must be owned by the owner of
    /// the temporary file (Unix).
    pub(crate) made: &'p [&'p Directory],
}

impl Plan<'_> {
    /// A plain write: the permissions of the file replaced, no other condition.
    pub(crate) const PLAIN: Plan<'static> = Plan {
        lend: Lend::Entry,
        unchanged: None,
        made: &[],
    };
}

/// Why a write failed after its temporary file was created.
enum Failure {
    /// The operating system's failure.
    Io(io::Error),
    /// A directory `oce` made is owned by someone else: it was swapped after it was made.
    NotMade,
    /// The entry is no longer the file that was read.
    Changed,
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Write `bytes` to `path` atomically, **never following a symbolic link at `path`** (module
/// header): its parent directory is opened once, following the caller's links, and the write is
/// [`write_in`] that directory with [`Plan::PLAIN`]. The caller has asked [`target_accepted`]
/// first; this is the backstop for an entry that changed since. The error is `write-failed` and a
/// detail that names the failure by kind, never by an operating-system message that could carry
/// a path.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), (&'static str, String)> {
    let Some((parent, name)) = split(path) else {
        return Err(failed(ErrorKind::InvalidInput.into()));
    };
    let directory = Directory::open(parent).map_err(failed)?;
    write_in(&directory, &name, bytes, &Plan::PLAIN)
}

/// Write `bytes` to `name` in `directory`, every step relative to that handle (module header),
/// under `plan`.
///
/// # Errors
/// `write-failed`: the target is untouched and no temporary file is left.
pub(crate) fn write_in(
    directory: &Directory,
    name: &OsStr,
    bytes: &[u8],
    plan: &Plan<'_>,
) -> Result<(), (&'static str, String)> {
    step();
    let mode = match plan.lend {
        Lend::Entry => directory
            .entry(name, false)
            .ok()
            .filter(|entry| entry.kind == Kind::File)
            .and_then(|entry| entry.mode),
        Lend::Read(mode) => mode,
    };
    let (temporary, mut file) = create_temporary(directory, name, mode)?;
    let written = fill(&mut file, bytes, mode, plan.made);
    drop(file);
    let result = written.and_then(|()| {
        if let Some(read) = plan.unchanged {
            step();
            let now = directory.entry(name, false)?;
            if now.kind != Kind::File || now.identity != read.identity {
                return Err(Failure::Changed);
            }
        }
        step();
        rename(directory, &temporary, name).map_err(Failure::Io)
    });
    result.map_err(|failure| {
        remove(directory, &temporary);
        match failure {
            Failure::Io(error) => failed(error),
            Failure::NotMade => (
                codes::WRITE_FAILED,
                "the output could not be written: a directory oce made was replaced by one it \
                 does not own; nothing is written there"
                    .to_owned(),
            ),
            Failure::Changed => (
                codes::WRITE_FAILED,
                "the output could not be written: the file changed after it was read; nothing \
                 is written"
                    .to_owned(),
            ),
        }
    })
}

/// Check the directories `made` against the temporary file's owner, then write, set the
/// permissions and sync (module header).
fn fill(
    file: &mut File,
    bytes: &[u8],
    mode: Option<u32>,
    made: &[&Directory],
) -> Result<(), Failure> {
    if !made.is_empty() {
        let mine = owner(file)?;
        for directory in made {
            if directory.owner()? != mine {
                return Err(Failure::NotMade);
            }
        }
    }
    step();
    file.write_all(bytes)?;
    set_mode(file, mode)?;
    file.sync_all()?;
    Ok(())
}

/// Create a fresh temporary file beside `name` with `O_EXCL`, trying [`NAME_ATTEMPTS`] unique
/// names; a name that already exists (a file, a directory or a planted link) is skipped, never
/// opened. On Unix the file is created no more permissive than `mode`, or `0600` when there is
/// none (module header).
fn create_temporary(
    directory: &Directory,
    name: &OsStr,
    mode: Option<u32>,
) -> Result<(OsString, File), (&'static str, String)> {
    for _ in 0..NAME_ATTEMPTS {
        let temporary = temporary_name(name, NEXT_NAME.fetch_add(1, Ordering::Relaxed));
        step();
        match create(directory, &temporary, mode) {
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

/// `.name.<pid>.<counter>.oce-tmp`, the temporary name beside `name`.
fn temporary_name(name: &OsStr, counter: u64) -> OsString {
    let full = Path::new(name)
        .file_name()
        .map_or_else(|| "output".into(), OsStr::to_string_lossy);
    let mut cut = full.len().min(NAME_PREFIX_BYTES);
    while !full.is_char_boundary(cut) {
        cut -= 1;
    }
    let prefix = &full[..cut];
    let pid = std::process::id();
    format!(".{prefix}.{pid}.{counter}.oce-tmp").into()
}

/// `dir/.name.<pid>.<counter>.oce-tmp` beside `target` (for the tests, which plant files there).
#[cfg(test)]
pub(crate) fn temporary_path(target: &Path, counter: u64) -> std::path::PathBuf {
    let name = target.file_name().unwrap_or_else(|| OsStr::new("output"));
    target.with_file_name(temporary_name(name, counter))
}

/// The counter value the next temporary name will use (for the unit tests).
#[cfg(test)]
pub(crate) fn peek_counter() -> u64 {
    NEXT_NAME.load(Ordering::Relaxed)
}

/// Create `temporary` in `directory`: `O_CREAT | O_EXCL | O_NOFOLLOW`, mode `mode & 0o777` or
/// [`NEW_FILE_MODE`] (Unix).
#[cfg(unix)]
fn create(directory: &Directory, temporary: &OsStr, mode: Option<u32>) -> io::Result<File> {
    use rustix::fs::{Mode, OFlags, openat};
    let bits = mode.map_or(NEW_FILE_MODE, |mode| mode & 0o777);
    let flags = OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    Ok(File::from(openat(
        directory.fd(),
        temporary,
        flags,
        Mode::from_raw_mode(raw_mode(bits)),
    )?))
}

/// Permission bits as the platform's raw mode type (narrower on macOS).
#[cfg(unix)]
#[allow(
    clippy::unnecessary_cast,
    clippy::cast_possible_truncation,
    reason = "permission bits fit every platform's mode_t"
)]
fn raw_mode(bits: u32) -> rustix::fs::RawMode {
    (bits & 0o7777) as rustix::fs::RawMode
}

#[cfg(not(unix))]
fn create(directory: &Directory, temporary: &OsStr, _mode: Option<u32>) -> io::Result<File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(temporary))
}

/// The owner of the temporary file: the user this write runs as (Unix); `None` elsewhere.
fn owner(file: &File) -> io::Result<Option<u32>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(Some(file.metadata()?.uid()))
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Ok(None)
    }
}

/// Give the temporary file `mode` (Unix mode bits) through its handle, so a `0600` or `0640` file
/// keeps its mode. Elsewhere this does nothing: the only permission bit is read-only, and a
/// read-only target refuses the rename regardless.
fn set_mode(file: &File, mode: Option<u32>) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match mode {
            Some(mode) => file.set_permissions(std::fs::Permissions::from_mode(mode & 0o7777)),
            None => Ok(()),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (file, mode);
        Ok(())
    }
}

/// `renameat` from the handle to itself (Unix); by path elsewhere.
fn rename(directory: &Directory, temporary: &OsStr, name: &OsStr) -> io::Result<()> {
    #[cfg(unix)]
    {
        Ok(rustix::fs::renameat(
            directory.fd(),
            temporary,
            directory.fd(),
            name,
        )?)
    }
    #[cfg(not(unix))]
    {
        std::fs::rename(directory.join(temporary), directory.join(name))
    }
}

/// Remove the temporary file after a failure: `unlinkat` from the handle (Unix), by path
/// elsewhere. A failure to remove it changes nothing the caller can act on.
fn remove(directory: &Directory, temporary: &OsStr) {
    #[cfg(unix)]
    {
        let _ = rustix::fs::unlinkat(directory.fd(), temporary, rustix::fs::AtFlags::empty());
    }
    #[cfg(not(unix))]
    {
        let _ = std::fs::remove_file(directory.join(temporary));
    }
}

fn failed(error: io::Error) -> (&'static str, String) {
    (
        codes::WRITE_FAILED,
        format!("the output could not be written: {}", error.kind()),
    )
}

#[cfg(test)]
#[path = "atomic_write_tests.rs"]
pub(crate) mod tests;
