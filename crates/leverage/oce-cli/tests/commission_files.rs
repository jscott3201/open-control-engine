//! `oce verify --commission` and the files around it (commissioning note, "CLI": "Reads" and
//! "Output", decision 37; issue #664):
//!
//! - `--out DIR` writes each output's `errors.csv`, the bound curves under `--bounds` where an
//!   envelope was built, then `receipt.json`, and the report lists every file per output;
//! - every curve equals the library's, and the exceedance columns equal `oce-verify`'s goldens
//!   and the independent derivation of its oracle case (`derive_run.py`), printed `%.16g`, with
//!   `nan` for an uncovered instant and `inf` for a non-finite engine sample;
//! - two runs give the same stdout and the same files; `--timings`, `--deny-warnings` and
//!   `--data` move only what they say;
//! - nothing is written on a refusal, and a write that fails part-way names its file.
//!
//! Each run starts in a scratch directory with the inputs copied in, so every path it prints is
//! relative. No time is asserted (`TESTING.md`).

mod support;

use std::fs;
use std::path::Path;

use oce_verify::{CommissionOptions, CommissioningSpec, SpecInputs, SpecLimits};
use serde_json::Value;
use support::commission::{
    csv_of, golden_curves, golden_receipt, oracle_layout, run_file, run_layout, signals_layout,
    subject, tree,
};
use support::{compare_or_bless, oce_in};

fn commission(dir: &Path, extra: &[&str]) -> support::Run {
    let mut args = vec!["verify", "--commission", "spec.json"];
    args.extend_from_slice(extra);
    oce_in(dir, &args, b"")
}

/// The library's run of the run fixture's case `case`, with the bound curves kept.
fn library_run(case: &str) -> oce_verify::CommissionRun {
    let spec = CommissioningSpec::from_json_bytes(&run_file("spec.json"), &SpecLimits::default())
        .expect("the spec loads");
    let mut files: Vec<(String, Vec<u8>)> = support::commission::RUN_FILES
        .iter()
        .map(|name| ((*name).to_owned(), run_file(name)))
        .collect();
    match case {
        "pass" => {}
        "fail" => files[2].1 = run_file("oracle_outputs.csv"),
        other => panic!("no case {other}"),
    }
    let inputs = SpecInputs {
        cxf: subject("plant"),
        files,
    };
    let mut options = CommissionOptions::default();
    options.bounds = true;
    oce_verify::run_commissioning(&spec, 0, &inputs, &options).expect("the library runs")
}

#[test]
fn out_writes_every_curve_and_the_receipt_and_lists_the_files_per_output() {
    for case in ["pass", "fail", "no_data"] {
        let dir = run_layout(&format!("commission_out_{case}"), case);
        fs::create_dir(dir.join("out")).expect("out");
        let run = commission(&dir, &["--out", "out"]);
        assert_eq!(run.code, i32::from(case != "pass"), "{}", run.transcript());
        let report = run.report();
        assert_eq!(report["output"], "out/receipt.json");
        assert_eq!(report["status"], case);
        // The receipt written is the library's golden, and the stdout receipt of a plain run.
        let receipt = fs::read(dir.join("out/receipt.json")).expect("the receipt");
        assert!(receipt == golden_receipt(case), "{case}: receipt.json");
        assert_eq!(commission(&dir, &[]).stdout, receipt);
        let parsed: Value = serde_json::from_slice(&receipt).expect("JSON");
        assert_eq!(report["receipt_id"], parsed["receipt_id"]);
        // One errors.csv per compared output, each the golden column printed `%.16g`.
        let golden = golden_curves(case);
        let curves = report["curves"].as_array().expect("curves");
        assert_eq!(curves.len(), golden.len(), "{case}");
        for (k, ((output, points), listed)) in golden.iter().zip(curves).enumerate() {
            assert_eq!(listed["index"], k);
            assert_eq!(listed["output"], output.as_str());
            assert_eq!(parsed["body"]["outputs"][k]["output"], output.as_str());
            assert_eq!(listed["files"][0], format!("out/outputs/{k}/errors.csv"));
            let written =
                fs::read_to_string(dir.join(format!("out/outputs/{k}/errors.csv"))).expect("csv");
            assert_eq!(written, csv_of(points), "{case} output {k}");
        }
        // Without --bounds nothing else is written: each output's errors.csv and the receipt.
        let names: Vec<String> = tree(&dir.join("out")).into_iter().map(|(n, _)| n).collect();
        let mut expected: Vec<String> = (0..golden.len())
            .map(|k| format!("outputs/{k}/errors.csv"))
            .collect();
        expected.push("receipt.json".to_owned());
        assert_eq!(names, expected, "{case}");
    }
}

