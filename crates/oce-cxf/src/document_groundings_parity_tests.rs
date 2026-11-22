//! A node many owners list reads, for every owner, as the owner's own copy of the node reads:
//! the same value, and the same evaluation budget spent (issue #749). And what the document
//! keeps is bounded by what it paid for.
//!
//! The shared and the separate documents are the same leaves, one listing one node and the
//! other each listing a node of its own named so that the ids sort in leaf order; their
//! diagnostics are compared whole once the separate ids are renamed to the shared one.

use std::collections::HashMap;

use oce_model::Value;

use super::tests::{leaves, read_counted, separate, shared, tables};
use super::{DocumentGroundings, Retained};
use crate::dto::{CxfDocument, CxfValue, Node};
use crate::scope_chain::ParamScope;
use crate::{CxfError, MAX_DOCUMENT_EVAL_FUEL, ResolveOptions, import_cxf, parse_document};

/// `bytes` with leaf `i`'s own `k` bound to `i + 1`, for `count` leaves.
fn own_k(bytes: &[u8], count: usize) -> Vec<u8> {
    let mut text = String::from_utf8(bytes.to_vec()).expect("utf-8");
    for i in 0..count {
        let from = format!("{{ \"@id\": \"urn:c{i}.k\", \"S231:value\": 1.0 }}");
        let to = format!("{{ \"@id\": \"urn:c{i}.k\", \"S231:value\": {}.0 }}", i + 1);
        assert_eq!(text.matches(&from).count(), 1);
        text = text.replacen(&from, &to, 1);
    }
    text.into_bytes()
}

/// A preserved one-dimensional array node `id` of `size` elements with the JSON `value`, led by
/// a comma.
fn array(id: &str, size: usize, value: &str) -> String {
    format!(
        ",\n    {{ \"@id\": \"{id}\", \"S231:isArray\": true, \"S231:numberDimensions\": 1, \
         \"S231:sizeOfDimensions\": \"({size})\", \"S231:value\": {value} }}"
    )
}

/// Each leaf's `q` entries (every entry after its own `k`).
fn q_values(model: &oce_model::ModelGraph) -> Vec<Vec<Value>> {
    model.blocks[5..]
        .iter()
        .map(|block| {
            block.params.values[1..]
                .iter()
                .map(|(_, v)| v.clone())
                .collect()
        })
        .collect()
}

/// The outcome of reading `bytes`: its tables when it reads, and its diagnostics whole, with the
/// separate nodes `urn:own<iii>.q` of `count` leaves renamed to `urn:shared.q`.
fn outcome(bytes: &[u8], count: usize) -> (Option<String>, String) {
    match import_cxf(bytes, &ResolveOptions::default()) {
        Ok((model, report)) => (
            Some(format!("{:?}", tables(&model))),
            renamed(&format!("{report:?}"), count),
        ),
        Err(CxfError::Validation(diagnostics)) => {
            (None, renamed(&format!("{diagnostics:?}"), count))
        }
        Err(other) => (None, other.to_string()),
    }
}

/// `text` with every `urn:own<iii>.q` of `count` leaves renamed `urn:shared.q`.
fn renamed(text: &str, count: usize) -> String {
    (0..count).fold(text.to_owned(), |text, i| {
        text.replace(&format!("urn:own{i:03}.q"), "urn:shared.q")
    })
}

#[test]
fn a_shared_array_expression_that_reads_each_leafs_k_holds_each_leafs_own_value() {
    // `fill(k, 2)` reads each leaf's own `k`, so no leaf may be handed another's elements.
    let nodes = array("urn:shared.q", 2, "\"fill(k, 2)\"");
    let bytes = own_k(&leaves(6, |_| "urn:shared.q".to_owned(), &nodes), 6);
    let (_, result) = read_counted(&bytes);
    let values = q_values(&result.expect("reads"));
    for (i, elements) in values.iter().enumerate() {
        let k = Value::Real((i + 1) as f64);
        assert_eq!(elements.len(), 2, "leaf {i}");
        assert!(
            elements.iter().all(|v| v.bit_eq(&k)),
            "leaf {i}: {elements:?}"
        );
    }
}

