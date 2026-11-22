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
//!   diagnostic per distinct cycle **per chain evaluation**. The lowering walk evaluates each
//!   composite's chain at most once (a second `containsBlock` reach is refused as
//!   `shared-instance`, issue #365), so at the lowering invocation that is one diagnostic per
//!   cycle per composite. Subject = the participant
//!   earliest in chained order; the message's arrow list is the participant *ring* in chained
//!   declaration order closing on the first — not the discovered edge path. Cycle members are
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
//! from the lowering view when both passes see it. Tagged rules apply only to COMPOSITE
//! own-chains at the specialize invocation (R20-9): a leaf's chain — whose values are member
//! modifications, a different level — still evaluates there so guards can ground, but records
//! no tagged findings; its cycle/duplicate participants simply fail to ground, and the leaf's
//! members are grounded (and diagnosed) at the fenced member level.
//!
//! Each grounded declaration is held in the type its `S231:isOfDataType` declares before it enters
//! the scope (`real_parameters::hold_in_declared_type`, issue #735): an Integer on a declared Real
//! is that Real to every sibling, child and guard that reads it, and a value of another kind than
//! a declared Real, Integer or Boolean is refused on the declaration.
//!
//! Dependency edges come from outer-scope identifier references in parsed `Expr` bindings (see
//! [`identifier_heads`]); non-`Expr` values contribute no edges. Everything here is deterministic:
//! iteration is always in chained declaration order, topological ties break toward the smallest
//! chained index, and no map iteration order feeds an output.
//!
//! Cost is bounded for hostile chains: the topological order is a heap-driven Kahn sort
//! (`O(d log d + edges)` for `d` declarations), own-name lookups are hash-indexed, and the
//! enclosing scope is a shared [`ScopeFrame`] chain rather than a copied entry list.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_expr::{EvalResult, ExprAst, Scope};
use oce_model::{EnumClassId, Value, enum_class_id, enum_member_ordinal};

use crate::document_groundings::DocumentGroundings;
use crate::dto::{CxfValue, Node};
use crate::ground::ground_failure;
use crate::scope_chain::{IndexedEntries, ScopeFrame};
use oce_diag::DiagnosticList;

use super::composite_rules::{ARRAY_PARAMETER, DECLARATION_CYCLE, DUPLICATE_DECLARATION};
use super::enumeration_parameters::validate_enumeration_parameter_value;
use super::local_name;
use super::parameter_attrs::CheckedParameters;
use super::real_parameters::{hold_in_declared_type, in_catalog_type};
use super::specialize::Specialization;

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
///   rules are returned in [`Evaluation::withheld`] for post-lowering reconciliation — for
///   composite chains only (R20-9; a leaf chain records no tagged findings at all).
pub(super) enum Pass<'a> {
    /// The pre-lowering composite walk (`collect_leaves` → `composite_scope`).
    Lowering {
        /// The completed specialization whose `is_inactive` filter excludes pruned declarations.
        specialization: &'a Specialization,
        /// The import's shared diagnostic vector; every finding of this pass emits into it.
        diags: &'a mut DiagnosticList,
        /// The parameter nodes whose descriptive attributes this resolve already checked, shared
        /// with leaf grounding so a node read twice is reported once (issue #559).
        checked: &'a mut CheckedParameters,
    },
    /// The conditional-guard specialization pass (`complete_scope`).
    Specialize {
        /// Whether the evaluated chain belongs to a runtime composite. Tagged rules apply only
        /// to composite own-chains at this invocation (R20-9): a leaf chain still evaluates —
        /// guards must ground — but its cycle/duplicate participants are refused silently, so
        /// they simply fail to ground and nothing lands in [`Evaluation::withheld`].
        composite_chain: bool,
        /// A leaf chain's bridged class path, `None` for a composite chain. Each grounded member
        /// of a leaf is held in the type that class's catalog gives it before a sibling or a
        /// guard reads it (issue #759), as resolver Step 7 holds it for the parameter table, so
        /// a guard over `2` and over `2.0` on a Real member reads one Real. An unregistered
        /// class types nothing, and nothing moves.
        leaf_class: Option<&'a str>,
    },
}

