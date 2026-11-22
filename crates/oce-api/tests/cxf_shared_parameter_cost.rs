//! What a parameter node many leaves list costs a load, in requested bytes and never in elapsed
//! time (issue #749; `TESTING.md`: cost is asserted with deterministic measures).
//!
//! Each leaf that listed a parameter node grounded its value again: lexing its text and building
//! its result anew. So a long string value cost `leaves × text` of lexing and allocation, while
//! each further leaf cost the document a few hundred bytes. The document's string budget (issue
//! #736) bounds what the tables hold, not that work: every listing was grounded before the budget
//! was charged, and 1,024 leaves listing one 60 KB value, a 702 KB document, requested 1.31 GB.
//! A value whose grounding looks up no name is now grounded once for the document and shared by
//! every leaf; each listing is still charged to the string budget, so the refusals stand.
//!
//! The added parameter `q` is a name the constant's class does not declare, which the facade's
//! load gate refuses (issue #748), so the reader (`oce_cxf::import_cxf`, the facade's first step)
//! is measured, and the facade is held to the reader's refusal.
//!
//! Measured with `allocation-counter`, whose counters are local to the measuring thread. The
//! ceilings have headroom over what was measured (in brackets, with what the same document cost
//! before the change); the headroom is far under the cost of grounding the value once more.

use std::sync::Arc;

use allocation_counter::{AllocationInfo, measure};
use oce_api::Engine;
use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::DiagCode;
use oce_model::{ModelGraph, Value};

const MODEL: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const GT: &str = "{ \"@id\": \"http://example.org#MinLoop.gt\" }\n      ],";
const LAST: &str = "\"S231:isOfDataType\": { \"@id\": \"S231:Boolean\" } }\n  ]";

/// The string budget (`oce-cxf`'s `MAX_DOCUMENT_STRING_PARAMETER_BYTES`), restated.
const STRING_BUDGET: usize = 8 * 1024 * 1024;

/// The issue's value: a string of 60,000 bytes.
const TEXT_BYTES: usize = 60_000;

/// [`MODEL`] with `leaves` more `Constant` leaves, each listing its own `k` and the one node
/// `urn:shared.q`, whose value is a string literal of `text` bytes.
fn shared(leaves: usize, text: usize) -> Vec<u8> {
    assert_eq!(MODEL.matches(GT).count(), 1);
    assert_eq!(MODEL.matches(LAST).count(), 1);
    let mut contains = String::new();
    let mut nodes = String::new();
    for i in 0..leaves {
        contains.push_str(&format!(", {{ \"@id\": \"urn:c{i}\" }}"));
        nodes.push_str(&format!(
            ",\n    {{ \"@id\": \"urn:c{i}\", \"@type\": \
             \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant\", \
             \"S231:hasParameter\": [ {{ \"@id\": \"urn:c{i}.k\" }}, \
             {{ \"@id\": \"urn:shared.q\" }} ], \"S231:hasOutput\": {{ \"@id\": \"urn:c{i}.y\" }} }},\n    \
             {{ \"@id\": \"urn:c{i}.k\", \"S231:value\": 1.0 }},\n    \
             {{ \"@id\": \"urn:c{i}.y\", \"@type\": \"S231:RealOutput\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }} }}"
        ));
    }
    let value = format!("\"\\\"{}\\\"\"", "y".repeat(text));
    nodes.push_str(&format!(
        ",\n    {{ \"@id\": \"urn:shared.q\", \"S231:value\": {value} }}"
    ));
    MODEL
        .replacen(
            GT,
            &format!("{{ \"@id\": \"http://example.org#MinLoop.gt\" }}{contains}\n      ],"),
            1,
        )
        .replacen(
            LAST,
            &format!("\"S231:isOfDataType\": {{ \"@id\": \"S231:Boolean\" }} }}{nodes}\n  ]"),
            1,
        )
        .into_bytes()
}

