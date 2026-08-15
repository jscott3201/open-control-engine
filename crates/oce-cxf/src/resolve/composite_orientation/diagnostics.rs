//! Diagnostic ownership for relations removed during composite-boundary lowering.

use super::*;

impl CompositeOrientation {
    /// First active relation that lowering erased without a later generic diagnostic.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::resolve) fn erased_relation_diagnostic(
        &self,
        doc: &CxfDocument,
        by_id: &HashMap<&str, &Node>,
        canonical: &HashMap<&str, Vec<&str>>,
        root: &str,
        specialization: &Specialization,
        reached_boundaries: &HashSet<&str>,
        preserved_missing_endpoints: &HashSet<&str>,
    ) -> Option<Diagnostic> {
        for node in &doc.graph {
            if specialization.is_inactive(&node.id) {
                continue;
            }
            for authored_target in node.is_connected_to.iter().map(|target| target.id.as_str()) {
                let source_is_erased = self.is_elided_boundary_source(&node.id);
                let target_is_inactive = specialization.is_inactive(authored_target);
                let relation_is_erased = source_is_erased
                    || (!target_is_inactive
                        && self.is_elided_boundary_source(authored_target)
                        && by_id.contains_key(authored_target));
                if !relation_is_erased {
                    continue;
                }
                if target_is_inactive {
                    if source_is_erased && !reached_boundaries.contains(node.id.as_str()) {
                        return Some(Diagnostic::error(
                            DiagCode::InactiveConditionalNode,
                            "connection targets an inactive conditional node",
                        ));
                    }
                    continue;
                }
                let (source, target, verdict) =
                    self.canonical_pair(&node.id, authored_target, by_id, root, specialization);
                if matches!(
                    verdict,
                    Verdict::Keep | Verdict::Swap | Verdict::Unknown | Verdict::Untouched
                ) && source_is_erased
                    && !reached_boundaries.contains(node.id.as_str())
                {
                    let missing_source =
                        !by_id.contains_key(source) && !self.synthesized.contains(source);
                    let missing_target =
                        !by_id.contains_key(target) && !self.synthesized.contains(target);
                    if missing_source && !preserved_missing_endpoints.contains(source) {
                        return Some(Diagnostic::error(
                            DiagCode::UnresolvedReference,
                            "connection source not found",
                        ));
                    }
                    if missing_target && !preserved_missing_endpoints.contains(target) {
                        return Some(Diagnostic::error(
                            DiagCode::UnresolvedReference,
                            "connection target not found",
                        ));
                    }
                    if (missing_source && preserved_missing_endpoints.contains(source))
                        || (missing_target && preserved_missing_endpoints.contains(target))
                    {
                        continue;
                    }
                }
                match verdict {
                    Verdict::Contradictory
                        if source_is_erased
                            && reached_boundaries.contains(node.id.as_str())
                            && self.expansion_reaches_generic_refusal(
                                authored_target,
                                Some(Polarity::Source),
                                canonical,
                                by_id,
                                root,
                                specialization,
                            ) => {}
                    Verdict::Contradictory
                        if !source_is_erased
                            && self.flat_polarity(&node.id, root).is_some_and(|polarity| {
                                self.expansion_reaches_generic_refusal(
                                    authored_target,
                                    Some(polarity),
                                    canonical,
                                    by_id,
                                    root,
                                    specialization,
                                )
                            }) => {}
                    Verdict::Contradictory => {
                        return Some(Diagnostic::error(
                            DiagCode::DirectionMismatch,
                            "boundary connection has contradictory endpoint directions",
                        ));
                    }
                    Verdict::SwapBlocked
                        if source_is_erased && reached_boundaries.contains(node.id.as_str()) => {}
                    Verdict::Unknown
                        if !source_is_erased
                            && self.expansion_reaches_generic_refusal(
                                authored_target,
                                None,
                                canonical,
                                by_id,
                                root,
                                specialization,
                            ) => {}
                    Verdict::Unknown
                        if source_is_erased
                            && reached_boundaries.contains(node.id.as_str())
                            && (!self.is_elided_boundary_source(authored_target)
                                || !by_id.contains_key(authored_target)) => {}
                    Verdict::SwapBlocked | Verdict::Unknown | Verdict::Untouched => {
                        return Some(Diagnostic::error(
                            DiagCode::DirectionMismatch,
                            "boundary connection direction cannot be derived",
                        ));
                    }
                    Verdict::Keep | Verdict::Swap => {}
                }
            }
        }
        None
    }

    fn expansion_reaches_generic_refusal(
        &self,
        target: &str,
        required_polarity: Option<Polarity>,
        canonical: &HashMap<&str, Vec<&str>>,
        by_id: &HashMap<&str, &Node>,
        root: &str,
        specialization: &Specialization,
    ) -> bool {
        enum Frame<'a> {
            Target(&'a str),
            Children {
                boundary: &'a str,
                next_child: usize,
            },
        }

        let mut active_path = HashSet::new();
        let mut frames = vec![Frame::Target(target)];
        while let Some(frame) = frames.pop() {
            let Frame::Target(target) = frame else {
                let Frame::Children {
                    boundary,
                    next_child,
                } = frame
                else {
                    unreachable!();
                };
                let Some(child) = canonical
                    .get(boundary)
                    .and_then(|children| children.get(next_child))
                    .copied()
                else {
                    active_path.remove(boundary);
                    continue;
                };
                frames.push(Frame::Children {
                    boundary,
                    next_child: next_child + 1,
                });
                frames.push(Frame::Target(child));
                continue;
            };

            if specialization.is_inactive(target) {
                return true;
            }
            if !self.is_elided_boundary_source(target) {
                return self
                    .flat_polarity(target, root)
                    .is_none_or(|polarity| required_polarity.is_none_or(|want| want == polarity));
            }
            if active_path.contains(target) || !by_id.contains_key(target) {
                return true;
            }
            active_path.insert(target);
            frames.push(Frame::Children {
                boundary: target,
                next_child: 0,
            });
        }
        false
    }
}
