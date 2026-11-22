//! The resolver's signature probes are charged to the constructed budget (issue #722).
//!
//! Step 8 of the resolver constructs each leaf to read its resolved signature, then drops it.
//! Each construction is charged to one `oce_blocks::ConstructedBudget` before the next: exactly
//! the budget is probed without a finding, and the first leaf that takes the total past it is one
//! `resource-limit-exceeded` error naming that leaf, after which no leaf is constructed or bound.
//! The leaves are `TimeTable`s sized by a lone corner cell (the load gate refuses such a table
//! later; the resolver sizes it all the same, which is what the budget bounds). The facade's
//! allocation measurements are in `oce-api/tests/cxf_constructed_budget.rs`.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic, Severity};
use oce_model::ModelGraph;
use serde_json::{Value as Json, json};

/// A table of 524,287 rows reports 1,048,576 words: four of them are exactly the budget.
const QUARTER_BUDGET: &str = "table_524287_2";
/// A one-row table: four words.
const SMALLEST: &str = "table_1_2";
const TABLE: &str = "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.TimeTable";
const CONSTANT: &str = "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant";

fn id(local: &str) -> String {
    format!("http://example.org#M.{local}")
}

/// One leaf of the root: its name and its nodes.
struct Leaf {
    name: String,
    nodes: Vec<Json>,
}

/// A `TimeTable` leaf `name` binding the one corner cell `cell`.
fn table(name: &str, cell: &str) -> Leaf {
    Leaf {
        name: name.to_owned(),
        nodes: vec![
            json!({ "@id": id(name), "@type": TABLE,
                    "S231:hasParameter": { "@id": id(&format!("{name}.{cell}")) },
                    "S231:hasOutput": { "@id": id(&format!("{name}.y")) } }),
            json!({ "@id": id(&format!("{name}.{cell}")), "@type": "S231:Parameter",
                    "S231:isOfDataType": { "@id": "S231:Real" }, "S231:value": 0.0 }),
            json!({ "@id": id(&format!("{name}.y")), "@type": "S231:RealOutput",
                    "S231:isOfDataType": { "@id": "S231:Real" } }),
        ],
    }
}

/// A `Constant` leaf `name` declaring two outputs: an interface mismatch wherever it stands.
fn mismatched_constant(name: &str) -> Leaf {
    Leaf {
        name: name.to_owned(),
        nodes: vec![
            json!({ "@id": id(name), "@type": CONSTANT,
                    "S231:hasParameter": { "@id": id(&format!("{name}.k")) },
                    "S231:hasOutput": [ { "@id": id(&format!("{name}.y")) },
                                        { "@id": id(&format!("{name}.z")) } ] }),
            json!({ "@id": id(&format!("{name}.k")), "S231:value": 1.0 }),
            json!({ "@id": id(&format!("{name}.y")), "@type": "S231:RealOutput",
                    "S231:isOfDataType": { "@id": "S231:Real" } }),
            json!({ "@id": id(&format!("{name}.z")), "@type": "S231:RealOutput",
                    "S231:isOfDataType": { "@id": "S231:Real" } }),
        ],
    }
}

