//! Minimal CXF exporter (#141): emit a flat, ground, single-root, scalar-parameter,
//! attribute-free [`ModelGraph`] as a CXF JSON-LD document.
//!
//! The contract is the RT-2 fixpoint, not source recovery: for a graph `G1` produced by
//! [`import_cxf`](crate::import_cxf), `import(export(G1))` lowers to a graph that renders
//! bit-identically to `G1` (floats compared by IEEE-754 bits). Cosmetic source content (labels,
//! layout, line numbers) is not in `ModelGraph`, so none of it comes back.
//!
//! ## Naming model
//! - The original root `@id` is not recorded in `ModelGraph`; the fixed synthetic
//!   [`EXPORT_ROOT_IRI`] stands in for it — re-import needs *a* root, not the original.
//! - Block node `@id`s are [`oce_model::BlockInstance::instance_iri`] verbatim (absent →
//!   rejected).
//! - Parameter node `@id`s are `<instance_iri>.<name>`; the importer recovers the parameter name
//!   as the segment after the last `.`, so names containing `.` are rejected.
//! - Connectors carry no usable source IRI (the resolver leaves `Connector::iri` `None` except
//!   for the AD-2 boundary-input overwrite), so port `@id`s are minted deterministically as
//!   `<instance_iri>.in<k>` / `<instance_iri>.out<k>` from the owner's port-list position.
//!   Re-import rebuilds wiring from `isConnectedTo`, so port names never need to round-trip.
//! - Each `external_inputs` connector's stored boundary IRI is emitted verbatim as a boundary
//!   node listed under the root's `hasInput` and wired `isConnectedTo` → the **minted** child
//!   port. Re-import then re-elides the boundary (AD-2), restoring both the `external_inputs`
//!   entry and the child connector's boundary IRI. The boundary `@id` never appears in the
//!   owning block's own `hasInput` list — sharing one `@id` between the root's and a child's
//!   port list re-imports as a rejection.
//!
//! ## Determinism
//! Emission order derives from the `ModelGraph` vectors alone: root, then blocks in `BlockId`
//! order (each followed by its parameter nodes in `ParamTable` order), then one port node per
//! connector in `ConnectorId` order, then boundary nodes in `external_inputs` first-occurrence
//! order. No map iteration feeds any ordering, and `write_document` serializes struct fields in
//! declaration order, so repeated exports are byte-identical.
//!
//! ## Value shape
//! Parameter bindings are bare JSON literals. `Real`s always serialize with a fractional part
//! (serde_json formats every finite `f64` with a `.` or exponent), so a whole-number Real never
//! re-grounds as an Integer; `Integer`s serialize bare, so they never come back as Reals.
//! Non-finite Reals would serialize as JSON `null` and are rejected instead. Strings are emitted
//! as quoted CDL string-literal expressions (`oce-expr` grounds them back to `Value::String`).
//!
//! ## Rejection surface
//! Everything outside the subset is a typed [`CxfError::Validation`] carrying
//! [`oce_diag::DiagCode::ExportUnsupported`] error diagnostics whose `subject` is the owning
//! block's `instance_iri` (connectors have none) — never a panic.

use std::collections::{BTreeMap, BTreeSet};

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{Attrs, Connector, Dir, IntAttrs, ModelGraph, RealAttrs, Value, ValueType};

use crate::dto::{Context, CxfDocument, CxfValue, IriRef, Node, OneOrMany};
use crate::{CxfError, bridge};

/// `@id` of the synthesized root composite. `ModelGraph` does not record the source document's
/// root IRI, so every export uses this fixed, collision-improbable URN. A (contrived) block whose
/// `instance_iri` equals it would surface as a loud `DuplicateId` on re-import, never silently.
const EXPORT_ROOT_IRI: &str = "urn:open-control:cxf-export:root";

/// The `S231` prefix binding emitted in `@context` (matches the import fixtures).
const S231_CONTEXT_IRI: &str = "http://data.ashrae.org/S231P#";

/// Base IRI for emitted instance `@type`s: `<base>Buildings.Controls.OBC.<class_path>` — the
/// inverse of [`bridge::class_path_of`], using the same base the import fixtures use. The
/// planner verifies each emitted `@type` re-bridges to the identical `class_iri` before
/// accepting the block.
const CLASS_IRI_BASE: &str = "http://example.org#";