/// The outcome of one own-declaration chain evaluation.
pub(super) struct Evaluation<'a> {
    /// The enclosing frame extended by the chain's grounded own entries **in chained
    /// declaration order** (not evaluation order). Own names are unique (duplicates refuse), so
    /// the own region's internal order can never change a name-lookup result. Shared by `Arc`
    /// with every consumer (child composites and leaves).
    pub(super) scope: Arc<ScopeFrame>,
    /// Tagged contract findings withheld for reconciliation. Always empty for
    /// [`Pass::Lowering`] (it emits directly); for [`Pass::Specialize`] it carries the
    /// `declaration-cycle` / `duplicate-declaration` findings of this chain.
    pub(super) withheld: Vec<TaggedFinding<'a>>,
}

/// One tagged contract finding of a chain, held as the document's own strings.
///
/// A finding is kept this way until it is certain to be reported, and its diagnostic is built
/// only then ([`TaggedFinding::diagnostic`]): a chain may re-bind one name any number of times,
/// and every such finding names the first declaration, so built diagnostics would hold
/// `findings × IRI bytes` where these hold three references each (GitHub issue #695).
#[derive(Debug, PartialEq, Eq)]
pub(super) enum TaggedFinding<'a> {
    /// `iri` re-binds the local name `name`, which `first` declared before it.
    Duplicate {
        /// The later declaration, refused.
        iri: &'a str,
        /// The local name both declare.
        name: &'a str,
        /// The first declaration of the name.
        first: &'a str,
    },
    /// The declarations of one reference cycle, in chained order.
    Cycle {
        /// The participants' IRIs, the earliest first.
        participants: Vec<&'a str>,
    },
}

impl TaggedFinding<'_> {
    /// The finding's diagnostic: `composite/duplicate-declaration` on the later declaration, or
    /// `composite/declaration-cycle` on the earliest participant, its path closing on that one.
    pub(super) fn diagnostic(&self) -> Diagnostic {
        match self {
            Self::Duplicate { iri, name, first } => Diagnostic::error(
                DUPLICATE_DECLARATION.code,
                DUPLICATE_DECLARATION.message(format!(
                    "own declaration {iri} re-binds local name `{name}` first declared at {first}"
                )),
            )
            .with_subject((*iri).to_owned()),
            Self::Cycle { participants } => {
                let earliest = participants.first().copied().unwrap_or_default();
                let mut path = participants.clone();
                path.push(earliest);
                Diagnostic::error(
                    DECLARATION_CYCLE.code,
                    DECLARATION_CYCLE.message(format!(
                        "cycle in the block's own declaration references: {}",
                        path.join(" -> ")
                    )),
                )
                .with_subject(earliest.to_owned())
            }
        }
    }
}

/// Tagged contract findings computed by the specialize pass, withheld until after lowering.
///
/// The two passes may both evaluate one chain; each chain's tagged findings must surface once
/// per import, from the lowering view when both passes visit it. The specialize pass therefore
/// records its findings here instead of emitting, and [`WithheldFindings::emit_unvisited`]
/// releases only the chains the lowering pass never evaluated. Chains are recorded in `@graph`
/// order (the specialize pass's iteration order), so release order is deterministic.
///
/// What is withheld is each finding's [`TaggedFinding`], references into the document, so the
/// store grows with the number of findings and never with the length of an IRI; a finding's
/// diagnostic is built when it is released, while the import's list has room for it.
#[derive(Default)]
pub(super) struct WithheldFindings<'a> {
    by_chain: Vec<(&'a str, Vec<TaggedFinding<'a>>)>,
}

impl<'a> WithheldFindings<'a> {
    /// Record one chain's withheld tagged findings (no-op when there are none).
    pub(super) fn record(&mut self, chain: &'a str, findings: Vec<TaggedFinding<'a>>) {
        if !findings.is_empty() {
            self.by_chain.push((chain, findings));
        }
    }

