//! `oce verify --commission <SPEC>`: one reference of a commissioning spec run against its trends,
//! and the receipt (commissioning note, "CLI", decisions 21 and 33 to 37; issue #664).
//!
//! Order, the first refusal winning, every refusal exit `2` with the `oce-report/1` report and
//! no receipt:
//!
//! 0. with `--out DIR`, before anything is read, a `DIR/receipt.json` that is a symbolic link,
//!    dangling or not, a directory or another non-file is `output-not-a-file`
//!    ([`super::commission_out::target`]; issue #758, ruled by the owner on 2026-11-22);
//! 1. the spec, bounded at `SpecLimits::MAX.max_bytes` (4 MiB) from its metadata, then read by
//!    `oce_verify::CommissioningSpec::from_json_bytes` (a refusal under the spec reader's own
//!    code, at its line and column);
//! 2. the reference: `--reference N`, or the only one; a spec with several and no flag is
//!    `reference-required`, its detail listing each index with its `sequence` and `model`
//!    (decision 35);
//! 3. the reference's own completeness, decided by the library before any other file is read:
//!    an index past the end (`reference-out-of-range`), no `cxf` or no `window`
//!    (`reference-incomplete`), no `sampling` (`sampling-required`);
//! 4. its files, each resolved inside the directory of the spec's path as given and bounded from
//!    its metadata ([`super::commission_files`]);
//! 5. with `--out DIR`, the output checks ([`super::commission_out::check`]);
//! 6. `oce_verify::run_commissioning`, which binds, replays, masks and compares, every refusal
//!    its own code;
//! 7. with `--out DIR`, the curves and then the receipt written ([`super::commission_out::write`]).
//!
//! A run that reaches a verdict prints the receipt on stdout (`Receipt::to_canonical_json`), the
//! bytes the library issued: exit `0` when every compared output passed, `1` when one failed or
//! the run is `no_data` (decision 21). With `--out DIR` stdout is the report instead, with
//! `passed`, `status`, `receipt_id` and `output`, as a vector run's `--out` prints it, and
//! `curves`: per compared output its `index`, its canonical path (`output`) and the `files`
//! written for it, so no reader infers anything from a file that is absent. A write that fails
//! part-way is `write-failed`, and its report's `curves` lists the files written before it.
//!
//! `--deny-warnings` (decision 33) makes exit `1` when the receipt counts a load warning or a
//! run-time warning (`load_warnings_total`, `warnings_total`), and changes nothing else.
//! `--timings` fills the header's timings, which `receipt_id` never covers: `load_ms` is reading
//! every file, `run_ms` the whole `run_commissioning` call, and `compare_ms` stays `0`. `--data`
//! is the receipt's `data` label (decision 41), inside `receipt_id`. `--render` changes stderr
//! only (decision 36).

use std::path::{Path, PathBuf};
use std::time::Instant;

use oce_verify::{
    CommissionOptions, CommissionRun, CommissioningSpec, Receipt, ReceiptBody, SpecInputs,
    SpecLimits, SpecReference, Timings, TrendData, run_commissioning,
};
use serde_json::{Value, json};

use super::Outcome;
use super::commission_files::{self, Files};
use super::commission_out::{self, RECEIPT};
use super::verify::whole_ms;
use crate::report::{Facts, Report};
use crate::{CommissionArgs, DataLabel, Exit, codes, input};

/// Run `verify --commission`.
pub(crate) fn run(args: &CommissionArgs) -> Outcome {
    let started = Instant::now();
    let mut report = Report::new("verify", Some(input::display_name(&args.spec)));
    if let Some(dir) = &args.out
        && let Err(refusal) = commission_out::target(dir)
    {
        report.error_at(
            refusal.code,
            refusal.detail,
            None,
            &Facts::about(&refusal.at),
        );
        return refused(report, "the output was refused before anything was read");
    }
    let spec_path = Path::new(&args.spec);
    let bound = usize::try_from(SpecLimits::MAX.max_bytes).unwrap_or(usize::MAX);
    let bytes = match input::read_file(spec_path, bound) {
        Ok(bytes) => bytes,
        Err((code, detail)) => {
            report.error(code, detail, None);
            return refused(report, "the commissioning spec could not be read");
        }
    };
    let spec = match CommissioningSpec::from_json_bytes(&bytes, &SpecLimits::default()) {
        Ok(spec) => spec,
        Err(error) => {
            report.spec_error(&error);
            return refused(report, "the commissioning spec was refused");
        }
    };
    let Some(index) = reference_index(&spec, args.reference, &mut report) else {
        return refused(
            report,
            "the spec has several references and none was chosen",
        );
    };
    let mut options = CommissionOptions::default().with_data(match args.data {
        DataLabel::Trended => TrendData::Trended,
        DataLabel::Synthetic => TrendData::Synthetic,
    });
    options.bounds = args.bounds;
    let Some(reference) = spec
        .references()
        .get(index)
        .filter(|reference| runnable(&spec, reference))
    else {
        // The library refuses this reference before it opens a byte: no file is read for it.
        return finish_run(
            args,
            report,
            run_commissioning(&spec, index, &SpecInputs::default(), &options),
            (started.elapsed(), started.elapsed()),
        );
    };
    let place = format!("references[{index}]");
    let named = commission_files::named(index, reference);
    let Files { inputs, read } = match commission_files::read(spec_path, &place, &named) {
        Ok(files) => files,
        Err(refusal) => {
            report.error_at(
                refusal.code,
                refusal.detail,
                None,
                &Facts {
                    cause: refusal.cause,
                    ..Facts::about(&refusal.at)
                },
            );
            return refused(report, "a file the reference names could not be read");
        }
    };
    if let Some(dir) = &args.out {
        let mut checked: Vec<(String, PathBuf)> = Vec::with_capacity(read.len() + 1);
        if let Ok(spec_file) = std::fs::canonicalize(spec_path) {
            checked.push((input::display_name(&args.spec), spec_file));
        }
        checked.extend(
            named
                .iter()
                .zip(read)
                .map(|(file, canonical)| (file.path.to_owned(), canonical)),
        );
        if let Err(refusal) = commission_out::check(dir, &checked) {
            report.error_at(
                refusal.code,
                refusal.detail,
                None,
                &Facts::about(&refusal.at),
            );
            return refused(report, "the output directory was refused");
        }
    }
    let loaded = started.elapsed();
    let running = Instant::now();
    let run = run_commissioning(&spec, index, &inputs, &options);
    finish_run(args, report, run, (loaded, running.elapsed()))
}

