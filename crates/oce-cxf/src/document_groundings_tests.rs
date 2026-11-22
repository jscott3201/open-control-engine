//! What a parameter node many owners list costs to ground, counted in groundings and parses and
//! never in elapsed time (issue #749; `TESTING.md`: work is pinned by a count of it).
//!
//! Every owner that lists a parameter node grounds its value: a leaf in resolver Step 7, a
//! composite in lowering, a guard scope in the specialization pass, and the two passes over a
//! composite's declarations parse its expression for their dependency order. So a node `N` owners
//! list was grounded `N` times in each pass, while each further owner costs the document one
//! reference. A value whose grounding looks up no name is now grounded once per pass and shared;
//! one that reads a name is still grounded under each owner's scope. The tables the document
//! builds are the ones each owner grounding its own copy builds.

use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{ModelGraph, Value};

use super::{DocumentGroundings, heads_parsed_by};
use crate::dto::{CxfDocument, CxfValue, Node};
use crate::parameter_reads::reads_counted_by;
use crate::{CxfError, ResolveOptions, import_cxf, parse_document};

const MODEL: &str = include_str!("../tests/fixtures/minimal_loop.jsonld");
const GT: &str = "{ \"@id\": \"http://example.org#MinLoop.gt\" }\n      ],";
const LAST: &str = "\"S231:isOfDataType\": { \"@id\": \"S231:Boolean\" } }\n  ]";

/// The groundings [`MODEL`]'s own parameters take: the constant's `k`, the delay's `y_start` and
/// `samplePeriod`, and the gain's `k`.
pub(super) const MODEL_GROUNDINGS: u64 = 4;

/// [`MODEL`] with `leaves` more `Constant` leaves, each listing its own `k` and then the node
/// `shared(i)` names for leaf `i`, and `nodes` declared at the end.
pub(super) fn leaves(leaves: usize, shared: impl Fn(usize) -> String, nodes: &str) -> Vec<u8> {
    assert_eq!(MODEL.matches(GT).count(), 1);
    assert_eq!(MODEL.matches(LAST).count(), 1);
    let mut contains = String::new();
    let mut declared = String::new();
    for i in 0..leaves {
        let q = shared(i);
        contains.push_str(&format!(", {{ \"@id\": \"urn:c{i}\" }}"));
        declared.push_str(&format!(
            ",\n    {{ \"@id\": \"urn:c{i}\", \"@type\": \
             \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant\", \
             \"S231:hasParameter\": [ {{ \"@id\": \"urn:c{i}.k\" }}, {{ \"@id\": \"{q}\" }} ], \
             \"S231:hasOutput\": {{ \"@id\": \"urn:c{i}.y\" }} }},\n    \
             {{ \"@id\": \"urn:c{i}.k\", \"S231:value\": 1.0 }},\n    \
             {{ \"@id\": \"urn:c{i}.y\", \"@type\": \"S231:RealOutput\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }} }}"
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
            &format!(
                "\"S231:isOfDataType\": {{ \"@id\": \"S231:Boolean\" }} }}{declared}{nodes}\n  ]"
            ),
            1,
        )
        .into_bytes()
}

/// A scalar parameter node `id` whose `S231:value` is the JSON `value`, led by a comma.
pub(super) fn node(id: &str, value: &str) -> String {
    format!(",\n    {{ \"@id\": \"{id}\", \"S231:value\": {value} }}")
}

/// `leaves` leaves listing the one node `urn:shared.q` of `value`.
pub(super) fn shared(count: usize, value: &str) -> Vec<u8> {
    leaves(
        count,
        |_| "urn:shared.q".to_owned(),
        &node("urn:shared.q", value),
    )
}

/// `leaves` leaves each listing a node of its own, `urn:own<iii>.q` (zero-padded, so the ids sort in leaf order), of `value`.
pub(super) fn separate(count: usize, value: &str) -> Vec<u8> {
    let nodes: String = (0..count)
        .map(|i| node(&format!("urn:own{i:03}.q"), value))
        .collect();
    leaves(count, |i| format!("urn:own{i:03}.q"), &nodes)
}

/// The groundings one read of `bytes` makes, and what it read.
pub(super) fn read_counted(bytes: &[u8]) -> (u64, Result<ModelGraph, CxfError>) {
    let (_, grounded, result) =
        reads_counted_by(|| import_cxf(bytes, &ResolveOptions::default()).map(|(model, _)| model));
    (grounded, result)
}

/// Every parameter table of `model`, by instance: names and values.
pub(super) fn tables(model: &ModelGraph) -> Vec<Vec<(Arc<str>, Value)>> {
    model
        .blocks
        .iter()
        .map(|block| block.params.values.clone())
        .collect()
}

/// Whether two sets of tables hold the same names and bit-identical values.
pub(super) fn same_tables(a: &[Vec<(Arc<str>, Value)>], b: &[Vec<(Arc<str>, Value)>]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|((na, va), (nb, vb))| na == nb && va.bit_eq(vb))
        })
}

