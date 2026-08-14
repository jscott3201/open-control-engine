//! Restricted nested-composite pre-lowering for CXF import.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use crate::bridge;
use crate::dto::{CxfDocument, IriRef, Node, OneOrMany};
use oce_diag::{DiagCode, Diagnostic};
use oce_expr::EvalResult;

use super::composite_orientation::CompositeOrientation;
use super::composite_rules::{BANNED_MODELICA_KEY, CONTAINS_CYCLE, REPLACEABLE, ROOT_COUNT};
use super::declaration_scope::{Pass, WithheldFindings, evaluate_declarations};
use super::specialize::Specialization;

/// Maximum supported composite depth during `containsBlock` lowering.
///
/// The outermost composite is depth one. Real inputs measured at depth three or less; this cap
/// bounds the recursive lowering walk well below the measured stack-exhaustion threshold.
const MAX_COMPOSITE_NESTING_DEPTH: usize = 64;

/// Maximum non-top boundary nodes one rewritten connection path may enter.
const MAX_COMPOSITE_BOUNDARY_HOPS: usize = 64;

/// Maximum rewrite targets examined across one document's boundary traversal.
const MAX_COMPOSITE_BOUNDARY_TARGETS: usize = 65_536;

/// Maximum aggregate target-IRI bytes examined across one document's boundary traversal.
const MAX_COMPOSITE_BOUNDARY_TARGET_BYTES: usize = 8 * 1024 * 1024;

/// A CXF document lowered to the existing single-root, flat-child resolver shape, with a sidecar
/// for canonical connections whose source is a node-less derived connector.
#[derive(Clone, Debug)]
pub(super) struct LoweredCxf {
    pub(super) doc: CxfDocument,
    pub(super) root_iri: Option<String>,
    pub(super) inherited_scope: HashMap<String, Vec<(Arc<str>, EvalResult)>>,
    pub(super) synthesized_connections: Vec<(String, Vec<String>)>,
    pub(super) boundary_traversal_failed: bool,
}

impl LoweredCxf {
    /// Authored edges followed by node-less derived-source edges in block-2 order.
    pub(super) fn connection_edges(&self) -> impl Iterator<Item = (&str, &str)> {
        let authored = self.doc.graph.iter().flat_map(|node| {
            node.is_connected_to
                .iter()
                .map(move |target| (node.id.as_str(), target.id.as_str()))
        });
        let synthesized = self
            .synthesized_connections
            .iter()
            .flat_map(|(source, targets)| {
                targets
                    .iter()
                    .map(move |target| (source.as_str(), target.as_str()))
            });
        authored.chain(synthesized)
    }
}

