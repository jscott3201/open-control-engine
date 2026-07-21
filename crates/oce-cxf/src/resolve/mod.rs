//! The Layer-A → flat-`ModelGraph` resolver (doc 04 §7.1).
//!
//! AD-1: there is **no** hierarchical Layer-B — composites are flattened away here and the resolver
//! emits the flat `oce_model::ModelGraph` (D1's executable truth) directly. Only the elementary
//! instances named by the top composite's `containsBlock` become [`oce_model::BlockInstance`]s; the
//! composite itself contributes only its boundary ports (`hasInput`/`hasOutput`) and child list.
//!
//! ## Determinism
//! Every assignment of a `BlockId`, `ConnectorId`, vector position, or sort key is driven by an
//! **array** order — `@graph` array position, `containsBlock` order, an instance's `hasInput`/
//! `hasOutput`/`hasParameter` array order, or `isConnectedTo` array order. The `by_id` /
//! `block_of_iri` / `conn_of_iri` maps are **lookup-only**: their iteration order never feeds a
//! model id, a vector position, or a diagnostic order. The resolver's determinism tests re-import
//! twice and byte-compare the whole `ModelGraph`, so any `HashMap`-iteration-order leak here is a
//! defect.
//!
//! ## Boundary-input elision (AD-2)
//! A flat `Connection` is output→input only. A composite boundary **input** wired to a child input
//! is an illegal `In→In` edge: instead of a connection, the driven child input is recorded in
//! `ModelGraph.external_inputs` (legal in-degree 0) and tagged with the boundary's IRI. A child
//! **output** wired to a composite boundary output is likewise elided — the child output *is* the
//! model output. Boundary ports therefore receive **no `ConnectorId`**.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic, has_errors};
use oce_expr::EvalResult;
use oce_model::{
    BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value,
    ValueType, enum_class_id, is_g36_integer_constant_package,
};

use crate::arrays::expand_array_param;
use crate::dto::{CxfDocument, Node};
use crate::ground::{ParamScope, ground_value};
use crate::{CxfError, bridge};

mod attrs;
mod composite;
mod composite_rules;
#[cfg(test)]
mod composite_rules_tests;
mod diags;
mod specialize;

use attrs::connector_attrs;
use composite::lower;
use diags::{finalize_diags, subject_of};
use specialize::{specialize, validate_g36_parameter_value};

/// The Ground-mode import mode. Only `Ground` exists today: `oce_model::Value` has no symbolic
/// variant, so a `Symbolic` mode would have no representable output. `#[non_exhaustive]` reserves
/// the future `Symbolic`/round-trip mode (OQ-3) without a breaking change.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub enum ImportMode {
    /// Evaluate all parameter bindings to ground literals (the executable mode).
    #[default]
    Ground,
}

/// Options for [`import_cxf`](crate::import_cxf). `#[non_exhaustive]` + [`Default`] so a future
/// `LibraryIndex` / `shacl` field is non-breaking. (A `LibraryIndex` is deferred: current fixtures
/// declare their elementary interfaces inline, so the "library join" collapses to a registry
/// existence check via the internal `bridge` module.)
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct ResolveOptions {
    /// Import mode (Ground only today).
    pub mode: ImportMode,
    /// If `true`, any `Warning` also fails the load (doc 04 §9). Default `false`.
    pub deny_warnings: bool,
}

/// The resolver's diagnostics in deterministic order. On the `Ok` path it carries `Warning`/`Info`
/// only — any `Error` is returned inside [`CxfError::Validation`] instead, with the graph withheld
/// (it may be structurally unsound). Invariant enforced by construction in the resolver. The report
/// also carries the model identity side-channel for consumers that need durable identity without
/// polluting [`ModelGraph`] execution state.
#[derive(Clone, Debug, Default)]
pub struct ValidationReport {
    /// The top-composite `@id` that names the CXF model.
    ///
    /// This is the raw DTO [`Node::id`] value as authored in the document. The resolver currently
    /// carries context entries losslessly but does not perform general JSON-LD `@id` expansion, so
    /// callers that need a durable model key should treat this as the source CXF model IRI for M3.
    /// It is `Some` on every successful resolver-owned import path and `None` only for manually
    /// default-constructed reports.
    pub model_iri: Option<String>,
    /// The (sorted, error-free on the `Ok` path) diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    /// Whether the report carries no diagnostics at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Whether any diagnostic is an error (always `false` on the `Ok` path).
    #[must_use]
    pub fn has_errors(&self) -> bool {
        has_errors(&self.diagnostics)
    }
}

