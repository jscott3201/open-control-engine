//! Every verb that writes a file against every kind of target (issue #758, ruled by the owner on
//! 2026-11-22), through the built binary: `emit -o`, `import -o`, `fmt --write` (whose target is
//! its input), `verify --out` of a vector set and `verify --commission --out`.
//!
//! The rule: the target must be absent or a regular file. A symbolic link, dangling or not, a
//! directory, a named pipe, a socket or a device is `output-not-a-file`, decided before anything
//! is read, and a refused run leaves every entry under the case's directory as it was, the file a
//! link names included. A target that is an input is `output-is-input`, and a hard link to an
//! input is one (Unix). A directory the caller's path passes through is followed: the caller named
//! it. `--commission` also refuses a regular `DIR/receipt.json` (`write-failed`, a stale receipt),
//! which the vector verb replaces.
//!
//! Links, hard links and named pipes are Unix cases. **Windows is untested**: there a symbolic
//! link or junction at the target is refused by the same check (std reports both as links) and a
//! hard link is not recognised as an input (std has no stable file index there).

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use support::commission::{RUN_FILES, run_file, subject};
use support::{Run, author_fixture, crate_dir, oce_in, scratch};

/// The five writers.
#[derive(Clone, Copy, Debug)]
enum Verb {
    Emit,
    Import,
    Fmt,
    Vectors,
    Commission,
}

/// The kinds of target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Absent,
    Regular,
    LinkInside,
    LinkOutside,
    Dangling,
    HardLinkToInput,
    LinkToInput,
    EmptyDirectory,
    FullDirectory,
    NamedPipe,
}

/// What a run must answer: exit `0` and the target written, or exit `2` with this code and every
/// entry left as it was.
#[derive(Debug, PartialEq, Eq)]
enum Answer {
    Written,
    Refused(String),
}

/// The kinds of target this platform can make.
fn targets() -> Vec<Target> {
    let mut all = vec![
        Target::Absent,
        Target::Regular,
        Target::EmptyDirectory,
        Target::FullDirectory,
    ];
    if cfg!(unix) {
        all.extend([
            Target::LinkInside,
            Target::LinkOutside,
            Target::Dangling,
            Target::HardLinkToInput,
            Target::LinkToInput,
            Target::NamedPipe,
        ]);
    }
    all
}

/// The table: what each verb answers for each kind of target.
fn expected(verb: Verb, target: Target) -> Answer {
    use Target::*;
    let refused = |code: &str| Answer::Refused(code.to_owned());
    match (verb, target) {
        (Verb::Fmt, Absent) => refused("not-found"),
        (Verb::Fmt, HardLinkToInput) => Answer::Written,
        (Verb::Commission, Regular) => refused("write-failed"),
        (_, Absent | Regular) => Answer::Written,
        (_, HardLinkToInput) => refused("output-is-input"),
        (_, LinkInside | LinkOutside | Dangling | LinkToInput) => refused("output-not-a-file"),
        (_, EmptyDirectory | FullDirectory | NamedPipe) => refused("output-not-a-file"),
    }
}

/// One case laid out: `base/work` is the working directory, `base/victim/victim.txt` a file
/// outside it that no run may write, `target` the path the verb writes (relative to `work`), and
/// `input` the file the verb reads that a hard link or a link may name.
struct Case {
    base: PathBuf,
    work: PathBuf,
    target: &'static str,
    #[cfg_attr(
        not(unix),
        expect(dead_code, reason = "only the Unix cases link or hard-link the input")
    )]
    input: &'static str,
    args: Vec<&'static str>,
}

const PRECIOUS: &[u8] = b"PRECIOUS";

fn author(relative: &str) -> Vec<u8> {
    fs::read(crate_dir().join(author_fixture(relative))).expect("an oce-author fixture")
}