    /// Emit the findings of every chain NOT in `evaluated` (the composite ids whose chains the
    /// lowering pass evaluated itself) into the shared diagnostic list.
    pub(super) fn emit_unvisited(self, evaluated: &HashSet<String>, diags: &mut DiagnosticList) {
        for (chain, findings) in self.by_chain {
            if !evaluated.contains(chain) {
                for finding in findings {
                    diags.raise(|| finding.diagnostic());
                }
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
/// `appended` joins the chain after `hasParameter` ⧺ `hasConstant` as full members — subject
/// to the same duplicate refusal, dependency edges, and cycle refusal — and exists for one
/// caller: a derivation-shaped parent's value-carrying `hasInstance` members at the specialize
/// invocation (`_spec/19` R19-15 site 3), pre-sorted by the caller. Every other caller passes
/// an empty slice.
///
/// Returns the extended scope entries plus (for the specialize pass) the withheld tagged
/// findings. Total and panic-free on any input document.
///
/// `groundings` is the pass's [`DocumentGroundings`]: a declaration whose node other owners list
/// too is parsed for its dependencies and grounded once for the document when it reads no name
/// (issue #749).
pub(super) fn evaluate_declarations<'a>(
    node: &'a Node,
    appended: &[&'a str],
    enclosing: &Arc<ScopeFrame>,
    by_id: &HashMap<&str, &'a Node>,
    groundings: &mut DocumentGroundings,
    mut pass: Pass<'_>,
) -> Evaluation<'a> {
    let mut withheld: Vec<TaggedFinding<'a>> = Vec::new();

    // 1. Collect the active chain in chained declaration order (parameters, then constants,
    // then the caller's appendix).
    let mut chain: Vec<Declaration<'_>> = Vec::new();
    for piri in node
        .has_parameter
        .iter()
        .chain(node.has_constant.iter())
        .map(|r| r.id.as_str())
        .chain(appended.iter().copied())
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
                let finding = TaggedFinding::Duplicate {
                    iri: chain[index].iri,
                    name: chain[index].name,
                    first: chain[first].iri,
                };
                record_tagged(&mut pass, &mut withheld, finding);
            }
        }
    }

    // 3. Dependency edges: parsed outer-scope identifiers in an `Expr` RHS that exactly match a
    // sibling local name. A declaration that can never ground contributes no outgoing edges — at
    // the lowering invocation that includes array-flagged declarations, which the array-parameter
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
        let Some(value @ CxfValue::Expr(text)) = &pnode.value else {
            continue;
        };
        let mut references: Vec<usize> = groundings
            .identifier_heads(pnode, value, text)
            .iter()
            .filter_map(|head| first_of_name.get(head.as_str()).copied())
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
        let finding = TaggedFinding::Cycle {
            participants: participants.iter().map(|&i| chain[i].iri).collect(),
        };
        record_tagged(&mut pass, &mut withheld, finding);
        for index in participants {
            chain[index].refused = true;
        }
    }

    // 5. Evaluate the surviving declarations topologically, ties broken toward the smallest
    // chained index: a Kahn sort whose ready set is a min-heap of chained indices. Every own
    // name — grounded, pending, failed, or refused (a cycle member or duplicate) — masks a
    // same-named enclosing binding for sibling RHS resolution; grounded entries are keyed by
    // chained index so the final scope region lies in chained declaration order. A reference to
    // a refused declaration is already satisfied (it will never ground); every cycle was refused
    // above, so the sort drains every surviving declaration.
    let own_names: HashSet<&str> = chain.iter().map(|d| d.name).collect();
    let mut grounded = OwnGrounded::default();
    let mut unmet = vec![0usize; chain.len()];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); chain.len()];
    let mut ready: BinaryHeap<Reverse<usize>> = BinaryHeap::new();
    for (index, declaration) in chain.iter().enumerate() {
        if declaration.refused {
            continue;
        }
        for &reference in &declaration.references {
            if !chain[reference].refused {
                unmet[index] += 1;
                dependents[reference].push(index);
            }
        }
        if unmet[index] == 0 {
            ready.push(Reverse(index));
        }
    }
    while let Some(Reverse(index)) = ready.pop() {
        evaluate_one(
            &chain[index],
            &node.id,
            index,
            enclosing,
            &own_names,
            &mut grounded,
            groundings,
            &mut pass,
        );
        for &dependent in &dependents[index] {
            unmet[dependent] -= 1;
            if unmet[dependent] == 0 {
                ready.push(Reverse(dependent));
            }
        }
    }
    let mut own = grounded.entries;
    own.sort_unstable_by_key(|&(index, ..)| index);
    let own: IndexedEntries = own
        .into_iter()
        .map(|(_, name, value)| (name, value))
        .collect();
    Evaluation {
        scope: ScopeFrame::extend(enclosing, own),
        withheld,
    }
}

