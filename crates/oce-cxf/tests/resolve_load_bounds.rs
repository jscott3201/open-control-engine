//! Resolver-level hostile-input bounds (issue #365): the single-parent `containsBlock` rule
//! (`shared-instance`), the per-document array-element budget (`resource-limit-exceeded`), and
//! linear-cost own-declaration chains. The full-pipeline allocation measurements live in
//! `oce-api/tests/hostile_load_bounds.rs`; these pin the resolver's exact diagnostics and that an
//! accepted document's model is unaffected.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as Json, json};

const CONSTANT: &str = "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant";

fn import(doc: &Json) -> Result<ModelGraph, Vec<Diagnostic>> {
    let bytes = serde_json::to_vec(doc).expect("serialize doc");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Ok((graph, _)) => Ok(graph),
        Err(CxfError::Validation(diags)) => Err(diags),
        Err(other) => panic!("expected a validation outcome, got {other:?}"),
    }
}

fn errors(doc: &Json) -> Vec<(DiagCode, Option<String>)> {
    import(doc)
        .expect_err("the document must be refused")
        .into_iter()
        .filter(Diagnostic::is_error)
        .map(|d| (d.code, d.subject.as_deref().map(str::to_owned)))
        .collect()
}

fn id(local: &str) -> String {
    format!("http://example.org#{local}")
}

fn refs(locals: &[&str]) -> Json {
    Json::Array(locals.iter().map(|l| json!({ "@id": id(l) })).collect())
}

/// A constant leaf `local` with output `local.y` and scalar `k`, plus extra parameter nodes.
fn constant(local: &str, extra: &[Json]) -> Vec<Json> {
    let mut params = vec![json!({ "@id": id(&format!("{local}.k")) })];
    params.extend(extra.iter().map(|p| json!({ "@id": p["@id"] })));
    let mut nodes = vec![
        json!({ "@id": id(local), "@type": CONSTANT, "S231:hasParameter": params,
                "S231:hasOutput": { "@id": id(&format!("{local}.y")) } }),
        json!({ "@id": id(&format!("{local}.k")), "@type": "S231:Parameter",
                "S231:isOfDataType": { "@id": "S231:Real" }, "S231:value": 2.5 }),
        json!({ "@id": id(&format!("{local}.y")), "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" } }),
    ];
    nodes.extend(extra.iter().cloned());
    nodes
}

fn composite(local: &str, children: &[&str]) -> Json {
    json!({ "@id": id(local), "@type": "S231:Block", "S231:containsBlock": refs(children) })
}

fn doc(nodes: Vec<Json>) -> Json {
    json!({ "@context": { "S231": "http://data.ashrae.org/S231P#" }, "@graph": nodes })
}

#[test]
fn a_leaf_listed_twice_by_one_composite_is_refused_once() {
    let mut nodes = vec![composite("M", &["M.c", "M.c"])];
    nodes.extend(constant("M.c", &[]));
    assert_eq!(
        errors(&doc(nodes)),
        [(DiagCode::SharedInstance, Some(id("M.c")))]
    );
}

#[test]
fn a_composite_shared_by_two_parents_is_refused_without_a_cycle() {
    // Acyclic diamond: M contains A and B, both contain S. Only the second reach refuses.
    let mut nodes = vec![
        composite("M", &["M.a", "M.b"]),
        composite("M.a", &["M.s"]),
        composite("M.b", &["M.s"]),
        composite("M.s", &["M.s.c"]),
    ];
    nodes.extend(constant("M.s.c", &[]));
    let diags = import(&doc(nodes)).expect_err("a shared composite refuses");
    let shared: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.code == DiagCode::SharedInstance)
        .collect();
    assert_eq!(shared.len(), 1, "{diags:?}");
    assert_eq!(shared[0].subject.as_deref(), Some(id("M.s").as_str()));
    assert_eq!(
        shared[0].message,
        format!(
            "containsBlock child {} is reached a second time (listed again by {}); a CDL \
             component instance has exactly one parent, so the second reach is refused rather \
             than expanded again",
            id("M.s"),
            id("M.b")
        )
    );
}

