//! The engine is pure: complete-frame execution is a function of the loaded bytes and the frames
//! the host supplies, and the facade reaches no storage crate at all.
//!
//! Corpus inputs are explicit synthetic in-domain observations, not a host fallback policy or a
//! correctness oracle; independent numerical evidence remains in the sequence suites. The
//! storage-isolation tests read the tracked manifest, lockfile and sources rather than trusting a
//! comment: `oce-api` must never depend on `oce-store` (or any `oce-store*` crate), so a host's
//! durability adapter can never be reached from a frame.

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use oce_api::{Engine, OcError, PointValueType, Value, ValueType};

/// The projection's carrier for a native value type (`Enum` travels as its integer ordinal).
fn projected_type(value_type: ValueType) -> PointValueType {
    match value_type {
        ValueType::Real => PointValueType::Real,
        ValueType::Boolean => PointValueType::Bool,
        _ => PointValueType::Int,
    }
}

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn corpus() -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(manifest_dir().join("../oce-cxf/tests/fixtures/g36"))
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonld"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 47, "corpus size positive control");
    paths
}

fn synthetic_inputs(engine: &Engine) -> Vec<(String, Value)> {
    engine
        .input_definitions()
        .unwrap()
        .into_iter()
        .map(|d| {
            let value = d.min.unwrap_or_else(|| d.value_type.zero_value());
            (d.path, value)
        })
        .collect()
}

fn run(engine: &mut Engine, inputs: &[(String, Value)]) -> Vec<String> {
    let entries: Vec<_> = inputs
        .iter()
        .map(|(p, v)| (p.as_str(), v.clone()))
        .collect();
    let mut trace = Vec::new();
    for (index, time) in [0.0, 0.0, 1.0].into_iter().enumerate() {
        let prepared = engine.prepare_frame(time, &entries).unwrap();
        let completed = engine.execute_frame(prepared).unwrap();
        assert_eq!(completed.sequence(), index as u64 + 1);
        assert!(completed.outputs().windows(2).all(|w| w[0].0 < w[1].0));
        for (path, value) in completed.outputs() {
            let bits = match value {
                Value::Real(real) => format!("{:016x}", real.to_bits()),
                other => format!("{other:?}"),
            };
            trace.push(format!("{index} {path} {bits}"));
        }
    }
    trace.push(format!("{:?}", engine.state_snapshot().unwrap().as_bytes()));
    trace
}

#[test]
fn complete_corpus_frames_are_a_function_of_the_bytes_and_the_frames_alone() {
    for path in corpus() {
        let bytes = std::fs::read(&path).unwrap();
        let mut first = Engine::new();
        let (_, loaded) = first.load_cxf(&bytes).unwrap();
        let inputs = synthetic_inputs(&first);
        // The plain-data input view names every frame determinant, with its value type.
        let points: Vec<_> = loaded.input_points().collect();
        for (input, value) in &inputs {
            assert!(
                points.iter().any(|(point, value_type)| point == input
                    && *value_type == projected_type(value.value_type())),
                "{}: {input} missing from LoadedModel::input_points",
                path.display()
            );
        }
        // A second engine, run after an unrelated third one, must produce the identical trace:
        // no hidden input, shared state or host-side store participates in a frame.
        let mut second = Engine::new();
        second.load_cxf(&bytes).unwrap();
        let mut unrelated = Engine::new();
        unrelated
            .load_cxf(include_bytes!("fixtures/legacy_frame_add.jsonld"))
            .unwrap();
        let unrelated_inputs = synthetic_inputs(&unrelated);
        let expected = run(&mut first, &inputs);
        run(&mut unrelated, &unrelated_inputs);
        drop(loaded);
        assert_eq!(run(&mut second, &inputs), expected, "{}", path.display());
    }
}

#[test]
fn a_missing_determinant_refuses_and_never_reuses_a_prior_value() {
    const A: &str = "urn:legacy-frame:a";
    const B: &str = "urn:legacy-frame:b";
    let mut engine = Engine::new();
    engine
        .load_cxf(include_bytes!("fixtures/legacy_frame_add.jsonld"))
        .unwrap();
    // A first complete frame leaves B = 2.0 in the latest-state image.
    let plan = engine
        .prepare_frame(0.0, &[(A, Value::Real(1.0)), (B, Value::Real(2.0))])
        .unwrap();
    assert!(
        engine.execute_frame(plan).unwrap().outputs()[0]
            .1
            .bit_eq(&Value::Real(3.0))
    );
    let before = engine.state_snapshot().unwrap();
    // Omitting B is a refusal, never a frame that silently reuses the 2.0 seen before.
    assert!(
        matches!(engine.prepare_frame(1.0, &[(A, Value::Real(5.0))]),
        Err(OcError::FrameMissingInput(path)) if path == B)
    );
    assert_eq!(
        before.as_bytes(),
        engine.state_snapshot().unwrap().as_bytes()
    );
    let plan = engine
        .prepare_frame(1.0, &[(A, Value::Real(5.0)), (B, Value::Real(-7.0))])
        .unwrap();
    let frame = engine.execute_frame(plan).unwrap();
    assert!(frame.outputs()[0].1.bit_eq(&Value::Real(-2.0)));
    assert_eq!(frame.sequence(), 2);
}

