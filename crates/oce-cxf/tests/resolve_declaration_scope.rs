//! Whole-document pins for the order-independent own-declaration scope (issue #240):
//! the six behavior probes that established the gap, graduated to their ruled post-states,
//! plus the permutation-invariance pins over loading documents, refusing documents, and
//! conditional-guard specialization.
//!
//! Permutation pin over LOADING documents — criterion, not enumeration: every accepted
//! `composite_contract` fixture with two or more own declarations in any composite qualifies,
//! plus the shadowed-forward-reference probe document below. At this head the qualifying
//! fixtures are `accepted/minimal_nested.jsonld` (root: `kBase`, `kTop`),
//! `accepted/forward_sibling_reference.jsonld` (root: `kDerived`, `kBase` + constant
//! `cShift`), and `accepted/string_literal_sibling_name.jsonld` (root: `a`, `b`);
//! `two_level_nesting` carries one declaration per composite,
//! `leaf_identity_parameter_modification`'s composite declares only `samplePeriod`, and the
//! declaring nodes of `registered_leaf_carveout`, `leaf_array_parameter_conditional_member`,
//! and `leaf_identity_parameter_modification` are registered leaves, whose member chains are a
//! different (order-sensitive, fenced) level.
//! The qualification scan below is generic, so a future fixture qualifies itself.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as Json, json};

// ---- import + render helpers (render mirrors composite_contract_corpus.rs) -------------------

fn import_json(doc: &Json) -> Result<(ModelGraph, Vec<Diagnostic>), Vec<Diagnostic>> {
    let bytes = serde_json::to_vec(doc).expect("serializable test document");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Ok((graph, report)) => Ok((graph, report.diagnostics)),
        Err(CxfError::Validation(diags)) => Err(diags),
        Err(other) => panic!("unexpected non-validation import failure: {other:?}"),
    }
}

fn import_clean(doc: &Json) -> ModelGraph {
    match import_json(doc) {
        Ok((graph, diags)) => {
            assert!(
                diags.is_empty(),
                "expected a warning-free load, got {diags:?}"
            );
            graph
        }
        Err(diags) => panic!("expected a clean load, got a refusal: {diags:#?}"),
    }
}

fn render(graph: &ModelGraph) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "blocks: {}", graph.blocks.len());
    for block in &graph.blocks {
        let _ = writeln!(
            out,
            "  B{} decl={} class={} instance_iri={:?}",
            block.id.0,
            block.decl_order,
            block.class_iri,
            block.instance_iri.as_deref()
        );
        let _ = writeln!(
            out,
            "    inputs={:?} outputs={:?}",
            block.inputs.iter().map(|id| id.0).collect::<Vec<_>>(),
            block.outputs.iter().map(|id| id.0).collect::<Vec<_>>()
        );
        for (name, value) in &block.params.values {
            let _ = writeln!(out, "    param {name}={}", render_value(value));
        }
    }
    let _ = writeln!(out, "connectors: {}", graph.connectors.len());
    for c in &graph.connectors {
        let _ = writeln!(
            out,
            "  C{} block=B{} dir={:?} type={:?} decl={} iri={:?}",
            c.id.0,
            c.block.0,
            c.dir,
            c.value_type,
            c.decl_order,
            c.iri.as_deref()
        );
    }
    let _ = writeln!(out, "connections: {}", graph.connections.len());
    for c in &graph.connections {
        let _ = writeln!(out, "  C{} -> C{}", c.from.0, c.to.0);
    }
    let _ = writeln!(
        out,
        "external_inputs: {:?}",
        graph
            .external_inputs
            .iter()
            .map(|id| id.0)
            .collect::<Vec<_>>()
    );
    let _ = writeln!(out, "boundary_outputs: {}", graph.boundary_outputs.len());
    for o in &graph.boundary_outputs {
        let _ = writeln!(out, "  {} <- C{}", o.iri, o.source.0);
    }
    out
}

fn render_value(value: &Value) -> String {
    match value {
        Value::Real(x) => format!("Real(0x{:016x})", x.to_bits()),
        Value::Integer(x) => format!("Integer({x})"),
        Value::Boolean(x) => format!("Boolean({x})"),
        Value::String(x) => format!("String({x:?})"),
        Value::Enum { class, ordinal } => format!("Enum(class={},ordinal={ordinal})", class.0),
    }
}