#[test]
fn bounds_writes_the_four_curves_only_where_an_envelope_was_built() {
    let dir = run_layout("commission_out_bounds", "fail");
    fs::create_dir(dir.join("out")).expect("out");
    let run = commission(&dir, &["--out", "out", "--bounds"]);
    assert_eq!(run.code, 1, "{}", run.transcript());
    let library = library_run("fail");
    let report = run.report();
    let written = fs::read(dir.join("out/receipt.json")).expect("receipt");
    assert!(
        written == golden_receipt("fail"),
        "--bounds moves no receipt byte"
    );
    let receipt: Value = serde_json::from_slice(&written).expect("JSON");
    let mut golden =
        String::from("# the files of --out --bounds for the run fixture's fail case\n");
    for (k, curves) in library.curves.iter().enumerate() {
        let built = receipt["body"]["outputs"][k].get("half_width").is_some();
        let listed: Vec<&str> = report["curves"][k]["files"]
            .as_array()
            .expect("files")
            .iter()
            .map(|f| f.as_str().expect("a name"))
            .collect();
        let errors = format!("out/outputs/{k}/errors.csv");
        match &curves.bounds {
            Some(bounds) => {
                assert!(
                    built,
                    "output {k}: the receipt has half_width with an envelope"
                );
                let names = ["reference", "test", "lowerBound", "upperBound"];
                let expected: Vec<String> = std::iter::once(errors)
                    .chain(names.iter().map(|n| format!("out/outputs/{k}/{n}.csv")))
                    .collect();
                assert_eq!(listed, expected, "output {k}");
                for (name, curve) in names.iter().zip([
                    &bounds.reference,
                    &bounds.test,
                    &bounds.lower,
                    &bounds.upper,
                ]) {
                    let path = dir.join(format!("out/outputs/{k}/{name}.csv"));
                    let written = fs::read_to_string(&path).expect("a bound file");
                    assert_eq!(written, curve.to_csv(), "output {k} {name}");
                }
            }
            None => {
                assert!(!built, "output {k}: no envelope, no half_width");
                assert_eq!(listed, [errors.as_str()], "output {k}");
            }
        }
    }
    // Both kinds are in the case: two Real outputs with an envelope, two discrete ones without.
    let built = library.curves.iter().filter(|c| c.bounds.is_some()).count();
    assert_eq!((built, library.curves.len()), (2, 4));
    for (name, bytes) in tree(&dir.join("out")) {
        golden.push_str(&format!(
            "=== {name}\n{}",
            String::from_utf8(bytes).expect("UTF-8")
        ));
    }
    compare_or_bless("commission/fail_bounds_files.txt", &golden);
}

#[test]
fn an_uncovered_instant_is_nan_in_errors_csv_as_the_independent_derivation_has_it() {
    // The oracle case: a frame refused for a missing sample leaves 10 s and 15 s uncovered.
    let oracle = String::from_utf8(run_file("oracle.txt")).expect("UTF-8");
    for envelope in ["funnel", "aligned-grid"] {
        let dir = oracle_layout(&format!("commission_oracle_{envelope}"), envelope);
        fs::create_dir(dir.join("out")).expect("out");
        let run = commission(&dir, &["--out", "out"]);
        assert!(run.code <= 1, "{}", run.transcript());
        let receipt: Value =
            serde_json::from_slice(&fs::read(dir.join("out/receipt.json")).expect("receipt"))
                .expect("JSON");
        let outputs = receipt["body"]["outputs"].as_array().expect("outputs");
        assert_eq!(outputs.len(), 4);
        for (k, output) in outputs.iter().enumerate() {
            let path = output["output"].as_str().expect("a path");
            let name = path.rsplit('.').next().expect("a last segment");
            let points = oracle_errors(&oracle, envelope, name);
            assert!(!points.is_empty(), "{envelope} {name}");
            let written =
                fs::read_to_string(dir.join(format!("out/outputs/{k}/errors.csv"))).expect("csv");
            assert_eq!(written, csv_of(&points), "{envelope} {name}");
            // 10 s is uncovered for every output (15 s too, where no indicator masks it).
            assert!(
                written.contains("\n10,nan\n"),
                "{envelope} {name}: {written}"
            );
        }
    }
}