/// One read of `bytes` by the CXF reader alone, measured. The unedited model is read once first.
fn read(bytes: &[u8]) -> (Result<ModelGraph, CxfError>, AllocationInfo) {
    import_cxf(MODEL.as_bytes(), &ResolveOptions::default()).expect("the model reads");
    let mut result = None;
    let info = measure(|| {
        result = Some(import_cxf(bytes, &ResolveOptions::default()).map(|(model, _)| model));
    });
    (result.expect("the read ran"), info)
}

/// `count` bytes as a `u64`.
fn bytes(count: usize) -> u64 {
    count as u64
}

#[test]
fn leaves_listing_one_long_string_ground_it_once() {
    // The issue's shape at 256, 512 and 1,024 leaves, against the same documents with a value of
    // one letter. The string budget holds 139 listings of the 60 KB value and refuses each later
    // one by name; the value is grounded once, so the long value costs the read a fixed number of
    // copies of it (reading the document and grounding it once) and the refusals, whatever the
    // number of leaves. [measured: 2,311,789 / 2,518,389 / 2,906,957 bytes requested more than
    // with the one-letter value, peaks 430,540 / 316,108 / 390,074 more; 26.5 KB a further leaf
    // either way. Before: 322,197,867 bytes at 256 leaves, 1.26 MB a leaf, and 1.31 GB at 1,024
    // (issue)]
    let mut totals = Vec::new();
    for leaves in [256, 512, 1_024] {
        let document = shared(leaves, TEXT_BYTES);
        let (result, info) = read(&document);
        let Err(CxfError::Validation(diagnostics)) = result else {
            panic!("the string budget refuses the later listings")
        };
        let refusals = leaves - STRING_BUDGET / TEXT_BYTES;
        let refused = diagnostics
            .iter()
            .filter(|d| d.code == DiagCode::ResourceLimitExceeded)
            .count();
        assert_eq!(refused, refusals);
        let (short, baseline) = read(&shared(leaves, 1));
        short.expect("a one-letter value fits");
        assert!(
            info.bytes_total <= baseline.bytes_total + bytes(48 * TEXT_BYTES + 1_024 * refusals),
            "{leaves} leaves: {} bytes requested, {} with a one-letter value",
            info.bytes_total,
            baseline.bytes_total
        );
        assert!(
            info.bytes_max <= baseline.bytes_max + bytes(16 * TEXT_BYTES + 1_024 * refusals),
            "{leaves} leaves: a peak of {}, {} with a one-letter value",
            info.bytes_max,
            baseline.bytes_max
        );
        totals.push(info.bytes_total);
    }
    // Linear in the leaves at what a leaf's own nodes cost the read, never a grounding a leaf.
    for (pair, added) in totals.windows(2).zip([256_u64, 512]) {
        let per_leaf = (pair[1] - pair[0]) / added;
        assert!(
            per_leaf <= 48 * 1024,
            "{per_leaf} bytes a further leaf: {totals:?}"
        );
    }
    // And the facade refuses the document on the reader's refusal.
    let error = Engine::new()
        .load_cxf(&shared(256, TEXT_BYTES))
        .expect_err("refused");
    assert!(
        error
            .all_diagnostics()
            .any(|d| d.code == DiagCode::ResourceLimitExceeded
                && d.message.starts_with("string parameter value"))
    );
}

