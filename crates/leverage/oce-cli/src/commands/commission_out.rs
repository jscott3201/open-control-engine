//! What `oce verify --commission --out DIR` writes (commissioning note, "CLI" "Output" and
//! decision 37; issue #664):
//!
//! - `DIR/outputs/<k>/errors.csv` for every compared output, `<k>` its zero-based index in the
//!   receipt's `outputs[]`, in decimal with no padding: the output's exceedance at each test
//!   instant (`oce_verify::OutputCurves::errors`);
//! - under `--bounds`, beside it, `reference.csv`, `test.csv`, `lowerBound.csv` and
//!   `upperBound.csv` for every output whose envelope was built (a Real output with an
//!   envelope; the receipt's `half_width` is present exactly for those);
//! - `DIR/receipt.json`, last, so a `receipt.json` this run wrote means its curves are complete.
//!
//! The report on stdout lists, under `curves`, every compared output with its index, its
//! canonical path and each file written for it ([`Written`]), so a reader never has to infer
//! anything, a pass least of all, from a file that is not there.
//!
//! Each CSV file is pyfunnel's shape (`Curve::to_csv`): the line `x,y`, then one line per point,
//! both numbers through the crate's `%.16g` writer (`oce_verify::format_g16`). So an instant
//! where the engine computed nothing (uncovered) is `nan`, and an instant of an output failed by
//! a non-finite engine sample is `inf`, as C's `printf` spells them; the library writes the
//! constant `f64::NAN`, never a NaN with its sign set. A CSV file holds numbers under a fixed
//! header and no name, and a file is named by the output's index, never by its path: nothing a
//! spec, a mapping or a CXF document writes can reach a file name or a cell.
//!
//! **Before anything is read** ([`target()`]): a `DIR/receipt.json` that is a symbolic link,
//! dangling or not, a directory or another non-file is `output-not-a-file`, the rule of every
//! verb that writes (issue #758, ruled by the owner on 2026-11-22). **Before the run**
//! ([`check()`]): `DIR` must be an existing directory (it is never created); `DIR/outputs` must
//! not exist (`write-failed`), so the curves of two runs are never mixed; a `DIR/receipt.json`
//! that is the spec or any file the run read is `output-is-input`; and a regular file at
//! `DIR/receipt.json` is `write-failed` (issue #664's review: a stale receipt), as is anything
//! that appeared there since the first check. So `DIR` holds neither name when the run starts, a
//! `receipt.json` beside the curves is always this run's, and a refusal before the first write
//! leaves nothing behind. **The writes** ([`write()`]): `outputs/` is made with a plain create,
//! which fails on anything already there, a directory a racing writer made included
//! (`write-failed`); each file goes through `write_atomically` (a fresh temporary file, created
//! owner-only, `0600` on Unix, synced, renamed over the entry), which never follows a link at the
//! entry, so a link planted at `receipt.json` between the check and the write is replaced, its
//! target untouched. `outputs/` and `outputs/<k>/` are made here and then reached again **by
//! path**, so whoever can change `DIR` while the curves are written can swap one for a link and
//! carry them outside `DIR`; writing through directory handles is issue #771. A write that fails
//! part-way is `write-failed` naming the file; the files already written stay, and the refusal
//! lists them as the report's `curves`: without `receipt.json` they are not a run's output. An
//! empty `DIR` ([`empty_dir`]) is refused as a missing one before anything is read.

use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use oce_verify::{Curve, OutputCurves};

use super::atomic_write::{Plan, not_a_target, write_in};
use super::directory::{Directory, Kind, MakeError};
use super::file_identity::Identity;
use super::{Named, input_at, target_accepted};
use crate::{codes, input};

/// The file the receipt is written to, inside `DIR`.
pub(crate) const RECEIPT: &str = "receipt.json";

/// The directory the curves are written under, inside `DIR`.
const OUTPUTS: &str = "outputs";

/// A refusal of the output: the file within `DIR` it concerns, the code and the sentence, and
/// what was written before it.
#[derive(Debug)]
pub(crate) struct Refused {
    /// The file or directory within `DIR` (`outputs/0/errors.csv`), the error's `at`.
    pub(crate) at: String,
    /// `write-failed` or `output-is-input`.
    pub(crate) code: &'static str,
    /// The sentence, naming the file under `DIR` as given.
    pub(crate) detail: String,
    /// The files written before the refusal, per output, as [`write()`] lists them: empty for a
    /// refusal before the first write. An output whose folder was made and none of whose files
    /// was written is not listed.
    pub(crate) written: Vec<Written>,
}

