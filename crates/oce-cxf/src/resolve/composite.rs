//! Restricted nested-composite pre-lowering for CXF import.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use crate::dto::{CxfDocument, IriRef, Node, OneOrMany};
use crate::ground::{ParamScope, ground_value};
use crate::{bridge, resolve::local_name};
use oce_diag::{DiagCode, Diagnostic};
use oce_expr::EvalResult;

use super::composite_rules::{
    ARRAY_PARAMETER, BANNED_MODELICA_KEY, CONTAINS_CYCLE, REPLACEABLE, ROOT_COUNT,
};
use super::specialize::{Specialization, validate_g36_parameter_value};

/// A CXF document lowered to the existing single-root, flat-child resolver shape.
#[derive(Clone, Debug)]
pub(super) struct LoweredCxf {
    pub(super) doc: CxfDocument,
    pub(super) root_iri: Option<String>,
    pub(super) inherited_scope: HashMap<String, Vec<(Arc<str>, EvalResult)>>,
}

/// Lower the supported nested-composite subset before dense block/connector ids are assigned.
pub(super) fn lower(
    doc: &CxfDocument,
    by_id: &HashMap<&str, &Node>,
    specialization: &Specialization,
    diags: &mut Vec<Diagnostic>,
) -> LoweredCxf {
    let mut lowered = doc.clone();
    let mut inherited_scope = HashMap::new();

    let Some(root) = root_composite(doc, by_id, diags) else {
        return LoweredCxf {
            doc: lowered,
            root_iri: None,
            inherited_scope,
        };
    };
    let root_iri = Some(root.to_owned());

    reject_unsupported_constructs(doc, specialization, diags);

    let boundary = BoundaryIndex::new(doc, by_id, root, specialization);
    let mut leaf_order = Vec::new();
    let mut stack = HashSet::new();
    let mut path = Vec::new();
    collect_leaves(
        root,
        Vec::new(),
        by_id,
        specialization,
        diags,
        &mut stack,
        &mut path,
        &mut leaf_order,
        &mut inherited_scope,
    );

    let rewritten = rewrite_connections(doc, by_id, specialization, &boundary);
    for node in &mut lowered.graph {
        let id = node.id.as_str();
        if id == root {
            node.contains_block = refs(leaf_order.clone());
        } else if boundary.composites.contains(id) {
            node.contains_block = OneOrMany::None;
        }
        if !specialization.is_inactive(id) {
            node.is_connected_to = refs(rewritten.get(id).cloned().unwrap_or_default());
        }
    }

    LoweredCxf {
        doc: lowered,
        root_iri,
        inherited_scope,
    }
}

fn root_composite<'a>(
    doc: &'a CxfDocument,
    by_id: &HashMap<&str, &Node>,
    diags: &mut Vec<Diagnostic>,
) -> Option<&'a str> {
    let composites: Vec<&str> = doc
        .graph
        .iter()
        .filter(|node| is_runtime_composite(node))
        .map(|node| node.id.as_str())
        .collect();
    let mut referenced = HashSet::new();
    for id in &composites {
        let Some(node) = by_id.get(id).copied() else {
            continue;
        };
        for child in node.contains_block.iter().map(|r| r.id.as_str()) {
            if by_id
                .get(child)
                .is_some_and(|node| is_runtime_composite(node))
            {
                referenced.insert(child);
            }
        }
    }
    let roots: Vec<&str> = composites
        .into_iter()
        .filter(|id| !referenced.contains(id))
        .collect();
    match roots.as_slice() {
        [root] => Some(*root),
        [] => {
            // A pure `containsBlock` cycle lands HERE, not in the cycle detector: every cycle
            // member is referenced, so classification yields zero candidate roots and `lower`
            // returns before `collect_leaves` can run.
            diags.push(Diagnostic::error(
                ROOT_COUNT.code,
                ROOT_COUNT.message(
                    "expected exactly one top composite root after nested classification, \
                     found zero candidate roots",
                ),
            ));
            None
        }
        candidates => {
            // Candidates are already in document `@graph` order (`composites` derives from
            // `doc.graph.iter()`); the first one is the deterministic subject.
            diags.push(
                Diagnostic::error(
                    ROOT_COUNT.code,
                    ROOT_COUNT.message(format!(
                        "expected exactly one top composite root after nested classification, \
                         found {} candidate roots: {}",
                        candidates.len(),
                        candidates.join(", ")
                    )),
                )
                .with_subject(candidates[0].to_owned()),
            );
            None
        }
    }
}