/// Whole-operation rejection for a zero-block graph: a root with no `containsBlock` is not a
/// runtime composite, and a root-only document re-imports as a zero-candidate-root
/// `MalformedDocument` — there is nothing warning-free to emit.
const MSG_EMPTY: &str =
    "CXF export requires at least one block: an empty ModelGraph has no runtime composite to emit";
/// A block without an `instance_iri` cannot name its CXF node (hand-built graphs only; every
/// imported block carries one).
const MSG_NO_INSTANCE_IRI: &str = "export subset: block has no instance_iri to name its CXF node";
/// The class path cannot round-trip through the class-IRI bridge: a `#` in it changes the
/// re-imported fragment (a silent identity flip), and a path already carrying the OBC prefix
/// re-bridges verbatim but can never name a registry class, so its bytes would always fail
/// re-import — both are rejected under this message.
const MSG_CLASS_BRIDGE: &str =
    "export subset: class path does not survive the class-IRI bridge round-trip";
/// Block/connector cross-references disagree (non-dense ids, wrong owner or direction, a
/// connector claimed twice or never, a connection endpoint out of range or not output→input).
const MSG_STRUCTURE: &str = "export subset: block/connector wiring is structurally inconsistent";
/// The connector carries declared §7.4.1 attributes; the minimal exporter emits none, so
/// accepting it would silently drop them.
const MSG_ATTRS: &str =
    "export subset: connector declares §7.4.1 attributes, which the minimal exporter cannot emit";
/// String connectors have no importable CXF form (§7.8 forbids them).
const MSG_STRING_CONNECTOR: &str =
    "export subset: String connectors are not permitted in CXF (§7.8)";
/// Enumeration connectors need an enum-class IRI inverse mapping the minimal exporter lacks.
const MSG_ENUM_CONNECTOR: &str =
    "export subset: enumeration-typed connectors are outside the minimal export subset";
/// An `external_inputs` entry whose connector has no stored boundary IRI cannot rebuild the
/// root's `hasInput` (hand-built graphs only; the resolver always stores it).
const MSG_EXTERNAL_IRI: &str =
    "export subset: external input carries no boundary IRI to rebuild the root hasInput";
/// Two emitted nodes would share one `@id` — duplicate block `instance_iri`s, a parameter named
/// like a minted port (`out0`), a boundary IRI reusing another node's id. The document would
/// fail re-import with `DuplicateId`; reject at plan time with the owner named instead.
const MSG_DUPLICATE_ID: &str =
    "export subset: emitted node @id collides with another emitted node @id";

/// Everything [`build`] needs, fully validated: no `Option` left to unwrap, no index that can
/// miss. Produced by [`plan`] only when the subset checks all pass.
struct Plan {
    blocks: Vec<PlannedBlock>,
    ports: Vec<PlannedPort>,
    boundaries: Vec<PlannedBoundary>,
}

/// One block node plus its parameter nodes, with every `@id` already minted.
struct PlannedBlock {
    iri: String,
    type_iri: String,
    /// `(node @id, binding)` in `ParamTable` order.
    params: Vec<(String, CxfValue)>,
    input_ports: Vec<String>,
    output_ports: Vec<String>,
}

/// One port node: minted `@id`, `isOfDataType` term, and `isConnectedTo` targets (outputs only).
struct PlannedPort {
    iri: String,
    datatype: &'static str,
    targets: Vec<String>,
}

/// One boundary-input node: the stored boundary IRI and the minted child ports it drives.
struct PlannedBoundary {
    iri: String,
    targets: Vec<String>,
}

/// Export `model` as a CXF document, or reject it with `ExportUnsupported` diagnostics.
///
/// # Errors
/// [`CxfError::Validation`] when any subset check fails; see the module docs for the surface.
pub(crate) fn document(model: &ModelGraph) -> Result<CxfDocument, CxfError> {
    plan(model).map(build).map_err(CxfError::Validation)
}

/// An `ExportUnsupported` error diagnostic with a subject.
fn reject(message: impl Into<String>, subject: &str) -> Diagnostic {
    Diagnostic::error(DiagCode::ExportUnsupported, message).with_subject(subject.to_owned())
}

