//! The blob round trip (issue #403): for every one of the 46 G36 catalog fixtures, load the CXF on
//! this target through the facade, encode the compiled-schedule blob, decode it with the `no_std`
//! execution entry (`oce_graph::blob::Executable`), run the same frames through both, and compare
//! bit for bit: every boundary output by its raw bits after every frame, the warnings each frame
//! emitted (source, message, time bits, in order), and the raw state words after load and after
//! every frame (`Engine::state_words` against `Executable::state_words`, the executable's own
//! layout; the portable snapshot is not compared here). The encoded blob must equal the checked-in
//! golden (`crates/oce-api/tests/fixtures/blob/`), and the entry's run must reproduce the pinned run
//! digest (`crates/oce-graph/tests/fixtures/blob_frame_digests.tsv`).
//!
//! **Frames.** The suite drives `oce-graph`'s shared schedule (`tests/support/frame_schedule.rs`,
//! included by path): 160 frames per fixture spanning about 3.3 hours, each input's value a
//! function of its slot's type and acceptance domain. It is the convention the blob goldens, the
//! run digests, the facade's dual-path test and the firmware image already use, and it is defined
//! for all 46 fixtures; the conformance driver's Tier-2 reference schedules cover only the
//! fixtures with reference traces, so reusing them would leave fixtures out.
//!
//! # Modes
//!
//! * Neither variable set: the facade against the entry on this target, plus the golden and
//!   digest checks.
//! * `OCE_ROUND_TRIP_OUT=<dir>`: also write the recorded artifact (below) into `<dir>`, created
//!   if absent. Every determinism cell does this in CI; the x86_64 debug cell's recording is the
//!   artifact the other legs replay.
//! * `OCE_ROUND_TRIP_EXPECT=<dir>`: also read the recorded artifact from `<dir>`, require its blob
//!   bytes to equal this target's encoding, decode and run the *recorded* blob (encoded on the
//!   recording host), and compare this target's run with the recorded one. The aarch64 and
//!   `wasm32-wasip1` legs do this in CI. The facade is available on every target the suite runs
//!   on, so the facade-against-entry comparison runs in this mode too.
//!
//! Setting both is refused. A relative directory resolves against the workspace root, as the CI
//! cells pass it: nextest runs this binary from the package directory (GitHub #438).
//!
//! # The recorded artifact, format version 1
//!
//! One directory, two files per fixture and nothing else (the reader refuses any other name):
//!
//! * `<fixture>.oceb` — the blob bytes the recording target encoded.
//! * `<fixture>.trace.tsv` — UTF-8 text, one record per line, fields separated by one tab, every
//!   line ending in `\n`, in this order:
//!
//! ```text
//! oce-blob-round-trip<TAB>1
//! fixture<TAB><name>
//! blob<TAB><byte length><TAB><FNV-1a-64 of the blob file, 16 hex digits>
//! outputs<TAB><n>
//! output<TAB><slot><TAB><path>              n lines, slots 0 to n-1
//! words<TAB><w>                             state words per snapshot
//! load<TAB><word>...                        w words after load
//! frame<TAB><k><TAB><time bits><TAB><sequence>  per frame, k from 0
//! out<TAB><value>...                        n boundary outputs, slot order
//! warn<TAB><source><TAB><message><TAB><time bits>   zero or more, emission order
//! state<TAB><word>...                       w words after the frame
//! end<TAB><frames>
//! ```
//!
//! Words and time bits are sixteen lowercase hex digits. A value is `r:<16 hex>` (a Real's IEEE-754
//! bits), `i:<decimal>`, `b:0` or `b:1`, `s:<text>` or `e:<class>:<ordinal>`. Text escapes `\`,
//! tab, newline and carriage return as `\\`, `\t`, `\n`, `\r`, and other control characters as
//! `\xHH`. Counts are canonical unsigned decimals. The reader bounds every file (32 MiB per trace,
//! 1 MiB per blob) before reading it, every declared count, and refuses anything off-grammar with a
//! typed error naming the file and line; it never panics.

#![allow(
    clippy::print_stdout,
    reason = "the per-fixture lines are the evidence a CI log shows"
)]

#[path = "blob_round_trip/artifact.rs"]
mod artifact;
#[path = "blob_round_trip/controls.rs"]
mod controls;
#[path = "../../oce-graph/tests/support/frame_schedule.rs"]
mod frame_schedule;
#[path = "blob_round_trip/trace.rs"]
mod trace;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use artifact::Recorded;
use frame_schedule::{DIGEST_TABLE, TICKS, parse_digests};
use trace::{FACADE_VS_ENTRY, GOLDEN_VS_ENCODED, RECORDED_VS_CURRENT};

/// The variable that selects record mode.
const OUT_VAR: &str = "OCE_ROUND_TRIP_OUT";
/// The variable that selects expect mode.
const EXPECT_VAR: &str = "OCE_ROUND_TRIP_EXPECT";

/// The workspace root, which every relative path resolves against.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_dir() -> PathBuf {
    workspace_root().join("crates/oce-cxf/tests/fixtures/g36")
}

fn golden_dir() -> PathBuf {
    workspace_root().join("crates/oce-api/tests/fixtures/blob")
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The CXF bytes of one fixture.
fn fixture_cxf(name: &str) -> Vec<u8> {
    read(&fixture_dir().join(format!("{name}.jsonld")))
}

/// The 46 G36 catalog fixtures, sorted: every `.jsonld` beside them but the resolver-contract
/// document `member_list_interface`, which is not a catalog fixture.
fn fixtures() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixture_dir())
        .unwrap_or_else(|error| panic!("read {}: {error}", fixture_dir().display()))
        .filter_map(|entry| {
            let entry = entry.unwrap_or_else(|error| {
                panic!("read an entry of {}: {error}", fixture_dir().display())
            });
            let name = entry
                .file_name()
                .into_string()
                .unwrap_or_else(|name| panic!("fixture directory entry {name:?} is not UTF-8"));
            name.strip_suffix(".jsonld").map(str::to_owned)
        })
        .filter(|name| name != "member_list_interface")
        .collect();
    names.sort_unstable();
    names
}