fn leaf_param<'a>(graph: &'a ModelGraph, instance_suffix: &str, param: &str) -> &'a Value {
    let block = graph
        .blocks
        .iter()
        .find(|b| {
            b.instance_iri
                .as_deref()
                .is_some_and(|iri| iri.ends_with(instance_suffix))
        })
        .unwrap_or_else(|| panic!("missing leaf {instance_suffix}"));
    block
        .params
        .values
        .iter()
        .find_map(|(name, value)| (name.as_ref() == param).then_some(value))
        .unwrap_or_else(|| panic!("{instance_suffix} must carry param {param}"))
}

// ---- JSON permutation helpers ----------------------------------------------------------------

fn corpus_fixture(rel: &str) -> Json {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/composite_contract")
        .join(rel);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fixture {} must be readable: {e}", path.display()));
    serde_json::from_str(&text).expect("fixture parses as JSON")
}

/// Apply `transform` to the named node's `S231:hasParameter`/`S231:hasConstant` arrays.
fn permute_declarations(doc: &Json, node_id: &str, transform: fn(&mut Vec<Json>)) -> Json {
    let mut permuted = doc.clone();
    let graph = permuted["@graph"].as_array_mut().expect("@graph array");
    let node = graph
        .iter_mut()
        .find(|node| node["@id"] == node_id)
        .unwrap_or_else(|| panic!("no node {node_id}"));
    for key in ["S231:hasParameter", "S231:hasConstant"] {
        if let Some(array) = node.get_mut(key).and_then(Json::as_array_mut) {
            transform(array);
        }
    }
    permuted
}

