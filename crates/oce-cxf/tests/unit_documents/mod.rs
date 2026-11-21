//! The document the unit-spelling tests share (issue #631): one composite that has every role a
//! unit can sit on, and the helpers that write a unit on one of them and read it back.
//!
//! The roles, by the node that carries the term:
//!
//! - [`ROOT_INPUT`] and [`ROOT_OUTPUT`]: the composite's own `hasInput` and `hasOutput` nodes;
//! - [`PORT`]: an instance port listed by its block's `hasInput`;
//! - [`MEMBER`]: a connector listed by its block's `hasInstance` (the dialect modelica-json
//!   writes), from which the resolver derives the block's interface;
//! - [`PARAMETER`] and [`MEMBER_PARAMETER`]: a parameter listed by `hasParameter`, and one listed
//!   by `hasInstance` and classified against the class signature.
//!
//! No node carries a term until a test writes one, and no two roles share a §7.10 cluster except
//! where a test connects them on purpose.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::Diagnostic;
use oce_model::{Attrs, ModelGraph};
use serde_json::{Map, Value as Json, json};

/// The S231 namespace.
pub(crate) const S231: &str = "http://data.ashrae.org/S231#";
/// The QUDT schema namespace the modelica-json translations bind `qudt` to.
pub(crate) const QUDT_HASH: &str = "http://qudt.org/schema/qudt#";
/// The QUDT schema namespace QUDT publishes and CXF-Core binds `qudt` to.
pub(crate) const QUDT_SLASH: &str = "http://qudt.org/schema/qudt/";
/// The QUDT unit namespace modelica-json binds `unit` to.
pub(crate) const UNIT_HASH: &str = "http://qudt.org/vocab/unit#";
/// The QUDT unit namespace QUDT publishes.
pub(crate) const UNIT_SLASH: &str = "http://qudt.org/vocab/unit/";

/// The composite's own input, which drives `add.u2`.
pub(crate) const ROOT_INPUT: &str = "http://example.org#U.uIn";
/// The composite's own output, driven by `add.y`.
pub(crate) const ROOT_OUTPUT: &str = "http://example.org#U.yOut";
/// An instance port: the `Add` input `con.y` drives.
pub(crate) const PORT: &str = "http://example.org#U.add.u1";
/// A `hasInstance` member connector: the output of `gai`, which drives nothing.
pub(crate) const MEMBER: &str = "http://example.org#U.gai.y";
/// A parameter listed by `hasParameter`.
pub(crate) const PARAMETER: &str = "http://example.org#U.con.k";
/// A parameter listed by `hasInstance`.
pub(crate) const MEMBER_PARAMETER: &str = "http://example.org#U.gai.k";
/// An Integer output, which permits no term.
pub(crate) const INTEGER_OUTPUT: &str = "http://example.org#U.icon.y";
/// A Real input no node lists, so nothing reads it.
pub(crate) const ORPHAN: &str = "http://example.org#U.orphan";

/// The four roles a unit is read on.
pub(crate) const CONNECTOR_ROLES: [&str; 4] = [ROOT_INPUT, ROOT_OUTPUT, PORT, MEMBER];
/// The two parameter roles, whose terms are checked and never used.
pub(crate) const PARAMETER_ROLES: [&str; 2] = [PARAMETER, MEMBER_PARAMETER];
/// Every role.
pub(crate) const ROLES: [&str; 6] = [
    ROOT_INPUT,
    ROOT_OUTPUT,
    PORT,
    MEMBER,
    PARAMETER,
    MEMBER_PARAMETER,
];

/// The `@context` modelica-json writes, less the `q` prefix nothing here uses.
pub(crate) fn upstream_context() -> Json {
    json!({ "S231": S231, "qudt": QUDT_HASH, "unit": UNIT_HASH })
}