/// `name` under `dir` as the report shows it: `DIR` as given, then `/` and `name` on every
/// platform, so a transcript is one text everywhere (`/name` for the root directory). An empty
/// `DIR`, which [`empty_dir`] refuses, shows `name` alone, the path it would join to.
pub(crate) fn shown(dir: &OsStr, name: &str) -> String {
    let dir = input::display_name(dir);
    if dir.is_empty() {
        return name.to_owned();
    }
    format!("{}/{name}", dir.trim_end_matches('/'))
}

/// Why `check()` refuses a `DIR` that does not exist.
const MISSING_DIR: &str = "the output directory does not exist; it is never created";

/// An empty `DIR` names no directory, and is refused as a missing one by both forms of
/// `verify --out` before anything is read (the vector form would otherwise write
/// `./receipt.json`).
pub(crate) fn empty_dir(dir: &OsStr) -> Option<Refused> {
    dir.is_empty().then(|| failed(dir, ".", MISSING_DIR))
}

/// A `write-failed` refusal about `at` within `DIR` (`.` for `DIR` itself, shown as given).
fn failed(dir: &OsStr, at: &str, why: impl std::fmt::Display) -> Refused {
    let name = if at == "." {
        input::display_name(dir)
    } else {
        shown(dir, at)
    };
    Refused {
        at: at.to_owned(),
        code: codes::WRITE_FAILED,
        detail: format!("`{name}`: {why}"),
        written: Vec::new(),
    }
}

/// The files written for one compared output, as the report lists them (`curves`).
#[derive(Debug)]
pub(crate) struct Written {
    /// The output's index in the receipt's `outputs[]`.
    pub(crate) index: usize,
    /// The output's canonical path.
    pub(crate) output: String,
    /// Each file written for it, `DIR` as given then `/outputs/<k>/<name>`, in the order
    /// written: `errors.csv` always, then the four bound files when its envelope was built and
    /// `--bounds` asked for them.
    pub(crate) files: Vec<String>,
}

/// The check made before anything is read (module header): `DIR/receipt.json` must be absent or
/// a regular file.
///
/// # Errors
/// `output-not-a-file` for a link, dangling or not, a directory or another non-file there.
pub(crate) fn target(dir: &OsStr) -> Result<(), Refused> {
    target_accepted(
        &Path::new(dir).join(RECEIPT),
        &shown(dir, RECEIPT),
        Named::Tool,
    )
    .map_err(|(code, detail)| Refused {
        at: RECEIPT.to_owned(),
        code,
        detail,
        written: Vec::new(),
    })
}

/// `DIR`, opened once by [`check()`]: everything [`write()`] creates is relative to this handle
/// (module header).
#[derive(Debug)]
pub(crate) struct Out<'d> {
    /// `DIR` as given, for the report.
    dir: &'d OsStr,
    /// `DIR`, opened once.
    handle: Directory,
}

/// The checks made before the run (module header), which open `DIR` once. `inputs` pairs each
/// file the run read with how the report names it (the spec as given, every other file by its
/// path as the spec writes it) and the identity of the file as it was read.
///
/// # Errors
/// `write-failed` for a `DIR` that is not an existing directory or cannot be opened, a
/// `DIR/outputs` that exists (a link included) or a regular `DIR/receipt.json`;
/// `output-is-input`, before that last, for a `DIR/receipt.json` that is one of `inputs`;
/// `output-not-a-file` for a non-file that appeared at `DIR/receipt.json` since [`target()`].
pub(crate) fn check<'d>(dir: &'d OsStr, inputs: &[(String, Identity)]) -> Result<Out<'d>, Refused> {
    let root = Path::new(dir);
    match fs::metadata(root) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Err(failed(dir, ".", "the output directory is not a directory")),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(failed(dir, ".", MISSING_DIR));
        }
        Err(error) => return Err(unreadable(dir, &error)),
    }
    // From here on `DIR` is this handle: nothing below it is reached by its path again.
    let handle = match Directory::open(root) {
        Ok(handle) => handle,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(failed(dir, ".", MISSING_DIR));
        }
        Err(error) => return Err(unreadable(dir, &error)),
    };
    match handle.entry(OsStr::new(OUTPUTS), false) {
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Ok(_) => return Err(outputs_exist(dir)),
        Err(error) => {
            return Err(failed(
                dir,
                OUTPUTS,
                format!("it cannot be checked: {}", error.kind()),
            ));
        }
    }
    if let Some(name) = input_at(receipt_identity(dir, &handle).as_ref(), inputs) {
        return Err(Refused {
            at: RECEIPT.to_owned(),
            code: codes::OUTPUT_IS_INPUT,
            detail: format!(
                "`{}` is `{name}`, which the run reads; nothing is written",
                shown(dir, RECEIPT)
            ),
            written: Vec::new(),
        });
    }
    // A non-file that appeared since the first check is refused as that check refuses it; a
    // regular file is a receipt this run did not write, which must never stand beside its curves.
    match handle.entry(OsStr::new(RECEIPT), false) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Out { dir, handle }),
        Ok(entry) if entry.kind != Kind::File => {
            let (code, detail) = not_a_target(entry.kind, &shown(dir, RECEIPT), Named::Tool);
            Err(Refused {
                at: RECEIPT.to_owned(),
                code,
                detail,
                written: Vec::new(),
            })
        }
        Ok(_) => Err(failed(
            dir,
            RECEIPT,
            "it already exists, and a receipt is never written over another; give an empty directory",
        )),
        Err(error) => Err(failed(
            dir,
            RECEIPT,
            format!("it cannot be checked: {}", error.kind()),
        )),
    }
}