/// Lower the supported nested-composite subset before dense block/connector ids are assigned.
///
/// `withheld` carries the specialize pass's withheld tagged findings; the chains this pass
/// evaluates itself report from the lowering view, and the rest are released here (R20-7
/// reconciliation) — including on the no-root early return, where no chain is evaluated.
pub(super) fn lower(
    doc: &CxfDocument,
    by_id: &HashMap<&str, &Node>,
    specialization: &Specialization,
    withheld: WithheldFindings,
    diags: &mut Vec<Diagnostic>,
) -> LoweredCxf {
    let mut lowered = doc.clone();
    let mut inherited_scope = HashMap::new();
    let mut evaluated_chains: HashSet<String> = HashSet::new();

    let Some(root) = root_composite(doc, by_id, diags) else {
        withheld.emit_unvisited(&evaluated_chains, diags);
        return LoweredCxf {
            doc: lowered,
            root_iri: None,
            inherited_scope,
            synthesized_connections: Vec::new(),
            boundary_traversal_failed: false,
        };
    };
    let root_iri = Some(root.to_owned());

    reject_unsupported_constructs(doc, specialization, diags);

    let boundary = CompositeOrientation::new(doc, by_id, root, specialization);
    let mut leaf_order = Vec::new();
    let mut stack = HashSet::new();
    let mut path = Vec::new();
    collect_leaves(
        root,
        1,
        Vec::new(),
        by_id,
        specialization,
        diags,
        &mut stack,
        &mut path,
        &mut leaf_order,
        &mut inherited_scope,
        &mut evaluated_chains,
    );
    withheld.emit_unvisited(&evaluated_chains, diags);

    let (mut rewritten, deferred_diagnostic) =
        match rewrite_connections(doc, by_id, root, specialization, &boundary) {
            Ok(rewritten) => rewritten,
            Err(diagnostic) => {
                diags.push(diagnostic);
                return LoweredCxf {
                    doc: lowered,
                    root_iri,
                    inherited_scope,
                    synthesized_connections: Vec::new(),
                    boundary_traversal_failed: true,
                };
            }
        };
    if let Some(diagnostic) = deferred_diagnostic {
        diags.push(diagnostic);
    }
    let synthesized_connections = boundary
        .synthesized_sources()
        .filter_map(|source| {
            rewritten
                .remove(source)
                .map(|targets| (source.to_owned(), targets))
        })
        .collect();
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
        synthesized_connections,
        boundary_traversal_failed: false,
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

pub(super) fn is_runtime_composite(node: &Node) -> bool {
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
    depth: usize,
    parent_scope: Vec<(Arc<str>, EvalResult)>,
    by_id: &HashMap<&str, &Node>,
    specialization: &Specialization,
    diags: &mut Vec<Diagnostic>,
    stack: &mut HashSet<String>,
    path: &mut Vec<String>,
    leaf_order: &mut Vec<String>,
    inherited_scope: &mut HashMap<String, Vec<(Arc<str>, EvalResult)>>,
    evaluated_chains: &mut HashSet<String>,
) {
    if depth > MAX_COMPOSITE_NESTING_DEPTH {
        diags.push(
            Diagnostic::error(
                DiagCode::MalformedDocument,
                format!(
                    "composite/nesting-too-deep: containsBlock nesting exceeds the supported \
                     depth ({MAX_COMPOSITE_NESTING_DEPTH})"
                ),
            )
            .with_subject(composite_id.to_owned()),
        );
        return;
    }
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
    evaluated_chains.insert(composite_id.to_owned());
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
                depth + 1,
                scope.clone(),
                by_id,
                specialization,
                diags,
                stack,
                path,
                leaf_order,
                inherited_scope,
                evaluated_chains,
            );
        } else {
            leaf_order.push(child.to_owned());
            inherited_scope.insert(child.to_owned(), scope.clone());
        }
    }
    stack.remove(composite_id);
    path.pop();
}

/// Extend the inherited scope chain with the composite's own declarations through the shared
/// order-independent mechanism ([`super::declaration_scope`]) at its lowering invocation:
/// inactive declarations filtered through the completed [`Specialization`], array-flagged
/// declarations refused (`composite/array-parameter`), and every finding emitted.
fn composite_scope(
    composite: &Node,
    scope: Vec<(Arc<str>, EvalResult)>,
    by_id: &HashMap<&str, &Node>,
    specialization: &Specialization,
    diags: &mut Vec<Diagnostic>,
) -> Vec<(Arc<str>, EvalResult)> {
    evaluate_declarations(
        composite,
        &[],
        scope,
        by_id,
        Pass::Lowering {
            specialization,
            diags,
        },
    )
    .entries
}

