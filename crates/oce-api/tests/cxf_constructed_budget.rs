//! The model-wide constructed budget through the full `Engine::load_cxf` pipeline (issue #700),
//! and why no valid document reaches it any more (issues #772, #773).
//!
//! Issue #700 reached `oce_graph::MAX_CONSTRUCTED_WORDS` (2^22 words) from a document of a few
//! thousand ports: a `CDL.Reals.MatrixGain` with `nin = nout = 1024` and no `K` held a dense
//! identity of 2^20 gains. Since issue #773 the counts are `K`'s own shape, so a dense matrix
//! writes its cells, and the document limits bound what cells cost: at most
//! `oce_api::MAX_CXF_BYTES` (8 MiB) of document and 2^20 array elements in all. The largest
//! construction a valid document reaches is measured here: one dense `K` of 2^20 cells (a
//! quarter of the budget) and what the rest of the bytes buy as ports, under half the budget; the cells past the element limit are refused before any block is built, and a
//! table sized by a lone corner cell is refused by the load gate (issue #772). The budget is
//! defence in depth for blobs and for hosts that build graphs directly: its refusal, the
//! receipt record and its place among the load's refusals are pinned on hand-built graphs in
//! `crates/oce-api/src/tests/load_constructed_budget_tests.rs`, and on blobs in
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

#[test]
fn the_largest_construction_a_valid_document_reaches_is_far_below_the_budget() {
    // The densest cells a document can write: one `K` of 2^20 elements, the whole array budget.
    let one = document(&[gain("g0", 1024, 1024)]);
    assert!(one.len() < MAX_CXF_BYTES, "{} bytes", one.len());
    let (_, loaded, info) = load(&one);
    assert_eq!(loaded.expect("a 1024 x 1024 K loads").block_count, 1);
    // The reader's cost of 2^20 elements is the array budget's (issue #710), under 1 GiB.
    assert!(info.bytes_max < 1 << 30, "load peak {}", info.bytes_max);
    let densest = gain_words(1024, 1024);
    assert_eq!(densest, 1_048_832);

    // What the document's remaining bytes buy as ports: a `MultiSum` costs one port node and
    // two references per input, about 180 bytes a word, so the rest of 8 MiB buys tens of
    // thousands of words, not millions.
    let with_sum = document(&[gain("g0", 1024, 1024), sum("s", 16_384)]);
    let per_input = (with_sum.len() - one.len()) as u64 / 16_384;
    assert!(per_input >= 100, "{per_input} bytes per input");
    assert!(with_sum.len() < MAX_CXF_BYTES, "{} bytes", with_sum.len());
    let (_, loaded, _) = load(&with_sum);
    assert_eq!(loaded.expect("the gain and the sum load").block_count, 2);
    let reached = densest + sum_words(16_384);
    let room = (MAX_CXF_BYTES - with_sum.len()) as u64;
    let bound = reached + sum_words(room / per_input + 1);
    assert!(
        4 * bound < 2 * MAX_CONSTRUCTED_WORDS,
        "a maximal valid document holds at most {bound} words, under half the budget"
    );

    // A second dense `K` is past the document's array elements and refused before any block is
    // built, holding about what the document text costs.
    let two = document(&[gain("g0", 1024, 1024), gain("g1", 1024, 1024)]);
    assert!(two.len() < MAX_CXF_BYTES, "{} bytes", two.len());
    let (_, refused, info) = load(&two);
    let error = refused.expect_err("2^21 array elements are refused");
    assert!(
        error
            .all_diagnostics()
            .any(|d| d.code == DiagCode::ResourceLimitExceeded),
        "{error:?}"
    );
    assert!(!matches!(error, OcError::Build(_)), "{error:?}");
    // The reader's own cost of expanding 2^20 elements is the array budget's (issue #710), not
    // the constructed one's; it is reported, not bounded, here.
    assert!(info.bytes_max < 1 << 30, "reader peak {}", info.bytes_max);
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
