//! What exporting a model costs for each array element and each connector, in requested bytes and
//! never in elapsed time (issues #803, #799; `TESTING.md`: cost is asserted with deterministic
//! measures).
//!
//! An array parameter `k[n]` is `n` scalar parameters, each written as a parameter node whose
//! `@id` is the block's IRI, `.` and the element's name, and listed again in the block's
//! `S231:hasParameter`. The exporter built every node of the document before serializing it, a
//! 1,120-byte node for each element, more while the node vector grew, and held a copy of each
//! element's `@id` in its plan and another in its duplicate check: a `MatrixGain` whose `K` is 64 by
//! 64 exported at a peak of 14.7 MB and 128 by 128 at 58.7 MB, for 0.4 MB and 1.7 MB of bytes. It
//! now writes one node at a time into a buffer of exactly the document's size, spells each `@id`
//! from the graph, and holds a parameter in its duplicate check as the block's IRI and the name,
//! borrowed. So an export's peak is about the bytes it returns, and its plan holds a fixed size for
//! each parameter whatever the IRI's length. Each connector's diagnostic subject, a copy of its
//! block's IRI, was built whether or not a finding used it (#799); it is now borrowed.
//!
//! Measured with `allocation-counter`, whose counters are local to the measuring thread. The
//! ceilings have headroom over what was measured (in brackets, with what the same model cost
//! before the change).

use std::sync::Arc;

use allocation_counter::{AllocationInfo, measure};
use oce_api::Engine;
use oce_cxf::{CxfError, ResolveOptions, export_with_report, import_cxf};
use oce_diag::DiagCode;
use oce_model::{
    BlockId, BlockInstance, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value, ValueType,
};

use crate::cxf_minted_array_cost::{MODEL, array, named_leaves, rooted_gain};

/// The most an export's peak may exceed the bytes it returns, as a fraction of them, and a
/// constant: its plan, one node and the deferral list. Holding a second copy of the bytes, or a
/// buffer that grew instead of being sized, breaks it.
fn within_its_bytes(peak: u64, bytes: usize) -> bool {
    peak <= bytes as u64 + bytes as u64 / 8 + 64 * 1024
}

/// One export of the model `engine` holds, measured: the bytes' length and capacity.
fn exported(engine: &Engine) -> (usize, usize, AllocationInfo) {
    let mut report = None;
    let info = measure(|| report = Some(engine.export_cxf()));
    let report = report.expect("ran").expect("the model exports");
    (report.bytes.len(), report.bytes.capacity(), info)
}

/// One export of `graph` by `oce_cxf`, measured, after a warm-up export of the minimal loop.
fn exported_graph(graph: &ModelGraph) -> (Result<usize, CxfError>, AllocationInfo) {
    let (model, _) = import_cxf(MODEL.as_bytes(), &ResolveOptions::default()).expect("reads");
    export_with_report(&model).expect("the model exports");
    let mut report = None;
    let info = measure(|| report = Some(export_with_report(graph)));
    (
        report.expect("ran").map(|r| {
            assert_eq!(r.bytes.capacity(), r.bytes.len(), "sized exactly");
            r.bytes.len()
        }),
        info,
    )
}

/// A `MatrixGain` of `rows` by `cols` under `root`, loaded through the facade.
fn gain(root: &str, rows: usize, cols: usize) -> Engine {
    let mut engine = Engine::new();
    engine
        .load_cxf(&rooted_gain(root, rows, cols))
        .expect("the gain loads");
    engine
}

#[test]
fn a_gain_exports_at_about_the_bytes_it_writes() {
    // `K` of 4,096, 8,192 and 16,384 elements under a short root: the peak is the bytes plus at
    // most an eighth, and grows by about the bytes an element writes, 102.
    // [measured: 420,886 / 838,890 / 1,667,236 bytes at peaks of 462,773 / 906,781 / 1,750,289,
    // 105 bytes an element; before, peaks of 14,693,743 / 29,371,687 / 58,704,353]
    Engine::new().load_cxf(MODEL.as_bytes()).expect("loads");
    let mut peaks = Vec::new();
    for (rows, cols) in [(64, 64), (64, 128), (128, 128)] {
        let (bytes, capacity, info) = exported(&gain("http://example.org#M", rows, cols));
        assert_eq!(capacity, bytes, "the buffer is sized exactly");
        assert!(
            within_its_bytes(info.bytes_max, bytes),
            "{rows}x{cols}: {bytes} bytes, {info:?}"
        );
        peaks.push(info.bytes_max);
    }
    let per_element = (peaks[2] - peaks[0]) / (16_384 - 4_096);
    assert!(
        per_element <= 128,
        "{per_element} bytes an element: {peaks:?}"
    );
}