/// A one-scenario vector set over AHU-0016's rule, listing `mutants`.
fn vector_set(mutants: &[&str]) -> String {
    let listed: Vec<Value> = mutants
        .iter()
        .map(|path| serde_json::json!({ "path": path }))
        .collect();
    serde_json::json!({
        "schema": "oce/vectors/v1",
        "clock": { "step_s": 60, "horizon_s": 60 },
        "mutants": listed,
        "scenarios": [{
            "name": "both_closed",
            "inputs": { "htg_vlv_cmd": 0.0, "clg_vlv_cmd": 0.0 },
            "expect": [{ "output": "yFault", "from_s": 0, "to_s": 60, "equals": false }]
        }]
    })
    .to_string()
}

fn ahu_rule() -> Vec<u8> {
    fs::read(
        crate_dir().join("../../oce-api/tests/fixtures/library_rules/AHU-0016/rule.cxf.jsonld"),
    )
    .expect("the AHU-0016 rule")
}

impl Case {
    fn lay_out(verb: Verb, name: &str) -> Self {
        let base = scratch(&format!("output_targets_{name}"));
        let work = base.join("work");
        fs::create_dir_all(&work).expect("work");
        fs::create_dir_all(base.join("victim")).expect("victim");
        fs::write(base.join("victim/victim.txt"), PRECIOUS).expect("victim");
        let (target, input, args): (&str, &str, Vec<&str>) = match verb {
            Verb::Emit => {
                fs::write(work.join("econ.oce"), author("text/econ_enable.oce")).expect("in");
                let args = vec!["emit", "econ.oce", "-o", "out.cxf.json"];
                ("out.cxf.json", "econ.oce", args)
            }
            Verb::Import => {
                fs::write(
                    work.join("rule.cxf.json"),
                    author("emit/econ_enable.cxf.json"),
                )
                .expect("in");
                (
                    "out.oce",
                    "rule.cxf.json",
                    vec!["import", "rule.cxf.json", "-o", "out.oce"],
                )
            }
            Verb::Fmt => ("t.oce", "t.oce", vec!["fmt", "--write", "t.oce"]),
            Verb::Vectors => {
                fs::write(work.join("vectors.json"), vector_set(&[])).expect("set");
                fs::write(work.join("rule.cxf.jsonld"), ahu_rule()).expect("rule");
                fs::create_dir(work.join("out")).expect("out");
                let args = vec![
                    "verify",
                    "vectors.json",
                    "--subject",
                    "rule.cxf.jsonld",
                    "--out",
                    "out",
                ];
                ("out/receipt.json", "vectors.json", args)
            }
            Verb::Commission => {
                fs::write(work.join("spec.json"), run_file("spec.json")).expect("spec");
                for file in RUN_FILES {
                    fs::write(work.join(file), run_file(file)).expect("a run file");
                }
                fs::write(work.join("plant.cxf.json"), subject("plant")).expect("plant");
                fs::create_dir(work.join("out")).expect("out");
                let args = vec!["verify", "--commission", "spec.json", "--out", "out"];
                ("out/receipt.json", "spec.json", args)
            }
        };
        Self {
            base,
            work,
            target,
            input,
            args,
        }
    }

    fn at(&self, relative: &str) -> PathBuf {
        self.work.join(relative)
    }

    /// The file a link inside names: beside the target.
    fn inner(&self) -> PathBuf {
        self.at(self.target)
            .parent()
            .expect("a parent")
            .join("inner.txt")
    }

