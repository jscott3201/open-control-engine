//! Hash-order independence: no `HashMap`/`HashSet` iteration order reaches any engine output.
//!
//! `std::collections::RandomState` seeds its SipHash keys from a thread-local that is filled from
//! the OS entropy source the first time a thread builds a hash collection, and steps the key for
//! every further `RandomState` the thread builds. So a load that runs on a freshly spawned thread
//! sees hasher seeds independent of every other run: any output whose order or bytes depend on
//! iterating a hash collection differs between such runs with overwhelming probability.
//!
//! The order-leak test loads every swept CXF document — the 46 G36 catalog fixtures and the one
//! resolver-contract document, the same corpus `export_content_oracle.rs` pins — on
//! [`ITERATIONS`] fresh threads each, and requires every run to produce the same bytes for each
//! output a host or a golden observes: the exported CXF document, its completeness, the canonical
//! content id of the loaded model and the export
//! receipt, the `LoadReport` and load receipt diagnostics, `input_definitions()`, the IO point
//! inventory, the resolved-model projection, the schedule and topology, the frame outputs and
//! assertion events of a fixed frame sequence, and the durable state snapshot bytes after it.
//! A second corpus of refused documents, each failing with several diagnostics, pins the order of
//! the load error and its failure receipt the same way.
//!
//! The harness is proved able to see a leak by [`harness_detects_hash_iteration_order`], which
//! runs a synthetic engine that emits `HashMap` iteration order through the same runner and
//! comparison, and by its sorted twin, which must compare equal. Without the first control a
//! runner that accidentally reused one seed would pass vacuously.

use std::collections::HashMap;
use std::fmt::Debug;
use std::fs;
use std::path::PathBuf;
use std::thread;

use oce_api::{Engine, InputDefinition, Value, ValueType};

/// Fresh-thread runs per document. The issue's acceptance asks for 20 repetitions; every run is a
/// full load, export, four frames and a snapshot, so 20 x 47 documents fits the per-PR budget
/// (measured in the PR description).
const ITERATIONS: usize = 20;

/// Frames executed before the snapshot. Enough to move delays, integrators and latches off their
/// initial state without making the test a simulation.
const FRAMES: u32 = 4;

/// One output surface, named so a divergence report says which output leaked.
const COMPONENTS: [&str; 13] = [
    "export bytes",
    "export completeness",
    "canonical content id",
    "export receipt diagnostics",
    "load report",
    "load receipt diagnostics",
    "input definitions",
    "point inventory",
    "resolved model",
    "schedule",
    "topology",
    "frame outputs and events",
    "state snapshot bytes",
];

/// FNV-1a-128 digest of a byte stream; a mismatch names the component, so the digest only has to
/// separate equal from unequal, not explain the difference.
fn digest(bytes: &[u8]) -> u128 {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    bytes.iter().fold(OFFSET, |hash, byte| {
        (hash ^ u128::from(*byte)).wrapping_mul(PRIME)
    })
}

fn debug_digest(value: &impl Debug) -> u128 {
    digest(format!("{value:?}").as_bytes())
}

/// Per-component digests of one run over one document, in [`COMPONENTS`] order.
type Fingerprint = Vec<u128>;

/// The first place two runs disagree.
#[derive(Debug, PartialEq, Eq)]
struct Divergence {
    /// Index of the document (or synthetic case) in the run's output.
    document: usize,
    /// Index into the fingerprint, i.e. the component that differed.
    component: usize,
    /// The run that differed from run 0.
    run: usize,
}

/// Run `observe` once per run on its own freshly spawned thread, so every run builds its hash
/// collections from independently seeded `RandomState`s. Runs execute concurrently; results come
/// back in run order.
fn run_on_fresh_threads<T, F>(runs: usize, observe: F) -> Vec<T>
where
    T: Send,
    F: Fn() -> T + Sync,
{
    thread::scope(|scope| {
        let handles: Vec<_> = (0..runs).map(|_| scope.spawn(&observe)).collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("observation thread panicked"))
            .collect()
    })
}