/// The oracle's `error` rows for one output of one case, as points (`nan` read as a NaN).
fn oracle_errors(text: &str, case: &str, output: &str) -> Vec<(f64, f64)> {
    let (mut in_case, mut inside) = (false, false);
    let mut points = Vec::new();
    let bits = |cell: &str| {
        if cell == "nan" {
            f64::NAN
        } else {
            f64::from_bits(u64::from_str_radix(cell, 16).expect("hex"))
        }
    };
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let cells: Vec<&str> = line.split('\t').collect();
        match cells[0] {
            "case" => in_case = cells[1] == case,
            "output" => inside = in_case && cells[1] == output,
            "receipt" => inside = false,
            "error" if inside => points.push((bits(cells[1]), bits(cells[2]))),
            _ => {}
        }
    }
    points
}

#[test]
fn a_non_finite_engine_sample_is_inf_at_every_compared_instant_and_the_spellings_are_pinned() {
    // `ratio` is zone.T / ahu.T: an ahu.T of zero at 10 s makes the engine's sample infinite,
    // so the output fails with no envelope and every compared instant's exceedance is +inf.
    let dir = signals_layout(
        "commission_infinity",
        &[
            (0, 1.0, 1.0, true),
            (10, 0.0, 1.0, true),
            (20, 1.0, 1.0, true),
        ],
    );
    fs::create_dir(dir.join("out")).expect("out");
    let run = commission(&dir, &["--out", "out", "--bounds"]);
    assert_eq!(run.code, 1, "{}", run.transcript());
    let written = fs::read_to_string(dir.join("out/outputs/0/errors.csv")).expect("csv");
    let mut expected = String::from("x,y\n");
    for t in 0..=20 {
        expected.push_str(&format!("{t},inf\n"));
    }
    assert_eq!(written, expected);
    // No envelope: no bound file, and the report says so.
    assert_eq!(
        run.report()["curves"][0]["files"],
        serde_json::json!(["out/outputs/0/errors.csv"])
    );
    // The writer's spellings, which the files above rely on.
    assert_eq!(oce_verify::format_g16(f64::NAN), "nan");
    assert_eq!(oce_verify::format_g16(f64::INFINITY), "inf");
    assert_eq!(oce_verify::format_g16(-0.0), "-0");
}

#[test]
fn two_runs_give_the_same_stdout_and_the_same_files() {
    let dir = oracle_layout("commission_twice", "funnel");
    for out in ["a", "b"] {
        fs::create_dir(dir.join(out)).expect("out");
    }
    let first = commission(&dir, &["--out", "a", "--bounds"]);
    let second = commission(&dir, &["--out", "b", "--bounds"]);
    let report = |run: &support::Run| {
        String::from_utf8(run.stdout.clone())
            .expect("UTF-8")
            .replace("\"b/", "\"a/")
    };
    assert_eq!(report(&first), report(&second));
    assert_eq!(tree(&dir.join("a")), tree(&dir.join("b")));
    assert_eq!(commission(&dir, &[]).stdout, commission(&dir, &[]).stdout);
}

#[test]
fn timings_deny_warnings_and_data_move_only_what_they_say() {
    let parse = |bytes: &[u8]| -> Value { serde_json::from_slice(bytes).expect("a receipt") };
    let dir = run_layout("commission_flags", "pass");
    let plain = commission(&dir, &[]);
    let timed = commission(&dir, &["--timings"]);
    assert_eq!((plain.code, timed.code), (0, 0));
    let (plain_receipt, timed_receipt) = (parse(&plain.stdout), parse(&timed.stdout));
    assert_eq!(timed_receipt["receipt_id"], plain_receipt["receipt_id"]);
    assert_eq!(timed_receipt["body"], plain_receipt["body"]);
    for key in ["load_ms", "run_ms", "compare_ms"] {
        assert!(timed_receipt["timings"][key].is_u64(), "{key}");
    }
    assert_eq!(timed_receipt["timings"]["compare_ms"], 0);
    let mut without_timings = timed_receipt.clone();
    without_timings["timings"] = plain_receipt["timings"].clone();
    assert_eq!(without_timings, plain_receipt, "only timings moved");
    // --data synthetic is inside the hashed part.
    let synthetic = parse(&commission(&dir, &["--data", "synthetic"]).stdout);
    assert_eq!(synthetic["body"]["data"], "synthetic");
    assert_eq!(plain_receipt["body"]["data"], "trended");
    assert_ne!(synthetic["receipt_id"], plain_receipt["receipt_id"]);
    let trended = commission(&dir, &["--data", "trended"]);
    assert_eq!(trended.stdout, plain.stdout, "trended is the default");
    // --deny-warnings: a clean run stays yes; a run whose Assert fired answers no, same bytes.
    assert_eq!(commission(&dir, &["--deny-warnings"]).code, 0);
    let warned = signals_layout(
        "commission_warned",
        &[(0, 2.0, 2.0, true), (10, 3.0, 3.0, false)],
    );
    let quiet = commission(&warned, &[]);
    let denied = commission(&warned, &["--deny-warnings"]);
    assert_eq!((quiet.code, denied.code), (0, 1), "{}", denied.transcript());
    assert_eq!(
        quiet.stdout, denied.stdout,
        "the receipt bytes do not change"
    );
    assert!(parse(&quiet.stdout)["body"]["warnings_total"].as_u64() > Some(0));
}