/// The chain's grounded own entries in evaluation order, keyed by chained index, with a name
/// index for own-first lookups (own names are unique after the duplicate refusal).
#[derive(Default)]
struct OwnGrounded {
    entries: Vec<(usize, Arc<str>, EvalResult)>,
    by_name: HashMap<Arc<str>, usize>,
}

impl OwnGrounded {
    fn push(&mut self, index: usize, name: Arc<str>, value: EvalResult) {
        self.by_name
            .entry(Arc::clone(&name))
            .or_insert(self.entries.len());
        self.entries.push((index, name, value));
    }

    fn lookup(&self, name: &str) -> Option<&EvalResult> {
        self.by_name
            .get(name)
            .and_then(|&position| self.entries.get(position))
            .map(|(_, _, value)| value)
    }
}

/// Record one tagged contract finding per the pass's emission policy: the lowering invocation
/// emits directly; the specialize invocation withholds it for post-lowering reconciliation on a
/// composite chain and records nothing for a leaf chain (R20-9 — the refused declarations still
/// fail to ground there, silently). The finding's diagnostic is built at lowering only while the
/// list has room for it, and for a withheld finding not before it is released.
fn record_tagged<'a>(
    pass: &mut Pass<'_>,
    withheld: &mut Vec<TaggedFinding<'a>>,
    finding: TaggedFinding<'a>,
) {
    match pass {
        Pass::Lowering { diags, .. } => diags.raise(|| finding.diagnostic()),
        Pass::Specialize {
            composite_chain: true,
            ..
        } => withheld.push(finding),
        Pass::Specialize {
            composite_chain: false,
            ..
        } => {}
    }
}

/// Push a generic (untagged) diagnostic — emitting at the lowering invocation only; the
/// specialize pass's generic machinery is non-emitting everywhere.
fn emit_generic(pass: &mut Pass<'_>, diag: Diagnostic) {
    if let Pass::Lowering { diags, .. } = pass {
        diags.push(diag);
    }
}

/// Validate and ground one surviving declaration against the own-first scope view, recording
/// the grounded entry or reporting the failure per the pass's emission policy. A value the
/// pass's other owners share is grounded once through `groundings` (issue #749).
#[allow(clippy::too_many_arguments)] // The chain's state, the scope it grounds under and the two
// accumulators; a one-shot struct bundling them would only rename the arguments.
fn evaluate_one(
    declaration: &Declaration<'_>,
    owner: &str,
    index: usize,
    enclosing: &ScopeFrame,
    own_names: &HashSet<&str>,
    grounded: &mut OwnGrounded,
    groundings: &mut DocumentGroundings,
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
    if let Pass::Lowering { diags, checked, .. } = pass {
        checked.refuse_malformed(pnode, declaration.iri, diags);
    }
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
            validate_enumeration_parameter_value(pnode, cxf_val, &scope, diags);
        }
        groundings.ground(pnode, cxf_val, &scope)
    };
    match &*outcome {
        Ok(value) => {
            let value = value.clone();
            // Held in its declared type before any sibling, child or guard reads it (issue
            // #735): the Real of an Integer on a declared Real. A value of another kind is
            // refused on the declaration, at the lowering invocation only (generic machinery).
            let value = match hold_in_declared_type(pnode, value) {
                Ok(value) => value,
                Err((fault, value)) => {
                    let diagnostic = fault.diagnostic(declaration.iri, declaration.name, &value);
                    emit_generic(pass, diagnostic);
                    value
                }
            };
            // A leaf's member is held in its catalog type too (issue #759). An Integer past
            // 2^53 stays as grounded: Step 7 refuses it on the same binding, so the document is
            // refused whatever the guard reads.
            let value = match pass {
                Pass::Specialize {
                    leaf_class: Some(class_path),
                    ..
                } => in_catalog_type(class_path, declaration.name, value)
                    .unwrap_or_else(Value::Integer),
                _ => value,
            };
            grounded.push(
                index,
                Arc::from(declaration.name),
                EvalResult::Scalar(value),
            );
        }
        Err(e) => emit_generic(pass, ground_failure(e, declaration.iri, owner)),
    }
}