/// Dependency names declared in one `[section]` of a Cargo manifest (inline-table form only,
/// which is the only form this workspace uses).
fn manifest_section_dependencies(manifest: &str, section: &str) -> Vec<String> {
    let mut in_section = false;
    let mut names = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == section;
            continue;
        }
        if in_section
            && !line.starts_with('#')
            && let Some((name, _)) = line.split_once('=')
        {
            names.push(name.trim().trim_matches('"').to_owned());
        }
    }
    names
}

#[test]
fn the_facade_manifest_declares_no_storage_dependency() {
    let manifest = std::fs::read_to_string(manifest_dir().join("Cargo.toml")).unwrap();
    let normal = manifest_section_dependencies(&manifest, "[dependencies]");
    assert!(
        normal.iter().any(|name| name == "oce-model"),
        "positive control: the parser sees the normal dependencies: {normal:?}"
    );
    for section in [
        "[dependencies]",
        "[build-dependencies]",
        "[dev-dependencies]",
    ] {
        for name in manifest_section_dependencies(&manifest, section) {
            assert!(
                !name.starts_with("oce-store"),
                "oce-api declares {name} under {section}"
            );
        }
    }
}

#[test]
fn the_manifest_parser_detects_a_reintroduced_storage_dependency() {
    let hostile = "[package]\nname = \"oce-api\"\n\n[dependencies]\n# comment\n\
                   oce-model = { workspace = true }\noce-store = { workspace = true }\n\n\
                   [dev-dependencies]\noce-store-extra = { workspace = true }\n";
    assert_eq!(
        manifest_section_dependencies(hostile, "[dependencies]"),
        ["oce-model", "oce-store"]
    );
    assert_eq!(
        manifest_section_dependencies(hostile, "[dev-dependencies]"),
        ["oce-store-extra"]
    );
}

/// Every package reachable from `root` through the lockfile's dependency lists. The lockfile does
/// not distinguish dependency kinds, so this over-approximates the normal graph: if a storage
/// crate is absent here, it is absent from the real normal graph too.
fn lockfile_closure(lockfile: &str, root: &str) -> BTreeSet<String> {
    let mut edges: Vec<(String, Vec<String>)> = Vec::new();
    for block in lockfile.split("[[package]]").skip(1) {
        let name = block
            .lines()
            .find_map(|line| line.strip_prefix("name = "))
            .map(|name| name.trim_matches('"').to_owned())
            .unwrap();
        let dependencies = block
            .split_once("dependencies = [")
            .map(|(_, rest)| {
                rest.split(']')
                    .next()
                    .unwrap()
                    .split(',')
                    .filter_map(|item| item.trim().trim_matches('"').split(' ').next())
                    .filter(|item| !item.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        edges.push((name, dependencies));
    }
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from([root.to_owned()]);
    while let Some(name) = queue.pop_front() {
        if !seen.insert(name.clone()) {
            continue;
        }
        for (_, dependencies) in edges.iter().filter(|(package, _)| *package == name) {
            queue.extend(dependencies.iter().cloned());
        }
    }
    seen
}

#[test]
fn no_storage_crate_is_reachable_from_the_facade_in_the_locked_graph() {
    let lockfile = std::fs::read_to_string(manifest_dir().join("../../Cargo.lock")).unwrap();
    let closure = lockfile_closure(&lockfile, "oce-api");
    for expected in ["oce-model", "oce-graph", "oce-blocks", "oce-cxf", "serde"] {
        assert!(
            closure.contains(expected),
            "positive control: {expected} must be reachable: {closure:?}"
        );
    }
    let storage: Vec<_> = closure
        .iter()
        .filter(|name| name.starts_with("oce-store"))
        .collect();
    assert!(storage.is_empty(), "oce-api reaches {storage:?}");
}

#[test]
fn the_lockfile_walk_detects_a_transitive_storage_edge() {
    let hostile = "version = 4\n\n[[package]]\nname = \"oce-api\"\nversion = \"0.2.0\"\n\
                   dependencies = [\n \"oce-model\",\n]\n\n[[package]]\nname = \"oce-model\"\n\
                   version = \"0.2.0\"\ndependencies = [\n \"oce-store 0.2.0\",\n]\n\n\
                   [[package]]\nname = \"oce-store\"\nversion = \"0.2.0\"\n";
    let closure = lockfile_closure(hostile, "oce-api");
    assert!(closure.contains("oce-store"), "{closure:?}");
}

fn rust_sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn no_facade_source_names_a_storage_crate_symbol() {
    let mut sources = Vec::new();
    rust_sources(&manifest_dir().join("src"), &mut sources);
    rust_sources(&manifest_dir().join("examples"), &mut sources);
    assert!(
        sources.len() > 50,
        "positive control: {} sources",
        sources.len()
    );
    for path in sources {
        let text = std::fs::read_to_string(&path).unwrap();
        for symbol in ["oce_store", "MemStore", "with_store"] {
            assert!(!text.contains(symbol), "{} names {symbol}", path.display());
        }
    }
}
