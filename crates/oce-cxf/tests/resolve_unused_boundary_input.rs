//! Pins for the `unused-boundary-input` warning: a root-declared boundary input that no
//! connection reads.
//!
//! Such a declaration is left out of `external_inputs` and `boundary_inputs` — the frame contract
//! requires every listed input to be supplied, so an unread one cannot be listed — and before the
//! warning existed it imported with zero diagnostics. The warning is now its only
//! representation, so these tests pin both directions: every unread shape warns exactly once with
//! the declared IRI as subject, and every read shape (a block target, a pass-through to a
//! boundary output, a nested-composite boundary hop, a fan-out through a nested composite, a
//! reverse-authored edge) stays silent. Declarations other checks already refuse, or that
//! specialization made inactive, must not warn on top of that verdict.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic, Severity};
use serde_json::{Value as JsonValue, json};

const EX: &str = "http://example.org#";
const ABS: &str = "http://example.org#Buildings.Controls.OBC.CDL.Reals.Abs";
const MESSAGE: &str = "declared boundary input is read by no connection";

fn iri(suffix: &str) -> String {
    format!("{EX}{suffix}")
}

fn r(suffix: &str) -> JsonValue {
    json!({ "@id": iri(suffix) })
}

fn refs(suffixes: &[&str]) -> JsonValue {
    JsonValue::Array(suffixes.iter().map(|suffix| r(suffix)).collect())
}

fn document(graph: Vec<JsonValue>) -> JsonValue {
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": EX },
        "@graph": graph,
    })
}

/// A Real port node, connected to `targets` when non-empty.
fn port(suffix: &str, kind: &str, targets: &[&str]) -> JsonValue {
    let mut node = json!({
        "@id": iri(suffix),
        "@type": format!("S231:Real{kind}"),
        "S231:isOfDataType": { "@id": "S231:Real" },
    });
    if !targets.is_empty() {
        node["S231:isConnectedTo"] = refs(targets);
    }
    node
}

/// One `Reals.Abs` leaf `<name>` with ports `<name>.u` and `<name>.y`; `y` drives `y_targets`.
fn abs(name: &str, y_targets: &[&str]) -> Vec<JsonValue> {
    vec![
        json!({
            "@id": iri(name),
            "@type": ABS,
            "S231:hasInput": r(&format!("{name}.u")),
            "S231:hasOutput": r(&format!("{name}.y")),
        }),
        port(&format!("{name}.u"), "Input", &[]),
        port(&format!("{name}.y"), "Output", y_targets),
    ]
}

fn root(blocks: &[&str], inputs: &[&str], outputs: &[&str]) -> JsonValue {
    json!({
        "@id": iri("M"),
        "@type": "S231:Block",
        "S231:containsBlock": refs(blocks),
        "S231:hasInput": refs(inputs),
        "S231:hasOutput": refs(outputs),
    })
}

/// `M.u → M.abs.u`, `M.abs.y → M.y`, plus each extra root input in `unread` declared with a
/// connection-free node. `unread` is both the `hasInput` tail and the node order.
fn single_abs_with_unread(unread: &[&str]) -> JsonValue {
    let mut inputs = vec!["M.u"];
    inputs.extend_from_slice(unread);
    let mut graph = vec![
        root(&["M.abs"], &inputs, &["M.y"]),
        port("M.u", "Input", &["M.abs.u"]),
    ];
    graph.extend(unread.iter().map(|suffix| port(suffix, "Input", &[])));
    graph.push(port("M.y", "Output", &[]));
    graph.extend(abs("M.abs", &["M.y"]));
    document(graph)
}

fn import_with(doc: &JsonValue, opts: &ResolveOptions) -> Result<Vec<Diagnostic>, Vec<Diagnostic>> {
    let bytes = serde_json::to_vec(doc).expect("serialize test document");
    match import_cxf(&bytes, opts) {
        Ok((_, report)) => Ok(report.diagnostics),
        Err(CxfError::Validation(diags)) => Err(diags),
        Err(other) => panic!("unexpected non-validation failure: {other:?}"),
    }
}

fn import(doc: &JsonValue) -> Result<Vec<Diagnostic>, Vec<Diagnostic>> {
    import_with(doc, &ResolveOptions::default())
}

fn unused(subject: &str) -> Diagnostic {
    Diagnostic::warning(DiagCode::UnusedBoundaryInput, MESSAGE).with_subject(iri(subject))
}

fn assert_clean(doc: &JsonValue, label: &str) {
    let diags = import(doc).unwrap_or_else(|diags| panic!("{label}: refused: {diags:#?}"));
    assert!(
        diags.is_empty(),
        "{label}: a read input must not warn: {diags:#?}"
    );
}

#[test]
fn an_unread_root_input_warns_once_with_its_declared_iri() {
    let diags = import(&single_abs_with_unread(&["M.v"])).expect("an unread input still loads");
    assert_eq!(diags, vec![unused("M.v")]);
    assert_eq!(diags[0].severity, Severity::Warning);
}

