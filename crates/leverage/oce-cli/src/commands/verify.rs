//! `oce verify`: a vector set replayed against a subject, and the receipt (vectors note, "CLI";
//! decisions 26, 30, 31 and 34; GitHub issue #522).
//!
//! Order, the first refusal winning:
//!
//! 1. the vector file, bounded at `VerifyLimits::max_bytes` (4 MiB) from its metadata, then read
//!    by `oce_verify::VectorSet::from_json_bytes`;
//! 2. the subject, in its form ([`super::verify_subject`]);
//! 3. the mutants, each resolved, held inside the vector file's directory and read once
//!    ([`super::verify_mutants`]);
//! 4. `oce_verify::run_vectors`, which binds, runs and compares.
//!
//! A run that reaches a verdict prints the receipt on stdout (`Receipt::to_canonical_json`): exit
//! `0` when every scenario passed, `1` when one failed or the set checked nothing. Every refusal
//! is exit `2` with the `oce-report/1` report and no receipt. With `--out DIR` the receipt is
//! written atomically to `DIR/receipt.json` (`atomic_write`: a fresh temporary file beside it,
//! synced, then renamed over its directory entry, so no reader ever sees part of a receipt and no
//! link there is written through) and stdout is the report, with `passed`, `status`, `receipt_id`
//! and `output`; on exit `2` nothing is written. `DIR` must exist: it is never created. Two
//! guards hold the output (issue #758, ruled by the owner on 2026-11-22):
//!
//! - before anything is read, a `DIR/receipt.json` that is a symbolic link, dangling or not, a
//!   directory or another non-file is `output-not-a-file` (`atomic_write::target_accepted`); a
//!   regular file there is replaced;
//! - after the reads and before the run, a `DIR/receipt.json` that is the vector file, the
//!   subject or a mutant, by path, through a link or (on Unix) as a hard link, is
//!   `output-is-input` (`file_identity::input_at`), so a run never replaces what it read.
//!
//! The two exit-only flags change the exit status and never a receipt byte (decisions 12 and 30):
//! `--require-mutants-caught` makes exit `1` when a mutant survived or the set lists none (owner
//! ruling of 2026-11-11), and `--deny-warnings` makes exit `1` when the subject warned at load or a
//! scenario left a warning unmatched.
//!
//! `--timings` (decision 31; owner ruling of 2026-11-11) fills the header's timings, which
//! `receipt_id` never covers: `load_ms` is reading every file and emitting a text or wire subject,
//! `run_ms` the whole `run_vectors` call (binding, every run and every comparison), and
//! `compare_ms` stays `0`, because comparing cannot be timed apart from running outside
//! `oce-verify`, which measures nothing.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use oce_verify::{
    Receipt, ReceiptBody, ScenarioStatus, Timings, VectorInputs, VectorSet, VerifyLimits,
    VerifyOptions, run_vectors,
};
use serde_json::Value;

use super::{Outcome, input_at, target_accepted, verify_mutants, verify_subject, write_atomically};
use crate::report::{Facts, Report};
use crate::{Exit, VectorArgs, codes, input};

/// The file the receipt is written to, inside `DIR`.
const RECEIPT: &str = "receipt.json";

