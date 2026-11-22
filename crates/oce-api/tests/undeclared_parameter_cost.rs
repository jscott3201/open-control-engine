//! What refusing undeclared parameter names costs, in requested bytes and allocations and never
//! in elapsed time (issue #748; `TESTING.md`: cost is asserted with deterministic measures).
//!
//! Each name a block binds is looked up among its class's declared names, and each undeclared
//! one is refused with a finding that names it and, when one is close, the declared name it
//! probably meant. A hostile table can carry many names, each long: the lookup allocates nothing,
//! the suggestion rejects a name whose length is far from every declared name's by its length
//! alone, a finding quotes at most a 64-byte excerpt of its name, and every finding is raised
//! through the load gate's bounded diagnostic list (8 MiB of text), so once the list has ended a
//! further name costs a count and nothing is built for it.
//!
//! Held here, through `oce_validate::validate_block_parameters` (the load gate's parameter checks
//! on one block, the path `class_ports` takes; no document bound applies) and through
//! `Engine::load_cxf` on documents under the facade's 8 MiB bound:
//!
//! - **a peak** of the list's budget plus a fixed amount a name, however long the names are;
//! - **requested bytes** and **allocations** that grow by a fixed amount a name between two
//!   sizes, never by a name's length;
//! - **the count**: every name is listed or counted in the list's closing error.
//!
//! Measured with `allocation-counter`, whose counters are local to the measuring thread. The
//! ceilings have headroom over what was measured (in brackets). What this cannot see: work that
//! allocates nothing, which here is one pass over a class's dozen declared names a binding.

use std::sync::Arc;

use allocation_counter::{AllocationInfo, measure};
use oce_api::Engine;
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{ParamTable, Value};

/// The load gate's budget of diagnostic text (`oce_diag::MAX_DIAGNOSTIC_TEXT_BYTES`), restated
/// so this test fails if it moves without review.
const BUDGET: u64 = 8 * 1024 * 1024;

const CONSTANT: &str = "CDL.Reals.Sources.Constant";

/// `count` distinct undeclared names of `bytes` bytes each (a decimal tail keeps them distinct),
/// all bound to `1.0`, after the constant's own `k`.
fn hostile_table(count: usize, bytes: usize) -> ParamTable {
    let mut values: Vec<(Arc<str>, Value)> = vec![(Arc::from("k"), Value::Real(1.0))];
    values.extend((0..count).map(|index| {
        let tail = format!("_x{index}");
        let name = format!("{}{tail}", "q".repeat(bytes - tail.len()));
        (Arc::from(name.as_str()), Value::Real(1.0))
    }));
    ParamTable { values }
}

/// The diagnostics the load gate refuses `table` with, and what refusing it cost.
fn refused(table: &ParamTable) -> (Vec<Diagnostic>, AllocationInfo) {
    let mut result = None;
    let info = measure(|| result = Some(oce_validate::validate_block_parameters(CONSTANT, table)));
    let error = result
        .expect("ran")
        .expect_err("the undeclared names are refused");
    (error.diagnostics, info)
}

/// How many names the refusal lists or counts: each listed `unknown-parameter` finding, plus the
/// count the closing error states when the list ended.
fn accounted(diagnostics: &[Diagnostic]) -> usize {
    let listed = diagnostics
        .iter()
        .filter(|d| d.code == DiagCode::UnknownParameter)
        .count();
    let counted = diagnostics
        .iter()
        .filter(|d| d.code == DiagCode::ResourceLimitExceeded)
        .map(|d| {
            d.message
                .split_whitespace()
                .find_map(|word| word.parse::<usize>().ok())
                .expect("the closing error counts what it left out")
        })
        .sum::<usize>();
    listed + counted
}

#[test]
fn many_short_undeclared_names_cost_a_fixed_amount_each() {
    // 50,000 and 100,000 names of 64 bytes: every one is listed or counted, and the second
    // 50,000 cost what the first did. [measured: 40.9 and 47.1 MB requested in 300,021 and
    // 328,994 allocations, about 123 bytes and 0.6 of an allocation a further name, at peaks of
    // 16.0 and 19.1 MB]
    let mut infos = Vec::new();
    for count in [50_000, 100_000] {
        let table = hostile_table(count, 64);
        let (diagnostics, info) = refused(&table);
        assert_eq!(accounted(&diagnostics), count, "{count}");
        assert!(
            info.bytes_max <= 2 * BUDGET + 128 * count as u64,
            "{count}: {} bytes held",
            info.bytes_max
        );
        infos.push(info);
    }
    let further = infos[1].bytes_total - infos[0].bytes_total;
    assert!(
        further <= 50_000 * 1_024,
        "{further} more bytes for 50,000 more names"
    );
    let allocations = infos[1].count_total - infos[0].count_total;
    assert!(
        allocations <= 50_000 * 8,
        "{allocations} more allocations for 50,000 more names"
    );
}

