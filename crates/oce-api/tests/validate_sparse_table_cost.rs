//! What a sparse flattened table costs a load, in requested bytes and diagnostics and never in
//! elapsed time (issue #726; `TESTING.md`: cost is asserted with deterministic measures).
//!
//! A `CDL.Reals.Sources.TimeTable` takes its shape from the largest `table_<row>_<col>` it
//! carries, so one parameter `table_524288_2` declares 1,048,576 cells and binds one. The
//! validator raised a `parameter-out-of-range` diagnostic for every absent cell and walked the
//! whole shape to find them: measured on the base, 3 such blocks (1,647 bytes of CXF here) were
//! 3,145,725 diagnostics and a 579 MB peak, and the issue measured 50 blocks (25,977 bytes) at
//! 52 million diagnostics, 9.5 GB and 198 s. The validator now raises one finding for a table's absent cells, with their count
//! and the first four in row order, and finds them from the cells the block binds.
//!
//! Since issue #722 the resolver and the validator charge the constructions they make to read
//! each block's signature to the constructed budget, so the issue's own documents of four or more
//! of those tables (each 1,048,578 words, a quarter of the budget and a row more) are refused by
//! the resolver before the validator runs (`cxf_constructed_budget.rs` pins that). The
//! validator's claims are therefore held on the issue's 3-block document and on tables of 40,000
//! rows (79,999 absent cells each), whose 3, 4 and 50 blocks stay inside the budget:
//!
//! - **the refusal**: typed, one `parameter-out-of-range` error per block, each naming the block,
//!   the count of absent cells and the first four;
//! - **a peak** that does not grow with the block count: what one block's constructor probe
//!   holds plus a multiple of the document;
//! - **allocations** that grow by a fixed number per block, never by the declared shape;
//! - **requested bytes** that grow by the two constructor probes a block and a little more.
//!
//! What this cannot see: work that allocates nothing. The constructor probes of the resolve and
//! validation passes each still zero-fill a table per block and drop it, so requested bytes in
//! total grow by two tables a block, up to the constructed budget a pass (issue #722); the
//! ceiling here is that and no more.

use allocation_counter::{AllocationInfo, measure};
use oce_api::{Engine, OcError};
use oce_diag::{DiagCode, Diagnostic, Severity};

const ROOT: &str = "http://example.org#M";

/// The block counts the issue measured: 1,673, 2,185 and 25,977 bytes of CXF.
const BLOCKS: [usize; 3] = [3, 4, 50];

/// The issue's tables: 524,288 rows of two columns.
const ISSUE_ROWS: usize = 524_288;

/// Tables of 40,000 rows: 80,002 words each, so 50 of them stay inside the constructed budget.
const ROWS: usize = 40_000;

/// The bytes one block's constructor probe holds for a `rows` x 2 table: its cells as `f64`.
fn probe_bytes(rows: usize) -> u64 {
    rows as u64 * 2 * 8
}

/// What one block's finding says for a table of `rows` x 2 that binds only its last cell.
fn absent_cells_message(rows: usize) -> String {
    let absent = rows * 2 - 1;
    format!(
        "table `table` on block `CDL.Reals.Sources.TimeTable` is missing {absent} required cells \
         of its {rows}x2 shape: table_1_1, table_1_2, table_2_1, table_2_2 and {} more",
        absent - 4
    )
}

/// A root composite of `blocks` `CDL.Reals.Sources.TimeTable` leaves, each with one parameter
/// `table_<rows>_2` and one Real output: the issue's document when `rows` is [`ISSUE_ROWS`].
fn sparse_tables(blocks: usize, rows: usize) -> Vec<u8> {
    let cell = format!("table_{rows}_2");
    let leaf = |index: usize| {
        let id = format!("{ROOT}.t{index}");
        [
            format!(
                r#"{{"@id":"{id}","@type":"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.TimeTable","S231:hasParameter":{{"@id":"{id}.{cell}"}},"S231:hasOutput":{{"@id":"{id}.y"}}}}"#
            ),
            format!(
                r#"{{"@id":"{id}.{cell}","@type":"S231:Parameter","S231:isOfDataType":{{"@id":"S231:Real"}},"S231:value":1.0}}"#
            ),
            format!(
                r#"{{"@id":"{id}.y","@type":"S231:RealOutput","S231:isOfDataType":{{"@id":"S231:Real"}}}}"#
            ),
        ]
    };
    let children = (0..blocks)
        .map(|index| format!(r#"{{"@id":"{ROOT}.t{index}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let mut nodes = vec![format!(
        r#"{{"@id":"{ROOT}","@type":"S231:Block","S231:containsBlock":[{children}]}}"#
    )];
    nodes.extend((0..blocks).flat_map(leaf));
    format!(
        r#"{{"@context":{{"S231":"http://data.ashrae.org/S231#"}},"@graph":[{}]}}"#,
        nodes.join(",")
    )
    .into_bytes()
}

