#![forbid(unsafe_code)]
//! `oce-cli`: the `oce` command-line tool (authoring note `docs/design/authoring-layer.md`, "CLI",
//! decisions 9 and 20; GitHub issue #489; vocabulary note `docs/design/vocabulary.md`, decision
//! 12, GitHub issue #500; vectors note `docs/design/vectors.md`, "CLI", decision 34, GitHub issue
//! #522; content-id note `docs/design/content-id.md`, GitHub issue #513; importer note
//! `docs/design/importer.md`, GitHub issue #514; diff note `docs/design/diff.md`, "CLI", decisions
//! 1 and 16, GitHub issue #668; commissioning note `docs/design/commissioning.md`, "CLI",
//! decisions 21 and 33 to 37, GitHub issue #664).
//!
//! The binary (`src/main.rs`) is a thin shell over [`run`], which parses the arguments with `clap`
//! and dispatches to one of nine commands over `oce-author`, `oce-vocab`, `oce-verify`, `oce-diff`
//! and the facade:
//!
//! - `emit`: an authored sequence (text or wire) to flat CXF, after both validation stages;
//! - `validate`: both validation stages, or stage 1 alone, as a JSON report;
//! - `fmt`: the canonical text form, `--check` and an atomic `--write`;
//! - `catalog`: the classes, their parameters and resolved ports, and the block vocabulary:
//!   descriptions, declared units and quantities, ranges, typed notes and upstream links;
//! - `vocab`: the text-form vocabulary, the unit scale factors and every code the tool can
//!   report;
//! - `verify`: a vector set replayed against a subject, or (`--commission`) one reference of a
//!   commissioning spec run against its trends, the receipt on stdout;
//! - `id`: the canonical content id of a CXF document or an authored sequence, alone on stdout;
//! - `import`: a flat CXF document into the authoring form, canonical text or wire;
//! - `diff`: the structural diff of two authoring models or two CXF documents.
//!
//! **Exit codes** follow the one `oce` rule: `0` yes (clean, every scenario passed, nothing
//! differs), `1` no (an error finding, a non-canonical file, a class whose ports do not resolve, a
//! failed or `no_data` verdict, any difference), `2` error (a usage error, input that cannot be
//! read, parsed or loaded, an over-size input, an I/O failure, a run that reached no verdict, an
//! input that has no content id, a document the importer refuses). **Output** is deterministic:
//! stdout carries exactly one document — the command's product (CXF, canonical text, a wire
//! document, a catalog listing, a receipt, a content id, a diff's Markdown table) or its JSON
//! report (`report.rs`, documented in the crate README) — with sorted keys, LF
//! line endings and no timestamps; a human-readable rendering goes to stderr only when stderr is a
//! terminal or `--render always` asks for it, with every format and control character written as
//! an escape. Nothing here panics on hostile input, and no path the tool prints is one it did not
//! receive as an argument.
//!
//! The `pub` items ([`Exit`], [`Streams`], [`run`], [`REPORT_SCHEMA`]) are exposed for the crate's
//! own binary and integration tests; they are not a supported library API.
//!
//! The crate sits above the facade: it depends on `oce-api` and the leverage crates `oce-author`,
//! `oce-vocab`, `oce-verify` and `oce-diff` only, and every engine it loads is a throwaway: the
//! validator's, the verifier's, or one `id` or `diff` loads to read a CXF document.

mod codes;
mod commands;
mod commission_codes;
mod diff_codes;
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
/// Exit status: 0 yes (clean), 1 no (error findings, a non-canonical file, a difference), 2 error
/// (usage, read, parse or size failure). Reports are JSON on stdout; see the crate README for the
/// contract.
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

/// The label a commissioning receipt gives its trend files (`--data`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum DataLabel {
    // Recorded by a real controller.
    Trended,
    // Generated, for instance from the engine and perturbed.
    Synthetic,
}