/// First `@type` term of a node, if any (defensive against an empty/multi `@type` array — never
/// indexes `[0]`).
fn first_type(node: &Node) -> Option<&str> {
    node.r#type
        .as_ref()
        .and_then(|t| t.as_slice().first())
        .map(String::as_str)
}

/// The trailing term of a `prefix:Local` / `…#Local` / `…/Local` IRI (for datatype/term matching).
fn term_of(iri: &str) -> &str {
    iri.rsplit([':', '#', '/']).next().unwrap_or(iri)
}

/// The local member name of a dotted instance-member `@id` — the segment after the **last `.`**,
/// e.g. `http://example.org#MinLoop.con.k` → `k`. This is the parameter key the block registry
/// looks up (`oce_blocks` reads `real_param(p, "k", …)`), so it MUST be the bare member name, not
/// the dotted path — a wrong name would silently fall back to the block's default value.
pub(crate) fn local_name(iri: &str) -> &str {
    iri.rsplit('.').next().unwrap_or(iri)
}
/// Map an `isOfDataType` term (`S231:Real` → [`ValueType::Real`], etc.). `None` if unrecognized.
fn value_type_of_datatype(iri: &str) -> Option<ValueType> {
    match term_of(iri) {
        "Real" => Some(ValueType::Real),
        "Integer" => Some(ValueType::Integer),
        "Boolean" => Some(ValueType::Boolean),
        _ => enum_class_id(iri)
            .map(ValueType::Enum)
            .or_else(|| is_g36_integer_constant_package(iri).then_some(ValueType::Integer)),
    }
}

