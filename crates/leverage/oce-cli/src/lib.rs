#![forbid(unsafe_code)]
//! `oce-cli`: the `oce` command-line tool (authoring note `docs/design/authoring-layer.md`, "CLI",
//! decisions 9 and 20; GitHub issue #489; vocabulary note `docs/design/vocabulary.md`, decision
//! 12, GitHub issue #500; vectors note `docs/design/vectors.md`, "CLI", decision 34, GitHub issue
//! #522).
//!
//! The binary (`src/main.rs`) is a thin shell over [`run`], which parses the arguments with `clap`
//! and dispatches to one of six commands over `oce-author`, `oce-vocab`, `oce-verify` and the
//! facade:
//!
//! - `emit`: an authored sequence (text or wire) to flat CXF, after both validation stages;
//! - `validate`: both validation stages, or stage 1 alone, as a JSON report;
//! - `fmt`: the canonical text form, `--check` and an atomic `--write`;
//! - `catalog`: the classes, their parameters and resolved ports, and the block vocabulary:
//!   descriptions, declared units and quantities, ranges, typed notes and upstream links;
//! - `vocab`: the text-form vocabulary, the unit scale factors and every code the tool can
//!   report;
//! - `verify`: a vector set replayed against a subject, the receipt on stdout.
//!
//! **Exit codes** follow the one `oce` rule: `0` yes (clean, every scenario passed), `1` no (an
//! error finding, a non-canonical file, a class whose ports do not resolve, a failed or `no_data`
//! verdict), `2` error (a usage error, input that cannot be read or parsed, an over-size input, an
//! I/O failure, a run that reached no verdict). **Output** is deterministic: stdout carries
//! exactly one document — the command's product (CXF, canonical text, a catalog listing, a
//! receipt) or its JSON report (`report.rs`, documented in the crate README) — with sorted keys, LF
//! line endings and no timestamps; a human-readable rendering goes to stderr only when stderr is a
//! terminal or `--render always` asks for it. Nothing here panics on hostile input, and no path
//! the tool prints is one it did not receive as an argument.
//!
//! The `pub` items ([`Exit`], [`Streams`], [`run`], [`REPORT_SCHEMA`]) are exposed for the crate's
//! own binary and integration tests; they are not a supported library API.
//!
//! The crate sits above the facade: it depends on `oce-api` and the leverage crates `oce-author`,
//! `oce-vocab` and `oce-verify` only, and every engine it loads is a throwaway of the validator's
//! or the verifier's own.

mod codes;
mod commands;
mod input;
mod report;
mod verify_codes;

use std::ffi::OsString;
use std::io::{IsTerminal, Read, Write};

use clap::{Args, Parser, Subcommand, ValueEnum};

pub use report::REPORT_SCHEMA;

/// The process exit status of one `oce` run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// `0`: the answer is yes (clean, canonical, resolved).
    Yes,
    /// `1`: the answer is no (an error finding, a non-canonical file, unresolved ports).
    No,
    /// `2`: the question could not be answered (usage, read, parse, size or I/O failure).
    Error,
}

impl Exit {
    /// The numeric exit status.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Yes => 0,
            Self::No => 1,
            Self::Error => 2,
        }
    }
}

/// Author, check and inspect Open Control Engine sequences.
///
/// Exit status: 0 yes (clean), 1 no (error findings, a non-canonical file), 2 error (usage, read,
/// parse or size failure). Reports are JSON on stdout; see the crate README for the contract.
#[derive(Debug, Parser)]
#[command(name = "oce", version, propagate_version = true)]
struct Cli {
    /// When to write a human-readable rendering of the report to stderr.
    #[arg(long, value_enum, default_value_t = Render::Auto, global = true)]
    render: Render,
    #[command(subcommand)]
    command: Command,
}

/// When the human-readable rendering goes to stderr.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Render {
    // Only when stderr is a terminal. (Plain comments: a doc comment here would become clap
    // help text with a whitespace-only line.)
    Auto,
    // Always.
    Always,
    // Never.
    Never,
}

/// The two authored forms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum Form {
    // The text form (`.oce`).
    Text,
    // The strict-JSON wire form (`.json`).
    Wire,
}

/// Where an authored sequence comes from.
#[derive(Debug, Args)]
pub(crate) struct Source {
    /// The input file (`.oce` text, `.json` wire), or `-` for stdin.
    pub(crate) file: OsString,
    /// The input form; overrides the extension, and is required for stdin.
    #[arg(long, value_enum)]
    pub(crate) from: Option<Form>,
}

/// The forms a `verify` subject is read in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum SubjectForm {
    // Flat CXF, given to the engine untouched (`.jsonld`, `.cxf.json`).
    Cxf,
    // The text form (`.oce`), validated and emitted first.
    Text,
    // The strict-JSON wire form (any other `.json`), validated and emitted first.
    Wire,
}