/// The two forms of `verify`, as its help and its usage errors print them: clap's own usage line
/// would write the two forms as one (`--commission <SPEC> [VECTORS]`), a combination it refuses.
const VERIFY_USAGE: &str = "oce verify [OPTIONS] --subject <SUBJECT> <VECTORS>\n       \
oce verify [OPTIONS] --commission <SPEC>";

/// The `verify` arguments only a vector run reads. A commissioning flag conflicts with each of
/// them as well as requiring `--commission`: clap drops a requirement whose argument conflicts
/// with one that is present, so `requires` alone would let `--reference` pass beside a vector set.
const VECTOR_ONLY: [&str; 4] = [
    "vectors",
    "subject",
    "subject_form",
    "require_mutants_caught",
];

/// What `verify` runs and how it answers: a vector set against a subject, or, with
/// `--commission`, one reference of a commissioning spec against its trends.
#[derive(Debug, Args)]
pub(crate) struct VerifyArgs {
    /// The vector set (`oce/vectors/v1`); mutant paths resolve against its directory.
    #[arg(required_unless_present = "commission", conflicts_with = "commission")]
    pub(crate) vectors: Option<OsString>,
    /// The sequence the vectors check: CXF (`.jsonld`, `.cxf.json`), text (`.oce`) or wire
    /// (`.json`).
    #[arg(
        long,
        required_unless_present = "commission",
        conflicts_with = "commission"
    )]
    pub(crate) subject: Option<OsString>,
    /// The subject's form; overrides the name.
    #[arg(long, value_enum, conflicts_with = "commission")]
    pub(crate) subject_form: Option<SubjectForm>,
    /// Run a commissioning spec (`oce/commissioning-spec/v1`, OBC §12.7) instead of a vector set:
    /// its CXF, mapping and trend files resolve against the directory of SPEC as given and may
    /// not leave it.
    #[arg(long, value_name = "SPEC")]
    pub(crate) commission: Option<OsString>,
    /// The zero-based index of the spec's reference to run; needed when it has several.
    #[arg(long, value_name = "N", requires = "commission", conflicts_with_all = VECTOR_ONLY)]
    pub(crate) reference: Option<usize>,
    /// Write the receipt to DIR/receipt.json (atomically) and print the report on stdout; with
    /// --commission also each output's exceedance curve, `DIR/outputs/<k>/errors.csv`.
    #[arg(long, value_name = "DIR")]
    pub(crate) out: Option<OsString>,
    /// With --commission and --out: also each Real output's reference, test and bound curves
    /// beside its errors.csv, under pyfunnel's names.
    #[arg(
        long,
        requires = "commission",
        requires = "out",
        conflicts_with_all = VECTOR_ONLY
    )]
    pub(crate) bounds: bool,
    /// With --commission: what the trend files are, the receipt's data label (trended when
    /// absent).
    #[arg(long, value_enum, requires = "commission", conflicts_with_all = VECTOR_ONLY)]
    pub(crate) data: Option<DataLabel>,
    /// Exit 1 when a mutant survived or the set lists none; no receipt byte changes.
    #[arg(long, conflicts_with = "commission")]
    pub(crate) require_mutants_caught: bool,
    /// Exit 1 when the subject warned at load or a scenario left a warning unmatched (with
    /// --commission: any load or run-time warning); no receipt byte changes.
    #[arg(long)]
    pub(crate) deny_warnings: bool,
    /// Measure the run into the receipt's timings, which receipt_id never covers.
    #[arg(long)]
    pub(crate) timings: bool,
}

/// A vector run of `verify`: the set, its subject and the flags a vector run reads.
#[derive(Debug)]
pub(crate) struct VectorArgs {
    /// The vector set.
    pub(crate) vectors: OsString,
    /// The subject.
    pub(crate) subject: OsString,
    /// The subject's form, when given.
    pub(crate) subject_form: Option<SubjectForm>,
    /// `--out DIR`.
    pub(crate) out: Option<OsString>,
    /// `--require-mutants-caught`.
    pub(crate) require_mutants_caught: bool,
    /// `--deny-warnings`.
    pub(crate) deny_warnings: bool,
    /// `--timings`.
    pub(crate) timings: bool,
}

