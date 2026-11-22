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
//! The budget also bounds the work a refused document costs (issue #722). The resolver and the
//! validator each construct every block to read its signature and drop it; each charges those
//! constructions to a budget of its own, so a document of tables sized by lone corner cells is
//! refused at the import stage once its probes cross the budget, and what the load requests no
//! longer grows by two tables a block. The last tests here pin that refusal, its stage, the edge
//! on both sides and the cost at 1x, 2x and 4x the block count.
//!
//! Peaks are `allocation_counter`'s `bytes_max` and work its `bytes_total`, never wall-clock time.

use allocation_counter::{AllocationInfo, measure};
use oce_api::{DiagCode, DiagnosticStage, Engine, MAX_CXF_BYTES, OcError};

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
    name: String,
    nodes: Vec<String>,
    real_inputs: Vec<String>,
    boolean_inputs: Vec<String>,
}

/// One `MatrixGain` leaf `name` whose `K` is written whole, `rows x cols` zeros as one preserved
/// array (the cheapest spelling: two bytes a cell), every input driven by the root's boundary
/// input.
fn gain(name: &str, rows: u64, cols: u64) -> Leaf {
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
        name: name.to_owned(),
        nodes,
        real_inputs: (1..=cols).map(|i| format!("{id}.u{i}")).collect(),
        boolean_inputs: Vec::new(),
    }
}

/// One `MultiSum` leaf of `nin` inputs, each driven by the root's boundary input: the class whose
/// words a scalar sets, paid for by one port node per input.
fn sum(name: &str, nin: u64) -> Leaf {
    let id = format!("{ROOT}.{name}");
    let mut nodes = vec![format!(
        r#"{{"@id":"{id}","@type":"http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiSum","S231:hasParameter":{{"@id":"{id}.nin"}},"S231:hasInput":[{}],"S231:hasOutput":{{"@id":"{id}.y"}}}}"#,
        refs((1..=nin).map(|i| format!("{id}.u{i}"))),
    )];
    nodes.push(integer(&format!("{id}.nin"), i64::try_from(nin).unwrap()));
    nodes.extend((1..=nin).map(|i| real(&format!("{id}.u{i}"), "Input", None)));
    nodes.push(real(&format!("{id}.y"), "Output", None));
    Leaf {
        name: name.to_owned(),
        nodes,
        real_inputs: (1..=nin).map(|i| format!("{id}.u{i}")).collect(),
        boolean_inputs: Vec::new(),
    }
}