/// Resolve a CXF document into the flat [`ModelGraph`] (doc 04 §7.1). See the module docs for the
/// determinism and boundary-elision contracts.
pub(crate) fn resolve(
    doc: &CxfDocument,
    opts: &ResolveOptions,
) -> Result<(ModelGraph, ValidationReport), CxfError> {
    let mut diags: Vec<Diagnostic> = Vec::new();

    // --- Step 1: @graph presence + index by @id (DuplicateId / MalformedDocument). by_id is
    // lookup-only. A poisoned index (empty graph, duplicate ids) is fatal — return immediately.
    if doc.graph.is_empty() {
        return Err(CxfError::Validation(vec![Diagnostic::error(
            DiagCode::MalformedDocument,
            "CXF @graph is empty — nothing to resolve",
        )]));
    }
    let mut by_id: HashMap<&str, &Node> = HashMap::with_capacity(doc.graph.len());
    for node in &doc.graph {
        if by_id.insert(node.id.as_str(), node).is_some() {
            diags.push(
                Diagnostic::error(DiagCode::DuplicateId, "duplicate @id in @graph")
                    .with_subject(node.id.clone()),
            );
        }
    }
    if has_errors(&diags) {
        return Err(CxfError::Validation(finalize_diags(diags, &HashMap::new())));
    }

    let specialization = specialize(doc, &by_id, &mut diags);
    let lowered = lower(doc, &by_id, &specialization, &mut diags);
    let doc = &lowered.doc;
    let mut by_id: HashMap<&str, &Node> = HashMap::with_capacity(doc.graph.len());
    for node in &doc.graph {
        by_id.insert(node.id.as_str(), node);
    }

    // --- Step 2: classify — the top composite selected by the pre-lowering pass names the flattened
    // instances and boundary ports. The root may have zero active children after specialization, so it
    // cannot be inferred from a non-empty `containsBlock` after pruning.
    let top = match lowered
        .root_iri
        .as_deref()
        .and_then(|root| by_id.get(root).copied())
    {
        Some(top) => top,
        None => {
            return Err(CxfError::Validation(finalize_diags(diags, &HashMap::new())));
        }
    };
    let child_iris: Vec<&str> = top
        .contains_block
        .iter()
        .map(|r| r.id.as_str())
        .filter(|iri| !specialization.is_inactive(iri))
        .collect();
    let boundary_in: HashSet<&str> = top
        .has_input
        .iter()
        .map(|r| r.id.as_str())
        .filter(|iri| !specialization.is_inactive(iri))
        .collect();
    let boundary_out: HashSet<&str> = top
        .has_output
        .iter()
        .map(|r| r.id.as_str())
        .filter(|iri| !specialization.is_inactive(iri))
        .collect();

    // --- Step 3: assign BlockId in containsBlock array order; bridge @type → class_path and check
    // the registry (Step 4 folded in). block_of_iri is lookup-only.
    let mut block_of_iri: HashMap<&str, BlockId> = HashMap::with_capacity(child_iris.len());
    let mut blocks: Vec<BlockInstance> = Vec::with_capacity(child_iris.len());
    // Remember each instance's node + port/param IRIs (in array order) for wiring/grounding.
    struct Inst<'a> {
        id: BlockId,
        node: &'a Node,
        input_iris: Vec<&'a str>,
        output_iris: Vec<&'a str>,
        inherited_scope: Vec<(Arc<str>, EvalResult)>,
    }
    let mut insts: Vec<Inst> = Vec::with_capacity(child_iris.len());
    for (k, &child) in child_iris.iter().enumerate() {
        let Some(node) = by_id.get(child).copied() else {
            diags.push(
                Diagnostic::error(
                    DiagCode::UnresolvedReference,
                    "containsBlock child not found",
                )
                .with_subject(child.to_owned()),
            );
            continue;
        };
        let id = BlockId(k as u32);
        block_of_iri.insert(child, id);
        // IRI → class_path bridge + registry existence (ClassNotFound covers non-subset, e.g. PID).
        let class_path = match first_type(node) {
            Some(t) => bridge::class_path_of(t),
            None => {
                diags.push(
                    Diagnostic::error(DiagCode::ClassNotFound, "instance node has no @type")
                        .with_subject(child.to_owned()),
                );
                ""
            }
        };
        if !class_path.is_empty() && oce_blocks::lookup(class_path).is_none() {
            diags.push(
                Diagnostic::error(
                    DiagCode::ClassNotFound,
                    format!("no registered block class for `{class_path}`"),
                )
                .with_subject(child.to_owned()),
            );
        }
        insts.push(Inst {
            id,
            node,
            input_iris: node
                .has_input
                .iter()
                .map(|r| r.id.as_str())
                .filter(|iri| !specialization.is_inactive(iri))
                .collect(),
            output_iris: node
                .has_output
                .iter()
                .map(|r| r.id.as_str())
                .filter(|iri| !specialization.is_inactive(iri))
                .collect(),
            inherited_scope: lowered
                .inherited_scope
                .get(child)
                .cloned()
                .unwrap_or_default(),
        });
        blocks.push(BlockInstance {
            id,
            // NOTE: `class_iri` holds the *bridged class_path* (e.g. "CDL.Reals.Add"), the join key
            // for `oce_blocks::lookup` — NOT the full @type IRI. Field name is a known wart inherited
            // from the spec sketch.
            class_iri: Arc::from(class_path),
            inputs: Vec::new(),            // filled in Step 5b
            outputs: Vec::new(),           // filled in Step 5b
            params: ParamTable::default(), // filled in Step 7
            decl_order: k as u32,
            instance_iri: Some(Arc::from(child)),
        });
    }

    // --- Step 5a: assign ConnectorId in @graph array order to every node referenced by an
    // instance port (boundary ports are referenced by the COMPOSITE, never an instance, so they are
    // naturally excluded → no ConnectorId). conn_of_iri is lookup-only.
    let instance_port_ids: HashSet<&str> = insts
        .iter()
        .flat_map(|i| i.input_iris.iter().chain(i.output_iris.iter()).copied())
        .collect();
    let mut conn_nodes: Vec<&Node> = Vec::new();
    let mut conn_of_iri: HashMap<&str, ConnectorId> = HashMap::new();
    for node in &doc.graph {
        if instance_port_ids.contains(node.id.as_str())
            && !conn_of_iri.contains_key(node.id.as_str())
        {
            let id = ConnectorId(conn_nodes.len() as u32);
            conn_of_iri.insert(node.id.as_str(), id);
            conn_nodes.push(node);
        }
    }

    // --- Step 5b: wiring — direction + owner come authoritatively from the instance port lists
    // (the side a connector is referenced on). Fill instance.inputs/outputs in array order.
    let mut owner_dir: Vec<Option<(BlockId, Dir)>> = vec![None; conn_nodes.len()];
    for inst in &insts {
        for (slot, iris, dir) in [
            (true, &inst.input_iris, Dir::In),
            (false, &inst.output_iris, Dir::Out),
        ] {
            for &iri in iris {
                match conn_of_iri.get(iri).copied() {
                    Some(cid) if owner_dir[cid.0 as usize].is_some() => {
                        // A connector IRI referenced by more than one instance port would otherwise
                        // be silently wired into two blocks (last-writer-wins owner + double-listed
                        // in both `inputs` vectors), which the scheduler would treat as one shared
                        // runtime cell. Reject it as malformed; do not overwrite or double-list.
                        diags.push(
                            Diagnostic::error(
                                DiagCode::MalformedDocument,
                                "connector referenced by multiple instance ports",
                            )
                            .with_subject(iri.to_owned()),
                        );
                    }
                    Some(cid) => {
                        owner_dir[cid.0 as usize] = Some((inst.id, dir));
                        let b = &mut blocks[inst.id.0 as usize];
                        if slot {
                            b.inputs.push(cid)
                        } else {
                            b.outputs.push(cid)
                        }
                    }
                    None => diags.push(
                        Diagnostic::error(DiagCode::UnresolvedReference, "instance port not found")
                            .with_subject(iri.to_owned()),
                    ),
                }
            }
        }
    }

    // --- Step 6: build connectors in ConnectorId order (value_type from isOfDataType, falling back
    // to the @type port term; dir/owner from Step 5b). The resolver PARSES each connector's declared
    // §7.4.1 attributes (unit/quantity/displayUnit/min/max) onto `Connector.attrs` so the §7.10 deep
    // gate (oce-validate) has something to *unify* — unification is oce-validate's job (AD-8), but the
    // declared attrs must flow from CXF first or the gate is dead on real input.
    let mut connectors: Vec<Connector> = Vec::with_capacity(conn_nodes.len());
    for (i, &node) in conn_nodes.iter().enumerate() {
        let vt = derive_value_type(node, &mut diags);
        let (block, dir) = owner_dir[i].unwrap_or_else(|| {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    "connector owned by no instance",
                )
                .with_subject(node.id.clone()),
            );
            (BlockId(0), Dir::In)
        });
        let mut c = Connector::new(ConnectorId(i as u32), block, dir, vt, i as u32);
        c.attrs = connector_attrs(node, vt, &mut diags);
        connectors.push(c);
    }

    // --- Step 7: ground parameters (Ground mode) in hasParameter/hasConstant array order. A later
    // binding may reference an earlier one via the incrementally-built ParamScope.
    for inst in &insts {
        let mut table: Vec<(Arc<str>, Value)> = Vec::new();
        let mut scope_entries: Vec<(Arc<str>, EvalResult)> = inst.inherited_scope.clone();
        // Collected (not lazily iterated) so the array branch can build the sibling-name set for its
        // collision check. Order = hasParameter array order, then hasConstant array order.
        let param_iris: Vec<&str> = inst
            .node
            .has_parameter
            .iter()
            .chain(inst.node.has_constant.iter())
            .map(|r| r.id.as_str())
            .collect();
        for &piri in &param_iris {
            let Some(pnode) = by_id.get(piri).copied() else {
                diags.push(
                    Diagnostic::error(DiagCode::UnresolvedReference, "parameter node not found")
                        .with_subject(piri.to_owned()),
                );
                continue;
            };
            let Some(cxf_val) = &pnode.value else {
                diags.push(
                    Diagnostic::error(
                        DiagCode::GroundingFailed,
                        "parameter has no value (Ground mode)",
                    )
                    .with_subject(piri.to_owned()),
                );
                continue;
            };
            validate_g36_parameter_value(
                pnode,
                cxf_val,
                &ParamScope::new(&scope_entries),
                &mut diags,
            );
            if pnode.is_array == Some(true) {
                // A preserved array parameter expands to per-element scalar entries (doc 04 §3.6.1).
                // Both CXF encodings (this, and pre-flattened k_1/k_2 scalars) converge here.
                expand_array_param(
                    piri,
                    pnode,
                    cxf_val,
                    &param_iris,
                    &mut table,
                    &mut scope_entries,
                    &mut diags,
                );
            } else {
                // Scalar parameter.
                let name: Arc<str> = Arc::from(local_name(piri));
                match ground_value(cxf_val, &ParamScope::new(&scope_entries)) {
                    Ok(v) => {
                        scope_entries.push((Arc::clone(&name), EvalResult::Scalar(v.clone())));
                        table.push((name, v));
                    }
                    Err(e) => diags.push(
                        Diagnostic::error(DiagCode::GroundingFailed, e.to_string())
                            .with_subject(piri.to_owned()),
                    ),
                }
            }
        }
        blocks[inst.id.0 as usize].params = ParamTable { values: table };
    }

    // --- Step 8: resolved class arity guard. The document's declared interface must match the
    // parameter-resolved class signature, or the engine's emit-by-port-index would later index past
    // `outputs`/`inputs` and PANIC on the tick. Connector value_type ↔ PortKind agreement is the
    // deeper §7.10 validation check.
    for inst in &insts {
        let block = &blocks[inst.id.0 as usize];
        let class_path = block.class_iri.as_ref();
        if class_path.is_empty() {
            continue;
        }
        let Some(entry) = oce_blocks::lookup(class_path) else {
            continue;
        };
        let probe = (entry.make)(&block.params);
        let sig = probe.resolved_signature();
        let (got_in, got_out) = (block.inputs.len(), block.outputs.len());
        let (want_in, want_out) = (sig.inputs.len(), sig.outputs.len());
        if got_in != want_in || got_out != want_out {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!(
                        "block interface mismatch for `{class_path}`: declared \
                         {got_in} input(s)/{got_out} output(s), class requires \
                         {want_in}/{want_out}"
                    ),
                )
                .with_subject(inst.node.id.clone()),
            );
        }
    }

    // --- Step 9: collect connections with boundary elision (source @graph order, isConnectedTo
    // array order). Mutates connectors[].iri for the elided boundary-input child.
    let mut connections: Vec<Connection> = Vec::new();
    let mut external_inputs: Vec<ConnectorId> = Vec::new();
    for node in &doc.graph {
        let source = node.id.as_str();
        if specialization.is_inactive(source) {
            if !node.is_connected_to.is_empty() {
                diags.push(
                    Diagnostic::error(
                        DiagCode::InactiveConditionalNode,
                        "inactive conditional node still carries active connections",
                    )
                    .with_subject(source.to_owned()),
                );
            }
            continue;
        }
        let src_is_boundary_in = boundary_in.contains(source);
        for tref in node.is_connected_to.iter() {
            let target = tref.id.as_str();
            if specialization.is_inactive(target) {
                diags.push(
                    Diagnostic::error(
                        DiagCode::InactiveConditionalNode,
                        "connection targets an inactive conditional node",
                    )
                    .with_subject(target.to_owned()),
                );
                continue;
            }
            if src_is_boundary_in {
                // boundary input → child input: elide; record external + attach boundary IRI (AD-2).
                match conn_of_iri.get(target).copied() {
                    Some(to) if connectors[to.0 as usize].dir != Dir::In => {
                        // A composite boundary INPUT may only drive a child INPUT — `external_inputs`
                        // are inputs by contract (oce-model). Driving an output is a direction error,
                        // and this elision path bypasses Step 10, so it must be checked here.
                        diags.push(
                            Diagnostic::error(
                                DiagCode::DirectionMismatch,
                                "boundary input drives a non-input connector",
                            )
                            .with_subject(target.to_owned()),
                        );
                    }
                    Some(to) => {
                        if !external_inputs.contains(&to) {
                            external_inputs.push(to);
                        }
                        connectors[to.0 as usize].iri = Some(Arc::from(source));
                    }
                    None => diags.push(
                        Diagnostic::error(
                            DiagCode::UnresolvedReference,
                            "boundary-input target not found",
                        )
                        .with_subject(target.to_owned()),
                    ),
                }
                continue;
            }
            if boundary_out.contains(target) {
                // child output → boundary output: elide (the child output IS the model output).
                continue;
            }
            match (
                conn_of_iri.get(source).copied(),
                conn_of_iri.get(target).copied(),
            ) {
                (Some(from), Some(to)) => connections.push(Connection { from, to }),
                (from, to) => {
                    if from.is_none() {
                        diags.push(
                            Diagnostic::error(
                                DiagCode::UnresolvedReference,
                                "connection source not found",
                            )
                            .with_subject(source.to_owned()),
                        );
                    }
                    if to.is_none() {
                        diags.push(
                            Diagnostic::error(
                                DiagCode::UnresolvedReference,
                                "connection target not found",
                            )
                            .with_subject(target.to_owned()),
                        );
                    }
                }
            }
        }
    }

    // --- Step 10: gross direction/type fail-fast on emitted edges (AD-8; deep §7.10 is oce-validate).
    for c in &connections {
        let (f, t) = (&connectors[c.from.0 as usize], &connectors[c.to.0 as usize]);
        if f.dir != Dir::Out || t.dir != Dir::In {
            diags.push(
                Diagnostic::error(
                    DiagCode::DirectionMismatch,
                    "connection is not output→input",
                )
                .with_subject(subject_of(f)),
            );
        }
        if f.value_type != t.value_type {
            diags.push(
                Diagnostic::error(
                    DiagCode::TypeMismatch,
                    format!(
                        "connected types differ: {:?} → {:?}",
                        f.value_type, t.value_type
                    ),
                )
                .with_subject(subject_of(t)),
            );
        }
    }

    // --- Step 11: boundary-aware single-assignment pre-check (AD-2). in-degree per input over the
    // emitted connections; in-degree 0 is legal iff the input is an external boundary input.
    let mut in_degree: HashMap<ConnectorId, u32> = HashMap::new();
    for c in &connections {
        *in_degree.entry(c.to).or_insert(0) += 1;
    }
    for conn in &connectors {
        if conn.dir != Dir::In {
            continue;
        }
        let deg = in_degree.get(&conn.id).copied().unwrap_or(0);
        if deg == 1 {
            continue;
        }
        if deg == 0 {
            if !external_inputs.contains(&conn.id) {
                diags.push(
                    Diagnostic::error(
                        DiagCode::SingleAssignment,
                        "input is undriven (in-degree 0)",
                    )
                    .with_subject(subject_of(conn)),
                );
            }
        } else {
            diags.push(
                Diagnostic::error(
                    DiagCode::SingleAssignment,
                    format!("input is multiply driven (in-degree {deg})"),
                )
                .with_subject(subject_of(conn)),
            );
        }
    }

    // --- Step 12: deterministic sort + return. On any Error (or any Warning under deny_warnings),
    // withhold the graph and return Err(CxfError::Validation).
    let diags = finalize_diags(diags, &conn_of_iri);
    if has_errors(&diags) || (opts.deny_warnings && !diags.is_empty()) {
        return Err(CxfError::Validation(diags));
    }
    let graph = ModelGraph {
        blocks,
        connectors,
        connections,
        external_inputs,
    };
    Ok((
        graph,
        ValidationReport {
            model_iri: Some(top.id.clone()),
            diagnostics: diags,
        },
    ))
}