/// A commissioning run of `verify` (`--commission`): the spec and the flags it reads.
#[derive(Debug)]
pub(crate) struct CommissionArgs {
    /// The commissioning spec.
    pub(crate) spec: OsString,
    /// `--reference N`, when given.
    pub(crate) reference: Option<usize>,
    /// `--out DIR`.
    pub(crate) out: Option<OsString>,
    /// `--bounds` (clap holds it to `--out`).
    pub(crate) bounds: bool,
    /// `--data`, trended when absent.
    pub(crate) data: DataLabel,
    /// `--deny-warnings`.
    pub(crate) deny_warnings: bool,
    /// `--timings`.
    pub(crate) timings: bool,
}

/// What one `verify` invocation runs.
#[derive(Debug)]
pub(crate) enum VerifyMode {
    /// A vector set against a subject.
    Vectors(VectorArgs),
    /// One reference of a commissioning spec.
    Commission(CommissionArgs),
}

impl VerifyArgs {
    /// The run these arguments ask for. clap requires `<VECTORS>` and `--subject` unless
    /// `--commission` is given and refuses them beside it, so the error is never reached; it is
    /// clap's own usage error all the same, never a panic.
    fn mode(self) -> Result<VerifyMode, clap::Error> {
        if let Some(spec) = self.commission {
            return Ok(VerifyMode::Commission(CommissionArgs {
                spec,
                reference: self.reference,
                out: self.out,
                bounds: self.bounds,
                data: self.data.unwrap_or(DataLabel::Trended),
                deny_warnings: self.deny_warnings,
                timings: self.timings,
            }));
        }
        match (self.vectors, self.subject) {
            (Some(vectors), Some(subject)) => Ok(VerifyMode::Vectors(VectorArgs {
                vectors,
                subject,
                subject_form: self.subject_form,
                out: self.out,
                require_mutants_caught: self.require_mutants_caught,
                deny_warnings: self.deny_warnings,
                timings: self.timings,
            })),
            _ => Err(clap::Error::raw(
                clap::error::ErrorKind::MissingRequiredArgument,
                "verify needs <VECTORS> and --subject, or --commission <SPEC>\n",
            )),
        }
    }
}

/// The forms `id` and `diff` read an input in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum IdForm {
    // Flat CXF, given to the engine untouched (`.jsonld`, `.cxf.json`).
    Cxf,
    // The text form (`.oce`), validated and emitted first.
    Text,
    // The strict-JSON wire form (any other `.json`), validated and emitted first.
    Wire,
}

/// What `id` reads and how it answers.
#[derive(Debug, Args)]
pub(crate) struct IdArgs {
    /// The input: CXF (`.jsonld`, `.cxf.json`), text (`.oce`) or wire (`.json`), or `-` for stdin.
    pub(crate) file: OsString,
    /// The input form; overrides the name, and is required for stdin.
    #[arg(long, value_enum)]
    pub(crate) from: Option<IdForm>,
    /// The instance namespace a text or wire file is emitted under, as for emit. A namespace
    /// never moves the id; one whose IRIs the engine refuses leaves the document with none.
    #[arg(long)]
    pub(crate) namespace: Option<String>,
    /// Print the JSON report, with content_id, instead of the id line.
    #[arg(long)]
    pub(crate) json: bool,
}

/// The document `diff` writes on stdout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum DiffFormat {
    // The JSON report, with the diff under `diff`.
    Json,
    // The diff as a Markdown table (`Change | Subject | Was | Now`), for a bundle card.
    Markdown,
}

