//! Load-time specialization helpers for source-pinned G36 profile constructs.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_expr::{BinOp, EvalResult, ExprAst, Scope, UnOp};
use oce_model::{
    EnumClassId, Value, enum_class_id, enum_member_ordinal, g36_enum_class_id,
    g36_integer_constant, is_g36_integer_constant_package, is_g36_type_path,
};

use crate::dto::{CxfValue, IriRef, Node};

use super::composite::is_runtime_composite;
use super::declaration_scope::{Pass, WithheldFindings, evaluate_declarations};
use super::instance_interface::{contains_block_referents, is_derivation_shaped};
use super::local_name;

/// The result of pruning load-time conditional components/connectors.
#[derive(Clone, Debug, Default)]
pub(super) struct Specialization {
    inactive: HashSet<String>,
}

impl Specialization {
    /// Whether a source node was pruned by a false conditional guard.
    pub(super) fn is_inactive(&self, iri: &str) -> bool {
        self.inactive.contains(iri)
    }
}

/// Evaluate source-profile conditional nodes and record the inactive subset.
///
/// Also returns the [`WithheldFindings`] this pass computed over the declaration chains it
/// grounded for guard scopes: tagged contract findings that must surface once per import, from
/// the lowering view when both passes visit a chain — `lower` reconciles and releases them.
///
/// Three sites read `S231:hasInstance`, each scoped to the **derivation-shaped** structural
/// shape and to nothing wider (`_spec/19` R19-15) — a root's list, a runtime composite's list,
/// an orphan's list and a both-carrying node's list are never read here:
/// - the conditional-child scan chains a derivation-shaped parent's member list, so a
///   conditional member is a candidate and its guard is evaluated like any other;
/// - a derivation-shaped parent's value-carrying members join its guard scope through the
///   shared declaration chain ([`complete_scope`]);
/// - a **pruned** derivation-shaped instance's members are marked inactive in a second pass
///   after the propagation loop completes ([`mark_pruned_instance_members`]).
pub(super) fn specialize(
    doc: &crate::dto::CxfDocument,
    by_id: &HashMap<&str, &Node>,
    diags: &mut Vec<Diagnostic>,
) -> (Specialization, WithheldFindings) {
    let mut specialization = Specialization::default();
    let mut withheld = WithheldFindings::default();
    let mut decisions: HashMap<&str, bool> = HashMap::new();
    let contains_referents = contains_block_referents(doc);

    for parent in &doc.graph {
        let derivation_shaped = is_derivation_shaped(parent, &contains_referents);
        let member_list: &[IriRef] = if derivation_shaped {
            parent.has_instance.as_slice()
        } else {
            &[]
        };
        let conditional_children: Vec<&Node> = parent
            .contains_block
            .iter()
            .chain(parent.has_input.iter())
            .chain(parent.has_output.iter())
            .chain(member_list.iter())
            .map(|r| r.id.as_str())
            .filter_map(|child_id| by_id.get(child_id).copied())
            .filter(|child| child.is_conditional == Some(true))
            .collect();
        if conditional_children.is_empty() {
            continue;
        }
        let members = if derivation_shaped {
            valued_member_declarations(parent, by_id)
        } else {
            Vec::new()
        };
        let scope = complete_scope(parent, &members, by_id, &mut withheld);
        for child in conditional_children {
            let active = match decisions.get(child.id.as_str()).copied() {
                Some(active) => active,
                None => {
                    let active = evaluate_guard(child, &scope, diags).unwrap_or(true);
                    decisions.insert(child.id.as_str(), active);
                    active
                }
            };
            if !active {
                mark_inactive(child, by_id, &mut specialization.inactive);
            }
        }
    }

    mark_pruned_instance_members(doc, &contains_referents, &mut specialization);

    (specialization, withheld)
}

/// A derivation-shaped parent's value-carrying members in ASCII-sorted `local_name` order —
/// the extra declarations its guard scope evaluates (R19-15 site 3). Value presence, not
/// classification, is the criterion: the class resolves after this pass runs. Sorted rather
/// than in array order because `hasInstance` array order must decide no value; under the
/// mutual scope the order decides refusal attribution only. Bounded honestly: this sort is
/// stable, so two members sharing one `local_name` tie and fall back to array order. Such a
/// pair is never owner-legal — both members refuse under `colliding-member-identity` — so what
/// array order can still move there is the attribution on an already-refused import, never
/// whether a document is accepted and never a grounded value on one that is.
fn valued_member_declarations<'a>(
    parent: &'a Node,
    by_id: &HashMap<&str, &'a Node>,
) -> Vec<&'a str> {
    let mut members: Vec<&str> = parent
        .has_instance
        .iter()
        .map(|r| r.id.as_str())
        .filter(|iri| by_id.get(iri).is_some_and(|node| node.value.is_some()))
        .collect();
    members.sort_by_key(|iri| local_name(iri));
    members
}