/// The declaring composite nodes of a fixture with two or more own declarations: nodes whose
/// `containsBlock` is non-empty, whose `@type` does not resolve to a registered leaf class
/// (Rule 1, as published), and whose chained declaration count is at least two.
fn qualifying_composites(doc: &Json) -> Vec<String> {
    let registered = |type_token: &str| {
        let fragment = type_token.rsplit('#').next().unwrap_or(type_token);
        let class_path = fragment
            .strip_prefix("Buildings.Controls.OBC.")
            .unwrap_or(fragment);
        oce_blocks::lookup(class_path).is_some()
    };
    let count = |node: &Json, key: &str| match node.get(key) {
        Some(Json::Array(items)) => items.len(),
        Some(_) => 1,
        None => 0,
    };
    doc["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .filter(|node| count(node, "S231:containsBlock") > 0)
        .filter(|node| !node["@type"].as_str().is_some_and(registered))
        .filter(|node| count(node, "S231:hasParameter") + count(node, "S231:hasConstant") >= 2)
        .map(|node| node["@id"].as_str().expect("node @id").to_owned())
        .collect()
}

/// The participant IRI set of one `composite/declaration-cycle` message.
fn cycle_participants(diag: &Diagnostic) -> BTreeSet<String> {
    let (_, list) = diag
        .message
        .split_once("references: ")
        .unwrap_or_else(|| panic!("not a declaration-cycle message: {diag:?}"));
    list.split(" -> ").map(str::to_owned).collect()
}

// ---- permutation pin (i): LOADING documents --------------------------------------------------

#[test]
fn qualifying_accepted_fixtures_load_byte_identically_under_declaration_permutations() {
    let mut qualified = 0usize;
    for fixture in [
        "accepted/forward_sibling_reference.jsonld",
        "accepted/leaf_array_parameter_conditional_member.jsonld",
        "accepted/leaf_identity_parameter_modification.jsonld",
        "accepted/minimal_nested.jsonld",
        "accepted/registered_leaf_carveout.jsonld",
        "accepted/string_literal_sibling_name.jsonld",
        "accepted/two_level_nesting.jsonld",
    ] {
        let doc = corpus_fixture(fixture);
        let baseline = render(&import_clean(&doc));
        for composite in qualifying_composites(&doc) {
            qualified += 1;
            let reversed = permute_declarations(&doc, &composite, |array| array.reverse());
            assert_eq!(
                render(&import_clean(&reversed)),
                baseline,
                "{fixture}: permuting {composite}'s declaration arrays must not change the \
                 rendered ModelGraph"
            );
            let rotated = permute_declarations(&doc, &composite, |array| array.rotate_left(1));
            assert_eq!(
                render(&import_clean(&rotated)),
                baseline,
                "{fixture}: rotating {composite}'s declaration arrays must not change the \
                 rendered ModelGraph"
            );
        }
    }
    assert_eq!(
        qualified, 3,
        "the qualification criterion must select exactly the three composites enumerated in the \
         module doc; a new qualifying fixture belongs in the fixture list above"
    );
}

// ---- permutation pin (ii): REFUSING documents ------------------------------------------------

#[test]
fn refusing_fixtures_keep_their_rule_ids_and_participant_sets_under_every_permutation() {
    // declaration_cycle: both permutations of the root's two-parameter array refuse with the
    // same rule id and the same participant SET; the subject relocates to whichever
    // participant is earliest in the permuted chained order.
    let doc = corpus_fixture("rejected/declaration_cycle.jsonld");
    let expected: BTreeSet<String> = ["http://example.org#M.a", "http://example.org#M.b"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    for (transform, subject) in [
        (
            (|_: &mut Vec<Json>| {}) as fn(&mut Vec<Json>),
            "http://example.org#M.a",
        ),
        (
            |array: &mut Vec<Json>| array.reverse(),
            "http://example.org#M.b",
        ),
    ] {
        let diags = import_json(&permute_declarations(
            &doc,
            "http://example.org#M",
            transform,
        ))
        .expect_err("the cycle refuses under every permutation");
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert_eq!(diags[0].code, DiagCode::MalformedDocument);
        assert_eq!(diags[0].subject.as_deref(), Some(subject));
        assert!(
            diags[0]
                .message
                .starts_with("composite/declaration-cycle: ")
        );
        assert_eq!(cycle_participants(&diags[0]), expected);
    }

    // self_reference and duplicate_declaration declare one-element arrays — the identity is
    // their only permutation — so re-assert their pinned refusals through this driver too.
    for (fixture, rule_prefix, subject) in [
        (
            "rejected/self_reference.jsonld",
            "composite/declaration-cycle: ",
            "http://example.org#M.sub.x",
        ),
        (
            "rejected/duplicate_declaration.jsonld",
            "composite/duplicate-declaration: ",
            "http://example.org#M.settings.k",
        ),
    ] {
        let diags = import_json(&corpus_fixture(fixture)).expect_err("refuses");
        assert_eq!(diags.len(), 1, "{fixture}: {diags:#?}");
        assert!(diags[0].message.starts_with(rule_prefix), "{fixture}");
        assert_eq!(diags[0].subject.as_deref(), Some(subject), "{fixture}");
    }
}

// ---- permutation pin (iii): specialization ---------------------------------------------------

/// The round-1 proof document: a COMPOSITE parent whose parameters are `flag=false` and
/// `have_x="flag"` (a sibling reference), one unconditional Constant leaf, and one conditional
/// Constant CONTAINED BLOCK guarded by `have_x`. Before the shared mechanism, `[have_x, flag]`
/// refused with three diagnostics while `[flag, have_x]` imported clean.
fn guarded_child_doc(param_order: [&str; 2]) -> Json {
    let refs: Vec<Json> = param_order
        .iter()
        .map(|name| json!({ "@id": format!("http://example.org#R.{name}") }))
        .collect();
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": [
            { "@id": "http://example.org#R", "@type": "S231:Block", "S231:label": "R",
              "S231:hasParameter": refs,
              "S231:containsBlock": [
                  { "@id": "http://example.org#R.con" },
                  { "@id": "http://example.org#R.condblk" } ],
              "S231:hasOutput": { "@id": "http://example.org#R.yOut" } },
            { "@id": "http://example.org#R.flag", "S231:label": "flag",
              "S231:value": false },
            { "@id": "http://example.org#R.have_x", "S231:label": "have_x",
              "S231:value": "flag" },
            { "@id": "http://example.org#R.con",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:label": "con",
              "S231:hasParameter": { "@id": "http://example.org#R.con.k" },
              "S231:hasOutput": { "@id": "http://example.org#R.con.y" } },
            { "@id": "http://example.org#R.con.k", "S231:label": "k", "S231:value": 1.0 },
            { "@id": "http://example.org#R.con.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" },
              "S231:isConnectedTo": { "@id": "http://example.org#R.yOut" } },
            { "@id": "http://example.org#R.condblk",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:label": "condblk",
              "S231:isConditionalComponent": true,
              "S231:conditionalExpression": "have_x",
              "S231:hasParameter": { "@id": "http://example.org#R.condblk.k" },
              "S231:hasOutput": { "@id": "http://example.org#R.condblk.y" } },
            { "@id": "http://example.org#R.condblk.k", "S231:label": "k", "S231:value": 2.0 },
            { "@id": "http://example.org#R.condblk.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } },
            { "@id": "http://example.org#R.yOut", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    })
}

#[test]
fn guard_specialization_prunes_the_conditional_child_identically_under_both_orders() {
    let baseline = render(&import_clean(&guarded_child_doc(["flag", "have_x"])));
    let reversed = render(&import_clean(&guarded_child_doc(["have_x", "flag"])));
    assert_eq!(
        baseline, reversed,
        "guard decisions must be order-independent"
    );
    let graph = import_clean(&guarded_child_doc(["have_x", "flag"]));
    assert_eq!(graph.blocks.len(), 1, "the false-guarded child is pruned");
}

// ---- probe graduations -----------------------------------------------------------------------

/// Probe (a): enclosing `k=1.0`, inner composite `[g="k", k=9.0]`. The own `k` shadows the
/// enclosing binding for the sibling RHS, so `g` grounds to 9.0 under both orders — the
/// original repro grounded `g` to 1.0 or 9.0 depending on array order.
fn shadowed_forward_reference_doc(param_order: [&str; 2]) -> Json {
    let refs: Vec<Json> = param_order
        .iter()
        .map(|name| json!({ "@id": format!("http://example.org#M.sub.{name}") }))
        .collect();
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": [
            { "@id": "http://example.org#M", "@type": "S231:Block",
              "S231:hasParameter": { "@id": "http://example.org#M.k" },
              "S231:containsBlock": [ { "@id": "http://example.org#M.sub" } ] },
            { "@id": "http://example.org#M.k", "S231:value": 1.0 },
            { "@id": "http://example.org#M.sub",
              "@type": "http://example.org#Vendor.Sequences.Inner",
              "S231:hasParameter": refs,
              "S231:containsBlock": [ { "@id": "http://example.org#M.sub.con" } ] },
            { "@id": "http://example.org#M.sub.g", "S231:value": "k" },
            { "@id": "http://example.org#M.sub.k", "S231:value": 9.0 },
            { "@id": "http://example.org#M.sub.con",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:hasParameter": { "@id": "http://example.org#M.sub.con.k" },
              "S231:hasOutput": { "@id": "http://example.org#M.sub.con.y" } },
            { "@id": "http://example.org#M.sub.con.k", "S231:value": "g" },
            { "@id": "http://example.org#M.sub.con.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    })
}

#[test]
fn own_declaration_shadows_enclosing_binding_for_sibling_references_in_both_orders() {
    // The §5.1 permutation obligation for the probe-(a) document: byte-identical rendered
    // ModelGraph AND identical diagnostic vectors across the orders (`import_clean` pins both
    // vectors to empty), plus the graduated value itself.
    let mut renders = Vec::new();
    for order in [["g", "k"], ["k", "g"]] {
        let graph = import_clean(&shadowed_forward_reference_doc(order));
        let k = leaf_param(&graph, ".sub.con", "k");
        assert!(
            k.bit_eq(&Value::Real(9.0)),
            "own k=9.0 must win over enclosing k=1.0 under order {order:?}, got {k:?}"
        );
        renders.push(render(&graph));
    }
    assert_eq!(
        renders[0], renders[1],
        "the rendered ModelGraph must be byte-identical across the two declaration orders"
    );
}

/// Probe (b): a reference to a name bound nowhere stays a loud grounding failure, and a
/// reference to a cycle-refused sibling fails the same way — the cycle member is absent from
/// the scope, it never falls through to anything.
#[test]
fn unknown_and_cycle_refused_identifiers_stay_loud_grounding_failures() {
    let doc = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": [
            { "@id": "http://example.org#M", "@type": "S231:Block",
              "S231:hasParameter": [
                  { "@id": "http://example.org#M.p" },
                  { "@id": "http://example.org#M.a" },
                  { "@id": "http://example.org#M.c" } ],
              "S231:containsBlock": [ { "@id": "http://example.org#M.con" } ] },
            { "@id": "http://example.org#M.p", "S231:value": "nosuch" },
            { "@id": "http://example.org#M.a", "S231:value": "a" },
            { "@id": "http://example.org#M.c", "S231:value": "a + 1.0" },
            { "@id": "http://example.org#M.con",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:hasParameter": { "@id": "http://example.org#M.con.k" },
              "S231:hasOutput": { "@id": "http://example.org#M.con.y" } },
            { "@id": "http://example.org#M.con.k", "S231:value": 1.0 },
            { "@id": "http://example.org#M.con.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    });
    let diags = import_json(&doc).expect_err("both failures refuse the import");
    let rendered: Vec<(DiagCode, Option<&str>, &str)> = diags
        .iter()
        .map(|d| (d.code, d.subject.as_deref(), d.message.as_str()))
        .collect();
    assert_eq!(
        rendered,
        vec![
            (
                DiagCode::MalformedDocument,
                Some("http://example.org#M.a"),
                "composite/declaration-cycle: cycle in the block's own declaration references: \
                 http://example.org#M.a -> http://example.org#M.a",
            ),
            (
                DiagCode::GroundingFailed,
                Some("http://example.org#M.c"),
                "expression binding did not ground: unknown identifier: a",
            ),
            (
                DiagCode::GroundingFailed,
                Some("http://example.org#M.p"),
                "expression binding did not ground: unknown identifier: nosuch",
            ),
        ],
        "the unknown identifier and the cycle-downstream reference both stay loud"
    );
}

/// Probes (c) and (d) graduate through the published corpus fixtures
/// (`rejected/declaration_cycle.jsonld`, `rejected/self_reference.jsonld`), whose pinned
/// participant lists live in `composite_contract_corpus.rs`; the permutation leg is the
/// refusing-documents pin above. Probe (c)'s pre-change two-loud-failures termination is
/// superseded by the single tagged refusal those pins assert.
#[test]
fn sibling_cycle_and_self_reference_refuse_through_the_corpus_fixtures() {
    for (fixture, participants) in [
        (
            "rejected/declaration_cycle.jsonld",
            vec!["http://example.org#M.a", "http://example.org#M.b"],
        ),
        (
            "rejected/self_reference.jsonld",
            vec!["http://example.org#M.sub.x"],
        ),
    ] {
        let diags = import_json(&corpus_fixture(fixture)).expect_err("refuses");
        assert_eq!(
            diags.len(),
            1,
            "{fixture}: one diagnostic per distinct cycle"
        );
        let expected: BTreeSet<String> = participants.into_iter().map(str::to_owned).collect();
        assert_eq!(cycle_participants(&diags[0]), expected, "{fixture}");
    }
}

/// A cycle-refused own name still MASKS a same-named enclosing binding: the non-cycle sibling
/// `c` referencing the refused `a` must fail loud through the untagged machinery, never
/// silently ground from the enclosing `a=1.0` (R20-2 — an own name denotes the own
/// declaration, everywhere).
#[test]
fn cycle_refused_own_name_still_masks_the_enclosing_binding() {
    let doc = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": [
            { "@id": "http://example.org#M", "@type": "S231:Block",
              "S231:hasParameter": { "@id": "http://example.org#M.a" },
              "S231:containsBlock": [ { "@id": "http://example.org#M.sub" } ] },
            { "@id": "http://example.org#M.a", "S231:value": 1.0 },
            { "@id": "http://example.org#M.sub",
              "@type": "http://example.org#Vendor.Sequences.Inner",
              "S231:hasParameter": [
                  { "@id": "http://example.org#M.sub.a" },
                  { "@id": "http://example.org#M.sub.b" },
                  { "@id": "http://example.org#M.sub.c" } ],
              "S231:containsBlock": [ { "@id": "http://example.org#M.sub.con" } ] },
            { "@id": "http://example.org#M.sub.a", "S231:value": "b" },
            { "@id": "http://example.org#M.sub.b", "S231:value": "a" },
            { "@id": "http://example.org#M.sub.c", "S231:value": "a + 1.0" },
            { "@id": "http://example.org#M.sub.con",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:hasParameter": { "@id": "http://example.org#M.sub.con.k" },
              "S231:hasOutput": { "@id": "http://example.org#M.sub.con.y" } },
            { "@id": "http://example.org#M.sub.con.k", "S231:value": 1.0 },
            { "@id": "http://example.org#M.sub.con.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    });
    let diags = import_json(&doc).expect_err("the cycle and c's loud failure refuse the import");
    let rendered: Vec<(DiagCode, Option<&str>, &str)> = diags
        .iter()
        .map(|d| (d.code, d.subject.as_deref(), d.message.as_str()))
        .collect();
    assert_eq!(
        rendered,
        vec![
            (
                DiagCode::MalformedDocument,
                Some("http://example.org#M.sub.a"),
                "composite/declaration-cycle: cycle in the block's own declaration references: \
                 http://example.org#M.sub.a -> http://example.org#M.sub.b -> \
                 http://example.org#M.sub.a",
            ),
            (
                DiagCode::GroundingFailed,
                Some("http://example.org#M.sub.c"),
                "expression binding did not ground: unknown identifier: a",
            ),
        ],
        "c must fail loud on the masked own `a` — a silent ground from the enclosing a=1.0 \
         would shrink this vector to the cycle diagnostic alone"
    );
}

/// A builtin call head is not a dependency edge: `p = "max(1.0, 2.0)"` beside a sibling
/// literally NAMED `max` must not manufacture a `p -> max` edge (oce-expr resolves call heads
/// only as builtins, never through the scope), so the document loads clean — with zero
/// diagnostics and one grounded value — under both declaration-array orders.
#[test]
fn builtin_call_head_beside_a_same_named_sibling_grounds_clean_in_both_orders() {
    for order in [["p", "max"], ["max", "p"]] {
        let refs: Vec<Json> = order
            .iter()
            .map(|name| json!({ "@id": format!("http://example.org#M.{name}") }))
            .collect();
        let doc = json!({
            "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
            "@graph": [
                { "@id": "http://example.org#M", "@type": "S231:Block",
                  "S231:hasParameter": refs,
                  "S231:containsBlock": [ { "@id": "http://example.org#M.con" } ] },
                { "@id": "http://example.org#M.p", "S231:value": "max(1.0, 2.0)" },
                { "@id": "http://example.org#M.max", "S231:value": "p" },
                { "@id": "http://example.org#M.con",
                  "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                  "S231:hasParameter": { "@id": "http://example.org#M.con.k" },
                  "S231:hasOutput": { "@id": "http://example.org#M.con.y" } },
                { "@id": "http://example.org#M.con.k", "S231:value": "p" },
                { "@id": "http://example.org#M.con.y", "@type": "S231:RealOutput",
                  "S231:isOfDataType": { "@id": "S231:Real" } }
            ]
        });
        let graph = import_clean(&doc);
        let k = leaf_param(&graph, ".con", "k");
        assert!(
            k.bit_eq(&Value::Real(2.0)),
            "p grounds through the builtin under order {order:?}, got {k:?}"
        );
    }
}

/// Probe (e): a parameter reads a later-declared sibling constant — the fixed
/// params-then-constants chain order made this loud-or-silent before; now it grounds to the
/// constant's value under both parameter-array orders.
#[test]
fn parameter_reads_sibling_constant_under_both_orders() {
    for order in [["p", "q"], ["q", "p"]] {
        let refs: Vec<Json> = order
            .iter()
            .map(|name| json!({ "@id": format!("http://example.org#M.{name}") }))
            .collect();
        let doc = json!({
            "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
            "@graph": [
                { "@id": "http://example.org#M", "@type": "S231:Block",
                  "S231:hasParameter": refs,
                  "S231:hasConstant": { "@id": "http://example.org#M.c" },
                  "S231:containsBlock": [ { "@id": "http://example.org#M.con" } ] },
                { "@id": "http://example.org#M.p", "S231:value": "c" },
                { "@id": "http://example.org#M.q", "S231:value": 1.0 },
                { "@id": "http://example.org#M.c", "S231:value": 7.0 },
                { "@id": "http://example.org#M.con",
                  "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                  "S231:hasParameter": { "@id": "http://example.org#M.con.k" },
                  "S231:hasOutput": { "@id": "http://example.org#M.con.y" } },
                { "@id": "http://example.org#M.con.k", "S231:value": "p" },
                { "@id": "http://example.org#M.con.y", "@type": "S231:RealOutput",
                  "S231:isOfDataType": { "@id": "S231:Real" } }
            ]
        });
        let graph = import_clean(&doc);
        let k = leaf_param(&graph, ".con", "k");
        assert!(
            k.bit_eq(&Value::Real(7.0)),
            "the parameter must read the sibling constant under order {order:?}, got {k:?}"
        );
    }
}

/// Probe (f): the dims path. The inner composite declares `n=3` (shadowing the enclosing
/// `n=2`) and `w="n"`; a leaf array's `sizeOfDimensions` reads `w` off the undivided
/// inherited scope. Before the rewrite, `[w, n]` read the enclosing `n=2` (clean import) and
/// `[n, w]` read the own `n=3` (refusal); now the own `n` wins under both orders and both
/// refuse on the same element-count divergence.
#[test]
fn dims_consumers_of_the_composite_scope_refuse_identically_under_both_orders() {
    for order in [["n", "w"], ["w", "n"]] {
        let refs: Vec<Json> = order
            .iter()
            .map(|name| json!({ "@id": format!("http://example.org#M.sub.{name}") }))
            .collect();
        let doc = json!({
            "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
            "@graph": [
                { "@id": "http://example.org#M", "@type": "S231:Block",
                  "S231:hasParameter": { "@id": "http://example.org#M.n" },
                  "S231:containsBlock": [ { "@id": "http://example.org#M.sub" } ] },
                { "@id": "http://example.org#M.n", "S231:value": 2 },
                { "@id": "http://example.org#M.sub",
                  "@type": "http://example.org#Vendor.Sequences.Inner",
                  "S231:hasParameter": refs,
                  "S231:containsBlock": [ { "@id": "http://example.org#M.sub.con" } ] },
                { "@id": "http://example.org#M.sub.n", "S231:value": 3 },
                { "@id": "http://example.org#M.sub.w", "S231:value": "n" },
                { "@id": "http://example.org#M.sub.con",
                  "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                  "S231:hasParameter": { "@id": "http://example.org#M.sub.con.k" },
                  "S231:hasOutput": { "@id": "http://example.org#M.sub.con.y" } },
                { "@id": "http://example.org#M.sub.con.k",
                  "S231:label": "k[2]",
                  "S231:isArray": true,
                  "S231:numberDimensions": 1,
                  "S231:sizeOfDimensions": "(w)",
                  "S231:value": [
                      { "@value": "2.0", "@type": "http://www.w3.org/2001/XMLSchema#double" },
                      { "@value": "3.0", "@type": "http://www.w3.org/2001/XMLSchema#double" }
                  ] },
                { "@id": "http://example.org#M.sub.con.y", "@type": "S231:RealOutput",
                  "S231:isOfDataType": { "@id": "S231:Real" } }
            ]
        });
        let diags = import_json(&doc).expect_err("the element-count divergence refuses");
        assert_eq!(diags.len(), 1, "order {order:?}: {diags:#?}");
        assert_eq!(diags[0].code, DiagCode::GroundingFailed, "order {order:?}");
        assert_eq!(
            diags[0].subject.as_deref(),
            Some("http://example.org#M.sub.con.k"),
            "order {order:?}"
        );
        assert_eq!(
            diags[0].message,
            "array value list has 2 element(s) but the declared dimensions imply 3",
            "the own n=3 must win consistently under order {order:?}"
        );
    }
}