/// Record an `@id` the document will emit. A repeat is rejected at plan time with the owning
/// block as subject — the emitted document would otherwise fail re-import with `DuplicateId`
/// (or worse, silently merge two nodes in a permissive consumer).
fn claim_emitted_id(
    seen: &mut BTreeSet<String>,
    id: &str,
    subject: &str,
    diags: &mut Vec<Diagnostic>,
) {
    if !seen.insert(id.to_owned()) {
        diags.push(reject(MSG_DUPLICATE_ID, subject));
    }
}

/// The diagnostic subject for a connector: its owning block's `instance_iri`, else a synthetic
/// position tag (mirrors the resolver's `connector#N` convention for IRI-less connectors).
fn owner_subject(g: &ModelGraph, c: &Connector, position: usize) -> String {
    g.blocks
        .get(c.block.0 as usize)
        .and_then(|b| b.instance_iri.as_deref())
        .map_or_else(|| format!("connector#{position}"), str::to_owned)
}

/// Validate the export subset and mint every `@id`. Returns the full diagnostic list (all
/// offenders, in block-then-connector-then-connection scan order) when anything is outside the
/// subset. All ordering derives from the `ModelGraph` vectors.
fn plan(g: &ModelGraph) -> Result<Plan, Vec<Diagnostic>> {
    if g.blocks.is_empty() {
        return Err(vec![Diagnostic::error(
            DiagCode::ExportUnsupported,
            MSG_EMPTY,
        )]);
    }
    // Dense-id invariant: every position-based index below (and re-import itself) assumes
    // `BlockId.0`/`ConnectorId.0` equal vector position. The resolver guarantees it; a
    // hand-built graph that breaks it is rejected wholesale rather than exported shifted.
    let dense = g
        .blocks
        .iter()
        .enumerate()
        .all(|(i, b)| b.id.0 as usize == i)
        && g.connectors
            .iter()
            .enumerate()
            .all(|(i, c)| c.id.0 as usize == i);
    if !dense {
        return Err(vec![Diagnostic::error(
            DiagCode::ExportUnsupported,
            MSG_STRUCTURE,
        )]);
    }

    let mut diags: Vec<Diagnostic> = Vec::new();
    let mut port_iri: Vec<Option<String>> = vec![None; g.connectors.len()];
    let mut blocks: Vec<PlannedBlock> = Vec::with_capacity(g.blocks.len());
    // Every @id the document will carry, for duplicate detection only — membership checks,
    // never iteration, so emission order still derives from the ModelGraph vectors alone.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    seen.insert(EXPORT_ROOT_IRI.to_owned());

    for (bi, b) in g.blocks.iter().enumerate() {
        let subject: String = match b.instance_iri.as_deref() {
            Some(iri) => iri.to_owned(),
            None => {
                let synthetic = format!("block#{bi}");
                diags.push(reject(MSG_NO_INSTANCE_IRI, &synthetic));
                synthetic
            }
        };
        claim_emitted_id(&mut seen, &subject, &subject, &mut diags);
        let type_iri = format!("{CLASS_IRI_BASE}{}{}", bridge::OBC_PREFIX, b.class_iri);
        // Self-oracle: run the actual import bridge over the @type we are about to emit — anything
        // that would re-import under a different class identity is rejected here, loudly. An
        // already-OBC-prefixed class path passes that round-trip (the bridge strips exactly the
        // one prefix we just added) yet can never name a registry class, so it is rejected
        // explicitly rather than emitted as bytes that always fail re-import.
        if b.class_iri.starts_with(bridge::OBC_PREFIX)
            || bridge::class_path_of(&type_iri) != b.class_iri.as_ref()
        {
            diags.push(reject(MSG_CLASS_BRIDGE, &subject));
        }

        let mut params = Vec::with_capacity(b.params.values.len());
        for (name, value) in &b.params.values {
            if name.is_empty() || name.contains('.') {
                diags.push(reject(
                    format!(
                        "export subset: parameter name `{name}` is not a bare member name \
                         (re-import recovers the name after the last `.`)"
                    ),
                    &subject,
                ));
                continue;
            }
            if let Some(binding) = param_binding(name, value, &subject, &mut diags) {
                let param_id = format!("{subject}.{name}");
                claim_emitted_id(&mut seen, &param_id, &subject, &mut diags);
                params.push((param_id, binding));
            }
        }

        let mut input_ports = Vec::with_capacity(b.inputs.len());
        for (k, cid) in b.inputs.iter().enumerate() {
            let minted = format!("{subject}.in{k}");
            if claim_port(
                g,
                bi,
                cid.0,
                Dir::In,
                &minted,
                &mut port_iri,
                &subject,
                &mut diags,
            ) {
                claim_emitted_id(&mut seen, &minted, &subject, &mut diags);
                input_ports.push(minted);
            }
        }
        let mut output_ports = Vec::with_capacity(b.outputs.len());
        for (k, cid) in b.outputs.iter().enumerate() {
            let minted = format!("{subject}.out{k}");
            if claim_port(
                g,
                bi,
                cid.0,
                Dir::Out,
                &minted,
                &mut port_iri,
                &subject,
                &mut diags,
            ) {
                claim_emitted_id(&mut seen, &minted, &subject, &mut diags);
                output_ports.push(minted);
            }
        }

        blocks.push(PlannedBlock {
            iri: subject,
            type_iri,
            params,
            input_ports,
            output_ports,
        });
    }

    // Every connector must be claimed by exactly one owner port list (double claims were caught
    // above); an orphan would silently vanish from the document.
    for (i, c) in g.connectors.iter().enumerate() {
        if port_iri[i].is_none() {
            diags.push(reject(MSG_STRUCTURE, &owner_subject(g, c, i)));
        }
    }

    // Connector-level subset checks: importable value type, and zero declared §7.4.1 attributes
    // (the exporter emits none, so any `Some` field would be silently dropped otherwise).
    let mut datatypes: Vec<&'static str> = Vec::with_capacity(g.connectors.len());
    for (i, c) in g.connectors.iter().enumerate() {
        let subject = owner_subject(g, c, i);
        datatypes.push(match c.value_type {
            ValueType::Real => "S231:Real",
            ValueType::Integer => "S231:Integer",
            ValueType::Boolean => "S231:Boolean",
            ValueType::String => {
                diags.push(reject(MSG_STRING_CONNECTOR, &subject));
                "S231:Real" // placeholder; the non-empty diags gate discards the plan
            }
            ValueType::Enum(_) => {
                diags.push(reject(MSG_ENUM_CONNECTOR, &subject));
                "S231:Real" // placeholder; the non-empty diags gate discards the plan
            }
        });
        if !c.attrs.matches(c.value_type) {
            diags.push(reject(MSG_STRUCTURE, &subject));
        } else if !attrs_are_empty(&c.attrs) {
            diags.push(reject(MSG_ATTRS, &subject));
        }
    }

    // Connections become each source port's isConnectedTo list, in `connections` vector order.
    let mut targets: Vec<Vec<String>> = vec![Vec::new(); g.connectors.len()];
    for conn in &g.connections {
        let (f, t) = (conn.from.0 as usize, conn.to.0 as usize);
        let output_to_input = g.connectors.get(f).is_some_and(|c| c.dir == Dir::Out)
            && g.connectors.get(t).is_some_and(|c| c.dir == Dir::In);
        if !output_to_input {
            // Name the source endpoint's owner when it is in range (else the target's); only a
            // fully dangling edge — both endpoints out of range — stays subjectless.
            let subject = g
                .connectors
                .get(f)
                .map(|c| owner_subject(g, c, f))
                .or_else(|| g.connectors.get(t).map(|c| owner_subject(g, c, t)));
            diags.push(match &subject {
                Some(s) => reject(MSG_STRUCTURE, s),
                None => Diagnostic::error(DiagCode::ExportUnsupported, MSG_STRUCTURE),
            });
            continue;
        }
        // An unclaimed target was already rejected by the orphan scan; nothing to add here.
        if let Some(to_iri) = port_iri[t].as_ref() {
            targets[f].push(to_iri.clone());
        }
    }

    // Boundary inputs: group `external_inputs` by their stored boundary IRI in first-occurrence
    // order (one boundary node may fan out to several child inputs).
    let mut boundaries: Vec<PlannedBoundary> = Vec::new();
    for cid in &g.external_inputs {
        let idx = cid.0 as usize;
        let Some(c) = g.connectors.get(idx) else {
            // Out of range: there is no connector, so no owner to name — genuinely subjectless
            // (like the whole-graph dense-id rejection, and unlike every per-node rejection).
            diags.push(Diagnostic::error(
                DiagCode::ExportUnsupported,
                MSG_STRUCTURE,
            ));
            continue;
        };
        let subject = owner_subject(g, c, idx);
        if c.dir != Dir::In {
            diags.push(reject(MSG_STRUCTURE, &subject));
            continue;
        }
        let Some(boundary_iri) = c.iri.as_deref() else {
            diags.push(reject(MSG_EXTERNAL_IRI, &subject));
            continue;
        };
        let Some(target) = port_iri[idx].as_ref() else {
            continue; // orphan connector: already rejected above
        };
        match boundaries.iter_mut().find(|pb| pb.iri == boundary_iri) {
            Some(pb) => pb.targets.push(target.clone()),
            None => {
                // First occurrence mints the boundary node, so its @id joins the emitted set;
                // later same-IRI entries reuse the node (fan-out grouping), not a duplicate.
                claim_emitted_id(&mut seen, boundary_iri, &subject, &mut diags);
                boundaries.push(PlannedBoundary {
                    iri: boundary_iri.to_owned(),
                    targets: vec![target.clone()],
                });
            }
        }
    }

    if !diags.is_empty() {
        return Err(diags);
    }

    // All checks passed: every connector is claimed exactly once, so every `port_iri` is `Some`.
    let mut ports = Vec::with_capacity(g.connectors.len());
    for (i, minted) in port_iri.into_iter().enumerate() {
        let Some(iri) = minted else {
            // Unreachable after the orphan scan above; kept total (never a panic) regardless.
            return Err(vec![Diagnostic::error(
                DiagCode::ExportUnsupported,
                MSG_STRUCTURE,
            )]);
        };
        ports.push(PlannedPort {
            iri,
            datatype: datatypes[i],
            targets: std::mem::take(&mut targets[i]),
        });
    }

    Ok(Plan {
        blocks,
        ports,
        boundaries,
    })
}