#[test]
fn an_unread_input_is_left_out_of_the_resolved_input_surface() {
    let bytes = serde_json::to_vec(&single_abs_with_unread(&["M.v"])).unwrap();
    let (graph, _) = import_cxf(&bytes, &ResolveOptions::default()).expect("loads");
    let declared: Vec<&str> = graph.boundary_inputs.iter().map(|b| &*b.iri).collect();
    assert_eq!(declared, vec![iri("M.u").as_str()]);
    assert_eq!(graph.external_inputs.len(), 1);
}

#[test]
fn deny_warnings_refuses_an_unread_input() {
    let mut opts = ResolveOptions::default();
    opts.deny_warnings = true;
    let diags = import_with(&single_abs_with_unread(&["M.v"]), &opts)
        .expect_err("deny_warnings turns the advisory into a refusal");
    assert_eq!(diags, vec![unused("M.v")]);
}

#[test]
fn several_unread_inputs_report_in_one_order_whatever_the_authored_order() {
    let forward = import(&single_abs_with_unread(&["M.a", "M.b", "M.c"])).expect("loads");
    let reversed = import(&single_abs_with_unread(&["M.c", "M.b", "M.a"])).expect("loads");
    let expected = vec![unused("M.a"), unused("M.b"), unused("M.c")];
    assert_eq!(forward, expected);
    assert_eq!(reversed, expected);
    let again = import(&single_abs_with_unread(&["M.c", "M.b", "M.a"])).expect("loads");
    assert_eq!(again, reversed, "repeat loads must be identical");
}

#[test]
fn a_twice_listed_unread_input_warns_once() {
    let mut doc = single_abs_with_unread(&["M.v"]);
    doc["@graph"][0]["S231:hasInput"] = refs(&["M.u", "M.v", "M.v"]);
    assert_eq!(import(&doc).expect("loads"), vec![unused("M.v")]);
}

#[test]
fn every_input_read_by_a_block_is_silent() {
    assert_clean(&single_abs_with_unread(&[]), "single block");
}

#[test]
fn a_reverse_authored_edge_reads_the_input() {
    // The child input carries the relation (`M.abs.u isConnectedTo M.u`); CXF lets either
    // endpoint carry it, so the input is read.
    let mut graph = vec![
        root(&["M.abs"], &["M.u"], &["M.y"]),
        port("M.u", "Input", &[]),
        port("M.y", "Output", &[]),
    ];
    let mut leaf = abs("M.abs", &["M.y"]);
    leaf[1]["S231:isConnectedTo"] = r("M.u");
    graph.extend(leaf);
    assert_clean(&document(graph), "reverse-authored edge");
}

#[test]
fn a_pass_through_to_a_boundary_output_reads_the_input() {
    let mut graph = vec![
        root(&["M.abs"], &["M.u", "M.p"], &["M.y", "M.q"]),
        port("M.u", "Input", &["M.abs.u"]),
        port("M.p", "Input", &["M.q"]),
        port("M.y", "Output", &[]),
        port("M.q", "Output", &[]),
    ];
    graph.extend(abs("M.abs", &["M.y"]));
    assert_clean(&document(graph), "pass-through");
}

/// Root `M` contains composite `M.sub` (inputs `M.sub.u`, `M.sub.w`; output `M.sub.y`) holding
/// leaves `M.sub.a` and `M.sub.b`. `sub_u_targets` are `M.sub.u`'s targets; `M.sub.w` is read by
/// nothing. The root input `M.u` drives `M.sub.u` and, when `root_w`, `M.w` drives `M.sub.w`.
fn nested(sub_u_targets: &[&str], root_w: bool) -> JsonValue {
    let inputs: &[&str] = if root_w { &["M.u", "M.w"] } else { &["M.u"] };
    let mut graph = vec![
        root(&["M.sub"], inputs, &["M.y"]),
        port("M.u", "Input", &["M.sub.u"]),
        port("M.y", "Output", &[]),
        json!({
            "@id": iri("M.sub"),
            "@type": "http://example.org#Vendor.Sequences.Fanout",
            "S231:containsBlock": refs(&["M.sub.a", "M.sub.b"]),
            "S231:hasInput": refs(&["M.sub.u", "M.sub.w"]),
            "S231:hasOutput": r("M.sub.y"),
        }),
        port("M.sub.u", "Input", sub_u_targets),
        port("M.sub.w", "Input", &[]),
        port("M.sub.y", "Output", &["M.y"]),
    ];
    if root_w {
        graph.insert(2, port("M.w", "Input", &["M.sub.w"]));
    }
    graph.extend(abs("M.sub.a", &["M.sub.y"]));
    graph.extend(abs("M.sub.b", &[]));
    document(graph)
}

#[test]
fn a_nested_composite_boundary_hop_reads_the_input() {
    // `M.sub.b.y` drives nothing, which is legal for an output; `M.sub.b.u` is driven by the
    // fan-out below, so the only interesting declaration here is the nested `M.sub.w`.
    assert_clean(
        &nested(&["M.sub.a.u", "M.sub.b.u"], false),
        "fan-out through a nested composite",
    );
}

