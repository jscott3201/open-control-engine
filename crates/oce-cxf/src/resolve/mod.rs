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
};

use crate::arrays::expand_array_param;
use crate::dto::{CxfDocument, Node};
use crate::ground::{ParamScope, ground_value};
use crate::{CxfError, bridge};

mod array_nodes;
mod attrs;
mod composite;
mod composite_orientation;
mod composite_rules;
#[cfg(test)]
mod composite_rules_tests;
mod connection_orientation;
mod diags;
mod pass_through;
mod port_binding;
#[cfg(test)]
mod port_binding_tests;
mod single_assignment;
mod specialize;
mod value_types;

use attrs::connector_attrs;
use composite::lower;
use connection_orientation::orient_edge;
use diags::{finalize_diags, subject_of};
use specialize::{specialize, validate_g36_parameter_value};
use value_types::{derive_value_type, first_type};

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

/// The local member name of a dotted instance-member `@id` — the segment after the **last `.`**,
/// e.g. `http://example.org#MinLoop.con.k` → `k`. This is the parameter key the block registry
/// looks up (`oce_blocks` reads `real_param(p, "k", …)`), so it MUST be the bare member name, not
/// the dotted path — a wrong name would silently fall back to the block's default value.
pub(crate) fn local_name(iri: &str) -> &str {
    iri.rsplit('.').next().unwrap_or(iri)
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
    let mut by_id: HashMap<&str, (usize, &Node)> = HashMap::with_capacity(doc.graph.len());
    for (index, node) in doc.graph.iter().enumerate() {
        if node.id.is_empty() {
            let subject = format!("connector at @graph[{index}]");
            diags.push(
                Diagnostic::error(
                    DiagCode::MissingConnectorId,
                    format!("{subject} is missing its authored @id"),
                )
                .with_subject(subject),
            );
            continue;
        }
        if let Some((first_index, _)) = by_id.insert(node.id.as_str(), (index, node)) {
            diags.push(
                Diagnostic::error(
                    DiagCode::DuplicateId,
                    format!(
                        "duplicate @id `{}` on @graph nodes {first_index} and {index}",
                        node.id
                    ),
                )
                .with_subject(node.id.clone()),
            );
        }
    }
    if has_errors(&diags) {
        return Err(CxfError::Validation(finalize_diags(diags, &HashMap::new())));
    }

    let by_id: HashMap<&str, &Node> = by_id
        .into_iter()
        .map(|(id, (_, node))| (id, node))
        .collect();
    let specialization = specialize(doc, &by_id, &mut diags);
    array_nodes::reject_unsupported(doc, &specialization, &mut diags);
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
    let mut missing_boundaries: HashSet<&str> = HashSet::new();
    for iri in top
        .has_input
        .iter()
        .chain(top.has_output.iter())
        .map(|r| r.id.as_str())
    {
        // Re-apply the specialization-filtered sets so an inactive conditional boundary whose
        // node is absent does not diagnose.
        if (boundary_in.contains(iri) || boundary_out.contains(iri))
            && !by_id.contains_key(iri)
            && missing_boundaries.insert(iri)
        {
            diags.push(
                Diagnostic::error(DiagCode::UnresolvedReference, "boundary node not found")
                    .with_subject(iri.to_owned()),
            );
        }
    }

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
        c.iri = Some(Arc::from(node.id.as_str()));
        c.iri_was_legacy_host_path = false;
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

    // --- Step 8: interface agreement — arity, then port identity. Arity must match or the
    // engine's emit-by-port-index would later index past `inputs`/`outputs` and PANIC on the tick.
    // Identity is the separate question of WHICH connector belongs at each index: a document that
    // names its ports after the class's declared ports is permuted into signature order rather than
    // read positionally, because the reference toolchain orders connectors alphabetically and a
    // same-kind transposition is invisible to every other guard. See `port_binding`.
    for inst in &insts {
        let idx = inst.id.0 as usize;
        let class_path = blocks[idx].class_iri.clone();
        if class_path.is_empty() {
            continue;
        }
        let Some(entry) = oce_blocks::lookup(&class_path) else {
            continue;
        };
        let want = {
            let probe = (entry.make)(&blocks[idx].params);
            let sig = probe.resolved_signature();
            (sig.inputs.len(), sig.outputs.len())
        };
        port_binding::check_and_bind(
            &mut blocks[idx],
            &class_path,
            &inst.node.id,
            want,
            (&inst.input_iris, &inst.output_iris),
            &mut diags,
        );
    }

    // --- Step 9: collect connections with boundary elision (source @graph order, isConnectedTo
    // array order). Mutates connectors[].iri for the elided boundary-input child.
    let mut connections: Vec<Connection> = Vec::new();
    let mut external_inputs: Vec<ConnectorId> = Vec::new();
    let mut pass_through_pairs: Vec<(String, String)> = Vec::new();
    let mut seen_pass_through_pairs: HashSet<(String, String)> = HashSet::new();
    let mut boundary_output_drivers: HashMap<String, HashSet<String>> = HashMap::new();
    // Boundary nodes never enter Step 6, so derive their types separately in deterministic graph
    // order. Keep failed derivations as `None`: the helper has already diagnosed the declaration,
    // and the elision arms must not add a placeholder-based TypeMismatch.
    let boundary_types =
        pass_through::derive_boundary_types(doc, &boundary_in, &boundary_out, &mut diags);
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
            // Both endpoints have now been screened for inactivity in their authored roles; the
            // orientation swap below only relabels which is the driver.
            let (source, target) = orient_edge(
                source,
                target,
                &boundary_in,
                &boundary_out,
                &conn_of_iri,
                &connectors,
            );
            if boundary_in.contains(source) && boundary_in.contains(target) {
                diags.push(
                    Diagnostic::error(
                        DiagCode::DirectionMismatch,
                        "connection joins two boundary inputs",
                    )
                    .with_subject(target.to_owned()),
                );
                continue;
            }
            if boundary_out.contains(source) && boundary_out.contains(target) {
                diags.push(
                    Diagnostic::error(
                        DiagCode::DirectionMismatch,
                        "connection joins two boundary outputs",
                    )
                    .with_subject(target.to_owned()),
                );
                continue;
            }
            if boundary_in.contains(source) && boundary_out.contains(target) {
                let source_node = by_id.get(source).copied();
                let target_node = by_id.get(target).copied();
                let array_endpoint = source_node
                    .is_some_and(|node| node.is_array == Some(true) || node.size_dims.is_some())
                    || target_node.is_some_and(|node| {
                        node.is_array == Some(true) || node.size_dims.is_some()
                    });
                if array_endpoint {
                    diags.push(
                        Diagnostic::error(
                            DiagCode::NonSubsetConstruct,
                            "array boundary pass-through endpoints are unsupported",
                        )
                        .with_subject(target.to_owned()),
                    );
                    continue;
                }
                if let (Some(source_type), Some(target_type)) = (
                    boundary_types.get(source).copied().flatten(),
                    boundary_types.get(target).copied().flatten(),
                ) && source_type != target_type
                {
                    diags.push(
                        Diagnostic::error(
                            DiagCode::TypeMismatch,
                            format!(
                                "connected types differ: {:?} → {:?}",
                                source_type, target_type
                            ),
                        )
                        .with_subject(target.to_owned()),
                    );
                    continue;
                }
                let pair = (source.to_owned(), target.to_owned());
                if seen_pass_through_pairs.insert(pair.clone()) {
                    boundary_output_drivers
                        .entry(target.to_owned())
                        .or_default()
                        .insert(format!("boundary-input:{source}"));
                    pass_through_pairs.push(pair);
                }
                continue;
            }
            if boundary_in.contains(source) {
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
                        // This elision path bypasses Step 10, so it must also compare the boundary
                        // input's type with the child input's type here.
                        if let Some(boundary_type) = boundary_types.get(source).copied().flatten() {
                            let child = &connectors[to.0 as usize];
                            if boundary_type != child.value_type {
                                diags.push(
                                    Diagnostic::error(
                                        DiagCode::TypeMismatch,
                                        format!(
                                            "connected types differ: {:?} → {:?}",
                                            boundary_type, child.value_type
                                        ),
                                    )
                                    .with_subject(target.to_owned()),
                                );
                            }
                        }
                        if !external_inputs.contains(&to) {
                            external_inputs.push(to);
                        }
                        connectors[to.0 as usize].iri = Some(Arc::from(source));
                        connectors[to.0 as usize].iri_was_legacy_host_path = true;
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
                // The driving end MUST be checked here for the same reason the boundary-input arm
                // checks its target: this path `continue`s past Step 10, so nothing downstream ever
                // looks at `source`. Without it, `<boundary output> isConnectedTo <anything>` — a
                // child input, a parameter node, or an `@id` in no node at all — elides to silence.
                match conn_of_iri.get(source).copied() {
                    Some(from) if connectors[from.0 as usize].dir != Dir::Out => diags.push(
                        Diagnostic::error(
                            DiagCode::DirectionMismatch,
                            "boundary output is driven by a non-output connector",
                        )
                        .with_subject(source.to_owned()),
                    ),
                    Some(from) => {
                        if let Some(boundary_type) = boundary_types.get(target).copied().flatten() {
                            let child = &connectors[from.0 as usize];
                            if child.value_type != boundary_type {
                                diags.push(
                                    Diagnostic::error(
                                        DiagCode::TypeMismatch,
                                        format!(
                                            "connected types differ: {:?} → {:?}",
                                            child.value_type, boundary_type
                                        ),
                                    )
                                    .with_subject(source.to_owned()),
                                );
                            }
                        }
                        boundary_output_drivers
                            .entry(target.to_owned())
                            .or_default()
                            .insert(format!("connector:{}", from.0));
                    }
                    None => diags.push(
                        Diagnostic::error(
                            DiagCode::UnresolvedReference,
                            "boundary-output source not found",
                        )
                        .with_subject(source.to_owned()),
                    ),
                }
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

    for (output, drivers) in &boundary_output_drivers {
        if drivers.len() > 1 {
            diags.push(
                Diagnostic::error(
                    DiagCode::SingleAssignment,
                    format!(
                        "boundary output is multiply driven (distinct drivers {})",
                        drivers.len()
                    ),
                )
                .with_subject(output.clone()),
            );
        }
    }

    // `external_inputs` was filled by walking `@graph`, appending each boundary target as it was
    // met — so within one boundary port the vector inherited the order of that port's
    // `isConnectedTo` ARRAY. Two documents that list one port's fan-out targets in different orders
    // describe the same model and produced transposed vectors.
    //
    // The order is observable, which is what makes this worth fixing rather than tidying: `export`
    // emits the composite's boundary nodes in `external_inputs` first-occurrence order AND fills
    // each boundary node's own `isConnectedTo` array from the same vector, so a transposition here
    // is a same-kind port swap — the hazard `port_binding` exists to prevent, and one no arity or
    // kind check can see, because the count and the kinds are unchanged.
    //
    // Re-key on the boundary port's own node position. Note the scope of the claim: `@graph` is an
    // unordered set in JSON-LD, so this makes the order independent of the *array* spelling, not of
    // node order — node order remains load-bearing here exactly as `resolve_golden` already records
    // for the rest of the resolver.
    let graph_pos: HashMap<&str, usize> = doc
        .graph
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    pass_through_pairs.sort_by_key(|(input, output)| {
        (
            graph_pos.get(input.as_str()).copied().unwrap_or(usize::MAX),
            graph_pos
                .get(output.as_str())
                .copied()
                .unwrap_or(usize::MAX),
        )
    });
    pass_through::materialize(
        pass_through_pairs,
        &boundary_types,
        &mut blocks,
        &mut connectors,
        &mut external_inputs,
        &mut diags,
    );
    // `ConnectorId` is not a tie-break here, it is the whole discriminator, and calling it a
    // tie-break would misdescribe what changed: every entry belonging to ONE boundary port shares
    // that port's node position, so the first key component only orders port GROUPS relative to
    // each other, and `ConnectorId` alone orders within a group. That second component is what
    // moved all fifteen goldens. It is assigned in Step 5a from `@graph` order over instance port
    // nodes, so it does not follow the boundary port's array spelling.
    external_inputs.sort_by_key(|id| {
        let pos = connectors[id.0 as usize]
            .iri
            .as_deref()
            .and_then(|iri| graph_pos.get(iri).copied())
            .unwrap_or(usize::MAX);
        (pos, id.0)
    });

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
    single_assignment::check(&connectors, &connections, &external_inputs, &mut diags);

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
        boundary_outputs: vec![],
    };
    Ok((
        graph,
        ValidationReport {
            model_iri: Some(top.id.clone()),
            diagnostics: diags,
        },
    ))
}
