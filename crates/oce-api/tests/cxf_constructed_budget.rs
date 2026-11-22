//! The model-wide constructed budget through the full `Engine::load_cxf` pipeline (issue #700),
//! and why no valid document reaches it any more (issues #772, #773).
//!
//! Issue #700 reached `oce_graph::MAX_CONSTRUCTED_WORDS` (2^22 words) from a document of a few
//! thousand ports: a `CDL.Reals.MatrixGain` with `nin = nout = 1024` and no `K` held a dense
//! identity of 2^20 gains. Since issue #773 the counts are `K`'s own shape, so a dense matrix
//! writes its cells, and the document limits bound what cells cost: at most
//! `oce_api::MAX_CXF_BYTES` (8 MiB) of document and 2^20 array elements in all (that limit is
//! pinned at its boundary by `hostile_load_bounds.rs`,
//! `document_array_budget_admits_exactly_its_limit_and_refuses_one_element_more`). The largest
//! construction a valid document reaches is bounded here from those limits and two measured
//! costs, a dense `K`'s words per cell and a port's bytes per word: under half the budget. A
//! table sized by a lone corner cell is refused by the load gate (issue #772). The budget is
//! defence in depth for blobs and for hosts that build graphs directly: its refusal, the
//! receipt record and its place among the load's refusals are pinned on hand-built graphs in
//! `crates/oce-api/src/engine_constructed_budget_tests.rs`, and on blobs in
//! `blob_constructed_budget.rs`.
//!
//! Peaks are `allocation_counter`'s `bytes_max`, never wall-clock time.

use allocation_counter::{AllocationInfo, measure};
use oce_api::{DiagCode, Engine, MAX_CXF_BYTES, OcError};

/// `oce_graph::MAX_CONSTRUCTED_WORDS`, restated so this test fails if the published budget moves
/// without review.
const MAX_CONSTRUCTED_WORDS: u64 = 1 << 22;

const ROOT: &str = "http://example.org#M";

/// What a `MatrixGain` of `rows` by `cols` reports: its gains and its two port-kind buffers, each
/// rounded up to whole words.
fn gain_words(rows: u64, cols: u64) -> u64 {
    rows * cols + rows.div_ceil(8) + cols.div_ceil(8)
}

/// What a `MultiSum` of `nin` inputs reports: its gains and its input kinds.
fn sum_words(nin: u64) -> u64 {
    nin + nin.div_ceil(8)
}

