//! Rule implementations and deterministic diagnostic ordering for the deep load gate.
//!
//! Every check here is **total and panic-free** on *any* [`ModelGraph`] — including a
//! structurally-malformed, hand-built one whose ids are out of range or whose [`Attrs`] variant
//! violates the R5 tag invariant (a public struct literal can do this, bypassing the checked
//! [`oce_model::Connector::with_attrs`] constructor). Every block/connector/connection index is
//! bounds-checked before use and every `Real`-attribute read goes through
//! [`oce_model::Attrs::as_real`] (which yields `None` on a mismatched tag) rather than an unwrap.
//! A malformed graph yields a [`DiagCode::MalformedDocument`] diagnostic, never an abort
//! (`08` R-ERR-1 / the safety-critical testing standard).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use oce_blocks::{PortKind, lookup};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{Attrs, Connector, ConnectorId, Dir, ModelGraph, ValueType};

// ---- shared helpers -------------------------------------------------------------------------

/// The diagnostic subject for a connector — its source IRI when known, else a synthetic
/// `connector#N` id. **Ported verbatim from `oce-cxf` `resolve.rs`** so resolve-time and
/// validate-time diagnostics share one subject convention; [`finalize_diags`] parses the synthetic
/// form back to a numeric id so IRI-less structural diagnostics still sort by ascending
/// `ConnectorId.0` (not lexicographically, where `connector#10` would precede `connector#3`).
pub(crate) fn subject_of(c: &Connector) -> Arc<str> {
    match &c.iri {
        Some(iri) => Arc::clone(iri),
        None => Arc::from(format!("connector#{}", c.id.0)),
    }
}

/// Map an `oce-blocks` [`PortKind`] to the model [`ValueType`] a conforming connector must carry.
fn port_value_type(kind: PortKind) -> ValueType {
    match kind {
        PortKind::Real => ValueType::Real,
        PortKind::Integer => ValueType::Integer,
        PortKind::Boolean => ValueType::Boolean,
    }
}

/// Build the lookup `source-IRI → ConnectorId` map [`finalize_diags`] uses to resolve a real
/// connector IRI back to its numeric id. Only connectors that carry an `iri` appear (hand-built
/// connectors typically do not — their diagnostics use the synthetic `connector#N` subject).
pub(crate) fn conn_of_iri(model: &ModelGraph) -> HashMap<&str, ConnectorId> {
    model
        .connectors
        .iter()
        .filter_map(|c| c.iri.as_deref().map(|iri| (iri, c.id)))
        .collect()
}

/// In-degree of every connector (count of connections whose `to` is that connector). Out-of-range
/// `to` endpoints are skipped here (they are reported as [`DiagCode::MalformedDocument`] by
/// [`check_connections`]); the returned vector is dense over `connectors.len()`.
fn in_degrees(model: &ModelGraph) -> Vec<u32> {
    let mut deg = vec![0u32; model.connectors.len()];
    for c in &model.connections {
        let to = c.to.0 as usize;
        if to < deg.len() {
            deg[to] += 1;
        }
    }
    deg
}

// ---- Rule 0: arena id integrity -------------------------------------------------------------

/// Every block and connector must reference the dense arenas the executor indexes by raw id:
///
/// - `model.blocks[i].id.0 == i` for every block (dense + unique [`oce_model::BlockId`] space).
/// - `connector.block.0 < model.blocks.len()` for every connector.
/// - every `BlockInstance.inputs` / `BlockInstance.outputs` [`ConnectorId`] is in range.
///
/// These are malformed-document errors because `oce-graph` intentionally keeps BUILD/tick arena
/// indexing lean and assumes validation has already proven these invariants.
pub(crate) fn check_arena_ids(model: &ModelGraph, diags: &mut Vec<Diagnostic>) {
    let block_count = model.blocks.len();
    let connector_count = model.connectors.len();

    for (index, blk) in model.blocks.iter().enumerate() {
        let id = blk.id.0 as usize;
        if id != index {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!(
                        "block id invariant violated: block at arena index {index} has BlockId({}); \
                         expected a dense unique BlockId equal to its arena index and < blocks={block_count}",
                        blk.id.0
                    ),
                )
                .with_subject(block_subject_of(blk)),
            );
        }
        check_block_port_ids(blk, "input", &blk.inputs, connector_count, diags);
        check_block_port_ids(blk, "output", &blk.outputs, connector_count, diags);
    }

    for c in &model.connectors {
        if c.block.0 as usize >= block_count {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!(
                        "connector id {} references an out-of-range block id {} (blocks={block_count})",
                        c.id.0, c.block.0
                    ),
                )
                .with_subject(subject_of(c)),
            );
        }
    }
}