/// Claim connector `raw_cid` for owner block `bi` under `dir`, recording `minted` as its port
/// `@id`. Returns `false` (with a structure diagnostic) when the connector is out of range, owned
/// by a different block, points the other way, or was already claimed.
#[allow(clippy::too_many_arguments)] // A claim needs the graph, the claim key, and the two
// accumulators; bundling them into a one-shot struct would only rename the arguments.
fn claim_port(
    g: &ModelGraph,
    bi: usize,
    raw_cid: u32,
    dir: Dir,
    minted: &str,
    port_iri: &mut [Option<String>],
    subject: &str,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let idx = raw_cid as usize;
    let claimable = g
        .connectors
        .get(idx)
        .is_some_and(|c| c.block.0 as usize == bi && c.dir == dir)
        && port_iri.get(idx).is_some_and(Option::is_none);
    if claimable {
        port_iri[idx] = Some(minted.to_owned());
    } else {
        diags.push(reject(MSG_STRUCTURE, subject));
    }
    claimable
}

/// Convert one ground parameter value to its canonical CXF binding, or reject it (enumeration
/// values have no literal form here; non-finite Reals would serialize as JSON `null`).
fn param_binding(
    name: &str,
    value: &Value,
    subject: &str,
    diags: &mut Vec<Diagnostic>,
) -> Option<CxfValue> {
    match value {
        Value::Real(r) if r.is_finite() => Some(CxfValue::Float(*r)),
        Value::Real(_) => {
            diags.push(reject(
                format!(
                    "export subset: parameter `{name}` is a non-finite Real, which cannot \
                     round-trip through JSON"
                ),
                subject,
            ));
            None
        }
        Value::Integer(i) => Some(CxfValue::Int(*i)),
        Value::Boolean(b) => Some(CxfValue::Bool(*b)),
        Value::String(s) => Some(CxfValue::Expr(string_literal(s))),
        Value::Enum { .. } => {
            diags.push(reject(
                format!(
                    "export subset: parameter `{name}` is enumeration-valued, which has no CXF \
                     literal form in the minimal export subset"
                ),
                subject,
            ));
            None
        }
    }
}

