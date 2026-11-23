//! What a load and each later view of a model cost for every wire, boundary target or boundary
//! output that names one long identity, in requested bytes and never in elapsed time (issue #811;
//! `TESTING.md`: cost is asserted with deterministic measures).
//!
//! A document can write a long IRI once, under a `@context` prefix, and then name its node from
//! any number of short nodes: a `Constant` whose output fans out to `n` blocks, a block with `n`
//! wired ports, a boundary input that drives `n` blocks or `n` boundary outputs. The facade held
//! a copy of the long IRI for each: every `ConnectionDto` copied its source point's key, every
//! `PointDto` its owner block's, the content id's records their block names and the state
//! manifest every connector key's block IRI. A fanned-out `Constant` with a 64 KiB IRI and 256
//! targets, a 161 KB document, was held at 17.4 MB and its id computed at a 33.7 MB peak. Each
//! is now a shared handle, a borrow or an index, so what is held or requested for each reference
//! no longer depends on the identity's length.
//!
//! Each test builds its shape at three sizes (`n` = 16, 32 and 64 references) and two identity
//! lengths, and reads from the four costs of a pair of sizes how many copies of the identity each
//! further reference costs: `((c(2n, L2) - c(2n, L1)) - (c(n, L2) - c(n, L1))) / (n (L2 - L1))`.
//! A copy restored anywhere makes that one or more; the ceilings below allow a small fraction.
//! The identity's own copies (its node, its key) cancel out, and so does everything a reference
//! costs whatever the length.
//!
//! Measured with `allocation-counter`, whose counters are local to the measuring thread.

use allocation_counter::{AllocationInfo, measure};
use oce_api::Engine;

const T: &str = "http://example.org#Buildings.Controls.OBC.CDL.";

/// The two identity lengths, and their difference.
const SHORT: usize = 1_024;
const LONG: usize = 8_192;

/// The reference counts each test doubles from: 16 → 32 and 32 → 64.
const SIZES: [usize; 2] = [16, 32];

/// The document's `@context`: `p` and `q` expand to two IRIs `l` bytes longer than they are
/// written, under which a short name is a long relative name too (no `#`, `/` or `:` follows).
fn context(l: usize) -> String {
    let long = "k".repeat(l);
    format!(
        "\"@context\": {{ \"S231\": \"http://data.ashrae.org/S231#\", \"p\": \"urn:x:{long}\", \
         \"q\": \"urn:y:{long}\" }}"
    )
}

fn port(id: &str, kind: &str, extra: &str) -> String {
    format!(
        "{{ \"@id\": \"{id}\", \"@type\": \"S231:Real{kind}\", \"S231:isOfDataType\": \
         {{ \"@id\": \"S231:Real\" }}{extra} }}"
    )
}

fn wired_to(targets: &[String]) -> String {
    let items: Vec<String> = targets
        .iter()
        .map(|target| format!("{{ \"@id\": \"{target}\" }}"))
        .collect();
    format!(", \"S231:isConnectedTo\": [ {} ]", items.join(", "))
}

fn constant(id: &str, output: &str, nodes: &mut Vec<String>) {
    nodes.push(format!(
        "{{ \"@id\": \"{id}\", \"@type\": \"{T}Reals.Sources.Constant\", \"S231:hasParameter\": \
         {{ \"@id\": \"{id}.k\" }}, \"S231:hasOutput\": {{ \"@id\": \"{id}.y\" }} }}"
    ));
    nodes.push(format!("{{ \"@id\": \"{id}.k\", \"S231:value\": 1.0 }}"));
    nodes.push(port(&format!("{id}.y"), "Output", output));
}

fn abs(id: &str, nodes: &mut Vec<String>) {
    nodes.push(format!(
        "{{ \"@id\": \"{id}\", \"@type\": \"{T}Reals.Abs\", \"S231:hasInput\": {{ \"@id\": \
         \"{id}.u\" }}, \"S231:hasOutput\": {{ \"@id\": \"{id}.y\" }} }}"
    ));
    nodes.push(port(&format!("{id}.u"), "Input", ""));
    nodes.push(port(&format!("{id}.y"), "Output", ""));
}

