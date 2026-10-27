//! Shared, hash-indexed parameter scopes for CXF grounding.
//!
//! Grounding resolves a binding's identifiers against the parameters already ground around it:
//! the enclosing composites' declaration chains (innermost first) and, for a leaf member, the
//! earlier-declared sibling members. Two properties keep that bounded on hostile input:
//!
//! - **Shared frames.** Each composite's grounded own declarations form one immutable
//!   [`ScopeFrame`] that points at its parent's frame. Every child composite and leaf shares the
//!   frame through an `Arc`, so inherited scope memory is `O(declarations + leaves)`; a flat
//!   `Vec` copied per leaf was `O(declarations × leaves)` (issue #365).
//! - **Indexed lookup.** Every frame and every sibling region keeps a name → position index, so
//!   one lookup is `O(frame depth)` hash probes (frame depth is bounded by the composite nesting
//!   cap) rather than a linear scan of every visible binding.
//!
//! Name resolution is unchanged from the slice scans these types replace: within one region the
//! most recently pushed binding of a name wins; across frames the innermost frame wins. Lookup is
//! read-only and never feeds a model id or vector order. Total and panic-free.

use std::collections::HashMap;
use std::sync::Arc;

use oce_expr::{EvalResult, Scope};
use oce_model::{EnumClassId, enum_class_id, enum_member_ordinal};

/// An append-only list of grounded `(name, value)` bindings with a latest-wins name index.
#[derive(Clone, Debug, Default)]
pub(crate) struct IndexedEntries {
    entries: Vec<(Arc<str>, EvalResult)>,
    /// Position of the most recently pushed binding of each name.
    latest: HashMap<Arc<str>, usize>,
}

impl IndexedEntries {
    /// Append one binding; it shadows any earlier binding of the same name in this region.
    pub(crate) fn push(&mut self, name: Arc<str>, value: EvalResult) {
        self.latest.insert(Arc::clone(&name), self.entries.len());
        self.entries.push((name, value));
    }

    /// The most recently pushed binding of `name`, if any.
    pub(crate) fn lookup(&self, name: &str) -> Option<&EvalResult> {
        self.latest
            .get(name)
            .and_then(|&index| self.entries.get(index))
            .map(|(_, value)| value)
    }
}

impl FromIterator<(Arc<str>, EvalResult)> for IndexedEntries {
    fn from_iter<I: IntoIterator<Item = (Arc<str>, EvalResult)>>(iter: I) -> Self {
        let mut indexed = Self::default();
        for (name, value) in iter {
            indexed.push(name, value);
        }
        indexed
    }
}

/// One composite's grounded own declarations, chained to its enclosing composite's frame.
///
/// Immutable once built and shared through `Arc` by every child composite and leaf. The chain
/// length is bounded by the composite nesting cap (64), because a composite that grounds no
/// binding reuses its parent's frame instead of adding an empty one.
#[derive(Debug, Default)]
pub(crate) struct ScopeFrame {
    parent: Option<Arc<ScopeFrame>>,
    own: IndexedEntries,
}

impl ScopeFrame {
    /// The empty outermost scope.
    pub(crate) fn root() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// `parent` extended by `own` bindings (in chained declaration order). An empty `own` returns
    /// `parent` itself, so frames are added only where bindings are.
    pub(crate) fn extend(parent: &Arc<Self>, own: IndexedEntries) -> Arc<Self> {
        if own.entries.is_empty() {
            return Arc::clone(parent);
        }
        Arc::new(Self {
            parent: Some(Arc::clone(parent)),
            own,
        })
    }

    /// Every binding of the chain laid out flat, outermost frame first and each frame in push
    /// order — the pre-sharing `Vec` layout, for tests that pin chained declaration order.
    #[cfg(test)]
    pub(crate) fn flattened(&self) -> Vec<(Arc<str>, EvalResult)> {
        let mut frames = Vec::new();
        let mut frame = Some(self);
        while let Some(current) = frame {
            frames.push(current);
            frame = current.parent.as_deref();
        }
        frames
            .into_iter()
            .rev()
            .flat_map(|current| current.own.entries.iter().cloned())
            .collect()
    }

    /// Resolve `name` innermost frame first; within a frame the latest binding wins.
    pub(crate) fn lookup(&self, name: &str) -> Option<&EvalResult> {
        let mut frame = Some(self);
        while let Some(current) = frame {
            if let Some(value) = current.own.lookup(name) {
                return Some(value);
            }
            frame = current.parent.as_deref();
        }
        None
    }
}