#[test]
fn a_shared_value_that_reads_k_and_is_listed_twice_holds_each_leafs_own_value() {
    // Each leaf lists `q = k + 1` twice, so its reads are kept for the leaf: they must still be
    // the leaf's own, never the first leaf's. So must a shared broadcast `[k * 2]`.
    let twice = String::from_utf8(own_k(&shared(6, "\"k + 1\""), 6))
        .expect("utf-8")
        .replace(
            "{ \"@id\": \"urn:shared.q\" } ]",
            "{ \"@id\": \"urn:shared.q\" }, { \"@id\": \"urn:shared.q\" } ]",
        );
    let (_, result) = read_counted(twice.as_bytes());
    for (i, values) in q_values(&result.expect("reads")).iter().enumerate() {
        let expected = Value::Real((i + 2) as f64);
        assert_eq!(values.len(), 2, "leaf {i}");
        assert!(
            values.iter().all(|v| v.bit_eq(&expected)),
            "leaf {i}: {values:?}"
        );
    }
    let nodes = array("urn:shared.q", 3, "[ \"k * 2\" ]");
    let bytes = own_k(&leaves(6, |_| "urn:shared.q".to_owned(), &nodes), 6);
    let (_, result) = read_counted(&bytes);
    for (i, values) in q_values(&result.expect("reads")).iter().enumerate() {
        let expected = Value::Real(2.0 * (i + 1) as f64);
        assert_eq!(values.len(), 3, "leaf {i}");
        assert!(
            values.iter().all(|v| v.bit_eq(&expected)),
            "leaf {i}: {values:?}"
        );
    }
}

/// The fuel one grounding of `text` spends.
fn fuel_of(text: &str) -> u64 {
    oce_expr::with_eval_budget(MAX_DOCUMENT_EVAL_FUEL, || {
        let before = oce_expr::eval_fuel_remaining().expect("metered");
        oce_expr::eval_str_finite(text, &ParamScope::empty()).expect("evaluates");
        before - oce_expr::eval_fuel_remaining().expect("metered")
    })
}

#[test]
fn every_leaf_is_charged_what_its_own_copy_costs_so_the_outcome_is_the_same() {
    // The review's case: 40 leaves of `sum(fill(1.0, 1000000))`. Written out 40 times the
    // document's evaluation budget refuses the leaves past it; shared, every owner is charged
    // what its own grounding would spend, so the same leaves are refused with the same
    // diagnostics. And at the edge: the most leaves that fit, and one more.
    let text = "sum(fill(1.0, 1000000))";
    let fit = usize::try_from(MAX_DOCUMENT_EVAL_FUEL / fuel_of(text)).expect("fits");
    assert!(fit < 40, "{fit} leaves fit the budget");
    let value = format!("\"{text}\"");
    for count in [fit, fit + 1, 40] {
        let shared_outcome = outcome(&shared(count, &value), count);
        let separate_outcome = outcome(&separate(count, &value), count);
        assert_eq!(shared_outcome, separate_outcome, "{count} leaves");
        let refused = shared_outcome.1.matches("too expensive").count();
        assert_eq!(refused, count - fit, "{count} leaves: {}", shared_outcome.1);
        assert_eq!(shared_outcome.0.is_some(), count == fit, "{count} leaves");
    }
}

#[test]
fn a_shared_broadcast_is_charged_once_a_leaf_as_its_own_copy_is() {
    // A broadcast's one value is charged once per instance (issue #736); shared by 3 leaves it is
    // charged once for each, as 3 copies are.
    let value = "[ \"sum(1:1048576)\" ]";
    let nodes = array("urn:shared.q", 40, value);
    let bytes = leaves(3, |_| "urn:shared.q".to_owned(), &nodes);
    let copies: String = (0..3)
        .map(|i| array(&format!("urn:own{i:03}.q"), 40, value))
        .collect();
    let separate = leaves(3, |i| format!("urn:own{i:03}.q"), &copies);
    let shared_outcome = outcome(&bytes, 3);
    assert!(shared_outcome.0.is_some(), "{}", shared_outcome.1);
    assert_eq!(shared_outcome, outcome(&separate, 3));
}

#[test]
fn a_shared_value_past_the_budget_is_refused_for_each_leaf_as_its_own_copy_is() {
    // Values that fail without the fuel failing are kept, charged what they spent, and refused
    // for each leaf; a name the scope lacks is refused for each leaf, unkept.
    for value in [
        "\"1/0\"",
        "\"nope + 1\"",
        "\"1 +\"",
        "\"{1, 2}\"",
        "\"1e308*10\"",
    ] {
        let count = 5;
        assert_eq!(
            outcome(&shared(count, value), count),
            outcome(&separate(count, value), count),
            "{value}"
        );
    }
}