fn multi_sum(id: &str, inputs: &[String], nodes: &mut Vec<String>) {
    let listed: Vec<String> = inputs
        .iter()
        .map(|input| format!("{{ \"@id\": \"{input}\" }}"))
        .collect();
    nodes.push(format!(
        "{{ \"@id\": \"{id}\", \"@type\": \"{T}Reals.MultiSum\", \"S231:hasParameter\": \
         {{ \"@id\": \"{id}.nin\" }}, \"S231:hasInput\": [ {} ], \"S231:hasOutput\": \
         {{ \"@id\": \"{id}.y\" }} }}",
        listed.join(", ")
    ));
    nodes.push(format!(
        "{{ \"@id\": \"{id}.nin\", \"S231:value\": {} }}",
        inputs.len()
    ));
    for input in inputs {
        nodes.push(port(input, "Input", ""));
    }
    nodes.push(port(&format!("{id}.y"), "Output", ""));
}

/// The shapes, each naming one long identity `n` times.
#[derive(Clone, Copy, Debug)]
enum Shape {
    /// A long `Constant` whose output drives `n` short `Abs` blocks (the issue's reproducer).
    FanOut,
    /// `n` short `Constant`s driving the `n` short-IRI inputs of one long `MultiSum`.
    FanIn,
    /// A long `Constant` driving every input of a long `MultiSum`: both ends long on each wire.
    Between,
    /// A long root boundary input driving `n` short `Abs` blocks.
    BoundaryFanOut,
    /// A long `Constant` driving `n` short root boundary outputs.
    BoundaryOutputs,
    /// A long root boundary input passed straight through to `n` short root boundary outputs.
    PassThrough,
}

