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
//! owner-only, `0600` on Unix, synced, renamed over the entry), which never follows a link, so a
//! link planted at `receipt.json` between the check and the write is replaced, its target
//! untouched. A write that fails part-way is `write-failed` naming the file; the files already
//! written stay, and the refusal lists them as the report's `curves`: without `receipt.json` they
//! are not a run's output.

use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use oce_verify::{Curve, OutputCurves};

use super::{Named, input_at, target_accepted, write_atomically};
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

/// The checks made before the run (module header). `inputs` pairs each file the run read with
/// how the report names it: the spec as given, every other file by its path as the spec writes it.
///
/// # Errors
/// `write-failed` for a `DIR` that is not an existing directory, a `DIR/outputs` that exists (a
/// link included) or a regular `DIR/receipt.json`; `output-is-input`, before that last, for a
/// `DIR/receipt.json` that is one of `inputs`; `output-not-a-file` for a non-file that appeared
/// at `DIR/receipt.json` since [`target()`].
pub(crate) fn check(dir: &OsStr, inputs: &[(String, PathBuf)]) -> Result<(), Refused> {
    let root = Path::new(dir);
    match fs::metadata(root) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Err(failed(dir, ".", "the output directory is not a directory")),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(failed(dir, ".", MISSING_DIR));
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
    if let Some(name) = input_at(&receipt, inputs) {
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
    target(dir)?;
    match fs::symlink_metadata(&receipt) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
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
        if let Err(error) = fs::create_dir(root.join(OUTPUTS).join(k.to_string())) {
            let refused = failed(
                dir,
                &folder,
                format!("it could not be created: {}", error.kind()),
            );
            return Err(with_written(refused, written));
        }
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
        for (name, curve) in files {
            let at = format!("{folder}/{name}");
            let path = root.join(OUTPUTS).join(k.to_string()).join(name);
            if let Err((_, why)) = write_atomically(&path, curve.to_csv().as_bytes()) {
                return Err(with_written(failed(dir, &at, why), written));
            }
            if let Some(entry) = written.last_mut() {
                entry.files.push(shown(dir, &at));
            }
        }
    }
    if let Err((_, why)) = write_atomically(&root.join(RECEIPT), receipt) {
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

    /// `count` outputs' curves, each a real run's (the type cannot be built outside its crate).
    fn curves(count: usize) -> Vec<OutputCurves> {
        let one = sample_curves();
        (0..count)
            .map(|k| {
                let mut copy = one.clone();
                copy.output = format!("urn:doc#top.y{k}");
                copy
            })
            .collect()
    }

    /// One `OutputCurves` from a real run (the type is non-exhaustive outside its crate).
    fn sample_curves() -> OutputCurves {
        let cxf = br#"{"@context": {"S231": "http://data.ashrae.org/S231#"}, "@graph": [
  {"@id": "urn:doc#top", "@type": "S231:Block",
   "S231:containsBlock": [{"@id": "urn:doc#top.not"}],
   "S231:hasInput": [{"@id": "urn:doc#top.u"}], "S231:hasOutput": [{"@id": "urn:doc#top.y"}]},
  {"@id": "urn:doc#top.not", "@type": "CDL.Logical.Not",
   "S231:hasInput": {"@id": "urn:doc#top.not.u"}, "S231:hasOutput": {"@id": "urn:doc#top.not.y"}},
  {"@id": "urn:doc#top.not.u", "@type": "S231:BooleanInput",
   "S231:isOfDataType": {"@id": "S231:Boolean"}},
  {"@id": "urn:doc#top.not.y", "@type": "S231:BooleanOutput",
   "S231:isOfDataType": {"@id": "S231:Boolean"}, "S231:isConnectedTo": {"@id": "urn:doc#top.y"}},
  {"@id": "urn:doc#top.u", "@type": "S231:BooleanInput",
   "S231:isOfDataType": {"@id": "S231:Boolean"}, "S231:isConnectedTo": {"@id": "urn:doc#top.not.u"}},
  {"@id": "urn:doc#top.y", "@type": "S231:BooleanOutput",
   "S231:isOfDataType": {"@id": "S231:Boolean"}}
]}"#;
        let spec = oce_verify::CommissioningSpec::from_json_str(
            r#"{"references": [{"model": "Inverter", "sequence": "inv", "cxf": "inv.json",
  "pointNameMapping": [
    {"cdl": {"name": "u", "type": "bool"}, "device": {"name": "Fan", "type": "bool"}},
    {"cdl": {"name": "y", "type": "bool"}, "device": {"name": "Alarm", "type": "bool"}}],
  "window": {"start": 0, "stop": 10}, "sampling": 10,
  "inputs": [{"path": "fan.csv", "format": "csv", "timeColumn": "time",
              "columns": {"Fan": "Fan"}}],
  "expected": [{"path": "alarm.csv", "format": "csv", "timeColumn": "time",
                "columns": {"Alarm": "Alarm"}}]}]}"#,
            &oce_verify::SpecLimits::default(),
        )
        .expect("the spec loads");
        let inputs = oce_verify::SpecInputs {
            cxf: cxf.to_vec(),
            files: vec![
                (
                    "fan.csv".to_owned(),
                    b"time,Fan\n0,true\n10,false\n".to_vec(),
                ),
                (
                    "alarm.csv".to_owned(),
                    b"time,Alarm\n0,false\n10,true\n".to_vec(),
                ),
            ],
        };
        let run = oce_verify::run_commissioning(
            &spec,
            0,
            &inputs,
            &oce_verify::CommissionOptions::default(),
        )
        .expect("runs");
        run.curves.into_iter().next().expect("one curve")
    }

    #[test]
    fn a_receipt_write_that_fails_lists_every_curve_written_before_it() {
        // A non-empty directory where the receipt goes (the check would refuse it; this is the
        // write alone): the rename fails after both outputs' curves are written.
        let dir = scratch("receipt_fails");
        fs::create_dir_all(dir.join(RECEIPT).join("inside")).expect("a directory at the receipt");
        let refused = write(dir.as_os_str(), &curves(2), b"{}").expect_err("refused");
        assert_eq!(
            (refused.code, refused.at.as_str()),
            (codes::WRITE_FAILED, RECEIPT)
        );
        let shown_dir = dir.to_string_lossy().into_owned();
        let listed: Vec<(usize, Vec<String>)> = refused
            .written
            .iter()
            .map(|output| (output.index, output.files.clone()))
            .collect();
        assert_eq!(
            listed,
            [
                (0, vec![format!("{shown_dir}/outputs/0/errors.csv")]),
                (1, vec![format!("{shown_dir}/outputs/1/errors.csv")]),
            ]
        );
        for k in 0..2 {
            assert!(dir.join(format!("outputs/{k}/errors.csv")).is_file());
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_planted_at_the_receipt_after_the_check_is_replaced_and_its_target_untouched() {
        // The race the check cannot close: a link appears at `receipt.json` between the check
        // and the write. The write replaces the entry and never writes where it points.
        let dir = scratch("planted_receipt");
        let victim = dir.join("victim.txt");
        fs::write(&victim, b"PRECIOUS").expect("victim");
        let out = dir.join("out");
        fs::create_dir(&out).expect("out");
        assert!(check(out.as_os_str(), &[]).is_ok());
        std::os::unix::fs::symlink(&victim, out.join(RECEIPT)).expect("a planted link");
        let written = write(out.as_os_str(), &curves(1), b"{}\n").expect("written");
        assert_eq!(written.len(), 1);
        assert_eq!(fs::read(&victim).expect("victim"), b"PRECIOUS");
        let entry = fs::symlink_metadata(out.join(RECEIPT)).expect("the receipt");
        assert!(entry.file_type().is_file());
        assert_eq!(fs::read(out.join(RECEIPT)).expect("receipt"), b"{}\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_non_file_that_appears_after_the_first_check_is_refused_by_the_second() {
        // `target()` runs before anything is read and `check()` after the reads; a link planted
        // between them is refused as the first check refuses it, and a regular file stays the
        // stale receipt `write-failed` names.
        let dir = scratch("late_link");
        let out = dir.join("out");
        fs::create_dir(&out).expect("out");
        assert!(target(out.as_os_str()).is_ok());
        std::os::unix::fs::symlink(dir.join("victim.txt"), out.join(RECEIPT)).expect("a link");
        let refused = check(out.as_os_str(), &[]).expect_err("a link");
        assert_eq!(
            (refused.code, refused.at.as_str()),
            (codes::OUTPUT_NOT_A_FILE, RECEIPT)
        );
        assert_eq!(
            target(out.as_os_str()).expect_err("a link").code,
            codes::OUTPUT_NOT_A_FILE
        );
        fs::remove_file(out.join(RECEIPT)).expect("unlink");
        fs::write(out.join(RECEIPT), b"{}\n").expect("a stale receipt");
        assert!(
            target(out.as_os_str()).is_ok(),
            "a regular file passes the first check"
        );
        let refused = check(out.as_os_str(), &[]).expect_err("a stale receipt");
        assert_eq!(refused.code, codes::WRITE_FAILED);
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
        // The root directory keeps its slash; an empty DIR (refused before any report names
        // it) is never shown as the root's file.
        assert_eq!(shown(OsStr::new("/"), RECEIPT), "/receipt.json");
        assert_eq!(shown(OsStr::new(""), RECEIPT), "receipt.json");
        assert_eq!(
            shown(OsStr::new("."), "outputs/0/errors.csv"),
            "./outputs/0/errors.csv"
        );
        let missing = failed(OsStr::new("absent"), ".", "gone");
        assert_eq!(missing.detail, "`absent`: gone");
    }
}
