//! The tests of `commission_out.rs`: the checks and the writes (the adversary of issue #771 is
//! `commission_out_adversary_tests.rs`).

use std::path::PathBuf;

use super::*;

/// `dir` opened as [`check()`] opens it, without its checks (a test of the write alone).
fn opened(dir: &Path) -> Out<'_> {
    Out {
        dir: dir.as_os_str(),
        handle: Directory::open(dir).expect("the directory"),
    }
}

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
    let out = check(dir.as_os_str(), &[]).expect("checked");
    fs::create_dir(dir.join(OUTPUTS)).expect("a racing writer's outputs");
    let refused = write(&out, &[], b"{}").expect_err("refused");
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
pub(super) fn curves(count: usize) -> Vec<OutputCurves> {
    thread_local! {
        /// The real run's curves, made once per thread: the adversaries write thousands of times.
        static ONE: OutputCurves = sample_curves();
    }
    let one = ONE.with(OutputCurves::clone);
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
    let run =
        oce_verify::run_commissioning(&spec, 0, &inputs, &oce_verify::CommissionOptions::default())
            .expect("runs");
    run.curves.into_iter().next().expect("one curve")
}

#[test]
fn a_receipt_write_that_fails_lists_every_curve_written_before_it() {
    // A non-empty directory where the receipt goes (the check would refuse it; this is the
    // write alone): the rename fails after both outputs' curves are written.
    let dir = scratch("receipt_fails");
    fs::create_dir_all(dir.join(RECEIPT).join("inside")).expect("a directory at the receipt");
    let refused = write(&opened(&dir), &curves(2), b"{}").expect_err("refused");
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
    let checked = check(out.as_os_str(), &[]).expect("checked");
    std::os::unix::fs::symlink(&victim, out.join(RECEIPT)).expect("a planted link");
    let written = write(&checked, &curves(1), b"{}\n").expect("written");
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
    let written = write(&opened(&dir), &[], b"{}\n").expect("written");
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

#[cfg(unix)]
#[test]
fn a_dir_removed_after_the_check_is_a_typed_refusal_that_writes_nothing() {
    let dir = scratch("vanished");
    let out = dir.join("out");
    fs::create_dir(&out).expect("out");
    let checked = check(out.as_os_str(), &[]).expect("checked");
    fs::remove_dir(&out).expect("DIR vanishes mid-run");
    let refused = write(&checked, &curves(1), b"{}\n").expect_err("refused");
    assert_eq!(
        (refused.code, refused.at.as_str()),
        (codes::WRITE_FAILED, OUTPUTS)
    );
    assert!(
        refused.detail.contains("it could not be created"),
        "{}",
        refused.detail
    );
    assert!(refused.written.is_empty());
    assert_eq!(
        fs::read_dir(&dir).expect("list").count(),
        0,
        "nothing written"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_dir_that_can_be_searched_but_not_listed_is_refused_as_unreadable() {
    // The handle is opened read-only (rustix has no `O_SEARCH` on macOS): issue #771 made this
    // a refusal where the path-based writer passed through.
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("search_only");
    let out = dir.join("out");
    fs::create_dir(&out).expect("out");
    fs::set_permissions(&out, fs::Permissions::from_mode(0o300)).expect("chmod");
    let listable = fs::read_dir(&out).is_ok();
    let result = check(out.as_os_str(), &[]).map(|_| ());
    fs::set_permissions(&out, fs::Permissions::from_mode(0o755)).expect("chmod back");
    if !listable {
        let refused = result.expect_err("refused");
        assert_eq!(
            (refused.code, refused.at.as_str()),
            (codes::WRITE_FAILED, ".")
        );
        assert!(
            refused
                .detail
                .ends_with("the output directory cannot be read: permission denied"),
            "{}",
            refused.detail
        );
    }
    let _ = fs::remove_dir_all(&dir);
}