fn real(id: &str, kind: &str, target: Option<&str>) -> String {
    let wire = target.map_or(String::new(), |to| {
        format!(r#","S231:isConnectedTo":{{"@id":"{to}"}}"#)
    });
    format!(
        r#"{{"@id":"{id}","@type":"S231:Real{kind}","S231:isOfDataType":{{"@id":"S231:Real"}}{wire}}}"#
    )
}

fn integer(id: &str, value: i64) -> String {
    format!(
        r#"{{"@id":"{id}","@type":"S231:Parameter","S231:isOfDataType":{{"@id":"S231:Integer"}},"S231:value":{value}}}"#
    )
}

fn refs(ids: impl Iterator<Item = String>) -> String {
    ids.map(|id| format!(r#"{{"@id":"{id}"}}"#))
        .collect::<Vec<_>>()
        .join(",")
}

/// One leaf block of the root: its nodes and the inputs the root's boundary inputs drive.
#[derive(Clone)]
struct Leaf {
    name: &'static str,
    nodes: Vec<String>,
    real_inputs: Vec<String>,
    boolean_inputs: Vec<String>,
}

/// One `MatrixGain` leaf `name` whose `K` is written whole, `rows x cols` zeros as one preserved
/// array (the cheapest spelling: two bytes a cell), every input driven by the root's boundary
/// input.
fn gain(name: &'static str, rows: u64, cols: u64) -> Leaf {
    let id = format!("{ROOT}.{name}");
    let zeros = vec!["0"; usize::try_from(rows * cols).unwrap()].join(",");
    let mut nodes = vec![format!(
        r#"{{"@id":"{id}","@type":"http://example.org#Buildings.Controls.OBC.CDL.Reals.MatrixGain","S231:hasParameter":[{{"@id":"{id}.K"}}],"S231:hasInput":[{}],"S231:hasOutput":[{}]}}"#,
        refs((1..=cols).map(|i| format!("{id}.u{i}"))),
        refs((1..=rows).map(|i| format!("{id}.y{i}"))),
    )];
    nodes.push(format!(
        r#"{{"@id":"{id}.K","@type":"S231:Parameter","S231:isArray":true,"S231:numberDimensions":2,"S231:sizeOfDimensions":"({rows}, {cols})","S231:value":[{zeros}]}}"#
    ));
    nodes.extend((1..=cols).map(|i| real(&format!("{id}.u{i}"), "Input", None)));
    nodes.extend((1..=rows).map(|i| real(&format!("{id}.y{i}"), "Output", None)));
    Leaf {
        name,
        nodes,
        real_inputs: (1..=cols).map(|i| format!("{id}.u{i}")).collect(),
        boolean_inputs: Vec::new(),
    }
}

/// One `MultiSum` leaf of `nin` inputs, each driven by the root's boundary input: the class whose
/// words a scalar sets, paid for by one port node per input.
fn sum(name: &'static str, nin: u64) -> Leaf {
    let id = format!("{ROOT}.{name}");
    let mut nodes = vec![format!(
        r#"{{"@id":"{id}","@type":"http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiSum","S231:hasParameter":{{"@id":"{id}.nin"}},"S231:hasInput":[{}],"S231:hasOutput":{{"@id":"{id}.y"}}}}"#,
        refs((1..=nin).map(|i| format!("{id}.u{i}"))),
    )];
    nodes.push(integer(&format!("{id}.nin"), i64::try_from(nin).unwrap()));
    nodes.extend((1..=nin).map(|i| real(&format!("{id}.u{i}"), "Input", None)));
    nodes.push(real(&format!("{id}.y"), "Output", None));
    Leaf {
        name,
        nodes,
        real_inputs: (1..=nin).map(|i| format!("{id}.u{i}")).collect(),
        boolean_inputs: Vec::new(),
    }
}

/// One `Reals.Sources.TimeTable` leaf whose only table cell is the flattened corner `cell`.
fn lone_corner_table(name: &'static str, cell: &str) -> Leaf {
    let id = format!("{ROOT}.{name}");
    Leaf {
        name,
        nodes: vec![
            format!(
                r#"{{"@id":"{id}","@type":"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.TimeTable","S231:hasParameter":{{"@id":"{id}.{cell}"}},"S231:hasOutput":{{"@id":"{id}.y1"}}}}"#
            ),
            format!(
                r#"{{"@id":"{id}.{cell}","@type":"S231:Parameter","S231:isOfDataType":{{"@id":"S231:Real"}},"S231:value":0.0}}"#
            ),
            real(&format!("{id}.y1"), "Output", None),
        ],
        real_inputs: Vec::new(),
        boolean_inputs: Vec::new(),
    }
}

/// A document whose root contains `leaves` in this order, with a Real and a Boolean boundary
/// input (each only if a leaf needs it) driving every leaf input not driven otherwise.
fn document(leaves: &[Leaf]) -> Vec<u8> {
    let driven = |inputs: &dyn Fn(&Leaf) -> &Vec<String>| -> Vec<String> {
        leaves
            .iter()
            .flat_map(|leaf| inputs(leaf).iter().cloned())
            .collect()
    };
    let boundary = |port: &str, kind: &str, targets: Vec<String>| {
        format!(
            r#"{{"@id":"{ROOT}.{port}","@type":"S231:{kind}Input","S231:isOfDataType":{{"@id":"S231:{kind}"}},"S231:isConnectedTo":[{}]}}"#,
            refs(targets.into_iter())
        )
    };
    let mut inputs = Vec::new();
    for (port, kind, targets) in [
        ("u", "Real", driven(&|leaf| &leaf.real_inputs)),
        ("t", "Boolean", driven(&|leaf| &leaf.boolean_inputs)),
    ] {
        if !targets.is_empty() {
            inputs.push((format!("{ROOT}.{port}"), boundary(port, kind, targets)));
        }
    }
    let mut nodes = vec![format!(
        r#"{{"@id":"{ROOT}","@type":"S231:Block","S231:containsBlock":[{}],"S231:hasInput":[{}]}}"#,
        refs(leaves.iter().map(|leaf| format!("{ROOT}.{}", leaf.name))),
        refs(inputs.iter().map(|(id, _)| id.clone())),
    )];
    nodes.extend(inputs.into_iter().map(|(_, node)| node));
    for leaf in leaves {
        nodes.extend(leaf.nodes.iter().cloned());
    }
    format!(
        r#"{{"@context":{{"S231":"http://data.ashrae.org/S231#"}},"@graph":[{}]}}"#,
        nodes.join(",\n")
    )
    .into_bytes()
}

fn load(bytes: &[u8]) -> (Engine, Result<oce_api::LoadReport, OcError>, AllocationInfo) {
    let mut engine = Engine::new();
    let mut result = None;
    let info = measure(|| result = Some(engine.load_cxf(bytes)));
    (engine, result.unwrap().map(|(report, _)| report), info)
}

/// The document limit on array elements (`hostile_load_bounds.rs` pins it), restated.
const MAX_DOCUMENT_ARRAY_ELEMENTS: u64 = 1 << 20;

#[test]
fn the_largest_construction_a_valid_document_reaches_is_far_below_the_budget() {
    // A dense `K` written whole loads, holding one word per cell and its port kinds.
    let dense = document(&[gain("g0", 256, 256)]);
    let (_, loaded, info) = load(&dense);
    assert_eq!(loaded.expect("a 256 x 256 K loads").block_count, 1);
    assert!(info.bytes_max < 1 << 28, "load peak {}", info.bytes_max);
    // The most cells a document holds are the array limit's; the shape that holds the most words
    // for them is one row (the most port kinds): 1,179,649 words.
    let cells_bound = gain_words(1, MAX_DOCUMENT_ARRAY_ELEMENTS)
        .max(gain_words(MAX_DOCUMENT_ARRAY_ELEMENTS, 1))
        .max(gain_words(1024, 1024));
    assert_eq!(cells_bound, 1_179_649);

    // What the document's bytes buy as ports: a `MultiSum` costs one port node and two
    // references per input, well over 100 bytes a word, so 8 MiB buys tens of thousands of words.
    let with_sum = document(&[gain("g0", 256, 256), sum("s", 16_384)]);
    let per_input = (with_sum.len() - dense.len()) as u64 / 16_384;
    assert!(per_input >= 100, "{per_input} bytes per input");
    let (_, loaded, _) = load(&with_sum);
    assert_eq!(loaded.expect("the gain and the sum load").block_count, 2);
    let ports_bound = sum_words(MAX_CXF_BYTES as u64 / per_input + 1);
    let bound = cells_bound + ports_bound;
    assert!(
        2 * bound < MAX_CONSTRUCTED_WORDS,
        "a maximal valid document holds at most {bound} words, under half the budget"
    );
}

#[test]
fn tables_sized_by_lone_corner_cells_are_refused_by_the_load_gate_before_any_is_built() {
    // Issue #700's shape in a document: four tables of 524,287 rows named by one corner cell each
    // would hold exactly the budget. Their cells do not fill their rectangles, so the load gate
    // refuses them (issue #772's shape rules), and no table is built.
    let leaves: Vec<Leaf> = ["t0", "t1", "t2", "t3"]
        .into_iter()
        .map(|name| lone_corner_table(name, "table_524287_2"))
        .collect();
    let bytes = document(&leaves);
    let (_, refused, info) = load(&bytes);
    let error = refused.expect_err("lone corner cells are refused");
    assert!(
        error
            .all_diagnostics()
            .any(|d| d.code == DiagCode::ParameterOutOfRange),
        "{error:?}"
    );
    assert!(!matches!(error, OcError::Build(_)), "{error:?}");
    // The reader sizes one block at a time to read its ports (one construction's peak, freed
    // before the next); the four tables are never held together, let alone instantiated.
    assert!(
        info.bytes_max < oce_blocks::MAX_CONSTRUCTION_PEAK_BYTES as u64 + (1 << 20),
        "at most one table is sized at a time: peak {} for {} bytes",
        info.bytes_max,
        bytes.len()
    );
    assert!(
        info.bytes_max < 2 * 8 * 1_048_576,
        "peak {}",
        info.bytes_max
    );
}
