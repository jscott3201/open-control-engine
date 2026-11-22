//! The commissioning spec and point-mapping readers (design note `docs/design/commissioning.md`,
//! "Test-spec file" and "Point mapping"; issue #658) and the trend sources (issue #660), as one
//! test binary.
//!
//! Each file of this directory is one topic and is compiled here as a module; the last test
//! below fails on a file that is not, so a test file cannot sit in the tree uncompiled. Select
//! one topic with `cargo nextest run -p oce-verify --test commissioning -E 'test(/^<file>::/)'`.

#[path = "../support/mod.rs"]
mod support;

mod building_33_day;
mod building_33_openmodelica;
mod building_33_transcription;
mod canonical_form;
mod closed_objects;
mod comment_stripping;
mod error_codes;
mod hostile_bytes;
mod limit_boundaries;
mod listings;
mod load_cost;
mod point_mapping;
mod run_bind;
mod run_bounds;
mod run_change_differential;
mod run_change_edges;
mod run_change_of_value;
mod run_change_readings;
mod run_comparison;
mod run_cost;
mod run_gate_row;
mod run_hostile;
mod run_masking;
mod run_parameters;
mod run_receipts;
mod run_refused_frames;
mod spec_fields;
mod trend_bounds;
mod trend_controller_output;
mod trend_cost;
mod trend_files;
mod trend_fixture;
mod trend_formats;
mod trend_holds;
mod trend_hostile;
mod trend_padded_files;

use oce_verify::{
    CommissionOptions, CommissionRun, CommissioningBody, CommissioningSpec, Location, OutputCurves,
    OutputReport, PointMapping, ReceiptBody, SourceError, SpecError, SpecInputs, SpecLimits,
    TrendLimits, TrendSources, VerifyError, run_commissioning,
};

/// The text of a fixture under `tests/fixtures/commissioning/`.
fn fixture_text(name: &str) -> String {
    String::from_utf8(support::fixture_bytes(&format!("commissioning/{name}"))).expect("UTF-8")
}

/// Load `text` as a spec under the default limits.
fn load(text: &str) -> Result<CommissioningSpec, SpecError> {
    CommissioningSpec::from_json_str(text, &SpecLimits::default())
}

/// The spec `text` is, which must load.
fn loaded(text: &str) -> CommissioningSpec {
    load(text).unwrap_or_else(|error| panic!("the spec loads: {} {error}\n{text}", error.code()))
}

/// The code `text` is refused with as a spec under the default limits, or `loaded`.
fn code_of(text: &str) -> &'static str {
    match load(text) {
        Ok(_) => "loaded",
        Err(error) => error.code(),
    }
}

/// The code, line and column `text` is refused with as a spec; the refusal must sit in the text.
fn refusal(text: &str) -> (&'static str, u32, u32) {
    let error = load(text).expect_err("the spec is refused");
    let Location { line, column } = error
        .at()
        .unwrap_or_else(|| panic!("{} carries no place: {error}", error.code()));
    (error.code(), line, column)
}

/// Load `text` as a point-mapping file under the default limits.
fn load_mapping(text: &str) -> Result<PointMapping, SpecError> {
    PointMapping::from_json_str(text, &SpecLimits::default())
}

/// A spec of one reference: `members` are the reference's members after `model` and `sequence`
/// (sequence `s`), written as the inside of a JSON object, or empty.
fn one_reference(members: &str) -> String {
    let separator = if members.is_empty() { "" } else { ", " };
    format!("{{\"references\": [{{\"model\": \"M\", \"sequence\": \"s\"{separator}{members}}}]}}")
}

/// A spec with no reference and the given top-level `members`, written as the inside of a JSON
/// object, or empty.
fn top_level(members: &str) -> String {
    let separator = if members.is_empty() { "" } else { ", " };
    format!("{{\"references\": []{separator}{members}}}")
}

/// The bytes of a file of the three-file trend fixture, `tests/fixtures/commissioning/trends/`.
fn trend_file(name: &str) -> Vec<u8> {
    support::fixture_bytes(&format!("commissioning/trends/{name}"))
}

/// The files the fixture's spec names, in the order given.
const TREND_FILES: [&str; 4] = ["points.json", "ahu.mos", "status.csv", "limits.csv"];

/// The fixture's spec.
fn trend_spec() -> CommissioningSpec {
    loaded(&String::from_utf8(trend_file("spec.json")).expect("UTF-8"))
}

