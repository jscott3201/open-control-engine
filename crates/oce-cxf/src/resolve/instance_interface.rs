//! Child-instance interface derivation from `S231:hasInstance` (`_spec/19`, scalar-only
//! slice 1) and the structural shape domain its pre-lowering consumers read.
//!
//! Three domains, defined once and never mixed (R19-2):
//!
//! - **derivation-shaped** — a `containsBlock` referent anywhere in the document that is not a
//!   runtime composite, declaring neither `hasInput` nor `hasOutput`, carrying a `hasInstance`
//!   list. A structural test, computable before lowering; reachability is NOT part of it. The
//!   two pre-lowering consumers read this set: the array scan over its active members and
//!   specialization over all of them ([`is_derivation_shaped`]).
//! - the **derivation domain** — the post-lowering instance population (`child_iris`: active
//!   leaves) restricted to nodes that declare neither port list and carry a list. Those, and
//!   only those, derive an interface ([`derive()`]). The post-lowering domain is `child_iris`
//!   and nothing else — an implementation MUST NOT cache a pre-lowering referent set and apply
//!   the [`is_runtime_composite`] predicate to it after lowering, because `lower` clears every
//!   non-root composite's `contains_block` and the predicate then answers false for a nested
//!   composite.
//! - the **comparison domain** — instances that declare `hasInput` or `hasOutput` AND carry a
//!   list. Where the class resolves, the list is compared one-directional — list minus own,
//!   over names the resolved class declares — and never derived: a Warning for a name carried
//!   on the list alone, an Error for a parameter valued differently on the two routes
//!   ([`DiagCode::ConflictingInterfaceDeclaration`]).
//!
//! `hasInstance` never feeds composite classification: `is_runtime_composite` stays
//! `!contains_block.is_empty() && !is_registered_leaf(node)`, so a node carrying `hasInstance`
//! with no `containsBlock` is a leaf (derivation-shaped when portless), never a runtime
//! composite — R20-9's `composite_chain` flag keeps its meaning unchanged, and `hasInstance` is
//! not a containment edge anywhere (R19-12).
//!
//! The derivation consumes only facts that are functions of the class path — declared port
//! names, per-port kinds, declared parameter names, all readable at Step 3/4 (R19-4) — and
//! classifies each member by `local_name` alone: declared port name → port (side from the
//! declared list it appears in), declared parameter name → parameter, anything else refused
//! (`composite/unsupported-instance-member`). A class publishing no port names refuses whole
//! (`composite/vector-port-instance`): its port count is a function of a parameter, one member
//! stands for N scalar connectors, and this slice derives scalar interfaces only. Members are
//! never reported as `UnresolvedReference` in their own right (R19-9): a node-less port member
//! becomes a synthesized connector, a node-less parameter member is refused with
//! `GroundingFailed` before `param_iris` is built, and derivation is skipped exactly when the
//! list cannot be classified in full (R19-10) — the skip replaces the arity diagnostic rather
//! than doubling it, and a skipped instance's members are withdrawn before Step 5a numbers
//! anything.
//!
//! §7.4.1 attribute fidelity is total-loss on this dialect by measurement (0 of 1,754 vendored
//! member nodes carry unit/quantity/min/max), so `oce-validate`'s unit unification has no input
//! here; a synthesized connector carries the default attribute set for its type.

use std::collections::{HashMap, HashSet};

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{BlockId, Dir, ValueType};

use crate::dto::{CxfDocument, Node};

use super::composite::is_runtime_composite;
use super::composite_rules::{
    COLLIDING_MEMBER_IDENTITY, UNSUPPORTED_INSTANCE_MEMBER, VECTOR_PORT_INSTANCE,
};
use super::local_name;
use super::specialize::Specialization;
use super::value_types::{first_type, try_derive_value_type};

/// Every IRI referenced by any node's `containsBlock`, active or not — the reachability-blind
/// referent set the derivation-shaped test reads. Lookup-only.
pub(super) fn contains_block_referents(doc: &CxfDocument) -> HashSet<&str> {
    doc.graph
        .iter()
        .flat_map(|node| node.contains_block.iter().map(|r| r.id.as_str()))
        .collect()
}

/// The structural shape test (R19-2): a `containsBlock` referent that is not a runtime
/// composite, declares neither port list, and carries a `hasInstance` list. Valid ONLY
/// pre-lowering — see the module docs for why it must never be applied to the lowered graph.
pub(super) fn is_derivation_shaped(node: &Node, contains_referents: &HashSet<&str>) -> bool {
    contains_referents.contains(node.id.as_str())
        && !is_runtime_composite(node)
        && node.has_input.is_empty()
        && node.has_output.is_empty()
        && !node.has_instance.is_empty()
}