/// Derive a connector's [`ValueType`], preferring `isOfDataType`, falling back to the `@type` port
/// term. Pushes a diagnostic (and returns a `Real` placeholder) on an unrecognized/absent type — so
/// the connectors vector stays dense and the load still fails via the diagnostic.
fn derive_value_type(node: &Node, diags: &mut Vec<Diagnostic>) -> ValueType {
    if let Some(dt) = &node.is_of_data_type {
        return value_type_of_datatype(&dt.id).unwrap_or_else(|| {
            diags.push(
                Diagnostic::error(DiagCode::UnresolvedReference, "unresolved isOfDataType")
                    .with_subject(dt.id.clone()),
            );
            ValueType::Real
        });
    }
    match first_type(node).map(term_of) {
        Some(t) if t.starts_with("Real") => ValueType::Real,
        Some(t) if t.starts_with("Integer") => ValueType::Integer,
        Some(t) if t.starts_with("Boolean") => ValueType::Boolean,
        Some(t) if t.starts_with("Analog") => {
            diags.push(
                Diagnostic::warning(
                    DiagCode::AnalogCoercedToReal,
                    "Analog connector coerced to Real",
                )
                .with_subject(node.id.clone()),
            );
            ValueType::Real
        }
        Some(t) if t.starts_with("String") => {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    "String connector not permitted (§7.8)",
                )
                .with_subject(node.id.clone()),
            );
            ValueType::Real
        }
        _ => {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    "connector lacks a recognized data type",
                )
                .with_subject(node.id.clone()),
            );
            ValueType::Real
        }
    }
}