fn is_runtime_composite(node: &Node) -> bool {
    !node.contains_block.is_empty() && !is_registered_leaf(node)
}

fn is_registered_leaf(node: &Node) -> bool {
    first_type(node).is_some_and(|type_iri| {
        let class_path = bridge::class_path_of(type_iri);
        oce_blocks::lookup(class_path).is_some()
    })
}

fn first_type(node: &Node) -> Option<&str> {
    node.r#type
        .as_ref()
        .and_then(|t| t.as_slice().first())
        .map(String::as_str)
}

fn reject_unsupported_constructs(
    doc: &CxfDocument,
    specialization: &Specialization,
    diags: &mut Vec<Diagnostic>,
) {
    for node in &doc.graph {
        if specialization.is_inactive(&node.id) {
            continue;
        }
        if node.is_replaceable == Some(true) {
            diags.push(
                Diagnostic::error(
                    REPLACEABLE.code,
                    REPLACEABLE
                        .message("replaceable CXF components must be resolved before import"),
                )
                .with_subject(node.id.clone()),
            );
        }
        for key in node.other.keys() {
            if unsupported_modelica_key(key) {
                diags.push(
                    Diagnostic::error(
                        BANNED_MODELICA_KEY.code,
                        BANNED_MODELICA_KEY.message(format!(
                            "unsupported Modelica construct `{key}` survived CXF lowering"
                        )),
                    )
                    .with_subject(node.id.clone()),
                );
            }
        }
    }
}

fn unsupported_modelica_key(key: &str) -> bool {
    let term = key.rsplit([':', '#', '/']).next().unwrap_or(key);
    matches!(
        term,
        "redeclare" | "constrainedby" | "extends" | "extendsFrom" | "moSource" | "modelicaSource"
    )
}

/// Depth-first `containsBlock` flattening. `stack` gives O(1) cycle membership; `path` mirrors it
/// as the ordered traversal spine so a detected cycle can name every participant in path order.
/// Both are pushed/popped together — `stack` and `path` always hold the same ids.
#[allow(clippy::too_many_arguments)]
fn collect_leaves(
    composite_id: &str,
    parent_scope: Vec<(Arc<str>, EvalResult)>,
    by_id: &HashMap<&str, &Node>,
    specialization: &Specialization,
    diags: &mut Vec<Diagnostic>,
    stack: &mut HashSet<String>,
    path: &mut Vec<String>,
    leaf_order: &mut Vec<String>,
    inherited_scope: &mut HashMap<String, Vec<(Arc<str>, EvalResult)>>,
) {
    if !stack.insert(composite_id.to_owned()) {
        // Reconstruct the cycle from the re-entered id's position on the traversal spine onward,
        // closing with the re-entered id itself. Traversal follows `containsBlock` document
        // order, so the participant list is deterministic. (`position` cannot miss — `stack`
        // membership implies `path` membership — but fall back to the whole spine, never panic.)
        let start = path
            .iter()
            .position(|id| id == composite_id)
            .unwrap_or_default();
        let mut participants: Vec<&str> = path[start..].iter().map(String::as_str).collect();
        participants.push(composite_id);
        diags.push(
            Diagnostic::error(
                CONTAINS_CYCLE.code,
                CONTAINS_CYCLE.message(format!(
                    "cycle in nested composite containsBlock graph: {}",
                    participants.join(" -> ")
                )),
            )
            .with_subject(composite_id.to_owned()),
        );
        return;
    }
    path.push(composite_id.to_owned());
    let Some(composite) = by_id.get(composite_id).copied() else {
        diags.push(
            Diagnostic::error(DiagCode::UnresolvedReference, "composite node not found")
                .with_subject(composite_id.to_owned()),
        );
        stack.remove(composite_id);
        path.pop();
        return;
    };
    let scope = composite_scope(composite, parent_scope, by_id, specialization, diags);
    for child in composite.contains_block.iter().map(|r| r.id.as_str()) {
        if specialization.is_inactive(child) {
            continue;
        }
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
        if is_runtime_composite(node) {
            collect_leaves(
                child,
                scope.clone(),
                by_id,
                specialization,
                diags,
                stack,
                path,
                leaf_order,
                inherited_scope,
            );
        } else {
            leaf_order.push(child.to_owned());
            inherited_scope.insert(child.to_owned(), scope.clone());
        }
    }
    stack.remove(composite_id);
    path.pop();
}

