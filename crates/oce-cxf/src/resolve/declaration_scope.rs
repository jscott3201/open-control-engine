//! Order-independent evaluation of a block's own `hasParameter`/`hasConstant` declaration chain
//! (issue #240).
//!
//! A block's own active declarations form ONE mutual scope: parameters first, constants second,
//! in array order — the *chained declaration order* — but every declaration's right-hand side may
//! reference any sibling, earlier or later. Evaluation is dependency-ordered (topological,
//! single-pass, never fixpoint), and both the grounded entries and the emitted diagnostics are
//! invariant under permutation of the two declaration arrays. An own local name always denotes
//! the own declaration inside sibling RHSs — it masks a same-named enclosing binding even when
//! the own declaration itself fails to ground; only names with no own binding fall through to
//! the enclosing entries (innermost first).
//!
//! Two refusal classes are contract rules ([`super::composite_rules`]):
//! - `composite/declaration-cycle` — a reference cycle among siblings, self-loops included; one
//!   diagnostic per distinct cycle, subject = the participant earliest in chained order, message
//!   naming every participant in chained order and closing on the first. Cycle members are
//!   excluded from validation and grounding and absent from the produced scope; declarations
//!   outside a cycle still ground (maximal progress), and a reference *to* a cycle member fails
//!   with the ordinary untagged `GroundingFailed` machinery.
//! - `composite/duplicate-declaration` — one local name declared more than once in one chain;
//!   every occurrence beyond the first (in chained order) refuses and is excluded, the first
//!   occurrence stays a normal declaration.
//!
//! One mechanism, two invocations ([`Pass`]): the pre-lowering composite walk and the
//! conditional-guard specialization pass both build their parameter scopes here — never as two
//! parallel loops. Emission policy differs by pass (see [`Pass`]); the tagged findings the
//! specialize pass computes are withheld in [`WithheldFindings`] and emit only for chains the
//! lowering pass does not itself evaluate, so one import reports each chain's findings once,
//! from the lowering view when both passes see it.
//!
//! Dependency edges come from identifier tokens in `Expr` bindings (see [`identifier_heads`]);
//! non-`Expr` values contribute no edges. Everything here is deterministic: iteration is always
//! in chained declaration order, topological ties break toward the smallest chained index, and
//! no map iteration order feeds an output.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_expr::{EvalResult, Scope};
use oce_model::{EnumClassId, enum_class_id, enum_member_ordinal};

use crate::dto::{CxfValue, Node};
use crate::ground::ground_value;

use super::composite_rules::{ARRAY_PARAMETER, DECLARATION_CYCLE, DUPLICATE_DECLARATION};
use super::local_name;
use super::specialize::{Specialization, validate_g36_parameter_value};

/// Which import pass is invoking the shared own-declaration evaluation.
///
/// The two passes see the same mechanism but differ in filtering and emission:
/// - [`Pass::Lowering`] filters inactive declarations through the completed [`Specialization`],
///   refuses array-flagged declarations (`composite/array-parameter`), runs the G36 value
///   validation, and pushes every diagnostic straight into the shared vector.
/// - [`Pass::Specialize`] runs before any `Specialization` exists, so it applies NO inactive
///   filter and NO array refusal (an array binding simply fails to ground, silently), skips
///   validation, and emits nothing directly: generic machinery (`GroundingFailed`,
///   `UnresolvedReference`, value validation) is non-emitting here, and the two tagged contract
///   rules are returned in [`Evaluation::withheld`] for post-lowering reconciliation.
pub(super) enum Pass<'a> {
    /// The pre-lowering composite walk (`collect_leaves` → `composite_scope`).
    Lowering {
        /// The completed specialization whose `is_inactive` filter excludes pruned declarations.
        specialization: &'a Specialization,
        /// The import's shared diagnostic vector; every finding of this pass emits into it.
        diags: &'a mut Vec<Diagnostic>,
    },
    /// The conditional-guard specialization pass (`complete_scope`).
    Specialize,
}

/// The outcome of one own-declaration chain evaluation.
pub(super) struct Evaluation {
    /// The enclosing entries followed by the chain's grounded own entries **in chained
    /// declaration order** (not evaluation order) — the positional layout every downstream
    /// scope consumer reads. Own names are unique (duplicates refuse), so the own region's
    /// internal order can never change a name-lookup result.
    pub(super) entries: Vec<(Arc<str>, EvalResult)>,
    /// Tagged contract findings withheld for reconciliation. Always empty for
    /// [`Pass::Lowering`] (it emits directly); for [`Pass::Specialize`] it carries the
    /// `declaration-cycle` / `duplicate-declaration` diagnostics of this chain.
    pub(super) withheld: Vec<Diagnostic>,
}