    /// Plant `kind` at the target (for `fmt --write`, the target is the file it reads).
    fn plant(&self, verb: Verb, kind: Target) {
        let target = self.at(self.target);
        let canonical_text = matches!(verb, Verb::Fmt);
        let filler: Vec<u8> = if canonical_text {
            author("text/fmt/sections.in.oce")
        } else {
            b"OLDBYTES".to_vec()
        };
        fs::write(self.inner(), &filler).expect("inner");
        match kind {
            Target::Absent => {}
            Target::Regular => fs::write(&target, &filler).expect("regular"),
            Target::EmptyDirectory => fs::create_dir(&target).expect("dir"),
            Target::FullDirectory => {
                fs::create_dir(&target).expect("dir");
                fs::write(target.join("inside"), b"x").expect("inside");
            }
            #[cfg(unix)]
            Target::LinkInside => std::os::unix::fs::symlink("inner.txt", &target).expect("link"),
            #[cfg(unix)]
            Target::LinkOutside => {
                if canonical_text {
                    fs::write(self.base.join("victim/victim.txt"), &filler).expect("victim");
                }
                std::os::unix::fs::symlink(self.base.join("victim/victim.txt"), &target)
                    .expect("link");
            }
            #[cfg(unix)]
            Target::Dangling => {
                std::os::unix::fs::symlink(self.base.join("victim/nowhere.txt"), &target)
                    .expect("link");
            }
            #[cfg(unix)]
            Target::HardLinkToInput => {
                if canonical_text {
                    // `fmt --write`'s target is its input: the file has a second name.
                    fs::write(&target, &filler).expect("t.oce");
                    fs::hard_link(&target, self.at("alias.oce")).expect("hard link");
                } else {
                    fs::hard_link(self.at(self.input), &target).expect("hard link");
                }
            }
            #[cfg(unix)]
            Target::LinkToInput => {
                if canonical_text {
                    fs::write(self.at("real.oce"), &filler).expect("real");
                    std::os::unix::fs::symlink("real.oce", &target).expect("link");
                } else {
                    std::os::unix::fs::symlink(self.at(self.input), &target).expect("link");
                }
            }
            #[cfg(unix)]
            Target::NamedPipe => {
                let made = std::process::Command::new("mkfifo")
                    .arg(&target)
                    .status()
                    .expect("mkfifo runs");
                assert!(made.success());
            }
            #[cfg(not(unix))]
            other => panic!("{other:?} is a Unix case"),
        }
    }
}