#[test]
fn a_value_many_leaves_list_is_grounded_once_for_the_document() {
    // The issue's shape at 16, 32 and 64 leaves, a string value: one grounding for the shared
    // node whatever the number of leaves, beside each leaf's own `k`. [measured: 21 / 37 / 69;
    // each leaf grounded the shared value again before: 36 / 68 / 132]
    for count in [16, 32, 64] {
        let (grounded, result) = read_counted(&shared(count, "\"\\\"text\\\"\""));
        assert_eq!(
            grounded,
            MODEL_GROUNDINGS + count as u64 + 1,
            "{count} leaves"
        );
        let model = result.expect("the reader reads it");
        let shared: Vec<&Arc<str>> = model
            .blocks
            .iter()
            .filter_map(|block| match &block.params.values.get(1)?.1 {
                Value::String(text) => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(shared.len(), count);
        assert!(shared.iter().all(|text| Arc::ptr_eq(text, shared[0])));
    }
}

#[test]
fn every_leaf_holds_what_grounding_its_own_copy_of_the_node_gives() {
    // The same values written once per leaf, each on its own node: the tables are equal entry by
    // entry, for a string, a Real, an Integer expression, a typed literal and a Boolean.
    for value in [
        "\"\\\"text\\\"\"",
        "\"2.5 * 2\"",
        "\"3 + 4\"",
        "{ \"@value\": \"0.5\", \"@type\": \"http://www.w3.org/2001/XMLSchema#double\" }",
        "\"true and not false\"",
        "1.5",
    ] {
        let (_, shared) = read_counted(&shared(8, value));
        let (_, separate) = read_counted(&separate(8, value));
        let (shared, separate) = (shared.expect("reads"), separate.expect("reads"));
        assert!(
            same_tables(&tables(&shared), &tables(&separate)),
            "{value}: a shared node grounds differently from a node of each leaf's own"
        );
    }
}

#[test]
fn a_value_that_reads_a_name_is_grounded_under_each_leaf() {
    // `k + 1` reads each leaf's own `k`, so each leaf grounds it: no sharing, and each leaf holds
    // what its own scope gives. [measured: 4 + 16 + 16]
    let (grounded, result) = read_counted(&shared(16, "\"k + 1\""));
    assert_eq!(grounded, MODEL_GROUNDINGS + 16 + 16);
    let model = result.expect("reads");
    assert!(model.blocks[5..].iter().all(|block| {
        block.params.values[1].0.as_ref() == "q"
            && block.params.values[1].1.bit_eq(&Value::Real(2.0))
    }));
    let (_, separate) = read_counted(&separate(16, "\"k + 1\""));
    assert!(same_tables(
        &tables(&model),
        &tables(&separate.expect("reads"))
    ));
}

#[test]
fn a_shared_value_that_fails_is_grounded_once_and_reported_for_every_leaf() {
    // `1/0` reads no name and fails: one grounding, and the failure on each leaf, as each
    // leaf's own copy reports it.
    let (grounded, result) = read_counted(&shared(8, "\"1/0\""));
    assert_eq!(grounded, MODEL_GROUNDINGS + 8 + 1);
    let Err(CxfError::Validation(diagnostics)) = result else {
        panic!("refused")
    };
    let failures: Vec<&Diagnostic> = diagnostics
        .iter()
        .filter(|d| d.code == DiagCode::NonFiniteParameter)
        .collect();
    assert_eq!(failures.len(), 8);
    let Err(CxfError::Validation(separate)) = read_counted(&separate(8, "\"1/0\"")).1 else {
        panic!("refused")
    };
    let separate: Vec<&Diagnostic> = separate
        .iter()
        .filter(|d| d.code == DiagCode::NonFiniteParameter)
        .collect();
    assert_eq!(failures, separate);
}

#[test]
fn a_node_one_leaf_lists_twice_is_not_shared_with_the_document() {
    // Listed twice by one leaf: the leaf's own reads keep it (one grounding), and the document
    // keeps nothing for a node no second owner lists.
    let bytes = leaves(
        1,
        |_| "urn:once.q".to_owned(),
        &node("urn:once.q", "\"\\\"t\\\"\""),
    );
    let twice = String::from_utf8(bytes).expect("utf-8").replace(
        "{ \"@id\": \"urn:once.q\" } ]",
        "{ \"@id\": \"urn:once.q\" }, { \"@id\": \"urn:once.q\" } ]",
    );
    let (grounded, _) = read_counted(twice.as_bytes());
    assert_eq!(grounded, MODEL_GROUNDINGS + 1 + 1);
    // Two leaves listing it: the edge at which the document shares it.
    let (two, _) = read_counted(&shared(2, "\"\\\"t\\\"\""));
    assert_eq!(two, MODEL_GROUNDINGS + 2 + 1);
    let (one, _) = read_counted(&shared(1, "\"\\\"t\\\"\""));
    assert_eq!(one, MODEL_GROUNDINGS + 1 + 1);
}

/// A root composite over `count` composites `urn:s<i>`, each declaring a Boolean `flag` of its
/// own and the node `q(i)` names, holding a `Constant` and a `Not` conditional on `flag`.
pub(super) fn composites(count: usize, q: impl Fn(usize) -> String, nodes: &str) -> Vec<u8> {
    let subs: Vec<String> = (0..count)
        .map(|i| format!("{{ \"@id\": \"urn:s{i}\" }}"))
        .collect();
    let mut graph = format!(
        "{{ \"@id\": \"urn:r\", \"@type\": \"S231:Block\", \"S231:containsBlock\": [ {} ] }}",
        subs.join(", ")
    );
    for i in 0..count {
        let q = q(i);
        graph.push_str(&format!(
            ",\n{{ \"@id\": \"urn:s{i}\", \"@type\": \"http://example.org#Vendor.Sub\", \
             \"S231:hasParameter\": [ {{ \"@id\": \"urn:s{i}.flag\" }}, {{ \"@id\": \"{q}\" }} ], \
             \"S231:containsBlock\": [ {{ \"@id\": \"urn:s{i}.cond\" }}, {{ \"@id\": \"urn:s{i}.con\" }} ] }},\n\
             {{ \"@id\": \"urn:s{i}.flag\", \"@type\": \"S231:Parameter\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:Boolean\" }}, \"S231:value\": false }},\n\
             {{ \"@id\": \"urn:s{i}.cond\", \"@type\": \
             \"http://example.org#Buildings.Controls.OBC.CDL.Logical.Not\", \
             \"S231:isConditionalComponent\": true, \"S231:conditionalExpression\": \"flag\" }},\n\
             {{ \"@id\": \"urn:s{i}.con\", \"@type\": \
             \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant\", \
             \"S231:hasParameter\": {{ \"@id\": \"urn:s{i}.con.k\" }}, \
             \"S231:hasOutput\": {{ \"@id\": \"urn:s{i}.con.y\" }} }},\n\
             {{ \"@id\": \"urn:s{i}.con.k\", \"S231:value\": 1.0 }},\n\
             {{ \"@id\": \"urn:s{i}.con.y\", \"@type\": \"S231:RealOutput\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }} }}"
        ));
    }
    format!(
        "{{ \"@context\": {{ \"S231\": \"http://data.ashrae.org/S231#\" }}, \"@graph\": [\n{graph}{nodes}\n] }}"
    )
    .into_bytes()
}

/// A String declaration node `id` of the expression `value`, led by a comma.
pub(super) fn declaration(id: &str, value: &str) -> String {
    format!(
        ",\n{{ \"@id\": \"{id}\", \"@type\": \"S231:Parameter\", \
         \"S231:isOfDataType\": {{ \"@id\": \"S231:String\" }}, \"S231:value\": {value} }}"
    )
}

#[test]
fn a_declaration_many_composites_list_is_parsed_and_grounded_once_a_pass() {
    // Composites, each with a conditional child: the specialization pass builds a guard scope
    // for each, and lowering evaluates each one's declarations. The shared string is parsed for
    // its dependencies and grounded once in each of the two passes; each composite's own `flag`
    // is grounded in both. [measured: parses 2 / 2 / 2 and groundings 2 + 2n; each composite
    // parsed and grounded the shared value again in each pass before: 2n parses, 4n groundings]
    for count in [8, 16, 32] {
        let bytes = composites(
            count,
            |_| "urn:shared.q".to_owned(),
            &declaration("urn:shared.q", "\"\\\"text\\\"\""),
        );
        let (parsed, (grounded, result)) = heads_parsed_by(|| read_counted(&bytes));
        assert_eq!(parsed, 2, "{count} composites");
        let own_k = count as u64;
        assert_eq!(grounded, 2 + 2 * count as u64 + own_k, "{count} composites");
        let separate: String = (0..count)
            .map(|i| declaration(&format!("urn:own{i:03}.q"), "\"\\\"text\\\"\""))
            .collect();
        let separate = composites(count, |i| format!("urn:own{i:03}.q"), &separate);
        let (parsed, (_, separate)) = heads_parsed_by(|| read_counted(&separate));
        assert_eq!(parsed, 2 * count as u64);
        match (result, separate) {
            (Ok(a), Ok(b)) => assert!(same_tables(&tables(&a), &tables(&b))),
            (Err(CxfError::Validation(a)), Err(CxfError::Validation(b))) => {
                let codes = |d: &[Diagnostic]| d.iter().map(|d| d.code).collect::<Vec<_>>();
                assert_eq!(codes(&a), codes(&b));
            }
            (a, b) => panic!("outcomes differ: {:?} / {:?}", a.is_ok(), b.is_ok()),
        }
    }
}

#[test]
fn exactly_the_expression_values_of_nodes_two_owners_list_are_shared() {
    // `a` is listed by two owners, `b` twice by one, `c` by one, `d` by three; `e` is listed by
    // two owners but has no node. Only the nodes of `a` and `d` are shared, and of their values
    // only an expression or a typed literal is kept: a literal grounds at no cost.
    let doc: CxfDocument = parse_document(
        br#"{ "@context": { "S231": "http://data.ashrae.org/S231#" }, "@graph": [
          { "@id": "urn:o1", "S231:hasParameter": [ { "@id": "urn:a" }, { "@id": "urn:b" },
            { "@id": "urn:b" }, { "@id": "urn:d" }, { "@id": "urn:e" } ] },
          { "@id": "urn:o2", "S231:hasParameter": { "@id": "urn:a" },
            "S231:hasConstant": [ { "@id": "urn:c" }, { "@id": "urn:d" }, { "@id": "urn:e" } ] },
          { "@id": "urn:o3", "S231:hasConstant": { "@id": "urn:d" } },
          { "@id": "urn:a", "S231:value": "1 + 1" },
          { "@id": "urn:b", "S231:value": "1 + 1" },
          { "@id": "urn:c", "S231:value": "1 + 1" },
          { "@id": "urn:d", "S231:value": "1 + 1" } ] }"#,
    )
    .expect("parses");
    let by_id: std::collections::HashMap<&str, &Node> = doc
        .graph
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let groundings = DocumentGroundings::of(&doc.graph, &by_id);
    let expression = CxfValue::Expr("1 + 1".to_owned());
    let typed = CxfValue::Typed {
        value: "1".to_owned(),
        datatype: "http://www.w3.org/2001/XMLSchema#double".to_owned(),
        extra: std::collections::BTreeMap::new(),
    };
    for (id, shared) in [
        ("urn:a", true),
        ("urn:b", false),
        ("urn:c", false),
        ("urn:d", true),
    ] {
        let node = by_id[id];
        assert_eq!(groundings.shares(node, &expression), shared, "{id}");
        assert_eq!(groundings.shares(node, &typed), shared, "{id}");
        for literal in [CxfValue::Float(1.0), CxfValue::Int(1), CxfValue::Bool(true)] {
            assert!(
                !groundings.shares(node, &literal),
                "{id}: a literal is never kept"
            );
        }
    }
    assert_eq!(groundings.shared.len(), 2);
}