/// A derivation-shaped node's members classified as ports of its class, for the pre-lowering
/// [`CompositeOrientation`](super::composite_orientation::CompositeOrientation) ownership index
/// (R19-14): `(member IRI, is_input)` per member whose `local_name` is a declared port name.
/// Classes publishing no port names contribute nothing (they are refused on the derivation
/// path), and non-port members claim no ownership.
pub(super) fn classified_port_members(node: &Node) -> Vec<(&str, bool)> {
    let Some(names) = first_type(node)
        .map(crate::bridge::class_path_of)
        .and_then(oce_blocks::port_names::port_names)
    else {
        return Vec::new();
    };
    node.has_instance
        .iter()
        .map(|r| r.id.as_str())
        .filter_map(|member| {
            let name = local_name(member);
            if names.inputs.contains(&name) {
                Some((member, true))
            } else if names.outputs.contains(&name) {
                Some((member, false))
            } else {
                None
            }
        })
        .collect()
}

/// The value type of a derived member connector node (R19-6): the signature `PortKind` is the
/// type; the node's own declared type — `isOfDataType`, else the first `@type` term —
/// overrides it when that declaration resolves; a declaration that does not resolve keeps its
/// existing diagnostic and the connector takes the signature kind rather than the `Real`
/// recovery placeholder; and **absence** of any declared type is not a diagnostic on this
/// path — the signature already types the connector.
pub(super) fn derive_member_value_type(
    node: &Node,
    fallback: ValueType,
    diags: &mut Vec<Diagnostic>,
) -> ValueType {
    if node.is_of_data_type.is_none() && first_type(node).is_none() {
        return fallback;
    }
    try_derive_value_type(node, diags).unwrap_or(fallback)
}

/// One synthesized connector (R19-5): a listed member whose IRI resolves to no node, or a
/// padded declared output the member list omits. Its identity is document-derived — the member
/// IRI as authored, or `<owner @id>.<declared port name>` — never `None` and never positional.
pub(super) struct SynthesizedConnector {
    /// The connector's durable IRI.
    pub(super) identity: String,
    /// The deriving instance.
    pub(super) owner: BlockId,
    /// Direction, from the declared list the member's name appears in (R19-14).
    pub(super) dir: Dir,
    /// The signature `PortKind`'s value type (R19-6 clause 1 — no node, no override).
    pub(super) value_type: ValueType,
}

/// A derived instance's signature-ordered port identity vectors. Listed-but-inactive names are
/// omitted (they derive no connector and enter neither `ConnectorId` block), unlisted inputs
/// are omitted (the short vector takes the existing arity refusal), and unlisted outputs are
/// padded (R19-7).
pub(super) struct DerivedInterface {
    /// Input identities in class-signature order.
    pub(super) inputs: Vec<String>,
    /// Output identities in class-signature order, padded identities included.
    pub(super) outputs: Vec<String>,
}

/// The document-wide outcome of interface derivation, consumed by Steps 5a-8.
#[derive(Default)]
pub(super) struct InstanceDerivation {
    interfaces: HashMap<String, DerivedInterface>,
    skipped: HashSet<String>,
    /// Block-2 connectors in the ruled total order: `(owner @graph position, the port's
    /// position in the class signature's inputs ⧺ outputs concatenation)`, appended after
    /// every block-1 connector. No key reads the `hasInstance` array (R19-13).
    pub(super) synthesized: Vec<SynthesizedConnector>,
    member_value_fallback: HashMap<String, ValueType>,
    param_appendix: HashMap<String, Vec<String>>,
}

impl InstanceDerivation {
    /// The derived interface of `instance`, `None` for skipped and non-derived instances.
    pub(super) fn interface(&self, instance: &str) -> Option<&DerivedInterface> {
        self.interfaces.get(instance)
    }

    /// Whether derivation refused `instance` (R19-10): it must not reach the Step-8
    /// interface-agreement check — its refusal replaces the arity diagnostic.
    pub(super) fn is_skipped(&self, instance: &str) -> bool {
        self.skipped.contains(instance)
    }

    /// The signature `PortKind` fallback for a node-bearing derived port member, keyed by
    /// member IRI — Step 6's R19-6 precedence input. `None` for every non-member node.
    pub(super) fn member_value_fallback(&self, iri: &str) -> Option<ValueType> {
        self.member_value_fallback.get(iri).copied()
    }