fn composite_scope(
    composite: &Node,
    mut scope: Vec<(Arc<str>, EvalResult)>,
    by_id: &HashMap<&str, &Node>,
    specialization: &Specialization,
    diags: &mut Vec<Diagnostic>,
) -> Vec<(Arc<str>, EvalResult)> {
    for piri in composite
        .has_parameter
        .iter()
        .chain(composite.has_constant.iter())
        .map(|r| r.id.as_str())
    {
        if specialization.is_inactive(piri) {
            continue;
        }
        let Some(pnode) = by_id.get(piri).copied() else {
            diags.push(
                Diagnostic::error(DiagCode::UnresolvedReference, "parameter node not found")
                    .with_subject(piri.to_owned()),
            );
            continue;
        };
        if pnode.is_array == Some(true) {
            diags.push(
                Diagnostic::error(
                    ARRAY_PARAMETER.code,
                    ARRAY_PARAMETER.message(
                        "array-valued composite parameters are not supported by this CXF \
                         lowering subset",
                    ),
                )
                .with_subject(piri.to_owned()),
            );
            continue;
        }
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
        validate_g36_parameter_value(pnode, cxf_val, &ParamScope::new(&scope), diags);
        match ground_value(cxf_val, &ParamScope::new(&scope)) {
            Ok(value) => scope.push((Arc::from(local_name(piri)), EvalResult::Scalar(value))),
            Err(e) => diags.push(
                Diagnostic::error(DiagCode::GroundingFailed, e.to_string())
                    .with_subject(piri.to_owned()),
            ),
        }
    }
    scope
}

#[derive(Clone, Debug, Default)]
struct BoundaryIndex {
    composites: HashSet<String>,
    inputs: HashSet<String>,
    outputs: HashSet<String>,
    top_inputs: HashSet<String>,
    top_outputs: HashSet<String>,
}

impl BoundaryIndex {
    fn new(
        doc: &CxfDocument,
        by_id: &HashMap<&str, &Node>,
        root: &str,
        specialization: &Specialization,
    ) -> Self {
        let mut index = BoundaryIndex::default();
        for node in &doc.graph {
            if !is_runtime_composite(node) || specialization.is_inactive(&node.id) {
                continue;
            }
            index.composites.insert(node.id.clone());
            for input in node.has_input.iter().map(|r| r.id.clone()) {
                index.inputs.insert(input.clone());
                if node.id == root {
                    index.top_inputs.insert(input);
                }
            }
            for output in node.has_output.iter().map(|r| r.id.clone()) {
                index.outputs.insert(output.clone());
                if node.id == root {
                    index.top_outputs.insert(output);
                }
            }
            for child in node.contains_block.iter().map(|r| r.id.as_str()) {
                if by_id
                    .get(child)
                    .is_some_and(|node| is_runtime_composite(node))
                {
                    index.composites.insert(child.to_owned());
                }
            }
        }
        index
    }
}

fn rewrite_connections(
    doc: &CxfDocument,
    by_id: &HashMap<&str, &Node>,
    specialization: &Specialization,
    boundary: &BoundaryIndex,
) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    let mut incoming: HashMap<&str, Vec<&str>> = HashMap::new();
    for node in &doc.graph {
        for target in node.is_connected_to.iter().map(|r| r.id.as_str()) {
            incoming.entry(target).or_default().push(node.id.as_str());
        }
    }
    let walk = BoundaryWalk {
        by_id,
        incoming: &incoming,
        specialization,
        boundary,
    };
    for node in &doc.graph {
        let source = node.id.as_str();
        if specialization.is_inactive(source) || node.is_connected_to.is_empty() {
            continue;
        }
        if boundary.inputs.contains(source) && !boundary.top_inputs.contains(source) {
            continue;
        }
        if boundary.outputs.contains(source) && !boundary.top_outputs.contains(source) {
            continue;
        }
        let mut targets = Vec::new();
        for target in node.is_connected_to.iter().map(|r| r.id.as_str()) {
            resolve_target(
                target,
                &walk,
                &mut HashSet::new(),
                Some(source),
                None,
                &mut targets,
            );
        }
        if !targets.is_empty() {
            out.entry(source.to_owned()).or_default().extend(targets);
        }
    }
    out
}