/// Run `verify` over a vector set.
pub(crate) fn run(args: &VectorArgs) -> Outcome {
    let started = Instant::now();
    let mut report = Report::new("verify", Some(input::display_name(&args.vectors)));
    report.set("subject", Value::from(input::display_name(&args.subject)));
    if let Some(dir) = &args.out
        && let Err((code, detail)) = target_accepted(&receipt_path(dir), &shown(dir))
    {
        report.error_at(code, detail, None, &Facts::about(RECEIPT));
        return refused(report, "the output was refused before anything was read");
    }
    let limits = VerifyLimits::default();
    let vectors = Path::new(&args.vectors);
    let bound = usize::try_from(limits.max_bytes).unwrap_or(usize::MAX);
    let bytes = match input::read_file(vectors, bound) {
        Ok(bytes) => bytes,
        Err((code, detail)) => {
            report.error(code, detail, None);
            return refused(report, "the vector file could not be read");
        }
    };
    let set = match VectorSet::from_json_bytes(&bytes, &limits) {
        Ok(set) => set,
        Err(error) => {
            report.vector_error(&error);
            return refused(report, "the vector file was refused");
        }
    };
    let Some(subject) = verify_subject::load(args, &mut report) else {
        return refused(report, "the subject could not be read or emitted");
    };
    let mutants = match verify_mutants::read(&set, vectors) {
        Ok(mutants) => mutants,
        Err(refusal) => {
            report.error_at(
                refusal.code,
                refusal.detail,
                None,
                &Facts::about(&refusal.at),
            );
            return refused(report, "a mutant could not be read");
        }
    };
    if let Some(dir) = &args.out {
        let mut read: Vec<(String, PathBuf)> = vec![
            (
                input::display_name(&args.vectors),
                PathBuf::from(&args.vectors),
            ),
            (
                input::display_name(&args.subject),
                PathBuf::from(&args.subject),
            ),
        ];
        read.extend(
            mutants
                .iter()
                .map(|mutant| (mutant.path.clone(), mutant.canonical.clone())),
        );
        if let Some(name) = input_at(&receipt_path(dir), &read) {
            let detail = format!(
                "`{}` is `{name}`, which the run reads; nothing is written",
                shown(dir)
            );
            report.error_at(codes::OUTPUT_IS_INPUT, detail, None, &Facts::about(RECEIPT));
            return refused(report, "the output is a file the run reads");
        }
    }
    let pairs: Vec<(&str, &[u8])> = mutants
        .iter()
        .map(|mutant| (mutant.path.as_str(), mutant.bytes.as_slice()))
        .collect();
    let loaded = started.elapsed();
    let running = Instant::now();
    let run = run_vectors(
        &set,
        &VectorInputs::new(&subject).with_mutants(&pairs),
        &VerifyOptions::default(),
    );
    let ran = running.elapsed();
    let mut receipt = match run {
        Ok(run) => run.receipt,
        Err(error) => {
            report.verify_error(&error);
            return refused(report, "the run reached no verdict");
        }
    };
    if args.timings {
        receipt.header.timings = Timings {
            load_ms: whole_ms(loaded),
            run_ms: whole_ms(ran),
            compare_ms: 0,
        };
    }
    answer(args, &receipt, report)
}

/// The exit status and stdout of a run that reached a verdict.
fn answer(args: &VectorArgs, receipt: &Receipt, mut report: Report) -> Outcome {
    let (status, exit) = verdict(args, receipt);
    let document = receipt.to_canonical_json().into_bytes();
    let Some(dir) = &args.out else {
        report.verdict(format!("{status}; the receipt is on stdout"));
        return Outcome::verdict(exit, report, document);
    };
    let shown = shown(dir);
    match write_atomically(&receipt_path(dir), &document) {
        Ok(()) => {
            report.set("passed", Value::Bool(receipt.header.passed));
            report.set("status", Value::from(status));
            report.set("receipt_id", Value::from(receipt.header.receipt_id.clone()));
            report.set("output", Value::from(shown.clone()));
            report.verdict(format!("{status}; the receipt is written to {shown}"));
            Outcome::with_exit(exit, report)
        }
        Err((code, detail)) => {
            report.error(code, detail, None);
            refused(report, "the receipt could not be written")
        }
    }
}

/// `DIR/receipt.json`, joined the platform's way.
fn receipt_path(dir: &OsStr) -> PathBuf {
    Path::new(dir).join(RECEIPT)
}

/// `DIR/receipt.json` as the report names it: `DIR` as given, then `/receipt.json` on every
/// platform, so a transcript is one text everywhere.
fn shown(dir: &OsStr) -> String {
    format!(
        "{}/{RECEIPT}",
        input::display_name(dir).trim_end_matches('/')
    )
}

/// The set's status and the exit status, the exit-only flags applied.
fn verdict(args: &VectorArgs, receipt: &Receipt) -> (&'static str, Exit) {
    let ReceiptBody::Vectors(body) = &receipt.body else {
        return ("unknown", Exit::Error);
    };
    let status = match body.status {
        ScenarioStatus::Pass => "pass",
        ScenarioStatus::Fail => "fail",
        ScenarioStatus::NoData => "no_data",
    };
    let warned = body.load_warnings_total > 0 || body.totals.warnings > 0;
    // Owner ruling of 2026-11-11: a set that lists no mutant cannot show its mutants caught.
    let uncaught = body
        .mutants
        .as_ref()
        .is_none_or(|section| section.survived > 0);
    let exit = if !receipt.header.passed
        || (args.deny_warnings && warned)
        || (args.require_mutants_caught && uncaught)
    {
        Exit::No
    } else {
        Exit::Yes
    };
    (status, exit)
}

/// A duration in whole milliseconds, saturating.
pub(crate) fn whole_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// A run that stopped before a verdict: exit `2`, the report on stdout, nothing written.
fn refused(mut report: Report, why: &str) -> Outcome {
    report.verdict(format!("refused: {why}; no receipt"));
    Outcome::judged(report)
}