/// Tagged contract findings computed by the specialize pass, withheld until after lowering.
///
/// The two passes may both evaluate one chain; each chain's tagged findings must surface once
/// per import, from the lowering view when both passes visit it. The specialize pass therefore
/// records its findings here instead of emitting, and [`WithheldFindings::emit_unvisited`]
/// releases only the chains the lowering pass never evaluated. Chains are recorded in `@graph`
/// order (the specialize pass's iteration order), so release order is deterministic.
#[derive(Default)]
pub(super) struct WithheldFindings {
    by_chain: Vec<(String, Vec<Diagnostic>)>,
}

impl WithheldFindings {
    /// Record one chain's withheld tagged findings (no-op when there are none).
    pub(super) fn record(&mut self, chain: &str, findings: Vec<Diagnostic>) {
        if !findings.is_empty() {
            self.by_chain.push((chain.to_owned(), findings));
        }
    }

    /// Emit the findings of every chain NOT in `evaluated` (the composite ids whose chains the
    /// lowering pass evaluated itself) into the shared diagnostic vector.
    pub(super) fn emit_unvisited(self, evaluated: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
        for (chain, findings) in self.by_chain {
            if !evaluated.contains(&chain) {
                diags.extend(findings);
            }
        }
    }
}

/// One collected declaration of the chain being evaluated.
struct Declaration<'a> {
    /// The declaration's `@id` as referenced by the chain.
    iri: &'a str,
    /// Its local name (the segment after the last `.`) — the scope binding key.
    name: &'a str,
    /// The declaration node, `None` when the `@id` resolves to no `@graph` node.
    node: Option<&'a Node>,
    /// Chained-order indices of the distinct sibling declarations this RHS references
    /// (sorted ascending; empty for non-`Expr` values and for declarations that never ground).
    references: Vec<usize>,
    /// Whether this declaration was refused (duplicate or cycle member) and is excluded from
    /// validation, grounding, and the produced scope.
    refused: bool,
}

/// Evaluate `node`'s own declaration chain against `enclosing`, per the module contract.
///
/// Returns the extended scope entries plus (for the specialize pass) the withheld tagged
/// findings. Total and panic-free on any input document.
pub(super) fn evaluate_declarations(
    node: &Node,
    enclosing: Vec<(Arc<str>, EvalResult)>,
    by_id: &HashMap<&str, &Node>,
    mut pass: Pass<'_>,
) -> Evaluation {
    let mut withheld: Vec<Diagnostic> = Vec::new();

    // 1. Collect the active chain in chained declaration order (parameters, then constants).
    let mut chain: Vec<Declaration<'_>> = Vec::new();
    for piri in node
        .has_parameter
        .iter()
        .chain(node.has_constant.iter())
        .map(|r| r.id.as_str())
    {
        if let Pass::Lowering { specialization, .. } = &pass
            && specialization.is_inactive(piri)
        {
            continue;
        }
        chain.push(Declaration {
            iri: piri,
            name: local_name(piri),
            node: by_id.get(piri).copied(),
            references: Vec::new(),
            refused: false,
        });
    }

    // 2. Refuse duplicate local names: every occurrence beyond the first (chained order)
    // refuses with the first occurrence named; the first stays a normal declaration.
    let mut first_of_name: HashMap<&str, usize> = HashMap::new();
    for index in 0..chain.len() {
        match first_of_name.get(chain[index].name) {
            None => {
                first_of_name.insert(chain[index].name, index);
            }
            Some(&first) => {
                chain[index].refused = true;
                let diag = Diagnostic::error(
                    DUPLICATE_DECLARATION.code,
                    DUPLICATE_DECLARATION.message(format!(
                        "own declaration {} re-binds local name `{}` first declared at {}",
                        chain[index].iri, chain[index].name, chain[first].iri
                    )),
                )
                .with_subject(chain[index].iri.to_owned());
                match &mut pass {
                    Pass::Lowering { diags, .. } => diags.push(diag),
                    Pass::Specialize => withheld.push(diag),
                }
            }
        }
    }

    // 3. Dependency edges: identifier head tokens of an `Expr` RHS that exactly match a sibling
    // local name. A declaration that can never ground contributes no outgoing edges — at the
    // lowering invocation that includes array-flagged declarations, which the array-parameter
    // rule refuses before grounding (the specialize invocation grounds them as it always has).
    let lowering = matches!(pass, Pass::Lowering { .. });
    for declaration in &mut chain {
        if declaration.refused {
            continue;
        }
        let Some(pnode) = declaration.node else {
            continue;
        };
        if lowering && pnode.is_array == Some(true) {
            continue;
        }
        let Some(CxfValue::Expr(text)) = &pnode.value else {
            continue;
        };
        let mut references: Vec<usize> = identifier_heads(text)
            .into_iter()
            .filter_map(|head| first_of_name.get(head).copied())
            .collect();
        references.sort_unstable();
        references.dedup();
        declaration.references = references;
    }

    // 4. Refuse reference cycles (self-loops included): one diagnostic per distinct strongly
    // connected component with an edge, participants in chained order closing on the first,
    // subject = the earliest participant. Members are excluded from validation and grounding.
    let mut components = strongly_connected_components(&chain);
    components.retain(|component| {
        component.len() > 1 || chain[component[0]].references.contains(&component[0])
    });
    components.sort_unstable_by_key(|component| component.iter().copied().min());
    for component in components {
        let mut participants: Vec<usize> = component;
        participants.sort_unstable();
        let mut path: Vec<&str> = participants.iter().map(|&i| chain[i].iri).collect();
        path.push(chain[participants[0]].iri);
        let diag = Diagnostic::error(
            DECLARATION_CYCLE.code,
            DECLARATION_CYCLE.message(format!(
                "cycle in the block's own declaration references: {}",
                path.join(" -> ")
            )),
        )
        .with_subject(chain[participants[0]].iri.to_owned());
        match &mut pass {
            Pass::Lowering { diags, .. } => diags.push(diag),
            Pass::Specialize => withheld.push(diag),
        }
        for index in participants {
            chain[index].refused = true;
        }
    }

    // 5. Evaluate the surviving declarations topologically, ties broken toward the smallest
    // chained index. Every own name — grounded, pending, or failed — masks a same-named
    // enclosing binding for sibling RHS resolution; grounded entries are keyed by chained index
    // so the final scope region lies in chained declaration order.
    let own_names: HashSet<&str> = chain
        .iter()
        .filter(|d| !d.refused)
        .map(|d| d.name)
        .collect();
    let mut grounded: Vec<(usize, Arc<str>, EvalResult)> = Vec::new();
    let mut remaining: Vec<usize> = Vec::new();
    let mut pending = vec![false; chain.len()];
    for (index, declaration) in chain.iter().enumerate() {
        if !declaration.refused {
            remaining.push(index);
            pending[index] = true;
        }
    }
    while !remaining.is_empty() {
        // The smallest-index declaration whose surviving references are all evaluated; every
        // cycle was refused above, so one always exists and the loop terminates.
        let position = remaining
            .iter()
            .position(|&candidate| {
                chain[candidate]
                    .references
                    .iter()
                    .all(|&reference| chain[reference].refused || !pending[reference])
            })
            .unwrap_or(0);
        let index = remaining.remove(position);
        pending[index] = false;
        evaluate_one(
            &chain[index],
            index,
            &enclosing,
            &own_names,
            &mut grounded,
            &mut pass,
        );
    }
    grounded.sort_unstable_by_key(|&(index, ..)| index);

    let mut entries = enclosing;
    entries.extend(grounded.into_iter().map(|(_, name, value)| (name, value)));
    Evaluation { entries, withheld }
}