/// One `Reals.Sources.TimeTable` leaf whose only table cell is the flattened corner `cell`.
fn lone_corner_table(name: &str, cell: &str) -> Leaf {
    let id = format!("{ROOT}.{name}");
    Leaf {
        name: name.to_owned(),
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

/// The bytes one probe of a 524,288 x 2 table holds: its cells as `f64`.
const PROBE_BYTES: u64 = 524_288 * 2 * 8;

/// A document of `blocks` tables, each sized by the one corner cell `cell`.
fn lone_corner_tables(blocks: usize, cell: &str) -> Vec<u8> {
    let leaves: Vec<Leaf> = (0..blocks)
        .map(|index| lone_corner_table(&format!("t{index}"), cell))
        .collect();
    document(&leaves)
}

/// The budget refusal of a load: its subject and message, and no other error beside it.
fn the_budget_refusal(error: &OcError) -> (String, String) {
    let errors: Vec<_> = error.all_diagnostics().filter(|d| d.is_error()).collect();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert_eq!(errors[0].code, DiagCode::ResourceLimitExceeded);
    (
        errors[0].subject.as_deref().unwrap_or_default().to_owned(),
        errors[0].message.clone(),
    )
}

fn crossing_message(requested: u64) -> String {
    format!(
        "constructed blocks too large: {requested} word(s) held by the blocks through this block \
         exceed the supported maximum (4194304 words). Remedy: reduce table, matrix or vector \
         sizes, or split the model."
    )
}

#[test]
fn probes_past_the_budget_are_refused_at_import_with_work_flat_in_the_block_count() {
    // The issue's shape: tables of 524,288 rows, one corner cell each, 8 MiB a probe. Each
    // reports 1,048,578 words, so the fourth crosses the budget in the resolver, which builds no
    // later table; the validator and the facade never run. Measured on the base (84db2d76,
    // debug, macOS arm64): 16 blocks requested 268,891,973 bytes, 32 537,780,453 and 64
    // 1,075,557,237, two probes (16.8 MB) a block.
    let mut measured = Vec::new();
    for blocks in [16, 32, 64] {
        let bytes = lone_corner_tables(blocks, "table_524288_2");
        let (_, refused, info) = load(&bytes);
        let error = refused.expect_err("past the constructed budget");
        assert!(
            matches!(error, OcError::Cxf(_) | OcError::LoadContext(_)),
            "refused by the resolver: {error:?}"
        );
        assert_eq!(
            the_budget_refusal(&error),
            (format!("{ROOT}.t3"), crossing_message(4 * ((1 << 20) + 2))),
            "{blocks} blocks"
        );
        // Four probes, each dropped before the next: at most one table held at a time.
        assert!(
            info.bytes_max < PROBE_BYTES + 64 * bytes.len() as u64 + (1 << 20),
            "{blocks} blocks: peak {}",
            info.bytes_max
        );
        assert!(
            info.bytes_total < 5 * PROBE_BYTES + 256 * bytes.len() as u64,
            "{blocks} blocks: {} bytes requested",
            info.bytes_total
        );
        measured.push((blocks, bytes.len() as u64, info));
    }
    // Flat in the block count past the budget: from 16 to 64 blocks the load requests less than
    // one more probe, all of it the document's own linear cost.
    let (_, small, at_16) = measured[0];
    let (_, large, at_64) = measured[2];
    let growth = at_64.bytes_total - at_16.bytes_total;
    assert!(
        growth < PROBE_BYTES && growth < 256 * (large - small),
        "{growth} more bytes requested from 16 to 64 blocks"
    );
}

#[test]
fn the_receipt_records_the_probe_refusal_at_the_import_stage() {
    let bytes = lone_corner_tables(5, "table_524287_2");
    let failure = Engine::new()
        .load_cxf_with_receipt(&bytes)
        .expect_err("past the budget");
    assert_eq!(failure.stage(), DiagnosticStage::Import);
    let rows: Vec<_> = failure
        .diagnostics()
        .records()
        .iter()
        .map(|record| (record.key().stage(), record.key().code().to_owned()))
        .collect();
    assert_eq!(
        rows,
        [(
            DiagnosticStage::Import,
            "resource-limit-exceeded".to_owned()
        )]
    );
    for _ in 0..2 {
        let again = Engine::new().load_cxf(&bytes).expect_err("past the budget");
        assert_eq!(
            the_budget_refusal(&again),
            (format!("{ROOT}.t4"), crossing_message(5 * (1 << 20)))
        );
    }
}

#[test]
fn exactly_the_budget_passes_both_probes_and_one_word_more_is_refused_by_the_first() {
    // Four quarter-budget tables are exactly the budget: neither probe pass refuses, and the
    // validator's shape rule refuses each unfilled table, as before issue #722.
    let at_budget = lone_corner_tables(4, "table_524287_2");
    let (_, refused, info) = load(&at_budget);
    let error = refused.expect_err("unfilled tables are refused by the load gate");
    let codes: Vec<_> = error.all_diagnostics().map(|d| d.code).collect();
    assert_eq!(codes, [DiagCode::ParameterOutOfRange; 4], "{error:?}");
    assert!(matches!(
        error,
        OcError::Validate(_) | OcError::LoadContext(_)
    ));
    // Two passes of four probes, one table held at a time.
    assert!(info.bytes_total < 9 * PROBE_BYTES, "{}", info.bytes_total);
    assert!(
        info.bytes_max < PROBE_BYTES + (1 << 20),
        "{}",
        info.bytes_max
    );

    // One four-word table more, first or last: the resolver refuses the fifth, alone.
    for leaves in [
        [
            "table_524287_2",
            "table_524287_2",
            "table_524287_2",
            "table_524287_2",
            "table_1_2",
        ],
        [
            "table_1_2",
            "table_524287_2",
            "table_524287_2",
            "table_524287_2",
            "table_524287_2",
        ],
    ] {
        let named: Vec<Leaf> = ["t0", "t1", "t2", "t3", "t4"]
            .iter()
            .zip(leaves)
            .map(|(name, cell)| lone_corner_table(name, cell))
            .collect();
        let (_, refused, _) = load(&document(&named));
        let error = refused.expect_err("one word past the budget");
        assert_eq!(
            the_budget_refusal(&error),
            (format!("{ROOT}.t4"), crossing_message((1 << 22) + 4)),
            "{leaves:?}"
        );
    }
}

/// One `MultiSum` leaf `name` of `nin` inputs, gains left to their default, that declares one
/// output and no input: an interface the class does not have, whose probe still builds `nin`
/// gains and input kinds before the arity is compared.
fn mismatched_sum(name: &str, nin: u64) -> Leaf {
    let id = format!("{ROOT}.{name}");
    Leaf {
        name: name.to_owned(),
        nodes: vec![
            format!(
                r#"{{"@id":"{id}","@type":"http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiSum","S231:hasParameter":{{"@id":"{id}.nin"}},"S231:hasOutput":{{"@id":"{id}.y"}}}}"#
            ),
            integer(&format!("{id}.nin"), i64::try_from(nin).unwrap()),
            real(&format!("{id}.y"), "Output", None),
        ],
        real_inputs: Vec::new(),
        boolean_inputs: Vec::new(),
    }
}

#[test]
fn probes_of_blocks_whose_interface_does_not_match_are_charged_too() {
    // The review's shape (GitHub #788): a `MultiSum` of 2^20 inputs declaring none, about 420
    // bytes of CXF a block, 9.4 MB a probe. Every probe is charged whether or not the block's
    // declared ports match its class, so the fourth crosses the budget (4 x 1,179,648 words) and
    // no later block is built; three interface mismatches are reported before it. Measured by
    // the review on the base: 151 MB requested at 16 blocks and 606 MB at 64.
    const NIN: u64 = 1 << 20;
    let probe = sum_words(NIN) * 8;
    let mut measured = Vec::new();
    for blocks in [16_usize, 64] {
        let leaves: Vec<Leaf> = (0..blocks)
            .map(|index| mismatched_sum(&format!("s{index}"), NIN))
            .collect();
        let bytes = document(&leaves);
        let (_, refused, info) = load(&bytes);
        let error = refused.expect_err("mismatched and past the budget");
        let limits: Vec<_> = error
            .all_diagnostics()
            .filter(|d| d.code == DiagCode::ResourceLimitExceeded)
            .map(|d| (d.subject.as_deref().unwrap_or_default(), d.message.clone()))
            .collect();
        assert_eq!(
            limits,
            [(
                format!("{ROOT}.s3").as_str(),
                crossing_message(4 * sum_words(NIN))
            )],
            "{blocks} blocks"
        );
        let mismatches = error
            .all_diagnostics()
            .filter(|d| d.message.starts_with("block interface mismatch"))
            .count();
        assert_eq!(mismatches, 3, "{blocks} blocks: {error:?}");
        assert!(
            info.bytes_total < 5 * probe + 256 * bytes.len() as u64,
            "{blocks} blocks: {} bytes requested",
            info.bytes_total
        );
        measured.push(info.bytes_total);
    }
    let growth = measured[1] - measured[0];
    assert!(
        growth < probe,
        "{growth} more bytes requested from 16 to 64 blocks, a probe is {probe}"
    );
}