/// Compare every run against run 0; `runs[r][d]` is the fingerprint of document `d` in run `r`.
fn first_divergence(runs: &[Vec<Fingerprint>]) -> Option<Divergence> {
    let (reference, rest) = runs.split_first()?;
    for (offset, run) in rest.iter().enumerate() {
        assert_eq!(
            run.len(),
            reference.len(),
            "runs observed different corpora"
        );
        for (document, (expected, actual)) in reference.iter().zip(run).enumerate() {
            if let Some(component) = expected.iter().zip(actual).position(|(e, a)| e != a) {
                return Some(Divergence {
                    document,
                    component,
                    run: offset + 1,
                });
            }
        }
    }
    None
}

/// Every `.jsonld` document in the swept CXF corpus, sorted.
fn corpus() -> Vec<(String, Vec<u8>)> {
    let dir = format!(
        "{}/../oce-cxf/tests/fixtures/g36",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .expect("read swept CXF fixture corpus")
        .map(|entry| entry.expect("fixture directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonld"))
        .collect();
    paths.sort();
    assert_eq!(
        paths.len(),
        47,
        "swept CXF corpus size moved (46 G36 + 1 contract)"
    );
    paths
        .into_iter()
        .map(|path| {
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .expect("UTF-8 fixture stem")
                .to_owned();
            let bytes = fs::read(&path).expect("read swept CXF fixture");
            (stem, bytes)
        })
        .collect()
}

/// A deterministic in-domain value for `definition` at frame `step`: a declared bound when there is
/// one (bounds are inclusive), otherwise a value that moves with the step so stateful blocks see
/// a changing signal.
fn frame_value(definition: &InputDefinition, step: u32) -> Value {
    if let Some(bound) = definition.min.as_ref().or(definition.max.as_ref()) {
        return bound.clone();
    }
    match definition.value_type {
        ValueType::Real => Value::Real(20.0 + f64::from(step) * 0.5),
        ValueType::Integer => Value::Integer(i64::from(step)),
        ValueType::Boolean => Value::Boolean(step.is_multiple_of(2)),
        other => panic!(
            "unbounded {other:?} input {} has no deterministic frame value",
            definition.path
        ),
    }
}

/// Load, export, step and snapshot one document, digesting every observed output.
fn observe_document(stem: &str, bytes: &[u8]) -> Fingerprint {
    let mut engine = Engine::new();
    let (receipt, loaded) = engine
        .load_cxf_with_receipt(bytes)
        .unwrap_or_else(|failure| panic!("{stem} loads: {:?}", failure.error()));
    let export = engine
        .export_cxf_with_receipt()
        .unwrap_or_else(|failure| panic!("{stem} exports: {:?}", failure.error()));
    let definitions = engine
        .input_definitions()
        .unwrap_or_else(|error| panic!("{stem} input definitions: {error:?}"));

    let mut frames = Vec::new();
    for step in 0..FRAMES {
        let values: Vec<(String, Value)> = definitions
            .iter()
            .map(|definition| (definition.path.clone(), frame_value(definition, step)))
            .collect();
        let entries: Vec<(&str, Value)> = values
            .iter()
            .map(|(path, value)| (path.as_str(), value.clone()))
            .collect();
        let prepared = engine
            .prepare_frame(f64::from(step), &entries)
            .unwrap_or_else(|error| panic!("{stem} prepares frame {step}: {error:?}"));
        let completed = engine
            .execute_frame(prepared)
            .unwrap_or_else(|error| panic!("{stem} executes frame {step}: {error:?}"));
        frames.push(format!(
            "{:?} {:?}",
            completed.outputs(),
            completed.diagnostics()
        ));
    }
    let snapshot = engine
        .state_snapshot()
        .unwrap_or_else(|error| panic!("{stem} snapshots after {FRAMES} frames: {error:?}"));
    let snapshot = digest(snapshot.as_bytes());

    let report = export.report();
    vec![
        digest(&report.bytes),
        debug_digest(&report.complete()),
        debug_digest(&engine.content_id()),
        debug_digest(export.diagnostics()),
        debug_digest(receipt.report()),
        debug_digest(receipt.diagnostics()),
        debug_digest(&definitions),
        debug_digest(&engine.io().to_vec()),
        debug_digest(&loaded.model),
        debug_digest(engine.schedule()),
        debug_digest(&engine.topology()),
        debug_digest(&frames),
        snapshot,
    ]
}

/// Observe every document of `corpus` on [`ITERATIONS`] fresh threads and panic, naming the
/// document and output, at the first byte difference between runs.
fn assert_hasher_seed_independent(
    corpus: &[(String, Vec<u8>)],
    components: &[&str],
    observe: fn(&str, &[u8]) -> Fingerprint,
) {
    let runs = run_on_fresh_threads(ITERATIONS, || {
        corpus
            .iter()
            .map(|(stem, bytes)| {
                let fingerprint = observe(stem, bytes);
                assert_eq!(fingerprint.len(), components.len());
                fingerprint
            })
            .collect::<Vec<_>>()
    });
    if let Some(divergence) = first_divergence(&runs) {
        panic!(
            "hash iteration order reached `{}` of `{}`: run {} differs from run 0",
            components[divergence.component], corpus[divergence.document].0, divergence.run
        );
    }
}

#[test]
fn engine_outputs_are_independent_of_hasher_seeds() {
    assert_hasher_seed_independent(&corpus(), &COMPONENTS, observe_document);
}

/// Refused documents whose load failure carries two or more diagnostics: the swept corpus loads
/// without warnings, so these are what exercise diagnostic ordering under differing seeds.
const REFUSALS: [&str; 4] = [
    "composite_contract/rejected/diamond_cycle.jsonld",
    "composite_contract/rejected/partial_port_declaration.jsonld",
    "invalid/two_undriven.jsonld",
    "node_scoped_contexts.jsonld",
];

/// The two outputs of a refused load.
const REFUSAL_COMPONENTS: [&str; 2] = ["load error", "load failure receipt"];

fn observe_refusal(stem: &str, bytes: &[u8]) -> Fingerprint {
    let failure = match Engine::new().load_cxf_with_receipt(bytes) {
        Ok(_) => panic!("{stem} must refuse to load"),
        Err(failure) => failure,
    };
    assert!(
        failure.diagnostics().records().len() >= 2,
        "{stem} no longer refuses with several diagnostics, so it no longer tests their order"
    );
    vec![
        debug_digest(failure.error()),
        debug_digest(failure.diagnostics()),
    ]
}

#[test]
fn refusal_diagnostics_are_independent_of_hasher_seeds() {
    let corpus: Vec<(String, Vec<u8>)> = REFUSALS
        .iter()
        .map(|relative| {
            let path = format!(
                "{}/../oce-cxf/tests/fixtures/{relative}",
                env!("CARGO_MANIFEST_DIR")
            );
            let bytes = fs::read(&path).unwrap_or_else(|error| panic!("read {path}: {error}"));
            ((*relative).to_owned(), bytes)
        })
        .collect();
    assert_hasher_seed_independent(&corpus, &REFUSAL_COMPONENTS, observe_refusal);
}

/// A stand-in engine with a fixed vocabulary of names; `sorted` decides whether it emits them in
/// `HashMap` iteration order (an order leak) or canonicalizes them first.
fn synthetic_engine(sorted: bool) -> Vec<Fingerprint> {
    let names: HashMap<String, usize> = (0..32).map(|i| (format!("point_{i:02}"), i)).collect();
    let mut emitted: Vec<&str> = names.keys().map(String::as_str).collect();
    if sorted {
        emitted.sort_unstable();
    }
    vec![vec![debug_digest(&emitted)]]
}

#[test]
fn harness_detects_hash_iteration_order() {
    let runs = run_on_fresh_threads(ITERATIONS, || synthetic_engine(false));
    let divergence = first_divergence(&runs);
    assert!(
        matches!(
            divergence,
            Some(Divergence {
                document: 0,
                component: 0,
                ..
            })
        ),
        "fresh threads must reseed RandomState, so 32 keys iterated on {ITERATIONS} threads \
         cannot all agree; got {divergence:?}"
    );
}

#[test]
fn harness_accepts_canonicalized_hash_contents() {
    let runs = run_on_fresh_threads(ITERATIONS, || synthetic_engine(true));
    assert_eq!(first_divergence(&runs), None);
}

#[test]
fn divergence_names_the_first_differing_run_document_and_component() {
    let runs = vec![
        vec![vec![1, 2], vec![3, 4]],
        vec![vec![1, 2], vec![3, 4]],
        vec![vec![1, 2], vec![3, 9]],
        vec![vec![7, 2], vec![3, 4]],
    ];
    assert_eq!(
        first_divergence(&runs),
        Some(Divergence {
            document: 1,
            component: 1,
            run: 2,
        })
    );
    assert_eq!(first_divergence(&runs[..2]), None);
    assert_eq!(first_divergence(&[]), None);
}