#[test]
fn long_undeclared_names_cost_a_fixed_amount_each_whatever_their_length() {
    // 200 and 400 names of 64 KiB: each finding quotes a 64-byte excerpt of its name, never the
    // name whole, so every one is listed in a short message and the second 200 cost about what
    // the first did, nothing by the 64 KiB. [measured: 202,936 and 405,464 bytes
    // requested in 1,613 and 3,214 allocations, about 1 KB and 8 allocations a name, at peaks of
    // 65,024 and 129,512 bytes; before the excerpt, 33.5 MB each]
    let mut infos = Vec::new();
    for count in [200, 400] {
        let table = hostile_table(count, 64 * 1024);
        let (diagnostics, info) = refused(&table);
        assert_eq!(accounted(&diagnostics), count, "{count}");
        assert!(
            diagnostics.iter().all(|d| d.message.len() < 256),
            "a message quotes a name whole"
        );
        assert!(
            info.bytes_max <= BUDGET,
            "{count}: {} bytes held",
            info.bytes_max
        );
        infos.push(info);
    }
    let further = infos[1].bytes_total - infos[0].bytes_total;
    assert!(
        further <= 200 * 1_024,
        "{further} more bytes for 200 more names of 64 KiB"
    );
}

/// A one-leaf document whose constant binds `k` and `count` undeclared names of `bytes` bytes.
fn hostile_document(count: usize, bytes: usize) -> Vec<u8> {
    let id = "http://example.org#M.con";
    let names: Vec<String> = (0..count)
        .map(|index| {
            let tail = format!("_x{index}");
            format!("{}{tail}", "q".repeat(bytes - tail.len()))
        })
        .collect();
    let references: String = names
        .iter()
        .map(|name| format!(r#",{{"@id":"{id}.{name}"}}"#))
        .collect();
    let nodes: String = names
        .iter()
        .map(|name| format!(r#",{{"@id":"{id}.{name}","S231:value":1.0}}"#))
        .collect();
    format!(
        r#"{{"@context":{{"S231":"http://data.ashrae.org/S231#"}},"@graph":[
{{"@id":"http://example.org#M","@type":"S231:Block","S231:containsBlock":{{"@id":"{id}"}},"S231:hasOutput":{{"@id":"http://example.org#M.y"}}}},
{{"@id":"http://example.org#M.y","@type":"S231:RealOutput","S231:isOfDataType":{{"@id":"S231:Real"}}}},
{{"@id":"{id}","@type":"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant","S231:hasParameter":[{{"@id":"{id}.k"}}{references}],"S231:hasOutput":{{"@id":"{id}.y"}}}},
{{"@id":"{id}.k","S231:value":1.0}}{nodes},
{{"@id":"{id}.y","@type":"S231:RealOutput","S231:isOfDataType":{{"@id":"S231:Real"}},"S231:isConnectedTo":{{"@id":"http://example.org#M.y"}}}}
]}}"#
    )
    .into_bytes()
}

#[test]
fn a_document_of_many_undeclared_names_costs_the_load_gate_a_fixed_amount_a_name() {
    // 10,000 and 20,000 names of 64 bytes (documents of 2.2 and 4.3 MB), each refused for its
    // names and nothing else. Reading the document is the reader's cost and is measured beside
    // the load: what the facade requests beyond it, the load gate's, grows by a fixed amount a
    // name, and the peak is the reader's. [measured: the load gate requested 7.2 and 14.3 MB
    // beyond the reader's 94 and 188 MB, about 720 bytes and 3 allocations a name; the peaks
    // were the reader's, 54 and 107 MB]
    let mut beyond = Vec::new();
    for count in [10_000, 20_000] {
        let document = hostile_document(count, 64);
        assert!(document.len() < oce_api::MAX_CXF_BYTES);
        let mut read = None;
        let reader = measure(|| {
            read = Some(oce_cxf::import_cxf(
                &document,
                &oce_cxf::ResolveOptions::default(),
            ));
        });
        read.expect("ran").expect("the reader admits the names");
        let mut engine = Engine::new();
        let mut result = None;
        let info = measure(|| result = Some(engine.load_cxf(&document)));
        let error = result.expect("ran").expect_err("refused");
        let diagnostics: Vec<Diagnostic> = error.all_diagnostics().cloned().collect();
        assert_eq!(accounted(&diagnostics), count, "{count}");
        assert!(
            diagnostics.iter().all(|d| matches!(
                d.code,
                DiagCode::UnknownParameter | DiagCode::ResourceLimitExceeded
            )),
            "{:?}",
            diagnostics.first()
        );
        assert!(
            info.bytes_max <= reader.bytes_max + BUDGET,
            "{count}: {} bytes held, the reader {}",
            info.bytes_max,
            reader.bytes_max
        );
        let gate_bytes = info.bytes_total - reader.bytes_total;
        let gate_allocations = info.count_total - reader.count_total;
        assert!(
            gate_bytes <= 2_048 * count as u64 && gate_allocations <= 8 * count as u64,
            "{count}: the load gate requested {gate_bytes} bytes in {gate_allocations} allocations"
        );
        beyond.push(gate_bytes);
    }
    assert!(
        beyond[1] - beyond[0] <= 10_000 * 1_024,
        "{beyond:?} bytes beyond the reader"
    );
}