/// A leaf of a class no registry entry has.
fn unregistered(name: &str) -> Leaf {
    Leaf {
        name: name.to_owned(),
        nodes: vec![json!({ "@id": id(name),
                            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.NoSuchClass" })],
    }
}

fn document(leaves: &[Leaf]) -> Vec<u8> {
    let mut graph = vec![json!({
        "@id": "http://example.org#M", "@type": "S231:Block",
        "S231:containsBlock": leaves.iter().map(|l| json!({ "@id": id(&l.name) })).collect::<Vec<_>>(),
    })];
    graph.extend(leaves.iter().flat_map(|leaf| leaf.nodes.iter().cloned()));
    serde_json::to_vec(&json!({
        "@context": { "S231": "http://data.ashrae.org/S231#" },
        "@graph": graph,
    }))
    .expect("serialize")
}

fn tables(cells: &[&str]) -> Vec<Leaf> {
    cells
        .iter()
        .enumerate()
        .map(|(index, cell)| table(&format!("t{index}"), cell))
        .collect()
}

fn import(leaves: &[Leaf]) -> Result<ModelGraph, Vec<Diagnostic>> {
    match import_cxf(&document(leaves), &ResolveOptions::default()) {
        Ok((graph, _)) => Ok(graph),
        Err(CxfError::Validation(diags)) => Err(diags),
        Err(other) => panic!("expected a validation outcome, got {other:?}"),
    }
}

fn budget_refusals(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags
        .iter()
        .filter(|d| d.code == DiagCode::ResourceLimitExceeded)
        .collect()
}

fn crossing(subject: &str, requested: u64) -> Diagnostic {
    Diagnostic::error(
        DiagCode::ResourceLimitExceeded,
        format!(
            "constructed blocks too large: {requested} word(s) held by the blocks through this \
             block exceed the supported maximum (4194304 words). Remedy: reduce table, matrix or \
             vector sizes, or split the model."
        ),
    )
    .with_subject(id(subject))
}

#[test]
fn exactly_the_budget_is_probed_and_one_word_more_is_refused_on_the_leaf_that_crosses_it() {
    let at_budget = import(&tables(&[QUARTER_BUDGET; 4])).expect("exactly the budget resolves");
    assert_eq!(at_budget.blocks.len(), 4);

    for cells in [
        [
            QUARTER_BUDGET,
            QUARTER_BUDGET,
            QUARTER_BUDGET,
            QUARTER_BUDGET,
            SMALLEST,
        ],
        [
            SMALLEST,
            QUARTER_BUDGET,
            QUARTER_BUDGET,
            QUARTER_BUDGET,
            QUARTER_BUDGET,
        ],
    ] {
        let diags = import(&tables(&cells)).expect_err("one word past the budget");
        // The only finding: the budget, on the fifth leaf in `containsBlock` order.
        assert_eq!(diags, [crossing("t4", (1 << 22) + 4)], "{cells:?}");
        assert_eq!(diags[0].severity, Severity::Error);
    }
}

#[test]
fn a_leaf_that_holds_more_than_the_rest_of_the_budget_is_refused_with_the_saturating_total() {
    // Four tables of 524,288 rows: each one row (two words) over a quarter, so the fourth
    // crosses, and the request counts the whole of it.
    let diags = import(&tables(&["table_524288_2"; 4])).expect_err("past the budget");
    assert_eq!(diags, [crossing("t3", 4 * ((1 << 20) + 2))]);
}

#[test]
fn no_leaf_after_the_crossing_is_bound_and_every_leaf_before_it_is() {
    let mut after = tables(&[QUARTER_BUDGET; 5]);
    after.push(mismatched_constant("c"));
    let diags = import(&after).expect_err("past the budget");
    assert_eq!(diags, [crossing("t4", 5 * (1 << 20))]);

    let mut before = vec![mismatched_constant("c")];
    before.extend(tables(&[QUARTER_BUDGET; 5]));
    let diags = import(&before).expect_err("past the budget and mismatched");
    let refusals = budget_refusals(&diags);
    assert_eq!(refusals, [&crossing("t4", 5 * (1 << 20))]);
    assert!(
        diags
            .iter()
            .any(|d| d.subject.as_deref() == Some(id("c").as_str())
                && d.code != DiagCode::ResourceLimitExceeded),
        "the mismatch before the crossing is still reported: {diags:?}"
    );
}

#[test]
fn an_unregistered_leaf_is_not_constructed_and_charges_nothing() {
    let mut leaves = tables(&[QUARTER_BUDGET; 2]);
    leaves.push(unregistered("u"));
    leaves.extend(tables(&[QUARTER_BUDGET; 2]).into_iter().map(|mut leaf| {
        let renamed = format!("{}b", leaf.name);
        let text = serde_json::to_string(&leaf.nodes)
            .expect("serialize")
            .replace(&format!("M.{}", leaf.name), &format!("M.{renamed}"));
        leaf.nodes = serde_json::from_str(&text).expect("parse");
        leaf.name = renamed;
        leaf
    }));
    let diags = import(&leaves).expect_err("the unregistered class is refused");
    assert!(budget_refusals(&diags).is_empty(), "{diags:?}");
    assert!(diags.iter().any(|d| d.code == DiagCode::ClassNotFound));
}

#[test]
fn the_probe_refusal_is_byte_identical_across_runs() {
    let leaves = tables(&[QUARTER_BUDGET; 6]);
    let first = import(&leaves).expect_err("past the budget");
    for _ in 0..3 {
        assert_eq!(import(&leaves).expect_err("past the budget"), first);
    }
}
