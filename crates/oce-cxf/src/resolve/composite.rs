//! Restricted nested-composite pre-lowering for CXF import.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use crate::bridge;
use crate::dto::{CxfDocument, IriRef, Node, OneOrMany};
use oce_diag::{DiagCode, Diagnostic};

use super::composite_orientation::{
    CanonicalConnections, CompositeOrientation, EndpointIx, SharedIris,
};
use super::composite_rules::{BANNED_MODELICA_KEY, REPLACEABLE, ROOT_COUNT};
use super::declaration_scope::WithheldFindings;
use super::specialize::Specialization;
use crate::scope_chain::ScopeFrame;
use containment::Containment;

mod containment;

/// Maximum non-top boundary nodes one rewritten connection path may enter.
const MAX_COMPOSITE_BOUNDARY_HOPS: usize = 64;

/// Maximum rewrite targets examined across one document's boundary traversal.
const MAX_COMPOSITE_BOUNDARY_TARGETS: usize = 65_536;

/// Maximum aggregate target-IRI bytes examined across one document's boundary traversal.
const MAX_COMPOSITE_BOUNDARY_TARGET_BYTES: usize = 8 * 1024 * 1024;

/// A CXF document lowered to the existing single-root, flat-child resolver shape, with a sidecar
/// for canonical connections whose source is a node-less derived connector.
///
/// The lowered edges live beside the document, not in its `isConnectedTo` slots: each target is
/// one shared allocation per distinct IRI, so a lowered list that names one long IRI many times
/// (a swapped fan-out) holds it once (GitHub issue #480).
#[derive(Clone, Debug)]
pub(super) struct LoweredCxf {
    /// The lowered document; every node's `isConnectedTo` is empty — see `targets`.
    pub(super) doc: CxfDocument,
    pub(super) root_iri: Option<String>,
    pub(super) inherited_scope: HashMap<String, Arc<ScopeFrame>>,
    /// Lowered targets per `@graph` node, aligned with `doc.graph`.
    targets: Vec<Vec<Arc<str>>>,
    pub(super) synthesized_connections: Vec<(Arc<str>, Vec<Arc<str>>)>,
    pub(super) boundary_traversal_failed: bool,
}

impl LoweredCxf {
    /// `doc` unlowered: its authored edges move to the side table as they stand.
    fn unlowered(
        mut doc: CxfDocument,
        root_iri: Option<String>,
        inherited_scope: HashMap<String, Arc<ScopeFrame>>,
        boundary_traversal_failed: bool,
    ) -> Self {
        let targets = doc.graph.iter_mut().map(take_authored_targets).collect();
        Self {
            doc,
            root_iri,
            inherited_scope,
            targets,
            synthesized_connections: Vec::new(),
            boundary_traversal_failed,
        }
    }

    /// Each lowered `@graph` node's `@id` with its lowered targets, in `@graph` order.
    /// Guarantees the same edges in the same order as the previously rewritten `isConnectedTo`.
    pub(super) fn graph_edge_groups(&self) -> impl Iterator<Item = (&str, &[Arc<str>])> {
        self.doc
            .graph
            .iter()
            .zip(&self.targets)
            .map(|(node, targets)| (node.id.as_str(), targets.as_slice()))
    }