/// The identity of the file `DIR/receipt.json` names, following a link there: from the handle on
/// Unix, by its path elsewhere (no handle there).
fn receipt_identity(dir: &OsStr, handle: &Directory) -> Option<Identity> {
    #[cfg(unix)]
    {
        let _ = dir;
        handle
            .entry(OsStr::new(RECEIPT), true)
            .ok()
            .and_then(|entry| entry.identity)
    }
    #[cfg(not(unix))]
    {
        let _ = handle;
        super::file_identity::identity_of(&Path::new(dir).join(RECEIPT))
    }
}

/// The refusal of a `DIR` that exists and cannot be opened.
fn unreadable(dir: &OsStr, error: &std::io::Error) -> Refused {
    failed(
        dir,
        ".",
        format!("the output directory cannot be read: {}", error.kind()),
    )
}

fn outputs_exist(dir: &OsStr) -> Refused {
    failed(
        dir,
        OUTPUTS,
        "it already exists, and the curves of two runs are never mixed; give an empty directory",
    )
}

/// Make the directory `name` (`at` within `DIR`) below `parent` (module header).
fn make(dir: &OsStr, parent: &Directory, name: &str, at: &str) -> Result<Directory, Refused> {
    parent.make(OsStr::new(name)).map_err(|error| match error {
        MakeError::Exists if at == OUTPUTS => outputs_exist(dir),
        MakeError::Exists => failed(dir, at, "it could not be created: entity already exists"),
        MakeError::Io(error) => failed(
            dir,
            at,
            format!("it could not be created: {}", error.kind()),
        ),
        MakeError::NotFresh => failed(
            dir,
            at,
            "it was replaced after it was made by something that is not an empty directory; \
             nothing is written there",
        ),
    })
}

/// Write the curves, then `receipt` (module header), each through the handle of the directory
/// it goes into, and list what was written for each output.
///
/// # Errors
/// `write-failed`, naming the file; the files already written stay.
pub(crate) fn write(
    out: &Out<'_>,
    curves: &[OutputCurves],
    receipt: &[u8],
) -> Result<Vec<Written>, Refused> {
    let dir = out.dir;
    let outputs = make(dir, &out.handle, OUTPUTS, OUTPUTS)?;
    let mut written = Vec::with_capacity(curves.len());
    for (k, output) in curves.iter().enumerate() {
        let folder = format!("{OUTPUTS}/{k}");
        let made = match make(dir, &outputs, &k.to_string(), &folder) {
            Ok(made) => made,
            Err(refused) => return Err(with_written(refused, written)),
        };
        let mut files: Vec<(&str, &Curve)> = vec![("errors.csv", &output.errors)];
        // The library keeps the bound curves only under `--bounds`, and only where an envelope
        // was built (`CommissionOptions::bounds`).
        if let Some(curves) = &output.bounds {
            files.extend([
                ("reference.csv", &curves.reference),
                ("test.csv", &curves.test),
                ("lowerBound.csv", &curves.lower),
                ("upperBound.csv", &curves.upper),
            ]);
        }
        written.push(Written {
            index: k,
            output: output.output.clone(),
            files: Vec::with_capacity(files.len()),
        });
        let mine = [&outputs, &made];
        let plan = Plan {
            made: &mine,
            ..Plan::PLAIN
        };
        for (name, curve) in files {
            let at = format!("{folder}/{name}");
            if let Err((_, why)) =
                write_in(&made, OsStr::new(name), curve.to_csv().as_bytes(), &plan)
            {
                return Err(with_written(failed(dir, &at, why), written));
            }
            if let Some(entry) = written.last_mut() {
                entry.files.push(shown(dir, &at));
            }
        }
    }
    if let Err((_, why)) = write_in(&out.handle, OsStr::new(RECEIPT), receipt, &Plan::PLAIN) {
        return Err(with_written(failed(dir, RECEIPT, why), written));
    }
    Ok(written)
}

/// `refused` with the files written before it, the outputs none of whose files was written left
/// out.
fn with_written(mut refused: Refused, mut written: Vec<Written>) -> Refused {
    written.retain(|output| !output.files.is_empty());
    refused.written = written;
    refused
}

#[cfg(test)]
#[path = "commission_out_tests.rs"]
mod tests;
