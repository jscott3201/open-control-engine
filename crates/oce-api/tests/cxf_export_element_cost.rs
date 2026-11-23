//! What exporting a model costs for each parameter, port and block, in requested bytes and never
//! in elapsed time (issues #803, #799; `TESTING.md`: cost is asserted with deterministic
//! measures).
//!
//! An array parameter `k[n]` is `n` scalar parameters, each written as a parameter node whose
//! `@id` is the block's IRI, `.` and the element's name, and listed again in the block's
//! `S231:hasParameter`; a port with no IRI of its own is written as `<block IRI>.in<k>` or
//! `.out<k>`. The exporter built every node of the document before serializing it, a 1,120-byte
//! node for each element, more while the node vector grew, and its plan held copies of every
//! block's IRI and of each minted `@id`: a `MatrixGain` whose `K` is 128 by 128 exported 1.7 MB at a
//! peak of 58.7 MB, and documents of many blocks or minted ports peaked at 3.6 and 4.1 times their
//! bytes. It now writes one node at a time into a buffer of exactly the document's size, spells
//! each `@id` from the graph, and holds every block's IRI borrowed and each parameter's and minted
//! port's `@id` as parts, so the plan holds a fixed size for each whatever the IRI's length. The
//! peak is then the bytes and a small share more, which each test states for its shape: an eighth
//! for array elements with short paths (where the duplicate check's fixed size counts), 1/16 for
//! minted ports, 1/32 for blocks, 1/100 for instances listing shared parameters. Each connector's
//! diagnostic subject, a copy of its block's IRI, was built whether or not a finding used it
//! (#799); it is now borrowed.
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
    within(peak, bytes, 8)
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
    // [measured: 420,886 / 838,890 / 1,667,236 bytes at peaks of 449,338 / 882,190 / 1,723,336,
    // 104 bytes an element; before, peaks of 14,693,743 / 29,371,687 / 58,704,353]
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
    // [measured: 2,102,346 / 4,139,818 / 8,093,802 / 15,828,568 bytes at peaks of 2,116,974 /
    // 4,161,870 / 8,122,254 / 15,869,468; before, peaks of 8,667,308 / 17,268,588 / 34,288,940 /
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
    // document may mint, exported. [measured: 6,709,540 bytes at a peak of 6,820,936; before,
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
    // [measured: peaks of 76,624 / 82,768 / 107,344, two copies of the IRI; before, 3,305,436 /
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
    // A couple of copies of the IRI, the refusal's subject among them. Each `@id` held whole was
    // 1,024 copies, and the block's planned node and its minted port's `@id` four more.
    assert!(peaks[2] - peaks[0] <= 4 * (16_384 - 1_024), "{peaks:?}");
}