fn check_block_port_ids(
    blk: &oce_model::BlockInstance,
    label: &str,
    ports: &[ConnectorId],
    connector_count: usize,
    diags: &mut Vec<Diagnostic>,
) {
    for (port_idx, cid) in ports.iter().enumerate() {
        if cid.0 as usize >= connector_count {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!(
                        "block id {} {label} port {port_idx} references an out-of-range \
                         connector id {} (connectors={connector_count})",
                        blk.id.0, cid.0
                    ),
                )
                .with_subject(block_subject_of(blk)),
            );
        }
    }
}

// ---- Rule 1: boundary-aware single assignment (§7.10 / §9.1.5) ------------------------------

/// Every `In` connector must have in-degree **exactly 1**, *except* a declared external boundary
/// input (`ModelGraph::external_inputs`, the AD-2 boundary-input elision), which legally has
/// in-degree 0. In-degree ≥ 2 is always a `shall`-error — even for a connector that *is* in
/// `external_inputs` (an external input driven from inside is doubly-assigned).
pub(crate) fn check_single_assignment(model: &ModelGraph, diags: &mut Vec<Diagnostic>) {
    let deg = in_degrees(model);
    let external: HashSet<u32> = model.external_inputs.iter().map(|c| c.0).collect();
    for c in &model.connectors {
        if c.dir != Dir::In {
            continue;
        }
        let d = deg.get(c.id.0 as usize).copied().unwrap_or(0);
        match d {
            1 => {}
            0 => {
                if !external.contains(&c.id.0) {
                    diags.push(
                        Diagnostic::error(
                            DiagCode::SingleAssignment,
                            "input connector has in-degree 0 and is not a declared external \
                             boundary input (§7.10)",
                        )
                        .with_subject(subject_of(c)),
                    );
                }
            }
            n => {
                diags.push(
                    Diagnostic::error(
                        DiagCode::SingleAssignment,
                        format!(
                            "input connector has in-degree {n}; single assignment requires \
                             exactly 1 (§7.10)"
                        ),
                    )
                    .with_subject(subject_of(c)),
                );
            }
        }
    }
}

// ---- Rule 2: per-connection direction + value-type (§9.1.6) ---------------------------------

/// For every connection: `from` must be an `Out` connector, `to` an `In` connector, and the two
/// must share a [`ValueType`] (CDL forbids implicit coercion, §7.10). An out-of-range endpoint is a
/// [`DiagCode::MalformedDocument`] (and the connection is otherwise skipped — never indexed).
pub(crate) fn check_connections(model: &ModelGraph, diags: &mut Vec<Diagnostic>) {
    let n = model.connectors.len() as u32;
    for (idx, c) in model.connections.iter().enumerate() {
        if c.from.0 >= n || c.to.0 >= n {
            diags.push(Diagnostic::error(
                DiagCode::MalformedDocument,
                format!(
                    "connection #{idx} references an out-of-range connector id \
                     (from={}, to={}, connectors={n})",
                    c.from.0, c.to.0
                ),
            ));
            continue;
        }
        let from = &model.connectors[c.from.0 as usize];
        let to = &model.connectors[c.to.0 as usize];
        if from.dir != Dir::Out {
            diags.push(
                Diagnostic::error(
                    DiagCode::DirectionMismatch,
                    "connection source is not an output connector (§9.1.6)",
                )
                .with_subject(subject_of(from)),
            );
        }
        if to.dir != Dir::In {
            diags.push(
                Diagnostic::error(
                    DiagCode::DirectionMismatch,
                    "connection target is not an input connector (§9.1.6)",
                )
                .with_subject(subject_of(to)),
            );
        }
        if from.value_type != to.value_type {
            diags.push(
                Diagnostic::error(
                    DiagCode::TypeMismatch,
                    format!(
                        "connection joins connectors of different value types: {:?} → {:?}; \
                         CDL forbids implicit coercion (§9.1.6)",
                        from.value_type, to.value_type
                    ),
                )
                .with_subject(subject_of(to)),
            );
        }
    }
}