    /// Authored edges followed by node-less derived-source edges in block-2 order.
    /// Guarantees the same edges in the same order as the previously rewritten `isConnectedTo`.
    pub(super) fn connection_edges(&self) -> impl Iterator<Item = (&str, &str)> {
        let synthesized = self
            .synthesized_connections
            .iter()
            .map(|(source, targets)| (&**source, targets.as_slice()));
        self.graph_edge_groups()
            .chain(synthesized)
            .flat_map(|(source, targets)| targets.iter().map(move |target| (source, &**target)))
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
    let lowered = doc.clone();

    let Some(root) = root_composite(doc, by_id, diags) else {
        withheld.emit_unvisited(&HashSet::new(), diags);
        return LoweredCxf::unlowered(lowered, None, HashMap::new(), false);
    };
    let root_iri = Some(root.to_owned());

    reject_unsupported_constructs(doc, specialization, diags);

    let mut walk = Containment::new(by_id, specialization, diags);
    let nesting_exceeded = walk.walk_root(root);
    let Containment {
        leaf_order,
        inherited_scope,
        evaluated_chains,
        ..
    } = walk;
    withheld.emit_unvisited(&evaluated_chains, diags);
    if nesting_exceeded {
        return LoweredCxf::unlowered(lowered, root_iri, inherited_scope, true);
    }

    let boundary = CompositeOrientation::new(doc, by_id, root, specialization);
    let canonical = boundary.canonical_connections(doc, by_id, specialization);
    let (mut rewritten, deferred_diagnostic) = match rewrite_connections(doc, &boundary, &canonical)
    {
        Ok(rewritten) => rewritten,
        Err(diagnostic) => {
            diags.push(diagnostic);
            return LoweredCxf::unlowered(lowered, root_iri, inherited_scope, true);
        }
    };
    if let Some(diagnostic) = deferred_diagnostic {
        diags.push(diagnostic);
    }
    let endpoints = &canonical.endpoints;
    let mut shared = SharedIris::new(endpoints);
    let mut share = |targets: Vec<EndpointIx>| -> Vec<Arc<str>> {
        targets
            .into_iter()
            .map(|target| shared.get(target))
            .collect()
    };
    let synthesized_connections = boundary
        .synthesized_sources()
        .filter_map(|source| {
            let targets = rewritten.remove(&endpoints.lookup(source)?)?;
            Some((Arc::from(source), share(targets)))
        })
        .collect();
    let mut lowered = lowered;
    let mut targets = Vec::with_capacity(lowered.graph.len());
    for node in &mut lowered.graph {
        if node.id == root {
            node.contains_block = refs(leaf_order.clone());
        } else if boundary.composites.contains(node.id.as_str()) {
            node.contains_block = OneOrMany::None;
        }
        // An inactive node keeps its authored edges for Step 9's inactive-node refusal.
        targets.push(if specialization.is_inactive(&node.id) {
            take_authored_targets(node)
        } else {
            node.is_connected_to = OneOrMany::None;
            endpoints
                .lookup(&node.id)
                .and_then(|source| rewritten.get(&source).cloned())
                .map(&mut share)
                .unwrap_or_default()
        });
    }
    LoweredCxf {
        doc: lowered,
        root_iri,
        inherited_scope,
        targets,
        synthesized_connections,
        boundary_traversal_failed: false,
    }
}

/// Move a node's authored `isConnectedTo` targets out of the document as shared IRIs.
fn take_authored_targets(node: &mut Node) -> Vec<Arc<str>> {
    std::mem::take(&mut node.is_connected_to)
        .iter()
        .map(|target| Arc::from(target.id.as_str()))
        .collect()
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

fn rewrite_connections(
    doc: &CxfDocument,
    boundary: &CompositeOrientation,
    canonical: &CanonicalConnections<'_>,
) -> Result<RewrittenConnections, Diagnostic> {
    let endpoints = &canonical.endpoints;
    let mut deferred = Vec::new();
    let mut budget = BoundaryBudget::default();
    let mut reached_boundaries = HashSet::new();
    for source in doc
        .graph
        .iter()
        .map(|node| node.id.as_str())
        .chain(boundary.synthesized_sources())
    {
        let Some(source) = endpoints.lookup(source) else {
            continue;
        };
        let authored_targets = canonical.targets(source);
        if authored_targets.is_empty() {
            continue;
        }
        let source_port = endpoints.get(source);
        if source_port.inactive {
            continue;
        }
        if source_port.synthesized {
            for _ in authored_targets {
                budget.examine_bytes(source_port.iri)?;
            }
        }
        if source_port.elided {
            continue;
        }
        let mut targets = Vec::new();
        for &target in authored_targets {
            resolve_authored_target(
                target,
                canonical,
                &mut budget,
                &mut reached_boundaries,
                &mut targets,
            )?;
        }
        // Lowered lists are NEVER deduplicated: forward+reverse restatements of one relation are
        // already collapsed in the canonical map, so any surviving duplicate is a genuine
        // double-drive that must stay visible to the single-assignment check.
        if canonical.is_crossed(source) {
            targets.sort_by_key(|target| endpoints.get(*target).position);
        }
        if !targets.is_empty() {
            deferred.push((source, targets));
        }
    }
    let preserved_missing_endpoints = deferred
        .iter()
        .flat_map(|(source, targets)| std::iter::once(*source).chain(targets.iter().copied()))
        .filter(|endpoint| {
            let port = endpoints.get(*endpoint);
            !port.node && !port.synthesized
        })
        .collect();
    let deferred_diagnostic = boundary.erased_relation_diagnostic(
        canonical,
        &reached_boundaries,
        &preserved_missing_endpoints,
    );
    Ok((deferred.into_iter().collect(), deferred_diagnostic))
}

/// Lowered targets per canonical source, and the deferred erased-relation diagnostic.
type RewrittenConnections = (HashMap<EndpointIx, Vec<EndpointIx>>, Option<Diagnostic>);

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
        self.examine_bytes(target)?;
        self.examined_targets += 1;
        Ok(())
    }