#[test]
fn a_block_whose_parameters_share_a_long_iri_exports_at_about_the_bytes_it_writes() {
    // The same block, all finite: the document spells the 4 KiB IRI twice for each of 256, 512
    // and 1,024 parameters, and that is what the export costs. [measured: 2,127,231 / 4,237,951 /
    // 8,459,466 bytes at peaks of 2,127,627 / 4,238,347 / 8,459,862; before, peaks of 7,018,879 /
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
    // listings mint their elements. [measured: 79,614 and 262,329 bytes at peaks of 84,982 and
    // 304,758; before, 3,638,980 and 14,181,471]
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
    // node, never a copy of the block's IRI. [measured: totals of 265,452 / 398,404 / 664,452,
    // 519 bytes a further output; before, 18,702,598 / 36,747,406 / 72,837,190, 70,494 bytes a
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

/// The Real and Boolean parameters of `CDL.Reals.PID` each instance lists, as shared nodes.
const PID_PARAMETERS: &[(&str, &str)] = &[
    ("k", "1.0"),
    ("Ti", "0.5"),
    ("Td", "0.1"),
    ("r", "1.0"),
    ("yMax", "1.0"),
    ("yMin", "0.0"),
    ("Ni", "0.9"),
    ("Nd", "10.0"),
    ("xi_start", "0.0"),
    ("yd_start", "0.0"),
    ("reverseActing", "true"),
];

/// A root holding `count` `CDL.Reals.PID` instances, each named by an IRI of `iri_bytes` bytes,
/// listing the shared parameter nodes `urn:p.<name>` and with both inputs driven by the root's
/// one boundary input. With `prefixed`, each instance IRI is written as `x:<i>` under a
/// `@context` prefix that expands to the long stem.
fn pid_instances(count: usize, iri_bytes: usize, prefixed: bool) -> Vec<u8> {
    let stem = format!("urn:{}", "i".repeat(iri_bytes - 8));
    let name = |i: usize| {
        if prefixed {
            format!("x:{i:04}")
        } else {
            format!("{stem}{i:04}")
        }
    };
    let mut nodes = Vec::new();
    let instances: Vec<String> = (0..count).map(name).collect();
    let refs = |ids: &[String]| {
        ids.iter()
            .map(|i| format!("{{\"@id\":\"{i}\"}}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    let inputs: Vec<String> = instances
        .iter()
        .flat_map(|c| [format!("{c}.u_s"), format!("{c}.u_m")])
        .collect();
    nodes.push(format!(
        "{{\"@id\":\"urn:M\",\"@type\":\"S231:Block\",\"S231:containsBlock\":[{}],\
         \"S231:hasInput\":[{{\"@id\":\"urn:M.u\"}}]}}",
        refs(&instances)
    ));
    nodes.push(format!(
        "{{\"@id\":\"urn:M.u\",\"@type\":\"S231:RealInput\",\"S231:isOfDataType\":\
         {{\"@id\":\"S231:Real\"}},\"S231:isConnectedTo\":[{}]}}",
        refs(&inputs)
    ));
    let parameters: Vec<String> = PID_PARAMETERS
        .iter()
        .map(|(p, _)| format!("urn:p.{p}"))
        .collect();
    for c in &instances {
        nodes.push(format!(
            "{{\"@id\":\"{c}\",\"@type\":\"http://example.org#Buildings.Controls.OBC.CDL.Reals.PID\",\
             \"S231:hasParameter\":[{}],\"S231:hasInput\":[{{\"@id\":\"{c}.u_s\"}},{{\"@id\":\"{c}.u_m\"}}],\
             \"S231:hasOutput\":{{\"@id\":\"{c}.y\"}}}}",
            refs(&parameters)
        ));
        for (port, kind) in [("u_s", "Input"), ("u_m", "Input"), ("y", "Output")] {
            nodes.push(format!(
                "{{\"@id\":\"{c}.{port}\",\"@type\":\"S231:Real{kind}\",\"S231:isOfDataType\":\
                 {{\"@id\":\"S231:Real\"}}}}"
            ));
        }
    }
    for (p, value) in PID_PARAMETERS {
        nodes.push(format!(
            "{{\"@id\":\"urn:p.{p}\",\"@type\":\"S231:Parameter\",\"S231:value\":{value}}}"
        ));
    }
    let context = if prefixed {
        format!("{{\"S231\":\"http://data.ashrae.org/S231#\",\"x\":\"{stem}\"}}")
    } else {
        "{\"S231\":\"http://data.ashrae.org/S231#\"}".to_owned()
    };
    format!(
        "{{\"@context\":{context},\"@graph\":[{}]}}",
        nodes.join(",\n")
    )
    .into_bytes()
}

/// `count` `Constant` blocks, each named by its own IRI of `iri_bytes` bytes, with one parameter
/// and one output that has no IRI of its own (so its `@id` is minted, `<block>.out0`).
fn many_blocks(iri_bytes: usize, count: usize) -> ModelGraph {
    let mut graph = ModelGraph::new();
    for i in 0..count {
        let id = BlockId(i as u32);
        let cid = ConnectorId(i as u32);
        graph.blocks.push(BlockInstance {
            id,
            class_iri: Arc::from("CDL.Reals.Sources.Constant"),
            inputs: vec![],
            outputs: vec![cid],
            params: ParamTable {
                values: vec![(Arc::from("k"), Value::Real(1.5))],
            },
            decl_order: id.0,
            instance_iri: Some(Arc::from(format!(
                "urn:{i:06}{}",
                "b".repeat(iri_bytes - 10)
            ))),
        });
        graph
            .connectors
            .push(Connector::new(cid, id, Dir::Out, ValueType::Real, 0));
    }
    graph
}

/// One block named by an IRI of `iri_bytes` bytes with `count` inputs and `count` outputs, none
/// with an IRI of its own, each output wired to the input of its position.
fn many_minted_ports(iri_bytes: usize, count: usize) -> ModelGraph {
    let mut graph = many_outputs(iri_bytes, count);
    for c in &mut graph.connectors {
        c.iri = None;
    }
    for k in 0..count {
        let cid = ConnectorId((count + k) as u32);
        graph.connectors.push(Connector::new(
            cid,
            BlockId(0),
            Dir::In,
            ValueType::Real,
            k as u32,
        ));
        graph.blocks[0].inputs.push(cid);
        graph.connections.push(oce_model::Connection {
            from: ConnectorId(k as u32),
            to: cid,
        });
    }
    graph
}

/// Whether `peak` is within `bytes` times `1 + 1/divisor`, plus 64 KiB.
fn within(peak: u64, bytes: usize, divisor: u64) -> bool {
    peak <= bytes as u64 + bytes as u64 / divisor + 64 * 1024
}

#[test]
fn instances_sharing_short_parameter_nodes_write_their_iri_once_per_parameter_and_port() {
    // 4, 8 and 16 `PID` instances under 32 KiB IRIs list 11 shared parameter nodes and have both
    // inputs driven by the root's boundary input. The document spells each IRI 10 times; with
    // the IRI written once under a `@context` prefix it is 45 KB at 16 instances. The export
    // writes each instance's IRI 32 times: twice for each parameter (its `@id` in the block's
    // `S231:hasParameter` and on its node), three times for each minted input port
    // (`<IRI>.in<k>` in the block's `S231:hasInput`, on its node, as the boundary's target),
    // twice for the output port, twice for the block. No loader budget counts these copies
    // (issue #807). The bytes are linear in the instances and the peak is the bytes.
    // [measured: 4,198,372 / 8,396,388 / 16,792,420 bytes, 1,049,504 an instance, at peaks of
    // 4,202,420 / 8,403,876 / 16,806,788 (1.001); before, peaks of 12,302,932 / 24,604,424 /
    // 49,207,408 (2.93), and 1.407 with the document written one node at a time but the plan
    // holding a copy of each id]
    Engine::new().load_cxf(MODEL.as_bytes()).expect("loads");
    let iri = 32 * 1024;
    let mut sizes = Vec::new();
    for count in [4, 8, 16] {
        let mut engine = Engine::new();
        engine
            .load_cxf(&pid_instances(count, iri, true))
            .expect("the instances load");
        let (bytes, capacity, info) = exported(&engine);
        assert_eq!(capacity, bytes);
        assert!(
            within(info.bytes_max, bytes, 100),
            "{count}: {bytes} bytes, {info:?}"
        );
        sizes.push(bytes);
    }
    let per_instance = (sizes[2] - sizes[0]) / 12;
    assert_eq!(sizes[1] - sizes[0], 4 * per_instance, "{sizes:?}");
    assert!(
        (32 * iri..32 * iri + 1_024).contains(&per_instance),
        "{per_instance} bytes an instance"
    );
    // The same document with every IRI spelled out exports the same bytes.
    let mut engine = Engine::new();
    engine
        .load_cxf(&pid_instances(4, iri, false))
        .expect("the instances load");
    assert_eq!(exported(&engine).0, sizes[0]);
}

#[test]
fn many_blocks_export_at_about_the_bytes_they_write() {
    // 256, 512 and 1,024 blocks under their own 4 KiB IRIs, each with one parameter and one
    // output whose `@id` is minted: the plan borrows every block's IRI and holds each minted
    // port as the block's IRI and its position, so the peak is within 1/32 of the bytes.
    // [measured: 6,356,465 / 12,712,689 / 25,425,137 bytes at peaks of 6,457,841 / 12,915,441 /
    // 25,830,641 (1.016); before, peaks of 22,633,865 / 45,266,313 / 90,531,209 (3.56)]
    for count in [256, 512, 1_024] {
        let (result, info) = exported_graph(&many_blocks(4_096, count));
        let bytes = result.expect("the blocks export");
        assert!(
            within(info.bytes_max, bytes, 32),
            "{count}: {bytes} bytes, {info:?}"
        );
    }
}

#[test]
fn many_minted_ports_export_at_about_the_bytes_they_write() {
    // One block under a 4 KiB IRI with 256, 512 and 1,024 inputs and outputs, none with an IRI
    // of its own, each output wired to an input: every port's `@id` is minted, and each input's
    // is written three times (the block's list, its node, its driver's target). The peak is
    // within 1/16 of the bytes. [measured: 5,299,027 / 10,590,035 / 21,172,171 bytes at peaks of
    // 5,434,391 / 10,860,567 / 21,713,039 (1.026); before, peaks of 21,586,323 / 43,163,283 /
    // 86,317,275 (4.08)]
    for count in [256, 512, 1_024] {
        let (result, info) = exported_graph(&many_minted_ports(4_096, count));
        let bytes = result.expect("the block exports");
        assert!(bytes >= count * 5 * 4_096, "{count}: {bytes} bytes");
        assert!(
            within(info.bytes_max, bytes, 16),
            "{count}: {bytes} bytes, {info:?}"
        );
    }
}