// ---- Rule 3: block interface ↔ block-signature agreement (AD-8, §7.8) -----------------------

/// Each block's port arity and connector [`ValueType`] must agree with the native block class's
/// [`oce_blocks::BlockSignature`]. This is the **reason `oce-validate` depends on `oce-blocks`**
/// (AD-8): the resolver derives a connector's value type from the CXF `isOfDataType`,
/// *independently* of the block class, so a document could type an input of `CDL.Reals.Add` as
/// `Boolean` — the resolver would record it, and the `read_real` hot-path reader would then silently
/// coerce it to `0.0` in release (a safety-critical silent wrong value). A hand-built graph can also
/// omit or add ports and would otherwise reach emit/gather by port index. This gate rejects those
/// mismatches at load.
///
/// An **unknown** class path is skipped (it is `oce-api`'s `OcError::Load` to report, R-IMPL-2).
/// Out-of-range port connector ids are reported by [`check_arena_ids`]; this rule still bounds them
/// before indexing so the validator remains panic-free even if rules are refactored later.
pub(crate) fn check_port_types(model: &ModelGraph, diags: &mut Vec<Diagnostic>) {
    let n = model.connectors.len() as u32;
    for blk in &model.blocks {
        let Some(entry) = lookup(&blk.class_iri) else {
            continue; // unknown class → oce-api OcError::Load owns this
        };
        let probe = (entry.make)(&blk.params);
        let sig = probe.signature();
        let (got_in, got_out) = (blk.inputs.len(), blk.outputs.len());
        let (want_in, want_out) = (sig.inputs.len(), sig.outputs.len());
        if got_in != want_in || got_out != want_out {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!(
                        "block interface mismatch for `{}`: declared {got_in} input(s)/{got_out} \
                         output(s), class requires {want_in}/{want_out}",
                        blk.class_iri
                    ),
                )
                .with_subject(block_subject_of(blk)),
            );
        }
        check_ports_dir(
            model,
            diags,
            n,
            &blk.class_iri,
            &blk.inputs,
            sig.inputs,
            "input",
        );
        check_ports_dir(
            model,
            diags,
            n,
            &blk.class_iri,
            &blk.outputs,
            sig.outputs,
            "output",
        );
    }
}

fn block_subject_of(blk: &oce_model::BlockInstance) -> Arc<str> {
    match &blk.instance_iri {
        Some(iri) => Arc::clone(iri),
        None => Arc::from(format!("block#{}", blk.id.0)),
    }
}

/// Compare one direction's worth of a block's port connectors against the signature port kinds.
fn check_ports_dir(
    model: &ModelGraph,
    diags: &mut Vec<Diagnostic>,
    n: u32,
    class_iri: &str,
    ports: &[ConnectorId],
    kinds: &[PortKind],
    label: &str,
) {
    for (port_idx, cid) in ports.iter().enumerate() {
        if cid.0 >= n {
            continue;
        }
        // Arity diagnostics are shared by resolver and `oce-validate` (AD-8). Once the block-level
        // arity diagnostic is emitted above, extra ports have no signature slot to compare.
        let Some(kind) = kinds.get(port_idx) else {
            continue;
        };
        let c = &model.connectors[cid.0 as usize];
        let want = port_value_type(*kind);
        if c.value_type != want {
            diags.push(
                Diagnostic::error(
                    DiagCode::PortKindMismatch,
                    format!(
                        "{label} port {port_idx} of block class {class_iri}: connector value type \
                         {:?} disagrees with the block-signature port kind {:?} (§7.8/§7.10)",
                        c.value_type, want
                    ),
                )
                .with_subject(subject_of(c)),
            );
        }
    }
}