#[test]
fn a_gain_under_a_long_root_exports_at_about_the_bytes_it_writes() {
    // The same under a 921-byte root, each element's `@id` 934 bytes or so: 1,024, 2,048 and
    // 4,096 elements, and 90 by 90, the most the minted path budget admits under this root.
    // [measured: 2,102,346 / 4,139,818 / 8,093,802 / 15,828,568 bytes at peaks of 2,328,248 /
    // 4,522,840 / 8,542,040 / 16,458,366; before, peaks of 8,667,308 / 17,268,588 / 34,288,940 /
    // 68,096,216]
    let root = format!("urn:{}", "r".repeat(917));
    for (rows, cols) in [(32, 32), (32, 64), (64, 64), (90, 90)] {
        let (bytes, capacity, info) = exported(&gain(&root, rows, cols));
        assert_eq!(capacity, bytes);
        assert!(
            within_its_bytes(info.bytes_max, bytes),
            "{rows}x{cols}: {bytes} bytes, {info:?}"
        );
    }
}

#[test]
fn a_gain_at_the_minted_budget_exports_at_about_the_bytes_it_writes() {
    // 256 by 256, the whole minted element budget, under the short root: the most elements a
    // document may mint, exported. [measured: 6,709,540 bytes at a peak of 6,875,281; before,
    // about 235 MB projected from 3.6 KB an element, not run]
    let (bytes, _, info) = exported(&gain("http://example.org#M", 256, 256));
    assert!(
        within_its_bytes(info.bytes_max, bytes),
        "{bytes} bytes, {info:?}"
    );
    assert!(info.bytes_max <= 12 * 1024 * 1024, "{info:?}");
}

/// A `Constant` block named by an IRI of `iri_bytes` bytes, shared by `Arc` as a host may share
/// it, with `count` Real parameters `k_1` … and, when `refused`, a last one that is not finite.
fn one_block(iri_bytes: usize, count: usize, refused: bool) -> ModelGraph {
    let iri: Arc<str> = Arc::from(format!("urn:{}", "b".repeat(iri_bytes - 4)));
    let mut values: Vec<(Arc<str>, Value)> = (1..=count)
        .map(|i| (Arc::from(format!("k_{i}")), Value::Real(i as f64)))
        .collect();
    if refused {
        values.push((Arc::from("nan"), Value::Real(f64::NAN)));
    }
    ModelGraph {
        blocks: vec![BlockInstance {
            id: BlockId(0),
            class_iri: Arc::from("CDL.Reals.Sources.Constant"),
            inputs: vec![],
            outputs: vec![ConnectorId(0)],
            params: ParamTable { values },
            decl_order: 0,
            instance_iri: Some(iri),
        }],
        connectors: vec![Connector::new(
            ConnectorId(0),
            BlockId(0),
            Dir::Out,
            ValueType::Real,
            0,
        )],
        ..ModelGraph::new()
    }
}

#[test]
fn the_plan_holds_a_fixed_size_for_each_parameter_whatever_its_blocks_iri() {
    // 1,024 parameters on a block whose IRI is 1, 4 and 16 KiB, with one more that is not
    // finite, so the export is refused once its plan is made and nothing is written: what the
    // plan held is the peak. It holds the IRI a few times, never once for each parameter.
    // [measured: peaks of 81,041 / 99,473 / 173,201, six copies of the IRI; before, 3,305,436 /
    // 12,767,196 / 50,614,236]
    let mut peaks = Vec::new();
    for iri_bytes in [1_024, 4_096, 16_384] {
        let (result, info) = exported_graph(&one_block(iri_bytes, 1_024, true));
        let Err(CxfError::Validation(diagnostics)) = result else {
            panic!("the export is refused: {result:?}");
        };
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagCode::ExportUnsupported);
        peaks.push(info.bytes_max);
    }
    // A few copies of the IRI: the block's planned node, its one port's minted `@id` (held as the
    // port's, as its key and in the block's outputs) and the refusal's subject. Each `@id` held
    // whole was 1,024 copies.
    assert!(peaks[2] - peaks[0] <= 8 * (16_384 - 1_024), "{peaks:?}");
}