/// Which region wins when a name is bound both by the enclosing frames and by a sibling member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Precedence {
    /// Enclosing first (issue #239): member VALUE expressions read the enclosing binding.
    EnclosingFirst,
    /// Siblings first — the undivided latest-wins view (siblings were pushed after the enclosing
    /// scope): array DIMENSION expressions read the earlier-grounded sibling.
    SiblingsFirst,
}

/// A read-only [`Scope`] over a leaf instance's enclosing frames plus its already-ground sibling
/// members, built incrementally by the resolver's Step-7 member grounding (a member binding sees
/// only the members grounded *before* it, in declaration order — a forward reference still
/// fails). A composite's *own* declaration chain is a different level: it grounds as one mutual,
/// order-independent scope in the resolver's declaration-scope mechanism (issue #240).
///
/// Two views exist:
/// - [`ParamScope::with_enclosing`] — the split view for member *value* expressions: the
///   enclosing frames are searched first, so an enclosing binding wins over a same-named sibling
///   (issue #239); sibling-only names keep resolving against earlier-declared siblings.
/// - [`ParamScope::undivided`] — the latest-wins view for array *dimension* expressions
///   (`sizeOfDimensions`): siblings, grounded after the enclosing scope, are searched first.
///
/// Split corollary (owner-ruled): a name bound BOTH by a sibling member and the enclosing scope
/// has two readings inside one block — value expressions read the enclosing binding, dimension
/// expressions read the sibling *when the sibling is grounded earlier*: member array order still
/// decides the dimension reading. That divergence is silent on the scalar path; on the array path
/// an element-count divergence refuses (`GroundingFailed`, both counts in the message), while a
/// value divergence with a matching count is silent, exactly like the scalar path.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ParamScope<'a> {
    enclosing: Option<&'a ScopeFrame>,
    siblings: Option<&'a IndexedEntries>,
    precedence: Precedence,
}

impl<'a> ParamScope<'a> {
    /// A scope with no bindings, for literal grounding that never consults a scope.
    pub(crate) const fn empty() -> Self {
        Self {
            enclosing: None,
            siblings: None,
            precedence: Precedence::EnclosingFirst,
        }
    }

    /// The split view: an enclosing binding wins over a same-named sibling.
    pub(crate) fn with_enclosing(enclosing: &'a ScopeFrame, siblings: &'a IndexedEntries) -> Self {
        Self {
            enclosing: Some(enclosing),
            siblings: Some(siblings),
            precedence: Precedence::EnclosingFirst,
        }
    }

    /// The undivided latest-wins view: a sibling binding wins over a same-named enclosing one.
    pub(crate) fn undivided(enclosing: &'a ScopeFrame, siblings: &'a IndexedEntries) -> Self {
        Self {
            enclosing: Some(enclosing),
            siblings: Some(siblings),
            precedence: Precedence::SiblingsFirst,
        }
    }

    fn enclosing_lookup(&self, name: &str) -> Option<&'a EvalResult> {
        self.enclosing.and_then(|frame| frame.lookup(name))
    }

    fn sibling_lookup(&self, name: &str) -> Option<&'a EvalResult> {
        self.siblings.and_then(|siblings| siblings.lookup(name))
    }
}

impl Scope for ParamScope<'_> {
    fn lookup(&self, name: &str) -> Option<&EvalResult> {
        match self.precedence {
            Precedence::EnclosingFirst => self
                .enclosing_lookup(name)
                .or_else(|| self.sibling_lookup(name)),
            Precedence::SiblingsFirst => self
                .sibling_lookup(name)
                .or_else(|| self.enclosing_lookup(name)),
        }
    }

    fn enum_class(&self, qualified: &str) -> Option<EnumClassId> {
        enum_class_id(qualified)
    }

    fn enum_ordinal(&self, class: EnumClassId, literal: &str) -> Option<u32> {
        enum_member_ordinal(class, literal)
    }
}

/// A [`Scope`] over one shared frame chain alone (no sibling region).
impl Scope for ScopeFrame {
    fn lookup(&self, name: &str) -> Option<&EvalResult> {
        ScopeFrame::lookup(self, name)
    }

    fn enum_class(&self, qualified: &str) -> Option<EnumClassId> {
        enum_class_id(qualified)
    }

    fn enum_ordinal(&self, class: EnumClassId, literal: &str) -> Option<u32> {
        enum_member_ordinal(class, literal)
    }
}