/// A document of `owners` owners each listing the node `urn:q`, whose value is `value`.
fn owned(owners: usize, value: &str) -> CxfDocument {
    let listings: String = (0..owners)
        .map(|i| {
            format!(
                "{{ \"@id\": \"urn:o{i}\", \"S231:hasParameter\": {{ \"@id\": \"urn:q\" }} }}, "
            )
        })
        .collect();
    parse_document(
        format!(
            "{{ \"@context\": {{ \"S231\": \"http://data.ashrae.org/S231#\" }}, \"@graph\": [ \
             {listings}{{ \"@id\": \"urn:q\", \"S231:value\": {value} }} ] }}"
        )
        .as_bytes(),
    )
    .expect("parses")
}

#[test]
fn an_array_evaluation_is_kept_only_at_the_count_the_element_budget_charged() {
    // `fill(1.0, 1000000)` read by an owner whose shape holds 1 element: the evaluation is the
    // owner's, refused by its shape, and nothing is kept, so a node the document declares at one
    // element cannot keep a million. Read at the count it builds, it is kept once.
    let doc = owned(2, "\"fill(1.0, 1000000)\"");
    let by_id: HashMap<&str, &Node> = doc.graph.iter().map(|n| (n.id.as_str(), n)).collect();
    let (node, value) = (
        by_id["urn:q"],
        doc.graph[2].value.as_ref().expect("a value"),
    );
    let CxfValue::Expr(text) = value else {
        panic!("an expression")
    };
    let mut groundings = DocumentGroundings::of(&doc.graph, &by_id);
    for _ in 0..3 {
        let evaluated = groundings.evaluate_array(node, value, text, 1, &ParamScope::empty());
        assert!(matches!(&*evaluated, Ok(oce_expr::EvalResult::Array(a)) if a.len() == 1_000_000));
    }
    assert_eq!(groundings.retained(), Retained::default());
    let evaluated = groundings.evaluate_array(node, value, text, 1_000_000, &ParamScope::empty());
    assert!(evaluated.is_ok());
    assert_eq!(
        groundings.retained(),
        Retained {
            arrays: 1,
            array_elements: 1_000_000,
            ..Retained::default()
        }
    );
    // An array that reads a name, or fails, is not kept either.
    let doc = owned(2, "\"fill(k, 2)\"");
    let by_id: HashMap<&str, &Node> = doc.graph.iter().map(|n| (n.id.as_str(), n)).collect();
    let (node, value) = (
        by_id["urn:q"],
        doc.graph[2].value.as_ref().expect("a value"),
    );
    let CxfValue::Expr(text) = value else {
        panic!("an expression")
    };
    let mut groundings = DocumentGroundings::of(&doc.graph, &by_id);
    assert!(
        groundings
            .evaluate_array(node, value, text, 2, &ParamScope::empty())
            .is_err()
    );
    assert_eq!(groundings.retained(), Retained::default());
}

#[test]
fn a_kept_string_is_the_literal_and_a_kept_error_quotes_at_most_its_text() {
    // What the scalar cache holds is built from the value's own text: a string result is its
    // literal, shared with every table, and an error is bounded by the text it quotes.
    let text = "y".repeat(60_000);
    let doc = owned(2, &format!("\"\\\"{text}\\\"\""));
    let by_id: HashMap<&str, &Node> = doc.graph.iter().map(|n| (n.id.as_str(), n)).collect();
    let (node, value) = (
        by_id["urn:q"],
        doc.graph[2].value.as_ref().expect("a value"),
    );
    let mut groundings = DocumentGroundings::of(&doc.graph, &by_id);
    let first = match &*groundings.ground(node, value, &ParamScope::empty()) {
        Ok(Value::String(text)) => text.clone(),
        other => panic!("a string, not {other:?}"),
    };
    let again = match &*groundings.ground(node, value, &ParamScope::empty()) {
        Ok(Value::String(text)) => text.clone(),
        other => panic!("a string, not {other:?}"),
    };
    assert!(std::sync::Arc::ptr_eq(&first, &again));
    let retained = groundings.retained();
    assert_eq!((retained.scalars, retained.string_bytes), (1, 60_000));
    // A typed literal that does not parse: its error quotes the lexical form once.
    let lexical = "x".repeat(60_000);
    let doc = owned(
        2,
        &format!(
            "{{ \"@value\": \"{lexical}\", \"@type\": \"http://www.w3.org/2001/XMLSchema#double\" }}"
        ),
    );
    let by_id: HashMap<&str, &Node> = doc.graph.iter().map(|n| (n.id.as_str(), n)).collect();
    let (node, value) = (
        by_id["urn:q"],
        doc.graph[2].value.as_ref().expect("a value"),
    );
    let mut groundings = DocumentGroundings::of(&doc.graph, &by_id);
    let error = match &*groundings.ground(node, value, &ParamScope::empty()) {
        Err(error) => error.to_string(),
        Ok(other) => panic!("refused, not {other:?}"),
    };
    assert!(error.len() <= lexical.len() + 256, "{} bytes", error.len());
    assert_eq!(groundings.retained().scalars, 1);
    // A node no second owner lists keeps nothing at all.
    let doc = owned(1, &format!("\"\\\"{text}\\\"\""));
    let by_id: HashMap<&str, &Node> = doc.graph.iter().map(|n| (n.id.as_str(), n)).collect();
    let (node, value) = (
        by_id["urn:q"],
        doc.graph[1].value.as_ref().expect("a value"),
    );
    let mut groundings = DocumentGroundings::of(&doc.graph, &by_id);
    assert!(groundings.ground(node, value, &ParamScope::empty()).is_ok());
    assert_eq!(groundings.retained(), Retained::default());
}