/// Mark a pruned derivation-shaped instance's `hasInstance` members inactive, under reference
/// exclusivity against the FINAL active set (R19-15 site 2): a member is marked only when no
/// node that remains active references it — through a `hasInput`/`hasOutput` list, through
/// `containsBlock`, or as a member of an active derivation-shaped list. An `isConnectedTo`
/// edge is deliberately NOT a reference: connection is not ownership, and counting it would
/// silence the inactive-node checks this traversal feeds. Runs as a second pass after the
/// propagation loop completes, because "remains active" is ambiguous mid-loop; one pass
/// suffices — it marks members, never instances, so no marking can prune another instance and
/// reopen the set.
fn mark_pruned_instance_members(
    doc: &crate::dto::CxfDocument,
    contains_referents: &std::collections::HashSet<&str>,
    specialization: &mut Specialization,
) {
    let mut active_referenced: HashSet<&str> = HashSet::new();
    for node in &doc.graph {
        if specialization.is_inactive(&node.id) {
            continue;
        }
        for r in node
            .has_input
            .iter()
            .chain(node.has_output.iter())
            .chain(node.contains_block.iter())
        {
            active_referenced.insert(r.id.as_str());
        }
        if is_derivation_shaped(node, contains_referents) {
            active_referenced.extend(node.has_instance.iter().map(|r| r.id.as_str()));
        }
    }
    let marks: Vec<String> = doc
        .graph
        .iter()
        .filter(|node| {
            specialization.is_inactive(&node.id) && is_derivation_shaped(node, contains_referents)
        })
        .flat_map(|node| node.has_instance.iter().map(|r| r.id.as_str()))
        .filter(|member| !active_referenced.contains(member))
        .map(str::to_owned)
        .collect();
    specialization.inactive.extend(marks);
}

fn mark_inactive(node: &Node, by_id: &HashMap<&str, &Node>, inactive: &mut HashSet<String>) {
    let mut work = vec![node];
    let mut visited = HashSet::new();
    while let Some(node) = work.pop() {
        if !visited.insert(node.id.clone()) {
            continue;
        }
        inactive.insert(node.id.clone());
        for child in node
            .has_input
            .iter()
            .chain(node.has_output.iter())
            .chain(node.has_parameter.iter())
            .chain(node.has_constant.iter())
        {
            inactive.insert(child.id.clone());
        }
        for child in node.contains_block.iter().map(|r| r.id.as_str()) {
            inactive.insert(child.to_owned());
            if let Some(child_node) = by_id.get(child).copied() {
                work.push(child_node);
            }
        }
    }
}

/// Build the guard-evaluation scope from the parent's own declarations through the shared
/// order-independent mechanism ([`super::declaration_scope`]) at its specialize invocation: no
/// inactive filter (no final [`Specialization`] exists while this pass runs) and non-emitting
/// generic machinery; a COMPOSITE chain's tagged findings are recorded on `withheld` for
/// post-lowering reconciliation, while a leaf chain records none (R20-9 — its cycle/duplicate
/// participants simply fail to ground). Guard-level contracts
/// (`ConditionalGuardUnknownParameter` et al.) are unaffected — they emit from guard
/// evaluation, not from scope construction. `members` is a derivation-shaped parent's
/// value-carrying member appendix (R19-15 site 3), empty everywhere else.
fn complete_scope(
    parent: &Node,
    members: &[&str],
    by_id: &HashMap<&str, &Node>,
    withheld: &mut WithheldFindings,
) -> CompleteScope {
    let evaluation = evaluate_declarations(
        parent,
        members,
        Vec::new(),
        by_id,
        Pass::Specialize {
            composite_chain: is_runtime_composite(parent),
        },
    );
    withheld.record(&parent.id, evaluation.withheld);
    CompleteScope {
        entries: evaluation.entries,
    }
}

/// Emit source-profile diagnostics for G36 enum parameters before generic grounding.
pub(super) fn validate_g36_parameter_value(
    node: &Node,
    value: &CxfValue,
    scope: &dyn Scope,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(type_iri) = node.is_of_data_type.as_ref().map(|iri| iri.id.as_str()) else {
        return;
    };
    if let Some(class) = g36_enum_class_id(type_iri) {
        validate_g36_enum_value(node, type_iri, class, value, scope, diags);
    } else if is_g36_type_path(type_iri) && !is_g36_integer_constant_package(type_iri) {
        diags.push(
            Diagnostic::error(
                DiagCode::UnknownEnumType,
                format!("unknown G36 enum type `{type_iri}`"),
            )
            .with_subject(node.id.clone()),
        );
    }
}

