//! Diagnostic ownership for relations removed during composite-boundary lowering.

use super::*;

impl CompositeOrientation {
    /// First active relation that lowering erased without a later generic diagnostic.
    ///
    /// Walks the authored edges in authored order over interned endpoints; a boundary expansion
    /// is answered once per distinct start and polarity, so repeated targets cost nothing extra.
    pub(in crate::resolve) fn erased_relation_diagnostic(
        &self,
        canonical: &CanonicalConnections<'_>,
        reached_boundaries: &HashSet<EndpointIx>,
        preserved_missing_endpoints: &HashSet<EndpointIx>,
    ) -> Option<Diagnostic> {
        let endpoints = &canonical.endpoints;
        let mut expansions = HashMap::new();
        let mut expansion_reaches = |target: EndpointIx, required_polarity: Option<Polarity>| {
            self.memoized_expansion_reaches(canonical, &mut expansions, target, required_polarity)
        };
        for &(authored_source, authored_target) in canonical.authored() {
            let source_port = endpoints.get(authored_source);
            if source_port.inactive {
                continue;
            }
            let target_port = endpoints.get(authored_target);
            let source_is_erased = source_port.elided;
            let target_is_inactive = target_port.inactive;
            let relation_is_erased =
                source_is_erased || (!target_is_inactive && target_port.elided && target_port.node);
            if !relation_is_erased {
                continue;
            }
            let source_reached = reached_boundaries.contains(&authored_source);
            if target_is_inactive {
                if source_is_erased && !source_reached {
                    return Some(Diagnostic::error(
                        DiagCode::InactiveConditionalNode,
                        "connection targets an inactive conditional node",
                    ));
                }
                continue;
            }
            let (source, target, verdict) =
                self.canonical_pair(endpoints, authored_source, authored_target);
            if matches!(
                verdict,
                Verdict::Keep | Verdict::Swap | Verdict::Unknown | Verdict::Untouched
            ) && source_is_erased
                && !source_reached
            {
                let is_missing = |id: EndpointIx| {
                    let port = endpoints.get(id);
                    !port.node && !port.synthesized
                };
                let missing_source = is_missing(source);
                let missing_target = is_missing(target);
                if missing_source && !preserved_missing_endpoints.contains(&source) {
                    return Some(Diagnostic::error(
                        DiagCode::UnresolvedReference,
                        "connection source not found",
                    ));
                }
                if missing_target && !preserved_missing_endpoints.contains(&target) {
                    return Some(Diagnostic::error(
                        DiagCode::UnresolvedReference,
                        "connection target not found",
                    ));
                }
                if (missing_source && preserved_missing_endpoints.contains(&source))
                    || (missing_target && preserved_missing_endpoints.contains(&target))
                {
                    continue;
                }
            }
            match verdict {
                Verdict::Contradictory
                    if source_is_erased
                        && source_reached
                        && expansion_reaches(authored_target, Some(Polarity::Source)) => {}
                Verdict::Contradictory
                    if !source_is_erased
                        && self.flat_polarity(source_port).is_some_and(|polarity| {
                            expansion_reaches(authored_target, Some(polarity))
                        }) => {}
                Verdict::Contradictory => {
                    return Some(Diagnostic::error(
                        DiagCode::DirectionMismatch,
                        "boundary connection has contradictory endpoint directions",
                    ));
                }
                Verdict::SwapBlocked if source_is_erased && source_reached => {}
                Verdict::Unknown
                    if !source_is_erased
                        && match self.flat_polarity(source_port) {
                            Some(polarity) => expansion_reaches(authored_target, Some(polarity)),
                            None if source_port.role.is_none() => {
                                expansion_reaches(authored_target, None)
                            }
                            None => false,
                        } => {}
                Verdict::Unknown
                    if source_is_erased
                        && source_reached
                        && expansion_reaches(authored_target, Some(Polarity::Source)) => {}
                Verdict::SwapBlocked | Verdict::Unknown | Verdict::Untouched => {
                    return Some(Diagnostic::error(
                        DiagCode::DirectionMismatch,
                        "boundary connection direction cannot be derived",
                    ));
                }
                Verdict::Keep | Verdict::Swap => {}
            }
        }
        None
    }

    /// [`Self::expansion_reaches_generic_refusal`] answered once per distinct `(target,
    /// polarity)` and read back from `memo` afterwards; the verdict is the unmemoized one.
    fn memoized_expansion_reaches(
        &self,
        canonical: &CanonicalConnections<'_>,
        memo: &mut HashMap<(EndpointIx, Option<Polarity>), bool>,
        target: EndpointIx,
        required_polarity: Option<Polarity>,
    ) -> bool {
        *memo.entry((target, required_polarity)).or_insert_with(|| {
            self.expansion_reaches_generic_refusal(canonical, target, required_polarity)
        })
    }

    fn expansion_reaches_generic_refusal(
        &self,
        canonical: &CanonicalConnections<'_>,
        target: EndpointIx,
        required_polarity: Option<Polarity>,
    ) -> bool {
        enum Frame {
            Target(EndpointIx),
            Children {
                boundary: EndpointIx,
                next_child: usize,
            },
        }

        let endpoints = &canonical.endpoints;
        let mut active_path = HashSet::new();
        let mut frames = vec![Frame::Target(target)];
        while let Some(frame) = frames.pop() {
            let target = match frame {
                Frame::Target(target) => target,
                Frame::Children {
                    boundary,
                    next_child,
                } => {
                    let Some(&child) = canonical.targets(boundary).get(next_child) else {
                        active_path.remove(&boundary);
                        continue;
                    };
                    frames.push(Frame::Children {
                        boundary,
                        next_child: next_child + 1,
                    });
                    frames.push(Frame::Target(child));
                    continue;
                }
            };
            let port = endpoints.get(target);
            if port.inactive {
                return true;
            }
            if !port.elided {
                if self
                    .flat_polarity(port)
                    .is_none_or(|polarity| required_polarity.is_none_or(|want| want == polarity))
                {
                    return true;
                }
                continue;
            }
            if active_path.contains(&target) || !port.node {
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

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