/// Push a generic (untagged) diagnostic — emitting at the lowering invocation only; the
/// specialize pass's generic machinery is non-emitting everywhere.
fn emit_generic(pass: &mut Pass<'_>, diag: Diagnostic) {
    if let Pass::Lowering { diags, .. } = pass {
        diags.push(diag);
    }
}

/// Validate and ground one surviving declaration against the own-first scope view, recording
/// the grounded entry or reporting the failure per the pass's emission policy.
fn evaluate_one(
    declaration: &Declaration<'_>,
    index: usize,
    enclosing: &[(Arc<str>, EvalResult)],
    own_names: &HashSet<&str>,
    grounded: &mut Vec<(usize, Arc<str>, EvalResult)>,
    pass: &mut Pass<'_>,
) {
    let Some(pnode) = declaration.node else {
        emit_generic(
            pass,
            Diagnostic::error(DiagCode::UnresolvedReference, "parameter node not found")
                .with_subject(declaration.iri.to_owned()),
        );
        return;
    };
    if matches!(pass, Pass::Lowering { .. }) && pnode.is_array == Some(true) {
        emit_generic(
            pass,
            Diagnostic::error(
                ARRAY_PARAMETER.code,
                ARRAY_PARAMETER.message(
                    "array-valued composite parameters are not supported by this CXF \
                     lowering subset",
                ),
            )
            .with_subject(declaration.iri.to_owned()),
        );
        return;
    }
    let Some(cxf_val) = &pnode.value else {
        emit_generic(
            pass,
            Diagnostic::error(
                DiagCode::GroundingFailed,
                "parameter has no value (Ground mode)",
            )
            .with_subject(declaration.iri.to_owned()),
        );
        return;
    };
    let outcome = {
        let scope = OwnDeclarationScope {
            enclosing,
            own_names,
            grounded,
        };
        if let Pass::Lowering { diags, .. } = pass {
            validate_g36_parameter_value(pnode, cxf_val, &scope, diags);
        }
        ground_value(cxf_val, &scope)
    };
    match outcome {
        Ok(value) => grounded.push((
            index,
            Arc::from(declaration.name),
            EvalResult::Scalar(value),
        )),
        Err(e) => emit_generic(
            pass,
            Diagnostic::error(DiagCode::GroundingFailed, e.to_string())
                .with_subject(declaration.iri.to_owned()),
        ),
    }
}