/// Own-first name resolution for a declaration RHS: an own local name always denotes the own
/// declaration (grounded or not — an ungrounded own name resolves to nothing rather than
/// falling through), and only names with no own binding read the enclosing entries, innermost
/// (latest-pushed) first. Lookup-only and total.
struct OwnDeclarationScope<'a> {
    enclosing: &'a ScopeFrame,
    own_names: &'a HashSet<&'a str>,
    grounded: &'a OwnGrounded,
}

impl Scope for OwnDeclarationScope<'_> {
    fn lookup(&self, name: &str) -> Option<&EvalResult> {
        if self.own_names.contains(name) {
            return self.grounded.lookup(name);
        }
        self.enclosing.lookup(name)
    }

    fn enum_class(&self, qualified: &str) -> Option<EnumClassId> {
        enum_class_id(qualified)
    }

    fn enum_ordinal(&self, class: EnumClassId, literal: &str) -> Option<u32> {
        enum_member_ordinal(class, literal)
    }
}

/// The outer-scope identifier references of an expression string.
///
/// Parsing applies the evaluator's real classification: literals, builtins, named constants, and
/// qualified enum references never become dependencies. A comprehension source uses the enclosing
/// scope; its iterator shadows the body. A multi-iterator comprehension contributes no edges because
/// evaluation rejects its clause count before reading any source or body. An expression that cannot
/// parse also contributes no edges because it cannot reach evaluation. Total; never panics.
pub(crate) fn identifier_heads(text: &str) -> Vec<String> {
    let Ok(expression) = oce_expr::parse(text) else {
        return Vec::new();
    };
    let mut heads = Vec::new();
    collect_identifier_heads(&expression, &mut Vec::new(), &mut heads);
    heads
}

fn collect_identifier_heads(
    expression: &ExprAst,
    bound: &mut Vec<Arc<str>>,
    heads: &mut Vec<String>,
) {
    match expression {
        ExprAst::Ident(name) => {
            if !bound.iter().any(|entry| entry == name) {
                heads.push(name.to_string());
            }
        }
        ExprAst::Real(_)
        | ExprAst::Int(_)
        | ExprAst::Bool(_)
        | ExprAst::Str(_)
        | ExprAst::Const(_)
        | ExprAst::EnumRef(_) => {}
        ExprAst::Unary(_, value) => collect_identifier_heads(value, bound, heads),
        ExprAst::Binary(_, left, right) => {
            // A left-associative chain (`a+b+…`) may be thousands of terms long, so its left
            // spine is walked iteratively; operands are still visited in source order.
            let mut rights = vec![right.as_ref()];
            let mut leftmost = left.as_ref();
            while let ExprAst::Binary(_, left, right) = leftmost {
                rights.push(right);
                leftmost = left;
            }
            collect_identifier_heads(leftmost, bound, heads);
            for right in rights.into_iter().rev() {
                collect_identifier_heads(right, bound, heads);
            }
        }
        ExprAst::Call(_, args) | ExprAst::ArrayLit(args) => {
            for arg in args {
                collect_identifier_heads(arg, bound, heads);
            }
        }
        ExprAst::Range { start, step, stop } => {
            collect_identifier_heads(start, bound, heads);
            if let Some(step) = step {
                collect_identifier_heads(step, bound, heads);
            }
            collect_identifier_heads(stop, bound, heads);
        }
        ExprAst::Index { base, indices } => {
            collect_identifier_heads(base, bound, heads);
            for index in indices {
                collect_identifier_heads(index, bound, heads);
            }
        }
        ExprAst::Comprehension { body, iters } => {
            // Evaluation rejects this shape before it reads any source or body, so no scope lookup
            // from the subtree can affect declaration order.
            let [(name, source)] = iters.as_slice() else {
                return;
            };
            collect_identifier_heads(source, bound, heads);
            bound.push(name.clone());
            collect_identifier_heads(body, bound, heads);
            bound.pop();
        }
        // `ExprAst` is non-exhaustive across the crate boundary. Any future child-bearing variant
        // must join an arm above so its free identifiers participate in declaration ordering.
        _ => {}
    }
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