#[test]
fn leaves_within_the_string_budget_share_one_allocation_of_the_value() {
    // 128 leaves of the 60 KB value fit the budget (7.7 MB of the 8 MiB counted): the reader
    // reads it, every table holds the one allocation, and what it requested is the reading of
    // its 128 leaves and a fixed number of copies of the value. [measured: a total of 5,646,623
    // bytes for a 116,122-byte document, a peak of 2,328,022; a grounding a leaf was 1.26 MB]
    let document = shared(128, TEXT_BYTES);
    let (result, info) = read(&document);
    let model = result.expect("128 listings fit the string budget");
    let values: Vec<&Arc<str>> = model
        .blocks
        .iter()
        .filter_map(|block| match block.params.values.get(1).map(|(_, v)| v) {
            Some(Value::String(text)) => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(values.len(), 128);
    assert!(values.iter().all(|text| text.len() == TEXT_BYTES));
    assert!(values.iter().all(|text| Arc::ptr_eq(text, values[0])));
    assert!(
        info.bytes_total <= bytes(2 * TEXT_BYTES) + 64 * bytes(document.len()),
        "{} bytes",
        info.bytes_total
    );
}

/// [`MODEL`] with `2 × nodes` more `Constant` leaves, leaf `i` listing its own `k` and the array
/// node `q(i)`, declared at one element with the value `fill(1.0, 1000000)`; `node_ids` are the
/// array nodes to declare.
fn mismatched_arrays(leaves: usize, q: impl Fn(usize) -> String, node_ids: &[String]) -> Vec<u8> {
    let mut contains = String::new();
    let mut nodes = String::new();
    for i in 0..leaves {
        contains.push_str(&format!(", {{ \"@id\": \"urn:c{i}\" }}"));
        nodes.push_str(&format!(
            ",\n    {{ \"@id\": \"urn:c{i}\", \"@type\": \
             \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant\", \
             \"S231:hasParameter\": [ {{ \"@id\": \"urn:c{i}.k\" }}, {{ \"@id\": \"{}\" }} ], \
             \"S231:hasOutput\": {{ \"@id\": \"urn:c{i}.y\" }} }},\n    \
             {{ \"@id\": \"urn:c{i}.k\", \"S231:value\": 1.0 }},\n    \
             {{ \"@id\": \"urn:c{i}.y\", \"@type\": \"S231:RealOutput\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }} }}",
            q(i)
        ));
    }
    for id in node_ids {
        nodes.push_str(&format!(
            ",\n    {{ \"@id\": \"{id}\", \"S231:isArray\": true, \"S231:numberDimensions\": 1, \
             \"S231:sizeOfDimensions\": \"(1)\", \"S231:value\": \"fill(1.0, 1000000)\" }}"
        ));
    }
    MODEL
        .replacen(
            GT,
            &format!("{{ \"@id\": \"http://example.org#MinLoop.gt\" }}{contains}\n      ],"),
            1,
        )
        .replacen(
            LAST,
            &format!("\"S231:isOfDataType\": {{ \"@id\": \"S231:Boolean\" }} }}{nodes}\n  ]"),
            1,
        )
        .into_bytes()
}

#[test]
fn an_array_node_declared_at_one_element_keeps_no_million_element_evaluation() {
    // Array nodes declared at one element whose value builds a million, each listed by two
    // leaves: each listing's evaluation is refused by its shape and dropped, never kept for the
    // other leaf, so the peak is one evaluation's whatever the number of such nodes, as with a
    // node for each leaf. [measured: shared peaks of 24,135,083 / 24,231,658 / 24,387,942,
    // separate 24,137,542 / 24,241,006 / 24,407,542;
    // when the refused evaluation was kept, 24.1 / 96.2 / 192.3 MB (review)]
    let mut peaks = Vec::new();
    for nodes in [1, 4, 8] {
        let shared_ids: Vec<String> = (0..nodes).map(|m| format!("urn:a{m:02}.q")).collect();
        let shared = mismatched_arrays(2 * nodes, |i| shared_ids[i / 2].clone(), &shared_ids);
        let own_ids: Vec<String> = (0..2 * nodes).map(|i| format!("urn:a{i:02}.q")).collect();
        let separate = mismatched_arrays(2 * nodes, |i| own_ids[i].clone(), &own_ids);
        let (shared_result, shared_info) = read(&shared);
        let (separate_result, separate_info) = read(&separate);
        assert!(
            shared_result.is_err() && separate_result.is_err(),
            "both refused by shape"
        );
        assert!(
            shared_info.bytes_max <= separate_info.bytes_max + 1024 * 1024,
            "{nodes} nodes: a peak of {} shared, {} separate",
            shared_info.bytes_max,
            separate_info.bytes_max
        );
        peaks.push(shared_info.bytes_max);
    }
    assert!(peaks[2] <= peaks[0] + 2 * 1024 * 1024, "{peaks:?}");
}