/// Load `bytes`, measured, and return the refusal's diagnostics.
fn refused(bytes: &[u8]) -> (Vec<Diagnostic>, AllocationInfo) {
    let mut engine = Engine::new();
    let mut result = None;
    let info = measure(|| result = Some(engine.load_cxf(bytes)));
    let error = result
        .expect("measured")
        .expect_err("a table with absent cells is refused");
    assert!(
        matches!(error, OcError::Validate(_) | OcError::LoadContext(_)),
        "refused by the validator: {error:?}"
    );
    (error.all_diagnostics().cloned().collect(), info)
}

#[test]
fn the_issue_documents_are_the_sizes_it_measured() {
    let sizes: Vec<usize> = BLOCKS
        .iter()
        .map(|&n| sparse_tables(n, ISSUE_ROWS).len())
        .collect();
    // Within a few bytes of the issue's documents (its node order and spacing are not known);
    // what matters is that a block costs about 500 bytes of CXF.
    for (size, issue) in sizes.iter().zip([1_673, 2_185, 25_977]) {
        assert!(
            size.abs_diff(issue) * 10 < issue,
            "{size} bytes, the issue's {issue}"
        );
    }
}

#[test]
fn a_sparse_table_is_refused_with_one_finding_per_block_naming_the_count_and_first_cells() {
    // The issue's 3-block document, and 3, 4 and 50 blocks of the smaller shape.
    let cases = [(3, ISSUE_ROWS)]
        .into_iter()
        .chain(BLOCKS.iter().map(|&blocks| (blocks, ROWS)));
    for (blocks, rows) in cases {
        let (diagnostics, _) = refused(&sparse_tables(blocks, rows));
        let findings: Vec<&Diagnostic> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert_eq!(findings.len(), blocks, "{blocks} blocks: {diagnostics:#?}");
        for finding in &findings {
            assert_eq!(finding.code, DiagCode::ParameterOutOfRange);
            assert_eq!(finding.message, absent_cells_message(rows));
        }
        // Located on each block, one finding a block, in the validator's order: by subject.
        let mut blocks_named: Vec<String> = (0..blocks).map(|i| format!("{ROOT}.t{i}")).collect();
        blocks_named.sort();
        let subjects: Vec<&str> = findings
            .iter()
            .map(|d| d.subject.as_deref().expect("a located finding"))
            .collect();
        assert_eq!(subjects, blocks_named);
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.code == DiagCode::ResourceLimitExceeded),
            "one finding per block is far inside the list's budget, and the tables inside the \
             constructed budget"
        );
    }
}

#[test]
fn the_peak_is_flat_and_allocations_and_bytes_grow_by_a_fixed_amount_a_block() {
    let measured: Vec<(usize, u64, AllocationInfo)> = BLOCKS
        .iter()
        .map(|&blocks| {
            let bytes = sparse_tables(blocks, ROWS);
            let (_, info) = refused(&bytes);
            (blocks, bytes.len() as u64, info)
        })
        .collect();
    for &(blocks, document, info) in &measured {
        // Measured: 0.73 MB at 3 blocks, 0.70 MB at 4 and 1.40 MB at 50 [debug, macOS arm64],
        // one probe's 0.64 MB of it. Before issue #726, the issue's 3 blocks peaked at 578.9 MB
        // and 4 at 704.6 MB.
        let ceiling = probe_bytes(ROWS) + 64 * document + 1024 * 1024;
        assert!(
            info.bytes_max <= ceiling,
            "{blocks} blocks: peak {} over {ceiling}",
            info.bytes_max
        );
    }
    let (few, _, few_info) = measured[0];
    let (many, _, many_info) = measured[2];
    let per_block = (many_info.count_total - few_info.count_total) / (many - few) as u64;
    // Measured: 134 allocations a block [debug, macOS arm64]. Before issue #726, the issue's 3
    // blocks made 3,147,126 allocations and 4 blocks 4,195,036: over a million a block.
    assert!(
        per_block <= 400,
        "{per_block} allocations a block, between {few} and {many} blocks"
    );
    let bytes_per_block = (many_info.bytes_total - few_info.bytes_total) / (many - few) as u64;
    // Measured: 1,308,861 bytes a block, the two constructor probes' 2 x 640,000 and 28,861
    // more [debug, macOS arm64]. Nothing of the validator's is sized by the shape: a vector of
    // the table's 40,000 times is 320,000 bytes a block more.
    assert!(
        bytes_per_block <= 2 * probe_bytes(ROWS) + 256 * 1024,
        "{bytes_per_block} requested bytes a block, between {few} and {many} blocks"
    );
}