fn rewrite_connections(
    doc: &CxfDocument,
    by_id: &HashMap<&str, &Node>,
    root: &str,
    specialization: &Specialization,
    boundary: &CompositeOrientation,
) -> Result<RewrittenConnections, Diagnostic> {
    let (canonical, crossed_drivers, deferred_diagnostic) =
        boundary.canonical_connections(doc, by_id, root, specialization);
    let mut deferred = Vec::new();
    let walk = BoundaryWalk {
        by_id,
        canonical: &canonical,
        specialization,
        boundary,
    };
    let mut budget = BoundaryBudget::default();
    for source in doc
        .graph
        .iter()
        .map(|node| node.id.as_str())
        .chain(boundary.synthesized_sources())
    {
        let Some(authored_targets) = canonical.get(source) else {
            continue;
        };
        if specialization.is_inactive(source) {
            continue;
        }
        if boundary.inputs.contains(source) && !boundary.top_inputs.contains(source) {
            continue;
        }
        if boundary.outputs.contains(source) && !boundary.top_outputs.contains(source) {
            continue;
        }
        let mut targets = Vec::new();
        for &target in authored_targets {
            resolve_authored_target(target, &walk, &mut budget, &mut targets)?;
        }
        // Lowered lists are NEVER deduplicated: forward+reverse restatements of one relation are
        // already collapsed in the canonical map, so any surviving duplicate is a genuine
        // double-drive that must stay visible to the single-assignment check.
        if crossed_drivers.contains(source) {
            targets.sort_by_key(|target| boundary.position(target));
        }
        if !targets.is_empty() {
            deferred.push((source, targets));
        }
    }
    Ok((
        deferred
            .into_iter()
            .map(|(source, targets)| {
                (
                    source.to_owned(),
                    targets.into_iter().map(str::to_owned).collect(),
                )
            })
            .collect(),
        deferred_diagnostic,
    ))
}

type RewrittenConnections = (HashMap<String, Vec<String>>, Option<Diagnostic>);

struct BoundaryWalk<'a, 'b> {
    by_id: &'a HashMap<&'a str, &'a Node>,
    canonical: &'b HashMap<&'b str, Vec<&'b str>>,
    specialization: &'a Specialization,
    boundary: &'a CompositeOrientation,
}

#[derive(Default)]
struct BoundaryBudget {
    examined_targets: usize,
    examined_target_bytes: usize,
}

impl BoundaryBudget {
    fn examine(&mut self, target: &str) -> Result<(), Diagnostic> {
        // Limit diagnostics omit the untrusted target because copying it would bypass the bound.
        if self.examined_targets >= MAX_COMPOSITE_BOUNDARY_TARGETS {
            return Err(Diagnostic::error(
                DiagCode::MalformedDocument,
                format!(
                    "composite boundary resolution exceeds the supported target examination count \
                     ({MAX_COMPOSITE_BOUNDARY_TARGETS})"
                ),
            ));
        }
        if target.len() > MAX_COMPOSITE_BOUNDARY_TARGET_BYTES - self.examined_target_bytes {
            return Err(Diagnostic::error(
                DiagCode::MalformedDocument,
                format!(
                    "composite boundary resolution exceeds the supported aggregate target IRI \
                     byte count ({MAX_COMPOSITE_BOUNDARY_TARGET_BYTES})"
                ),
            ));
        }
        self.examined_targets += 1;
        self.examined_target_bytes += target.len();
        Ok(())
    }
}

enum BoundaryFrame<'a> {
    Target(&'a str),
    Children {
        boundary: &'a str,
        next_child: usize,
    },
}

fn is_elided_boundary(target: &str, walk: &BoundaryWalk<'_, '_>) -> bool {
    (walk.boundary.inputs.contains(target) && !walk.boundary.top_inputs.contains(target))
        || (walk.boundary.outputs.contains(target) && !walk.boundary.top_outputs.contains(target))
}

fn resolve_authored_target<'a, 'b>(
    target: &'b str,
    walk: &BoundaryWalk<'a, 'b>,
    budget: &mut BoundaryBudget,
    out: &mut Vec<&'b str>,
) -> Result<(), Diagnostic> {
    if is_elided_boundary(target, walk) {
        resolve_target(target, walk, budget, out)
    } else {
        out.push(target);
        Ok(())
    }
}