    fn examine_bytes(&mut self, iri: &str) -> Result<(), Diagnostic> {
        if iri.len() > MAX_COMPOSITE_BOUNDARY_TARGET_BYTES - self.examined_target_bytes {
            return Err(Diagnostic::error(
                DiagCode::MalformedDocument,
                format!(
                    "composite boundary resolution exceeds the supported aggregate target IRI \
                     byte count ({MAX_COMPOSITE_BOUNDARY_TARGET_BYTES})"
                ),
            ));
        }
        self.examined_target_bytes += iri.len();
        Ok(())
    }
}

enum BoundaryFrame {
    Target(EndpointIx),
    Children {
        boundary: EndpointIx,
        next_child: usize,
    },
}

fn resolve_authored_target(
    target: EndpointIx,
    canonical: &CanonicalConnections<'_>,
    budget: &mut BoundaryBudget,
    reached_boundaries: &mut HashSet<EndpointIx>,
    out: &mut Vec<EndpointIx>,
) -> Result<(), Diagnostic> {
    if canonical.endpoints.get(target).elided {
        resolve_target(target, canonical, budget, reached_boundaries, out)
    } else {
        out.push(target);
        Ok(())
    }
}

/// Resolve one target with path-local cycle state and ordered depth-first expansion.
fn resolve_target(
    target: EndpointIx,
    canonical: &CanonicalConnections<'_>,
    budget: &mut BoundaryBudget,
    reached_boundaries: &mut HashSet<EndpointIx>,
    out: &mut Vec<EndpointIx>,
) -> Result<(), Diagnostic> {
    let mut active_path: HashSet<EndpointIx> = HashSet::new();
    let mut frames = vec![BoundaryFrame::Target(target)];
    while let Some(frame) = frames.pop() {
        let target = match frame {
            BoundaryFrame::Target(target) => target,
            BoundaryFrame::Children {
                boundary,
                next_child,
            } => {
                let Some(&child) = canonical.targets(boundary).get(next_child) else {
                    active_path.remove(&boundary);
                    continue;
                };
                frames.push(BoundaryFrame::Children {
                    boundary,
                    next_child: next_child + 1,
                });
                frames.push(BoundaryFrame::Target(child));
                continue;
            }
        };
        let port = canonical.endpoints.get(target);

        budget.examine(port.iri)?;
        if port.inactive {
            out.push(target);
            continue;
        }
        if !port.elided {
            out.push(target);
            continue;
        }

        // Preserve authored boundary-cycle and missing-node visibility before applying the resource
        // bound: both continue through the resolver's ordinary dangling-reference diagnostic.
        if active_path.contains(&target) || !port.node {
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

        reached_boundaries.insert(target);
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
        let specialization = Specialization::default();
        let boundary = CompositeOrientation::default();
        let mut endpoints = super::super::composite_orientation::LoweringEndpoints::default();
        let target = endpoints.intern(
            "http://example.org#leaf.u",
            &boundary,
            &by_id,
            &specialization,
        );
        let canonical =
            CanonicalConnections::new(endpoints, Vec::new(), vec![Vec::new()], HashSet::new());
        let mut budget = BoundaryBudget {
            examined_targets: MAX_COMPOSITE_BOUNDARY_TARGETS,
            examined_target_bytes: MAX_COMPOSITE_BOUNDARY_TARGET_BYTES,
        };
        let mut out = Vec::new();
        let mut reached_boundaries = HashSet::new();
        resolve_authored_target(
            target,
            &canonical,
            &mut budget,
            &mut reached_boundaries,
            &mut out,
        )
        .expect("ordinary wiring is outside boundary budgets");
        assert_eq!(out, [target]);
        assert_eq!(budget.examined_targets, MAX_COMPOSITE_BOUNDARY_TARGETS);
        assert_eq!(
            budget.examined_target_bytes,
            MAX_COMPOSITE_BOUNDARY_TARGET_BYTES
        );
    }
}