#[test]
fn doubly_listed_chains_refuse_each_level_once_at_any_depth() {
    // The issue #365 exponential shape, up to the 64-level nesting cap.
    for depth in [1usize, 2, 10, 63] {
        let level = |i: usize| format!("M.c{i}");
        let mut nodes = vec![composite("M", &[&level(1), &level(1)])];
        for i in 1..depth {
            nodes.push(composite(&level(i), &[&level(i + 1), &level(i + 1)]));
        }
        nodes.push(composite(&level(depth), &["M.leaf", "M.leaf"]));
        nodes.extend(constant("M.leaf", &[]));
        let shared = errors(&doc(nodes))
            .into_iter()
            .filter(|(code, _)| *code == DiagCode::SharedInstance)
            .count();
        assert_eq!(shared, depth + 1, "depth {depth}");
    }
}

#[test]
fn distinct_children_in_one_composite_still_lower_in_order() {
    // Control: the single-parent rule must not touch ordinary documents.
    let mut nodes = vec![
        composite("M", &["M.a", "M.b"]),
        composite("M.a", &["M.a.c"]),
    ];
    nodes.extend(constant("M.a.c", &[]));
    nodes.extend(constant("M.b", &[]));
    let graph = import(&doc(nodes)).expect("distinct children load");
    let order: Vec<Option<&str>> = graph
        .blocks
        .iter()
        .map(|b| b.instance_iri.as_deref())
        .collect();
    assert_eq!(
        order,
        [Some(id("M.a.c").as_str()), Some(id("M.b").as_str())]
    );
}

fn broadcast(owner: &str, name: &str, len: usize) -> Json {
    json!({ "@id": id(&format!("{owner}.{name}")), "S231:label": format!("{name}[{len}]"),
            "S231:isArray": true, "S231:numberDimensions": 1,
            "S231:sizeOfDimensions": format!("({len})"), "S231:value": [ 1.0 ] })
}

#[test]
fn the_array_budget_spans_every_instance_of_the_document() {
    // Two leaves, 2^19 elements each, exactly fill the 2^20 document budget; one element more
    // on the second leaf refuses on that leaf's parameter only.
    let half = 1usize << 19;
    let build = |second: usize| {
        let mut nodes = vec![composite("M", &["M.a", "M.b"])];
        nodes.extend(constant("M.a", &[broadcast("M.a", "p", half)]));
        nodes.extend(constant("M.b", &[broadcast("M.b", "p", second)]));
        doc(nodes)
    };
    let graph = import(&build(half)).expect("exactly the budget loads");
    let elements: usize = graph
        .blocks
        .iter()
        .map(|b| {
            b.params
                .values
                .iter()
                .filter(|(n, _)| n.starts_with("p_"))
                .count()
        })
        .sum();
    assert_eq!(elements, 1 << 20);
    assert_eq!(
        errors(&build(half + 1)),
        [(DiagCode::ResourceLimitExceeded, Some(id("M.b.p")))]
    );
}

#[test]
fn an_exhausted_budget_refuses_the_next_element_but_admits_an_empty_array() {
    // `big` consumes the whole budget; `over` (one element) is refused before minting; an
    // empty array after it still fits (it needs nothing), so it raises no diagnostic.
    let mut zero = broadcast("M.a", "zero", 0);
    zero["S231:value"] = json!([]);
    let mut nodes = vec![composite("M", &["M.a"])];
    nodes.extend(constant(
        "M.a",
        &[
            broadcast("M.a", "big", 1 << 20),
            broadcast("M.a", "over", 1),
            zero,
        ],
    ));
    let diags = import(&doc(nodes)).expect_err("the budget is exhausted");
    let refused: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].code, DiagCode::ResourceLimitExceeded);
    assert_eq!(refused[0].subject.as_deref(), Some(id("M.a.over").as_str()));
}