/// Own-first name resolution for a declaration RHS: an own local name always denotes the own
/// declaration (grounded or not — an ungrounded own name resolves to nothing rather than
/// falling through), and only names with no own binding read the enclosing entries, innermost
/// (latest-pushed) first. Lookup-only and total.
struct OwnDeclarationScope<'a> {
    enclosing: &'a [(Arc<str>, EvalResult)],
    own_names: &'a HashSet<&'a str>,
    grounded: &'a [(usize, Arc<str>, EvalResult)],
}

impl Scope for OwnDeclarationScope<'_> {
    fn lookup(&self, name: &str) -> Option<&EvalResult> {
        if self.own_names.contains(name) {
            return self
                .grounded
                .iter()
                .find(|(_, entry, _)| entry.as_ref() == name)
                .map(|(_, _, value)| value);
        }
        self.enclosing
            .iter()
            .rev()
            .find(|(entry, _)| entry.as_ref() == name)
            .map(|(_, value)| value)
    }

    fn enum_class(&self, qualified: &str) -> Option<EnumClassId> {
        enum_class_id(qualified)
    }

    fn enum_ordinal(&self, class: EnumClassId, literal: &str) -> Option<u32> {
        enum_member_ordinal(class, literal)
    }
}

/// The identifier *head* tokens of an expression string — the census-family tokenizer.
///
/// Criterion: identifiers tokenize after numeric literals, so an exponent suffix never yields a
/// token (`1e-3` contributes nothing); a dotted path (`Types.Mode.occupied`) contributes only
/// its head segment (`Types`); everything else is a candidate token. Known accepted
/// false-positive mode: a sibling name inside a string literal in the expression is tokenized
/// like code — the CXF profile has no string-literal parameter arithmetic, so no fixture
/// exercises it. Total; never panics.
pub(super) fn identifier_heads(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut heads = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_digit() {
            // Numeric literal: digits and dots, then an optional signed exponent — consumed
            // whole so `1e-3` never yields an `e` token.
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                let mut j = i + 1;
                if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j].is_ascii_digit() {
                    i = j;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
        } else if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let head = &text[start..i];
            // A dotted path contributes only its head: consume the trailing `.segment` runs.
            while i + 1 < bytes.len()
                && bytes[i] == b'.'
                && (bytes[i + 1].is_ascii_alphabetic() || bytes[i + 1] == b'_')
            {
                i += 2;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
            }
            heads.push(head);
        } else {
            i += 1;
        }
    }
    heads
}

/// Strongly connected components of the chain's reference graph (iterative Tarjan — no
/// input-driven recursion, so a hostile declaration count cannot exhaust the stack). Refused
/// declarations are skipped; edge targets that were refused are ignored. Deterministic: roots
/// and edges are visited in chained-index order. Component membership is what the caller
/// consumes; it re-sorts participants by chained index itself.
fn strongly_connected_components(chain: &[Declaration<'_>]) -> Vec<Vec<usize>> {
    const UNVISITED: usize = usize::MAX;
    let n = chain.len();
    let mut order = vec![UNVISITED; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut frames: Vec<(usize, usize)> = Vec::new();
    let mut components: Vec<Vec<usize>> = Vec::new();
    let mut next_order = 0usize;

    for root in 0..n {
        if chain[root].refused || order[root] != UNVISITED {
            continue;
        }
        order[root] = next_order;
        low[root] = next_order;
        next_order += 1;
        stack.push(root);
        on_stack[root] = true;
        frames.push((root, 0));
        while let Some(frame) = frames.last_mut() {
            let node = frame.0;
            if frame.1 < chain[node].references.len() {
                let target = chain[node].references[frame.1];
                frame.1 += 1;
                if chain[target].refused {
                    continue;
                }
                if order[target] == UNVISITED {
                    order[target] = next_order;
                    low[target] = next_order;
                    next_order += 1;
                    stack.push(target);
                    on_stack[target] = true;
                    frames.push((target, 0));
                } else if on_stack[target] {
                    low[node] = low[node].min(order[target]);
                }
            } else {
                frames.pop();
                if let Some(parent) = frames.last() {
                    low[parent.0] = low[parent.0].min(low[node]);
                }
                if low[node] == order[node] {
                    let mut component = Vec::new();
                    while let Some(member) = stack.pop() {
                        on_stack[member] = false;
                        component.push(member);
                        if member == node {
                            break;
                        }
                    }
                    components.push(component);
                }
            }
        }
    }
    components
}