/// The shared composite under `context`, carrying no term: a Real `Constant` driving an `Add`
/// whose other input is the root input and whose output drives the root output and a
/// `MultiplyByParameter` written in the `hasInstance` dialect; an Integer `Constant`; and one
/// orphan Real input.
pub(crate) fn document(context: Json) -> Json {
    let id = |local: &str| format!("http://example.org#U.{local}");
    let reference = |local: &str| json!({ "@id": id(local) });
    let real = |local: &str, kind: &str, to: &[&str]| {
        let mut node = json!({
            "@id": id(local),
            "@type": format!("S231:Real{kind}"),
            "S231:isOfDataType": { "@id": "S231:Real" }
        });
        if !to.is_empty() {
            node["S231:isConnectedTo"] = to.iter().map(|target| reference(target)).collect();
        }
        node
    };
    let class = |path: &str| format!("http://example.org#Buildings.Controls.OBC.CDL.{path}");
    let graph = vec![
        json!({
            "@id": "http://example.org#U",
            "@type": "S231:Block",
            "S231:containsBlock": [
                reference("con"), reference("add"), reference("gai"), reference("icon")
            ],
            "S231:hasInput": reference("uIn"),
            "S231:hasOutput": reference("yOut")
        }),
        json!({
            "@id": id("con"),
            "@type": class("Reals.Sources.Constant"),
            "S231:hasParameter": reference("con.k"),
            "S231:hasOutput": reference("con.y")
        }),
        json!({ "@id": id("con.k"), "@type": "S231:Parameter", "S231:value": 1.0 }),
        real("con.y", "Output", &["add.u1"]),
        json!({
            "@id": id("add"),
            "@type": class("Reals.Add"),
            "S231:hasInput": [reference("add.u1"), reference("add.u2")],
            "S231:hasOutput": reference("add.y")
        }),
        real("add.u1", "Input", &[]),
        real("add.u2", "Input", &[]),
        real("add.y", "Output", &["yOut", "gai.u"]),
        real("uIn", "Input", &["add.u2"]),
        real("yOut", "Output", &[]),
        json!({
            "@id": id("gai"),
            "@type": class("Reals.MultiplyByParameter"),
            "S231:hasInstance": [reference("gai.u"), reference("gai.y"), reference("gai.k")]
        }),
        real("gai.u", "Input", &[]),
        real("gai.y", "Output", &[]),
        json!({
            "@id": id("gai.k"),
            "@type": "S231:Parameter",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:value": 2.0
        }),
        json!({
            "@id": id("icon"),
            "@type": class("Integers.Sources.Constant"),
            "S231:hasParameter": reference("icon.k"),
            "S231:hasOutput": reference("icon.y")
        }),
        json!({ "@id": id("icon.k"), "@type": "S231:Parameter", "S231:value": 1 }),
        json!({
            "@id": INTEGER_OUTPUT,
            "@type": "S231:IntegerOutput",
            "S231:isOfDataType": { "@id": "S231:Integer" }
        }),
        real("orphan", "Input", &[]),
    ];
    json!({ "@context": context, "@graph": graph })
}

/// The shared composite under [`upstream_context`].
pub(crate) fn upstream() -> Json {
    document(upstream_context())
}

/// The node `id` of `doc`.
pub(crate) fn node_mut<'a>(doc: &'a mut Json, id: &str) -> &'a mut Map<String, Json> {
    doc["@graph"]
        .as_array_mut()
        .expect("@graph")
        .iter_mut()
        .find(|node| node["@id"] == id)
        .and_then(Json::as_object_mut)
        .unwrap_or_else(|| panic!("no node `{id}`"))
}

/// `doc` with the property `key`, exactly as written, set to `value` on the node `id`.
pub(crate) fn with(mut doc: Json, id: &str, key: &str, value: Json) -> Json {
    node_mut(&mut doc, id).insert(key.to_owned(), value);
    doc
}

/// Import `doc`.
pub(crate) fn import(doc: &Json) -> Result<ModelGraph, CxfError> {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    import_cxf(&bytes, &ResolveOptions::default()).map(|(graph, report)| {
        assert!(
            report.diagnostics.is_empty(),
            "a loaded test document warns: {:?}",
            report.diagnostics
        );
        graph
    })
}

/// The graph `doc` loads to.
pub(crate) fn loaded(doc: &Json) -> ModelGraph {
    import(doc).unwrap_or_else(|error| panic!("expected a load: {error:?}"))
}

/// Every diagnostic of the refusal of `doc`, in the order the resolver reports them.
pub(crate) fn refusal(doc: &Json) -> Vec<Diagnostic> {
    match import(doc) {
        Err(CxfError::Validation(diags)) => diags,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// The declared `(unit, quantity, displayUnit)` of the connector role `id` in `graph`, as the
/// resolver read them and before any §7.10 propagation: a root input's declaration, a root
/// output's declaration, or a connector's own attributes.
pub(crate) fn terms(
    graph: &ModelGraph,
    id: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    let attrs = match id {
        ROOT_INPUT => graph
            .boundary_inputs
            .iter()
            .find(|input| input.iri.as_ref() == id)
            .map(|input| &input.attrs),
        ROOT_OUTPUT => graph
            .boundary_outputs
            .iter()
            .find(|output| output.iri.as_ref() == id)
            .map(|output| &output.attrs),
        _ => graph
            .connectors
            .iter()
            .find(|connector| connector.iri.as_deref() == Some(id))
            .map(|connector| &connector.attrs),
    };
    match attrs.unwrap_or_else(|| panic!("no connector role `{id}` in the graph")) {
        Attrs::Real(real) => (
            real.unit.as_deref().map(str::to_owned),
            real.quantity.as_deref().map(str::to_owned),
            real.display_unit.as_deref().map(str::to_owned),
        ),
        other => panic!("`{id}` is not a Real connector: {other:?}"),
    }
}

/// The `(unit, quantity, displayUnit)` of a connector role that declares only `unit`.
pub(crate) fn unit_only(unit: &str) -> (Option<String>, Option<String>, Option<String>) {
    (Some(unit.to_owned()), None, None)
}

/// The `(unit, quantity, displayUnit)` of a connector role that declares only `displayUnit`.
pub(crate) fn display_only(unit: &str) -> (Option<String>, Option<String>, Option<String>) {
    (None, None, Some(unit.to_owned()))
}

/// The `(unit, quantity, displayUnit)` of a connector role that declares nothing.
pub(crate) const NO_TERMS: (Option<String>, Option<String>, Option<String>) = (None, None, None);

/// The complete rendering of the graph `doc` loads to, for comparing two documents' outcomes.
pub(crate) fn rendered(doc: &Json) -> String {
    format!("{:?}", loaded(doc))
}