/// What `verify` runs and how it answers.
#[derive(Debug, Args)]
pub(crate) struct VerifyArgs {
    /// The vector set (`oce/vectors/v1`); mutant paths resolve against its directory.
    pub(crate) vectors: OsString,
    /// The sequence the vectors check: CXF (`.jsonld`, `.cxf.json`), text (`.oce`) or wire
    /// (`.json`).
    #[arg(long)]
    pub(crate) subject: OsString,
    /// The subject's form; overrides the name.
    #[arg(long, value_enum)]
    pub(crate) subject_form: Option<SubjectForm>,
    /// Write the receipt to DIR/receipt.json (atomically) and print the report on stdout.
    #[arg(long, value_name = "DIR")]
    pub(crate) out: Option<OsString>,
    /// Exit 1 when a mutant survived or the set lists none; no receipt byte changes.
    #[arg(long)]
    pub(crate) require_mutants_caught: bool,
    /// Exit 1 when the subject warned at load or a scenario left a warning unmatched; no receipt
    /// byte changes.
    #[arg(long)]
    pub(crate) deny_warnings: bool,
    /// Measure the run into the receipt's timings, which receipt_id never covers.
    #[arg(long)]
    pub(crate) timings: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate a sequence, then write it as flat CXF.
    ///
    /// Runs both validation stages and writes nothing on an error finding (exit 1, the report on
    /// stdout). Without -o, the CXF document is stdout.
    Emit {
        #[command(flatten)]
        source: Source,
        /// The instance namespace every IRI starts with; must end in `#`, `/` or `:`.
        #[arg(long)]
        namespace: Option<String>,
        /// Write the CXF here (atomically) and print the report on stdout; `-` is stdout.
        #[arg(short = 'o', long = "output")]
        output: Option<OsString>,
    },
    /// Validate a sequence and print the findings as a JSON report.
    ///
    /// Stage 1 judges the model against the catalog; stage 2 (only when stage 1 is clean) loads
    /// the emitted CXF in a throwaway engine. Text input carries spans.
    Validate {
        #[command(flatten)]
        source: Source,
        /// Run stage 1 only (no engine load).
        #[arg(long)]
        author_side: bool,
    },
    /// Print the canonical text form of a sequence.
    ///
    /// Wire input is converted to text. --check answers whether the file is already canonical in
    /// its own form; --write replaces a text file atomically.
    Fmt {
        #[command(flatten)]
        source: Source,
        /// Print a report instead of the text; exit 1 when the input is not canonical.
        #[arg(long, conflicts_with = "write")]
        check: bool,
        /// Rewrite the text file (a temporary file, then a rename; a hard link keeps the old bytes).
        #[arg(long)]
        write: bool,
    },
    /// List the block classes, or describe one: its ports, parameters, units, ranges and notes.
    Catalog {
        /// A class path (`CDL.Reals.Hysteresis`); every class when absent.
        class: Option<String>,
        /// Print the JSON report instead of text.
        #[arg(long)]
        json: bool,
        /// A parameter value for port resolution (`nin=3`, `k_2=1.5`); repeatable.
        #[arg(long = "param", value_name = "NAME=VALUE", requires = "class")]
        params: Vec<String>,
    },
    /// Print the text-form vocabulary, the unit scale factors and every code oce reports.
    Vocab {
        /// Print the JSON report instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Replay a vector set against a subject and print the receipt.
    ///
    /// Stdout is the receipt when the run reaches a verdict: exit 0 when every scenario passed, 1
    /// when one failed or the set checked nothing. Anything that prevents a verdict is exit 2 with
    /// the JSON report. With --out the receipt goes to DIR/receipt.json and the report to stdout.
    Verify(VerifyArgs),
}

/// The streams one run reads and writes, and whether stderr is a terminal.
pub struct Streams<'a> {
    /// Standard input (`-`).
    pub stdin: &'a mut dyn Read,
    /// Standard output: exactly one document per run.
    pub stdout: &'a mut dyn Write,
    /// Standard error: clap's messages and the optional human-readable rendering.
    pub stderr: &'a mut dyn Write,
    /// Whether stderr is a terminal (`--render auto`).
    pub stderr_is_terminal: bool,
}

impl Streams<'_> {
    /// Whether stderr is a terminal, read from the process.
    #[must_use]
    pub fn process_stderr_is_terminal() -> bool {
        std::io::stderr().is_terminal()
    }
}

/// Run `oce` with `args` (the first is the program name, which is ignored) over `streams`. Never panics; every
/// failure is an [`Exit::Error`] with its report or clap's message.
pub fn run<I, T>(args: I, streams: &mut Streams<'_>) -> Exit
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    // The program is always named `oce`, whatever path or `.exe` suffix started it, so usage
    // lines and help are the same bytes on every platform.
    let args =
        std::iter::once(OsString::from("oce")).chain(args.into_iter().skip(1).map(Into::into));
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => return clap_exit(&error, streams),
    };
    let render = match cli.render {
        Render::Auto => streams.stderr_is_terminal,
        Render::Always => true,
        Render::Never => false,
    };
    let outcome = match cli.command {
        Command::Emit {
            source,
            namespace,
            output,
        } => commands::emit::run(&source, namespace.as_deref(), output.as_deref(), streams),
        Command::Validate {
            source,
            author_side,
        } => commands::validate::run(&source, author_side, streams),
        Command::Fmt {
            source,
            check,
            write,
        } => commands::fmt::run(&source, check, write, streams),
        Command::Catalog {
            class,
            json,
            params,
        } => commands::catalog::run(class.as_deref(), json, &params),
        Command::Vocab { json } => commands::vocab::run(json),
        Command::Verify(args) => commands::verify::run(&args),
    };
    commands::finish(outcome, render, streams)
}

/// Print clap's help, version or usage error and map it to an exit status: help and version are
/// stdout and `0`; every other clap error is stderr and `2`.
fn clap_exit(error: &clap::Error, streams: &mut Streams<'_>) -> Exit {
    let text = error.render().to_string();
    if error.use_stderr() {
        // A failed write to stderr leaves nothing else to report; the status still says error.
        let _ = streams.stderr.write_all(text.as_bytes());
        let _ = streams.stderr.flush();
        Exit::Error
    } else if streams
        .stdout
        .write_all(text.as_bytes())
        .and_then(|()| streams.stdout.flush())
        .is_ok()
    {
        Exit::Yes
    } else {
        Exit::Error
    }
}