    /// Node-bearing classified parameter members of `instance` in class-signature order — the
    /// `param_iris` appendix (R19-13). Empty for skipped and non-derived instances.
    pub(super) fn param_appendix(&self, instance: &str) -> &[String] {
        self.param_appendix
            .get(instance)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Every derived port identity, for Step 5a's block-1 reference union. Node-less and
    /// padded identities are inert there (no `@graph` node matches them).
    pub(super) fn derived_port_identities(&self) -> impl Iterator<Item = &str> {
        self.interfaces
            .values()
            .flat_map(|d| d.inputs.iter().chain(d.outputs.iter()))
            .map(String::as_str)
    }
}

/// Derive every derivation-domain instance's interface and compare every comparison-domain
/// instance's list, in `BlockId` order. `authored` is the pre-lowering expanded document —
/// the instance-member test (R19-12) reads its `containsBlock` referents and carriers, which
/// lowering erases from the working graph.
pub(super) fn derive(
    authored: &CxfDocument,
    instances: &[(BlockId, &Node, &str)],
    by_id: &HashMap<&str, &Node>,
    graph_pos: &HashMap<&str, usize>,
    specialization: &Specialization,
    diags: &mut Vec<Diagnostic>,
) -> InstanceDerivation {
    let authored_referents = contains_block_referents(authored);
    let authored_carriers: HashSet<&str> = authored
        .graph
        .iter()
        .filter(|n| !n.contains_block.is_empty() || !n.has_instance.is_empty())
        .map(|n| n.id.as_str())
        .collect();

    let mut out = InstanceDerivation::default();
    let mut minted: HashSet<String> = HashSet::new();
    let mut synth_keyed: Vec<((usize, usize), SynthesizedConnector)> = Vec::new();

    for &(block, node, class_path) in instances {
        if node.has_instance.is_empty() || class_path.is_empty() {
            continue;
        }
        if oce_blocks::lookup(class_path).is_none() {
            // `ClassNotFound` is the instance's whole verdict: no member classified, none
            // synthesized, no per-member diagnostic (R19-3).
            continue;
        }
        if !node.has_input.is_empty() || !node.has_output.is_empty() {
            compare_declared_interfaces(node, class_path, by_id, diags);
            continue;
        }
        derive_one(
            block,
            node,
            class_path,
            by_id,
            graph_pos,
            specialization,
            &authored_referents,
            &authored_carriers,
            &mut minted,
            &mut synth_keyed,
            &mut out,
            diags,
        );
    }

    synth_keyed.sort_by_key(|a| a.0);
    out.synthesized = synth_keyed.into_iter().map(|(_, s)| s).collect();
    out
}

/// The catalog entry for a registered class path. `catalog()` covers every registry entry, so
/// this misses only when `lookup` would too.
fn catalog_entry(class_path: &str) -> Option<&'static oce_blocks::CatalogEntry> {
    oce_blocks::catalog()
        .iter()
        .find(|e| e.class_path == class_path)
}

fn port_value_type(kind: oce_blocks::PortKind) -> ValueType {
    match kind {
        oce_blocks::PortKind::Real => ValueType::Real,
        oce_blocks::PortKind::Integer => ValueType::Integer,
        oce_blocks::PortKind::Boolean => ValueType::Boolean,
    }
}

/// R19-12: a member must be a **direct** member of its owner — the owner's `@id`, one `.`,
/// one further segment — and must not itself be a block instance (a `containsBlock` referent
/// anywhere in the authored document, or a node carrying `containsBlock` or `hasInstance`),
/// or `hasInstance` would become a second containment edge the cycle detector does not walk.
fn member_domain_violation(
    owner: &str,
    member: &str,
    authored_referents: &HashSet<&str>,
    authored_carriers: &HashSet<&str>,
) -> Option<Diagnostic> {
    let direct = member
        .strip_prefix(owner)
        .and_then(|rest| rest.strip_prefix('.'))
        .is_some_and(|segment| !segment.is_empty() && !segment.contains('.'));
    if !direct {
        return Some(
            Diagnostic::error(
                UNSUPPORTED_INSTANCE_MEMBER.code,
                UNSUPPORTED_INSTANCE_MEMBER
                    .message(format!("member is not a direct member of `{owner}`")),
            )
            .with_subject(member.to_owned()),
        );
    }
    if authored_referents.contains(member) || authored_carriers.contains(member) {
        return Some(
            Diagnostic::error(
                UNSUPPORTED_INSTANCE_MEMBER.code,
                UNSUPPORTED_INSTANCE_MEMBER
                    .message(format!("member `{member}` is itself a block instance")),
            )
            .with_subject(member.to_owned()),
        );
    }
    None
}