/// The fuel the expression `text` spends from a fresh document budget before it is refused, and
/// the budget its refusal names.
fn refused_after(text: &str) -> (u64, u64) {
    oce_expr::with_eval_budget(MAX_DOCUMENT_EVAL_FUEL, || {
        let before = oce_expr::eval_fuel_remaining().expect("metered");
        let Err(oce_expr::ExprError::ExpressionTooExpensive { limit }) =
            oce_expr::eval_str_finite(text, &ParamScope::empty())
        else {
            panic!("{text} is refused for its cost")
        };
        (
            before - oce_expr::eval_fuel_remaining().expect("metered"),
            limit,
        )
    })
}

#[test]
fn a_refusal_for_the_evaluation_budget_is_never_handed_to_another_owner() {
    // A value refused at one evaluation's cap (`MAX_EVAL_FUEL`), after spending `spent`: an owner
    // with more than the cap left meets the same cap, but an owner with fewer units left than the
    // cap and at least `spent` meets the document's budget, and is refused naming that one.
    // Leaf 0's own `k` spends just enough that the fourth leaf is that owner; each leaf must be
    // refused naming the budget its own grounding meets, shared or not: the cap for the first
    // three, the document's for the fourth and fifth. (Keeping the first refusal for later owners
    // would name the cap for the fourth too.)
    let term = "sum(fill(1.0, 1048576))";
    let value = [term; 8].join(" + ");
    let (spent, limit) = refused_after(&value);
    assert_eq!(limit, oce_expr::MAX_EVAL_FUEL);
    let filler = |n: u64| format!("sum(fill(1.0, 1000000)) + sum(fill(1.0, {n}))");
    let (base, step) = (
        fuel_of(&filler(1)),
        fuel_of(&filler(1_001)) - fuel_of(&filler(1)),
    );
    let target = MAX_DOCUMENT_EVAL_FUEL - 3 * spent - (spent + oce_expr::MAX_EVAL_FUEL) / 2;
    let n = 1 + (target - base) * 1_000 / step;
    let fourth_left = MAX_DOCUMENT_EVAL_FUEL - fuel_of(&filler(n)) - 3 * spent;
    assert!(
        (spent..oce_expr::MAX_EVAL_FUEL).contains(&fourth_left),
        "the fourth leaf has {fourth_left} units left"
    );
    let with_filler = |bytes: Vec<u8>| {
        let from = "{ \"@id\": \"urn:c0.k\", \"S231:value\": 1.0 }";
        let to = format!(
            "{{ \"@id\": \"urn:c0.k\", \"S231:value\": \"{}\" }}",
            filler(n)
        );
        let text = String::from_utf8(bytes).expect("utf-8");
        assert_eq!(text.matches(from).count(), 1);
        text.replacen(from, &to, 1).into_bytes()
    };
    let quoted = format!("\"{value}\"");
    let shared_outcome = outcome(&with_filler(shared(5, &quoted)), 5);
    assert_eq!(
        shared_outcome,
        outcome(&with_filler(separate(5, &quoted)), 5)
    );
    let refusals = &shared_outcome.1;
    assert_eq!(
        refusals
            .matches(&format!("({} units)", oce_expr::MAX_EVAL_FUEL))
            .count(),
        3
    );
    assert_eq!(
        refusals
            .matches(&format!("({MAX_DOCUMENT_EVAL_FUEL} units)"))
            .count(),
        2
    );
}