/// What `diff` compares and how it answers.
#[derive(Debug, Args)]
pub(crate) struct DiffArgs {
    /// The base (earlier) side: text (`.oce`), wire (`.json`) or CXF (`.jsonld`, `.cxf.json`), or
    /// `-` for stdin.
    pub(crate) base: OsString,
    /// The head (later) side, in the base's form, or `-` for stdin (on one side only).
    pub(crate) head: OsString,
    /// The form of both sides; overrides the names, and is required for stdin.
    #[arg(long, value_enum)]
    pub(crate) from: Option<IdForm>,
    /// Turn the rename pass off: every unmatched block is removed or added, and the diff carries
    /// the note rename-pass-off. graph_equal does not change.
    #[arg(long)]
    pub(crate) no_renames: bool,
    /// The document on stdout: the JSON report with the diff, or the diff as a Markdown table.
    /// The text rendering goes to stderr under --render.
    #[arg(long, value_enum, default_value_t = DiffFormat::Json)]
    pub(crate) format: DiffFormat,
}

/// What `import` reads and what it writes.
#[derive(Debug, Args)]
pub(crate) struct ImportArgs {
    /// The flat CXF document, or `-` for stdin.
    pub(crate) file: OsString,
    /// Write the sequence here (atomically) and print the report on stdout; `-` is stdout. The
    /// name must not read as another form (.oce is text, .json is wire, .jsonld and .cxf.json
    /// are CXF) and must not be the input file.
    #[arg(short = 'o', long = "output")]
    pub(crate) output: Option<OsString>,
    /// Write the wire form (strict JSON) instead of the text form.
    #[arg(long)]
    pub(crate) wire: bool,
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
        /// Write the CXF here (atomically) and print the report on stdout; `-` is stdout. The
        /// name must not read as text or wire (.oce, .json) and the file must not be the input
        /// (on Unix, nor the file stdin was opened from).
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
    /// Replay a vector set against a subject, or a commissioning spec against its trends, and
    /// print the receipt.
    ///
    /// Stdout is the receipt when the run reaches a verdict: exit 0 when every scenario (with
    /// --commission, every compared output) passed, 1 when one failed or the run checked nothing.
    /// Anything that prevents a verdict is exit 2 with the JSON report. With --out the receipt
    /// goes to DIR/receipt.json and the report to stdout.
    #[command(override_usage = VERIFY_USAGE)]
    Verify(VerifyArgs),
    /// Print the canonical content id of a CXF document or an authored sequence.
    ///
    /// Stdout is the id and a line feed, exit 0. A text or wire file is validated and emitted
    /// first, as emit does. An input that cannot be read, parsed or loaded has no id: exit 2
    /// with the JSON report.
    Id(IdArgs),
    /// Read a flat CXF document into the authoring form.
    ///
    /// Stdout is the sequence as canonical text (the wire form with --wire), exit 0. With -o it
    /// is written to a file and stdout is the JSON report, which lists what the importer read
    /// and did not carry. A document the importer refuses is exit 2 with the report. The
    /// sequence is not validated: run validate or emit on the result.
    Import(ImportArgs),
    /// Compare two sequences, or two CXF documents, and print the structural diff.
    ///
    /// Stdout is the JSON report with the diff (the diff as a Markdown table with --format
    /// markdown): exit 0 when nothing differs, 1 for any difference, a reordering or a comment
    /// included (summary.graph_equal answers "same structure"). Both sides are one form and are
    /// never validated. A side that cannot be read, parsed or loaded is exit 2 with the report.
    Diff(DiffArgs),
}

/// The streams one run reads and writes, and whether stderr is a terminal.
pub struct Streams<'a> {
    /// Standard input (`-`). The overwrite guard of `-o` compares the process's own file
    /// descriptor 0 (Unix), so a reader that is not the process's stdin is never checked.
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
        Command::Verify(args) => match args.mode() {
            Ok(VerifyMode::Vectors(args)) => commands::verify::run(&args),
            Ok(VerifyMode::Commission(args)) => commands::verify_commission::run(&args),
            Err(error) => return clap_exit(&error, streams),
        },
        Command::Id(args) => commands::id::run(&args, streams),
        Command::Import(args) => commands::import::run(&args, streams),
        Command::Diff(args) => commands::diff::run(&args, render, streams),
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