/// Derive one derivation-domain instance: member-domain check, scalar-only gate, name
/// partition, mint-time identity uniqueness, then the signature-ordered vectors and block-2
/// synthesis. Every skip verdict is reached here — before Step 5a numbers anything — so a
/// skipped instance's members never enter either `ConnectorId` block (R19-10).
#[allow(clippy::too_many_arguments)]
fn derive_one(
    block: BlockId,
    node: &Node,
    class_path: &str,
    by_id: &HashMap<&str, &Node>,
    graph_pos: &HashMap<&str, usize>,
    specialization: &Specialization,
    authored_referents: &HashSet<&str>,
    authored_carriers: &HashSet<&str>,
    minted: &mut HashSet<String>,
    synth_keyed: &mut Vec<((usize, usize), SynthesizedConnector)>,
    out: &mut InstanceDerivation,
    diags: &mut Vec<Diagnostic>,
) {
    let owner = node.id.as_str();
    let mut skip = false;

    // R19-12 member domain, before classification: a member outside the owner is never
    // classified by a name it borrowed. One diagnostic per offending member.
    let mut members: Vec<&str> = Vec::new();
    for member in node.has_instance.iter().map(|r| r.id.as_str()) {
        match member_domain_violation(owner, member, authored_referents, authored_carriers) {
            Some(diag) => {
                diags.push(diag);
                skip = true;
            }
            None => members.push(member),
        }
    }

    // R19-1, the governing scalar-only scope: no declared port names, no partition.
    let Some(names) = oce_blocks::port_names::port_names(class_path) else {
        diags.push(
            Diagnostic::error(
                VECTOR_PORT_INSTANCE.code,
                VECTOR_PORT_INSTANCE.message(format!(
                    "instance of class `{class_path}` derives its port count from a parameter; \
                     this subset derives scalar interfaces only"
                )),
            )
            .with_subject(owner.to_owned()),
        );
        out.skipped.insert(owner.to_owned());
        return;
    };
    let Some(entry) = catalog_entry(class_path) else {
        return;
    };
    let param_names: Vec<&str> = entry.param_defaults.iter().map(|p| p.name).collect();

    // R19-3: partition by name, first clause winning. Classification reads only the name and
    // the class signature, so it never needs the member's node.
    let mut input_slot: Vec<Option<&str>> = vec![None; names.inputs.len()];
    let mut output_slot: Vec<Option<&str>> = vec![None; names.outputs.len()];
    let mut listing_count: HashMap<&str, usize> = HashMap::new();
    let mut classified_params: Vec<&str> = Vec::new();
    for member in &members {
        let name = local_name(member);
        if let Some(i) = names.inputs.iter().position(|n| *n == name) {
            input_slot[i] = Some(member);
            *listing_count.entry(member).or_insert(0) += 1;
        } else if let Some(j) = names.outputs.iter().position(|n| *n == name) {
            output_slot[j] = Some(member);
            *listing_count.entry(member).or_insert(0) += 1;
        } else if param_names.contains(&name) {
            classified_params.push(member);
        } else {
            diags.push(
                Diagnostic::error(
                    UNSUPPORTED_INSTANCE_MEMBER.code,
                    UNSUPPORTED_INSTANCE_MEMBER.message(format!(
                        "`{name}` is neither a declared port nor a declared parameter of \
                         `{class_path}`"
                    )),
                )
                .with_subject((*member).to_owned()),
            );
            skip = true;
        }
    }

    // R19-8, mint-time uniqueness. Connector half: a node-less port member IRI listed twice
    // mints one identity twice for one owner (a node-bearing duplicate listing binds once and
    // mints nothing). Iterated in member order for deterministic emission.
    let mut reported_twice: HashSet<&str> = HashSet::new();
    for member in &members {
        if listing_count.get(member).copied().unwrap_or(0) > 1
            && !by_id.contains_key(member)
            && reported_twice.insert(member)
        {
            diags.push(
                Diagnostic::error(
                    COLLIDING_MEMBER_IDENTITY.code,
                    COLLIDING_MEMBER_IDENTITY.message(format!(
                        "connector identity `{member}` is minted twice by `{owner}`"
                    )),
                )
                .with_subject((*member).to_owned()),
            );
            skip = true;
        }
    }
    // Parameter half: unique within the instance, against the node's own
    // `hasParameter` ⧺ `hasConstant` names and among the classified members themselves.
    let authored_param_names: HashSet<&str> = node
        .has_parameter
        .iter()
        .chain(node.has_constant.iter())
        .map(|r| local_name(r.id.as_str()))
        .collect();
    let mut seen_param_members: HashSet<&str> = HashSet::new();
    for member in &classified_params {
        let name = local_name(member);
        if !seen_param_members.insert(member) {
            diags.push(
                Diagnostic::error(
                    COLLIDING_MEMBER_IDENTITY.code,
                    COLLIDING_MEMBER_IDENTITY
                        .message(format!("parameter `{name}` is declared twice by `{owner}`")),
                )
                .with_subject((*member).to_owned()),
            );
            skip = true;
        } else if authored_param_names.contains(name) {
            diags.push(
                Diagnostic::error(
                    COLLIDING_MEMBER_IDENTITY.code,
                    COLLIDING_MEMBER_IDENTITY.message(format!(
                        "parameter `{name}` is declared by both `hasParameter` and \
                         `hasInstance` on `{owner}`"
                    )),
                )
                .with_subject((*member).to_owned()),
            );
            skip = true;
        }
    }
    // Padded identities against every `@graph` node and every minted identity, checked before
    // anything is committed so a colliding instance withdraws whole.
    let mut padded: Vec<(usize, String)> = Vec::new();
    for (j, name) in names.outputs.iter().enumerate() {
        if output_slot[j].is_none() {
            let identity = format!("{owner}.{name}");
            if by_id.contains_key(identity.as_str()) {
                diags.push(
                    Diagnostic::error(
                        COLLIDING_MEMBER_IDENTITY.code,
                        COLLIDING_MEMBER_IDENTITY.message(format!(
                            "synthesized connector identity `{identity}` is already an \
                             `@graph` node"
                        )),
                    )
                    .with_subject(identity.clone()),
                );
                skip = true;
            } else if minted.contains(identity.as_str()) {
                diags.push(
                    Diagnostic::error(
                        COLLIDING_MEMBER_IDENTITY.code,
                        COLLIDING_MEMBER_IDENTITY.message(format!(
                            "connector identity `{identity}` is minted twice by `{owner}`"
                        )),
                    )
                    .with_subject(identity.clone()),
                );
                skip = true;
            } else {
                padded.push((j, identity));
            }
        }
    }

    if skip {
        out.skipped.insert(owner.to_owned());
        return;
    }

    // R19-3's last row: a node-less parameter member is refused by the derivation itself,
    // with the existing Ground-mode message, and never enters `param_iris` — which is what
    // keeps Step 7's `UnresolvedReference` "parameter node not found" arm unreachable for
    // members. Activity does not filter parameters.
    for member in &classified_params {
        if !by_id.contains_key(member) {
            diags.push(
                Diagnostic::error(
                    DiagCode::GroundingFailed,
                    "parameter has no value (Ground mode)",
                )
                .with_subject((*member).to_owned()),
            );
        }
    }

    // Signature-ordered vectors and block-2 synthesis. An inactive member derives no
    // connector and enters neither block; an unlisted input leaves the vector short.
    let owner_pos = graph_pos.get(owner).copied().unwrap_or(usize::MAX);
    let mut interface = DerivedInterface {
        inputs: Vec::new(),
        outputs: Vec::new(),
    };
    for (i, _) in names.inputs.iter().enumerate() {
        let Some(member) = input_slot[i] else {
            continue;
        };
        if specialization.is_inactive(member) {
            continue;
        }
        interface.inputs.push(member.to_owned());
        let kind = port_value_type(entry.inputs[i].kind);
        if by_id.contains_key(member) {
            out.member_value_fallback.insert(member.to_owned(), kind);
        } else {
            minted.insert(member.to_owned());
            synth_keyed.push((
                (owner_pos, i),
                SynthesizedConnector {
                    identity: member.to_owned(),
                    owner: block,
                    dir: Dir::In,
                    value_type: kind,
                },
            ));
        }
    }
    for (j, _) in names.outputs.iter().enumerate() {
        let concat_pos = names.inputs.len() + j;
        let kind = port_value_type(entry.outputs[j].kind);
        if let Some(member) = output_slot[j] {
            if specialization.is_inactive(member) {
                continue;
            }
            interface.outputs.push(member.to_owned());
            if by_id.contains_key(member) {
                out.member_value_fallback.insert(member.to_owned(), kind);
            } else {
                minted.insert(member.to_owned());
                synth_keyed.push((
                    (owner_pos, concat_pos),
                    SynthesizedConnector {
                        identity: member.to_owned(),
                        owner: block,
                        dir: Dir::Out,
                        value_type: kind,
                    },
                ));
            }
        } else if let Some((_, identity)) = padded.iter().find(|(slot, _)| *slot == j) {
            interface.outputs.push(identity.clone());
            minted.insert(identity.clone());
            synth_keyed.push((
                (owner_pos, concat_pos),
                SynthesizedConnector {
                    identity: identity.clone(),
                    owner: block,
                    dir: Dir::Out,
                    value_type: kind,
                },
            ));
        }
    }

    // R19-13: node-bearing classified parameter members append to `param_iris` in
    // class-signature order — `CatalogEntry::param_defaults` order — with any name outside
    // the signature following in sorted-name order (defensively; clause 3 refused them all).
    let mut appendix: Vec<&str> = classified_params
        .iter()
        .copied()
        .filter(|m| by_id.contains_key(m))
        .collect();
    appendix.sort_by(|a, b| {
        let pos = |m: &str| {
            param_names
                .iter()
                .position(|n| *n == local_name(m))
                .unwrap_or(usize::MAX)
        };
        pos(a)
            .cmp(&pos(b))
            .then_with(|| local_name(a).cmp(local_name(b)))
    });
    if !appendix.is_empty() {
        out.param_appendix.insert(
            owner.to_owned(),
            appendix.into_iter().map(str::to_owned).collect(),
        );
    }
    out.interfaces.insert(owner.to_owned(), interface);
}

