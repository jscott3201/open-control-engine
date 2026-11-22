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
//! **Before the run** ([`check()`]): `DIR` must be an existing directory (it is never created) and
//! `DIR/outputs` must not exist (`write-failed`), so the curves of two runs are never mixed; and
//! `DIR/receipt.json` must not be the spec or any file the run read (`output-is-input`). So a
//! refusal before the first write leaves nothing behind. **The writes** ([`write()`]): `outputs/` is
//! made with a plain create, which fails on anything already there, a directory a racing writer
//! made included (`write-failed`); each file goes through `write_atomically` (a fresh temporary
//! file, synced, renamed over the target). A write that fails part-way is `write-failed` naming
//! the file, and the files already written stay: without `receipt.json` they are not a run's
//! output.

use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use oce_verify::{Curve, OutputCurves};

use super::{same_file, write_atomically};
use crate::{codes, input};

/// The file the receipt is written to, inside `DIR`.
pub(crate) const RECEIPT: &str = "receipt.json";

/// The directory the curves are written under, inside `DIR`.
const OUTPUTS: &str = "outputs";

/// A refusal of the output: the file within `DIR` it concerns, the code and the sentence.
#[derive(Debug)]
pub(crate) struct Refused {
    /// The file or directory within `DIR` (`outputs/0/errors.csv`), the error's `at`.
    pub(crate) at: String,
    /// `write-failed` or `output-is-input`.
    pub(crate) code: &'static str,
    /// The sentence, naming the file under `DIR` as given.
    pub(crate) detail: String,
}

/// `name` under `dir` as the report shows it: `DIR` as given, then `/` and `name` on every
/// platform, so a transcript is one text everywhere.
pub(crate) fn shown(dir: &OsStr, name: &str) -> String {
    format!("{}/{name}", input::display_name(dir).trim_end_matches('/'))
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

/// The checks made before the run (module header). `inputs` pairs each file the run read with
/// how the report names it: the spec as given, every other file by its path as the spec writes it.
///
/// # Errors
/// `write-failed` for a `DIR` that is not an existing directory or a `DIR/outputs` that exists;
/// `output-is-input` for a `DIR/receipt.json` that is one of `inputs`.
pub(crate) fn check(dir: &OsStr, inputs: &[(String, PathBuf)]) -> Result<(), Refused> {
    let root = Path::new(dir);
    match fs::metadata(root) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Err(failed(dir, ".", "the output directory is not a directory")),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(failed(
                dir,
                ".",
                "the output directory does not exist; it is never created",
            ));
        }
        Err(error) => {
            return Err(failed(
                dir,
                ".",
                format!("the output directory cannot be read: {}", error.kind()),
            ));
        }
    }
    match fs::symlink_metadata(root.join(OUTPUTS)) {
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
    let receipt = root.join(RECEIPT);
    if let Some((name, _)) = inputs.iter().find(|(_, read)| same_file(&receipt, read)) {
        return Err(Refused {
            at: RECEIPT.to_owned(),
            code: codes::OUTPUT_IS_INPUT,
            detail: format!(
                "`{}` is `{name}`, which the run reads; nothing is written",
                shown(dir, RECEIPT)
            ),
        });
    }
    Ok(())
}

fn outputs_exist(dir: &OsStr) -> Refused {
    failed(
        dir,
        OUTPUTS,
        "it already exists, and the curves of two runs are never mixed; give an empty directory",
    )
}

/// Write the curves, then `receipt` (module header), and list what was written for each output.
///
/// # Errors
/// `write-failed`, naming the file; the files already written stay.
pub(crate) fn write(
    dir: &OsStr,
    curves: &[OutputCurves],
    receipt: &[u8],
) -> Result<Vec<Written>, Refused> {
    let root = Path::new(dir);
    match fs::create_dir(root.join(OUTPUTS)) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => return Err(outputs_exist(dir)),
        Err(error) => {
            return Err(failed(
                dir,
                OUTPUTS,
                format!("it could not be created: {}", error.kind()),
            ));
        }
    }
    let mut written = Vec::with_capacity(curves.len());
    for (k, output) in curves.iter().enumerate() {
        let folder = format!("{OUTPUTS}/{k}");
        fs::create_dir(root.join(OUTPUTS).join(k.to_string())).map_err(|error| {
            failed(
                dir,
                &folder,
                format!("it could not be created: {}", error.kind()),
            )
        })?;
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
        let mut names = Vec::with_capacity(files.len());
        for (name, curve) in files {
            let at = format!("{folder}/{name}");
            let path = root.join(OUTPUTS).join(k.to_string()).join(name);
            write_atomically(&path, curve.to_csv().as_bytes())
                .map_err(|(_, why)| failed(dir, &at, why))?;
            names.push(shown(dir, &at));
        }
        written.push(Written {
            index: k,
            output: output.output.clone(),
            files: names,
        });
    }
    write_atomically(&root.join(RECEIPT), receipt).map_err(|(_, why)| failed(dir, RECEIPT, why))?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "oce-cli-commission-out-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create the directory");
        dir
    }

    #[test]
    fn an_outputs_directory_made_after_the_check_still_refuses_the_write_and_writes_nothing() {
        // The race the check before the run cannot close: `outputs/` appears between the check
        // and the write. The plain create refuses it, and the receipt is not written.
        let dir = scratch("race");
        assert!(check(dir.as_os_str(), &[]).is_ok());
        fs::create_dir(dir.join(OUTPUTS)).expect("a racing writer's outputs");
        let refused = write(dir.as_os_str(), &[], b"{}").expect_err("refused");
        assert_eq!(
            (refused.code, refused.at.as_str()),
            (codes::WRITE_FAILED, OUTPUTS)
        );
        // The refusal of an existing `outputs/`, not a failed create's generic one.
        assert_eq!(refused.detail, outputs_exist(dir.as_os_str()).detail);
        assert!(!dir.join(RECEIPT).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_run_with_no_compared_output_writes_an_empty_outputs_and_the_receipt() {
        let dir = scratch("empty");
        let written = write(dir.as_os_str(), &[], b"{}\n").expect("written");
        assert!(written.is_empty());
        assert!(dir.join(OUTPUTS).is_dir());
        assert_eq!(fs::read_dir(dir.join(OUTPUTS)).expect("list").count(), 0);
        assert_eq!(fs::read(dir.join(RECEIPT)).expect("receipt"), b"{}\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_shown_name_keeps_dir_as_given_with_one_slash() {
        assert_eq!(shown(OsStr::new("out"), RECEIPT), "out/receipt.json");
        assert_eq!(shown(OsStr::new("out/"), RECEIPT), "out/receipt.json");
        assert_eq!(
            shown(OsStr::new("."), "outputs/0/errors.csv"),
            "./outputs/0/errors.csv"
        );
        let missing = failed(OsStr::new("absent"), ".", "gone");
        assert_eq!(missing.detail, "`absent`: gone");
    }
}
