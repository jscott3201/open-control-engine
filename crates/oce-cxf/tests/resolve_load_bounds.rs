//! Resolver-level hostile-input bounds (issue #365): the single-parent `containsBlock` rule
//! (`shared-instance`), the per-document array-element budget and its budget of element-path
//! bytes (`resource-limit-exceeded`, issue #710), and linear-cost own-declaration chains. The full-pipeline allocation measurements live in
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
    json!({ "@context": { "S231": "http://data.ashrae.org/S231#" }, "@graph": nodes })
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

/// The per-document budget of element-path bytes (`oce-cxf`'s `MAX_DOCUMENT_ARRAY_PATH_BYTES`),
/// restated so this test fails if the budget moves without review.
const PATH_BUDGET: usize = 64 * 1024 * 1024;

/// The bytes of the paths `<instance IRI of owner>.<base>_<i>` for `i` from 1 to `len` together,
/// added up path by path: what the facade's parameter table keys each element by.
fn path_bytes(owner: &str, base: &str, len: usize) -> usize {
    let prefix = id(owner).len() + 1 + base.len() + 1;
    (1..=len).map(|i| prefix + i.to_string().len()).sum()
}

/// The element names `base` mints on `block`, in table order, and their values.
fn minted<'g>(graph: &'g ModelGraph, block: &str, base: &str) -> Vec<(&'g str, &'g Value)> {
    let prefix = format!("{base}_");
    graph
        .blocks
        .iter()
        .find(|b| b.instance_iri.as_deref() == Some(id(block).as_str()))
        .expect("the block")
        .params
        .values
        .iter()
        .filter(|(name, _)| name.starts_with(&prefix))
        .map(|(name, value)| (name.as_ref(), value))
        .collect()
}

#[test]
fn the_path_budget_spans_every_instance_and_admits_exactly_its_limit() {
    // Two 1,024-element arrays of long names on two leaves, the second leaf under a 4 KB
    // instance IRI, and a one-element array on it whose path takes what is left of the budget to
    // the byte. The second leaf's IRI and names are multibyte, so a count of characters instead
    // of bytes would admit the document one byte past the budget. One byte more refuses the last
    // array, and only it, naming the bytes it needs and the bytes already used.
    let leaf = format!("M.{}", "é".repeat(2_000));
    let (a, b) = ("a".repeat(32_000), "β".repeat(14_737));
    let used = path_bytes("M.a", &a, 1_024) + path_bytes(&leaf, &b, 1_024);
    // What the last path leaves for its name: the IRI, the `.` and `_1` are fixed.
    let rest = PATH_BUDGET - used - (id(&leaf).len() + 1 + 2);
    assert_eq!(rest, 5_358);
    let build = |ascii: usize| {
        let c = format!("{}{}", "γ".repeat(1_339), "c".repeat(ascii));
        let mut nodes = vec![composite("M", &["M.a", &leaf])];
        nodes.extend(constant("M.a", &[broadcast("M.a", &a, 1_024)]));
        nodes.extend(constant(
            &leaf,
            &[broadcast(&leaf, &b, 1_024), broadcast(&leaf, &c, 1)],
        ));
        (doc(nodes), c)
    };

    let (exact, c) = build(rest - 2 * 1_339);
    assert_eq!(c.len(), rest);
    let graph = import(&exact).expect("paths totalling exactly the budget load");
    let mut total = 0;
    for (block, base, len) in [("M.a", &a, 1_024), (&leaf, &b, 1_024), (&leaf, &c, 1)] {
        let names = minted(&graph, block, base);
        let expected: Vec<String> = (1..=len).map(|i| format!("{base}_{i}")).collect();
        assert!(
            names
                .iter()
                .map(|(name, _)| *name)
                .eq(expected.iter().map(String::as_str)),
            "{block}: every element is minted, in order"
        );
        assert!(
            names
                .iter()
                .all(|(_, value)| value.bit_eq(&Value::Real(1.0)))
        );
        total += names
            .iter()
            .map(|(name, _)| id(block).len() + 1 + name.len())
            .sum::<usize>();
    }
    assert_eq!(total, PATH_BUDGET);

    let (over, c) = build(rest - 2 * 1_339 + 1);
    let diags = import(&over).expect_err("one byte past the budget refuses");
    let refused: Vec<&Diagnostic> = diags.iter().filter(|d| d.is_error()).collect();
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].code, DiagCode::ResourceLimitExceeded);
    assert_eq!(
        refused[0].subject.as_deref(),
        Some(id(&format!("{leaf}.{c}")).as_str())
    );
    assert_eq!(
        refused[0].message,
        format!(
            "array parameter's 1 element path(s) (instance IRI, `.`, element name) need {} \
             byte(s) but the element paths of the document's array parameters already use {used} \
             of the 67108864-byte per-document budget",
            PATH_BUDGET - used + 1
        )
    );
}

#[test]
fn a_parameter_refused_for_its_paths_draws_no_element() {
    // A 1 KiB name over 65,536 elements needs 68,998,302 bytes of paths, past the path budget on
    // its own. The array after it takes every element the document has left with that one's
    // 65,536 included, so it fits only if the refused one drew none: the one refusal is the long
    // name's.
    let long = "k".repeat(1_024);
    assert_eq!(path_bytes("M.a", &long, 65_536), 68_998_302);
    let rest = (1 << 20) - 65_536 + 1;
    let mut nodes = vec![composite("M", &["M.a"])];
    nodes.extend(constant(
        "M.a",
        &[broadcast("M.a", &long, 65_536), broadcast("M.a", "p", rest)],
    ));
    let subject = id(&format!("M.a.{long}"));
    assert_eq!(
        errors(&doc(nodes)),
        [(DiagCode::ResourceLimitExceeded, Some(subject))]
    );
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