/// The comparison domain's one-directional detector (R19-2): over names the resolved class
/// declares, a **list-minus-own** name difference is a Warning (the shape is legal CXF), and
/// a parameter name valued on both routes with different authored values is an Error — two
/// values for one name state a contradiction. The subject is the instance node in both
/// halves; values compare as authored `S231:value` payloads.
fn compare_declared_interfaces(
    node: &Node,
    class_path: &str,
    by_id: &HashMap<&str, &Node>,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(entry) = catalog_entry(class_path) else {
        return;
    };
    let param_names: Vec<&str> = entry.param_defaults.iter().map(|p| p.name).collect();
    let mut class_declared: HashSet<&str> = param_names.iter().copied().collect();
    if let Some(names) = oce_blocks::port_names::port_names(class_path) {
        class_declared.extend(names.inputs.iter().copied());
        class_declared.extend(names.outputs.iter().copied());
    }

    let own: HashSet<&str> = node
        .has_input
        .iter()
        .chain(node.has_output.iter())
        .chain(node.has_parameter.iter())
        .chain(node.has_constant.iter())
        .map(|r| local_name(r.id.as_str()))
        .filter(|name| class_declared.contains(name))
        .collect();
    let mut missing: Vec<&str> = node
        .has_instance
        .iter()
        .map(|r| local_name(r.id.as_str()))
        .filter(|name| class_declared.contains(name) && !own.contains(name))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    if !missing.is_empty() {
        diags.push(
            Diagnostic::warning(
                DiagCode::ConflictingInterfaceDeclaration,
                format!(
                    "hasInstance carries class-declared name(s) `{}` declared on none of the \
                     node's own hasInput/hasOutput/hasParameter/hasConstant routes",
                    missing.join("`, `")
                ),
            )
            .with_subject(node.id.clone()),
        );
    }

    for pname in &param_names {
        let valued = |iri: &str| {
            (local_name(iri) == *pname)
                .then(|| by_id.get(iri).and_then(|n| n.value.as_ref()))
                .flatten()
        };
        let own_value = node
            .has_parameter
            .iter()
            .chain(node.has_constant.iter())
            .find_map(|r| valued(r.id.as_str()));
        let list_value = node.has_instance.iter().find_map(|r| valued(r.id.as_str()));
        if let (Some(a), Some(b)) = (own_value, list_value)
            && a != b
        {
            diags.push(
                Diagnostic::error(
                    DiagCode::ConflictingInterfaceDeclaration,
                    format!(
                        "parameter `{pname}` is declared with different values by hasParameter \
                         and hasInstance"
                    ),
                )
                .with_subject(node.id.clone()),
            );
        }
    }
}