/// The fixture's files, handed in under their paths in the order `names` gives.
fn trend_inputs(names: &[&str]) -> SpecInputs {
    SpecInputs {
        cxf: Vec::new(),
        files: names
            .iter()
            .map(|name| ((*name).to_owned(), trend_file(name)))
            .collect(),
    }
}

/// A spec of one reference (sequence `s`) with the time origin `origin`, a point mapping inline
/// and the reference's other `members`, written as the inside of a JSON object.
fn trend_spec_with(origin: Option<&str>, mapping: &str, members: &str) -> CommissioningSpec {
    let time = origin.map_or(String::new(), |origin| {
        format!(r#""time": {{"origin": "{origin}"}}, "#)
    });
    loaded(&format!(
        r#"{{{time}"references": [{{"model": "M", "sequence": "s",
            "pointNameMapping": {mapping}, {members}}}]}}"#
    ))
}

/// One mapped point, as a mapping list entry: `cdl` and `device` as `(name, unit, type)`.
fn point(cdl: (&str, Option<&str>, &str), device: (&str, Option<&str>, &str)) -> String {
    let side = |(name, unit, kind): (&str, Option<&str>, &str)| {
        let unit = unit.map_or(String::new(), |unit| format!(r#", "unit": "{unit}""#));
        format!(r#"{{"name": "{name}", "type": "{kind}"{unit}}}"#)
    };
    format!(r#"{{"cdl": {}, "device": {}}}"#, side(cdl), side(device))
}

/// Replay `inputs` against the `signals` subject as a commissioning run sets replay up: the
/// recorded times, equal times allowed, the sources' holds, a refused frame recorded and the run
/// continued, the inputs retained in the trace.
fn replay_signals(inputs: oce_verify::TrendInputs) -> oce_replay::Trace {
    use oce_replay::{InputMap, OnRefusal, Replay, ReplayLimits, ReplayOptions, TraceOptions};
    let mut engine = oce_api::Engine::new();
    engine
        .load_cxf(&support::subject("signals"))
        .expect("the signals subject loads");
    let definitions = engine.input_definitions().expect("the inputs");
    let map = InputMap::resolve(
        &definitions,
        &inputs.column_specs(),
        &inputs.column_map(),
        &inputs.missing_policy(),
        &inputs.overrides(),
    )
    .expect("the merged columns map onto the subject's inputs");
    let mut options = ReplayOptions::new(inputs.time_base());
    options.duplicate_time = inputs.duplicate_time();
    options.on_refusal = OnRefusal::RecordAndContinue;
    options.trace = TraceOptions {
        inputs: true,
        ..TraceOptions::default()
    };
    let source = inputs
        .into_source(&map, &ReplayLimits::default())
        .expect("the merged source binds");
    let (_, trace) = Replay::new(engine, map, source, options)
        .expect("the replay starts")
        .run()
        .expect("the replay runs");
    trace
}

/// Read reference 0 of `spec` from `files` (path, bytes) under the default limits.
fn read_trends(
    spec: &CommissioningSpec,
    files: &[(&str, &[u8])],
) -> Result<TrendSources, SourceError> {
    let inputs = SpecInputs {
        cxf: Vec::new(),
        files: files
            .iter()
            .map(|(path, bytes)| ((*path).to_owned(), bytes.to_vec()))
            .collect(),
    };
    TrendSources::read(spec, 0, &inputs, &TrendLimits::default())
}

/// The bytes of a file of the run fixture, `tests/fixtures/commissioning/run/`.
fn run_file(name: &str) -> Vec<u8> {
    support::fixture_bytes(&format!("commissioning/run/{name}"))
}

/// The run fixture's spec.
fn run_spec() -> CommissioningSpec {
    loaded(&String::from_utf8(run_file("spec.json")).expect("UTF-8"))
}

/// The run fixture's spec with each `(old, new)` of `edits` replaced in its text; an `old`
/// that does not occur fails the test.
fn run_spec_edited(edits: &[(&str, &str)]) -> CommissioningSpec {
    let text = String::from_utf8(support::edited(&run_file("spec.json"), edits)).expect("UTF-8");
    loaded(&text)
}

/// The run fixture's file `name` with each `(old, new)` of `edits` replaced.
fn run_file_edited(name: &str, edits: &[(&str, &str)]) -> Vec<u8> {
    support::edited(&run_file(name), edits)
}

/// The files the run fixture's spec names, with `replaced` (path, bytes) in place of the
/// fixture's file of the same path or added beside them, and the `plant` subject as the CXF.
fn run_inputs(replaced: &[(&str, &[u8])]) -> SpecInputs {
    let mut files: Vec<(String, Vec<u8>)> = ["points.json", "inputs.csv", "outputs.csv", "fan.csv"]
        .iter()
        .map(|name| {
            let bytes = replaced
                .iter()
                .find(|(path, _)| path == name)
                .map_or_else(|| run_file(name), |(_, bytes)| bytes.to_vec());
            ((*name).to_owned(), bytes)
        })
        .collect();
    for (path, bytes) in replaced {
        if !files.iter().any(|(given, _)| given == path) {
            files.push(((*path).to_owned(), bytes.to_vec()));
        }
    }
    SpecInputs {
        cxf: support::subject("plant"),
        files,
    }
}

/// Run reference 0 of `spec` on `inputs` under `options`.
fn commission(
    spec: &CommissioningSpec,
    inputs: &SpecInputs,
    options: &CommissionOptions,
) -> Result<CommissionRun, VerifyError> {
    run_commissioning(spec, 0, inputs, options)
}

/// Run reference 0 of `spec` on `inputs` under the default options; it must give a receipt.
fn commissioned(spec: &CommissioningSpec, inputs: &SpecInputs) -> CommissionRun {
    commission(spec, inputs, &CommissionOptions::default()).unwrap_or_else(|error| {
        panic!(
            "the run gives a receipt: {} {:?} at {}: {error}",
            error.code(),
            error.cause(),
            error.subject()
        )
    })
}

/// The refusal of reference 0 of `spec` on `inputs` under the default options.
fn refused_run(spec: &CommissioningSpec, inputs: &SpecInputs) -> VerifyError {
    match commission(spec, inputs, &CommissionOptions::default()) {
        Ok(run) => panic!(
            "the run is refused, and gave:\n{}",
            run.receipt.to_canonical_json()
        ),
        Err(error) => error,
    }
}

/// The commissioning body of `receipt`.
fn body_of(receipt: &oce_verify::Receipt) -> &CommissioningBody {
    match &receipt.body {
        ReceiptBody::Commissioning(body) => body,
        _ => panic!("a commissioning run issues a commissioning receipt"),
    }
}

/// The report of the output whose canonical path ends with `.{name}`.
fn output_named<'b>(body: &'b CommissioningBody, name: &str) -> &'b OutputReport {
    body.outputs
        .iter()
        .find(|output| output.output.ends_with(&format!(".{name}")))
        .unwrap_or_else(|| panic!("an output named {name}"))
}

/// The curves of the output whose canonical path ends with `.{name}`.
fn curves_named<'r>(run: &'r CommissionRun, name: &str) -> &'r OutputCurves {
    run.curves
        .iter()
        .find(|curves| curves.output.ends_with(&format!(".{name}")))
        .unwrap_or_else(|| panic!("curves named {name}"))
}

#[test]
fn every_test_file_of_this_directory_is_compiled_into_this_binary() {
    let directory = support::crate_dir().join("tests/commissioning");
    let root = std::fs::read_to_string(directory.join("main.rs")).expect("read main.rs");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&directory).expect("read the directory") {
        let name = entry.expect("an entry").file_name();
        let name = name.to_str().expect("a UTF-8 file name").to_owned();
        // What a file manager or an editor leaves behind (`.DS_Store`) is no test file.
        if name.starts_with('.') {
            continue;
        }
        if let Some(module) = name.strip_suffix(".rs")
            && module != "main"
        {
            files.push(module.to_owned());
        } else {
            assert_eq!(name, "main.rs", "only Rust files live here");
        }
    }
    files.sort();
    assert!(files.len() >= 10, "{files:?}");
    let declared: Vec<String> = root
        .lines()
        .filter_map(|line| line.strip_prefix("mod ")?.strip_suffix(';'))
        // `support` is the crate's shared helper module, one directory up.
        .filter(|module| *module != "support")
        .map(str::to_owned)
        .collect();
    assert_eq!(
        declared, files,
        "each file is one `mod` line of main.rs, in name order, and each line a file"
    );
}