#[test]
fn a_block_whose_parameters_share_a_long_iri_exports_at_about_the_bytes_it_writes() {
    // The same block, all finite: the document spells the 4 KiB IRI twice for each of 256, 512
    // and 1,024 parameters, and that is what the export costs. [measured: 2,127,231 / 4,237,951 /
    // 8,459,466 bytes at peaks of 2,148,849 / 4,259,569 / 8,481,084; before, peaks of 7,018,879 /
    // 14,015,871 / 28,009,880]
    for count in [256, 512, 1_024] {
        let (result, info) = exported_graph(&one_block(4_096, count, false));
        let bytes = result.expect("the block exports");
        assert!(bytes >= count * 2 * 4_096);
        assert!(
            within_its_bytes(info.bytes_max, bytes),
            "{count}: {bytes} bytes, {info:?}"
        );
    }
}

#[test]
fn string_arrays_and_a_node_many_leaves_list_export_at_about_the_bytes_they_write() {
    // Read by the reader alone (the class declares neither name): a written string array of
    // 1,024 elements on one leaf, and a written Real `q[1024]` listed by 4 leaves, whose further
    // listings mint their elements. [measured: 79,614 and 262,329 bytes at peaks of 87,149 and
    // 305,950; before, 3,638,980 and 14,181,471]
    // Each element is the CDL literal `"s\"<i>"`, written as a JSON string.
    let strings: Vec<String> = (0..1_024).map(|i| format!(r#""\"s\\\"{i}\"""#)).collect();
    let written_strings = array("urn:a.s", &[1_024], &format!("[{}]", strings.join(", ")));
    let written_q = array(
        "urn:a.q",
        &[1_024],
        &format!("[{}]", vec!["1.5"; 1_024].join(", ")),
    );
    for (what, leaves, listed, node) in [
        ("strings", 1, "urn:a.s", written_strings),
        ("one node listed by four", 4, "urn:a.q", written_q),
    ] {
        let doc = named_leaves(
            leaves,
            |i| format!("urn:c{i}"),
            |_| vec![listed.to_owned()],
            &[node],
        );
        let (graph, _) = import_cxf(&doc, &ResolveOptions::default()).expect("reads");
        let (result, info) = exported_graph(&graph);
        let bytes = result.expect("exports");
        assert!(
            within_its_bytes(info.bytes_max, bytes),
            "{what}: {bytes} bytes, {info:?}"
        );
    }
}

/// A `Constant` block named by an IRI of `iri_bytes` bytes with `count` Real outputs, each with
/// a short IRI of its own.
fn many_outputs(iri_bytes: usize, count: usize) -> ModelGraph {
    let iri: Arc<str> = Arc::from(format!("urn:{}", "o".repeat(iri_bytes - 4)));
    let connectors: Vec<Connector> = (0..count)
        .map(|k| {
            let mut c = Connector::new(
                ConnectorId(k as u32),
                BlockId(0),
                Dir::Out,
                ValueType::Real,
                k as u32,
            );
            c.iri = Some(Arc::from(format!("urn:y{k}")));
            c
        })
        .collect();
    ModelGraph {
        blocks: vec![BlockInstance {
            id: BlockId(0),
            class_iri: Arc::from("CDL.Reals.Sources.Constant"),
            inputs: vec![],
            outputs: connectors.iter().map(|c| c.id).collect(),
            params: ParamTable { values: vec![] },
            decl_order: 0,
            instance_iri: Some(iri),
        }],
        connectors,
        ..ModelGraph::new()
    }
}

#[test]
fn a_connector_with_no_finding_copies_nothing_of_its_blocks_iri() {
    // 256, 512 and 1,024 outputs on a block whose IRI is 64 KiB, each output named by its own
    // short IRI: the document names the block once, and a further output costs its own port
    // node, never a copy of the block's IRI. [measured: totals of 473,096 / 614,752 / 898,328,
    // 553 bytes a further output; before, 18,702,598 / 36,747,406 / 72,837,190, 70,494 bytes a
    // further output]
    let mut totals = Vec::new();
    for count in [256, 512, 1_024] {
        let (result, info) = exported_graph(&many_outputs(64 * 1024, count));
        let bytes = result.expect("the block exports");
        // The block's IRI is written twice (the root's `containsBlock`, the block's `@id`), and
        // each output adds its own short port node.
        assert!(
            bytes <= 2 * 64 * 1024 + count * 128,
            "{count}: {bytes} bytes"
        );
        totals.push(info.bytes_total);
    }
    let per_output = (totals[2] - totals[0]) / (1_024 - 256);
    assert!(
        per_output <= 1_024,
        "{per_output} bytes an output: {totals:?}"
    );
}