fn validate_g36_enum_value(
    node: &Node,
    type_iri: &str,
    declared: EnumClassId,
    value: &CxfValue,
    scope: &dyn Scope,
    diags: &mut Vec<Diagnostic>,
) {
    match value {
        CxfValue::Expr(text) => {
            validate_g36_enum_expr(node, type_iri, declared, text, scope, diags)
        }
        CxfValue::Int(_) | CxfValue::Float(_) | CxfValue::Typed { .. } => diags.push(
            Diagnostic::error(
                DiagCode::EnumIntegerStandin,
                "G36 enum parameters require canonical enum literals, not numeric stand-ins",
            )
            .with_subject(node.id.clone()),
        ),
        CxfValue::Bool(_) | CxfValue::List(_) => diags.push(
            Diagnostic::error(
                DiagCode::UnknownEnumLiteral,
                "G36 enum parameter value is not a canonical enum literal",
            )
            .with_subject(node.id.clone()),
        ),
    }
}

fn validate_g36_enum_expr(
    node: &Node,
    type_iri: &str,
    declared: EnumClassId,
    text: &str,
    scope: &dyn Scope,
    diags: &mut Vec<Diagnostic>,
) {
    if g36_integer_constant(text).is_some() {
        diags.push(
            Diagnostic::error(
                DiagCode::EnumIntegerStandin,
                "G36 integer constants cannot stand in for enum literals",
            )
            .with_subject(node.id.clone()),
        );
        return;
    }
    let Some((class_path, literal)) = text.rsplit_once('.') else {
        if let Some(EvalResult::Scalar(Value::Enum { class, .. })) = scope.lookup(text) {
            if *class == declared {
                return;
            }
            diags.push(
                Diagnostic::error(
                    DiagCode::TypeMismatch,
                    format!(
                        "G36 enum parameter reference `{text}` does not match declared type `{type_iri}`"
                    ),
                )
                .with_subject(node.id.clone()),
            );
            return;
        }
        diags.push(
            Diagnostic::error(
                DiagCode::UnknownEnumLiteral,
                format!("G36 enum literal `{text}` is not fully qualified"),
            )
            .with_subject(node.id.clone()),
        );
        return;
    };
    let Some(actual) = g36_enum_class_id(class_path) else {
        let code = if is_g36_type_path(class_path) {
            DiagCode::UnknownEnumType
        } else {
            DiagCode::TypeMismatch
        };
        diags.push(
            Diagnostic::error(code, format!("`{text}` is not a literal of `{type_iri}`"))
                .with_subject(node.id.clone()),
        );
        return;
    };
    if actual != declared {
        diags.push(
            Diagnostic::error(
                DiagCode::TypeMismatch,
                format!("G36 enum literal `{text}` does not match declared type `{type_iri}`"),
            )
            .with_subject(node.id.clone()),
        );
        return;
    }
    if enum_member_ordinal(actual, literal).is_none() {
        diags.push(
            Diagnostic::error(
                DiagCode::UnknownEnumLiteral,
                format!("unknown G36 enum literal `{text}`"),
            )
            .with_subject(node.id.clone()),
        );
    }
}

fn evaluate_guard(node: &Node, scope: &CompleteScope, diags: &mut Vec<Diagnostic>) -> Option<bool> {
    let Some(expr) = node.cond_expr.as_deref() else {
        diags.push(
            Diagnostic::error(
                DiagCode::ConditionalGuardUnsupported,
                "conditional node is missing S231:conditionalExpression",
            )
            .with_subject(node.id.clone()),
        );
        return None;
    };
    let normalized = normalize_guard_expr(expr);
    let ast = match oce_expr::parse(&normalized) {
        Ok(ast) => ast,
        Err(e) => {
            diags.push(
                Diagnostic::error(
                    DiagCode::ConditionalGuardUnsupported,
                    format!("conditional guard did not parse: {e}"),
                )
                .with_subject(node.id.clone()),
            );
            return None;
        }
    };
    if !validate_guard_ast(&ast, scope, node, diags) {
        return None;
    }
    match oce_expr::eval(&ast, scope) {
        Ok(EvalResult::Scalar(Value::Boolean(value))) => Some(value),
        Ok(_) => {
            diags.push(
                Diagnostic::error(
                    DiagCode::ConditionalGuardUnsupported,
                    "conditional guard did not evaluate to Boolean",
                )
                .with_subject(node.id.clone()),
            );
            None
        }
        Err(e) => {
            diags.push(
                Diagnostic::error(
                    DiagCode::ConditionalGuardUnsupported,
                    format!("conditional guard failed to evaluate: {e}"),
                )
                .with_subject(node.id.clone()),
            );
            None
        }
    }
}