struct BoundaryWalk<'a> {
    by_id: &'a HashMap<&'a str, &'a Node>,
    incoming: &'a HashMap<&'a str, Vec<&'a str>>,
    specialization: &'a Specialization,
    boundary: &'a BoundaryIndex,
}

fn resolve_target(
    target: &str,
    walk: &BoundaryWalk<'_>,
    seen: &mut HashSet<String>,
    arrived_from: Option<&str>,
    fallback_predecessor: Option<&str>,
    out: &mut Vec<String>,
) {
    if walk.specialization.is_inactive(target) {
        out.push(target.to_owned());
        return;
    }
    if walk.boundary.inputs.contains(target) && !walk.boundary.top_inputs.contains(target) {
        follow_boundary(target, walk, seen, arrived_from, fallback_predecessor, out);
        return;
    }
    if walk.boundary.outputs.contains(target) {
        if walk.boundary.top_outputs.contains(target) {
            out.push(target.to_owned());
        } else {
            follow_boundary(target, walk, seen, arrived_from, fallback_predecessor, out);
        }
        return;
    }
    out.push(target.to_owned());
}

fn follow_boundary(
    boundary_iri: &str,
    walk: &BoundaryWalk<'_>,
    seen: &mut HashSet<String>,
    arrived_from: Option<&str>,
    fallback_predecessor: Option<&str>,
    out: &mut Vec<String>,
) {
    if !seen.insert(boundary_iri.to_owned()) {
        // Preserve authored boundary-cycle visibility by sending the revisited non-top IRI to the
        // resolver's ordinary dangling-reference diagnostic.
        out.push(boundary_iri.to_owned());
        return;
    }
    let Some(node) = walk.by_id.get(boundary_iri).copied() else {
        out.push(boundary_iri.to_owned());
        seen.remove(boundary_iri);
        return;
    };
    let authored = node
        .is_connected_to
        .iter()
        .map(|r| r.id.as_str())
        .collect::<Vec<_>>();
    let follows_incoming = authored.is_empty();
    let targets = if follows_incoming {
        walk.incoming
            .get(boundary_iri)
            .into_iter()
            .flat_map(|targets| targets.iter().copied())
            .filter(|target| {
                (walk.boundary.inputs.contains(*target) || walk.boundary.outputs.contains(*target))
                    && !walk.boundary.top_inputs.contains(*target)
                    && !walk.boundary.top_outputs.contains(*target)
                    && Some(*target) != arrived_from
            })
            .collect::<Vec<_>>()
    } else {
        authored
    };
    for target in targets {
        // A reverse-spelled fallback follows an incoming boundary edge. At the node it reaches,
        // exclude only the manufactured mirror edge back to that exact predecessor; any other
        // authored target remains visible, including a downstream cycle.
        if Some(target) != fallback_predecessor {
            resolve_target(
                target,
                walk,
                seen,
                Some(boundary_iri),
                follows_incoming.then_some(boundary_iri),
                out,
            );
        }
    }
    seen.remove(boundary_iri);
}

fn refs(ids: Vec<String>) -> OneOrMany<IriRef> {
    match ids.as_slice() {
        [] => OneOrMany::None,
        [one] => OneOrMany::One(IriRef {
            id: one.clone(),
            other: BTreeMap::new(),
        }),
        _ => OneOrMany::Many(
            ids.into_iter()
                .map(|id| IriRef {
                    id,
                    other: BTreeMap::new(),
                })
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_cdl_block_with_contains_block_is_not_a_runtime_composite() {
        let node: Node = serde_json::from_value(serde_json::json!({
            "@id": "http://example.org#B",
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Add",
            "S231:containsBlock": { "@id": "http://example.org#B.protected" }
        }))
        .expect("test node");
        assert!(!is_runtime_composite(&node));
    }
}