#[test]
fn an_unread_input_of_a_nested_composite_interface_does_not_warn() {
    // `M.sub.w` is the nested composite's own declaration, not the host's frame contract; the
    // warning is scoped to the root, so the document above loads clean while `M.sub.w` is unread.
    let diags = import(&nested(&["M.sub.a.u", "M.sub.b.u"], false)).expect("loads");
    assert!(
        diags
            .iter()
            .all(|d| d.subject.as_deref() != Some(&*iri("M.sub.w"))),
        "nested interface declarations are out of scope: {diags:#?}"
    );
}

#[test]
fn a_root_input_whose_only_route_ends_at_an_unread_nested_input_warns() {
    // `M.w → M.sub.w`, and nothing inside `M.sub` reads `M.sub.w`: no block observes `M.w`.
    let diags = import(&nested(&["M.sub.a.u", "M.sub.b.u"], true)).expect("loads");
    assert_eq!(diags, vec![unused("M.w")]);
}

#[test]
fn a_root_input_without_a_node_is_refused_without_an_unused_warning() {
    let mut doc = single_abs_with_unread(&[]);
    doc["@graph"][0]["S231:hasInput"] = refs(&["M.u", "M.ghost"]);
    let diags = import(&doc).expect_err("a declared input with no node is refused");
    assert_eq!(
        diags,
        vec![
            Diagnostic::error(DiagCode::UnresolvedReference, "boundary node not found")
                .with_subject(iri("M.ghost"))
        ]
    );
}

#[test]
fn an_unread_iri_declared_as_input_and_output_reports_only_the_shadowing_refusal() {
    let mut doc = single_abs_with_unread(&["M.io"]);
    doc["@graph"][0]["S231:hasOutput"] = refs(&["M.y", "M.io"]);
    let diags = import(&doc).expect_err("dual-listed IRI is refused");
    assert!(
        diags
            .iter()
            .all(|d| d.code != DiagCode::UnusedBoundaryInput),
        "the shadowing refusal owns the dual-listed IRI: {diags:#?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == DiagCode::BoundaryOutputShadowsConnector)
    );
}

#[test]
fn an_inactive_conditional_root_input_does_not_warn() {
    let mut doc = single_abs_with_unread(&["M.opt"]);
    let graph = doc["@graph"].as_array_mut().unwrap();
    graph[0]["S231:hasParameter"] = r("M.have_opt");
    graph.push(json!({
        "@id": iri("M.have_opt"),
        "@type": "S231:Parameter",
        "S231:isOfDataType": { "@id": "S231:Boolean" },
        "S231:value": { "@value": "false", "@type": "http://www.w3.org/2001/XMLSchema#boolean" },
    }));
    let opt = graph
        .iter_mut()
        .find(|node| node["@id"] == iri("M.opt"))
        .unwrap();
    opt["S231:isConditionalComponent"] = json!(true);
    opt["S231:conditionalExpression"] = json!("have_opt");
    assert_clean(&doc, "inactive conditional input");
}

#[test]
fn an_unread_input_beside_a_refusal_still_reports_its_warning() {
    // A resolver refusal elsewhere in the document does not hide the advisory: the failure
    // receipt carries both, in the resolver's deterministic order.
    let mut doc = single_abs_with_unread(&["M.v"]);
    let graph = doc["@graph"].as_array_mut().unwrap();
    graph[0]["S231:containsBlock"] = refs(&["M.abs", "M.absent"]);
    let diags = import(&doc).expect_err("missing child is refused");
    assert!(diags.contains(&unused("M.v")), "{diags:#?}");
    assert!(
        diags
            .iter()
            .any(|d| d.code == DiagCode::UnresolvedReference && d.is_error())
    );
}

#[test]
fn an_unread_root_input_aliasing_a_child_port_reports_only_the_role_alias_refusal() {
    // The root `hasInput` names `M.abs2.y`, an existing port of a second contained block, with no
    // edge. The role-alias refusal owns that IRI; an unread-input advisory on top would describe a
    // declaration the load has already rejected as malformed.
    let mut doc = single_abs_with_unread(&[]);
    let graph = doc["@graph"].as_array_mut().unwrap();
    graph[0]["S231:containsBlock"] = refs(&["M.abs", "M.abs2"]);
    graph[0]["S231:hasInput"] = refs(&["M.u", "M.abs2.y"]);
    graph.extend(abs("M.abs2", &[]));
    let diags = import(&doc).expect_err("a role alias is refused");
    assert!(
        diags
            .iter()
            .all(|d| d.code != DiagCode::UnusedBoundaryInput),
        "the role-alias refusal owns the aliased IRI: {diags:#?}"
    );
    assert!(
        diags.iter().any(|d| d.code == DiagCode::MalformedDocument
            && d.subject.as_deref() == Some(&*iri("M.abs2.y"))),
        "{diags:#?}"
    );
}