// ---- §7.10 attribute unification (doc 02 §9 R13.1–R13.4) ------------------------------------

/// A `Real` attribute unified by §7.10 (doc 02 §9 R13.1/R13.2 lists exactly these four:
/// `quantity`, `unit`, `min`, `max`).
#[derive(Copy, Clone)]
enum RealAttr {
    Unit,
    Quantity,
    Min,
    Max,
}

impl RealAttr {
    fn label(self) -> &'static str {
        match self {
            RealAttr::Unit => "unit",
            RealAttr::Quantity => "quantity",
            RealAttr::Min => "min",
            RealAttr::Max => "max",
        }
    }

    /// `unit`/`quantity` conflicts are [`DiagCode::UnitQuantityMismatch`]; `min`/`max` (numeric
    /// bounds) are the analogous [`DiagCode::BoundMismatch`].
    fn conflict_code(self) -> DiagCode {
        match self {
            RealAttr::Unit | RealAttr::Quantity => DiagCode::UnitQuantityMismatch,
            RealAttr::Min | RealAttr::Max => DiagCode::BoundMismatch,
        }
    }
}

/// An `Integer` bound unified by §7.10 (Integer carries no unit/quantity, §3.1 matrix — only
/// `min`/`max`).
#[derive(Copy, Clone)]
enum IntAttr {
    Min,
    Max,
}

impl IntAttr {
    fn label(self) -> &'static str {
        match self {
            IntAttr::Min => "min",
            IntAttr::Max => "max",
        }
    }
}

/// A declared §7.10 attribute value, normalized so one gather-then-decide algorithm handles both
/// the string-valued attributes (`unit`/`quantity`) and the numeric ones (`min`/`max`). Equality is
/// **exact**: text by byte content (no trim/normalize/casefold), `Real` bounds **by bit pattern**
/// (`to_bits`, the crate float-equality convention — never `==`/ε).
#[derive(Clone)]
enum AttrVal {
    Text(Arc<str>),
    Real(f64),
    Int(i64),
}

impl AttrVal {
    fn same(&self, other: &AttrVal) -> bool {
        match (self, other) {
            (AttrVal::Text(a), AttrVal::Text(b)) => a.as_ref() == b.as_ref(),
            (AttrVal::Real(a), AttrVal::Real(b)) => a.to_bits() == b.to_bits(),
            (AttrVal::Int(a), AttrVal::Int(b)) => a == b,
            _ => false,
        }
    }

    fn display(&self) -> String {
        match self {
            AttrVal::Text(s) => s.to_string(),
            AttrVal::Real(x) => format!("{x:?}"),
            AttrVal::Int(i) => i.to_string(),
        }
    }
}

/// Read a connector's `displayUnit` (R13.3) — `None` if not `Real` or unset.
fn read_display_unit(c: &Connector) -> Option<Arc<str>> {
    c.attrs.as_real().and_then(|ra| ra.display_unit.clone())
}