/// Resolve one target with path-local cycle state and ordered depth-first expansion.
fn resolve_target<'a, 'b>(
    target: &'b str,
    walk: &BoundaryWalk<'a, 'b>,
    budget: &mut BoundaryBudget,
    out: &mut Vec<&'b str>,
) -> Result<(), Diagnostic> {
    let mut active_path: HashSet<&'b str> = HashSet::new();
    let mut frames = vec![BoundaryFrame::Target(target)];
    while let Some(frame) = frames.pop() {
        let BoundaryFrame::Target(target) = frame else {
            let BoundaryFrame::Children {
                boundary,
                next_child,
            } = frame
            else {
                unreachable!();
            };
            let Some(child) = walk
                .canonical
                .get(boundary)
                .and_then(|children| children.get(next_child))
                .copied()
            else {
                active_path.remove(boundary);
                continue;
            };
            frames.push(BoundaryFrame::Children {
                boundary,
                next_child: next_child + 1,
            });
            frames.push(BoundaryFrame::Target(child));
            continue;
        };

        budget.examine(target)?;
        if walk.specialization.is_inactive(target) {
            out.push(target);
            continue;
        }
        if !is_elided_boundary(target, walk) {
            out.push(target);
            continue;
        }

        // Preserve authored boundary-cycle and missing-node visibility before applying the resource
        // bound: both continue through the resolver's ordinary dangling-reference diagnostic.
        if active_path.contains(target) || !walk.by_id.contains_key(target) {
            out.push(target);
            continue;
        }
        if active_path.len() >= MAX_COMPOSITE_BOUNDARY_HOPS {
            // Keep the refusal path bounded for the same reason as BoundaryBudget::examine.
            return Err(Diagnostic::error(
                DiagCode::MalformedDocument,
                format!(
                    "composite boundary resolution exceeds the supported isConnectedTo hop count \
                     ({MAX_COMPOSITE_BOUNDARY_HOPS})"
                ),
            ));
        }

        active_path.insert(target);
        frames.push(BoundaryFrame::Children {
            boundary: target,
            next_child: 0,
        });
    }
    Ok(())
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

    #[test]
    fn boundary_target_budget_accepts_the_limit_and_refuses_before_overflow() {
        let mut budget = BoundaryBudget {
            examined_targets: MAX_COMPOSITE_BOUNDARY_TARGETS - 1,
            examined_target_bytes: 0,
        };
        assert!(budget.examine("http://example.org#at-limit").is_ok());
        assert_eq!(budget.examined_targets, MAX_COMPOSITE_BOUNDARY_TARGETS);
        let diagnostic = budget
            .examine("http://example.org#over-limit")
            .expect_err("the attempted target beyond the limit must reject");
        assert_eq!(diagnostic.code, DiagCode::MalformedDocument);
        assert_eq!(diagnostic.subject, None);
        assert_eq!(budget.examined_targets, MAX_COMPOSITE_BOUNDARY_TARGETS);
    }

    #[test]
    fn boundary_target_byte_budget_accepts_the_limit_and_refuses_before_overflow() {
        let mut budget = BoundaryBudget {
            examined_targets: 0,
            examined_target_bytes: MAX_COMPOSITE_BOUNDARY_TARGET_BYTES - 1,
        };
        assert!(budget.examine("x").is_ok());
        let diagnostic = budget
            .examine("y")
            .expect_err("the attempted byte beyond the limit must reject");
        assert_eq!(diagnostic.code, DiagCode::MalformedDocument);
        assert_eq!(diagnostic.subject, None);
        assert_eq!(
            budget.examined_target_bytes,
            MAX_COMPOSITE_BOUNDARY_TARGET_BYTES
        );
    }

    #[test]
    fn ordinary_direct_target_does_not_consume_boundary_budget() {
        let by_id = HashMap::new();
        let canonical = HashMap::new();
        let specialization = Specialization::default();
        let boundary = CompositeOrientation::default();
        let walk = BoundaryWalk {
            by_id: &by_id,
            canonical: &canonical,
            specialization: &specialization,
            boundary: &boundary,
        };
        let mut budget = BoundaryBudget {
            examined_targets: MAX_COMPOSITE_BOUNDARY_TARGETS,
            examined_target_bytes: MAX_COMPOSITE_BOUNDARY_TARGET_BYTES,
        };
        let mut out = Vec::new();
        resolve_authored_target("http://example.org#leaf.u", &walk, &mut budget, &mut out)
            .expect("ordinary wiring is outside boundary budgets");
        assert_eq!(out, ["http://example.org#leaf.u"]);
        assert_eq!(budget.examined_targets, MAX_COMPOSITE_BOUNDARY_TARGETS);
        assert_eq!(
            budget.examined_target_bytes,
            MAX_COMPOSITE_BOUNDARY_TARGET_BYTES
        );
    }
}