/// The document of `shape` with `n` references and identities `l` bytes longer than written.
fn document(shape: Shape, l: usize, n: usize) -> Vec<u8> {
    let mut nodes = Vec::new();
    let mut blocks: Vec<String> = Vec::new();
    let mut inputs: Vec<String> = Vec::new();
    let mut outputs: Vec<String> = Vec::new();
    let short = |stem: &str| (0..n).map(|i| format!("urn:{stem}{i}")).collect::<Vec<_>>();
    match shape {
        Shape::FanOut => {
            let targets: Vec<String> = short("a").iter().map(|a| format!("{a}.u")).collect();
            constant("p:c", &wired_to(&targets), &mut nodes);
            blocks.push("p:c".into());
            for a in short("a") {
                abs(&a, &mut nodes);
                blocks.push(a);
            }
        }
        Shape::FanIn => {
            let ports: Vec<String> = (1..=n).map(|i| format!("urn:m.u{i}")).collect();
            for (c, input) in short("c").into_iter().zip(&ports) {
                constant(&c, &wired_to(std::slice::from_ref(input)), &mut nodes);
                blocks.push(c);
            }
            multi_sum("p:m", &ports, &mut nodes);
            blocks.push("p:m".into());
        }
        Shape::Between => {
            let ports: Vec<String> = (1..=n).map(|i| format!("urn:m.u{i}")).collect();
            constant("p:c", &wired_to(&ports), &mut nodes);
            multi_sum("q:m", &ports, &mut nodes);
            blocks.extend(["p:c".into(), "q:m".into()]);
        }
        Shape::BoundaryFanOut => {
            let targets: Vec<String> = short("a").iter().map(|a| format!("{a}.u")).collect();
            nodes.push(port("p:u", "Input", &wired_to(&targets)));
            inputs.push("p:u".into());
            for a in short("a") {
                abs(&a, &mut nodes);
                blocks.push(a);
            }
        }
        Shape::PassThrough => {
            let targets = short("y");
            nodes.push(port("p:u", "Input", &wired_to(&targets)));
            inputs.push("p:u".into());
            for y in targets {
                nodes.push(port(&y, "Output", ""));
                outputs.push(y);
            }
            // A root contains at least one block.
            constant("urn:c", "", &mut nodes);
            blocks.push("urn:c".into());
        }
        Shape::BoundaryOutputs => {
            let targets = short("y");
            constant("p:c", &wired_to(&targets), &mut nodes);
            blocks.push("p:c".into());
            for y in targets {
                nodes.push(port(&y, "Output", ""));
                outputs.push(y);
            }
        }
    }
    let list = |ids: &[String]| {
        ids.iter()
            .map(|id| format!("{{ \"@id\": \"{id}\" }}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "{{ {}, \"@graph\": [ {{ \"@id\": \"urn:top\", \"@type\": \"S231:Block\", \
         \"S231:containsBlock\": [ {} ], \"S231:hasInput\": [ {} ], \"S231:hasOutput\": [ {} ] }},\n\
         {} ] }}",
        context(l),
        list(&blocks),
        list(&inputs),
        list(&outputs),
        nodes.join(",\n")
    )
    .into_bytes()
}

/// What one model costs at each stage, in bytes.
#[derive(Clone, Copy, Debug, Default)]
struct Costs {
    /// The peak of the CXF reader alone (`oce_cxf::import_cxf`), and what its graph holds.
    import_peak: u64,
    import_held: i64,
    /// The peak of `load_cxf`.
    load_peak: u64,
    /// Every byte `load_cxf` requests, freed or not: a path copied and dropped for each
    /// reference shows here though it never raises the peak.
    load_requested: u64,
    /// What the returned `LoadedModel` holds once the engine is dropped.
    loaded_held: i64,
    /// What the engine holds beside the `LoadedModel`.
    engine_held: i64,
    /// The peak of `content_id`.
    content_id_peak: u64,
    /// Every byte `content_id` requests, freed or not.
    content_id_requested: u64,
    /// The peak of `compile_blob`.
    blob_peak: u64,
    /// Every byte `compile_blob` requests, freed or not.
    blob_requested: u64,
    /// The peak of `canonical_content_bytes`, and the bytes' length.
    canonical_peak: u64,
    canonical_len: usize,
    /// The peak of `state_snapshot`, and the snapshot's length.
    snapshot_peak: u64,
    snapshot_len: usize,
    /// The peak of `checkpoint`.
    checkpoint_peak: u64,
}

fn costs(shape: Shape, l: usize, n: usize) -> Costs {
    let document = document(shape, l, n);
    // Warm up: a first load fills the registry's and the catalog's lazily built tables.
    let mut warm = Engine::new();
    warm.load_cxf(&document).expect("the shape loads");
    warm.content_id().expect("id");
    drop(warm.state_snapshot().expect("snapshot"));
    let mut graph = None;
    let import = measure(|| {
        graph = Some(
            oce_cxf::import_cxf(&document, &oce_cxf::ResolveOptions::default()).expect("reads"),
        );
    });
    drop(graph);
    let mut kept = None;
    let load = measure(|| {
        let mut engine = Engine::new();
        let (_, loaded) = engine.load_cxf(&document).expect("the shape loads");
        kept = Some((engine, loaded));
    });
    let (engine, loaded) = kept.expect("ran");
    let mut loaded_only = None;
    let loaded_info = measure(|| {
        let mut engine = Engine::new();
        let (_, loaded) = engine.load_cxf(&document).expect("the shape loads");
        drop(engine);
        loaded_only = Some(loaded);
    });
    drop((loaded, loaded_only));
    let peak = |info: AllocationInfo| info.bytes_max;
    let content_id = measure(|| drop(engine.content_id().expect("id")));
    let blob = measure(|| drop(engine.compile_blob().expect("blob")));
    let mut canonical_len = 0;
    let canonical = measure(|| {
        canonical_len = engine.canonical_content_bytes().expect("bytes").len();
    });
    let mut snapshot_len = 0;
    let snapshot = measure(|| {
        snapshot_len = engine.state_snapshot().expect("snapshot").as_bytes().len();
    });
    let checkpoint = measure(|| drop(engine.checkpoint().expect("checkpoint")));
    Costs {
        import_peak: import.bytes_max,
        import_held: import.bytes_current,
        load_peak: peak(load),
        load_requested: load.bytes_total,
        loaded_held: loaded_info.bytes_current,
        engine_held: load.bytes_current - loaded_info.bytes_current,
        content_id_peak: peak(content_id),
        blob_peak: peak(blob),
        blob_requested: blob.bytes_total,
        content_id_requested: content_id.bytes_total,
        canonical_peak: peak(canonical),
        canonical_len,
        snapshot_peak: peak(snapshot),
        snapshot_len,
        checkpoint_peak: peak(checkpoint),
    }
}

/// The costs of `shape` at both identity lengths and every size: `(l, n)` in the order
/// [`copies`] reads them.
struct Grid(Vec<((usize, usize), Costs)>);

fn grid(shape: Shape) -> Grid {
    let mut points = Vec::new();
    for l in [SHORT, LONG] {
        for n in [SIZES[0], SIZES[1], 2 * SIZES[1]] {
            points.push(((l, n), costs(shape, l, n)));
        }
    }
    Grid(points)
}

impl Grid {
    /// The copies of the identity each further reference costs at `stage`, for each pair of
    /// sizes.
    fn copies(&self, stage: impl Fn(&Costs) -> f64) -> Vec<f64> {
        let at = |l: usize, n: usize| {
            self.0
                .iter()
                .find(|(point, _)| *point == (l, n))
                .map(|(_, costs)| stage(costs))
                .expect("measured")
        };
        SIZES
            .iter()
            .map(|&n| {
                let further = (at(LONG, 2 * n) - at(SHORT, 2 * n)) - (at(LONG, n) - at(SHORT, n));
                further / (n as f64 * (LONG - SHORT) as f64)
            })
            .collect()
    }
}

/// Every stage, by name, and the copies of the identity a reference may cost there beyond what
/// the stage returns (the canonical and snapshot bytes are the format's and carry the names).
const SHAPES: [Shape; 6] = [
    Shape::FanOut,
    Shape::FanIn,
    Shape::Between,
    Shape::BoundaryFanOut,
    Shape::BoundaryOutputs,
    Shape::PassThrough,
];

/// The most a reference may cost of the identity at a stage that returns nothing per reference.
const NONE: f64 = 0.05;

#[test]
fn a_load_holds_each_long_identity_once_however_many_references_name_it() {
    // The issue's terms. [measured: 0.00 copies a reference at every shape and both sizes, for
    // the reader, the LoadedModel, the engine and the load's peak; before, the LoadedModel held 1
    // (fan-out, fan-in) or 2 (both ends long), the reader held 1 and peaked at 2 for a boundary
    // input passed through, and the engine held that 1]
    for shape in SHAPES {
        let grid = grid(shape);
        for (stage, copies) in [
            ("reader peak", grid.copies(|c| c.import_peak as f64)),
            ("reader graph", grid.copies(|c| c.import_held as f64)),
            ("LoadedModel", grid.copies(|c| c.loaded_held as f64)),
            ("engine", grid.copies(|c| c.engine_held as f64)),
            ("load peak", grid.copies(|c| c.load_peak as f64)),
            ("load requested", grid.copies(|c| c.load_requested as f64)),
        ] {
            assert!(
                copies.iter().all(|c| *c <= NONE),
                "{shape:?} {stage}: {copies:?} copies a reference"
            );
        }
    }
}

#[test]
fn an_id_and_a_blob_cost_their_records_whatever_the_names_they_repeat() {
    // [measured: 0.00 copies a reference for `content_id` and `compile_blob`; before, 2 (one
    // in the records, one in the bytes), 4 with both ends long]
    for shape in SHAPES {
        let grid = grid(shape);
        for (stage, copies) in [
            ("content_id", grid.copies(|c| c.content_id_peak as f64)),
            (
                "content_id requested",
                grid.copies(|c| c.content_id_requested as f64),
            ),
            ("compile_blob", grid.copies(|c| c.blob_peak as f64)),
            (
                "compile_blob requested",
                grid.copies(|c| c.blob_requested as f64),
            ),
        ] {
            assert!(
                copies.iter().all(|c| *c <= NONE),
                "{shape:?} {stage}: {copies:?} copies a reference"
            );
        }
    }
}

#[test]
fn the_canonical_bytes_cost_what_they_spell() {
    // The bytes repeat a block's name in every record that names it: that is the format. The
    // peak is the bytes and nothing more for each reference. [measured: the peak's copies equal
    // the bytes' (1, or 2 with both ends long, and 0 for a boundary input's targets); before,
    // one more]
    for shape in SHAPES {
        let grid = grid(shape);
        let peak = grid.copies(|c| c.canonical_peak as f64);
        let bytes = grid.copies(|c| c.canonical_len as f64);
        for (peak, bytes) in peak.iter().zip(&bytes) {
            assert!(
                *peak <= bytes + NONE,
                "{shape:?}: a peak of {peak} copies a reference for {bytes} in the bytes"
            );
        }
    }
}

#[test]
fn a_state_manifest_names_each_block_once_however_many_keys_carry_it() {
    // A snapshot's bytes spell a block's IRI in every connector key that names the block; that
    // is the format, and `MAX_SNAPSHOT_BYTES` caps the bytes. The manifest the snapshot and the
    // checkpoint build shares the IRI instead, and its encoding is counted before it is written,
    // so what is left for each reference is the bytes: a snapshot encodes the manifest twice (once
    // for the fingerprint), writes the snapshot and reads it back, and a checkpoint encodes the
    // manifest once. [measured, copies a reference (snapshot peak / bytes / checkpoint peak):
    // fan-out 8 / 2 / 2, fan-in 22 / 6 / 5, both ends 30 / 8 / 7, boundary input 4 / 1 / 1,
    // boundary outputs 4 / 1 / 1, passed through 60 / 16 / 14; before, measured between 1 KiB
    // and 16 KiB identities, the snapshot peaked at 12.45, 22.65, 41, 6.23, 7 and 82 and the
    // checkpoint at 6.45, 8.65, 21, 3.23, 4 and 44]
    // Each shape's measured copies (snapshot peak, checkpoint peak), pinned to within half a
    // copy: one copy of a key restored anywhere in the manifest moves one of them by a whole copy
    // a reference, which the byte-relative ceilings below alone could absorb.
    let measured = |shape| match shape {
        Shape::FanOut => (8.0, 2.0),
        Shape::FanIn => (22.0, 5.0),
        Shape::Between => (30.0, 7.0),
        Shape::BoundaryFanOut | Shape::BoundaryOutputs => (4.0, 1.0),
        Shape::PassThrough => (60.0, 14.0),
    };
    for shape in SHAPES {
        let grid = grid(shape);
        let bytes = grid.copies(|c| c.snapshot_len as f64);
        let snapshot = grid.copies(|c| c.snapshot_peak as f64);
        let checkpoint = grid.copies(|c| c.checkpoint_peak as f64);
        let (snapshot_measured, checkpoint_measured) = measured(shape);
        for value in &snapshot {
            assert!(
                (value - snapshot_measured).abs() < 0.5,
                "{shape:?}: a snapshot peak of {value} copies a reference, {snapshot_measured} \
                 measured"
            );
        }
        for value in &checkpoint {
            assert!(
                (value - checkpoint_measured).abs() < 0.5,
                "{shape:?}: a checkpoint peak of {value} copies a reference, \
                 {checkpoint_measured} measured"
            );
        }
        for ((bytes, snapshot), checkpoint) in bytes.iter().zip(&snapshot).zip(&checkpoint) {
            assert!(
                *snapshot <= 4.0 * bytes + NONE,
                "{shape:?}: a snapshot peak of {snapshot} copies a reference for {bytes} in the \
                 bytes"
            );
            assert!(
                *checkpoint <= bytes + NONE,
                "{shape:?}: a checkpoint peak of {checkpoint} copies a reference for {bytes} in the \
                 snapshot's bytes"
            );
        }
    }
}