fn normalize_guard_expr(expr: &str) -> String {
    let mut out = String::with_capacity(expr.len());
    let mut chars = expr.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '!' {
            if chars.peek() == Some(&'=') {
                chars.next();
                out.push_str("<>");
            } else {
                out.push_str("not ");
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn validate_guard_ast(
    ast: &ExprAst,
    scope: &CompleteScope,
    node: &Node,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    match ast {
        ExprAst::Ident(name) => validate_boolean_param(name, scope, node, diags),
        ExprAst::Unary(UnOp::Not, inner) => match inner.as_ref() {
            ExprAst::Ident(name) => validate_boolean_param(name, scope, node, diags),
            _ => unsupported(node, "only `!parameter` is allowed in G36 guards", diags),
        },
        ExprAst::Binary(BinOp::And | BinOp::Or, left, right) => {
            let l_ok = validate_guard_ast(left, scope, node, diags);
            let r_ok = validate_guard_ast(right, scope, node, diags);
            l_ok && r_ok
        }
        ExprAst::Binary(BinOp::Eq | BinOp::Ne, left, right) => {
            validate_enum_comparison(left, right, scope, node, diags)
        }
        _ => unsupported(
            node,
            "guard uses functions, arithmetic, runtime references, or literals outside the profile",
            diags,
        ),
    }
}

fn validate_boolean_param(
    name: &str,
    scope: &CompleteScope,
    node: &Node,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    match scope.value(name) {
        Some(Value::Boolean(_)) => true,
        Some(_) => unsupported(node, "bare guard parameter must be Boolean", diags),
        None => unknown_param(node, name, diags),
    }
}

fn validate_enum_comparison(
    left: &ExprAst,
    right: &ExprAst,
    scope: &CompleteScope,
    node: &Node,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let ExprAst::Ident(param) = left else {
        return unsupported(
            node,
            "G36 enum comparisons must be `parameter == Enum.literal`",
            diags,
        );
    };
    let Some(param_value) = scope.value(param) else {
        return unknown_param(node, param, diags);
    };
    let Value::Enum {
        class: param_class, ..
    } = param_value
    else {
        return unsupported(node, "G36 enum comparison parameter is not an enum", diags);
    };
    let ExprAst::EnumRef(reference) = right else {
        return unsupported(
            node,
            "G36 enum comparison requires a fully-qualified enum literal",
            diags,
        );
    };
    let Some((class_path, literal)) = reference.rsplit_once('.') else {
        diags.push(
            Diagnostic::error(
                DiagCode::UnknownEnumLiteral,
                format!("G36 enum literal `{reference}` is not fully qualified"),
            )
            .with_subject(node.id.clone()),
        );
        return false;
    };
    let Some(reference_class) = enum_class_id(class_path) else {
        let code = if is_g36_type_path(class_path) {
            DiagCode::UnknownEnumType
        } else {
            DiagCode::ConditionalGuardUnsupported
        };
        diags.push(
            Diagnostic::error(code, format!("unsupported guard enum type `{class_path}`"))
                .with_subject(node.id.clone()),
        );
        return false;
    };
    if enum_member_ordinal(reference_class, literal).is_none() {
        diags.push(
            Diagnostic::error(
                DiagCode::UnknownEnumLiteral,
                format!("unknown guard enum literal `{reference}`"),
            )
            .with_subject(node.id.clone()),
        );
        return false;
    }
    if *param_class != reference_class {
        return unsupported(node, "guard compares different enum classes", diags);
    }
    true
}

fn unknown_param(node: &Node, name: &str, diags: &mut Vec<Diagnostic>) -> bool {
    diags.push(
        Diagnostic::error(
            DiagCode::ConditionalGuardUnknownParameter,
            format!("conditional guard references unknown parameter `{name}`"),
        )
        .with_subject(node.id.clone()),
    );
    false
}

fn unsupported(node: &Node, message: &str, diags: &mut Vec<Diagnostic>) -> bool {
    diags.push(
        Diagnostic::error(DiagCode::ConditionalGuardUnsupported, message)
            .with_subject(node.id.clone()),
    );
    false
}

struct CompleteScope {
    entries: Vec<(Arc<str>, EvalResult)>,
}

impl CompleteScope {
    fn value(&self, name: &str) -> Option<&Value> {
        self.entries.iter().rev().find_map(|(entry_name, result)| {
            if entry_name.as_ref() == name {
                match result {
                    EvalResult::Scalar(value) => Some(value),
                    _ => None,
                }
            } else {
                None
            }
        })
    }
}

impl Scope for CompleteScope {
    fn lookup(&self, name: &str) -> Option<&EvalResult> {
        self.entries
            .iter()
            .rev()
            .find(|(entry_name, _)| entry_name.as_ref() == name)
            .map(|(_, value)| value)
    }

    fn enum_class(&self, qualified: &str) -> Option<EnumClassId> {
        enum_class_id(qualified)
    }

    fn enum_ordinal(&self, class: EnumClassId, literal: &str) -> Option<u32> {
        enum_member_ordinal(class, literal)
    }
}