/// A scratch directory's name and how to spoil its run.
type Case = (&'static str, fn(&Path));

#[test]
fn nothing_is_written_on_a_refusal_before_the_first_write() {
    // A file the library refuses, a file the CLI cannot read, a reference the library cannot run.
    let cases: [Case; 3] = [
        ("commission_out_refused_source", |dir| {
            support::commission::edit(dir, "inputs.csv", "10,21,1,true,2,50", "10,21,1,x,2,50");
        }),
        ("commission_out_refused_missing", |dir| {
            fs::remove_file(dir.join("fan.csv")).expect("remove");
        }),
        ("commission_out_refused_sampling", |dir| {
            support::commission::edit(dir, "spec.json", r#""sampling": 5,"#, "");
        }),
    ];
    for (name, prepare) in cases {
        let dir = run_layout(name, "pass");
        fs::create_dir(dir.join("out")).expect("out");
        prepare(&dir);
        let run = commission(&dir, &["--out", "out"]);
        assert_eq!(run.code, 2, "{name}: {}", run.transcript());
        assert_eq!(
            fs::read_dir(dir.join("out")).expect("out").count(),
            0,
            "{name}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_receipt_that_is_an_input_by_link_is_output_is_input_and_nothing_is_written() {
    for (name, link) in [
        ("commission_out_hard_link", true),
        ("commission_out_symbolic_link", false),
    ] {
        let dir = run_layout(name, "pass");
        fs::create_dir(dir.join("out")).expect("out");
        let receipt = dir.join("out/receipt.json");
        if link {
            fs::hard_link(dir.join("inputs.csv"), &receipt).expect("a hard link");
        } else {
            std::os::unix::fs::symlink(dir.join("points.json"), &receipt).expect("a link");
        }
        let before = (
            fs::read(dir.join("inputs.csv")),
            fs::read(dir.join("points.json")),
        );
        let run = commission(&dir, &["--out", "out"]);
        assert_eq!(run.code, 2, "{name}: {}", run.transcript());
        let error = &run.report()["errors"][0];
        assert_eq!(error["code"], "output-is-input", "{name}");
        let read = if link { "inputs.csv" } else { "points.json" };
        assert!(
            error["detail"]
                .as_str()
                .expect("detail")
                .contains(&format!("`{read}`")),
            "{name}: {error}"
        );
        let after = (
            fs::read(dir.join("inputs.csv")),
            fs::read(dir.join("points.json")),
        );
        assert_eq!(before.0.ok(), after.0.ok(), "{name}");
        assert_eq!(before.1.ok(), after.1.ok(), "{name}");
        assert!(!dir.join("out/outputs").exists(), "{name}");
    }
}

#[test]
fn the_output_checks_come_before_the_run_so_an_existing_outputs_beats_a_run_refusal() {
    // The engine would refuse a frame of this run; the output directory is checked first, so
    // the refusal is the directory's and the run never starts.
    let dir = run_layout("commission_out_before_run", "pass");
    support::commission::edit(
        &dir,
        "inputs.csv",
        "10,21,1,true,2,50",
        "10,21,1,true,2,2000",
    );
    assert_eq!(
        commission(&dir, &[]).report()["errors"][0]["code"],
        "frame-refused"
    );
    fs::create_dir_all(dir.join("out/outputs")).expect("an earlier run's outputs");
    let run = commission(&dir, &["--out", "out"]);
    let error = &run.report()["errors"][0];
    assert_eq!(
        (error["code"].as_str(), error["at"].as_str()),
        (Some("write-failed"), Some("outputs"))
    );
}

#[test]
fn a_hostile_name_reaches_no_csv_cell_and_is_escaped_on_stderr() {
    // A device point named with a formula, quotes, a comma and a right-to-left override, and a
    // model with a line separator. None can reach a CSV file: the cells are numbers under a
    // fixed header, and the files are named by index.
    let dir = run_layout("commission_hostile_names", "pass");
    let hostile = "=HYPERLINK(\\\"x\\\"),1\u{202e}";
    support::commission::edit(
        &dir,
        "points.json",
        r#""name": "Setpoint""#,
        &format!(r#""name": "{hostile}""#),
    );
    support::commission::edit(
        &dir,
        "spec.json",
        r#""Setpoint": "Setpoint""#,
        &format!(r#""Setpoint": "{hostile}""#),
    );
    support::commission::edit(
        &dir,
        "spec.json",
        r#""model": "Plant""#,
        "\"model\": \"Pl\u{2028}ant\"",
    );
    fs::create_dir(dir.join("out")).expect("out");
    let run = commission(&dir, &["--out", "out", "--bounds"]);
    assert_eq!(run.code, 0, "{}", run.transcript());
    let receipt = fs::read_to_string(dir.join("out/receipt.json")).expect("receipt");
    // The library's JSON escapes the quotes and writes the override as `\u202e`.
    assert!(
        receipt.contains(r#"=HYPERLINK(\"x\"),1\u202e"#),
        "the receipt keeps the name, JSON-escaped"
    );
    let files = tree(&dir.join("out/outputs"));
    assert_eq!(
        files.len(),
        12,
        "four errors.csv and two outputs' four bound files"
    );
    for (name, bytes) in files {
        let text = String::from_utf8(bytes).expect("UTF-8");
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("x,y"), "{name}");
        for line in lines {
            let (x, y) = line.split_once(',').expect("two cells");
            for cell in [x, y] {
                assert!(
                    cell.bytes()
                        .all(|b| b.is_ascii_digit() || b"+-.e".contains(&b))
                        || ["nan", "inf", "-inf", "-nan"].contains(&cell),
                    "{name}: {line}"
                );
            }
        }
    }
    // A refusal that quotes the model: two references, no --reference, rendered on stderr.
    let spec = fs::read_to_string(dir.join("spec.json")).expect("spec");
    let at = spec.find("    {\n      \"model\"").expect("the reference");
    let end = spec.rfind("\n  ]").expect("the end");
    let two = format!("{},\n{}{}", &spec[..end], &spec[at..end], &spec[end..]);
    fs::write(dir.join("two.json"), two).expect("write");
    let refused = oce_in(
        &dir,
        &["--render", "always", "verify", "--commission", "two.json"],
        b"",
    );
    assert_eq!(refused.code, 2, "{}", refused.transcript());
    let detail = refused.report()["errors"][0]["detail"]
        .as_str()
        .expect("detail")
        .to_owned();
    assert!(
        detail.contains("Pl\u{2028}ant"),
        "the report carries the name exactly"
    );
    let stderr = String::from_utf8(refused.stderr).expect("UTF-8");
    assert!(stderr.contains("Pl\\u{2028}ant"), "{stderr}");
    assert!(!stderr.contains('\u{2028}'), "{stderr}");
}

#[test]
fn render_auto_follows_the_terminal_and_a_closed_stdout_is_an_error() {
    use oce_cli::{Exit, Streams, run};
    use std::io::{self, Write};
    struct Closed;
    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
    }
    let dir = run_layout("commission_streams", "fail");
    let spec = dir.join("spec.json");
    let spec = spec.to_str().expect("a UTF-8 scratch path");
    let args = ["oce", "verify", "--commission", spec];
    let once = |terminal: bool, stdout: &mut dyn Write| {
        let mut stdin: &[u8] = b"";
        let mut stderr = Vec::new();
        let mut streams = Streams {
            stdin: &mut stdin,
            stdout,
            stderr: &mut stderr,
            stderr_is_terminal: terminal,
        };
        (run(args.iter().copied(), &mut streams), stderr)
    };
    let (mut plain, mut shown) = (Vec::new(), Vec::new());
    let (quiet_exit, quiet_err) = once(false, &mut plain);
    let (shown_exit, shown_err) = once(true, &mut shown);
    assert_eq!((quiet_exit, shown_exit), (Exit::No, Exit::No));
    assert!(
        plain == golden_receipt("fail") && shown == plain,
        "stdout is the receipt either way"
    );
    assert!(quiet_err.is_empty());
    let rendered = String::from_utf8(shown_err).expect("UTF-8");
    assert!(
        rendered.ends_with(": fail; the receipt is on stdout\n"),
        "{rendered}"
    );
    // A stdout that cannot be written turns the verdict into an error.
    let (closed, rendered) = once(true, &mut Closed);
    assert_eq!(closed, Exit::Error);
    assert!(
        String::from_utf8(rendered)
            .expect("UTF-8")
            .contains("stdout could not be written"),
    );
}