/// §7.10 attribute unification (doc 02 §9 R13.1–R13.4), as a **gather-then-decide cluster
/// algorithm** — always **deterministic**, and **order-independent for every structurally-valid
/// graph**.
///
/// Under single assignment every `In` connector has in-degree ≤ 1, and in a *structurally-valid*
/// graph an `In` connector is never a connection *source* (using one as a `from` is a
/// [`check_connections`] `DirectionMismatch` that fails the load). So a connected component is a
/// disjoint **star**: one output connector and the inputs it drives, and no connector belongs to two
/// clusters — the result is then genuinely order-independent. (We cluster only inputs whose
/// in-degree is exactly 1; a multiply-driven input is a [`check_single_assignment`] error, excluded
/// so a cluster never double-*writes* a connector.) For each star, each `Real` attribute in
/// `{unit, quantity, min, max}` and each `Integer` bound in `{min, max}` is *gathered* (the set of
/// declared, non-default values across all members) then *decided*:
/// - **R13.1** ≥ 2 distinct values → a `shall`-error ([`DiagCode::UnitQuantityMismatch`] for
///   unit/quantity, [`DiagCode::BoundMismatch`] for min/max); no mutation.
/// - **R13.2** exactly 1 distinct value → propagate it to every member whose attribute is unset.
/// - 0 declared → nothing to do.
///
/// A naive single-sweep that *wrote mid-traversal* would be **non-confluent** (the blame for a
/// three-way conflict would depend on connection order); deciding on the gathered set removes that.
/// A *malformed* chained graph (`out → In → …`, where an `In` connector is wrongly reused as a
/// source — itself a load-failing `DirectionMismatch`) can place one connector in two clusters; even
/// there the `is_none` no-overwrite guard prevents any double-write and the pinned ascending
/// `roots.sort_unstable()` keeps the outcome **deterministic** (the transitive propagation is then
/// sort-ordered rather than confluent, but such a graph never passes [`validate`](crate::validate)).
/// Equality is **exact** (text by content, `Real` bounds by `to_bits`) — a deliberate tripwire
/// (`"K"` ≠ `"K "`). `displayUnit` divergence is the advisory R13.3 warning only (never an error,
/// never propagated). `nominal`/`unbounded` are **not** unified (R13.4) — those are the *only* two
/// §7.10 attributes excluded; `min`/`max` are NOT in R13.4 and ARE unified here.
pub(crate) fn unify_clusters(model: &mut ModelGraph, diags: &mut Vec<Diagnostic>) {
    let deg = in_degrees(model);
    let n = model.connectors.len() as u32;

    // Accumulate star clusters: output-connector id → driven (single-assignment) input ids.
    // HashMap is used only to accumulate; iteration is over a sorted key list so the emission is
    // deterministic regardless of graph shape (order-independent outright for the valid disjoint-star
    // case; see the type-doc above for the malformed-chain caveat).
    let mut clusters: HashMap<u32, Vec<u32>> = HashMap::new();
    for c in &model.connections {
        if c.from.0 >= n || c.to.0 >= n {
            continue; // out-of-range — check_connections reports it
        }
        if deg.get(c.to.0 as usize).copied().unwrap_or(0) != 1 {
            continue; // multiply-driven input — excluded for confluence (check_single_assignment errors)
        }
        clusters.entry(c.from.0).or_default().push(c.to.0);
    }
    let mut roots: Vec<u32> = clusters.keys().copied().collect();
    roots.sort_unstable();

    for root in roots {
        let mut members: Vec<u32> = Vec::with_capacity(clusters[&root].len() + 1);
        members.push(root);
        members.extend(clusters[&root].iter().copied());
        for attr in [
            RealAttr::Unit,
            RealAttr::Quantity,
            RealAttr::Min,
            RealAttr::Max,
        ] {
            unify_real(model, &members, attr, diags);
        }
        for attr in [IntAttr::Min, IntAttr::Max] {
            unify_int(model, &members, attr, diags);
        }
        warn_display_unit_divergence(model, &members, diags);
    }
}

/// Read a `Real` attribute off a connector — `None` if the connector is not a `Real` (the
/// `as_real()` guard also tolerates a tag-invariant-violating hand-built connector) or it is unset.
fn read_real(c: &Connector, attr: RealAttr) -> Option<AttrVal> {
    let ra = c.attrs.as_real()?;
    match attr {
        RealAttr::Unit => ra.unit.clone().map(AttrVal::Text),
        RealAttr::Quantity => ra.quantity.clone().map(AttrVal::Text),
        RealAttr::Min => ra.min.map(AttrVal::Real),
        RealAttr::Max => ra.max.map(AttrVal::Real),
    }
}