/// Whether the library would run `reference` as far as its first file: a `cxf`, a `window` and a
/// `sampling`, its own or the spec's. Anything else the library refuses before it reads a byte.
fn runnable(spec: &CommissioningSpec, reference: &SpecReference) -> bool {
    reference.cxf().is_some()
        && reference.window().is_some()
        && spec.sampling_for(reference).is_some()
}

/// The reference to run: `--reference N`, else the only one (index `0` also for a spec with
/// none, which the library then refuses as out of range); `None`, with `reference-required`
/// recorded, for a spec with several and no flag.
fn reference_index(
    spec: &CommissioningSpec,
    chosen: Option<usize>,
    report: &mut Report,
) -> Option<usize> {
    let references = spec.references();
    match chosen {
        Some(index) => Some(index),
        None if references.len() < 2 => Some(0),
        None => {
            let listed: Vec<String> = references
                .iter()
                .enumerate()
                .map(|(index, reference)| {
                    format!(
                        "{index} (sequence `{}`, model `{}`)",
                        reference.sequence(),
                        reference.model()
                    )
                })
                .collect();
            report.error_at(
                codes::REFERENCE_REQUIRED,
                format!(
                    "the spec has {} references; choose one with --reference N: {}",
                    references.len(),
                    listed.join(", ")
                ),
                None,
                &Facts::about("references"),
            );
            None
        }
    }
}

/// The outcome of a `run_commissioning` call: the verdict, or its refusal. `took` is the time
/// reading the files and the time of the call, for `--timings`.
fn finish_run(
    args: &CommissionArgs,
    mut report: Report,
    run: Result<CommissionRun, oce_verify::VerifyError>,
    took: (std::time::Duration, std::time::Duration),
) -> Outcome {
    let mut run = match run {
        Ok(run) => run,
        Err(error) => {
            report.verify_error(&error);
            return refused(report, "the run reached no verdict");
        }
    };
    if args.timings {
        run.receipt.header.timings = Timings {
            load_ms: whole_ms(took.0),
            run_ms: whole_ms(took.1),
            compare_ms: 0,
        };
    }
    answer(args, &run, report)
}

/// The exit status and stdout of a run that reached a verdict.
fn answer(args: &CommissionArgs, run: &CommissionRun, mut report: Report) -> Outcome {
    let (status, exit) = verdict(args, &run.receipt);
    let document = run.receipt.to_canonical_json().into_bytes();
    let Some(dir) = &args.out else {
        report.verdict(format!("{status}; the receipt is on stdout"));
        return Outcome::verdict(exit, report, document);
    };
    let shown = commission_out::shown(dir, RECEIPT);
    match commission_out::write(dir, &run.curves, &document) {
        Ok(written) => {
            report.set("curves", curves_json(written));
            report.set("passed", Value::Bool(run.receipt.header.passed));
            report.set("status", Value::from(status));
            report.set(
                "receipt_id",
                Value::from(run.receipt.header.receipt_id.clone()),
            );
            report.set("output", Value::from(shown.clone()));
            report.verdict(format!("{status}; the receipt is written to {shown}"));
            Outcome::with_exit(exit, report)
        }
        Err(refusal) => {
            // The files written before the failure, so no reader has to walk the directory.
            report.set("curves", curves_json(refusal.written));
            report.error_at(
                refusal.code,
                refusal.detail,
                None,
                &Facts::about(&refusal.at),
            );
            refused(report, "the output could not be written")
        }
    }
}

/// The report's `curves`: per output written, its `files`, `index` and canonical path.
fn curves_json(written: Vec<commission_out::Written>) -> Value {
    Value::Array(
        written
            .into_iter()
            .map(|output| {
                json!({
                    "files": output.files,
                    "index": output.index,
                    "output": output.output,
                })
            })
            .collect(),
    )
}

/// The run's status and the exit status, `--deny-warnings` applied.
fn verdict(args: &CommissionArgs, receipt: &Receipt) -> (&'static str, Exit) {
    let ReceiptBody::Commissioning(body) = &receipt.body else {
        return ("unknown", Exit::Error);
    };
    let warned = body.load_warnings_total > 0 || body.warnings_total > 0;
    let exit = if !receipt.header.passed || (args.deny_warnings && warned) {
        Exit::No
    } else {
        Exit::Yes
    };
    (body.status.as_str(), exit)
}

/// A run that stopped before a verdict: exit `2`, the report on stdout, no receipt.
fn refused(mut report: Report, why: &str) -> Outcome {
    report.verdict(format!("refused: {why}; no receipt"));
    Outcome::judged(report)
}