#[test]
fn a_long_forward_reference_chain_grounds_in_dependency_order() {
    // 4096 composite parameters, each declared BEFORE the one it references: the topological
    // order must evaluate them last-to-first. The heap-driven sort keeps this linearithmic.
    let n = 4096usize;
    let mut params = Vec::new();
    let mut nodes = Vec::new();
    for i in 0..n {
        let pid = id(&format!("M.q{i}"));
        params.push(json!({ "@id": pid }));
        let value = if i + 1 == n {
            json!(1.0)
        } else {
            json!(format!("q{} + 1.0", i + 1))
        };
        nodes.push(json!({ "@id": pid, "@type": "S231:Parameter",
                           "S231:isOfDataType": { "@id": "S231:Real" }, "S231:value": value }));
    }
    let leaf_param = json!({ "@id": id("M.c.top"), "@type": "S231:Parameter",
                             "S231:isOfDataType": { "@id": "S231:Real" }, "S231:value": "q0" });
    nodes.push(
        json!({ "@id": id("M"), "@type": "S231:Block", "S231:hasParameter": params,
                       "S231:containsBlock": refs(&["M.c"]) }),
    );
    nodes.extend(constant("M.c", &[leaf_param]));
    let graph = import(&doc(nodes)).expect("the chain grounds");
    let top = graph.blocks[0]
        .params
        .values
        .iter()
        .find(|(name, _)| name.as_ref() == "top")
        .map(|(_, v)| v.clone())
        .expect("leaf parameter grounded");
    assert!(top.bit_eq(&Value::Real(n as f64)), "{top:?}");
}

/// A constant leaf carrying `bindings` extra Integer parameters with the given expression text.
fn costly_leaf(bindings: &[&str]) -> Json {
    let extra: Vec<Json> = bindings
        .iter()
        .enumerate()
        .map(|(i, text)| {
            json!({ "@id": id(&format!("M.c.e{i}")), "@type": "S231:Parameter",
                    "S231:isOfDataType": { "@id": "S231:Integer" }, "S231:value": text })
        })
        .collect();
    let mut nodes = vec![composite("M", &["M.c"])];
    nodes.extend(constant("M.c", &extra));
    doc(nodes)
}

#[test]
fn the_document_evaluation_budget_admits_exactly_its_limit_and_refuses_one_unit_more() {
    // Each `sum(fill(1, 1048574))` costs exactly 2^21 units (pinned in oce-expr's fuel tests),
    // so 32 of them spend exactly MAX_DOCUMENT_EVAL_FUEL = 2^26.
    assert_eq!(oce_cxf::MAX_DOCUMENT_EVAL_FUEL, 1 << 26);
    let full = vec!["sum(fill(1, 1048574))"; 32];
    let graph = import(&costly_leaf(&full)).expect("exactly the document budget loads");
    assert_eq!(
        graph.blocks[0].params.values.len(),
        33,
        "k plus 32 grounded bindings"
    );

    // One more unit — even the cheapest binding, a literal expression — is refused, naming
    // the document budget, and only that binding is.
    let mut over = full.clone();
    over.push("1 + 0");
    let diags = import(&costly_leaf(&over)).expect_err("one unit past the budget refuses");
    let errors: Vec<&Diagnostic> = diags.iter().filter(|d| d.is_error()).collect();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, DiagCode::GroundingFailed);
    assert_eq!(errors[0].subject.as_deref(), Some(id("M.c.e32").as_str()));
    assert!(
        errors[0].message.contains("work budget (67108864 units)"),
        "{errors:?}"
    );
}

#[test]
fn every_import_starts_with_a_fresh_document_budget() {
    // The budget is per import, not per thread: a second import after an exhausted one loads.
    let full = vec!["sum(fill(1, 1048574))"; 32];
    import(&costly_leaf(&full)).expect("first import spends the whole budget");
    import(&costly_leaf(&full)).expect("a later import gets its own budget");
}