/// Whether a connector's attribute set is entirely undeclared (every field `None`). The minimal
/// exporter emits no §7.4.1 attributes, so anything else must be rejected, not dropped.
fn attrs_are_empty(attrs: &Attrs) -> bool {
    match attrs {
        Attrs::Real(a) => *a == RealAttrs::default(),
        Attrs::Integer(a) => *a == IntAttrs::default(),
        Attrs::Boolean(_) | Attrs::String(_) | Attrs::Enum(_) => true,
    }
}

/// Quote a `Value::String` payload as a CDL string-literal expression. Only `\` and `"` need
/// escaping (the `oce-expr` lexer resolves both; every other character — including raw control
/// characters — passes through the JSON string layer verbatim).
fn string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        if ch == '"' || ch == '\\' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

/// Render a validated [`Plan`] as the CXF document. Infallible: every `@id` and binding was
/// minted by [`plan`], and the emission order is fixed by the plan's vectors.
fn build(plan: Plan) -> CxfDocument {
    let mut graph = Vec::new();

    let mut root = blank_node(EXPORT_ROOT_IRI.to_owned());
    root.r#type = Some(OneOrMany::One("S231:Block".to_owned()));
    root.has_input = refs(plan.boundaries.iter().map(|b| b.iri.clone()).collect());
    root.contains_block = refs(plan.blocks.iter().map(|b| b.iri.clone()).collect());
    graph.push(root);

    for b in plan.blocks {
        let mut node = blank_node(b.iri);
        node.r#type = Some(OneOrMany::One(b.type_iri));
        node.has_input = refs(b.input_ports);
        node.has_output = refs(b.output_ports);
        node.has_parameter = refs(b.params.iter().map(|(id, _)| id.clone()).collect());
        graph.push(node);
        for (id, binding) in b.params {
            let mut pnode = blank_node(id);
            pnode.value = Some(binding);
            graph.push(pnode);
        }
    }

    for port in plan.ports {
        let mut node = blank_node(port.iri);
        node.is_of_data_type = Some(iri_ref(port.datatype.to_owned()));
        node.is_connected_to = refs(port.targets);
        graph.push(node);
    }

    for boundary in plan.boundaries {
        let mut node = blank_node(boundary.iri);
        node.is_connected_to = refs(boundary.targets);
        graph.push(node);
    }

    let mut context = BTreeMap::new();
    context.insert(
        "S231".to_owned(),
        serde_json::Value::String(S231_CONTEXT_IRI.to_owned()),
    );
    CxfDocument {
        context: Context::Map(context),
        graph,
        other: BTreeMap::new(),
    }
}