/// Write a unified `Real` attribute into a connector, **only if** its slot is currently unset.
fn write_real(c: &mut Connector, attr: RealAttr, value: &AttrVal) {
    let Attrs::Real(ra) = &mut c.attrs else {
        return;
    };
    match (attr, value) {
        (RealAttr::Unit, AttrVal::Text(s)) if ra.unit.is_none() => ra.unit = Some(Arc::clone(s)),
        (RealAttr::Quantity, AttrVal::Text(s)) if ra.quantity.is_none() => {
            ra.quantity = Some(Arc::clone(s));
        }
        (RealAttr::Min, AttrVal::Real(x)) if ra.min.is_none() => ra.min = Some(*x),
        (RealAttr::Max, AttrVal::Real(x)) if ra.max.is_none() => ra.max = Some(*x),
        _ => {}
    }
}

/// Gather-then-decide one `Real` attribute over one star cluster (R13.1 conflict / R13.2 propagate).
fn unify_real(
    model: &mut ModelGraph,
    members: &[u32],
    attr: RealAttr,
    diags: &mut Vec<Diagnostic>,
) {
    let mut declared: Vec<AttrVal> = Vec::new();
    for &m in members {
        if let Some(c) = model.connectors.get(m as usize)
            && let Some(v) = read_real(c, attr)
        {
            declared.push(v);
        }
    }
    decide_and_apply(
        model,
        members,
        &declared,
        attr.label(),
        attr.conflict_code(),
        diags,
        |c, v| write_real(c, attr, v),
    );
}

/// Gather-then-decide one `Integer` bound over one star cluster.
fn unify_int(model: &mut ModelGraph, members: &[u32], attr: IntAttr, diags: &mut Vec<Diagnostic>) {
    let mut declared: Vec<AttrVal> = Vec::new();
    for &m in members {
        if let Some(c) = model.connectors.get(m as usize)
            && let Some(ia) = c.attrs.as_integer()
        {
            let v = match attr {
                IntAttr::Min => ia.min,
                IntAttr::Max => ia.max,
            };
            if let Some(i) = v {
                declared.push(AttrVal::Int(i));
            }
        }
    }
    decide_and_apply(
        model,
        members,
        &declared,
        attr.label(),
        DiagCode::BoundMismatch,
        diags,
        |c, value| {
            if let (Attrs::Integer(ia), AttrVal::Int(i)) = (&mut c.attrs, value) {
                match attr {
                    IntAttr::Min if ia.min.is_none() => ia.min = Some(*i),
                    IntAttr::Max if ia.max.is_none() => ia.max = Some(*i),
                    _ => {}
                }
            }
        },
    );
}

/// The shared R13.1/R13.2 decision: ≥ 2 distinct declared → conflict error (no mutation); exactly 1
/// → propagate to every member via `apply` (which itself no-ops on already-set slots).
fn decide_and_apply(
    model: &mut ModelGraph,
    members: &[u32],
    declared: &[AttrVal],
    label: &str,
    conflict_code: DiagCode,
    diags: &mut Vec<Diagnostic>,
    apply: impl Fn(&mut Connector, &AttrVal),
) {
    let Some(first) = declared.first() else {
        return; // 0 declared — nothing to unify
    };
    if declared.iter().all(|v| v.same(first)) {
        // R13.2: propagate the single agreed value to every unset member. Order-independent — the
        // value is identical regardless of which member it came from.
        let value = first.clone();
        for &m in members {
            if let Some(c) = model.connectors.get_mut(m as usize) {
                apply(c, &value);
            }
        }
    } else {
        // R13.1: ≥ 2 distinct declared values → hard error. Subject = the cluster's output
        // connector (the common driver, deterministic). Values are sorted for a stable message.
        let mut distinct: Vec<String> = declared.iter().map(AttrVal::display).collect();
        distinct.sort();
        distinct.dedup();
        let mut d = Diagnostic::error(
            conflict_code,
            format!(
                "connected connectors declare conflicting {label} values {distinct:?}; \
                 §7.10 (R13.1) requires agreement"
            ),
        );
        if let Some(c) = members
            .first()
            .and_then(|&r| model.connectors.get(r as usize))
        {
            d = d.with_subject(subject_of(c));
        }
        diags.push(d);
    }
}