/// What a run of the suite does beyond the facade-against-entry comparison.
#[derive(Debug, PartialEq, Eq)]
enum Mode {
    /// Compare on this target only.
    Local,
    /// Also write the recorded artifact into the directory.
    Record(PathBuf),
    /// Also compare with the recorded artifact in the directory.
    Expect(PathBuf),
}

/// The mode the two variables select, resolving a relative directory against the workspace root.
/// An empty value counts as unset.
///
/// # Errors
/// Both variables set.
fn mode_from(out: Option<OsString>, expect: Option<OsString>) -> Result<Mode, String> {
    let resolve = |value: OsString| {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            path
        } else {
            workspace_root().join(path)
        }
    };
    match (
        out.filter(|v| !v.is_empty()),
        expect.filter(|v| !v.is_empty()),
    ) {
        (None, None) => Ok(Mode::Local),
        (Some(out), None) => Ok(Mode::Record(resolve(out))),
        (None, Some(expect)) => Ok(Mode::Expect(resolve(expect))),
        (Some(_), Some(_)) => Err(format!(
            "set {OUT_VAR} or {EXPECT_VAR}, not both: a run records an artifact or checks one"
        )),
    }
}

/// What one fixture's round trip covered, for the evidence line.
struct Covered {
    blob_bytes: usize,
    outputs: usize,
    words: usize,
    warnings: usize,
    digest: String,
}

/// One fixture's round trip in `mode`, with its recorded artifact in expect mode.
fn round_trip(
    name: &str,
    digest: &str,
    recorded: Option<&Recorded>,
    mode: &Mode,
) -> Result<Covered, String> {
    let golden = read(&golden_dir().join(format!("{name}.oceb")));
    let (facade, blob, definitions) =
        trace::run_facade(&fixture_cxf(name)).map_err(|error| format!("{name}: {error}"))?;
    trace::compare_bytes(name, GOLDEN_VS_ENCODED, &golden, &blob).map_err(|m| m.to_string())?;
    // In expect mode the entry decodes the bytes the recording host encoded, not its own.
    let decoded: &[u8] = match recorded {
        Some(recorded) => {
            trace::compare_bytes(name, RECORDED_VS_CURRENT, &recorded.blob, &blob)
                .map_err(|m| m.to_string())?;
            &recorded.blob
        }
        None => &blob,
    };
    let (entry, run_digest) =
        trace::run_entry(decoded, &definitions).map_err(|error| format!("{name}: {error}"))?;
    trace::compare(name, FACADE_VS_ENTRY, &facade, &entry).map_err(|m| m.to_string())?;
    if run_digest != digest {
        return Err(format!(
            "{name}: run digest {run_digest}, but {DIGEST_TABLE} pins {digest}"
        ));
    }
    if let Some(recorded) = recorded {
        trace::compare(name, RECORDED_VS_CURRENT, &recorded.trace, &entry)
            .map_err(|m| m.to_string())?;
    }
    if let Mode::Record(dir) = mode {
        artifact::write(dir, name, &blob, &entry)?;
    }
    Ok(Covered {
        blob_bytes: blob.len(),
        outputs: entry.outputs.len(),
        words: entry.initial_words.len(),
        warnings: entry.frames.iter().map(|f| f.warnings.len()).sum(),
        digest: run_digest,
    })
}

#[test]
fn every_fixture_round_trips_through_its_blob_bit_for_bit() {
    let mode = mode_from(std::env::var_os(OUT_VAR), std::env::var_os(EXPECT_VAR))
        .unwrap_or_else(|error| panic!("{error}"));
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), 46, "the G36 catalog has 46 fixtures");
    let table = String::from_utf8(read(
        &workspace_root().join("crates/oce-graph").join(DIGEST_TABLE),
    ))
    .expect("the digest table is UTF-8");
    let digests = parse_digests(&table);
    let names: Vec<&String> = digests.iter().map(|(name, _)| name).collect();
    assert_eq!(names, fixtures.iter().collect::<Vec<_>>(), "{DIGEST_TABLE}");

    let recorded = match &mode {
        Mode::Expect(dir) => Some(
            artifact::read_dir(dir, &fixtures)
                .unwrap_or_else(|error| panic!("the recorded artifact is refused: {error}")),
        ),
        Mode::Record(dir) => {
            std::fs::create_dir_all(dir)
                .unwrap_or_else(|error| panic!("create {}: {error}", dir.display()));
            None
        }
        Mode::Local => None,
    };
    println!(
        "blob round trip: {mode:?}, {} fixtures, {TICKS} frames each",
        fixtures.len()
    );

    let mut failures = Vec::new();
    for (index, (name, (_, digest))) in fixtures.iter().zip(&digests).enumerate() {
        let recorded = recorded.as_ref().map(|all| &all[index]);
        match round_trip(name, digest, recorded, &mode) {
            Ok(covered) => println!(
                "blob round trip {name}: blob={}B outputs={} words={} warnings={} digest={}",
                covered.blob_bytes,
                covered.outputs,
                covered.words,
                covered.warnings,
                covered.digest
            ),
            Err(error) => failures.push(error),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} fixtures failed the blob round trip:\n{}",
        failures.len(),
        fixtures.len(),
        failures.join("\n")
    );
}