#[test]
fn an_array_expression_many_leaves_list_is_evaluated_once() {
    // A shared array node `q[2]` whose value is an expression, listed by 16 leaves: evaluated
    // once, and every leaf holds its two elements. A positional list of two expressions is
    // grounded once an element. [measured: 4 + 16 + 1 and 4 + 16 + 2]
    let array = |value: &str| {
        format!(
            ",\n    {{ \"@id\": \"urn:shared.q\", \"S231:isArray\": true, \
             \"S231:numberDimensions\": 1, \"S231:sizeOfDimensions\": \"(2)\", \
             \"S231:value\": {value} }}"
        )
    };
    let expression = leaves(16, |_| "urn:shared.q".to_owned(), &array("\"{1.5, 2.5}\""));
    let (grounded, result) = read_counted(&expression);
    assert_eq!(grounded, MODEL_GROUNDINGS + 16 + 1);
    let model = result.expect("reads");
    assert!(model.blocks[5..].iter().all(|block| {
        let values = &block.params.values;
        values.len() == 3
            && values[1].1.bit_eq(&Value::Real(1.5))
            && values[2].1.bit_eq(&Value::Real(2.5))
    }));
    let positional = leaves(
        16,
        |_| "urn:shared.q".to_owned(),
        &array("[ \"1.5\", \"2.5\" ]"),
    );
    let (grounded, result) = read_counted(&positional);
    assert_eq!(grounded, MODEL_GROUNDINGS + 16 + 2);
    assert!(same_tables(
        &tables(&model),
        &tables(&result.expect("reads"))
    ));
}