/// R13.3: divergent `displayUnit`s across a star are an advisory warning — non-computational, never
/// an error, never propagated.
fn warn_display_unit_divergence(model: &ModelGraph, members: &[u32], diags: &mut Vec<Diagnostic>) {
    let mut declared: Vec<Arc<str>> = Vec::new();
    for &m in members {
        if let Some(c) = model.connectors.get(m as usize)
            && let Some(du) = read_display_unit(c)
        {
            declared.push(du);
        }
    }
    if declared.len() < 2 || declared.iter().all(|v| v.as_ref() == declared[0].as_ref()) {
        return;
    }
    let mut distinct: Vec<String> = declared.iter().map(|v| v.to_string()).collect();
    distinct.sort();
    distinct.dedup();
    let mut d = Diagnostic::warning(
        DiagCode::DisplayUnitDivergence,
        format!(
            "connected connectors declare divergent displayUnit values {distinct:?}; \
             non-computational, advisory only (§7.17, R13.3)"
        ),
    );
    if let Some(c) = members
        .first()
        .and_then(|&r| model.connectors.get(r as usize))
    {
        d = d.with_subject(subject_of(c));
    }
    diags.push(d);
}

// ---- deterministic diagnostic ordering ------------------------------------------------------

/// Sort diagnostics into the pinned deterministic order: connector subjects by `ConnectorId.0`,
/// then block subjects by `BlockId.0`, then other subjects, followed by `DiagCode` string and
/// message tie-breakers. Synthetic `connector#N` and `block#N` subjects are parsed numerically so
/// IRI-less structural diagnostics never sort lexicographically (`#10` before `#3`). **Ported from
/// `oce-cxf` `resolve.rs`** for connector subjects so the resolver's and the validator's diagnostic
/// streams use one ordering discipline (`_spec/11-m1-cxf-plan.md` §2 determinism rule).
pub(crate) fn finalize_diags(
    mut diags: Vec<Diagnostic>,
    conn_of_iri: &HashMap<&str, ConnectorId>,
) -> Vec<Diagnostic> {
    // Resolve a subject IRI to its `ConnectorId.0`: either via a real source IRI (in `conn_of_iri`)
    // OR the synthetic `connector#N` form `subject_of` mints for an IRI-less connector. Both must
    // map to the numeric id so the structural diagnostics (single-assignment / direction / type) —
    // which are IRI-less in practice — sort by ascending `ConnectorId.0` per the pinned rule, not
    // by lexicographic string order (where `connector#10` would precede `connector#3`).
    let subject_key = |d: &Diagnostic| -> (u8, u32) {
        let Some(s) = d.subject.as_deref() else {
            return (2, u32::MAX);
        };
        if let Some(c) = conn_of_iri.get(s) {
            return (0, c.0);
        }
        if let Some(id) = s
            .strip_prefix("connector#")
            .and_then(|num| num.parse::<u32>().ok())
        {
            return (0, id);
        }
        if let Some(id) = s
            .strip_prefix("block#")
            .and_then(|num| num.parse::<u32>().ok())
        {
            return (1, id);
        }
        (2, u32::MAX)
    };
    diags.sort_by(|a, b| {
        subject_key(a)
            .cmp(&subject_key(b))
            .then_with(|| a.subject.as_deref().cmp(&b.subject.as_deref()))
            .then_with(|| a.code.as_str().cmp(b.code.as_str()))
            .then_with(|| a.message.cmp(&b.message))
    });
    diags
}