/// A [`Node`] with only its `@id` set — every other field absent/empty.
fn blank_node(id: String) -> Node {
    Node {
        id,
        r#type: None,
        has_input: OneOrMany::None,
        has_output: OneOrMany::None,
        has_parameter: OneOrMany::None,
        has_constant: OneOrMany::None,
        contains_block: OneOrMany::None,
        has_instance: OneOrMany::None,
        is_connected_to: OneOrMany::None,
        is_of_data_type: None,
        label: None,
        access: None,
        value: None,
        min: None,
        max: None,
        unit: None,
        quantity: None,
        display_unit: None,
        is_final: None,
        is_array: None,
        n_dims: None,
        size_dims: None,
        fmu_path: None,
        is_conditional: None,
        cond_expr: None,
        is_replaceable: None,
        other: BTreeMap::new(),
    }
}

/// A bare `{"@id": …}` cross-reference.
fn iri_ref(id: String) -> IriRef {
    IriRef {
        id,
        other: BTreeMap::new(),
    }
}

/// Wrap `@id`s as a [`OneOrMany`] edge: absent for zero, a bare object for one (the producer
/// byte style [`OneOrMany::One`] serializes to), an array otherwise.
fn refs(ids: Vec<String>) -> OneOrMany<IriRef> {
    let mut ids = ids;
    match ids.len() {
        0 => OneOrMany::None,
        1 => OneOrMany::One(iri_ref(ids.remove(0))),
        _ => OneOrMany::Many(ids.into_iter().map(iri_ref).collect()),
    }
}