/// Every entry under `dir`, by relative path, with what it is: a file's bytes, a link's target, a
/// directory, a named pipe (never opened).
fn snapshot(dir: &Path) -> Vec<(String, String)> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<(String, String)>) {
        for entry in fs::read_dir(at).expect("list") {
            let path = entry.expect("entry").path();
            let relative = path
                .strip_prefix(root)
                .expect("under the root")
                .to_string_lossy()
                .into_owned();
            let kind = fs::symlink_metadata(&path).expect("metadata").file_type();
            let what = if kind.is_symlink() {
                format!("link {}", fs::read_link(&path).expect("a link").display())
            } else if kind.is_dir() {
                walk(root, &path, out);
                "directory".to_owned()
            } else if kind.is_file() {
                format!("file {:?}", fs::read(&path).expect("read"))
            } else {
                "other".to_owned()
            };
            out.push((relative, what));
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

fn assert_case(verb: Verb, target: Target) {
    let name = format!("{verb:?}_{target:?}").to_lowercase();
    let case = Case::lay_out(verb, &name);
    case.plant(verb, target);
    let before = snapshot(&case.base);
    let run: Run = oce_in(&case.work, &case.args, b"");
    let answer = match run.code {
        0 | 1 => Answer::Written,
        _ => Answer::Refused(
            run.report()["errors"][0]["code"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        ),
    };
    assert_eq!(
        answer,
        expected(verb, target),
        "{name}: {}",
        run.transcript()
    );
    let victim = fs::read(case.base.join("victim/victim.txt")).expect("victim");
    if target != Target::LinkOutside || !matches!(verb, Verb::Fmt) {
        assert_eq!(victim, PRECIOUS, "{name}: the file outside was written");
    }
    assert!(
        !case.base.join("victim/nowhere.txt").exists(),
        "{name}: a dangling link was written through"
    );
    match answer {
        Answer::Refused(code) => {
            assert_eq!(
                snapshot(&case.base),
                before,
                "{name}: a refusal changed an entry"
            );
            let error = &run.report()["errors"][0];
            if code == "output-not-a-file" {
                let detail = error["detail"].as_str().expect("detail");
                assert!(
                    detail.starts_with(&format!("`{}` is ", case.target)),
                    "{name}: the detail names the target as given: {detail}"
                );
                assert!(detail.ends_with("nothing is read or written"), "{detail}");
                // The way out: a name the caller gave is left for the file a link names; a name
                // that is oce's own (`DIR/receipt.json`) is its owner's to remove or replace.
                let own = matches!(verb, Verb::Vectors | Verb::Commission);
                let link = matches!(
                    target,
                    Target::LinkInside
                        | Target::LinkOutside
                        | Target::Dangling
                        | Target::LinkToInput
                );
                assert_eq!(
                    detail.contains(": remove or replace that entry;"),
                    own,
                    "{name}: {detail}"
                );
                assert_eq!(
                    detail.contains(": give the path of the file it names;"),
                    !own && link,
                    "{name}: {detail}"
                );
                // Nothing was read: no stage ran and no list was made.
                let report = run.report();
                for key in ["dropped", "curves", "passed", "output", "changed"] {
                    assert!(report.get(key).is_none(), "{name}: {key}");
                }
                if let Some(stages) = report.get("stages") {
                    assert_eq!(stages, &serde_json::json!([]), "{name}");
                }
            }
        }
        Answer::Written => {
            let entry = fs::symlink_metadata(case.at(case.target)).expect("the target");
            assert!(entry.is_file(), "{name}: the target is a regular file");
            assert_eq!(
                fs::read(case.inner()).expect("inner"),
                if matches!(verb, Verb::Fmt) {
                    author("text/fmt/sections.in.oce")
                } else {
                    b"OLDBYTES".to_vec()
                },
                "{name}: the file beside the target was written"
            );
            let left: Vec<_> = snapshot(&case.base)
                .into_iter()
                .filter(|(path, _)| path.ends_with(".oce-tmp"))
                .collect();
            assert!(left.is_empty(), "{name}: {left:?}");
            if target == Target::HardLinkToInput {
                // `fmt --write` replaced its file: the other name keeps the old bytes.
                assert_eq!(
                    fs::read(case.at("alias.oce")).expect("alias"),
                    author("text/fmt/sections.in.oce")
                );
                assert_eq!(
                    fs::read(case.at("t.oce")).expect("t"),
                    author("text/fmt/sections.out.oce")
                );
            }
        }
    }
}

fn every_target(verb: Verb) {
    for target in targets() {
        assert_case(verb, target);
    }
}

#[test]
fn emit_writes_only_an_absent_path_or_a_regular_file() {
    every_target(Verb::Emit);
}

#[test]
fn import_writes_only_an_absent_path_or_a_regular_file() {
    every_target(Verb::Import);
}

#[test]
fn fmt_write_rewrites_only_a_regular_file() {
    every_target(Verb::Fmt);
}

#[test]
fn a_vector_receipt_is_written_only_to_an_absent_path_or_a_regular_file() {
    every_target(Verb::Vectors);
}

#[test]
fn a_commissioning_receipt_is_written_only_to_an_absent_path() {
    every_target(Verb::Commission);
}

#[cfg(unix)]
#[test]
fn a_directory_the_caller_names_through_a_link_is_followed_by_every_verb() {
    // `lnk -> ../victim`: the caller named the path, so the write lands in `victim/`.
    for (verb, args, written) in [
        (
            Verb::Emit,
            vec!["emit", "econ.oce", "-o", "lnk/e.cxf.json"],
            "e.cxf.json",
        ),
        (
            Verb::Import,
            vec!["import", "rule.cxf.json", "-o", "lnk/i.oce"],
            "i.oce",
        ),
        (Verb::Fmt, vec!["fmt", "--write", "lnk/t.oce"], "t.oce"),
        (
            Verb::Vectors,
            vec![
                "verify",
                "vectors.json",
                "--subject",
                "rule.cxf.jsonld",
                "--out",
                "lnk",
            ],
            "receipt.json",
        ),
        (
            Verb::Commission,
            vec!["verify", "--commission", "spec.json", "--out", "lnk"],
            "receipt.json",
        ),
    ] {
        let case = Case::lay_out(verb, &format!("parent_link_{verb:?}").to_lowercase());
        fs::write(
            case.base.join("victim/t.oce"),
            author("text/fmt/sections.in.oce"),
        )
        .expect("t");
        std::os::unix::fs::symlink("../victim", case.at("lnk")).expect("link");
        let run = oce_in(&case.work, &args, b"");
        assert!(run.code < 2, "{verb:?}: {}", run.transcript());
        assert!(
            fs::symlink_metadata(case.base.join("victim").join(written))
                .expect("written")
                .is_file(),
            "{verb:?}"
        );
        assert!(
            fs::symlink_metadata(case.at("lnk"))
                .expect("lnk")
                .is_symlink()
        );
    }
}

/// A vector run with `--out out`, the set at `vectors`, the subject at `subject`.
fn vector_run(work: &Path, vectors: &str, subject: &str, form: &[&'static str]) -> Run {
    let mut args = vec!["verify", vectors, "--subject", subject, "--out", "out"];
    args.extend_from_slice(form);
    oce_in(work, &args, b"")
}

/// The one error of a refused run: its code, `at` and detail.
fn refusal(run: &Run) -> (String, String, String) {
    assert_eq!(run.code, 2, "{}", run.transcript());
    let error = &run.report()["errors"][0];
    let text = |key: &str| error[key].as_str().unwrap_or_default().to_owned();
    (text("code"), text("at"), text("detail"))
}

#[test]
fn a_vector_receipt_that_is_the_set_or_the_subject_by_path_is_output_is_input() {
    let case = Case::lay_out(Verb::Vectors, "vector_input_by_path");
    // The set itself at `out/receipt.json`.
    fs::write(case.at("out/receipt.json"), vector_set(&[])).expect("the set");
    let before = snapshot(&case.base);
    let run = vector_run(&case.work, "out/receipt.json", "rule.cxf.jsonld", &[]);
    let (code, at, detail) = refusal(&run);
    assert_eq!(
        (code.as_str(), at.as_str()),
        ("output-is-input", "receipt.json")
    );
    assert_eq!(
        detail,
        "`out/receipt.json` is `out/receipt.json`, which the run reads; nothing is written"
    );
    assert_eq!(snapshot(&case.base), before);
    // The subject at `out/receipt.json`, another spelling of it on the command line.
    fs::write(case.at("out/receipt.json"), ahu_rule()).expect("the subject");
    let before = snapshot(&case.base);
    let run = vector_run(
        &case.work,
        "vectors.json",
        "./out/../out/receipt.json",
        &["--subject-form", "cxf"],
    );
    let (code, _, detail) = refusal(&run);
    assert_eq!(code, "output-is-input");
    assert!(detail.contains("`./out/../out/receipt.json`"), "{detail}");
    assert_eq!(snapshot(&case.base), before);
}

#[test]
fn a_vector_receipt_that_is_a_mutant_by_path_is_output_is_input() {
    let case = Case::lay_out(Verb::Vectors, "vector_mutant_by_path");
    let mutant = fs::read(
        crate_dir().join("../oce-verify/tests/fixtures/AHU-0016/mutants/late_alarm.cxf.json"),
    )
    .expect("a mutant");
    fs::write(case.at("out/receipt.json"), &mutant).expect("the mutant");
    fs::write(case.at("vectors.json"), vector_set(&["out/receipt.json"])).expect("set");
    let before = snapshot(&case.base);
    let run = vector_run(&case.work, "vectors.json", "rule.cxf.jsonld", &[]);
    let (code, at, detail) = refusal(&run);
    assert_eq!(
        (code.as_str(), at.as_str()),
        ("output-is-input", "receipt.json")
    );
    assert!(
        detail.contains("is `out/receipt.json`, which the run reads"),
        "{detail}"
    );
    assert_eq!(snapshot(&case.base), before);
    // The same set with `--out` elsewhere runs: the guard is about the receipt's path alone.
    fs::create_dir(case.at("elsewhere")).expect("elsewhere");
    let run = oce_in(
        &case.work,
        &[
            "verify",
            "vectors.json",
            "--subject",
            "rule.cxf.jsonld",
            "--out",
            "elsewhere",
        ],
        b"",
    );
    assert!(run.code < 2, "{}", run.transcript());
}

#[cfg(unix)]
#[test]
fn a_vector_receipt_that_an_input_names_through_a_link_or_a_hard_link_is_output_is_input() {
    let case = Case::lay_out(Verb::Vectors, "vector_input_by_link");
    fs::write(case.at("out/receipt.json"), ahu_rule()).expect("a regular receipt.json");
    // The subject named through a link to `out/receipt.json`.
    std::os::unix::fs::symlink("out/receipt.json", case.at("subject.cxf.jsonld")).expect("link");
    let before = snapshot(&case.base);
    let run = vector_run(&case.work, "vectors.json", "subject.cxf.jsonld", &[]);
    let (code, _, detail) = refusal(&run);
    assert_eq!(code, "output-is-input");
    assert!(detail.contains("is `subject.cxf.jsonld`"), "{detail}");
    assert_eq!(snapshot(&case.base), before);
    // A mutant that is a hard link to `out/receipt.json`.
    let mutant = fs::read(
        crate_dir().join("../oce-verify/tests/fixtures/AHU-0016/mutants/late_alarm.cxf.json"),
    )
    .expect("a mutant");
    fs::write(case.at("out/receipt.json"), &mutant).expect("receipt");
    fs::hard_link(case.at("out/receipt.json"), case.at("m.cxf.json")).expect("hard link");
    fs::write(case.at("vectors.json"), vector_set(&["m.cxf.json"])).expect("set");
    let before = snapshot(&case.base);
    let run = vector_run(&case.work, "vectors.json", "rule.cxf.jsonld", &[]);
    let (code, _, detail) = refusal(&run);
    assert_eq!(code, "output-is-input");
    assert!(detail.contains("is `m.cxf.json`"), "{detail}");
    assert_eq!(snapshot(&case.base), before);
}

#[test]
fn a_vector_receipt_already_there_is_replaced_by_the_next_run() {
    // Ruled by the owner on 2026-11-22: the vector verb keeps replacing a regular receipt.json.
    let case = Case::lay_out(Verb::Vectors, "vector_replace");
    fs::write(case.at("out/receipt.json"), b"{\"stale\": true}\n").expect("stale");
    let run = oce_in(&case.work, &case.args, b"");
    assert_eq!(run.code, 0, "{}", run.transcript());
    let written = fs::read(case.at("out/receipt.json")).expect("receipt");
    let plain = oce_in(
        &case.work,
        &["verify", "vectors.json", "--subject", "rule.cxf.jsonld"],
        b"",
    );
    assert_eq!(written, plain.stdout);
}

#[cfg(unix)]
#[test]
fn the_target_is_judged_before_any_input_is_read() {
    // A dangling link at the target and an input that is not there: each verb answers about the
    // target, so a refused output never costs a read (for `fmt --write` the target is the input,
    // so its row in the table already says so).
    for verb in [Verb::Emit, Verb::Import, Verb::Vectors, Verb::Commission] {
        let name = format!("before_reads_{verb:?}").to_lowercase();
        let case = Case::lay_out(verb, &name);
        case.plant(verb, Target::Dangling);
        fs::remove_file(case.at(case.input)).expect("remove the input");
        let run = oce_in(&case.work, &case.args, b"");
        let (code, _, _) = refusal(&run);
        assert_eq!(code, "output-not-a-file", "{name}: {}", run.transcript());
    }
}

#[test]
fn an_empty_out_is_refused_by_both_forms_as_a_missing_directory() {
    // Until issue #758's review the vector form wrote `./receipt.json` for `--out ""`.
    for verb in [Verb::Vectors, Verb::Commission] {
        let name = format!("empty_out_{verb:?}").to_lowercase();
        let case = Case::lay_out(verb, &name);
        let mut args: Vec<&str> = case.args.clone();
        let out = args
            .iter()
            .position(|arg| *arg == "out")
            .expect("--out out");
        args[out] = "";
        let before = snapshot(&case.base);
        let run = oce_in(&case.work, &args, b"");
        let (code, at, detail) = refusal(&run);
        assert_eq!(
            (code.as_str(), at.as_str(), detail.as_str()),
            (
                "write-failed",
                ".",
                "``: the output directory does not exist; it is never created"
            ),
            "{name}: {}",
            run.transcript()
        );
        assert_eq!(snapshot(&case.base), before, "{name}: nothing written");
    }
}
